//! Build a patch WAD that overrides a shipped `stringdb` with corrected text.
//!
//! This is the Tier-1 (text) delivery path for the unofficial fix pack. It lifts a stringdb block
//! out of a base WAD, rewrites individual strings, re-stamps the container CSUM, and emits a
//! single-block patch WAD.
//!
//! # Why the output WAD name matters
//!
//! WAD overlay resolution is **last-mounted-wins**, and the mount order is
//! (`FUN_004BFAF0` @ `0x004BFAF0`):
//!
//! ```text
//! Loading.wad → loading-patch.wad → <level>.wad → <level>-patch.wad → [gated] → English.wad → English-patch.wad
//! ```
//!
//! `shell.wad` and `vz.wad` SHARE the single `<level>.wad` slot — shell serves the front end, vz
//! serves gameplay — and both ship a byte-identical `English` stringdb. So patching only one of
//! them fixes only half the game. `English-patch.wad` mounts last in *every* session, so in
//! principle one file overrides both. See `docs/fixpack/wad_duplicate_inventory.md`.
//!
//! # Safety
//!
//! By default only **equal-length** rewrites are accepted, which keeps the container layout
//! byte-identical so nothing but the edited text and the CSUM changes — any post-edit difference is
//! then attributable to the edit alone.
//!
//! `--allow-resize` lifts that. Arbitrary-length corrections re-lay-out the container (descriptor
//! offsets and sizes) and re-splice it into the block (the entry table's `chunk_size`). It is gated
//! on `--selftest-resize`, which re-lays-out the shipped container substituting NOTHING and requires
//! the result byte-for-byte: anything the layout logic fails to model shows up there as a diff
//! rather than as a container that validates cleanly and is quietly wrong in someone's game.
//!
//! Usage:
//!   stringdb_patch --source-wad <wad> --out <English-patch.wad>
//!                  [--asset English] [--allow-resize]
//!                  --set-text "Old exact text=New text"      (repeatable)
//!                  --set 0xDEADBEEF=New text                 (repeatable, by key hash)
//!
//!   stringdb_patch --source-wad <wad> --out <unused> --dump-layout      # the container's chunks
//!   stringdb_patch --source-wad <wad> --out <unused> --selftest-resize  # prove the rebuild

use mercs2_formats::crc32::crc32_mercs2;
use mercs2_formats::ffcs::{load_ffcs_archive, read_u32_le};
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::stringdb;
use mercs2_formats::types::TYPE_HASH_STRINGDB;
use mercs2_formats::ucfx::{parse_block_entry_table, verify_ucfx_container};
use std::fs::File;

/// Byte range of a named chunk body inside a UCFX container. Mirrors `ucfx::extract_chunk_body`,
/// but returns the position so the body can be written back rather than only read.
fn find_chunk_range(container: &[u8], tag: &[u8; 4]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    if n_desc > container.len().saturating_sub(20) / 20 {
        return None;
    }
    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > container.len() || &container[row..row + 4] != tag {
            continue;
        }
        let u0 = read_u32_le(container, row + 4);
        if u0 == 0xFFFF_FFFF {
            continue;
        }
        let size = read_u32_le(container, row + 8) as usize;
        let start = if data_area_off > 0 { data_area_off + u0 as usize } else { 8 + u0 as usize };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

/// One chunk body inside a UCFX container, as the descriptor table describes it.
#[derive(Clone, Copy)]
struct Desc {
    /// Byte offset of this descriptor's 20-byte row within the container.
    row: usize,
    tag: [u8; 4],
    /// Offset of the body relative to the data area (`0xFFFF_FFFF` = no body).
    rel: u32,
    size: usize,
    /// Absolute body start within the container, for descriptors that have one.
    start: usize,
}

/// Every descriptor in the container, in table order, with absolute body positions resolved.
fn container_descs(container: &[u8]) -> Result<(usize, Vec<Desc>), String> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let base = if data_area_off > 0 { data_area_off } else { 8 };
    let n_desc = read_u32_le(container, 16) as usize;
    if n_desc > container.len().saturating_sub(20) / 20 {
        return Err(format!("implausible descriptor count {n_desc}"));
    }
    let mut out = Vec::with_capacity(n_desc);
    for i in 0..n_desc {
        let row = 20 + i * 20;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&container[row..row + 4]);
        let rel = read_u32_le(container, row + 4);
        let size = read_u32_le(container, row + 8) as usize;
        let start = if rel == 0xFFFF_FFFF { 0 } else { base + rel as usize };
        if rel != 0xFFFF_FFFF && start + size > container.len() {
            return Err(format!(
                "descriptor {i} ({}) runs past the container: {start}+{size} > {}",
                String::from_utf8_lossy(&tag),
                container.len()
            ));
        }
        out.push(Desc { row, tag, rel, size, start });
    }
    Ok((base, out))
}

