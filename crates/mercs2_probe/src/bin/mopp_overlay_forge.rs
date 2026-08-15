//! `mopp_overlay_forge` — item 3 of the MOPP in-game plan: wrap a MOPP-swapped PHY2 collision cell in
//! a `vz-patch.wad` overlay that OVERRIDES exactly that one block, last-wins.
//!
//! Three swap modes drive the 3-rung in-game ladder over the SAME resident spawn-floor cell:
//!   --mode identity    (default) re-serialize the SAME decoded MOPP bytes → block is byte-identical to
//!                       base. Proves the deploy pipeline is sound + non-breaking (rung 1).
//!   --mode return-all   swap the floor MOPP idx 0 to `encode_return_all(N)` (N = source key count):
//!                       universal-collision. Proves OUR compiler's bytecode is game-valid + correct —
//!                       the player stands only if the game ran our MOPP (rung 3, positive attribution).
//!   --mode empty        swap the floor MOPP(s) to the emit-nothing MOPP (`[0x00]`, a lone RETURN → 0
//!                       keys). Removes this cell's collision → the player FALLS THROUGH. Negative
//!                       control (rung 2, positive attribution: our block drives collision). `--all-mopps`
//!                       empties every MOPP in the chosen container (the whole floor shell).
//!
//! Pipeline (all offline-gated; base `vz.wad` is READ-ONLY and stays pristine):
//!   1. decompress the target block; walk its entry table -> model containers.
//!   2. pick the container carrying a PHY2 with an hkpMoppCode (or --model <hash>).
//!   3. extract that PHY2 body; run `swap_phy2_mopp(.., mode)` for each targeted MOPP.
//!   4. splice the swapped body back into the container — RESIZE-AWARE (descriptor size, later bodies'
//!      offsets, CSUM) — and patch the block entry table's chunk_size -> reassemble the block.
//!      GATE: rebuilt block re-walks clean (CSUM/descriptors), and for identity is byte-identical to base.
//!   5. carry EVERY ASET row that points at this block; `PatchBlock::from_decompressed` (sges +
//!      packed_field); `build_patch_wad_multi` (auto-sentinels dangling LOD rungs -> no 549GB wedge).
//!   6. `aset_refcheck` the written WAD separately as the deploy gate.
//!
//! Usage:
//!   mopp_overlay_forge --block 2612 --report
//!   mopp_overlay_forge --block 2612 --mode return-all --out .../vz-patch-mopp-returnall.wad \
//!       --merge-into .../live-vz-patch.wad --merge-out .../...-MERGED.wad
//!   mopp_overlay_forge --block 2612 --mode empty --all-mopps --out .../vz-patch-mopp-empty.wad ...

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::patch_wad::{
    build_patch_wad_multi, merge_patch_wads, AsetEntry, PatchBlock, FFCS_CERT_BLOB,
};
use mercs2_formats::phy2_moppswap::{
    count_phy2_mopps, decode_phy2_mopp_keys, swap_phy2_mopp, validate_swapped_body, SwapMode,
};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{walk_decompressed_block, ParsedBlock};
use sha2::{Digest, Sha256};
use std::path::Path;

fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}
fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn has(a: &[String], n: &str) -> bool {
    a.iter().any(|x| x == n)
}
fn mode_name(m: SwapMode) -> &'static str {
    match m {
        SwapMode::Identity => "identity",
        SwapMode::ReturnAll => "return-all",
        SwapMode::Empty => "empty",
        SwapMode::Spatial => "spatial",
    }
}

/// Locate the PHY2 chunk inside a UCFX container: returns (body_start, body_size) relative to the
/// container. `body_start` is absolute in the container bytes.
fn phy2_span_in_container(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return None;
    }
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

/// First MOPP index whose source keys are a clean contiguous [0..N-1] run.
fn first_contiguous_mopp(body: &[u8]) -> usize {
    for i in 0..count_phy2_mopps(body) {
        if let Ok(d) = decode_phy2_mopp_keys(body, i) {
            let (ks, range, missing) = d.key_summary();
            if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1)) {
                return i;
            }
        }
    }
    0
}

