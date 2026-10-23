//! Which banks and cues play each voice-over line — the routing the Audio page lists clips by, and
//! what its "Add to Shipment" routes are seeded from.
//!
//! A line is a record of the `vo_stream.english` wavebank; a cue plays it when the cue's group
//! (single-track) or a sound entry of one of its tracks (multi-track) names
//! `{m2("vo_stream"), record}`. The tables are `English.wad`'s, read through
//! [`mercs2_quartermaster::sound::wavebank_players`] — the same read a `replace_sound_wave` build
//! resolves its line with. The inventory's own `bank` label is a bucket, not this routing (it files
//! 9,135 lines under `vo_misc`, whose soundbank plays 160), so it only names the banks: a label `L`
//! names the soundbank entry `m2("L.english")`.

use mercs2_audio::encode::{cue_groups, RETAIL_CATEGORY_NAMES};
use mercs2_audio::soundbank::{CueBody, Group, GroupForm, Soundbank};
use mercs2_audio::wave::WavebankFile;
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_quartermaster::manifest::{Contribution, Language, SoundCue};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The wavebank the lines are records of, as the groups name it.
pub const VO_WAVEBANK: &str = "vo_stream";
/// The language whose copy the page reads (the `.pws` it plays is `vo_stream.english.pws`).
pub const LANGUAGE: Language = Language::English;

/// One cue that plays a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    /// The soundbank's entry hash.
    pub entry: u32,
    /// The cue's index in the soundbank.
    pub cue: usize,
    /// The cue's guid.
    pub guid: u32,
    /// The group of the soundbank that plays the line (the first, when several do).
    pub group: usize,
    /// Whether the cue is multi-track.
    pub multi_track: bool,
    /// How many distinct waves the cue can play: what replacing the cue with one wave replaces.
    pub waves: usize,
}

/// One soundbank that plays lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Bank {
    /// The name the inventory labels it with, or its entry hash as `0xHHHHHHHH`.
    pub name: String,
    /// The soundbank's entry hash (`m2("<name>.english")` for a named one).
    pub entry: u32,
    /// The records it plays, ascending.
    pub lines: Vec<u32>,
}

/// The routing of every line.
#[derive(Debug, Clone)]
pub struct Routes {
    /// The wavebank: each record's clip hash and length.
    pub wavebank: WavebankFile,
    /// Every soundbank read, by entry hash.
    pub soundbanks: BTreeMap<u32, Soundbank>,
    /// The cues that play each record.
    pub plays: BTreeMap<u32, Vec<Play>>,
    /// The banks that play at least one line, by name.
    pub banks: Vec<Bank>,
    /// The records no cue plays.
    pub unplayed: Vec<u32>,
}

impl Routes {
    /// Read `English.wad` beside `vz` and route every line; `labels` are the inventory's bank labels.
    pub fn build(vz: &Path, labels: &[String]) -> Result<Routes, String> {
        let english = mercs2_quartermaster::sound::sibling_wad(vz, &format!("{}.wad", LANGUAGE.token()))?;
        let mut game = mercs2_quartermaster::GameStack::open(&[vz.to_path_buf(), english]).map_err(|e| e.to_string())?;
        let players = mercs2_quartermaster::sound::wavebank_players(&mut game, &mut None, VO_WAVEBANK, Some(LANGUAGE))?;
        let tables = players
            .carriers
            .into_iter()
            .next()
            .ok_or_else(|| format!("internal error: {} was read from no carrier", players.entry))?;
        Ok(Routes::from_tables(tables.wavebank, tables.soundbanks, labels))
    }

