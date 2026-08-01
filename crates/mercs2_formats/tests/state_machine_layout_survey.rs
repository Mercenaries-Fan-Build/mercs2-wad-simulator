//! Is the destruction family's LEAF LAYOUT canonical enough to REGENERATE from the parsed model?
//!
//! The same-shape writer overlays onto existing leaves, so it never has to know the exact leaf
//! sequence. A writer that ADDS or REMOVES states must generate that sequence — which is only
//! byte-identical on a no-op if retail follows one fixed rule. This measures the rule:
//!
//! - Does every state emit BOTH an Enter and an Exit `CHDR`/`CEXE`, or are some omitted?
//! - In what order (Enter before Exit)?
//! - Is `INFO` always `[5, switch_count, node_count]`, and does `switch_count == SWIT words`?
//! - Is the leaf order exactly `INFO, (NODE, (STAT, CHDR, CEXE, CHDR, CEXE)*)*, SWIT`?
//!
//! If those hold across all 1,311 destructibles, a generator that emits that exact layout
//! reproduces any retail family byte-for-byte, and adding a state is just emitting one more of the
//! per-state group. Whatever it finds is recorded here so the generator can rely on it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::orchestrator::{LIST_ENTER, LIST_EXIT};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_ID_MODEL;
use mercs2_formats::ucfx::parse_block_entry_table;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() { 0 } else { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) }
}

#[derive(Clone, Copy)]
struct Row {
    tag: [u8; 4],
    off: u32,
    size: u32,
    kids: u32,
}

fn rows_of(buf: &[u8]) -> (usize, Vec<Row>) {
    let mut rows = Vec::new();
    if buf.len() < 20 {
        return (0, rows);
    }
    let data_off = u32_le(buf, 4) as usize;
    let ndesc = u32_le(buf, 16) as usize;
    for d in 0..ndesc {
        let ro = 20 + d * 20;
        if ro + 20 > buf.len() {
            break;
        }
        rows.push(Row {
            tag: [buf[ro], buf[ro + 1], buf[ro + 2], buf[ro + 3]],
            off: u32_le(buf, ro + 4),
            size: u32_le(buf, ro + 8),
            kids: u32_le(buf, ro + 16),
        });
    }
    (data_off, rows)
}

fn children_of(rows: &[Row], p: usize) -> Vec<usize> {
    let end = (p + rows[p].kids as usize + 1).min(rows.len());
    let mut out = Vec::new();
    let mut i = p + 1;
    while i < end {
        out.push(i);
        i += rows[i].kids as usize + 1;
    }
    out
}

fn family_parent(rows: &[Row]) -> Option<usize> {
    (0..rows.len())
        .find(|&p| rows[p].kids > 0 && children_of(rows, p).iter().any(|&c| &rows[c].tag == b"NODE"))
}

#[derive(Default)]
struct Census {
    families: usize,
    /// The per-state Enter/Exit shape: which lists a state carries, in order. Keyed by a compact
    /// spelling like "E,X" (enter then exit), "E", "X", "" (neither).
    state_shapes: BTreeMap<String, usize>,
    /// Whole-family leaf-order signatures that are NOT the canonical
    /// `INFO (NODE (STAT CHDR CEXE)*)* SWIT`.
    noncanonical: Vec<String>,
    info_word0: BTreeSet<u32>,
    info_switch_eq_swit: usize,
    info_switch_ne_swit: usize,
    /// Does a state ever repeat a list (two Enters)? That would break "one CHDR per list".
    repeated_list: usize,
    /// Among "EX" states, is the Exit list ever empty? If never, then "has an Exit leaf" == "exit
    /// list is non-empty" and a generator can decide purely from the model.
    ex_empty_exit: usize,
    ex_states: usize,
    /// Among "E" states, is the Enter list ever empty?
    e_empty_enter: usize,
    e_states: usize,
    /// The rule under test: a state emits a list's CHDR/CEXE leaf IFF that list is non-empty. These
    /// must both be 0 for it to hold — a leaf present with an empty list, or (impossible via parse) a
    /// non-empty list with no leaf.
    enter_leaf_but_empty: usize,
    exit_leaf_but_empty: usize,
    states_total: usize,
    states_no_leaves: usize,
}

#[test]
fn how_canonical_is_the_family_layout() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let mut file = std::fs::File::open(&wad).expect("open");
    let size = file.metadata().unwrap().len();
    let archive = load_ffcs_archive(&mut file, size).expect("ffcs");
    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    for a in archive.aset.iter().filter(|a| a.type_id == TYPE_ID_MODEL) {
        for b in a.lod_chain() {
            if b != 0xFFFF {
                blocks.insert(b);
            }
        }
    }

    let mut c = Census::default();
    for bi in blocks {
        let Ok(dec) = decompress_block(&mut file, &archive.indx, bi) else { continue };
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = (pos + e.chunk_size as usize).min(dec.len());
            if pos >= end {
                break;
            }
            survey(&dec[pos..end], &mut c);
            pos = end;
        }
    }

    eprintln!("\n════ FAMILY LEAF-LAYOUT SURVEY ════");
    eprintln!("families                 {}", c.families);
    eprintln!("per-state Enter/Exit shapes (order matters): {:?}", c.state_shapes);
    eprintln!("INFO word0 distinct      {:?}", c.info_word0);
    eprintln!("INFO switch_count == SWIT words: {} ;  != : {}", c.info_switch_eq_swit, c.info_switch_ne_swit);
    eprintln!("states repeating a list  {}", c.repeated_list);
    eprintln!("EX states {} of which empty-exit {}", c.ex_states, c.ex_empty_exit);
    eprintln!("E  states {} of which empty-enter {}", c.e_states, c.e_empty_enter);
    eprintln!("states total {}  with NO command leaves {}", c.states_total, c.states_no_leaves);
    eprintln!(
        "RULE (leaf iff non-empty): enter_leaf_but_empty {}  exit_leaf_but_empty {}",
        c.enter_leaf_but_empty, c.exit_leaf_but_empty
    );
    eprintln!("non-canonical families   {}", c.noncanonical.len());
    for s in c.noncanonical.iter().take(10) {
        eprintln!("    {s}");
    }
    eprintln!("═══════════════════════════════════\n");
}

