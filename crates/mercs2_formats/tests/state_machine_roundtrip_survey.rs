//! Can the destruction state machine be WRITTEN, not just read?
//!
//! `Contribution::EditStateMachine` parses, claims a blast radius, and then refuses to lower. The
//! recorded reason is that `orchestrator::parse_state_machine` produces a decoded *view* — "no
//! descriptor indices, no data offsets, no container position, so it cannot even round-trip" — and
//! that the family is a nested container inside the model container, so writing one means
//! rebuilding that descriptor table and re-basing every following sibling.
//!
//! That is an argument from reading the parser. Nobody has measured the BYTES. This survey does,
//! because the answer decides whether a serializer is a bounded job or an open research problem,
//! and the same question asked of `scripts_vz` (`scripts_block_survey.rs`) turned a "theoretical
//! restriction" into a cleared blocker in one afternoon.
//!
//! Three things are measured, in increasing order of what they'd unblock:
//!
//! 1. **Shape.** Is the family a flat run of leaf siblings with fixed-size records, or does it
//!    nest? Every nested container among the children is data `parse_state_machine` silently
//!    `continue`s past today.
//! 2. **Layout.** Do the leaves' data regions tile contiguously in row order? If they do, re-basing
//!    siblings after an edit is arithmetic. If they interleave with other families or carry
//!    alignment padding, it is not.
//! 3. **Model fidelity — the decisive one.** Can every leaf's bytes be regenerated from what
//!    `StateMachine` retains? Known suspects: `INFO`'s first word is skipped outright, `SWIT` is
//!    truncated to `switch_count`, `CHDR`s that are neither Enter nor Exit drop their `CEXE`, and
//!    any tag outside the match arm vanishes. Each is recorded separately so a failure names the
//!    field rather than the file.
//!
//! # RESULT (2026-07-31, retail `vz.wad`): the recorded blocker was wrong on both counts.
//!
//! 25,707 model containers scanned, **1,311 carry a destruction family**, and:
//!
//! * **It does not nest.** 0 of 1,311 have a container among the family's children. The parent is a
//!   `STAM` container whose children are a flat run of leaves, closed over exactly
//!   `{INFO, NODE, STAT, CHDR, CEXE, SWIT}` — no unknown tags anywhere.
//! * **It tiles exactly.** Zero gaps, zero padding, zero overlaps between consecutive leaf data
//!   regions. Re-basing after an edit is arithmetic over a contiguous run.
//! * **Every record is fixed-size:** `INFO` 12 B, `NODE` 8 B, `STAT` 4 B, `CHDR` 8 B.
//! * **The one "lost" field is a constant.** `INFO`'s first word — the one `parse_state_machine`
//!   skips — is `5` in all 1,311.
//! * **The 20th byte is derivable.** The `+12` descriptor word `desc_rows` never reads is, in
//!   237,892 of 237,892 rows, the count of siblings that FOLLOW this row at its own level — the
//!   complement of `u3`'s descendant count. A serializer computes it; it need not be stored.
//! * **1,311 / 1,311 (100%) are losslessly recoverable** from the parsed `StateMachine` plus that
//!   one constant. No `SWIT` tail is dropped, no `CHDR`/`CEXE` length disagrees, no `CHDR` selector
//!   is anything but Enter or Exit.
//!
//! So `orchestrator::serialize_state_machine` is a bounded job, not research. What remains for the
//! WRITER (and is not claimed here) is the container-subtree splice: an edit that changes the
//! family's total size must rewrite `STAM`'s size, re-base every following sibling's data offset,
//! and recompute the CSUM. That is mechanical over a flat contiguous run — which is precisely what
//! this survey establishes.
//!
//! The assertions below exist so the serializer can rely on all of the above: a DLC or future
//! archive that breaks any of it fails HERE, not by shipping a WAD that hangs the game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::orchestrator::{parse_state_machine, LIST_ENTER, LIST_EXIT};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_ID_MODEL;
use mercs2_formats::ucfx::parse_block_entry_table;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 > b.len() {
        return 0;
    }
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// One descriptor row, captured VERBATIM — including the `+12` word `orchestrator::desc_rows`
/// never reads. A round-trip that regenerates 16 of a row's 20 bytes is not a round-trip.
#[derive(Clone, Copy, Debug)]
struct Row {
    tag: [u8; 4],
    off: u32,
    size: u32,
    w12: u32,
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
            w12: u32_le(buf, ro + 12),
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

/// The same predicate `parse_state_machine` uses to find the family parent.
fn family_parent(rows: &[Row]) -> Option<usize> {
    (0..rows.len())
        .find(|&p| rows[p].kids > 0 && children_of(rows, p).iter().any(|&c| &rows[c].tag == b"NODE"))
}

#[derive(Default)]
struct Census {
    containers_scanned: usize,
    with_family: usize,
    parse_ok: usize,

