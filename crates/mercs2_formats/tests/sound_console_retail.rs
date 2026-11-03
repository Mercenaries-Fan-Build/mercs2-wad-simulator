//! The console bakes' sound tables, converted, against PC retail.
//!
//! Every `soundbank` and `sounddb` of the Xbox and PS3 bakes of `vz.wad`, converted by
//! `be_to_le::audio`, is compared with its PC twin byte for byte, container framing included, so a field
//! width the converter reads wrong shows up here rather than as a cue the game never finds.
//!
//! Every sounddb must be equal. A soundbank may differ only in two fields, each the bakes' own data
//! rather than a conversion: a single-track cue's `+0x16`, which no engine code reads (`audio_code_map.md`
//! §11.4) and where each platform keeps the other half of one `f32` (PC `0x3E99`, console `0x999A`:
//! `0x3E99_999A` is 0.3); and the index of a wave in a wavebank `vz.wad` does not hold, `vo_stream` in
//! `English.wad`, whose records each platform baked in its own order. The test counts both and fails on
//! any other byte.
//!
//! Game-gated: built by the `retail` feature, reads the `vz_wad`, `xbox_vz_wad` and `ps3_vz_wad` of the
//! repo-root `.mercs2-local.toml`, and fails if one is absent.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::Path;

use mercs2_formats::be_to_le::audio::{convert_sounddb, convert_soundbank};
use mercs2_formats::be_to_le::texture::typed_chunks;
use mercs2_formats::dlc_input::{parse_be_ffcs, parse_be_indx, read_be_block};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths::{local_config_ps3_vz_wad, local_config_vz_wad, local_config_xbox_vz_wad};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_SOUNDDB, TYPE_HASH_WAVEBANK};
use mercs2_formats::ucfx::{block_chunk_spans, UCFX_CSUM_BYTES};

type Convert = fn(&[u8]) -> Result<Vec<u8>, String>;
const TABLES: [(u32, &str, Convert); 2] =
    [(TYPE_HASH_SOUNDBANK, "soundbank", convert_soundbank), (TYPE_HASH_SOUNDDB, "sounddb", convert_sounddb)];
/// Where a sound table's body starts in its one-row container.
const BODY: usize = 40;

struct Pc {
    /// `(name, type)` → the container without its `CSUM` trailer.
    tables: HashMap<(u32, u32), Vec<u8>>,
    wavebanks: HashSet<u32>,
}

fn pc_tables(pc: &Path) -> Pc {
    let mut f = File::open(pc).unwrap();
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).unwrap();
    let (mut tables, mut wavebanks) = (HashMap::new(), HashSet::new());
    for b in 0..ar.indx.len() {
        let dec = decompress_block(&mut f, &ar.indx, b as u16).unwrap();
        for (e, r) in block_chunk_spans(&dec).unwrap() {
            if e.type_hash == TYPE_HASH_WAVEBANK {
                wavebanks.insert(e.name_hash);
            }
            if TABLES.iter().any(|(t, ..)| *t == e.type_hash) {
                let c = &dec[r];
                assert_eq!(&c[c.len() - UCFX_CSUM_BYTES..c.len() - 4], b"CSUM");
                let twin = c[..c.len() - UCFX_CSUM_BYTES].to_vec();
                assert!(tables.insert((e.name_hash, e.type_hash), twin).is_none(), "0x{:08X} twice in the PC bake", e.name_hash);
            }
        }
    }
    Pc { tables, wavebanks }
}

fn u32_at(b: &[u8], o: usize) -> usize {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize
}
fn u16_at(b: &[u8], o: usize) -> usize {
    usize::from(u16::from_le_bytes([b[o], b[o + 1]]))
}