    /// Route every record of `wavebank` through `soundbanks` (`(entry hash, table)`).
    pub fn from_tables(wavebank: WavebankFile, soundbanks: Vec<(u32, Soundbank)>, labels: &[String]) -> Routes {
        let names: BTreeMap<u32, &str> =
            labels.iter().map(|l| (m2(&format!("{l}.{}", LANGUAGE.token())), l.as_str())).collect();
        let vo = wavebank.bank_hash;
        let mut plays: BTreeMap<u32, Vec<Play>> = BTreeMap::new();
        for (entry, sb) in &soundbanks {
            for (ci, cue) in sb.cues.iter().enumerate() {
                let groups: Vec<usize> = cue_groups(&cue.body)
                    .into_iter()
                    .filter(|(b, _)| *b == sb.bank_hash)
                    .map(|(_, g)| usize::from(g))
                    .collect();
                let reached: Vec<(usize, u32, u32)> = groups
                    .iter()
                    .filter_map(|g| sb.groups.get(*g).map(|grp| (g, grp)))
                    .flat_map(|(g, grp)| grp.waves().iter().map(move |w| (*g, w.wavebank, w.index)))
                    .collect();
                let waves = reached.iter().map(|(_, b, i)| (*b, *i)).collect::<BTreeSet<_>>().len();
                let mut seen = BTreeSet::new();
                for (g, b, i) in reached {
                    if b != vo || !seen.insert(i) {
                        continue;
                    }
                    plays.entry(i).or_default().push(Play {
                        entry: *entry,
                        cue: ci,
                        guid: cue.guid,
                        group: g,
                        multi_track: matches!(cue.body, CueBody::MultiTrack(_)),
                        waves,
                    });
                }
            }
        }
        let mut by_bank: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for (line, ps) in &plays {
            for p in ps {
                by_bank.entry(p.entry).or_default().insert(*line);
            }
        }
        let mut banks: Vec<Bank> = by_bank
            .into_iter()
            .map(|(entry, lines)| Bank {
                name: names.get(&entry).map(|n| n.to_string()).unwrap_or_else(|| format!("0x{entry:08X}")),
                entry,
                lines: lines.into_iter().collect(),
            })
            .collect();
        banks.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let unplayed = (0..wavebank.records.len() as u32).filter(|i| !plays.contains_key(i)).collect();
        Routes { wavebank, soundbanks: soundbanks.into_iter().collect(), plays, banks, unplayed }
    }

    /// The name of the bank with entry `entry`.
    pub fn bank_name(&self, entry: u32) -> String {
        self.banks.iter().find(|b| b.entry == entry).map(|b| b.name.clone()).unwrap_or_else(|| format!("0x{entry:08X}"))
    }

    /// The record's clip hash.
    pub fn clip_hash(&self, line: u32) -> Option<u32> {
        self.wavebank.records.get(line as usize).map(|r| r.clip_hash)
    }

    /// `replace_sound_wave` of record `line`: the line as its `name` when the name's hash is the
    /// record's clip hash, else as the clip hash; the WAV to write is `src/<stem>.wav`.
    pub fn line_seed(&self, line: u32, name: Option<&str>, stem: &str) -> Option<Contribution> {
        let clip = self.clip_hash(line)?;
        Some(Contribution::ReplaceSoundWave {
            wavebank: VO_WAVEBANK.to_string(),
            language: Some(LANGUAGE),
            line: reference(clip, name),
            wave: PathBuf::from(format!("src/{stem}.wav")),
        })
    }

    /// `replace_sound_cue` of the cue `play` is, playing record `line`: the bank by name, the cue
    /// as `name` when its hash is the guid, else as the guid, and every value the retail cue and the
    /// group that plays the line carry ([`retail_cue`]).
    pub fn cue_seed(&self, play: &Play, line: u32, name: Option<&str>, stem: &str) -> Option<Contribution> {
        let sb = self.soundbanks.get(&play.entry)?;
        let cue = sb.cues.get(play.cue)?;
        let group = sb.groups.get(play.group)?;
        let mut seeded = retail_cue(cue, group, self.clip_hash(line)?);
        seeded.name = reference(play.guid, name);
        seeded.wave = PathBuf::from(format!("src/{stem}.wav"));
        Some(Contribution::ReplaceSoundCue {
            bank: self.bank_name(play.entry),
            language: Some(LANGUAGE),
            category: category_name(group.head.category),
            cue: seeded,
        })
    }

    /// `replace_sound_bank` of the bank `entry`: its name and the category its groups share (empty,
    /// for the author, when they carry more than one); its cues are the author's.
    pub fn bank_seed(&self, entry: u32) -> Option<Contribution> {
        let sb = self.soundbanks.get(&entry)?;
        let cats: BTreeSet<u32> = sb.groups.iter().map(|g| g.head.category).collect();
        let category = match cats.iter().collect::<Vec<_>>().as_slice() {
            [one] => category_name(**one),
            _ => String::new(),
        };
        Some(Contribution::ReplaceSoundBank {
            bank: self.bank_name(entry),
            language: Some(LANGUAGE),
            category,
            cues: Vec::new(),
        })
    }
}