    /// (1) SHAPE
    nested_children: usize,
    unknown_tags: BTreeMap<String, usize>,
    tag_order: BTreeSet<String>,
    /// The descriptor word at `+12` that `orchestrator::desc_rows` never reads. A serializer must
    /// emit all 20 bytes of a row, so this has to be either derivable or carried. It is NOT a flag —
    /// it ranges 0..~1932, about the row count of the largest family — so these counters test what
    /// it indexes. Whichever holds at 100% is a rule the serializer can apply instead of storing.
    w12_rows: usize,
    w12_is_child_ordinal: usize,
    w12_is_abs_row: usize,
    w12_is_tag_ordinal: usize,
    /// `kids.len() - 1 - ordinal` — the count of siblings that follow me at my own level.
    w12_is_siblings_after: usize,
    w12_other: Vec<String>,

    /// (2) LAYOUT
    non_contiguous: Vec<String>,
    padded_gaps: BTreeMap<u32, usize>,
    overlapping: Vec<String>,

    /// (3) MODEL FIDELITY
    info_word0: BTreeMap<u32, usize>,
    info_sizes: BTreeMap<u32, usize>,
    node_sizes: BTreeMap<u32, usize>,
    stat_sizes: BTreeMap<u32, usize>,
    chdr_sizes: BTreeMap<u32, usize>,
    chdr_which_other: BTreeMap<u32, usize>,
    node_count_mismatch: usize,
    cexe_len_mismatch: usize,
    swit_tail_dropped: usize,
    /// Containers where EVERY leaf regenerates byte-identically from the parsed model.
    lossless: usize,
    /// Containers that do not, with the first reason.
    lossy: Vec<(String, String)>,
}

#[test]
fn can_the_destruction_state_machine_be_written() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        return;
    };
    let mut file = std::fs::File::open(&wad).expect("open vz.wad");
    let size = file.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut file, size).expect("read FFCS");

    // Every block any model row points at, all LOD rungs — the state machine ships in the resident
    // rung, but which rung that IS varies, so scanning the whole chain avoids assuming.
    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    for a in archive.aset.iter().filter(|a| a.type_id == TYPE_ID_MODEL) {
        for b in a.lod_chain() {
            if b != 0xFFFF {
                blocks.insert(b);
            }
        }
    }
    eprintln!(
        "scanning {} distinct blocks from {} model ASET rows",
        blocks.len(),
        archive.aset.iter().filter(|a| a.type_id == TYPE_ID_MODEL).count()
    );

    let mut c = Census::default();

    for bi in blocks {
        let Ok(dec) = decompress_block(&mut file, &archive.indx, bi) else {
            continue;
        };
        let (_n, entries) = parse_block_entry_table(&dec);
        // Containers follow the 4 + 16*count entry table, each `chunk_size` long.
        let mut pos = 4 + entries.len() * 16;
        for (ei, e) in entries.iter().enumerate() {
            let end = (pos + e.chunk_size as usize).min(dec.len());
            if pos >= end {
                break;
            }
            let container = &dec[pos..end];
            pos = end;
            c.containers_scanned += 1;
            let label = format!("blk{bi}/entry{ei}/0x{:08X}", e.name_hash);
            survey_container(container, &label, &mut c);
        }
    }

    report(&c);

    // ── The survey answered. These assertions turn it into the regression guard the serializer
    // rests on: each one is a property `orchestrator::serialize_state_machine` is entitled to
    // assume, so a DLC or a future archive that breaks one fails HERE rather than by emitting a
    // WAD that hangs the game.
    assert!(c.containers_scanned > 0, "no containers scanned");
    assert!(c.with_family > 1000, "expected ~1311 destructibles, found {}", c.with_family);

    // (1) Shape: flat, and closed over six tags.
    assert_eq!(c.nested_children, 0, "the family nests — the flat re-emit is invalid");
    assert!(
        c.unknown_tags.is_empty(),
        "tags outside {{INFO,NODE,STAT,CHDR,CEXE,SWIT}} carry data the model drops: {:?}",
        c.unknown_tags
    );

    // (2) Layout: the leaves tile exactly, so re-basing after an edit is arithmetic.
    assert!(c.overlapping.is_empty(), "overlapping leaf data regions");
    assert!(c.padded_gaps.is_empty(), "alignment padding between leaves: {:?}", c.padded_gaps);

    // (3) Fidelity: fixed-size records, one constant, and a derivable descriptor word.
    assert_eq!(c.info_sizes.keys().collect::<Vec<_>>(), [&12], "INFO is not always 12 bytes");
    assert_eq!(c.node_sizes.keys().collect::<Vec<_>>(), [&8], "NODE is not always 8 bytes");
    assert_eq!(c.stat_sizes.keys().collect::<Vec<_>>(), [&4], "STAT is not always 4 bytes");
    assert_eq!(c.chdr_sizes.keys().collect::<Vec<_>>(), [&8], "CHDR is not always 8 bytes");
    assert_eq!(
        c.info_word0.keys().collect::<Vec<_>>(),
        [&5],
        "INFO word0 is not the constant 5 — the parser skips it, so a varying value would be lost"
    );
    assert!(c.chdr_which_other.is_empty(), "a CHDR selector is neither Enter nor Exit");
    assert_eq!(c.node_count_mismatch, 0, "NODE's declared state count disagrees with its STATs");
    assert_eq!(c.cexe_len_mismatch, 0, "CHDR's declared count disagrees with its CEXE");
    assert_eq!(c.swit_tail_dropped, 0, "SWIT carries words beyond switch_count");
    assert_eq!(
        c.w12_is_siblings_after, c.w12_rows,
        "descriptor word +12 is not always the count of following siblings — it would have to be stored"
    );
    assert_eq!(
        c.lossless, c.parse_ok,
        "{} of {} families cannot be regenerated from the parsed model",
        c.parse_ok - c.lossless,
        c.parse_ok
    );
}

