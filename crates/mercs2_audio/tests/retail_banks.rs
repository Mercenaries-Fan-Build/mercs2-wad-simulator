//! Retail proof of the three bank codecs, against the installed `vz.wad`.
//!
//! Gated on the game: set `MERCS2_GAME_DIR` (the install root, its `data` folder, or `vz.wad` itself).
//! Without it every test here prints `SKIPPING` and returns — loudly, never silently green.
//!
//! What this proves, over EVERY audio table in `vz.wad` (95 wavebanks, 76 soundbanks, 77 sounddbs):
//! * each `data` body re-encodes byte-identically from its parsed records — the header framing, the
//!   soundbank group/cue offset tables, the record-relative wave offsets, the 16-byte blob alignment
//!   and the zero padding a body carries after its last blob;
//! * each container is exactly `mercs2_formats::ucfx::build_wrapped_block`'s wrapping of that body,
//!   and a bank's soundbank, sounddb and wavebank sit in one block under one name hash;
//! * the sounddb's third field is the SOUNDBANK cue index;
//! * the `ui_PDA_Open_01_st` group/cue values the encoder presets carry;
//! * how many retail cues the corrected chain resolves, and why the rest do not.
//!
//! ```text
//! MERCS2_GAME_DIR=/path/to/game cargo test -p mercs2_audio --test retail_banks -- --nocapture
//! ```

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::sync::OnceLock;

use mercs2_audio::duration::{cue_length_s, wave_length_s};
use mercs2_audio::encode::{RETAIL_CATEGORIES, UI_PDA_OPEN_CUE, UI_PDA_OPEN_GROUP};
use mercs2_audio::soundbank::{CueBody, GroupForm, Soundbank};
use mercs2_audio::sounddb::SoundDb;
use mercs2_audio::wave::{WaveData, WavebankFile};
use mercs2_audio::{AudioEngine, ResolveError};
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths::vz_wad_from_env;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_WAVEBANK};
use mercs2_formats::ucfx::{build_wrapped_block, extract_data_chunk, walk_decompressed_block};

const TYPE_HASH_SOUNDDB: u32 = mercs2_audio::sounddb::ASSET_TYPE_SOUNDDB;
const TYPE_ID_SOUNDDB: u32 = 13;

/// One audio table pulled from the WAD.
struct Table {
    block: u16,
    name_hash: u32,
    type_hash: u32,
    body: Vec<u8>,
}

/// Every wavebank / soundbank / sounddb entry of every block the ASET table registers one in, read
/// once per test binary. `None` (after a loud SKIPPING line) when the game is not configured.
fn retail_tables() -> Option<&'static [Table]> {
    static TABLES: OnceLock<Option<Vec<Table>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let path = vz_wad_from_env()?;
        Some(read_tables(&path))
    });
    if tables.is_none() {
        eprintln!("SKIPPING: set MERCS2_GAME_DIR to the Mercenaries 2 install to run the retail bank tests");
    }
    tables.as_deref()
}

/// Pull the audio tables out of `path`, checking every container against `build_wrapped_block` on
/// the way out.
fn read_tables(path: &std::path::Path) -> Vec<Table> {
    let mut f = File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let size = f.metadata().expect("wad metadata").len();
    let arch = load_ffcs_archive(&mut f, size).expect("FFCS archive");
    let mut blocks: Vec<u16> = arch
        .aset
        .iter()
        .filter(|a| [TYPE_ID_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_SOUNDDB].contains(&a.type_id))
        .map(|a| a.block_index())
        .collect();
    blocks.sort_unstable();
    blocks.dedup();

    let mut out = Vec::new();
    for b in blocks {
        let dec = decompress_block(&mut f, &arch.indx, b).unwrap_or_else(|e| panic!("block {b}: {e}"));
        let (parsed, issues) = walk_decompressed_block(&dec, "retail_banks");
        assert!(issues.is_empty(), "block {b} walks cleanly: {} issues", issues.len());
        for (i, e) in parsed.entries.iter().enumerate() {
            if ![TYPE_HASH_WAVEBANK, TYPE_HASH_SOUNDBANK, TYPE_HASH_SOUNDDB].contains(&e.type_hash) {
                continue;
            }
            let container = &parsed.containers[i];
            let body = extract_data_chunk(container).expect("audio container has a data chunk");
            let wrapped = build_wrapped_block(e.name_hash, e.type_hash, &body);
            assert_eq!(e.field_c, 0, "block {b} entry {i}: entry-table +0x08");
            assert!(
                wrapped[20..] == container[..],
                "block {b} entry {i} (0x{:08X}/0x{:08X}): container is build_wrapped_block's wrapping",
                e.name_hash,
                e.type_hash
            );
            out.push(Table { block: b, name_hash: e.name_hash, type_hash: e.type_hash, body });
        }
    }
    out
}

fn of_type(tables: &[Table], type_hash: u32) -> impl Iterator<Item = &Table> {
    tables.iter().filter(move |t| t.type_hash == type_hash)
}