/// `name` when it hashes to `hash`, else `hash` as `0xHHHHHHHH`.
fn reference(hash: u32, name: Option<&str>) -> String {
    match name {
        Some(n) if m2(n) == hash => n.to_string(),
        _ => format!("0x{hash:08X}"),
    }
}

/// The category tree's name for `hash`, or empty when it is one of the unnamed categories.
fn category_name(hash: u32) -> String {
    RETAIL_CATEGORY_NAMES.iter().find(|n| m2(n) == hash).map(|n| n.to_string()).unwrap_or_default()
}

/// A linear gain as the dB the manifest writes (`sound::db_to_gain` is its inverse).
fn gain_db(gain: f32) -> f64 {
    20.0 * f64::from(gain).log10()
}

/// The [`SoundCue`] fields `cue` and `group` carry, with `clip_hash` for the new record; `name` and
/// `wave` empty. A single-wave group's base volume and pitch are values; a multi-wave group's are
/// ranges the engine draws in (`floats_4c` `[1]`–`[2]`, `[4]`–`[5]`), which a one-wave cue carries
/// only when the range is one value — otherwise 0 dB and 0 semitones, for the author to set. The
/// cue's `+0x16` is a single-track cue's only.
pub fn retail_cue(cue: &mercs2_audio::soundbank::Cue, group: &Group, clip_hash: u32) -> SoundCue {
    let (gain, pitch) = match &group.form {
        GroupForm::Single { gain, unknown_30, .. } => (gain_db(*gain), *unknown_30),
        GroupForm::Multi(m) => {
            let f = m.floats_4c;
            (if f[1] == f[2] { gain_db(f[1]) } else { 0.0 }, if f[4] == f[5] { f[4] } else { 0.0 })
        }
    };
    let h = &group.head;
    SoundCue {
        name: String::new(),
        wave: PathBuf::new(),
        group_gain_db: gain,
        cue_gain_db: gain_db(cue.gain),
        pitch_semitones: pitch,
        positional: h.unknown_14 != 0,
        min_distance: h.min_distance,
        max_distance: h.max_distance,
        distance_exponent: h.distance_exponent,
        doppler_scale: h.doppler_scale,
        start_limit: cue.byte_06,
        sound_id: h.sound_id,
        priority: h.unknown_10,
        group_20: h.unknown_20,
        cue_16: match cue.body {
            CueBody::SingleTrack { unknown_16, .. } => unknown_16,
            CueBody::MultiTrack(_) => 0,
        },
        clip_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_audio::encode::{
        build_general, CueBodySpec, CueDef, GroupFormSpec, GroupHeadParams, GroupSpec, MultiGroupParams, Pcm16, TablesSpec,
        WaveSpec,
    };
    use mercs2_audio::wave::{WaveData, WaveRecord};

    /// A streamed `vo_stream` wavebank of `n` records, clip hash `0x100 + i`.
    fn vo(n: u32) -> WavebankFile {
        WavebankFile {
            bank_hash: m2(VO_WAVEBANK),
            stream_name: Some("vo_stream.english.pws".into()),
            records: (0..n)
                .map(|i| WaveRecord {
                    clip_hash: 0x100 + i,
                    channels: 1,
                    format: 4,
                    sample_rate: 22050,
                    frames: 44100,
                    data: WaveData::Streamed { offset: 0, size: 0, word_1c: 0 },
                })
                .collect(),
        }
    }

    fn head() -> GroupHeadParams {
        GroupHeadParams {
            unknown_10: 0.75,
            unknown_14: 1,
            min_distance: 3.0,
            max_distance: 40.0,
            unknown_20: 1.0,
            distance_exponent: 2.0,
            doppler_scale: 0.5,
        }
    }

    /// A soundbank `name` whose groups play the given vo_stream records: a cue per group.
    fn bank(name: &str, groups: &[&[u32]]) -> Soundbank {
        let hash = m2(name);
        let mut tables = build_general(&TablesSpec {
            name: name.to_string(),
            hash,
            waves: vec![WaveSpec { clip_hash: 1, pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![1; 8] } }],
            groups: groups
                .iter()
                .map(|ws| GroupSpec {
                    sound_id: 7,
                    category: "vo".into(),
                    head: head(),
                    form: if ws.len() == 1 {
                        GroupFormSpec::Single { wave: 0, gain: 0.5, unknown_30: 1.5, weight: 1.0 }
                    } else {
                        GroupFormSpec::Multi {
                            params: MultiGroupParams {
                                byte_2c: 0,
                                selection: 1,
                                byte_2f: 0,
                                unknown_30: 0.0,
                                unknown_34: 0.0,
                                unknown_3c: 0.0,
                                unknown_40: 0.0,
                                word_48: 0,
                                floats_4c: [0.0, 0.25, 0.25, 0.0, -1.0, 1.0],
                                unknown_64: 0.0,
                            },
                            waves: ws.iter().map(|_| (0, 1.0 / ws.len() as f32)).collect(),
                        }
                    },
                })
                .collect(),
            cues: (0..groups.len())
                .map(|g| CueDef {
                    name: format!("{name}_{g}"),
                    guid: m2(&format!("{name}_{g}")),
                    byte_06: 2,
                    gain: 0.5,
                    body: CueBodySpec::SingleTrack { group: g, unknown_16: 9 },
                })
                .collect(),
        })
        .expect("builds");
        // Point the groups at vo_stream records, as the vo banks' groups are.
        for (g, ws) in groups.iter().enumerate() {
            let refs: &mut [mercs2_audio::soundbank::WaveRef] = match &mut tables.soundbank.groups[g].form {
                GroupForm::Single { wave, .. } => std::slice::from_mut(wave),
                GroupForm::Multi(m) => &mut m.waves,
            };
            for (r, i) in refs.iter_mut().zip(ws.iter()) {
                (r.wavebank, r.index) = (m2(VO_WAVEBANK), *i);
            }
        }
        tables.soundbank
    }

    /// Lines are listed under every bank whose cues play them (by the label naming `<label>.english`,
    /// or the entry hash when no label does), a cue that picks among several takes counts them, and
    /// a record no cue plays is unplayed.
    #[test]
    fn lines_route_to_the_banks_that_play_them() {
        let chris = m2("vo_Chris.english");
        let other = m2("vo_unknown.english");
        let r = Routes::from_tables(
            vo(5),
            vec![(chris, bank("vo_Chris", &[&[0], &[1, 2]])), (other, bank("vo_unknown", &[&[2]]))],
            &["vo_Chris".to_string(), "vo_misc".to_string()],
        );
        let names: Vec<(String, Vec<u32>)> = r.banks.iter().map(|b| (b.name.clone(), b.lines.clone())).collect();
        assert_eq!(names, vec![(format!("0x{other:08X}"), vec![2]), ("vo_Chris".to_string(), vec![0, 1, 2])]);
        assert_eq!(r.unplayed, vec![3, 4]);
        let two = &r.plays[&2];
        assert_eq!(two.len(), 2, "record 2 is played by a cue of each bank");
        let take = two.iter().find(|p| p.entry == chris).expect("vo_Chris plays it");
        assert_eq!((take.cue, take.group, take.waves, take.multi_track), (1, 1, 2, false));
    }

    /// The seeds: a line by its name when the name is the record's clip hash's, else by the hash; a
    /// cue by name or guid with every retail value (a multi-wave group's volume range of one value
    /// carried, its pitch range of two left at 0); a bank by name with the category its groups share.
    #[test]
    fn the_seeds_carry_the_retail_values() {
        let chris = m2("vo_Chris.english");
        let r = Routes::from_tables(vo(3), vec![(chris, bank("vo_Chris", &[&[0], &[1, 2]]))], &["vo_Chris".to_string()]);
        let Some(Contribution::ReplaceSoundWave { wavebank, language, line, wave }) = r.line_seed(1, Some("nope"), "00001__x") else {
            panic!("a line seed")
        };
        assert_eq!((wavebank.as_str(), language, line.as_str()), ("vo_stream", Some(Language::English), "0x00000101"));
        assert_eq!(wave, PathBuf::from("src/00001__x.wav"));

        let play = r.plays[&0][0].clone();
        let Some(Contribution::ReplaceSoundCue { bank: b, language, category, cue }) = r.cue_seed(&play, 0, Some("vo_Chris_0"), "s") else {
            panic!("a cue seed")
        };
        assert_eq!((b.as_str(), language, category.as_str(), cue.name.as_str()), ("vo_Chris", Some(Language::English), "vo", "vo_Chris_0"));
        let back = |db: f64| mercs2_quartermaster::sound::db_to_gain(db);
        assert_eq!(back(cue.group_gain_db), 0.5);
        assert_eq!(back(cue.cue_gain_db), 0.5);
        assert_eq!(cue.pitch_semitones, 1.5);
        assert!(cue.positional);
        assert_eq!(
            (cue.min_distance, cue.max_distance, cue.distance_exponent, cue.doppler_scale, cue.priority, cue.group_20),
            (3.0, 40.0, 2.0, 0.5, 0.75, 1.0)
        );
        assert_eq!((cue.start_limit, cue.sound_id, cue.cue_16, cue.clip_hash), (2, 7, 9, 0x100));

        let variants = r.plays[&1][0].clone();
        let Some(Contribution::ReplaceSoundCue { cue, .. }) = r.cue_seed(&variants, 1, None, "s") else { panic!("a cue seed") };
        assert_eq!(cue.name, format!("0x{:08X}", m2("vo_Chris_1")));
        assert_eq!(back(cue.group_gain_db), 0.25, "a volume range of one value");
        assert_eq!(cue.pitch_semitones, 0.0, "a pitch range of two values is the author's");

        let Some(Contribution::ReplaceSoundBank { bank: b, category, cues, .. }) = r.bank_seed(chris) else { panic!("a bank seed") };
        assert_eq!((b.as_str(), category.as_str(), cues.len()), ("vo_Chris", "vo", 0));
    }
}