/// Print the container's descriptor table.
///
/// Resizing a chunk means re-laying-out every body after it, so the first thing anyone doing that
/// needs is the shipped layout — including the padding between bodies, which is what determines
/// whether a rebuilt container can stay byte-identical everywhere the edit did not reach.
fn dump_layout(container: &[u8]) -> Result<(), String> {
    let (base, descs) = container_descs(container)?;
    println!(
        "container {} bytes, data area at 0x{base:X}, {} descriptor(s)",
        container.len(),
        descs.len()
    );
    let mut bodies: Vec<&Desc> = descs.iter().filter(|d| d.rel != 0xFFFF_FFFF).collect();
    bodies.sort_by_key(|d| d.start);
    let mut cursor = base;
    for d in &bodies {
        let gap = d.start.saturating_sub(cursor);
        println!(
            "  {:<6} rel 0x{:06X}  start 0x{:06X}  size {:>9}  end 0x{:06X}{}",
            String::from_utf8_lossy(&d.tag),
            d.rel,
            d.start,
            d.size,
            d.start + d.size,
            if gap > 0 { format!("   (+{gap} B padding before)") } else { String::new() }
        );
        cursor = d.start + d.size;
    }
    let trailer = container.len().saturating_sub(cursor);
    println!("  tail: {trailer} B after the last body (CSUM trailer is 8)");
    for d in descs.iter().filter(|d| d.rel == 0xFFFF_FFFF) {
        println!("  {:<6} (no body)", String::from_utf8_lossy(&d.tag));
    }
    Ok(())
}

/// Rebuild a container with new bodies for some of its chunks, re-laying out everything after them.
///
/// Measured shape of the shipped `english_P000_Q3` stringdb container, which is what this relies on:
///
/// ```text
///   header + descriptors   0x00..0x50   (20 + 3*20, exactly — no padding)
///   INFO                   0x50         8 B
///   KEYS                   0x58         146,396 B
///   STRS                   0x23C34      1,229,068 B
///   CSUM trailer                        8 B
///   total                               1,375,560 B  (= 0x50 + 8 + 146396 + 1229068 + 8)
/// ```
///
/// Bodies are **contiguous** — no alignment padding anywhere — so the rebuild is header, then bodies
/// in their original order, then the trailer. If a container ever turns up with padding between
/// bodies this REFUSES rather than guessing whether that padding was structural or incidental:
/// silently dropping it would produce a container that still validates and is subtly wrong.
fn rebuild_container(
    container: &[u8],
    replacements: &[([u8; 4], &[u8])],
) -> Result<Vec<u8>, String> {
    let (base, descs) = container_descs(container)?;

    let mut bodies: Vec<Desc> = descs.iter().copied().filter(|d| d.rel != 0xFFFF_FFFF).collect();
    bodies.sort_by_key(|d| d.start);

    // The invariant this rebuild depends on. Checked, not assumed.
    let mut cursor = base;
    for d in &bodies {
        if d.start != cursor {
            return Err(format!(
                "chunk {} starts at 0x{:X} but the previous body ended at 0x{cursor:X} — this \
                 container has padding between bodies, and reproducing that layout is not \
                 implemented. Refusing rather than emitting a container that validates and is wrong.",
                String::from_utf8_lossy(&d.tag),
                d.start
            ));
        }
        cursor += d.size;
    }
    if container.len() != cursor + 8 {
        return Err(format!(
            "container is {} B but bodies end at 0x{cursor:X} + an 8 B trailer = {} B; unexpected \
             tail, refusing",
            container.len(),
            cursor + 8
        ));
    }

    let mut out = Vec::with_capacity(container.len());
    out.extend_from_slice(&container[..base]);

    // Bodies in their shipped order, substituting the new ones, recording each new relative offset.
    let mut new_pos: Vec<(usize, u32, usize)> = Vec::new(); // (row, rel, size)
    for d in &bodies {
        let rel = (out.len() - base) as u32;
        let body: &[u8] = match replacements.iter().find(|(t, _)| *t == d.tag) {
            Some((_, b)) => b,
            None => &container[d.start..d.start + d.size],
        };
        out.extend_from_slice(body);
        new_pos.push((d.row, rel, body.len()));
    }

    // Trailer, with a placeholder CRC that restamp_csum overwrites.
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&[0u8; 4]);

    // Re-point the descriptors. Descriptor COUNT and order never change, so the header and the rows'
    // other fields stay exactly as shipped.
    for (row, rel, size) in new_pos {
        out[row + 4..row + 8].copy_from_slice(&rel.to_le_bytes());
        out[row + 8..row + 12].copy_from_slice(&(size as u32).to_le_bytes());
    }
    Ok(out)
}