#[test]
fn every_retail_table_re_encodes_byte_identically() {
    let Some(tables) = retail_tables() else { return };

    // ---- wavebanks ------------------------------------------------------------------------------
    let (mut wavebanks, mut streamed, mut padded, mut clips) = (0, 0, 0, 0);
    let mut pad_sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for t in of_type(tables, TYPE_HASH_WAVEBANK) {
        let file = WavebankFile::parse(&t.body)
            .unwrap_or_else(|e| panic!("wavebank 0x{:08X}: {e}", t.name_hash));
        assert_eq!(file.bank_hash, t.name_hash, "wavebank +0x04 is the entry's name hash");
        assert_eq!(file.to_bytes().expect("re-encodes"), t.body, "wavebank 0x{:08X}", t.name_hash);
        wavebanks += 1;
        clips += file.records.len();
        if file.stream_name.is_some() {
            streamed += 1;
            continue;
        }
        // The bytes after the last blob: zero fill up to the 16-byte boundary the body ends on.
        let last_end = embedded_end(&file);
        if last_end != t.body.len() {
            padded += 1;
            *pad_sizes.entry(t.body.len() - last_end).or_default() += 1;
            assert_eq!(t.body.len() % 16, 0);
            assert!(t.body.len() - last_end < 16);
        }
    }
    println!(
        "wavebanks: {wavebanks} re-encoded byte-identically ({clips} records, {streamed} streamed); \
         {padded} end past their last blob, by {pad_sizes:?} bytes of 16-alignment fill"
    );
    assert_eq!(wavebanks, 95);
    assert_eq!(streamed, 2);
    // 54 of the 93 embedded banks do not end at their last blob: the body runs on to the next
    // 16-byte boundary in zero fill, which the byte-identical re-encode above reproduces.
    assert_eq!(padded, 54);

    // ---- soundbanks -----------------------------------------------------------------------------
    let (mut soundbanks, mut single_g, mut multi_g, mut single_c, mut multi_c) = (0, 0, 0, 0, 0);
    for t in of_type(tables, TYPE_HASH_SOUNDBANK) {
        let sb = Soundbank::parse(&t.body)
            .unwrap_or_else(|e| panic!("soundbank 0x{:08X}: {e}", t.name_hash));
        assert_eq!(sb.bank_hash, t.name_hash, "soundbank +0x04 is the entry's name hash");
        assert_eq!(sb.to_bytes().expect("re-encodes"), t.body, "soundbank 0x{:08X}", t.name_hash);
        soundbanks += 1;
        for g in &sb.groups {
            match g.form {
                GroupForm::Single { .. } => single_g += 1,
                GroupForm::Multi(_) => multi_g += 1,
            }
        }
        for c in &sb.cues {
            match &c.body {
                CueBody::SingleTrack { soundbank, .. } => {
                    assert_eq!(*soundbank, sb.bank_hash, "a single-track cue names its own bank");
                    single_c += 1
                }
                CueBody::MultiTrack(m) => {
                    multi_c += 1;
                    for sound in m.tracks.iter().flat_map(|t| t.sounds.iter()) {
                        for entry in &sound.entries {
                            assert!(
                                (entry.group_index as usize) < sb.groups.len() || entry.soundbank != sb.bank_hash,
                                "a multi-track entry into its own bank names an existing group"
                            );
                        }
                    }
                }
            }
        }
    }
    println!(
        "soundbanks: {soundbanks} re-encoded byte-identically; groups {single_g} single-wave + \
         {multi_g} multi-wave; cues {single_c} single-track + {multi_c} multi-track"
    );
    assert_eq!(soundbanks, 76);

    // ---- sounddbs -------------------------------------------------------------------------------
    let soundbanks: HashMap<(u16, u32), Soundbank> = of_type(tables, TYPE_HASH_SOUNDBANK)
        .map(|t| ((t.block, t.name_hash), Soundbank::parse(&t.body).expect("parsed above")))
        .collect();
    let mut sounddbs = 0;
    for t in of_type(tables, TYPE_HASH_SOUNDDB) {
        let db = SoundDb::parse(&t.body).unwrap_or_else(|e| panic!("sounddb 0x{:08X}: {e}", t.name_hash));
        assert_eq!(db.self_hash, t.name_hash);
        assert_eq!(db.to_bytes().expect("re-encodes"), t.body, "sounddb 0x{:08X}", t.name_hash);
        sounddbs += 1;
        if t.name_hash == m2("mercs2globals") {
            assert!(db.cues.is_empty());
            assert_eq!(db.categories, RETAIL_CATEGORIES, "the encoder's category table is retail's");
            assert_eq!(db.params, vec![0xD11A_DEF6, 0xD913_464B]);
            continue;
        }
        // A per-bank sounddb lists every cue of the same-named soundbank in the same block, exactly
        // once, and its third field is that cue's index in the SOUNDBANK.
        let sb = soundbanks
            .get(&(t.block, t.name_hash))
            .unwrap_or_else(|| panic!("sounddb 0x{:08X} has a soundbank beside it", t.name_hash));
        assert!(db.categories.is_empty() && db.params.is_empty());
        assert_eq!(db.cues.len(), sb.cues.len());
        let mut seen = vec![false; sb.cues.len()];
        for e in &db.cues {
            assert_eq!(e.bank_hash, t.name_hash);
            let i = e.cue_index as usize;
            assert_eq!(sb.cues[i].guid, e.guid, "sounddb 0x{:08X}: cue index {i}", t.name_hash);
            assert!(!std::mem::replace(&mut seen[i], true), "each cue listed once");
        }
    }
    println!("sounddbs: {sounddbs} re-encoded byte-identically");
    assert_eq!(sounddbs, 77);

    // ---- one block, one name hash ---------------------------------------------------------------
    // Every soundbank has its sounddb in the same block under the same name hash, and all but one
    // also have their wavebank there. The exception, 0xDCCF8AFA in sound_resident, plays waves from
    // other blocks' wavebanks (its groups name them), so it ships no wavebank of its own.
    let sibling = |t: &Table, ty: u32| {
        tables.iter().any(|u| u.block == t.block && u.name_hash == t.name_hash && u.type_hash == ty)
    };
    let mut without_wavebank = Vec::new();
    for t in of_type(tables, TYPE_HASH_SOUNDBANK) {
        assert!(sibling(t, TYPE_HASH_SOUNDDB), "soundbank 0x{:08X} has its sounddb beside it", t.name_hash);
        if !sibling(t, TYPE_HASH_WAVEBANK) {
            without_wavebank.push(t.name_hash);
        }
    }
    println!("soundbanks without a same-block wavebank: {without_wavebank:08X?}");
    assert_eq!(without_wavebank, vec![0xDCCF_8AFA]);
}