/// Replace the PHY2 body at absolute container offset `pstart` (old length `psize`) with `new_body`,
/// fixing the UCFX descriptor table (this PHY2's `body_size`; any body located AFTER it shifts by the
/// size delta), splicing the data region, and recomputing the trailing CSUM. Returns the rebuilt
/// container. `delta == 0` (in-place-size swaps: identity, empty) produces a container with only the
/// changed body bytes + recomputed CSUM — byte-identical when the body itself is unchanged.
fn replace_phy2_in_container(
    container: &[u8],
    pstart: usize,
    psize: usize,
    new_body: &[u8],
) -> Result<Vec<u8>, String> {
    if container.len() < 28 || &container[0..4] != b"UCFX" {
        return Err("container is not a UCFX packet".into());
    }
    if container.len() < 8 || &container[container.len() - 8..container.len() - 4] != b"CSUM" {
        return Err("container has no CSUM trailer".into());
    }
    let csum_start = container.len() - 8;
    if pstart + psize > csum_start {
        return Err("PHY2 body overlaps the CSUM trailer".into());
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().unwrap()) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().unwrap()) as usize;
    let delta = new_body.len() as i64 - psize as i64;
    let phy2_rel = pstart
        .checked_sub(if data_area_off > 0 { data_area_off } else { 8 })
        .ok_or("PHY2 body starts before the data area")?;

    // Patch the descriptor table (rows sit before the data area, i.e. before `pstart`).
    let mut c = container.to_vec();
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > c.len() {
            break;
        }
        let ru0 = u32::from_le_bytes(c[r + 4..r + 8].try_into().unwrap());
        if ru0 == 0xFFFF_FFFF {
            continue;
        }
        let ru0u = ru0 as usize;
        if ru0u == phy2_rel && &c[r..r + 4] == b"PHY2" {
            c[r + 8..r + 12].copy_from_slice(&(new_body.len() as u32).to_le_bytes());
        } else if ru0u > phy2_rel {
            let nv = (ru0 as i64 + delta) as u32;
            c[r + 4..r + 8].copy_from_slice(&nv.to_le_bytes());
        }
    }

    // Splice the data region and recompute the CSUM over everything before the trailer.
    let mut out = Vec::with_capacity((c.len() as i64 + delta).max(0) as usize);
    out.extend_from_slice(&c[..pstart]);
    out.extend_from_slice(new_body);
    out.extend_from_slice(&c[pstart + psize..csum_start]);
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// One MOPP's key summary for the report / gate lines.
fn mopp_line(body: &[u8], idx: usize) -> String {
    match decode_phy2_mopp_keys(body, idx) {
        Ok(d) => {
            let (ks, range, missing) = d.key_summary();
            let tag = if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1))
            {
                "single-subpart [0..N-1]"
            } else if missing.is_empty() {
                "contiguous (offset base)"
            } else {
                "multi-subpart / gaps"
            };
            format!(
                "{} keys, range {:?}, {} missing, err {:?}  [{tag}]",
                ks.len(),
                range,
                missing.len(),
                d.error
            )
        }
        Err(e) => format!("decode failed: {e}"),
    }
}

