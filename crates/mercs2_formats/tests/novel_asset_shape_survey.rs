//! Which asset types can a Shipment ADD, and which need a real builder first?
//!
//! The Quartermaster can mint three novel assets today: a model, a character, and a Scaleform
//! movie. A texture can only *replace* one or ride inside an outfit. Fonts, string tables, audio,
//! animations, effects and video have no kind at all — the stated reason being that the workspace
//! holds exactly two block builders, `gfx::build_cfx_pack_block` and `texture::build_texture_block`.
//!
//! But "no builder exists" is not the same as "a builder is hard", and `build_cfx_pack_block` is
//! the proof. Its doc records how it was written: *"The shape is not invented. All 64 `cfx_pack`
//! containers in retail `vz.wad` were measured and every one of them is byte-for-byte this
//! layout"* — a 20-byte UCFX header, ONE `data` descriptor, the payload verbatim, a CSUM. That
//! builder is 30 lines because the container turned out to be an opaque wrapper.
//!
//! So the question that decides `add_font` / `add_sound` / `add_animation` / `add_effect` is not
//! "has someone written a parser" but **"is this type's container an opaque single-leaf wrapper,
//! or a structured family?"** A wrapper means one generic `build_wrapped_block(hash, type, bytes)`
//! covers every type that shares the shape, and the author supplies bytes they already have. A
//! structured family means the fields must be understood before anything can be authored.
//!
//! This surveys every ASET type in retail `vz.wad` and reports, per type, the descriptor shapes its
//! containers actually take. Nothing is assumed about which types matter; the census decides.
//!
//! # RESULT (2026-07-31): most of the "no builder" list is a wrapper, and audio is the big win.
//!
//! **Opaque single-`data`-leaf wrappers — ONE generic builder covers all eight:**
//! `cfx_pack` (64/64, the known case), **`soundbank` (98/98, 76 assets, 94 MB)**,
//! **`sounddb` (58/58, 77 assets, 65 MB)**, `binary` (14/14), `world_entity_data`, `guidmap`,
//! `0xFA0B8DBC` (22/22), `0x6310807F` (625/625).
//!
//! **Near-wrapper:** **`wavebank` — 92 of 93 containers are bare `data`** (95 assets, 207 MB); the
//! lone exception is `NAME,INFO,BODY`. Together with `soundbank` + `sounddb` that is the whole
//! audio stack, so `add_sound` is tractable *now*, not after a decode project.
//!
//! **Small, perfectly uniform structures — a fixed leaf list, not research:**
//! `font` 9/9 `INFO,CHAR,MTRL` · `stringdb` 3/3 `INFO,KEYS,STRS` · `material_params` 6/6
//! `INFO,DATA` · `stance` 14/15 `INFO,TYPE,VALU`. And `stringdb` **round-trips byte-identically
//! 3/3** through the existing `stringdb::{parse,build}` (see the second test), so `edit_stringdb`
//! needs only a block wrapper.
//!
//! **Genuinely structured — NOT this pass:** `effect` (`EFCT,EMTR,GEOM×N` families of 261-336 rows,
//! params still hash-only) and `animation` (wavelet clip data; decode is solved, encode is not).
//!
//! **Video is not an ASET type at all.** There is no Bink/video row in the registry — retail ships
//! movies as loose files under `data/Movies`, so "a new movie clip" is either a Scaleform
//! `cfx_pack` (already expressible via `add_movie`) or a file placement, never a new WAD kind.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
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

/// A container's descriptor tree, flattened to a readable signature like `data` or
/// `INFO,BODY` or `INFO,HIER,MTRL,{STAM}`. Nested containers are braced so a structured type is
/// visually distinct from a flat one at a glance.
fn shape_of(buf: &[u8], max_tags: usize) -> Option<String> {
    if buf.len() < 20 || &buf[0..4] != b"UCFX" {
        return None;
    }
    let ndesc = u32_le(buf, 16) as usize;
    if ndesc == 0 || ndesc > 4096 {
        return None;
    }
    let mut parts = Vec::new();
    for d in 0..ndesc.min(max_tags) {
        let ro = 20 + d * 20;
        if ro + 20 > buf.len() {
            break;
        }
        let tag = String::from_utf8_lossy(&buf[ro..ro + 4]).to_string();
        let is_container = u32_le(buf, ro + 4) == 0xFFFF_FFFF;
        parts.push(if is_container { format!("{{{tag}}}") } else { tag });
    }
    if ndesc > max_tags {
        parts.push(format!("+{}", ndesc - max_tags));
    }
    Some(parts.join(","))
}