/// End of the last embedded blob, in body offsets.
fn embedded_end(file: &WavebankFile) -> usize {
    let table_end = 24 + 36 * file.records.len();
    file.records.iter().fold(table_end, |cursor, r| match &r.data {
        WaveData::Embedded(bytes) => ((cursor + 15) & !15) + bytes.len(),
        WaveData::Streamed { .. } => cursor,
    })
}

#[test]
fn ui_pda_open_group_and_cue_values_match_the_presets() {
    let Some(tables) = retail_tables() else { return };
    let ui_hud = m2("ui_hud");
    let body = |ty| &tables.iter().find(|t| t.name_hash == ui_hud && t.type_hash == ty).expect("ui_hud table").body;
    let db = SoundDb::parse(body(TYPE_HASH_SOUNDDB)).expect("sounddb");
    let sb = Soundbank::parse(body(TYPE_HASH_SOUNDBANK)).expect("soundbank");
    let wb = WavebankFile::parse(body(TYPE_HASH_WAVEBANK)).expect("wavebank");

    let guid = m2("ui_PDA_Open_01_st");
    let entry = db.find_cue(guid).expect("ui_PDA_Open_01_st is in ui_hud's sounddb");
    assert_eq!(entry.cue_index, 57);
    let cue = &sb.cues[entry.cue_index as usize];
    assert_eq!(cue.guid, guid);
    let CueBody::SingleTrack { soundbank, group_index, unknown_16 } = cue.body else {
        panic!("ui_PDA_Open_01_st is single-track")
    };
    assert_eq!((soundbank, group_index), (ui_hud, 70));
    assert_eq!(cue.byte_06, UI_PDA_OPEN_CUE.byte_06);
    assert_eq!(cue.gain.to_bits(), UI_PDA_OPEN_CUE.gain.to_bits());
    assert_eq!(unknown_16, UI_PDA_OPEN_CUE.unknown_16);

    let group = &sb.groups[70];
    let h = &group.head;
    assert_eq!(h.category, m2("ui"));
    assert_eq!(h.sound_id, guid, "this group's sound id is the cue guid");
    let GroupForm::Single { gain, unknown_30, wave } = group.form else {
        panic!("group 70 is single-wave")
    };
    let p = UI_PDA_OPEN_GROUP;
    for (field, got, want) in [
        ("+0x10", h.unknown_10, p.unknown_10),
        ("+0x18 min distance", h.min_distance, p.min_distance),
        ("+0x1C max distance", h.max_distance, p.max_distance),
        ("+0x20", h.unknown_20, p.unknown_20),
        ("+0x24 pitch", h.pitch, p.pitch),
        ("+0x28", h.unknown_28, p.unknown_28),
        ("+0x2C gain", gain, p.gain),
        ("+0x30", unknown_30, p.unknown_30),
        ("wave weight", wave.weight, p.wave_weight),
    ] {
        assert_eq!(got.to_bits(), want.to_bits(), "group 70 {field}: {got} vs preset {want}");
    }
    assert_eq!(h.unknown_14, p.unknown_14);

    assert_eq!(wave.wavebank, ui_hud);
    let rec = &wb.records[wave.index as usize];
    assert_eq!(rec.clip_hash, guid, "the clip hash is the cue guid here too");
    assert_eq!(cue.length_s.to_bits(), (wave_length_s(rec).unwrap() as f32).to_bits());
    println!(
        "ui_PDA_Open_01_st: cue 57 -> group 70 -> ui_hud wave {} ({} frames @ {} Hz, {:.4} s)",
        wave.index, rec.frames, rec.sample_rate, cue.length_s
    );
}

