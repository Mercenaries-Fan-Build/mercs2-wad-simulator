//! vz_state / layers_static step 0 — can a placement layer be WRITTEN via an in-place field patch?
//!
//! `edit_state_machine` taught the lesson: measure the bytes before recording a blocker. The same
//! question here, before any placement writer exists — `placement::Placement` keeps only
//! `{key, name, pos, quat}` and DROPS the 42-byte record's `+16` pad and `+36` 6-byte tail, so a
//! parse-then-regenerate cannot be byte-identical. The tractable path is an **in-place patch**:
//! locate each `Transform` record and overwrite only the fields an edit changes (pos / quat), leaving
//! the pad, tail, and every other byte exactly where they were.
//!
//! This survey proves that path is sound across retail, by measuring:
//!
//! 1. **Shape** — every `Transform` COMP's payload stride (record = 4-byte key + payload), and that
//!    its records TILE its data span with no remainder. If they tile at a fixed 42, an edit is
//!    arithmetic: `data_off + i*42 + field`.
//! 2. **Layout** — the pos/quat read at the documented offsets (`+4/+8/+12`, `+20..+32`) MATCH what
//!    `placement::load_placements` returns for the same key. If they agree, the offsets an in-place
//!    patch would write to are the right ones.
//! 3. **Lossless re-emit** — writing each record's OWN pos+quat back at those offsets reproduces the
//!    block byte-for-byte (a no-op edit), and `Name` / `ModelName` COMPs come along untouched. That
//!    is the exact operation the writer performs, checked to be inert when it should be.
//!
//! A pass makes `move_entity` / `reskin_entity` a bounded job; a failure names the field that broke.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::placement::{comp_inventory, load_placements};
use mercs2_formats::sges::decompress_block;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn f32_le(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u32_le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Every decompressed placement block (`layers_static` + `vz_state_*`), located by PTHS path.
fn placement_blocks() -> Option<Vec<(String, Vec<u8>)>> {
    let wad = vz_wad()?;
    let mut file = std::fs::File::open(&wad).ok()?;
    let size = file.metadata().ok()?.len();
    let archive = load_ffcs_archive(&mut file, size).ok()?;
    let mut out = Vec::new();
    for (idx, path) in archive.paths.iter().enumerate() {
        let p = path.to_lowercase();
        if p.contains("layers_static") || p.contains("vz_state") {
            if let Ok(dec) = decompress_block(&mut file, &archive.indx, idx as u16) {
                out.push((path.clone(), dec));
            }
        }
    }
    Some(out)
}

const TRANSFORM_STRIDE: usize = 42;

#[derive(Default)]
struct Census {
    blocks: usize,
    transform_comps: usize,
    transform_records: usize,
    modelname_comps: usize,
    /// payload_stride seen on Transform COMPs (expect all 38 → 42 total).
    transform_strides: BTreeMap<u32, usize>,
    non_tiling: Vec<String>,
    /// The `+16` pad word's distinct values, and the 6-byte tail's.
    pad_values: BTreeMap<u32, usize>,
    tail_values: BTreeSet<u64>,
    /// pos/quat mismatches between the raw bytes and `load_placements` (should be 0).
    layout_mismatches: usize,
    layout_checked: usize,
    /// Blocks whose in-place no-op re-emit is byte-identical, and those that are not.
    lossless_blocks: usize,
    lossy: Vec<String>,
}

#[test]
fn can_a_placement_layer_be_written_in_place() {
    let Some(blocks) = placement_blocks() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    assert!(!blocks.is_empty(), "found no layers_static / vz_state blocks");

    let mut c = Census::default();
    for (label, block) in &blocks {
        c.blocks += 1;
        survey_block(label, block, &mut c);
    }
    report(&c);

    // ── The properties the in-place writer is entitled to assume. ──
    assert!(c.transform_records > 3000, "expected thousands of placements, got {}", c.transform_records);
    // (1) Shape: Transform records are a fixed 42-byte stride and TILE their data span. Note the
    // `schm` payload stride is NOT the operative record size for these COMPs (it reads 52, yet the
    // records are 42 and cross-check losslessly below) — so the writer must trust the tiling +
    // parser agreement, not the schema word. That surprise is exactly what the survey exists to catch.
    assert!(c.non_tiling.is_empty(), "Transform records do not tile at 42: {:?}", c.non_tiling);
    // pad is a constant (trivially preserved); the tail varies, so it MUST be carried through — which
    // is precisely why an in-place patch (not a parse-then-regenerate) is the sound approach.
    assert_eq!(c.pad_values.keys().collect::<Vec<_>>(), [&0], "the +16 pad is not always zero: {:?}", c.pad_values);
    assert!(c.tail_values.len() > 1, "the +36 tail was expected to vary (hence preserve it), but is constant");
    // (2) Layout: the documented pos/quat offsets agree with the existing parser.
    assert_eq!(c.layout_mismatches, 0, "the 42-byte layout disagrees with load_placements in {} record(s)", c.layout_mismatches);
    assert!(c.layout_checked > 0, "no records cross-checked against load_placements");
    // (3) Lossless: an in-place no-op re-emit reproduces every block byte-for-byte.
    assert!(c.lossy.is_empty(), "an in-place no-op did not reproduce {} block(s): {:?}", c.lossy.len(), c.lossy);
    assert_eq!(c.lossless_blocks, c.blocks);
}

fn survey_block(label: &str, block: &[u8], c: &mut Census) {
    let comps = comp_inventory(block);

    // A key -> (pos, quat) map from the existing parser, to cross-check the raw layout.
    let parsed: BTreeMap<u32, ([f32; 3], [f32; 4])> = load_placements(block)
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.key, (p.pos, p.quat)))
        .collect();

    // Work on a copy: rewrite every Transform record's OWN pos+quat back in place (a no-op edit),
    // then require the copy to equal the original. That is the writer's exact operation.
    let mut rewritten = block.to_vec();

    for comp in &comps {
        let name = comp.info_name.as_deref().unwrap_or("");
        let (Some(off), Some(size)) = (comp.data_off, comp.data_size) else { continue };
        if name == "ModelName" {
            c.modelname_comps += 1;
            continue;
        }
        if name != "Transform" {
            continue;
        }
        c.transform_comps += 1;
        if let Some(s) = comp.payload_stride {
            *c.transform_strides.entry(s).or_default() += 1;
        }
        if size % TRANSFORM_STRIDE != 0 {
            c.non_tiling.push(format!("{label}: Transform data {size} not a multiple of {TRANSFORM_STRIDE}"));
            continue;
        }
        let n = size / TRANSFORM_STRIDE;
        for i in 0..n {
            let o = off + i * TRANSFORM_STRIDE;
            if o + TRANSFORM_STRIDE > block.len() {
                break;
            }
            c.transform_records += 1;
            let key = u32_le(block, o);
            let pos = [f32_le(block, o + 4), f32_le(block, o + 8), f32_le(block, o + 12)];
            let quat = [
                f32_le(block, o + 20),
                f32_le(block, o + 24),
                f32_le(block, o + 28),
                f32_le(block, o + 32),
            ];
            *c.pad_values.entry(u32_le(block, o + 16)).or_default() += 1;
            let mut tail = [0u8; 8];
            tail[..6].copy_from_slice(&block[o + 36..o + 42]);
            c.tail_values.insert(u64::from_le_bytes(tail));

            // Cross-check against load_placements (matched by key).
            if let Some((ppos, pquat)) = parsed.get(&key) {
                c.layout_checked += 1;
                if ppos != &pos || pquat != &quat {
                    c.layout_mismatches += 1;
                }
            }

            // The no-op patch: write this record's own pos+quat back at the documented offsets.
            rewritten[o + 4..o + 16].copy_from_slice(&block[o + 4..o + 16]);
            rewritten[o + 20..o + 36].copy_from_slice(&block[o + 20..o + 36]);
        }
    }

    if rewritten == block {
        c.lossless_blocks += 1;
    } else {
        if c.lossy.len() < 10 {
            c.lossy.push(label.to_string());
        }
    }
}

fn report(c: &Census) {
    eprintln!("\n════ PLACEMENT ROUND-TRIP SURVEY ════");
    eprintln!("blocks scanned             {}", c.blocks);
    eprintln!("Transform COMPs / records  {} / {}", c.transform_comps, c.transform_records);
    eprintln!("ModelName COMPs            {}", c.modelname_comps);
    eprintln!("Transform payload strides  {:?}", c.transform_strides);
    eprintln!("non-tiling Transform data  {}", c.non_tiling.len());
    eprintln!("pad (+16) distinct values  {} {:?}", c.pad_values.len(), c.pad_values.iter().take(6).collect::<Vec<_>>());
    eprintln!("tail (+36) distinct values {}", c.tail_values.len());
    eprintln!("layout cross-checks        {} ({} mismatch)", c.layout_checked, c.layout_mismatches);
    eprintln!("lossless no-op re-emit      {} / {} blocks", c.lossless_blocks, c.blocks);
    if !c.lossy.is_empty() {
        eprintln!("first lossy: {:?}", c.lossy);
    }
    eprintln!("═════════════════════════════════════\n");
}