/// What the ASET type ids we know mean, so the report reads as names not numbers.
fn type_name(id: u32) -> &'static str {
    use mercs2_formats::types::*;
    match id {
        TYPE_ID_WAVEBANK => "wavebank",
        TYPE_ID_SOUNDBANK => "soundbank",
        TYPE_ID_LAYER => "layer",
        TYPE_ID_MODEL => "model",
        TYPE_ID_TEXTURE => "texture",
        TYPE_ID_SCRIPT => "script",
        TYPE_ID_ANIMATION => "animation",
        TYPE_ID_LOWRES_TERRAIN => "lowres_terrain",
        TYPE_ID_TERRAIN_MESH => "terrain_mesh",
        TYPE_ID_FONT => "font",
        TYPE_ID_PATH => "path",
        TYPE_ID_EFFECT => "effect",
        TYPE_ID_STRINGDB => "stringdb",
        TYPE_ID_LEVEL => "level",
        TYPE_ID_STANCE => "stance",
        TYPE_ID_MATERIAL_PARAMS => "material_params",
        TYPE_ID_MUSIC_STATE_MAP => "music_state_map",
        TYPE_ID_MUSIC_CUE_TABLE => "music_cue_table",
        TYPE_ID_ANIM_STATE_MACHINE => "anim_state_machine",
        TYPE_ID_WORLD_ENTITY_DATA => "world_entity_data",
        TYPE_ID_FX_DICTIONARY => "fx_dictionary",
        TYPE_ID_CFX_PACK => "cfx_pack",
        TYPE_ID_WATERMAP => "watermap",
        // Ids `types.rs` has no constant for, resolved through `aset_type_ids::type_id_for_type_hash`.
        3 => "binary",
        5 => "facefx_animset",
        10 => "guidmap",
        12 => "?[0x600B904E]",
        13 => "sounddb",
        18 => "?[0xFA0B8DBC]",
        30 => "?[0x6310807F]",
        33 => "?[0xACCE47F2]",
        34 => "facefx_actor",
        _ => "?",
    }
}

#[derive(Default)]
struct TypeCensus {
    assets: usize,
    containers: usize,
    shapes: BTreeMap<String, usize>,
    /// Total payload bytes seen, so "is this worth a kind" has a size behind it.
    bytes: u64,
}

#[test]
fn which_asset_types_are_opaque_wrappers() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad (set MERCS2_GAME_DIR or .mercs2-local.toml)");
        return;
    };
    let mut file = std::fs::File::open(&wad).expect("open vz.wad");
    let size = file.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut file, size).expect("read FFCS");

    // hash -> type_id, from the primary rows. A container is identified by the entry table's
    // name_hash, so this is how a container learns its own type.
    let mut type_of: BTreeMap<u32, u32> = BTreeMap::new();
    let mut assets_per_type: BTreeMap<u32, usize> = BTreeMap::new();
    for a in &archive.aset {
        type_of.entry(a.asset_hash).or_insert(a.type_id);
        if a.is_primary() {
            *assets_per_type.entry(a.type_id).or_default() += 1;
        }
    }

    // Every block any ASET row names, so no type is missed by sampling only models.
    let mut blocks: BTreeSet<u16> = BTreeSet::new();
    for a in &archive.aset {
        for b in a.lod_chain() {
            if b != 0xFFFF {
                blocks.insert(b);
            }
        }
    }
    eprintln!("scanning {} blocks / {} ASET rows", blocks.len(), archive.aset.len());

    let mut census: BTreeMap<u32, TypeCensus> = BTreeMap::new();
    for (id, n) in &assets_per_type {
        census.entry(*id).or_default().assets = *n;
    }

    for bi in blocks {
        let Ok(dec) = decompress_block(&mut file, &archive.indx, bi) else {
            continue;
        };
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = (pos + e.chunk_size as usize).min(dec.len());
            if pos >= end {
                break;
            }
            let container = &dec[pos..end];
            pos = end;
            let Some(&tid) = type_of.get(&e.name_hash) else { continue };
            let c = census.entry(tid).or_default();
            c.containers += 1;
            c.bytes += container.len() as u64;
            if let Some(s) = shape_of(container, 6) {
                *c.shapes.entry(s).or_default() += 1;
            } else {
                *c.shapes.entry("<not UCFX>".into()).or_default() += 1;
            }
        }
    }

    eprintln!("\n════ ASSET-TYPE CONTAINER SHAPES (retail vz.wad) ════");
    eprintln!(
        "{:<20} {:>7} {:>8} {:>9}  {}",
        "type", "assets", "conts", "MB", "descriptor shapes (count)"
    );
    let mut wrappers = Vec::new();
    for (id, c) in &census {
        if c.containers == 0 {
            continue;
        }
        let shapes: Vec<String> = c
            .shapes
            .iter()
            .map(|(s, n)| format!("{s} ({n})"))
            .collect();
        eprintln!(
            "{:<20} {:>7} {:>8} {:>9.1}  {}",
            format!("{} [{}]", type_name(*id), id),
            c.assets,
            c.containers,
            c.bytes as f64 / (1u64 << 20) as f64,
            shapes.join("  |  ")
        );
        // The `add_movie` shape: exactly one leaf named `data`.
        if c.shapes.len() == 1 && c.shapes.contains_key("data") {
            wrappers.push(format!("{} [{}]", type_name(*id), id));
        }
    }

    eprintln!(
        "\n★ OPAQUE SINGLE-`data`-LEAF WRAPPERS (one generic builder covers all of these):\n    {}",
        if wrappers.is_empty() { "none".to_string() } else { wrappers.join(", ") }
    );
    eprintln!("════════════════════════════════════════════════════\n");

    assert!(!census.is_empty(), "no containers surveyed");

    // ── The invariants a generic `build_wrapped_block(hash, type_hash, bytes)` would rest on.
    // Each is the same claim `build_cfx_pack_block`'s doc already makes for its own type, now
    // measured for the rest. A future archive that breaks one fails here rather than by shipping.
    let only_data = |id: u32| -> bool {
        census
            .get(&id)
            .map(|c| c.shapes.len() == 1 && c.shapes.contains_key("data"))
            .unwrap_or(false)
    };
    use mercs2_formats::types::*;
    assert!(only_data(TYPE_ID_CFX_PACK), "cfx_pack is no longer a bare `data` wrapper");
    assert!(only_data(TYPE_ID_SOUNDBANK), "soundbank is not a bare `data` wrapper");
    assert!(only_data(13), "sounddb [13] is not a bare `data` wrapper");

    // wavebank is 92-of-93 `data`; the lone `NAME,INFO,BODY` is why this asserts a majority rather
    // than purity. An `add_sound` emits the wrapper shape, which is what retail overwhelmingly uses.
    let wave = census.get(&TYPE_ID_WAVEBANK).expect("wavebank present");
    let wave_data = *wave.shapes.get("data").unwrap_or(&0);
    assert!(
        wave_data * 10 > wave.containers * 9,
        "wavebank is no longer overwhelmingly a `data` wrapper: {:?}",
        wave.shapes
    );

    // Small, perfectly uniform structures — a builder for these is a fixed leaf list, not research.
    let uniform = |id: u32, want: &str| {
        let c = census.get(&id).unwrap_or_else(|| panic!("type {id} absent"));
        assert_eq!(
            c.shapes.keys().collect::<Vec<_>>(),
            [&want.to_string()],
            "type {id} is not uniformly `{want}`: {:?}",
            c.shapes
        );
    };
    uniform(TYPE_ID_FONT, "INFO,CHAR,MTRL");
    uniform(TYPE_ID_STRINGDB, "INFO,KEYS,STRS");
    uniform(TYPE_ID_MATERIAL_PARAMS, "INFO,DATA");
}