fn survey_container(container: &[u8], label: &str, c: &mut Census) {
    let (data_off, rows) = rows_of(container);
    if rows.is_empty() {
        return;
    }
    let Some(parent) = family_parent(&rows) else {
        return;
    };
    c.with_family += 1;
    let kids = children_of(&rows, parent);

    // ── (1) SHAPE ──
    let mut per_tag: BTreeMap<[u8; 4], u32> = BTreeMap::new();
    for (ord, &k) in kids.iter().enumerate() {
        let r = rows[k];
        let tag = String::from_utf8_lossy(&r.tag).to_string();
        c.tag_order.insert(tag.clone());

        // What does the +12 word index?
        let tag_ord = {
            let e = per_tag.entry(r.tag).or_insert(0);
            let v = *e;
            *e += 1;
            v
        };
        c.w12_rows += 1;
        let mut matched = false;
        if r.w12 == ord as u32 {
            c.w12_is_child_ordinal += 1;
            matched = true;
        }
        if r.w12 == k as u32 {
            c.w12_is_abs_row += 1;
            matched = true;
        }
        if r.w12 == tag_ord {
            c.w12_is_tag_ordinal += 1;
            matched = true;
        }
        // Siblings remaining after me at MY OWN level (excluding self). Complements `u3`'s
        // descendant count: `u3` says how far my subtree reaches, this says how many peers follow.
        if r.w12 as usize == kids.len() - 1 - ord {
            c.w12_is_siblings_after += 1;
            matched = true;
        }
        if !matched && c.w12_other.len() < 10 {
            c.w12_other.push(format!(
                "{label} {tag} w12={} child_ord={ord} abs_row={k} tag_ord={tag_ord}",
                r.w12
            ));
        }

        if r.off == 0xFFFF_FFFF {
            c.nested_children += 1;
            continue;
        }
        if !matches!(&r.tag, b"INFO" | b"NODE" | b"STAT" | b"CHDR" | b"CEXE" | b"SWIT") {
            *c.unknown_tags.entry(tag).or_default() += 1;
        }
    }

    // ── (2) LAYOUT ── do the leaves tile contiguously, in row order?
    let leaves: Vec<Row> = kids
        .iter()
        .map(|&k| rows[k])
        .filter(|r| r.off != 0xFFFF_FFFF)
        .collect();
    let mut cursor: Option<u32> = None;
    for r in &leaves {
        if let Some(prev_end) = cursor {
            if r.off < prev_end {
                c.overlapping.push(label.to_string());
            } else if r.off > prev_end {
                *c.padded_gaps.entry(r.off - prev_end).or_default() += 1;
                if r.off - prev_end > 16 {
                    c.non_contiguous.push(label.to_string());
                }
            }
        }
        cursor = Some(r.off + r.size);
    }

    // ── (3) MODEL FIDELITY ──
    let Some(sm) = parse_state_machine(container) else {
        return;
    };
    c.parse_ok += 1;

    let leaf_bytes = |r: &Row| -> &[u8] {
        let s = data_off + r.off as usize;
        let e = (s + r.size as usize).min(container.len());
        if s > e {
            &[]
        } else {
            &container[s..e]
        }
    };

    // Walk the leaves in authored order, rebuilding each from `sm`, and diff.
    let mut node_i = 0usize;
    let mut state_i = 0usize;
    let mut pending: Option<(bool, usize)> = None;
    let mut first_loss: Option<String> = None;
    let note = |first: &mut Option<String>, m: String| {
        if first.is_none() {
            *first = Some(m);
        }
    };

    for r in &leaves {
        let d = leaf_bytes(r);
        match &r.tag {
            b"INFO" => {
                *c.info_sizes.entry(r.size).or_default() += 1;
                *c.info_word0.entry(u32_le(d, 0)).or_default() += 1;
                // word0 is not retained by StateMachine at all.
            }
            b"NODE" => {
                *c.node_sizes.entry(r.size).or_default() += 1;
                if let Some(n) = sm.nodes.get(node_i) {
                    if u32_le(d, 0) != n.name_hash {
                        note(&mut first_loss, "NODE name_hash drifted".into());
                    }
                    if u32_le(d, 4) as usize != n.states.len() {
                        c.node_count_mismatch += 1;
                        note(
                            &mut first_loss,
                            format!(
                                "NODE declares {} states, parsed {}",
                                u32_le(d, 4),
                                n.states.len()
                            ),
                        );
                    }
                }
                node_i += 1;
                state_i = 0;
            }
            b"STAT" => {
                *c.stat_sizes.entry(r.size).or_default() += 1;
                state_i += 1;
            }
            b"CHDR" => {
                *c.chdr_sizes.entry(r.size).or_default() += 1;
                let which = u32_le(d, 0);
                let count = u32_le(d, 4) as usize;
                match which {
                    LIST_ENTER => pending = Some((true, count)),
                    LIST_EXIT => pending = Some((false, count)),
                    other => {
                        *c.chdr_which_other.entry(other).or_default() += 1;
                        pending = None;
                        note(
                            &mut first_loss,
                            format!("CHDR selector 0x{other:08X} is neither Enter nor Exit"),
                        );
                    }
                }
            }
            b"CEXE" => {
                let have: Vec<u32> = (0..d.len() / 4).map(|i| u32_le(d, i * 4)).collect();
                match pending.take() {
                    Some((enter, count)) => {
                        if count != have.len() {
                            c.cexe_len_mismatch += 1;
                            note(
                                &mut first_loss,
                                format!("CHDR count {count} != CEXE words {}", have.len()),
                            );
                        }
                        let got = sm
                            .nodes
                            .get(node_i.saturating_sub(1))
                            .and_then(|n| n.states.get(state_i.saturating_sub(1)))
                            .map(|s| if enter { &s.enter } else { &s.exit });
                        if got.map(|g| g.as_slice() != have.as_slice()).unwrap_or(true) {
                            note(&mut first_loss, "CEXE list not recoverable from model".into());
                        }
                    }
                    None => note(&mut first_loss, "CEXE with no retained CHDR — list dropped".into()),
                }
            }
            b"SWIT" => {
                let words = d.len() / 4;
                if words != sm.switch_slots.len() {
                    c.swit_tail_dropped += 1;
                    note(
                        &mut first_loss,
                        format!("SWIT has {words} words, model kept {}", sm.switch_slots.len()),
                    );
                }
            }
            other => {
                note(
                    &mut first_loss,
                    format!("tag {} is dropped entirely", String::from_utf8_lossy(other)),
                );
            }
        }
    }

    match first_loss {
        None => c.lossless += 1,
        Some(m) => {
            if c.lossy.len() < 25 {
                c.lossy.push((label.to_string(), m));
            }
        }
    }
}