/// Against `English.wad`: every line routed through all its soundbanks — 12,954 played, 34 no cue
/// plays, 46 banks (the 27 labelled vo banks, vo_Jen playing 586 lines and vo_misc 160, and 19
/// per-scene banks known by hash) — and a seeded cue of a single-wave group whose every value
/// re-encodes to the retail bits.
#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;

    fn routes() -> Routes {
        let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
        let home = crate::index::require_data_home(&mut |_, _| {}).unwrap_or_else(|e| panic!("{e}"));
        let idx = crate::audioidx::AudioIndex::load(&home).unwrap_or_else(|e| panic!("{e}"));
        let labels: Vec<String> = idx.banks().into_iter().map(|(b, _)| b).collect();
        Routes::build(&vz, &labels).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn english_wad_routes_every_line() {
        let r = routes();
        assert_eq!(r.wavebank.records.len(), 12_988);
        assert_eq!(r.plays.len(), 12_954);
        assert_eq!(r.unplayed.len(), 34);
        let lines = |n: &str| r.banks.iter().find(|b| b.name == n).unwrap_or_else(|| panic!("no bank {n}")).lines.len();
        assert_eq!(lines("vo_Jen"), 586);
        assert_eq!(lines("vo_misc"), 160);
        // The 27 labelled vo banks, and 19 of English.wad's per-scene banks known only by hash.
        assert_eq!(r.banks.len(), 46);
        assert_eq!(r.banks.iter().filter(|b| b.name.starts_with("vo_")).count(), 27);

        // A single-track cue on a single-wave group: its seed re-encodes to the retail bits.
        let (line, play) = r
            .plays
            .iter()
            .flat_map(|(l, ps)| ps.iter().map(move |p| (*l, p)))
            .find(|(_, p)| {
                let sb = &r.soundbanks[&p.entry];
                !p.multi_track && matches!(sb.groups[p.group].form, GroupForm::Single { .. })
            })
            .expect("a single-wave line");
        let Some(Contribution::ReplaceSoundCue { cue, category, .. }) = r.cue_seed(play, line, None, "s") else { panic!("seed") };
        let sb = &r.soundbanks[&play.entry];
        let (retail, group) = (&sb.cues[play.cue], &sb.groups[play.group]);
        let GroupForm::Single { gain, unknown_30, .. } = group.form else { unreachable!() };
        let back = mercs2_quartermaster::sound::db_to_gain;
        assert_eq!(back(cue.group_gain_db).to_bits(), gain.to_bits());
        assert_eq!(back(cue.cue_gain_db).to_bits(), retail.gain.to_bits());
        assert_eq!(cue.pitch_semitones.to_bits(), unknown_30.to_bits());
        assert_eq!(cue.clip_hash, r.clip_hash(line).unwrap());
        assert_eq!(m2(&category), group.head.category);
    }
}