/// The cue-length rule (`mercs2_audio::duration`), on every cue of `vz.wad` and `English.wad`. It
/// reproduces every length bit for bit except these 16, whose authored values no rule over the tables yields: 9 far from any end of the cue
/// (hand-set or stale), 6 one unit in the last place off, and one −1 on a cue with nothing looping.
const LENGTH_EXCEPTIONS: [(u32, usize); 16] = [
    (0x0873_D14E, 55),
    (0x08E4_3A91, 2),
    (0x5EE5_CB98, 4),
    (0x7664_67E0, 0),
    (0x874E_66BC, 5),
    (0xAF27_F8D2, 10),
    (0xB796_AE64, 22),
    (0xB796_AE64, 25),
    (0xB796_AE64, 65),
    (0xB796_AE64, 66),
    (0xDCCF_8AFA, 6),
    (0xDCCF_8AFA, 7),
    (0xDCCF_8AFA, 45),
    (0xEB61_D6E1, 8),
    (0xEB61_D6E1, 9),
    (0xF217_5845, 59),
];

#[test]
fn cue_length_rule_reproduces_every_retail_cue_but_sixteen() {
    let Some(tables) = retail_tables() else { return };
    let Some(english) = mercs2_formats::game_paths::wad_from_env("English.wad").map(|p| read_tables(&p)) else {
        return eprintln!("SKIPPING: English.wad not found beside vz.wad; nine vz.wad cues play its waves");
    };
    let wavebanks: HashMap<u32, WavebankFile> = of_type(tables, TYPE_HASH_WAVEBANK)
        .chain(of_type(&english, TYPE_HASH_WAVEBANK))
        .map(|t| {
            let f = WavebankFile::parse(&t.body).expect("wavebank");
            (f.bank_hash, f)
        })
        .collect();
    let soundbanks: HashMap<u32, Soundbank> = of_type(tables, TYPE_HASH_SOUNDBANK)
        .chain(of_type(&english, TYPE_HASH_SOUNDBANK))
        .map(|t| {
            let sb = Soundbank::parse(&t.body).expect("soundbank");
            (sb.bank_hash, sb)
        })
        .collect();
    let (mut total, mut matched) = (0, 0);
    let mut exceptions = Vec::new();
    for sb in soundbanks.values() {
        for (i, cue) in sb.cues.iter().enumerate() {
            total += 1;
            let len = cue_length_s(
                &cue.body,
                |bank, g| soundbanks.get(&bank).and_then(|b| b.groups.get(g as usize)),
                |bank, w| wavebanks.get(&bank).and_then(|b| b.records.get(w as usize)),
            )
            .unwrap_or_else(|e| panic!("soundbank 0x{:08X} cue {i}: {e}", sb.bank_hash));
            if len.to_bits() == cue.length_s.to_bits() {
                matched += 1;
            } else {
                exceptions.push((sb.bank_hash, i));
            }
        }
    }
    exceptions.sort_unstable();
    println!(
        "cue length rule: {matched} of {total} vz.wad + English.wad cues bit for bit; exceptions {exceptions:08X?}"
    );
    assert_eq!(soundbanks.len(), 76 + 68, "every vz.wad and English.wad soundbank, none shadowed");
    assert_eq!(exceptions, LENGTH_EXCEPTIONS);
}

/// Resolve every `vz.wad` sounddb entry through the chain — every track of a multi-track cue, every
/// entry of every sound, every wave of every group — with every `vz.wad` bank resident, and then again
/// with `English.wad`'s wavebanks resident too (the game mounts it as the language archive; two
/// wavebanks `vz.wad` cues name live there). Only a wave that streams from a `.pws`, or a wavebank not
/// resident, may stop a cue, and each such cue is named; anything else fails the test. Every resolved
/// cue is then started once through the engine's own picks (fixed seed).
#[test]
fn every_retail_cue_resolves_but_the_streamed_and_absent_ones() {
    let Some(tables) = retail_tables() else { return };
    let english = mercs2_formats::game_paths::wad_from_env("English.wad").map(|p| read_tables(&p));

    let mut eng = AudioEngine::default();
    eng.set_rng_seed(0x5EED_0001);
    for t in of_type(tables, TYPE_HASH_WAVEBANK) {
        eng.load_wavebank(&t.body).expect("wavebank loads");
    }
    for t in of_type(tables, TYPE_HASH_SOUNDBANK) {
        eng.load_soundbank(&t.body).expect("soundbank loads");
    }
    let mut catalog = SoundDb::default();
    for t in of_type(tables, TYPE_HASH_SOUNDDB) {
        catalog.merge(&SoundDb::parse(&t.body).expect("sounddb"));
    }
    eng.set_sounddb(catalog);
    let vz_only = tally(&mut eng, tables, "vz.wad banks");
    assert_eq!(vz_only.total, 1198);
    assert_eq!(vz_only.streamed.values().sum::<usize>(), 177);
    assert_eq!(vz_only.absent.values().map(Vec::len).sum::<usize>(), 9);
    assert_eq!(vz_only.resolved, 1198 - 177 - 9);
    assert_eq!(vz_only.filtered, FILTERED);
    assert_eq!(vz_only.filtered_audible, FILTERED, "each filtered cue mixes audibly through its filter");
    assert_eq!(vz_only.looping, LOOPING_WAVE, "every cue that reaches a looping wave plays");
    assert_eq!(vz_only.played, vz_only.resolved);
    assert_eq!(vz_only.played, 1012);

    let Some(english) = english else {
        return eprintln!("SKIPPING the English.wad pass: English.wad not found beside vz.wad");
    };
    for t in of_type(&english, TYPE_HASH_WAVEBANK) {
        eng.load_wavebank(&t.body).expect("English.wad wavebank loads");
    }
    let with_english = tally(&mut eng, tables, "vz.wad banks + English.wad wavebanks");
    assert_eq!(with_english.streamed.values().sum::<usize>(), 179);
    assert!(with_english.absent.is_empty());
    assert_eq!(with_english.resolved, 1198 - 179);
    let mut looping: Vec<u32> = LOOPING_WAVE.iter().chain(&LOOPING_WAVE_ENGLISH).copied().collect();
    looping.sort_unstable();
    assert_eq!(with_english.looping, looping);
    assert_eq!(with_english.filtered, FILTERED);
    assert_eq!(with_english.filtered_audible, FILTERED, "each filtered cue mixes audibly through its filter");
    assert_eq!(with_english.played, 1019);
}