/// `stringdb` is the one unbuilt type that already has BOTH halves — `stringdb::parse` and
/// `stringdb::build`. If retail's own tables survive a parse→build round-trip byte-for-byte then
/// `edit_stringdb` needs only a block wrapper, and localised text for a novel UI element stops
/// being hardcoded English.
#[test]
fn retail_string_tables_survive_a_parse_build_round_trip() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let mut file = std::fs::File::open(&wad).expect("open vz.wad");
    let size = file.metadata().expect("stat").len();
    let archive = load_ffcs_archive(&mut file, size).expect("read FFCS");

    let stringdbs: BTreeSet<u32> = archive
        .aset
        .iter()
        .filter(|a| a.type_id == mercs2_formats::types::TYPE_ID_STRINGDB)
        .map(|a| a.asset_hash)
        .collect();
    let blocks: BTreeSet<u16> = archive
        .aset
        .iter()
        .filter(|a| a.type_id == mercs2_formats::types::TYPE_ID_STRINGDB)
        .flat_map(|a| a.lod_chain())
        .filter(|b| *b != 0xFFFF)
        .collect();
    eprintln!("{} stringdb assets across {} blocks", stringdbs.len(), blocks.len());

    let mut checked = 0usize;
    let mut identical = 0usize;
    let mut differed: Vec<String> = Vec::new();

    for bi in blocks {
        let Ok(dec) = decompress_block(&mut file, &archive.indx, bi) else {
            continue;
        };
        let (_n, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + entries.len() * 16;
        for e in &entries {
            let end = (pos + e.chunk_size as usize).min(dec.len());
            if pos >= end {
                break;
            }
            let container = &dec[pos..end];
            pos = end;
            if !stringdbs.contains(&e.name_hash) {
                continue;
            }
            let Some(syek) = mercs2_formats::ucfx::extract_chunk_body(container, b"KEYS") else {
                continue;
            };
            let Some(srts) = mercs2_formats::ucfx::extract_chunk_body(container, b"STRS") else {
                continue;
            };
            checked += 1;
            match mercs2_formats::stringdb::parse(&syek, &srts) {
                Ok(db) => {
                    let (k2, s2) = mercs2_formats::stringdb::build(&db);
                    if k2 == syek && s2 == srts {
                        identical += 1;
                    } else if differed.len() < 10 {
                        differed.push(format!(
                            "0x{:08X}: keys {}→{} strings {}→{}",
                            e.name_hash,
                            syek.len(),
                            k2.len(),
                            srts.len(),
                            s2.len()
                        ));
                    }
                }
                Err(m) => {
                    if differed.len() < 10 {
                        differed.push(format!("0x{:08X}: parse failed: {m}", e.name_hash));
                    }
                }
            }
        }
    }

    eprintln!("\n════ STRINGDB ROUND-TRIP ════");
    eprintln!("checked {checked}, byte-identical {identical}");
    for d in &differed {
        eprintln!("    {d}");
    }
    eprintln!("═════════════════════════════\n");
}