fn survey(container: &[u8], c: &mut Census) {
    let (data_off, rows) = rows_of(container);
    if rows.is_empty() {
        return;
    }
    let Some(parent) = family_parent(&rows) else { return };
    let Some(sm) = mercs2_formats::orchestrator::parse_state_machine(container) else { return };
    c.families += 1;
    let kids = children_of(&rows, parent);
    let leaf = |r: &Row| -> &[u8] {
        let s = data_off + r.off as usize;
        let e = (s + r.size as usize).min(container.len());
        if s > e { &[] } else { &container[s..e] }
    };

    // Per-state list shape (E / EX), correlated with the PARSED list lengths so emptiness is known.
    let flat: Vec<&mercs2_formats::orchestrator::StateDef> =
        sm.nodes.iter().flat_map(|n| n.states.iter()).collect();
    let mut sig = String::new();
    let mut state_idx = 0usize; // increments on STAT; the just-STARTed state is state_idx-1
    let mut cur: Vec<char> = Vec::new();
    let mut seen_lists: BTreeSet<char> = BTreeSet::new();

    for &k in &kids {
        let r = rows[k];
        match &r.tag {
            b"INFO" => {
                sig.push('I');
                c.info_word0.insert(u32_le(leaf(&r), 0));
            }
            b"NODE" => {
                flush_state(&mut cur, &mut seen_lists, state_idx, &flat, c);
                sig.push('N');
            }
            b"STAT" => {
                flush_state(&mut cur, &mut seen_lists, state_idx, &flat, c);
                state_idx += 1;
                sig.push('S');
            }
            b"CHDR" => {
                sig.push('C');
                let which = u32_le(leaf(&r), 0);
                let letter = if which == LIST_ENTER { 'E' } else if which == LIST_EXIT { 'X' } else { '?' };
                if !seen_lists.insert(letter) {
                    c.repeated_list += 1;
                }
                cur.push(letter);
            }
            b"CEXE" => sig.push('c'),
            b"SWIT" => {
                flush_state(&mut cur, &mut seen_lists, state_idx, &flat, c);
                sig.push('W');
                let words = leaf(&r).len() / 4;
                if let Some(inf) = kids.iter().map(|&i| rows[i]).find(|r| &r.tag == b"INFO") {
                    let sc = u32_le(leaf(&inf), 4) as usize;
                    if sc == words { c.info_switch_eq_swit += 1 } else { c.info_switch_ne_swit += 1 }
                }
            }
            _ => sig.push('?'),
        }
    }
    flush_state(&mut cur, &mut seen_lists, state_idx, &flat, c);

    if !is_canonical(&sig) && c.noncanonical.len() < 10 {
        c.noncanonical.push(sig);
    }
}

/// Record the just-finished state's shape and correlate it with its parsed list lengths.
fn flush_state(
    cur: &mut Vec<char>,
    seen: &mut BTreeSet<char>,
    state_idx: usize,
    flat: &[&mercs2_formats::orchestrator::StateDef],
    c: &mut Census,
) {
    // Called once per state — even a state with NO command leaves, so `cur`/`seen` may be empty.
    if state_idx < 1 {
        cur.clear();
        seen.clear();
        return;
    }
    c.states_total += 1;
    let shape: String = cur.iter().collect();
    if shape.is_empty() {
        c.states_no_leaves += 1;
    }
    *c.state_shapes.entry(if shape.is_empty() { "(none)".into() } else { shape.clone() }).or_insert(0) += 1;
    if let Some(st) = flat.get(state_idx - 1) {
        let has_enter = shape.contains('E');
        let has_exit = shape.contains('X');
        if has_enter && st.enter.is_empty() {
            c.enter_leaf_but_empty += 1;
        }
        if has_exit && st.exit.is_empty() {
            c.exit_leaf_but_empty += 1;
        }
        if shape == "EX" {
            c.ex_states += 1;
            if st.exit.is_empty() { c.ex_empty_exit += 1; }
        } else if shape == "E" {
            c.e_states += 1;
            if st.enter.is_empty() { c.e_empty_enter += 1; }
        }
    }
    cur.clear();
    seen.clear();
}

/// I (N (S (C c)*)* )* W
fn is_canonical(sig: &str) -> bool {
    let b = sig.as_bytes();
    let mut i = 0;
    if b.first() != Some(&b'I') {
        return false;
    }
    i += 1;
    while i < b.len() && b[i] == b'N' {
        i += 1;
        while i < b.len() && b[i] == b'S' {
            i += 1;
            while i + 1 < b.len() && b[i] == b'C' && b[i + 1] == b'c' {
                i += 2;
            }
        }
    }
    i == b.len().saturating_sub(1) && b.get(i) == Some(&b'W')
}
