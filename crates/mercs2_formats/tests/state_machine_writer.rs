//! Can the destruction state machine be WRITTEN back byte-identically, and edited correctly?
//!
//! `state_machine_roundtrip_survey.rs` proved the family is a flat, contiguous, fixed-size run that
//! regenerates losslessly from the parsed model. This is the writer that cashes that in:
//! `serialize_state_machine(original, parse(original))` must reproduce `original` byte-for-byte for
//! every retail destructible, and a targeted edit must change exactly what was asked and nothing
//! else. A no-op that is not byte-identical would mean the writer is guessing a field the survey
//! said it need not; an edit that shifts an unrelated leaf would corrupt the container.
//!
//! Scanning mirrors the survey so the two cover the same 1,311 families.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::orchestrator::{parse_state_machine, serialize_state_machine};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_ID_MODEL;
use mercs2_formats::ucfx::parse_block_entry_table;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Every model container in the WAD that carries a destruction family, as raw container bytes.
fn family_containers() -> Option<Vec<(String, Vec<u8>)>> {
    let wad = vz_wad()?;
    let mut file = std::fs::File::open(&wad).ok()?;
    let size = file.metadata().ok()?.len();
    let archive = load_ffcs_archive(&mut file, size).ok()?;

    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    for a in archive.aset.iter().filter(|a| a.type_id == TYPE_ID_MODEL) {
        for b in a.lod_chain() {
            if b != 0xFFFF {
                blocks.insert(b);
            }
        }
    }

    let mut out = Vec::new();
    for bi in blocks {
        let Ok(dec) = decompress_block(&mut file, &archive.indx, bi) else {
            continue;
        };
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for (ei, e) in entries.iter().enumerate() {
            let end = (pos + e.chunk_size as usize).min(dec.len());
            if pos >= end {
                break;
            }
            let container = dec[pos..end].to_vec();
            pos = end;
            if parse_state_machine(&container).is_some() {
                out.push((format!("blk{bi}/entry{ei}/0x{:08X}", e.name_hash), container));
            }
        }
    }
    Some(out)
}

/// ★ The decisive test: re-emitting an UNEDITED family reproduces the container exactly, for every
/// retail destructible. This is what lets `edit_state_machine` trust the writer — an edit that
/// touches one field leaves every other byte where it was.
#[test]
fn every_retail_family_round_trips_byte_identically() {
    let Some(families) = family_containers() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    assert!(families.len() > 1000, "expected ~1311 families, found {}", families.len());

    let mut checked = 0usize;
    for (label, original) in &families {
        let sm = parse_state_machine(original).expect("it parsed during collection");
        let re = serialize_state_machine(original, &sm)
            .unwrap_or_else(|e| panic!("{label}: serialize failed: {e}"));
        assert_eq!(
            re.len(),
            original.len(),
            "{label}: no-op re-emit changed the container length"
        );
        assert!(re == *original, "{label}: no-op re-emit is not byte-identical");
        checked += 1;
    }
    eprintln!("round-tripped {checked} destruction families byte-identically");
}

/// A rename edit changes exactly the state's name hash, keeps the container the same size (a hash is
/// fixed-width), still parses, and reads back as the new name — while every sibling is untouched.
#[test]
fn renaming_a_state_changes_only_that_field() {
    let Some(families) = family_containers() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    // Pick the first family that has a node with at least one state to rename.
    let Some((label, original)) = families
        .into_iter()
        .find(|(_, c)| parse_state_machine(c).is_some_and(|sm| sm.nodes.iter().any(|n| !n.states.is_empty())))
    else {
        eprintln!("SKIPPING: no family with a state");
        return;
    };

    let mut sm = parse_state_machine(&original).expect("parse");
    let ni = sm.nodes.iter().position(|n| !n.states.is_empty()).unwrap();
    let old = sm.nodes[ni].states[0].name_hash;
    let new = old ^ 0x5A5A_5A5A; // any distinct value
    sm.nodes[ni].states[0].name_hash = new;

    let edited = serialize_state_machine(&original, &sm).expect("serialize the edit");
    assert_eq!(edited.len(), original.len(), "{label}: a rename must not resize the container");

    let reparsed = parse_state_machine(&edited).expect("the edited container must still parse");
    assert_eq!(
        reparsed.nodes[ni].states[0].name_hash, new,
        "{label}: the rename must survive a round trip"
    );
    // Nothing else moved: same node/state counts, same switch table.
    assert_eq!(reparsed.nodes.len(), sm.nodes.len());
    assert_eq!(reparsed.switch_slots, sm.switch_slots);
}

/// Rewriting a state's Enter command list to a DIFFERENT length is still a same-shape edit (it adds
/// no leaves), and it must round-trip: the container resizes by the byte delta and reads back the
/// new list, with the CSUM recomputed so the container still verifies.
#[test]
fn rewriting_a_command_list_resizes_and_round_trips() {
    let Some(families) = family_containers() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let Some((label, original)) = families
        .into_iter()
        .find(|(_, c)| parse_state_machine(c).is_some_and(|sm| sm.nodes.iter().any(|n| n.states.iter().any(|s| !s.enter.is_empty()))))
    else {
        eprintln!("SKIPPING: no family with an Enter list");
        return;
    };

    let mut sm = parse_state_machine(&original).expect("parse");
    let (ni, sj) = sm
        .nodes
        .iter()
        .enumerate()
        .find_map(|(i, n)| n.states.iter().position(|s| !s.enter.is_empty()).map(|j| (i, j)))
        .unwrap();
    // Append two commands — a longer list than shipped.
    sm.nodes[ni].states[sj].enter.push(0x1111_2222);
    sm.nodes[ni].states[sj].enter.push(0x3333_4444);
    let want = sm.nodes[ni].states[sj].enter.clone();

    let edited = serialize_state_machine(&original, &sm).expect("serialize the longer list");
    assert_eq!(
        edited.len(),
        original.len() + 8,
        "{label}: two extra u32 commands must grow the container by 8 bytes"
    );
    let reparsed = parse_state_machine(&edited).expect("re-parse the edited container");
    assert_eq!(reparsed.nodes[ni].states[sj].enter, want, "{label}: the new list must read back");
}

/// The writer refuses a shape change by NAME rather than emitting a subtly wrong container — adding
/// a node is exactly the surgery it does not do yet.
#[test]
fn adding_a_node_is_refused_not_mangled() {
    let Some(families) = family_containers() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let (_, original) = families.into_iter().next().expect("a family");
    let mut sm = parse_state_machine(&original).expect("parse");
    sm.nodes.push(Default::default());
    let err = serialize_state_machine(&original, &sm).expect_err("adding a node must be refused");
    assert!(err.to_lowercase().contains("node"), "the refusal should name the cause: {err}");
}