/// The `vz.wad` cues (sounddb guids) that can reach a group that loops its wave (`+0x2C` ≠ 0); the
/// wave plays `1 + count` times (`FUN_00839e90`, see `mercs2_audio::mixer::PcmSource::with_loops`).
const LOOPING_WAVE: [u32; 278] = [
    0x0016_FCE7, 0x00BD_27AB, 0x0585_E46D, 0x05A8_9198, 0x06C9_BEB2, 0x0745_A4F8, 0x0787_3231, 0x088B_F1D4,
    0x08DA_6515, 0x097F_1626, 0x0B8B_F9DB, 0x0CDF_A3A7, 0x0DF4_2E83, 0x10E8_4EAF, 0x113C_ACF2, 0x128D_7756,
    0x1594_8DB2, 0x1683_9356, 0x16B3_E27E, 0x174D_62D1, 0x178E_BD44, 0x1793_3A72, 0x19F9_C7B0, 0x1B2C_8599,
    0x1C23_83FC, 0x1C89_B183, 0x1F17_3024, 0x1FF7_A450, 0x2130_2C2F, 0x219E_5C43, 0x229D_0B74, 0x24B3_3959,
    0x25B0_115D, 0x25C8_D0C8, 0x279F_A753, 0x2A89_6BC9, 0x2AAA_0B62, 0x2B5D_D674, 0x2C58_9023, 0x2C66_5119,
    0x2D07_10A8, 0x2D3A_7CCA, 0x2D9F_6C15, 0x2DA2_EA12, 0x2DEA_8C16, 0x2EE5_D43B, 0x2F5B_FA94, 0x311F_5638,
    0x3121_8B78, 0x312B_9CAF, 0x3187_437C, 0x31A4_AC81, 0x322A_B2D9, 0x32E1_1E58, 0x3465_9B09, 0x34BE_40AB,
    0x3743_1832, 0x381B_2F51, 0x3860_47CD, 0x3909_3900, 0x3995_AE8F, 0x3A24_9447, 0x3C64_6729, 0x40E5_D97A,
    0x42D3_9BA5, 0x442A_E8E1, 0x4525_CD97, 0x46A9_AF4E, 0x4757_5E61, 0x4957_F4DD, 0x4A10_F91C, 0x4A6F_FDF7,
    0x4BB9_F74C, 0x4CFF_252A, 0x4D8F_1F3E, 0x4E6B_DC65, 0x4E72_43A4, 0x4FEC_6846, 0x5167_007A, 0x51C2_CE22,
    0x51DA_112F, 0x53D4_95E6, 0x56F9_41BA, 0x5889_FEA0, 0x589F_A9B7, 0x59FE_30CE, 0x5A7D_35D3, 0x5B83_7744,
    0x5C0F_C7AE, 0x5C18_56C1, 0x5E0D_A733, 0x6044_C34D, 0x6187_8B4C, 0x61E7_222D, 0x62F8_148E, 0x63B1_E6A7,
    0x63E1_188C, 0x642D_0999, 0x646F_2829, 0x6766_5F05, 0x683D_534F, 0x687B_29B1, 0x6BDF_4558, 0x6D39_7DDC,
    0x6DA1_804E, 0x6F35_D8CE, 0x6F80_7724, 0x709D_32D8, 0x7139_E42B, 0x7226_B409, 0x7255_3CA9, 0x7281_D15E,
    0x72A1_8B02, 0x77BB_255F, 0x780A_8ED2, 0x7863_D6E7, 0x787B_2676, 0x78DA_AE01, 0x796E_BCC7, 0x79FB_FC79,
    0x7A90_1A3C, 0x7AE1_B42B, 0x7BE1_5F8C, 0x7D1B_4318, 0x7D23_E93E, 0x7DB0_D0B0, 0x7EA1_0363, 0x7EC0_F47A,
    0x7F0E_F16C, 0x7F97_C440, 0x800B_20B5, 0x80FB_0A1E, 0x8114_C5B4, 0x828C_7F55, 0x8498_32CB, 0x8503_8C6B,
    0x8573_20E0, 0x857F_A9C6, 0x86D9_56AD, 0x873A_44F7, 0x89B1_7D30, 0x8A31_1ED6, 0x8A45_6562, 0x8B54_901F,
    0x8C2B_F988, 0x8D4B_3D07, 0x8F89_69FE, 0x8FC2_7EB1, 0x900B_FD44, 0x924A_EDC3, 0x94FA_3E86, 0x9566_9C4C,
    0x9575_5D14, 0x972C_79DF, 0x9908_462F, 0x999A_C2DD, 0x99D0_2C22, 0x9A41_6330, 0x9D14_1AE0, 0x9D43_565B,
    0x9EBE_1CC1, 0x9F25_9FE3, 0x9FD1_47BC, 0xA1DE_2A81, 0xA25C_431C, 0xA29E_B5EE, 0xA3C3_7324, 0xA4EB_978E,
    0xA51B_442A, 0xA5B5_B30B, 0xA5C3_D085, 0xA6A6_0B2E, 0xA772_04C8, 0xA84F_5013, 0xA88D_9D17, 0xAD02_81B5,
    0xAD86_1896, 0xAD96_4C6F, 0xADE4_96EE, 0xAF98_A6E8, 0xB0C4_BA27, 0xB14C_F8E8, 0xB18B_DE1B, 0xB246_FB78,
    0xB24A_5DF2, 0xB501_6596, 0xB509_E514, 0xB53E_7A38, 0xB5BB_A500, 0xB637_3CAA, 0xB865_0B74, 0xB9B4_ACA0,
    0xBA9B_C27A, 0xBBF9_8914, 0xBC28_C45C, 0xBE9B_2CA7, 0xC113_AAC1, 0xC205_CD8E, 0xC28D_6B28, 0xC29C_B8CA,
    0xC44B_EEFD, 0xC472_7A58, 0xC5BB_001A, 0xC67B_B828, 0xC6DC_6B1B, 0xC6F2_B995, 0xC77B_97FA, 0xC87E_4E19,
    0xC881_51C6, 0xC984_7AD0, 0xC98E_5D53, 0xCBDB_7A1D, 0xCBE5_6258, 0xCC23_4CDC, 0xCCC9_06AD, 0xCE45_81B0,
    0xCFAA_F998, 0xD0CC_4C8E, 0xD107_09A9, 0xD1FF_2AD9, 0xD2E9_8367, 0xD312_E3BC, 0xD825_6DCA, 0xD91E_AF87,
    0xD945_7847, 0xD99D_69C9, 0xDA7A_2F61, 0xDACA_9352, 0xDB73_1F7E, 0xDB91_4A57, 0xDBA9_AD29, 0xDDB6_01B1,
    0xDFBA_467C, 0xE084_C48A, 0xE0EF_D6D6, 0xE128_7EB7, 0xE12D_2050, 0xE18D_6D7E, 0xE271_2561, 0xE32D_D777,
    0xE36B_4631, 0xE388_8734, 0xE40B_9F62, 0xE467_E3E5, 0xE587_97C5, 0xE5D5_10D8, 0xE810_4BF1, 0xE8AA_7210,
    0xE8C9_0E8D, 0xE90F_17D4, 0xE979_C660, 0xEA05_46E8, 0xEA67_7C41, 0xEA88_B1FB, 0xEB2E_2137, 0xEBBC_159D,
    0xEC33_A9B3, 0xEC48_3E3F, 0xECB8_826E, 0xECCB_2207, 0xEDD5_7EC3, 0xEE97_6688, 0xEEC3_EA85, 0xEF1D_3325,
    0xEF54_4942, 0xF034_6812, 0xF129_A4FA, 0xF23B_9836, 0xF425_F957, 0xF7F1_5237, 0xF88C_4BFB, 0xF99A_8E4F,
    0xFA7E_BB45, 0xFA82_AEE1, 0xFAED_86A8, 0xFC63_9562, 0xFE9E_7626, 0xFF9C_12D5,
];