fn print_report(parsed: &ParsedBlock, block: u16) {
    println!("=== mopp_overlay_forge REPORT — block {block} MOPP inventory ===");
    let mut any = false;
    for (i, c) in parsed.containers.iter().enumerate() {
        let Some((s, sz)) = phy2_span_in_container(c) else {
            continue;
        };
        let body = &c[s..s + sz];
        let n = count_phy2_mopps(body);
        if n == 0 {
            continue;
        }
        any = true;
        let nh = parsed.entries[i].name_hash;
        println!(
            "container[{i}] name 0x{nh:08X}  container {} B  PHY2 {sz} B @ +0x{s:X}  {n} MOPP(s):",
            c.len()
        );
        for m in 0..n {
            println!("    mopp[{m}]: {}", mopp_line(body, m));
        }
    }
    if !any {
        println!("(no container with a PHY2+MOPP in this block)");
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mopp_overlay_forge: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block: u16 = arg(&args, "--block").and_then(|s| s.parse().ok()).unwrap_or(2612);
    let model_hash: Option<u32> = arg(&args, "--model")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());
    let report = has(&args, "--report");
    let all_mopps = has(&args, "--all-mopps");
    let mode = match arg(&args, "--mode").as_deref() {
        None | Some("identity") => SwapMode::Identity,
        Some("return-all") | Some("returnall") => SwapMode::ReturnAll,
        Some("empty") => SwapMode::Empty,
        Some(other) => return Err(format!("unknown --mode {other} (identity|return-all|empty)")),
    };
    let default_out = format!(
        "C:/Users/Shadow/AppData/Local/Temp/hunt/mopp/overlay/vz-patch-mopp-{}.wad",
        match mode {
            SwapMode::Identity => "identity",
            SwapMode::ReturnAll => "returnall",
            SwapMode::Empty => "empty",
            SwapMode::Spatial => "spatial",
        }
    );
    let out = arg(&args, "--out").unwrap_or(default_out);

    let vz = game_paths::vz_wad(Path::new("."))
        .ok_or("vz.wad not found — set MERCS2_GAME_DIR or .mercs2-local.toml")?;
    let mut f = std::fs::File::open(&vz).map_err(|e| format!("open {vz:?}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("ffcs: {e}"))?;

    let path = ar
        .paths
        .get(block as usize)
        .cloned()
        .ok_or_else(|| format!("block {block} has no PTHS path"))?;

    // 1. decompress + walk
    let dec = decompress_block(&mut f, &ar.indx, block).map_err(|e| format!("decompress {block}: {e}"))?;
    let dec_sha = sha256_hex(&dec);
    let (parsed, issues) = walk_decompressed_block(&dec, "target");
    for is in &issues {
        eprintln!("  base walk issue: {} :: {}", is.context, is.detail);
    }

    if report {
        println!("base wad : {}", vz.display());
        println!("target   : block {block}  '{path}'");
        println!("decompressed block: {} B  sha256 {dec_sha}", dec.len());
        print_report(&parsed, block);
        return Ok(());
    }

    println!("=== mopp_overlay_forge (mode={}) ===", mode_name(mode));
    println!("base wad : {}", vz.display());
    println!("target   : block {block}  '{path}'");
    println!("decompressed block: {} B  sha256 {dec_sha}", dec.len());

    // 2. pick the container carrying a PHY2+MOPP (optionally constrained to --model hash)
    let mut chosen: Option<usize> = None;
    for (i, c) in parsed.containers.iter().enumerate() {
        if let Some(mh) = model_hash {
            if parsed.entries[i].name_hash != mh {
                continue;
            }
        }
        if let Some((s, sz)) = phy2_span_in_container(c) {
            if count_phy2_mopps(&c[s..s + sz]) > 0 {
                chosen = Some(i);
                break;
            }
        }
    }
    let ci = chosen.ok_or("no container with a PHY2+MOPP found in this block")?;
    let ename = parsed.entries[ci].name_hash;
    let (pstart, psize) = phy2_span_in_container(&parsed.containers[ci]).unwrap();
    let base_body = parsed.containers[ci][pstart..pstart + psize].to_vec();
    let n_mopps = count_phy2_mopps(&base_body);
    println!(
        "container[{ci}] name 0x{ename:08X}: PHY2 body {psize} B @ container+0x{pstart:X}, {n_mopps} MOPP(s)",
        );
    println!("  src PHY2 body sha256 {}", sha256_hex(&base_body));

    // Which MOPP indices does this mode target?
    let default_idx = arg(&args, "--mopp").and_then(|s| s.parse::<usize>().ok());
    let targets: Vec<usize> = match mode {
        SwapMode::Empty if all_mopps => (0..n_mopps).collect(),
        _ => vec![default_idx.unwrap_or_else(|| first_contiguous_mopp(&base_body))],
    };
    println!("  targeting MOPP index(es): {targets:?}");

    // 3. apply the swap(s) sequentially. Identity & empty are size-preserving (in-place), so successive
    //    MOPP indices stay valid across calls; return-all is applied to a single index.
    let mut body = base_body.clone();
    for &idx in &targets {
        if idx >= count_phy2_mopps(&body) {
            return Err(format!("mopp index {idx} out of range ({n_mopps} MOPPs)"));
        }
        let src_keys = decode_phy2_mopp_keys(&body, idx)?.keys;
        let (nb, rep) = swap_phy2_mopp(&body, idx, mode)?;
        let expected: Option<&[u32]> = if mode == SwapMode::Identity { Some(&src_keys) } else { None };
        let g = validate_swapped_body(&nb, idx, mode, expected);
        let gate_ok = g.reparse_ok
            && g.mopp_present
            && g.decode_clean
            && g.decode_coverage_full
            && g.keys_as_expected
            && g.mesh_still_decodes;
        println!(
            "  mopp[{idx}]: {} src keys -> {} decoded key(s), buf {}->{} B, packfile {}->{} B ({}), offline-gate {}",
            rep.source_key_count,
            g.decoded_key_count,
            rep.old_buf_len,
            rep.new_buf_len,
            rep.old_packfile_size,
            rep.new_packfile_size,
            if rep.grew { "GREW/appended" } else { "in-place" },
            if gate_ok { "PASS" } else { "FAIL" }
        );
        if !gate_ok {
            return Err(format!("mopp[{idx}] offline gate FAILED: {g:?}"));
        }
        if mode == SwapMode::Identity && nb != body {
            return Err("identity swap was not byte-identical".into());
        }
        body = nb;
    }
    let new_body = body;
    println!("  swapped PHY2 body: {} B  sha256 {}", new_body.len(), sha256_hex(&new_body));

    // 4. splice the swapped body back into the container (RESIZE-AWARE) + rebuild the block.
    let new_container = replace_phy2_in_container(&parsed.containers[ci], pstart, psize, &new_body)?;
    let header_end = 4 + parsed.entry_count as usize * 16;
    let mut rebuilt = Vec::with_capacity(dec.len() + new_container.len());
    rebuilt.extend_from_slice(&dec[0..header_end]); // count + entry table
    // patch this container's entry-table chunk_size to the (possibly resized) container length.
    let cs_off = 4 + ci * 16 + 12;
    rebuilt[cs_off..cs_off + 4].copy_from_slice(&(new_container.len() as u32).to_le_bytes());
    for (i, c) in parsed.containers.iter().enumerate() {
        if i == ci {
            rebuilt.extend_from_slice(&new_container);
        } else {
            rebuilt.extend_from_slice(c);
        }
    }
    let rebuilt_sha = sha256_hex(&rebuilt);
    println!("rebuilt block: {} B  sha256 {rebuilt_sha}", rebuilt.len());

    // GATE A: rebuilt block re-walks clean (CSUM + descriptor bounds) — the resize-correctness proof.
    let (_rp, ri) = walk_decompressed_block(&rebuilt, "rebuilt");
    if !ri.is_empty() {
        for is in &ri {
            eprintln!("  rebuilt walk issue: {} :: {}", is.context, is.detail);
        }
        return Err("rebuilt block failed the re-walk (CSUM/descriptor) gate".into());
    }
    println!("  RE-WALK GATE: rebuilt block walks clean (CSUM + descriptors OK)");
    // GATE B (identity only): byte-identical to base.
    if mode == SwapMode::Identity {
        println!(
            "  IDENTITY GATE — rebuilt == base decompressed: {}",
            if rebuilt == dec { "YES (byte-identical)" } else { "NO !!" }
        );
        if rebuilt != dec {
            return Err("identity rebuilt block is NOT byte-identical to base".into());
        }
    }

    // 5. carry EVERY ASET row that points at this block; build the overlay
    let aset: Vec<AsetEntry> = ar
        .aset
        .iter()
        .filter(|e| e.block_index() == block)
        .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
        .collect();
    let tier = ar.indx.get(block as usize).map(|i| i.packed_field);
    println!(
        "carrying {} ASET row(s) for block {block}; inherit tier {:?}",
        aset.len(),
        tier.map(|t| t >> 24)
    );
    let pblk = PatchBlock::from_decompressed(&rebuilt, path.clone(), aset, tier)?;
    println!(
        "patch block: {} declared pages, sges {} B",
        pblk.declared_pages(),
        pblk.compressed_data.len()
    );

    if let Some(parent) = Path::new(&out).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let wad = build_patch_wad_multi(std::slice::from_ref(&pblk), 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad).map_err(|e| format!("write {out}: {e}"))?;
    println!("\nWROTE standalone overlay: {out}");
    println!("  {} B  sha256 {}", wad.len(), sha256_hex(&wad));

    // Optional: also emit a variant that MERGES this block into a live vz-patch.wad (append; the live
    // overlay is preserved, our block added, last-wins for c33294).
    if let Some(live) = arg(&args, "--merge-into") {
        let existing = std::fs::read(&live).map_err(|e| format!("read live {live}: {e}"))?;
        let merged = merge_patch_wads(&existing, vec![pblk], false)?;
        let mout = arg(&args, "--merge-out")
            .unwrap_or_else(|| format!("{}-MERGED.wad", out.trim_end_matches(".wad")));
        std::fs::write(&mout, &merged).map_err(|e| format!("write {mout}: {e}"))?;
        println!("\nWROTE merged overlay (live + our block): {mout}");
        println!("  live in : {} ({} B)", live, existing.len());
        println!("  {} B  sha256 {}", merged.len(), sha256_hex(&merged));
    }

    println!("\nMount as data/vz-patch.wad (overlay, last-wins). Gate next: aset_refcheck <wad>");
    Ok(())
}
