//! Retail census of the soundbank, sounddb and wavebank fields an author declares, over the three
//! PC archives that carry audio tables: `vz.wad`, `English.wad` and `shell.wad`.
//!
//! Game-gated, built by the `retail` feature: the archives are found through the repo-root
//! `.mercs2-local.toml` (`vz_wad`), with `English.wad` and `shell.wad` read from the same `data`
//! folder; a missing file fails the test.
//!
//! Each test measures one field and asserts what the retail data holds, so the counts and value
//! sets quoted in the format documentation stay tied to the data:
//! * group `+0x00` sound id against the guids of the cues that play the group, and where the three
//!   ids `FUN_008369e0` gates on the language hash sit;
//! * group `+0x10` (the value `PalSoundInstance::GetWavePriority` scales, `0x00837EDF`) and group
//!   `+0x20`;
//! * cue `+0x06` (the start limit `FUN_00834ad0` compares with the cue's live-instance counter);
//! * single-track cue `+0x16`;
//! * the single-wave group's wave weight (`+0x3C`);
//! * the wave record's `+0x00` clip hash against the group's sound id and the playing cue's guid;
//! * cue guids shared between per-bank sounddbs, within an archive and across archives;
//! * the block-entry name hash of every audio table against the bank hash inside it.
//!
//! ```text
//! cargo xtask retail-test
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use mercs2_audio::soundbank::{CueBody, GroupForm, Soundbank};
use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::WavebankFile;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths::local_config_vz_wad;
use mercs2_formats::hash::{pandemic_hash_m2 as m2, pandemic_hash_m2_extend as m2_extend};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_WAVEBANK};
use mercs2_formats::ucfx::{extract_data_chunk, walk_decompressed_block};

const TYPE_HASH_SOUNDDB: u32 = mercs2_audio::sounddb::ASSET_TYPE_SOUNDDB;
const TYPE_ID_SOUNDDB: u32 = 13;

/// The group sound ids `FUN_008369e0` (`0x00836A29`..`0x00836A3C`) refuses to start unless the Pal
/// engine's language hash (`+0x78`) is `m2("english")`.
const LANGUAGE_GATED_SOUND_IDS: [u32; 3] = [0xEA13_43AA, 0xC05D_8686, 0xBB8A_E67D];

/// One audio table pulled from an archive.
struct Table {
    block: u16,
    name_hash: u32,
    type_hash: u32,
    body: Vec<u8>,
}

/// One archive's audio tables, parsed, with its block path names.
struct Archive {
    tables: Vec<Table>,
    paths: Vec<String>,
    soundbanks: Vec<Soundbank>,
    sounddbs: Vec<SoundDb>,
    wavebanks: HashMap<u32, WavebankFile>,
}

/// The `data` folder holding the `vz.wad` named by the repo-root `.mercs2-local.toml`; panics with
/// the resolver's message when it is not configured.
fn data_dir() -> PathBuf {
    let vz = local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
    vz.parent()
        .unwrap_or_else(|| panic!("{} has no parent folder", vz.display()))
        .to_path_buf()
}

fn vz() -> &'static Archive {
    static A: OnceLock<Archive> = OnceLock::new();
    A.get_or_init(|| read_archive("vz.wad"))
}

fn english() -> &'static Archive {
    static A: OnceLock<Archive> = OnceLock::new();
    A.get_or_init(|| read_archive("English.wad"))
}

fn shell() -> &'static Archive {
    static A: OnceLock<Archive> = OnceLock::new();
    A.get_or_init(|| read_archive("shell.wad"))
}

