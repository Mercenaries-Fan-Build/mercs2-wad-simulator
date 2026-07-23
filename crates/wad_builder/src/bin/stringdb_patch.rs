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
//! byte-identical so nothing but the edited text and the CSUM changes. `--allow-resize` lifts that,
//! but then descriptor offsets and the entry table must be rewritten — not yet implemented, so it
//! currently refuses rather than emitting a subtly-wrong container.
//!
//! Usage:
//!   stringdb_patch --source-wad <wad> --out <English-patch.wad>
//!                  [--asset English]
//!                  --set-text "Old exact text=New text"      (repeatable)
//!                  --set 0xDEADBEEF=New text                 (repeatable, by key hash)

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

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--source-wad" => source = it.next(),
            "--out" => out = it.next(),
            "--asset" => asset = it.next().ok_or("--asset needs a name")?,
            "--allow-resize" => allow_resize = true,
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
    if by_text.is_empty() && by_hash.is_empty() {
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
    for e in entries.iter() {
        let (start, end) = (pos, pos + e.chunk_size as usize);
        if e.type_hash == TYPE_HASH_STRINGDB && target.is_none() {
            target = Some((start, end));
        }
        pos = end;
    }
    let (cstart, cend) = target.ok_or("no stringdb container in that block")?;
    println!("stringdb container at [0x{cstart:X}..0x{cend:X}] ({} bytes)", cend - cstart);

    let container = &block[cstart..cend];
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
    if nk.len() != klen || ns.len() != slen {
        if !allow_resize {
            return Err(format!(
                "edit changes chunk sizes (KEYS {klen}->{}, STRS {slen}->{}). Equal-length edits \
                 keep the container layout byte-identical; resizing needs descriptor + entry-table \
                 rewriting, which is not implemented. Re-run with same-length text.",
                nk.len(),
                ns.len()
            ));
        }
        return Err("--allow-resize is not implemented yet (descriptor/entry-table rewrite needed)".into());
    }

    block[cstart + koff..cstart + koff + klen].copy_from_slice(&nk);
    block[cstart + soff..cstart + soff + slen].copy_from_slice(&ns);
    let crc = restamp_csum(&mut block[cstart..cend])?;
    println!("re-stamped CSUM = 0x{crc:08X}");

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

    assert_eq!(block.len(), original_len, "equal-length edit must not change block size");
    let changed = block.iter().zip(decomp.iter()).filter(|(a, b)| a != b).count();
    println!("{changed} byte(s) differ from the shipped block");

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