/// The cues that resolve only with `English.wad`'s wavebanks and reach a looping wave too.
const LOOPING_WAVE_ENGLISH: [u32; 5] = [0x2417_22F2, 0x904F_C40D, 0xA156_2A38, 0xDF09_1314, 0xDF37_C1E2];

/// The cues whose waves carry the kind-9 filter (their first event is kind 9).
const FILTERED: [u32; 2] = [0xD8CE_1427, 0xF23B_9836];

#[derive(Default)]
struct Tally {
    total: usize,
    resolved: usize,
    played: usize,
    streamed: BTreeMap<u32, usize>,
    absent: BTreeMap<u32, Vec<u32>>,
    /// Played cues that can reach a looping wave (a group `+0x2C` count), sorted.
    looping: Vec<u32>,
    /// Played cues whose decoded first events hold a kind-9 record, sorted.
    filtered: Vec<u32>,
    /// Played cues whose voices carried a filter in the mixer and mixed audible samples, sorted.
    filtered_audible: Vec<u32>,
}

fn tally(eng: &mut AudioEngine, tables: &[Table], label: &str) -> Tally {
    let mut t = Tally::default();
    let (mut multi_track, mut multi_wave, mut fired, mut played) = (0, 0, 0, 0);
    let (mut looped, mut with_children) = (0, 0);
    for table in of_type(tables, TYPE_HASH_SOUNDDB) {
        let db = SoundDb::parse(&table.body).expect("sounddb");
        for e in &db.cues {
            t.total += 1;
            match eng.resolve_cue(e) {
                Ok(r) => {
                    t.resolved += 1;
                    if r.sounds.iter().any(|s| s.selection.is_some()) {
                        multi_track += 1;
                    }
                    if r.sounds.iter().any(|s| s.choices.iter().any(|c| c.selection.is_some())) {
                        multi_wave += 1;
                    }
                    let loops = r.multitrack.as_ref().is_some_and(|m| m.byte_10 != 0 || m.tracks.iter().any(|t| t.byte_00 != 0));
                    let wave_loops = r.sounds.iter().any(|s| s.choices.iter().any(|c| c.loop_byte != 0));
                    match play_once(eng, e, &r) {
                        Ok(Played { fired: n, children, filtered_audible }) => {
                            fired += n;
                            played += 1;
                            looped += usize::from(loops);
                            with_children += usize::from(children);
                            if wave_loops {
                                t.looping.push(e.guid);
                            }
                            let filters = r.multitrack.as_ref().is_some_and(|m| {
                                m.events.iter().take(m.curves.len()).any(|a| {
                                    matches!(a, mercs2_audio::multitrack::Automation::Kind9 { .. })
                                })
                            });
                            if filters {
                                t.filtered.push(e.guid);
                            }
                            if filtered_audible {
                                t.filtered_audible.push(e.guid);
                            }
                        }
                        Err(err) => panic!("cue 0x{:08X} refused at start: {err}", e.guid),
                    }
                }
                Err(ResolveError::Streamed { clip_hash }) => {
                    let _ = clip_hash;
                    *t.streamed.entry(e.bank_hash).or_default() += 1
                }
                Err(ResolveError::WavebankNotResident(h)) => t.absent.entry(h).or_default().push(e.guid),
                Err(other) => panic!("cue 0x{:08X}: structural resolve failure: {other}", e.guid),
            }
        }
    }
    t.played = played;
    t.looping.sort_unstable();
    t.filtered.sort_unstable();
    t.filtered_audible.sort_unstable();
    println!(
        "[{label}] {} retail cues: {} resolve ({multi_track} multi-track, {multi_wave} reaching a \
         multi-wave group); {played} played for 2 s each ({looped} with a track or cue loop, {} \
         reaching a looping wave, {} filtered, {with_children} starting a child cue), firing {fired} sounds",
        t.total,
        t.resolved,
        t.looping.len(),
        t.filtered.len()
    );
    for (bank, n) in &t.streamed {
        println!("  not resolved, a wave streams from a .pws: {n} cues of soundbank 0x{bank:08X}");
    }
    for (wb, cues) in &t.absent {
        println!("  not resolved, wavebank 0x{wb:08X} is not resident: cues {cues:08X?}");
    }
    t
}