/// The container offsets of a PC soundbank's two bake-specific fields: each single-track cue's `+0x16`,
/// and the index of each wave in a wavebank outside `held`.
fn bake_fields(c: &[u8], held: &HashSet<u32>) -> (HashSet<usize>, HashSet<usize>) {
    let b = &c[BODY..];
    let (groups, cues) = (u16_at(b, 8), u16_at(b, 10));
    let (group_table, cue_section, cue_table) = (u32_at(b, 0x14), u32_at(b, 0x18), u32_at(b, 0x1C));
    let mut words = HashSet::new();
    for i in 0..cues {
        let cue = cue_section + u32_at(b, cue_table + 4 * i);
        if b[cue + 5] == 0 {
            words.insert(BODY + cue + 0x16);
        }
    }
    let mut indices = HashSet::new();
    for i in 0..groups {
        let g = 0x20 + u32_at(b, group_table + 4 * i);
        let waves: Vec<usize> = match u32_at(b, g + 0x0C) {
            0 => vec![g + 0x34],
            1 => (0..usize::from(b[g + 0x2D])).map(|k| g + 0x68 + 12 * k).collect(),
            form => panic!("group {i} form {form}"),
        };
        for w in waves {
            if !held.contains(&(u32_at(b, w) as u32)) {
                indices.insert(BODY + w + 4);
            }
        }
    }
    (words, indices)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Tally {
    soundbanks: usize,
    sounddbs: usize,
    /// Soundbanks equal byte for byte.
    equal_soundbanks: usize,
    /// Single-track cue `+0x16` values that differ.
    cue_words: usize,
    /// Indices into a wavebank outside `vz.wad` that differ.
    foreign_indices: usize,
}

/// Convert every sound table of a console bake and compare it with its PC twin.
fn compare(console: &Path, pc: &Pc) -> Tally {
    let doh = std::fs::read(console).unwrap();
    let (_, rows) = parse_be_ffcs(&doh).unwrap();
    let indx = rows.iter().find(|r| r.tag == "INDX").unwrap();
    let mut t = Tally::default();
    let mut failures = Vec::new();
    for e in parse_be_indx(&doh, indx.offset as usize, indx.meta as usize) {
        let be = read_be_block(&doh, &e).unwrap();
        for (ty, name, convert) in TABLES {
            for (hash, chunk) in typed_chunks(&be, ty).unwrap() {
                let trailer = &chunk[chunk.len() - UCFX_CSUM_BYTES..chunk.len() - 4];
                assert!(trailer == b"CSUM" || trailer == b"MUSC", "{name} 0x{hash:08X} has no CSUM trailer");
                let twin = pc.tables.get(&(hash, ty)).unwrap_or_else(|| panic!("{name} 0x{hash:08X} has no PC twin"));
                let ours = match convert(&chunk[..chunk.len() - UCFX_CSUM_BYTES]) {
                    Ok(ours) => ours,
                    Err(err) => {
                        failures.push(format!("{name} 0x{hash:08X}: {err}"));
                        continue;
                    }
                };
                if ty == TYPE_HASH_SOUNDDB {
                    t.sounddbs += 1;
                } else {
                    t.soundbanks += 1;
                }
                if ours == *twin {
                    t.equal_soundbanks += usize::from(ty == TYPE_HASH_SOUNDBANK);
                    continue;
                }
                if ours.len() != twin.len() || ty == TYPE_HASH_SOUNDDB {
                    failures.push(format!("{name} 0x{hash:08X}: {} bytes against {}, or a sounddb that differs", ours.len(), twin.len()));
                    continue;
                }
                let (words, indices) = bake_fields(twin, &pc.wavebanks);
                let mut seen = HashSet::new();
                for at in (0..ours.len()).filter(|&o| ours[o] != twin[o]) {
                    let field = at & !1;
                    if words.contains(&field) {
                        t.cue_words += usize::from(seen.insert(field));
                    } else if indices.contains(&field) {
                        t.foreign_indices += usize::from(seen.insert(field));
                    } else {
                        failures.push(format!(
                            "{name} 0x{hash:08X}: +0x{at:X} differs outside the bake-specific fields: ours {:02X?} PC {:02X?}",
                            &ours[at & !3..(at & !3) + 4],
                            &twin[at & !3..(at & !3) + 4]
                        ));
                        break;
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}: {} tables differ from PC:\n{}", console.display(), failures.len(), failures.join("\n"));
    eprintln!("{}: {t:?}", console.display());
    t
}

#[test]
fn every_console_sound_table_equals_its_pc_twin() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pc = pc_tables(&local_config_vz_wad(root).unwrap_or_else(|e| panic!("{e}")));
    let xbox = compare(&local_config_xbox_vz_wad(root).unwrap_or_else(|e| panic!("{e}")), &pc);
    let ps3 = compare(&local_config_ps3_vz_wad(root).unwrap_or_else(|e| panic!("{e}")), &pc);
    assert_eq!(xbox, ps3, "the Xbox and PS3 bakes convert alike");
    let measured = Tally { soundbanks: 76, sounddbs: 77, equal_soundbanks: 36, cue_words: 127, foreign_indices: 34 };
    assert_eq!(xbox, measured);
}
