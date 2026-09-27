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
}

#[derive(Default)]
struct Tally {
    total: usize,
    resolved: usize,
    streamed: BTreeMap<u32, usize>,
    absent: BTreeMap<u32, Vec<u32>>,
}

fn tally(eng: &mut AudioEngine, tables: &[Table], label: &str) -> Tally {
    let mut t = Tally::default();
    let (mut multi_track, mut multi_wave, mut fired, mut played) = (0, 0, 0, 0);
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
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
                    match play_once(eng, e, &r) {
                        Ok(n) => {
                            fired += n;
                            played += 1;
                        }
                        Err(reason) => *refused.entry(reason).or_default() += 1,
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
    println!(
        "[{label}] {} retail cues: {} resolve ({multi_track} multi-track, {multi_wave} reaching a \
         multi-wave group); {played} played for 2 s each, firing {fired} sounds",
        t.total, t.resolved
    );
    for (reason, n) in &refused {
        println!("  refused at start: {n} cues — {reason}");
    }
    for (bank, n) in &t.streamed {
        println!("  not resolved, a wave streams from a .pws: {n} cues of soundbank 0x{bank:08X}");
    }
    for (wb, cues) in &t.absent {
        println!("  not resolved, wavebank 0x{wb:08X} is not resident: cues {cues:08X?}");
    }
    t
}

/// Start a resolved cue with every cue-local curve parameter at its curve's first point, run it for
/// two seconds of 1/60 s frames, and return how many sounds fired. A cue the engine cannot play here
/// (looping, or automation with no counterpart) is refused at start; anything else failing panics.
fn play_once(eng: &mut AudioEngine, e: &mercs2_audio::CueEntry, r: &mercs2_audio::ResolvedCue) -> Result<usize, String> {
    use mercs2_audio::multitrack::Automation;
    let params: Vec<(u32, f32)> = r
        .multitrack
        .iter()
        .flat_map(|m| {
            m.params.iter().filter_map(move |&p| {
                m.events.iter().chain(m.tracks.iter().flat_map(|t| t.automation.iter())).find_map(|a| match a {
                    Automation::Curve { param, points, .. } if *param == p => points.first().map(|pt| (p, pt.0)),
                    _ => None,
                })
            })
        })
        .collect();
    eng.pool = mercs2_audio::VoicePool::new(64);
    match eng.cue_sound_with_params(e.guid, None, &params) {
        Ok(h) => {
            let mut fired = 0;
            for _ in 0..120 {
                eng.tick(1.0 / 60.0);
                fired = fired.max(eng.cue_instances(h).len());
            }
            eng.stop_sound(h);
            Ok(fired)
        }
        Err(err @ mercs2_audio::CueError::Looping { .. }) | Err(err @ mercs2_audio::CueError::Automation(_)) => {
            Err(err.to_string())
        }
        Err(other) => panic!("cue 0x{:08X}: {other}", e.guid),
    }
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
        let gain = eng.pool.get(inst.voice).expect("voice").gain;
        println!("track ramp {from}->{to} over {duration_s}s: t={t} factor {ramp}, voice gain {gain}");
        assert_eq!(gain, inst.volume * (ramp * cue_volume));
    }
}