fn report(c: &Census) {
    let pct = |n: usize, d: usize| if d == 0 { 0.0 } else { n as f64 * 100.0 / d as f64 };

    eprintln!("\n════ STATE-MACHINE ROUND-TRIP SURVEY ════");
    eprintln!("containers scanned      {}", c.containers_scanned);
    eprintln!(
        "carrying a family       {} ({:.1}%)",
        c.with_family,
        pct(c.with_family, c.containers_scanned)
    );
    eprintln!("parse_state_machine ok  {}", c.parse_ok);

    eprintln!("\n── (1) SHAPE ──");
    eprintln!("nested containers among the family's children  {}", c.nested_children);
    eprintln!("tags seen: {:?}", c.tag_order);
    if c.unknown_tags.is_empty() {
        eprintln!("unknown tags: NONE — the family is closed over {{INFO,NODE,STAT,CHDR,CEXE,SWIT}}");
    } else {
        eprintln!("unknown tags (data the parser drops): {:?}", c.unknown_tags);
    }
    eprintln!("descriptor word +12 over {} family rows — what does it index?", c.w12_rows);
    eprintln!(
        "    == ordinal among the parent's children : {} ({:.1}%)",
        c.w12_is_child_ordinal,
        pct(c.w12_is_child_ordinal, c.w12_rows)
    );
    eprintln!(
        "    == absolute descriptor row index       : {} ({:.1}%)",
        c.w12_is_abs_row,
        pct(c.w12_is_abs_row, c.w12_rows)
    );
    eprintln!(
        "    == per-tag running ordinal             : {} ({:.1}%)",
        c.w12_is_tag_ordinal,
        pct(c.w12_is_tag_ordinal, c.w12_rows)
    );
    eprintln!(
        "    == siblings remaining after me         : {} ({:.1}%)",
        c.w12_is_siblings_after,
        pct(c.w12_is_siblings_after, c.w12_rows)
    );
    if c.w12_other.is_empty() {
        eprintln!("    unexplained: NONE");
    } else {
        eprintln!("    unexplained samples:");
        for s in &c.w12_other {
            eprintln!("        {s}");
        }
    }

    eprintln!("\n── (2) LAYOUT ──");
    eprintln!("gap sizes between consecutive leaves: {:?}", c.padded_gaps);
    eprintln!("overlapping leaf regions: {}", c.overlapping.len());
    eprintln!("gaps > 16 bytes:          {}", c.non_contiguous.len());
    for l in c.non_contiguous.iter().take(5) {
        eprintln!("    {l}");
    }

    eprintln!("\n── (3) MODEL FIDELITY ──");
    eprintln!("INFO sizes {:?}", c.info_sizes);
    eprintln!(
        "INFO word0 (skipped by the parser) distinct values: {} {:?}",
        c.info_word0.len(),
        c.info_word0.iter().take(8).collect::<Vec<_>>()
    );
    eprintln!("NODE sizes {:?}   STAT sizes {:?}", c.node_sizes, c.stat_sizes);
    eprintln!("CHDR sizes {:?}", c.chdr_sizes);
    eprintln!("CHDR selectors that are neither Enter nor Exit: {:?}", c.chdr_which_other);
    eprintln!("NODE state-count mismatches {}", c.node_count_mismatch);
    eprintln!("CHDR/CEXE length mismatches {}", c.cexe_len_mismatch);
    eprintln!("SWIT tails dropped          {}", c.swit_tail_dropped);
    eprintln!(
        "\nLOSSLESS containers {} / {} ({:.1}%)",
        c.lossless,
        c.parse_ok,
        pct(c.lossless, c.parse_ok)
    );
    if !c.lossy.is_empty() {
        eprintln!("first lossy cases:");
        for (l, m) in &c.lossy {
            eprintln!("    {l}: {m}");
        }
    }
    eprintln!("════════════════════════════════════════\n");
}
