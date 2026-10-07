//! Every `animation` container in retail `vz.wad` rebuilds byte-identically from its own chunks.
//!
//! This is the proof `anim_container` rests on. It walks every block an ASET type-16 row names,
//! parses each `animation` container with the strict reader, writes it back with the writer, and
//! requires the bytes to match — for both kinds (Havok clips with and without `evnt`, and the
//! `MANM` keyframe animations). It also decodes every `evnt` chunk and re-encodes it, and checks the
//! clip / `trnm` / `evnt` pairing rules the Quartermaster's lint enforces, so a rule that retail
//! itself breaks cannot be shipped as an error.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml`, and fails if it is absent. The hermetic suite never builds it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mercs2_formats::anim_container::{
    build_container, build_evnt, classify, clip_pairing_problems, parse_container, parse_evnt,
    AnimContainerKind, CLIP_INFO, HAVOK_PACKFILE_MAGIC,
};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION};
use mercs2_formats::ucfx::parse_block_entry_table;

#[test]
fn every_retail_animation_container_rebuilds_byte_identically() {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut file = std::fs::File::open(&wad).expect("open vz.wad");
    let size = file.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut file, size).expect("read FFCS");

    // Every block an animation row names, and the shape of those rows.
    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    let mut anim_hashes: BTreeSet<u32> = BTreeSet::new();
    for a in archive.aset.iter().filter(|a| a.type_id == TYPE_ID_ANIMATION) {
        anim_hashes.insert(a.asset_hash);
        assert_eq!(a.secondary_ref, 0xFFFF_FFFF, "animation row 0x{:08X} names a LOD rung", a.asset_hash);
        assert_eq!(a.packed_block_ref & 0xFFFF, 0xFFFF, "animation row 0x{:08X} names a sub-rung", a.asset_hash);
        for b in a.lod_chain() {
            if b != 0xFFFF {
                blocks.insert(b);
            }
        }
    }

    let (mut clips_plain, mut clips_evnt, mut keyframe) = (0usize, 0usize, 0usize);
    let mut events_total = 0usize;
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for bi in blocks {
        let dec = decompress_block(&mut file, &archive.indx, bi).expect("decompress");
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            let container = &dec[pos..end];
            pos = end;
            if e.type_hash != TYPE_HASH_ANIMATION {
                continue;
            }
            let label = format!("block {bi} 0x{:08X}", e.name_hash);
            let chunks = match parse_container(container) {
                Ok(c) => c,
                Err(m) => {
                    failures.push(format!("{label}: parse: {m}"));
                    continue;
                }
            };
            if build_container(&chunks) != container {
                failures.push(format!("{label}: rebuild differs"));
                continue;
            }
            match classify(&chunks) {
                Ok(AnimContainerKind::Keyframe) => keyframe += 1,
                Ok(AnimContainerKind::HavokClip { has_events }) => {
                    if has_events {
                        clips_evnt += 1;
                    } else {
                        clips_plain += 1;
                    }
                    if chunks[0].body != CLIP_INFO {
                        failures.push(format!("{label}: info is {:02X?}", chunks[0].body));
                    }
                    if !chunks[1].body.starts_with(&HAVOK_PACKFILE_MAGIC) {
                        failures.push(format!("{label}: data is not a Havok packfile"));
                    }
                    let evnt = chunks.get(3).map(|c| c.body.as_slice());
                    if let Some(body) = evnt {
                        match parse_evnt(body) {
                            Ok(ev) => {
                                events_total += ev.len();
                                for x in &ev {
                                    *categories.entry(x.category.clone()).or_default() += 1;
                                }
                                if build_evnt(&ev).as_deref() != Ok(body) {
                                    failures.push(format!("{label}: evnt re-encode differs"));
                                }
                            }
                            Err(m) => failures.push(format!("{label}: evnt: {m}")),
                        }
                    }
                    for p in clip_pairing_problems(&chunks[1].body, &chunks[2].body, evnt) {
                        failures.push(format!("{label}: pairing: {p}"));
                    }
                }
                Err(m) => failures.push(format!("{label}: {m}")),
            }
        }
    }

    eprintln!(
        "animation containers: {} Havok clips ({clips_plain} without evnt, {clips_evnt} with), \
         {keyframe} MANM keyframe; {events_total} events; categories {categories:?}; \
         {} animation asset hashes",
        clips_plain + clips_evnt,
        anim_hashes.len()
    );
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
    // The census this module's docs quote. A different archive changes these; say so loudly.
    assert_eq!(clips_plain, 1969);
    assert_eq!(clips_evnt, 2263);
    assert_eq!(keyframe, 29);
}