/// Every wavebank / soundbank / sounddb entry of every block the ASET table registers one in.
fn read_archive(file_name: &str) -> Archive {
    let path = data_dir().join(file_name);
    assert!(
        path.is_file(),
        "{file_name} not found beside vz.wad at {}",
        path.display()
    );
    let mut f = File::open(&path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let size = f.metadata().unwrap_or_else(|e| panic!("{}: {e}", path.display())).len();
    let arch = load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut blocks: Vec<u16> = arch
        .aset
        .iter()
        .filter(|a| [TYPE_ID_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_SOUNDDB].contains(&a.type_id))
        .map(|a| a.block_index())
        .collect();
    blocks.sort_unstable();
    blocks.dedup();

    let mut tables = Vec::new();
    for b in blocks {
        let dec = decompress_block(&mut f, &arch.indx, b).unwrap_or_else(|e| panic!("{file_name} block {b}: {e}"));
        let (parsed, issues) = walk_decompressed_block(&dec, "retail_fields");
        assert!(
            issues.is_empty(),
            "{file_name} block {b} walks cleanly: {} issues",
            issues.len()
        );
        for (i, e) in parsed.entries.iter().enumerate() {
            if ![TYPE_HASH_WAVEBANK, TYPE_HASH_SOUNDBANK, TYPE_HASH_SOUNDDB].contains(&e.type_hash) {
                continue;
            }
            let body = extract_data_chunk(&parsed.containers[i])
                .unwrap_or_else(|| panic!("{file_name} block {b} entry {i} has no data chunk"));
            tables.push(Table {
                block: b,
                name_hash: e.name_hash,
                type_hash: e.type_hash,
                body,
            });
        }
    }

    let mut soundbanks = Vec::new();
    let mut sounddbs = Vec::new();
    let mut wavebanks = HashMap::new();
    for t in &tables {
        let what = format!(
            "{file_name} block {} 0x{:08X}/0x{:08X}",
            t.block, t.name_hash, t.type_hash
        );
        match t.type_hash {
            TYPE_HASH_SOUNDBANK => soundbanks.push(Soundbank::parse(&t.body).unwrap_or_else(|e| panic!("{what}: {e}"))),
            TYPE_HASH_SOUNDDB => sounddbs.push(SoundDb::parse(&t.body).unwrap_or_else(|e| panic!("{what}: {e}"))),
            _ => {
                let w = WavebankFile::parse(&t.body).unwrap_or_else(|e| panic!("{what}: {e}"));
                assert!(
                    wavebanks.insert(w.bank_hash, w).is_none(),
                    "{file_name}: one wavebank per bank hash"
                );
            }
        }
    }
    Archive {
        tables,
        paths: arch.paths,
        soundbanks,
        sounddbs,
        wavebanks,
    }
}

/// The bank hash every audio table carries at `+0x04`.
fn inner_bank_hash(t: &Table) -> u32 {
    u32::from_le_bytes(t.body[4..8].try_into().expect("a table is at least 8 bytes"))
}

/// For each group of `sb`, the guids of the cues in `sb` that play it: a single-track cue's group,
/// and every multi-track sound entry that names `sb`.
fn players(sb: &Soundbank) -> HashMap<usize, BTreeSet<u32>> {
    let mut out: HashMap<usize, BTreeSet<u32>> = HashMap::new();
    for c in &sb.cues {
        match &c.body {
            CueBody::SingleTrack {
                soundbank, group_index, ..
            } => {
                if *soundbank == sb.bank_hash {
                    out.entry(*group_index as usize).or_default().insert(c.guid);
                }
            }
            CueBody::MultiTrack(m) => {
                for e in m
                    .tracks
                    .iter()
                    .flat_map(|t| t.sounds.iter())
                    .flat_map(|s| s.entries.iter())
                {
                    if e.soundbank == sb.bank_hash {
                        out.entry(e.group_index as usize).or_default().insert(c.guid);
                    }
                }
            }
        }
    }
    out
}

/// A measured histogram: each value with its count, ascending by value.
type Counts<T> = &'static [(T, usize)];

/// A histogram of `f32` values keyed by their bit pattern.
fn f32_histogram(values: impl Iterator<Item = f32>) -> Vec<(u32, usize)> {
    let mut h: BTreeMap<u32, usize> = BTreeMap::new();
    for v in values {
        *h.entry(v.to_bits()).or_default() += 1;
    }
    h.into_iter().collect()
}

/// The three archives, named.
fn archives() -> [(&'static str, &'static Archive); 3] {
    [("vz.wad", vz()), ("English.wad", english()), ("shell.wad", shell())]
}

/// Group `+0x00` against the guids of the cues that play the group. `FUN_008369e0` is the one known
/// reader (the language gate below); the value is not the playing cue's guid in most groups.
#[test]
fn group_sound_id_against_the_playing_cue_guids() {
    // (archive, sound id equals a playing cue's guid, differs from every playing cue's guid, no cue
    // in the bank plays the group)
    let expected = [
        ("vz.wad", 411, 1278, 87),
        ("English.wad", 12893, 740, 3),
        ("shell.wad", 131, 145, 14),
    ];
    for ((name, a), (ename, eq_expected, ne_expected, orphan_expected)) in archives().into_iter().zip(expected) {
        assert_eq!(name, ename);
        let (mut eq, mut ne, mut orphan) = (0, 0, 0);
        for sb in &a.soundbanks {
            let p = players(sb);
            for (gi, g) in sb.groups.iter().enumerate() {
                match p.get(&gi) {
                    None => orphan += 1,
                    Some(guids) if guids.contains(&g.head.sound_id) => eq += 1,
                    Some(_) => ne += 1,
                }
            }
        }
        println!("{name}: sound id = a playing cue's guid {eq}, differs {ne}, group played by no cue {orphan}");
        assert_eq!((eq, ne, orphan), (eq_expected, ne_expected, orphan_expected), "{name}");
    }
}

/// Where the sound ids `FUN_008369e0` gates on the language hash sit, and the cues that play them.
#[test]
fn language_gated_sound_ids() {
    let mut found = Vec::new();
    for (name, a) in archives() {
        for sb in &a.soundbanks {
            let p = players(sb);
            for (gi, g) in sb.groups.iter().enumerate() {
                if LANGUAGE_GATED_SOUND_IDS.contains(&g.head.sound_id) {
                    let guids: Vec<u32> = p.get(&gi).map(|s| s.iter().copied().collect()).unwrap_or_default();
                    found.push((name, g.head.sound_id, sb.bank_hash, gi, guids));
                }
            }
        }
    }
    println!("{found:08X?}");
    assert_eq!(
        found,
        vec![
            ("vz.wad", 0xEA13_43AA, 0xB796_AE64, 17, vec![0xEA13_43AA]),
            ("vz.wad", 0xC05D_8686, 0xEB61_D6E1, 13, vec![0xD051_A52F]),
            ("vz.wad", 0xBB8A_E67D, 0xDD45_73C5, 100, vec![]),
            ("English.wad", 0xC05D_8686, 0x5078_7BFF, 142, vec![0x23DC_D31E]),
            ("shell.wad", 0xBB8A_E67D, 0xDD45_73C5, 100, vec![]),
        ]
    );
}

/// Group `+0x10`, the priority: `GetWavePriority` returns it times the wave's distance volume
/// (`0x00837EDF`), and a new instance takes a voice from the lowest-priority wave only when its own
/// value is higher (`FUN_00837830`).
#[test]
fn group_priority_values() {
    let expected: [(&str, Counts<u32>); 3] = [
        (
            "vz.wad",
            &[
                (0x0000_0000, 2),
                (0x3E99_999A, 281),
                (0x3ECC_CCCD, 141),
                (0x3F00_0000, 84),
                (0x3F19_999A, 108),
                (0x3F40_0000, 12),
                (0x3F53_F7CF, 1),
                (0x3F59_999A, 545),
                (0x3F66_6666, 40),
                (0x3F73_3333, 176),
                (0x3F74_7AE1, 56),
                (0x3F75_C28F, 140),
                (0x3F78_51EC, 37),
                (0x3F7A_E148, 1),
                (0x3F80_0000, 152),
            ],
        ),
        (
            "English.wad",
            &[
                (0x0000_0000, 11),
                (0x3E4C_CCCD, 3),
                (0x3F33_3333, 7532),
                (0x3F66_6666, 2),
                (0x3F7A_E148, 6088),
            ],
        ),
        (
            "shell.wad",
            &[(0x3F66_6666, 36), (0x3F73_3333, 102), (0x3F80_0000, 152)],
        ),
    ];
    for ((name, a), (ename, values)) in archives().into_iter().zip(expected) {
        assert_eq!(name, ename);
        let h = f32_histogram(
            a.soundbanks
                .iter()
                .flat_map(|sb| sb.groups.iter())
                .map(|g| g.head.unknown_10),
        );
        println!("{name}: group +0x10 {h:08X?}");
        assert_eq!(h, values, "{name}");
    }
}

/// Group `+0x20`: 1.0 in every group but one. No engine reader is known: the only copy is into the
/// wave (`0x00838F70`, wave `+0x68`), whose getter (wave vtable `+0x44`, `0x00838F30`) has no call site.
#[test]
fn group_word_20_values() {
    let mut odd = Vec::new();
    for (name, a) in archives() {
        let h = f32_histogram(
            a.soundbanks
                .iter()
                .flat_map(|sb| sb.groups.iter())
                .map(|g| g.head.unknown_20),
        );
        println!("{name}: group +0x20 {h:08X?}");
        let total: usize = a.soundbanks.iter().map(|sb| sb.groups.len()).sum();
        let ones = h
            .iter()
            .find(|(bits, _)| *bits == 1.0f32.to_bits())
            .map_or(0, |(_, n)| *n);
        assert_eq!(ones + odd_count(a), total, "{name}");
        for sb in &a.soundbanks {
            for (gi, g) in sb.groups.iter().enumerate() {
                if g.head.unknown_20 != 1.0 {
                    odd.push((name, sb.bank_hash, gi, g.head.unknown_20.to_bits()));
                }
            }
        }
    }
    assert_eq!(odd, vec![("vz.wad", 0xF217_5845, 105, 0x3FFF_FCB9)]);
}

fn odd_count(a: &Archive) -> usize {
    a.soundbanks
        .iter()
        .flat_map(|sb| sb.groups.iter())
        .filter(|g| g.head.unknown_20 != 1.0)
        .count()
}

/// Cue `+0x06`, the start limit: `FUN_00834ad0` starts a cue only while the u16 at `+0x04` of the
/// cue's runtime record is below it (or it is 0); `FUN_008354e0` adds one when an instance plays and
/// `FUN_00835850` takes one away when it finishes.
#[test]
fn cue_start_limit_values() {
    let expected: [(&str, Counts<u8>); 3] = [
        (
            "vz.wad",
            &[
                (0, 920),
                (1, 9),
                (2, 47),
                (3, 30),
                (4, 30),
                (5, 125),
                (7, 4),
                (10, 23),
                (15, 2),
                (20, 8),
            ],
        ),
        ("English.wad", &[(0, 13634), (1, 2)]),
        ("shell.wad", &[(0, 265), (1, 3), (2, 2)]),
    ];
    for ((name, a), (ename, values)) in archives().into_iter().zip(expected) {
        assert_eq!(name, ename);
        let mut h: BTreeMap<u8, usize> = BTreeMap::new();
        for c in a.soundbanks.iter().flat_map(|sb| sb.cues.iter()) {
            *h.entry(c.byte_06).or_default() += 1;
        }
        let h: Vec<(u8, usize)> = h.into_iter().collect();
        println!("{name}: cue +0x06 {h:?}");
        assert_eq!(h, values, "{name}");
    }
}

/// Single-track cue `+0x16`: 0, or one non-zero value shared by every single-track cue in the bank
/// that carries it. No engine reader is known: the cue's `{soundbank, group}` reference is read
/// only at `+0x00` and `+0x04` (`FUN_0082e7d0`, `FUN_0083d410`).
#[test]
fn single_track_cue_word_16_values() {
    // (archive, histogram, banks carrying a non-zero value)
    let expected: [(&str, Counts<u16>, usize); 3] = [
        (
            "vz.wad",
            &[
                (0x0000, 378),
                (0x3D75, 3),
                (0x3DA3, 4),
                (0x3DB8, 5),
                (0x3DCC, 10),
                (0x3E99, 68),
                (0x3EB6, 3),
                (0x3ECC, 8),
                (0x3F19, 5),
                (0x3F33, 9),
                (0x3F80, 7),
                (0x3F81, 5),
            ],
            37,
        ),
        ("English.wad", &[(0x0000, 11985), (0x036E, 1627)], 41),
        ("shell.wad", &[(0x0000, 187)], 0),
    ];
    for ((name, a), (ename, values, banks_expected)) in archives().into_iter().zip(expected) {
        assert_eq!(name, ename);
        let mut h: BTreeMap<u16, usize> = BTreeMap::new();
        let mut banks = 0;
        for sb in &a.soundbanks {
            let mut non_zero = BTreeSet::new();
            for c in &sb.cues {
                if let CueBody::SingleTrack { unknown_16, .. } = &c.body {
                    *h.entry(*unknown_16).or_default() += 1;
                    if *unknown_16 != 0 {
                        non_zero.insert(*unknown_16);
                    }
                }
            }
            assert!(
                non_zero.len() <= 1,
                "{name} bank 0x{:08X}: +0x16 values {non_zero:04X?}",
                sb.bank_hash
            );
            banks += non_zero.len();
        }
        let h: Vec<(u16, usize)> = h.into_iter().collect();
        println!("{name}: single-track cue +0x16 {h:04X?} in {banks} banks");
        assert_eq!(h, values, "{name}");
        assert_eq!(banks, banks_expected, "{name}");
    }
}

/// The single-wave group's wave weight (`+0x3C`) is 1.0 in every retail group. `FUN_0083d410` returns
/// the single-wave group's wave (`+0x34`) without reading it; only the multi-wave pickers
/// (`FUN_0083d450`, `FUN_0083d5c0`) read weights.
#[test]
fn single_wave_group_weight_is_one() {
    for ((name, a), expected) in archives().into_iter().zip([344, 12181, 161]) {
        let weights = a
            .soundbanks
            .iter()
            .flat_map(|sb| sb.groups.iter())
            .filter_map(|g| match &g.form {
                GroupForm::Single { wave, .. } => Some(wave.weight),
                GroupForm::Multi(_) => None,
            });
        let h = f32_histogram(weights);
        println!("{name}: single-wave weights {h:08X?}");
        assert_eq!(h, vec![(1.0f32.to_bits(), expected)], "{name}");
    }
}

/// The wave record's `+0x00` clip hash, for each single-wave group whose wavebank is in the same
/// archive, against the group's sound id and the guids of the cues that play the group; and how many
/// wavebanks repeat a clip hash. `FUN_00837830` reads the record at `+0x05`, `+0x06`, `+0x08`,
/// `+0x0C`, `+0x14`, `+0x18`, `+0x1C` and `+0x20`, not `+0x00`.
#[test]
fn wave_clip_hash_against_sound_id_and_cue_guid() {
    // (archive, clip = sound id, clip ≠ sound id, clip = a playing cue's guid, clip ≠ every playing
    // cue's guid, wavebanks with a repeated clip hash)
    let expected = [
        ("vz.wad", 142, 198, 47, 273, 0),
        ("English.wad", 11472, 709, 12174, 7, 0),
        ("shell.wad", 75, 86, 33, 119, 0),
    ];
    for ((name, a), e) in archives().into_iter().zip(expected) {
        assert_eq!(name, e.0);
        let (mut sid_eq, mut sid_ne, mut cue_eq, mut cue_ne) = (0, 0, 0, 0);
        for sb in &a.soundbanks {
            let p = players(sb);
            for (gi, g) in sb.groups.iter().enumerate() {
                let GroupForm::Single { wave, .. } = &g.form else {
                    continue;
                };
                let Some(wb) = a.wavebanks.get(&wave.wavebank) else {
                    continue;
                };
                let clip = wb.records[wave.index as usize].clip_hash;
                if clip == g.head.sound_id {
                    sid_eq += 1
                } else {
                    sid_ne += 1
                }
                if let Some(guids) = p.get(&gi) {
                    if guids.contains(&clip) {
                        cue_eq += 1
                    } else {
                        cue_ne += 1
                    }
                }
            }
        }
        let repeated = a
            .wavebanks
            .values()
            .filter(|wb| wb.records.iter().map(|r| r.clip_hash).collect::<BTreeSet<_>>().len() != wb.records.len())
            .count();
        println!(
            "{name}: clip = sound id {sid_eq}, differs {sid_ne}; clip = playing cue guid {cue_eq}, differs {cue_ne}; \
             wavebanks repeating a clip hash {repeated}"
        );
        assert_eq!((name, sid_eq, sid_ne, cue_eq, cue_ne, repeated), e);
    }
}

/// Cue guids in the per-bank sounddbs: unique within each archive; the guids `vz.wad` and
/// `shell.wad` share all come from the banks both archives carry. When two loaded tables list one
/// guid, FindCue (`FUN_00835a70`) returns the entry of the table loaded first.
#[test]
fn cue_guids_across_sounddbs() {
    let mut owner: HashMap<u32, Vec<(&str, u32)>> = HashMap::new();
    for (name, a) in archives() {
        let mut seen = BTreeSet::new();
        for db in &a.sounddbs {
            for c in &db.cues {
                assert!(seen.insert(c.guid), "{name}: cue guid 0x{:08X} in two sounddbs", c.guid);
                owner.entry(c.guid).or_default().push((name, db.self_hash));
            }
        }
    }
    let mut shared: BTreeMap<Vec<(&str, u32)>, usize> = BTreeMap::new();
    for tables in owner.values().filter(|v| v.len() > 1) {
        *shared.entry(tables.clone()).or_default() += 1;
    }
    let shared: Vec<(Vec<(&str, u32)>, usize)> = shared.into_iter().collect();
    println!("cue guids listed by more than one archive's sounddbs: {shared:08X?}");
    let both = |bank: &str| vec![("vz.wad", m2(bank)), ("shell.wad", m2(bank))];
    assert_eq!(
        shared,
        vec![(both("music"), 158), (both("ui_hud"), 88), (both("ui_shell"), 24)]
    );
    assert_eq!(
        shell().sounddbs.iter().map(|db| db.cues.len()).sum::<usize>(),
        158 + 88 + 24
    );

    // The tables of the banks both archives carry, byte for byte.
    let mut identical = BTreeSet::new();
    let mut different = BTreeSet::new();
    for s in &shell().tables {
        let twin = vz()
            .tables
            .iter()
            .find(|v| v.name_hash == s.name_hash && v.type_hash == s.type_hash);
        let twin = twin.unwrap_or_else(|| {
            panic!(
                "shell.wad 0x{:08X}/0x{:08X} is also in vz.wad",
                s.name_hash, s.type_hash
            )
        });
        if twin.body == s.body {
            identical.insert((s.name_hash, s.type_hash));
        } else {
            different.insert((s.name_hash, s.type_hash));
        }
    }
    println!("shell.wad tables identical to vz.wad's {identical:08X?}; different {different:08X?}");
    assert_eq!(identical.len(), 10);
    assert!(different.is_empty());
}

/// The block-entry name hash of every audio table: the bank hash inside the table (`+0x04`) in
/// `vz.wad` and `shell.wad`, and `m2(<bank name>.english)` in `English.wad` — the name
/// mrxsoundbanks.lua `_GetLocalizedName` builds for a `vo_` bank. The block paths spell 42 of the
/// English bank names out.
#[test]
fn english_wad_entries_are_keyed_by_the_localized_name() {
    for (name, a) in [("vz.wad", vz()), ("shell.wad", shell())] {
        for t in &a.tables {
            assert_eq!(t.name_hash, inner_bank_hash(t), "{name} block {}", t.block);
        }
    }
    let e = english();
    for t in &e.tables {
        assert_eq!(
            t.name_hash,
            m2_extend(inner_bank_hash(t), ".english"),
            "English.wad block {} bank 0x{:08X}",
            t.block,
            inner_bank_hash(t)
        );
    }
    assert_eq!(e.tables.len(), 179);
    let mut spelled = Vec::new();
    for p in &e.paths {
        let stem = p.rsplit('\\').next().unwrap_or(p).trim_end_matches("_P000_Q3.block");
        if let Some(bank) = stem.strip_suffix(".english") {
            let keyed = e
                .tables
                .iter()
                .filter(|t| t.name_hash == m2(stem) && inner_bank_hash(t) == m2(bank))
                .count();
            assert!(
                keyed > 0,
                "block path {p}: no table keyed m2({stem}) holding bank m2({bank})"
            );
            spelled.push(bank.to_string());
        }
    }
    assert_eq!(spelled.len(), 42);
    let vo_stream = e
        .tables
        .iter()
        .find(|t| t.type_hash == TYPE_HASH_WAVEBANK && inner_bank_hash(t) == m2("vo_stream"));
    let vo_stream = vo_stream.expect("English.wad holds the vo_stream wavebank");
    assert_eq!(vo_stream.name_hash, m2("vo_stream.english"));
    assert_eq!(vo_stream.name_hash, 0xEADF_9519);
}

/// The `.pws` name each streamed wavebank carries at `+0x18`: `FUN_0082e8f0` hands this field to the
/// stream-file table (`[0x011763F4]`), the name `Sound.OpenStreamFile(path, alias)` registers as the
/// alias.
#[test]
fn streamed_wavebank_names() {
    let mut names = Vec::new();
    for (name, a) in archives() {
        let mut v: Vec<(u32, String)> = a
            .wavebanks
            .values()
            .filter_map(|wb| wb.stream_name.clone().map(|n| (wb.bank_hash, n)))
            .collect();
        v.sort();
        for (bank, stream) in v {
            names.push((name, bank, stream));
        }
    }
    println!("{names:?}");
    let expected: Vec<(&str, u32, String)> = vec![
        ("vz.wad", m2("music"), "music.pws".into()),
        ("vz.wad", m2("ambience"), "ambience.pws".into()),
        ("English.wad", m2("vo_stream"), "vo_stream.pws".into()),
        ("shell.wad", m2("music"), "music.pws".into()),
    ];
    let mut expected = expected;
    expected.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    names.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
    assert_eq!(names, expected);
}