/// Re-stamp the trailing `CSUM` (JAMCRC over everything before the 8-byte trailer).
fn restamp_csum(container: &mut [u8]) -> Result<u32, String> {
    let n = container.len();
    if n < 8 || &container[n - 8..n - 4] != b"CSUM" {
        return Err("container has no CSUM trailer".into());
    }
    let crc = crc32_mercs2(&container[..n - 8]);
    container[n - 4..].copy_from_slice(&crc.to_le_bytes());
    Ok(crc)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("stringdb_patch: error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (mut source, mut out, mut asset) = (None, None, "English".to_string());
    let mut by_text: Vec<(String, String)> = Vec::new();
    let mut by_hash: Vec<(u32, String)> = Vec::new();
    let mut allow_resize = false;
    let mut dump_layout_only = false;
    let mut selftest = false;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source-wad" => source = it.next(),
            "--out" => out = it.next(),
            "--asset" => asset = it.next().ok_or("--asset needs a name")?,
            "--allow-resize" => allow_resize = true,
            "--dump-layout" => dump_layout_only = true,
            "--selftest-resize" => selftest = true,
            "--set-text" => {
                let v = it.next().ok_or("--set-text needs OLD=NEW")?;
                let (o, n) = v.split_once('=').ok_or("--set-text wants OLD=NEW")?;
                by_text.push((o.to_string(), n.to_string()));
            }
            "--set" => {
                let v = it.next().ok_or("--set needs 0xHASH=NEW")?;
                let (h, n) = v.split_once('=').ok_or("--set wants 0xHASH=NEW")?;
                let h = h.trim().trim_start_matches("0x");
                by_hash.push((u32::from_str_radix(h, 16).map_err(|e| format!("bad hash: {e}"))?, n.to_string()));
            }
            o => return Err(format!("unknown arg {o}")),
        }
    }
    let source = source.ok_or("--source-wad required")?;
    let out = out.ok_or("--out required")?;
    if by_text.is_empty() && by_hash.is_empty() && !dump_layout_only && !selftest {
        return Err("at least one --set-text or --set required".into());
    }

    let mut f = File::open(&source).map_err(|e| format!("open {source}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("parse {source}: {e}"))?;

    // Locate the stringdb asset's home block.
    let hash = pandemic_hash_m2(&asset);
    let primary = ar
        .aset
        .iter()
        .find(|e| e.asset_hash == hash && e.is_primary())
        .ok_or_else(|| format!("asset '{asset}' (0x{hash:08X}): no primary ASET row in {source}"))?;
    let bi = primary.block_index() as usize;
    let path = ar.paths.get(bi).cloned().ok_or_else(|| format!("block {bi} has no path"))?;
    println!("asset '{asset}' (0x{hash:08X}) -> block {bi} '{path}'");

    let decomp = decompress_block(&mut f, &ar.indx, bi as u16)?;
    let original_len = decomp.len();
    let mut block = decomp.clone();

    // Walk the entry table to find the stringdb container's byte range in the block.
    let (count, entries) = parse_block_entry_table(&block);
    let mut pos = 4 + count as usize * 16;
    let mut target: Option<(usize, usize)> = None;
    let mut target_row = 0usize;   /* the entry table row, so a resize can update its chunk_size */
    for (i, e) in entries.iter().enumerate() {
        let (start, end) = (pos, pos + e.chunk_size as usize);
        if e.type_hash == TYPE_HASH_STRINGDB && target.is_none() {
            target = Some((start, end));
            target_row = 4 + i * 16;
        }
        pos = end;
    }
    let (cstart, cend) = target.ok_or("no stringdb container in that block")?;
    println!("stringdb container at [0x{cstart:X}..0x{cend:X}] ({} bytes)", cend - cstart);

    let container = &block[cstart..cend];
    if dump_layout_only {
        return dump_layout(container);
    }
    if selftest {
        // ★ The gate for --allow-resize, in the same spirit as stringdb_roundtrip: re-lay-out the
        // container substituting NOTHING, and require the result byte-for-byte. Anything the layout
        // logic fails to model — a padding rule, a header field that tracks a size, a descriptor
        // field beyond offset/size — shows up here as a diff rather than as a container that
        // validates cleanly and is quietly wrong in someone's game.
        let mut rebuilt = rebuild_container(container, &[])?;
        restamp_csum(&mut rebuilt)?;   // the rebuild leaves a placeholder CRC for its caller
        if rebuilt.len() != container.len() {
            return Err(format!(
                "SELFTEST FAILED: no-op rebuild is {} B, shipped is {} B",
                rebuilt.len(),
                container.len()
            ));
        }
        match rebuilt.iter().zip(container.iter()).position(|(a, b)| a != b) {
            Some(i) => {
                return Err(format!(
                    "SELFTEST FAILED: no-op rebuild differs at byte 0x{i:X} (got 0x{:02X}, \
                     shipped 0x{:02X})",
                    rebuilt[i], container[i]
                ))
            }
            None => {
                println!("SELFTEST OK: no-op rebuild of {} B is byte-identical", rebuilt.len());
                return Ok(());
            }
        }
    }
    // PC containers tag these KEYS/STRS; the SYEK/SRTS in format_reference.md is the Xbox
    // byte order read as ASCII. Try both so this also works on a big-endian source.
    let (ktag, stag) = [(b"KEYS", b"STRS"), (b"SYEK", b"SRTS")]
        .into_iter()
        .find(|(k, s)| find_chunk_range(container, k).is_some() && find_chunk_range(container, s).is_some())
        .ok_or("container has neither KEYS/STRS nor SYEK/SRTS")?;
    let (koff, klen) = find_chunk_range(container, ktag).unwrap();
    let (soff, slen) = find_chunk_range(container, stag).unwrap();

    let mut db = stringdb::parse(&container[koff..koff + klen], &container[soff..soff + slen])?;
    println!("parsed {} keys, {} B heap, {:?}", db.entries.len(), db.heap_bytes, db.endian);

    // Apply edits. A miss is fatal: a fix that silently does nothing is worse than a failed build.
    for (old, new) in &by_text {
        let n = db.replace_exact_text(old, new);
        if n == 0 {
            return Err(format!("--set-text: no string exactly matches {old:?}"));
        }
        println!("  set-text {old:?} -> {new:?}  ({n} entr{})", if n == 1 { "y" } else { "ies" });
    }
    for (h, new) in &by_hash {
        if !db.set_by_hash(*h, new) {
            return Err(format!("--set: key 0x{h:08X} not present in this stringdb"));
        }
        println!("  set 0x{h:08X} -> {new:?}");
    }

    let (nk, ns) = stringdb::build(&db);
    let resized = nk.len() != klen || ns.len() != slen;

    if resized && !allow_resize {
        return Err(format!(
            "edit changes chunk sizes (KEYS {klen}->{}, STRS {slen}->{}). Equal-length edits keep \
             the container layout byte-identical, so any post-edit difference is attributable to \
             the edit alone — pass --allow-resize to re-lay-out the container instead.",
            nk.len(),
            ns.len()
        ));
    }

    let (crc, new_cend);
    if resized {
        // Re-lay-out the container, then re-splice it into the block and correct the entry table's
        // chunk_size for this entry — the containers after it shift, and the block is a flat
        // concatenation, so nothing else needs touching.
        let mut rebuilt = rebuild_container(&block[cstart..cend], &[(*ktag, &nk), (*stag, &ns)])?;
        crc = restamp_csum(&mut rebuilt)?;
        println!(
            "resized: KEYS {klen} -> {}, STRS {slen} -> {}, container {} -> {} B",
            nk.len(),
            ns.len(),
            cend - cstart,
            rebuilt.len()
        );
        let mut nb = Vec::with_capacity(block.len() + rebuilt.len());
        nb.extend_from_slice(&block[..cstart]);
        nb.extend_from_slice(&rebuilt);
        nb.extend_from_slice(&block[cend..]);
        new_cend = cstart + rebuilt.len();
        block = nb;
        block[target_row + 12..target_row + 16]
            .copy_from_slice(&((new_cend - cstart) as u32).to_le_bytes());
    } else {
        block[cstart + koff..cstart + koff + klen].copy_from_slice(&nk);
        block[cstart + soff..cstart + soff + slen].copy_from_slice(&ns);
        crc = restamp_csum(&mut block[cstart..cend])?;
        new_cend = cend;
    }
    println!("re-stamped CSUM = 0x{crc:08X}");
    let cend = new_cend;

    // Self-gate: never emit a container we cannot validate. A bad CSUM makes the engine reject the
    // block, which would look exactly like "the patch route doesn't work" and send us chasing the
    // wrong thing.
    if let Some(issues) = verify_ucfx_container(&block[cstart..cend], "patched stringdb", TYPE_HASH_STRINGDB) {
        for i in &issues {
            eprintln!("  ISSUE {}: {}", i.context, i.detail);
        }
        return Err(format!("patched container failed UCFX validation ({} issue(s))", issues.len()));
    }
    println!("UCFX validation: OK (CSUM + descriptor bounds)");

    if !resized {
        assert_eq!(block.len(), original_len, "equal-length edit must not change block size");
    }
    if resized {
        println!("block {original_len} -> {} B", block.len());
    } else {
        let changed = block.iter().zip(decomp.iter()).filter(|(a, b)| a != b).count();
        println!("{changed} byte(s) differ from the shipped block");
    }

    // Carry every ASET row pointing at this block so the block's full advertisement survives.
    let aset: Vec<AsetEntry> = ar
        .aset
        .iter()
        .filter(|e| e.block_index() as usize == bi)
        .map(|e| AsetEntry::new(e.asset_hash, e.secondary_ref, e.packed_block_ref, e.type_id))
        .collect();
    let tier = ar.indx.get(bi).map(|i| i.packed_field);
    let blk = PatchBlock::from_decompressed(&block, path.clone(), aset, tier)?;
    println!("patch block: {} aset rows, {} pages declared", blk.aset_entries.len(), blk.declared_pages());

    let wad = build_patch_wad_multi(&[blk], 0, None, &FFCS_CERT_BLOB)?;
    std::fs::write(&out, &wad).map_err(|e| format!("write {out}: {e}"))?;
    println!("Wrote {out} ({} bytes)", wad.len());
    Ok(())
}