/// What one cue's run showed.
struct Played {
    /// The most sounds it held at once.
    fired: usize,
    /// Whether it started a child cue.
    children: bool,
    /// Whether one of its voices carried a filter while the mix it rendered was audible.
    filtered_audible: bool,
}

/// Start a resolved cue with every cue-local curve parameter at its curve's first point, run it for
/// two seconds of 1/60 s frames, stop it (a stop starts tail cues), run another second, and return
/// what it showed ([`Played`]). A refusal at start is returned to the caller.
fn play_once(
    eng: &mut AudioEngine,
    e: &mercs2_audio::CueEntry,
    r: &mercs2_audio::ResolvedCue,
) -> Result<Played, mercs2_audio::CueError> {
    use mercs2_audio::multitrack::Automation;
    let params: Vec<(u32, f32)> = r
        .multitrack
        .iter()
        .flat_map(|m| {
            m.params.iter().filter_map(move |&p| {
                m.events
                    .iter()
                    .chain(m.curves.iter())
                    .chain(m.tracks.iter().flat_map(|t| t.automation.iter()))
                    .find_map(|a| match a {
                    Automation::Curve { param, points, .. } if *param == p => points.first().map(|pt| (p, pt.0)),
                    _ => None,
                })
            })
        })
        .collect();
    eng.stop_and_flush_all_sounds();
    eng.pool = mercs2_audio::VoicePool::new(64);
    let h = eng.cue_sound_with_params(e.guid, None, &params)?;
    let (mut fired, mut children, mut filtered_audible) = (0, false, false);
    for _ in 0..120 {
        eng.tick(1.0 / 60.0);
        let filtering = eng.cue_voices(h).iter().any(|&v| eng.mixer.filter(v).is_some());
        let block = eng.render(735);
        filtered_audible |= filtering && mercs2_audio::mixer::rms_i16(&block) > 0.0;
        fired = fired.max(eng.cue_instances(h).len());
        children |= !eng.cue_children(h).is_empty();
    }
    eng.stop_sound(h);
    children |= !eng.cue_children(h).is_empty();
    for _ in 0..60 {
        eng.tick(1.0 / 60.0);
        eng.render(735);
    }
    eng.stop_and_flush_all_sounds();
    Ok(Played { fired, children, filtered_audible })
}

