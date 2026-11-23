//! `model-chains` — every model's LOD chain as its ASET row names it, one row per (model, rung).
//!
//! An edit that re-emits a model has to carry each rung the row names, so this measures what a reader
//! of that chain may assume: that each rung block holds the model's `(hash, model)` entry exactly once
//! (the engine's rung gate spins on a block lacking it or holding it twice), that the block's INDX tier
//! byte is the rung's `N` (the stream manager's eviction class, copied to `node+0x18`), that the path
//! ends `_P00N_Q(3-N)`, and that only `_P000` carries a destruction state machine. It also counts the
//! rung blocks other assets share. Exit 1 when any rung breaks one of those.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::File;
use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_MODEL, TYPE_ID_MODEL};
use mercs2_formats::ucfx::parse_block_entry_table;

/// The block indices of a row's rungs, `_P000` first; `None` where the slot is the sentinel.
fn rungs(e: &mercs2_formats::ffcs::AsetEntry) -> [Option<u16>; 4] {
    let s = |v: u16| (v != 0xFFFF).then_some(v);
    [
        Some(e.block_index()),
        s((e.packed_block_ref & 0xFFFF) as u16),
        s((e.secondary_ref >> 16) as u16),
        s((e.secondary_ref & 0xFFFF) as u16),
    ]
}

pub fn model_chains(wad: &Path, out: &Path) -> Result<bool, String> {
    let mut file = File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", wad.display()))?;

    // Which (row, rung) pairs each block serves, so every block is inflated once.
    let rows: Vec<&mercs2_formats::ffcs::AsetEntry> = archive.aset.iter().filter(|e| e.type_id == TYPE_ID_MODEL).collect();
    let mut wanted: BTreeMap<u16, Vec<(usize, usize)>> = BTreeMap::new();
    let mut gaps = 0usize;
    for (r, e) in rows.iter().enumerate() {
        let chain = rungs(e);
        let mut seen_gap = false;
        for (n, b) in chain.iter().enumerate() {
            match b {
                Some(b) if seen_gap => {
                    gaps += 1;
                    wanted.entry(*b).or_default().push((r, n));
                }
                Some(b) => wanted.entry(*b).or_default().push((r, n)),
                None => seen_gap = true,
            }
        }
    }

    // Each rung's entry: (third table word, container length), to test the word against the byte sum of
    // the rungs before it.
    let mut words: BTreeMap<(usize, usize), (u32, u32)> = BTreeMap::new();
    let mut tsv = String::from("model\trung\tblock\tpath\tholdings\ttier\tpath_matches\tother_assets\tmachine\tfield_c\tlen\n");
    let (mut bad_holdings, mut bad_tier, mut bad_path, mut machine_off_p000) = (0usize, 0usize, 0usize, 0usize);
    let (mut rung_rows, mut shared_finer, mut multi_rung_machines) = (0usize, BTreeSet::new(), BTreeSet::new());
    let mut machine_models = BTreeSet::new();
    let mut per_rung = [0usize; 4];
    for (&b, uses) in &wanted {
        let dec = decompress_block(&mut file, &archive.indx, b)?;
        let (_, entries) = parse_block_entry_table(&dec);
        let mut spans = Vec::with_capacity(entries.len());
        let mut at = 4 + entries.len() * 16;
        for e in &entries {
            let end = at + e.chunk_size as usize;
            spans.push((e.name_hash, e.type_hash, at..end.min(dec.len()), e.field_c));
            at = end;
        }
        let path = archive.paths.get(b as usize).cloned().unwrap_or_default();
        let tier = archive.indx[b as usize].packed_field >> 24;
        for &(r, n) in uses {
            let hash = rows[r].asset_hash;
            let mine: Vec<_> = spans.iter().filter(|(h, t, _, _)| *h == hash && *t == TYPE_HASH_MODEL).collect();
            let others = spans.iter().filter(|(h, _, _, _)| *h != hash).map(|(h, _, _, _)| *h).collect::<BTreeSet<_>>().len();
            let machine = mine
                .first()
                .and_then(|(_, _, s, _)| dec.get(s.clone()))
                .is_some_and(|c| mercs2_formats::orchestrator::parse_state_machine(c).is_some());
            let (field_c, len) = mine.first().map_or((u32::MAX, 0), |(_, _, s, w)| (*w, s.len() as u32));
            words.insert((r, n), (field_c, len));
            let suffix = format!("_p{n:03}_q{}.block", 3 - n);
            let path_matches = path.to_lowercase().ends_with(&suffix);
            rung_rows += 1;
            per_rung[n] += 1;
            if mine.len() != 1 {
                bad_holdings += 1;
            }
            if tier != n as u32 {
                bad_tier += 1;
            }
            if !path_matches {
                bad_path += 1;
            }
            if machine {
                machine_models.insert(hash);
                if n != 0 {
                    machine_off_p000 += 1;
                }
            }
            if n > 0 && others > 0 {
                shared_finer.insert(b);
            }
            writeln!(tsv, "0x{hash:08X}\t{n}\t{b}\t{path}\t{}\t{tier}\t{path_matches}\t{others}\t{machine}\t{field_c}\t{len}", mine.len())
                .map_err(|e| e.to_string())?;
        }
    }
    // The third word of rung N against the lengths of rungs 0..N.
    let (mut word_is_sum, mut word_other) = ([0usize; 4], [0usize; 4]);
    let mut word_examples = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        let mut sum = 0u32;
        for n in 0..4 {
            let Some(&(w, len)) = words.get(&(r, n)) else { break };
            if w == sum {
                word_is_sum[n] += 1;
            } else {
                word_other[n] += 1;
                if word_examples.len() < 5 {
                    word_examples.push(format!("0x{:08X} rung {n}: word {w}, sum before {sum}", row.asset_hash));
                }
            }
            sum = sum.wrapping_add(len);
        }
    }
    for e in &rows {
        if machine_models.contains(&e.asset_hash) && rungs(e)[1..].iter().any(Option::is_some) {
            multi_rung_machines.insert(e.asset_hash);
        }
    }
    std::fs::write(out, tsv).map_err(|e| format!("{}: {e}", out.display()))?;

    let distinct: BTreeSet<u32> = rows.iter().map(|e| e.asset_hash).collect();
    println!("[model-chains] {}: {} model rows, {} distinct hashes", wad.display(), rows.len(), distinct.len());
    println!("  rungs named: {rung_rows} (P000 {}, P001 {}, P002 {}, P003 {})", per_rung[0], per_rung[1], per_rung[2], per_rung[3]);
    println!("  rungs past a sentinel gap: {gaps}");
    println!("  holdings != 1: {bad_holdings}");
    println!("  INDX tier != N: {bad_tier}");
    println!("  path not _P00N_Q(3-N): {bad_path}");
    println!("  models with a state machine: {} ({} with finer rungs)", machine_models.len(), multi_rung_machines.len());
    println!("  state machine outside _P000: {machine_off_p000}");
    println!("  finer-rung blocks shared with other assets: {}", shared_finer.len());
    println!("  entry word == byte sum of earlier rungs, by rung: {word_is_sum:?}; otherwise: {word_other:?}");
    for x in &word_examples {
        println!("    {x}");
    }
    println!("  rows: {}", out.display());
    Ok(gaps == 0 && bad_holdings == 0 && bad_tier == 0 && bad_path == 0 && machine_off_p000 == 0)
}