/// Retail `sound_resident` cue 20 (guid 0xF2937330): two tracks, each a volume ramp (the second a
/// fade from 1 to 0 over 0.579 s). Three 0.1 s frames in, each voice's gain is its instance's base
/// volume × the track ramp's value × clamp01(cue gain), with the ramp read from the decoded cue.
#[test]
fn a_retail_fade_reaches_the_voices() {
    use mercs2_audio::multitrack::{Automation, Target};
    let Some(tables) = retail_tables() else { return };
    let mut eng = AudioEngine::default();
    eng.set_rng_seed(3);
    for t in of_type(tables, TYPE_HASH_WAVEBANK) {
        eng.load_wavebank(&t.body).expect("wavebank loads");
    }
    let mut cue = None;
    for t in of_type(tables, TYPE_HASH_SOUNDBANK) {
        eng.load_soundbank(&t.body).expect("soundbank loads");
        let sb = Soundbank::parse(&t.body).expect("soundbank");
        if sb.bank_hash == 0xDCCF_8AFA {
            cue = Some(sb.cues[20].clone());
        }
    }
    for t in of_type(tables, TYPE_HASH_SOUNDDB) {
        let db = SoundDb::parse(&t.body).expect("sounddb");
        if db.self_hash == 0xDCCF_8AFA {
            eng.set_sounddb(db);
        }
    }
    let cue = cue.expect("sound_resident cue 20");
    assert_eq!(cue.guid, 0xF293_7330);
    let CueBody::MultiTrack(m) = &cue.body else { panic!("multi-track") };

    let h = eng.cue_sound(cue.guid, None).expect("the cue plays");
    for _ in 0..3 {
        eng.tick(0.1);
    }
    let t = 0.1f32 + 0.1 + 0.1;
    let cue_volume = if cue.gain > 1.0 { 1.0 } else { cue.gain.max(0.0) };
    let instances = eng.cue_instances(h);
    assert_eq!(instances.len(), 2, "one sound per track");
    for (track, inst) in m.tracks.iter().zip(&instances) {
        let Automation::Ramp { target: Target::Volume, start_s, duration_s, from, to, mode: 0, .. } = track.automation[0] else {
            panic!("the track's first record is a plain volume ramp")
        };
        assert!(t > start_s, "active");
        let ramp = (to - from) / ((duration_s + start_s) - start_s) * (t - start_s) + from;
        let ramp = ramp.clamp(0.0, 1.0);
        let gain = eng.pool.get(inst.voice.expect("a voice")).expect("voice").gain;
        println!("track ramp {from}->{to} over {duration_s}s: t={t} factor {ramp}, voice gain {gain}");
        assert_eq!(gain, inst.volume * (ramp * cue_volume));
    }
}

/// Whether a sound instance is positional comes from its group's `+0x14` byte (`FUN_00837830`,
/// `0x008378A9`), not from the sounddb record, which has no flag field. On retail `vz.wad`: every
/// group's `+0x14` word is 0 or 1; a cue started at a position gives instances of a `+0x14` = 1
/// group a positional voice and instances of a `+0x14` = 0 group a 2D one.
#[test]
fn group_plus_0x14_decides_whether_an_instance_is_positional() {
    let Some(tables) = retail_tables() else { return };
    let mut eng = AudioEngine::default();
    eng.set_rng_seed(9);
    let (mut set, mut clear) = (0, 0);
    for t in of_type(tables, TYPE_HASH_WAVEBANK) {
        eng.load_wavebank(&t.body).expect("wavebank loads");
    }
    for t in of_type(tables, TYPE_HASH_SOUNDBANK) {
        eng.load_soundbank(&t.body).expect("soundbank loads");
        for g in &Soundbank::parse(&t.body).expect("soundbank").groups {
            match g.head.unknown_14 {
                0 => clear += 1,
                1 => set += 1,
                other => panic!("group +0x14 = {other}"),
            }
        }
    }
    let mut catalog = SoundDb::default();
    for t in of_type(tables, TYPE_HASH_SOUNDDB) {
        catalog.merge(&SoundDb::parse(&t.body).expect("sounddb"));
    }
    eng.set_sounddb(catalog.clone());
    println!("group +0x14: {set} groups set, {clear} clear");
    assert!(set > 0 && clear > 0);
    let single = |want: bool| {
        catalog.cues.iter().find(|c| {
            eng.resolve_cue(c).is_ok_and(|r| {
                r.multitrack.is_none() && r.sounds[0].choices[0].positional == want
            })
        })
    };
    let (Some(pos_cue), Some(flat_cue)) = (single(true).copied(), single(false).copied()) else {
        panic!("retail has single-track cues on both kinds of group")
    };
    let at = Some(mercs2_core::glam::Vec3::new(3.0, 0.0, 0.0));
    for (cue, want) in [(pos_cue, true), (flat_cue, false)] {
        let h = eng.cue_sound(cue.guid, at).expect("the cue plays");
        eng.tick(0.02);
        assert_eq!(eng.cue_instances(h)[0].positional, want, "cue 0x{:08X}", cue.guid);
        let h2 = eng.cue_sound(cue.guid, None).expect("the cue plays");
        eng.tick(0.02);
        assert!(!eng.cue_instances(h2)[0].positional, "no position, no emitter: 2D");
    }
}
