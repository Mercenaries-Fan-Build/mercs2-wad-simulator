//! The sound editors' preview: each cue as the game ships it beside the cue as `qm build` makes it,
//! every wave either side can play drawn as a waveform and playable.
//!
//! "Yours" is read back from the blocks qm's own sound build emits for the item
//! (`sound::lower_shipment_sound`): the soundbank, sounddb and wavebank tables that ship, not the
//! authored WAV. "Shipped" is the game's copy of the bank in each WAD that carries it
//! (`sound::carried_banks`), its waves in the wavebanks of that WAD, a streamed one read from its
//! `.pws` in the game's `Audios` folder. A cue that picks among several waves lists every wave it can
//! reach; the engine's pick among them (§11.8) is not simulated. The waves play as recorded: the
//! cue's gain and pitch are shown as values, not applied.
//!
//! The build runs under the Shipment name [`SHIPMENT`]: a Shipment's override wavebank and its loader
//! lines carry its name, which is the Shipment's the item is copied into.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use mercs2_audio::encode::cue_groups;
use mercs2_audio::soundbank::{CueBody, Soundbank};
use mercs2_audio::wave::{WaveRecord, WavebankFile};
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::patch_wad::PatchBlock;
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK};
use mercs2_quartermaster::game::GameStack;
use mercs2_quartermaster::manifest::{asset_hash, Contribution, Language, LoadSession, SoundCue};
use mercs2_quartermaster::sound::{self as qs, Carrier};
use mercs2_quartermaster::NameTable;

use crate::audioplay::Status;
use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};

/// The Shipment name the item's build runs under.
pub const SHIPMENT: &str = "<shipment>";

/// Min and max of each waveform column.
const PEAKS: usize = 160;
/// A waveform's height.
const WAVE_H: f32 = 28.0;
/// The height a list scrolls past.
const LIST_H: f32 = 420.0;

/// Whether `c` is a sound kind.
pub fn of(c: &Contribution) -> bool {
    matches!(
        c,
        Contribution::AddSound { .. }
            | Contribution::LoadSound { .. }
            | Contribution::ReplaceSoundBank { .. }
            | Contribution::ReplaceSoundCue { .. }
            | Contribution::ReplaceSoundWave { .. }
    )
}

/// The ids the player is given: one per wave read, across every build.
static WAVE_IDS: AtomicU32 = AtomicU32::new(1);

/// A wavebank and where a streamed one's samples are.
#[derive(Clone)]
struct Bank {
    file: WavebankFile,
    /// The `.pws` of a streamed bank: `Err` when it was not found.
    pws: Result<Option<PathBuf>, String>,
}

/// Wavebanks by bank hash, the first that holds one winning.
type Waves<'a> = [&'a BTreeMap<u32, Bank>];

/// One wave a cue can play.
#[derive(Clone)]
pub struct Wave {
    /// The player's id for it.
    pub id: u32,
    /// The `(wavebank bank hash, index)` a group plays it by.
    pub at: (u32, u32),
    /// The record's clip hash, when its wavebank is held.
    pub clip: Option<u32>,
    /// The record and its `.pws`, or why the wave cannot be read.
    pub source: Result<(WaveRecord, Option<PathBuf>), String>,
    pub seconds: f32,
}

impl Wave {
    fn read(at: (u32, u32), banks: &Waves<'_>) -> Wave {
        let found = banks.iter().find_map(|b| b.get(&at.0));
        let source = match found {
            None => Err(format!("no wavebank 0x{:08X} is read beside the bank", at.0)),
            Some(b) => match (b.file.records.get(at.1 as usize), &b.pws) {
                (None, _) => Err(format!("wavebank 0x{:08X} has {} records, and the group plays record {}", at.0, b.file.records.len(), at.1)),
                (Some(_), Err(e)) => Err(e.clone()),
                (Some(r), Ok(pws)) => Ok((r.clone(), pws.clone())),
            },
        };
        Wave::from_source(at, source)
    }

    fn from_source(at: (u32, u32), source: Result<(WaveRecord, Option<PathBuf>), String>) -> Wave {
        let id = WAVE_IDS.fetch_add(1, Ordering::Relaxed);
        let (clip, seconds) = match &source {
            Ok((r, _)) => (Some(r.clip_hash), crate::vostream::record_seconds(r)),
            Err(_) => (None, 0.0),
        };
        Wave { id, at, clip, source, seconds }
    }

    /// Its samples, for the player.
    pub fn pcm(&self) -> Result<crate::vostream::Pcm, String> {
        let (r, pws) = self.source.as_ref().map_err(|e| e.clone())?;
        crate::vostream::record_pcm(r, pws.as_deref())
    }
}

/// Min and max of [`PEAKS`] columns of `samples` (channels folded together).
fn peaks(samples: &[i16]) -> Vec<[i16; 2]> {
    if samples.is_empty() {
        return Vec::new();
    }
    let per = samples.len().div_ceil(PEAKS);
    samples.chunks(per).map(|c| [c.iter().copied().min().unwrap_or(0), c.iter().copied().max().unwrap_or(0)]).collect()
}

/// One cue as one side carries it.
#[derive(Clone)]
pub struct CueSide {
    /// The cue's `+0x0C` length, seconds.
    pub length_s: f32,
    /// What a `SoundCue` would carry for it (`audioroute::retail_cue`), when it plays one group of its
    /// bank and that group plays one wave.
    pub values: Option<SoundCue>,
    /// The groups of its bank it plays.
    pub groups: usize,
    pub multi_track: bool,
    /// Every wave those groups can play, once each.
    pub waves: Vec<Wave>,
    /// Groups of other soundbanks it reaches, `(soundbank hash, group)`, which are not read.
    pub foreign: Vec<(u32, u16)>,
}

fn side(sb: &Soundbank, cue: usize, banks: &Waves<'_>) -> CueSide {
    let c = &sb.cues[cue];
    let mut own: Vec<usize> = Vec::new();
    let mut foreign = Vec::new();
    for (b, g) in cue_groups(&c.body) {
        if b == sb.bank_hash {
            if !own.contains(&usize::from(g)) {
                own.push(usize::from(g));
            }
        } else if !foreign.contains(&(b, g)) {
            foreign.push((b, g));
        }
    }
    let mut seen = BTreeSet::new();
    let mut waves = Vec::new();
    for g in &own {
        let Some(group) = sb.groups.get(*g) else { continue };
        for w in group.waves() {
            if seen.insert((w.wavebank, w.index)) {
                waves.push(Wave::read((w.wavebank, w.index), banks));
            }
        }
    }
    let values = match (own.as_slice(), waves.as_slice()) {
        ([g], [w]) => sb.groups.get(*g).and_then(|group| w.clip.map(|clip| crate::audioroute::retail_cue(c, group, clip))),
        _ => None,
    };
    CueSide { length_s: c.length_s, values, groups: own.len(), multi_track: matches!(c.body, CueBody::MultiTrack(_)), waves, foreign }
}

/// One cue of a bank: the game's and the build's.
#[derive(Clone)]
pub struct CueRow {
    pub guid: u32,
    pub shipped: Option<CueSide>,
    pub yours: Option<CueSide>,
}

/// The rows of every cue of `yours` (in its order), then of `shipped` that `yours` lacks; with `only`,
/// the cues it names.
fn rows(shipped: Option<(&Soundbank, &Waves<'_>)>, yours: Option<(&Soundbank, &Waves<'_>)>, only: Option<&BTreeSet<u32>>) -> Vec<CueRow> {
    let wanted = |g: u32| only.is_none_or(|o| o.contains(&g));
    let find = |sb: &Soundbank, g: u32| sb.cues.iter().position(|c| c.guid == g);
    let mut guids: Vec<u32> = Vec::new();
    for sb in yours.iter().map(|(s, _)| *s).chain(shipped.iter().map(|(s, _)| *s)) {
        for c in &sb.cues {
            if wanted(c.guid) && !guids.contains(&c.guid) {
                guids.push(c.guid);
            }
        }
    }
    guids
        .into_iter()
        .map(|guid| CueRow {
            guid,
            shipped: shipped.and_then(|(sb, w)| find(sb, guid).map(|i| side(sb, i, w))),
            yours: yours.and_then(|(sb, w)| find(sb, guid).map(|i| side(sb, i, w))),
        })
        .collect()
}

/// One bank's rows.
pub struct BankView {
    /// What the rows are: the bank's entry and the WAD.
    pub title: String,
    pub rows: Vec<CueRow>,
    /// The cue the contribution replaces (`replace_sound_cue`).
    pub target: Option<u32>,
    /// Whether the build's side is read: `Err` with why, when the build refused or ships nothing for
    /// this WAD's copy.
    pub built: Result<(), String>,
}

/// A replaced line: the game's record and the build's.
pub struct LineView {
    pub shipped: Wave,
    pub yours: Result<Wave, String>,
}

/// A wavebank's records, each a line.
pub struct LinesView {
    pub title: String,
    pub lines: Vec<Wave>,
}

/// What one contribution shows.
#[derive(Default)]
pub struct View {
    pub banks: Vec<BankView>,
    pub line: Option<LineView>,
    pub lines: Vec<LinesView>,
    /// What could not be read of the game's side.
    pub problems: Vec<String>,
}

/// One block the build ships.
pub struct Ship {
    /// Where: the overlay, the shell patch or a language's patch.
    pub to: String,
    pub path: String,
    /// Each table: its entry and what it holds.
    pub tables: Vec<(u32, String)>,
}

/// What the build ships, and what each session's loader loads.
pub struct Build {
    pub ships: Vec<Ship>,
    pub loaders: Vec<(LoadSession, String)>,
    pub log: Vec<String>,
}

/// What an item's sound contributions show and build.
pub struct ItemSound {
    pub views: BTreeMap<usize, View>,
    pub build: Result<Build, String>,
}

impl ItemSound {
    /// The wave the player knows as `id`.
    pub fn wave(&self, id: u32) -> Option<&Wave> {
        self.views.values().find_map(|v| {
            let in_rows = v.banks.iter().flat_map(|b| &b.rows).flat_map(|r| r.shipped.iter().chain(r.yours.iter())).flat_map(|s| &s.waves);
            let in_line = v.line.iter().flat_map(|l| std::iter::once(&l.shipped).chain(l.yours.as_ref().ok()));
            let in_lines = v.lines.iter().flat_map(|l| &l.lines);
            in_rows.chain(in_line).chain(in_lines).find(|w| w.id == id)
        })
    }
}

/// The tables the build ships: each soundbank by the WAD whose copy it replaces (the overlay
/// [`Carrier::Vz`], the shell patch [`Carrier::Shell`], a language's patch), and every wavebank by
/// its bank hash, wherever it ships. An override wavebank of a `vo_*` bank ships to the level of each
/// session that loads the bank, not beside the bank's soundbank in the language's patch.
#[derive(Default)]
struct Built {
    soundbanks: BTreeMap<(Carrier, u32), Soundbank>,
    wavebanks: BTreeMap<u32, Bank>,
}

/// Read every table of `blocks` into `built` under `to`, and each block into `ships`.
fn read_blocks(blocks: &[PatchBlock], to: Carrier, label: &str, built: &mut Built, ships: &mut Vec<Ship>) -> Result<(), String> {
    for b in blocks {
        let raw = mercs2_formats::sges::decompress_sges(&b.compressed_data).map_err(|e| format!("{}: {e}", b.path_string))?;
        let (parsed, issues) = mercs2_formats::ucfx::walk_decompressed_block(&raw, &b.path_string);
        if let Some(i) = issues.first() {
            return Err(format!("{}: {}", i.context, i.detail));
        }
        let mut tables = Vec::new();
        for (e, c) in parsed.entries.iter().zip(&parsed.containers) {
            let body = mercs2_formats::ucfx::extract_data_chunk(c).ok_or_else(|| format!("{}: entry 0x{:08X} has no data chunk", b.path_string, e.name_hash))?;
            let what = match e.type_hash {
                TYPE_HASH_SOUNDBANK => {
                    let sb = Soundbank::parse(&body).map_err(|err| format!("{}: soundbank 0x{:08X}: {err}", b.path_string, e.name_hash))?;
                    let what = format!("soundbank, {} cue(s), {} group(s), {} B", sb.cues.len(), sb.groups.len(), body.len());
                    built.soundbanks.insert((to, e.name_hash), sb);
                    what
                }
                TYPE_HASH_WAVEBANK => {
                    let wb = WavebankFile::parse(&body).map_err(|err| format!("{}: wavebank 0x{:08X}: {err}", b.path_string, e.name_hash))?;
                    let what = format!("wavebank, {} wave(s), {} B", wb.records.len(), body.len());
                    if let Some(other) = built.wavebanks.get(&wb.bank_hash).filter(|b| b.file != wb) {
                        return Err(format!(
                            "the build ships two different wavebanks 0x{:08X} ({} and {} waves)",
                            wb.bank_hash,
                            other.file.records.len(),
                            wb.records.len()
                        ));
                    }
                    built.wavebanks.insert(wb.bank_hash, Bank { file: wb, pws: Ok(None) });
                    what
                }
                mercs2_audio::sounddb::ASSET_TYPE_SOUNDDB => format!("sounddb, {} B", body.len()),
                t => format!("type 0x{t:08X}, {} B", body.len()),
            };
            tables.push((e.name_hash, what));
        }
        ships.push(Ship { to: label.to_string(), path: b.path_string.clone(), tables });
    }
    Ok(())
}

/// The file a streamed wavebank of `carrier` reads from: its stream name is the alias `<name>.pws`
/// the stream is opened under, and the file is `<localized name>.pws`, the language appended to a
/// `vo_` name (`MrxSoundBanks._GetLocalizedName`, `mrxsoundbanks.lua:80-108`; `link::sound_subscriptions`
/// opens a Shipment's streams the same way).
fn stream_file(alias: &str, carrier: Carrier) -> Result<String, String> {
    let stem = alias
        .len()
        .checked_sub(4)
        .filter(|&n| alias.is_char_boundary(n) && alias[n..].eq_ignore_ascii_case(".pws"))
        .map(|n| &alias[..n])
        .ok_or_else(|| format!("the stream name {alias:?} does not end in .pws"))?;
    Ok(match carrier {
        Carrier::Language(l) if qs::is_vo_bank(stem) => format!("{stem}.{}.pws", l.token()),
        _ => alias.to_string(),
    })
}

/// The game's wavebanks of a bank `carrier` carries, each streamed one with its `.pws` beside `vz`.
fn game_waves(vz: &Path, carrier: Carrier, wavebanks: BTreeMap<u32, WavebankFile>) -> BTreeMap<u32, Bank> {
    wavebanks
        .into_iter()
        .map(|(h, file)| {
            let pws = match &file.stream_name {
                None => Ok(None),
                Some(s) => stream_file(s, carrier).and_then(|f| crate::vostream::game_pws(vz, &f)).map(Some),
            };
            (h, Bank { file, pws })
        })
        .collect()
}

/// Whether the build's side of a bank of `carrier` is read: the build's refusal, or that it ships
/// no copy there.
fn built_side(refused: &Option<String>, yours: Option<&Soundbank>, carrier: Carrier) -> Result<(), String> {
    match (refused, yours) {
        (Some(e), _) => Err(e.clone()),
        (None, None) => Err(format!("the build ships no copy of the bank to {}: no session loads it from there", carrier.wad())),
        (None, Some(_)) => Ok(()),
    }
}

/// What `contributions` (an item at `dir`) show and build, against the game whose `vz.wad` is the
/// first of `wads`.
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution]) -> Result<ItemSound, String> {
    let vz = wads.first().ok_or("no game archive is open")?.clone();
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let mut manifest = crate::depot::item_manifest(&item);
    manifest.shipment.name = SHIPMENT.to_string();
    let paths = mercs2_quartermaster::compat::game_stack_paths(&vz, [&manifest])?;
    let mut game = GameStack::open(&paths).map_err(|e| e.to_string())?;
    let s = mercs2_quartermaster::discover::LoadedShipment {
        root: dir.to_path_buf(),
        manifest_path: dir.join(crate::depot::ITEM_FILE),
        format: mercs2_quartermaster::Format::Yaml,
        manifest,
    };

    let mut built = Built::default();
    let mut log = Vec::new();
    let build = qs::lower_shipment_sound(&s, Some(&mut game), &mut log)
        .map_err(|(i, kind, m)| format!("contributions[{i}] ({kind}): {m}"))
        .and_then(|out| {
            let mut ships = Vec::new();
            read_blocks(&out.overlay, Carrier::Vz, "overlay", &mut built, &mut ships)?;
            read_blocks(&out.shell, Carrier::Shell, "shell patch", &mut built, &mut ships)?;
            for (l, blocks) in &out.language {
                read_blocks(blocks, Carrier::Language(*l), &format!("{}-patch", l.token()), &mut built, &mut ships)?;
            }
            let order = [SHIPMENT.to_string()];
            let mut loaders = Vec::new();
            for session in LoadSession::ALL {
                let lua = mercs2_quartermaster::link::session_sound_subscriptions(&out.registrations, session, &order).map_err(|e| e.to_string())?;
                if !lua.is_empty() {
                    loaders.push((session, lua));
                }
            }
            Ok(Build { ships, loaders, log })
        });
    let refused = build.as_ref().err().cloned();

    let mut shell: Option<GameStack> = None;
    let mut views = BTreeMap::new();
    let bw = &built.wavebanks;
    for (index, c) in s.manifest.contributions.iter().enumerate() {
        let mut v = View::default();
        match c {
            Contribution::AddSound { bank, .. } => {
                let entry = m2(bank);
                let title = format!("{bank} as built");
                match (&refused, built.soundbanks.iter().find(|((_, h), _)| *h == entry)) {
                    (Some(e), _) => v.banks.push(BankView { title, rows: Vec::new(), target: None, built: Err(e.clone()) }),
                    (None, None) => v.problems.push(format!("the build ships no soundbank {bank:?} (0x{entry:08X})")),
                    (None, Some((_, sb))) => {
                        v.banks.push(BankView { title, rows: rows(None, Some((sb, &[bw])), None), target: None, built: Ok(()) });
                    }
                }
            }
            Contribution::ReplaceSoundBank { bank, language, .. } | Contribution::ReplaceSoundCue { bank, language, .. } => {
                let target = match c {
                    Contribution::ReplaceSoundCue { cue, .. } => Some(asset_hash(&cue.name)),
                    _ => None,
                };
                match qs::carried_banks(&mut game, &mut shell, bank, *language) {
                    Err(e) => v.problems.push(e),
                    Ok(carried) => {
                        let entry = qs::entry_hash(bank, *language);
                        for cb in carried {
                            let gw = game_waves(&vz, cb.carrier, cb.wavebanks);
                            let yours = built.soundbanks.get(&(cb.carrier, entry));
                            let waves: [&BTreeMap<u32, Bank>; 2] = [bw, &gw];
                            v.banks.push(BankView {
                                title: format!("{} in {}", qs::entry_name(bank, *language), cb.carrier.wad()),
                                rows: rows(Some((&cb.soundbank, &[&gw])), yours.map(|sb| (sb, &waves[..])), None),
                                target,
                                built: built_side(&refused, yours, cb.carrier),
                            });
                        }
                    }
                }
            }
            Contribution::ReplaceSoundWave { wavebank, language, line, .. } => match qs::line_targets(&mut game, &mut shell, wavebank, *language, line) {
                Err(e) => v.problems.push(e),
                Ok(t) => {
                    let carrier = language.map_or(Carrier::Vz, Carrier::Language);
                    let pws = match &t.wavebank.stream_name {
                        None => Ok(None),
                        Some(sn) => stream_file(sn, carrier).and_then(|f| crate::vostream::game_pws(&vz, &f)).map(Some),
                    };
                    let shipped = Wave::from_source(t.from, pws.map(|p| (t.record.clone(), p)));
                    let mut yours: Result<Wave, String> = Err(refused.clone().unwrap_or_else(|| "the build moves no group of a bank that plays the line".into()));
                    for h in &t.entries {
                        let carried = match qs::carried_banks(&mut game, &mut shell, &format!("0x{h:08X}"), *language) {
                            Ok(c) => c,
                            Err(e) => {
                                v.problems.push(e);
                                continue;
                            }
                        };
                        for cb in carried {
                            let gw = game_waves(&vz, cb.carrier, cb.wavebanks);
                            let waves: [&BTreeMap<u32, Bank>; 2] = [bw, &gw];
                            let theirs = built.soundbanks.get(&(cb.carrier, *h));
                            // The groups of the game's copy that play the line, and the cues that reach them.
                            let plays: BTreeSet<usize> = cb
                                .soundbank
                                .groups
                                .iter()
                                .enumerate()
                                .filter(|(_, g)| g.waves().iter().any(|w| (w.wavebank, w.index) == t.from))
                                .map(|(i, _)| i)
                                .collect();
                            let only: BTreeSet<u32> = cb
                                .soundbank
                                .cues
                                .iter()
                                .filter(|c| cue_groups(&c.body).iter().any(|(b, g)| *b == cb.soundbank.bank_hash && plays.contains(&usize::from(*g))))
                                .map(|c| c.guid)
                                .collect();
                            // The wave the build moved those groups to.
                            if let (Some(sb), Err(_)) = (theirs, &yours) {
                                let moved = plays.iter().find_map(|g| {
                                    let before = cb.soundbank.groups.get(*g)?.waves();
                                    let after = sb.groups.get(*g)?.waves();
                                    before.iter().zip(after).find(|(b, _)| (b.wavebank, b.index) == t.from).map(|(_, a)| (a.wavebank, a.index))
                                });
                                if let Some(at) = moved {
                                    yours = Ok(Wave::read(at, &waves));
                                }
                            }
                            v.banks.push(BankView {
                                title: format!("0x{h:08X} in {}", cb.carrier.wad()),
                                rows: rows(Some((&cb.soundbank, &[&gw])), theirs.map(|sb| (sb, &waves[..])), Some(&only)),
                                target: None,
                                built: built_side(&refused, theirs, cb.carrier),
                            });
                        }
                    }
                    v.line = Some(LineView { shipped, yours });
                }
            },
            Contribution::LoadSound { soundbank, wavebank, stream, .. } => {
                let names = (soundbank.as_deref(), wavebank.as_deref(), stream.as_deref());
                load_sound_view(&mut v, (&vz, &mut game, &mut shell), (dir, &s.manifest.contributions), names);
            }
            _ => continue,
        }
        views.insert(index, v);
    }
    Ok(ItemSound { views, build })
}

/// The languages a `vo_` name is loaded in: each whose WAD is beside `vz`; any other name, none.
fn languages_of(vz: &Path, name: &str) -> Vec<Option<Language>> {
    if !qs::is_vo_bank(name) {
        return vec![None];
    }
    Language::ALL.iter().filter(|l| qs::sibling_wad(vz, &format!("{}.wad", l.token())).is_ok()).map(|l| Some(*l)).collect()
}

/// The item's `level_asset` row of `type_hash` named `entry`: its first rung's table, read.
fn level_table(dir: &Path, contributions: &[Contribution], entry: u32, type_hash: u32) -> Option<Result<Vec<u8>, String>> {
    contributions.iter().find_map(|c| {
        let Contribution::LevelAsset { asset_type, name, rungs, .. } = c else { return None };
        let (th, _) = mercs2_formats::types::type_by_name(asset_type)?;
        (th == type_hash && asset_hash(name) == entry).then(|| {
            let rung = rungs.first().ok_or_else(|| format!("level_asset {name:?} has no rung"))?;
            let path = dir.join(&rung.file);
            let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            mercs2_formats::ucfx::extract_data_chunk(&bytes).ok_or_else(|| format!("{}: no data chunk", path.display()))
        })
    })
}

/// The item's `add_level` stream named for the wavebank entry `entry`.
fn level_stream(dir: &Path, contributions: &[Contribution], entry: u32) -> Option<PathBuf> {
    contributions.iter().find_map(|c| {
        let Contribution::AddLevel { streams, .. } = c else { return None };
        streams.iter().find(|s| s.file_stem().and_then(|n| n.to_str()).is_some_and(|n| m2(n) == entry)).map(|s| dir.join(s))
    })
}

/// A wavebank's records, each a line.
fn lines_of(title: String, b: &Bank) -> LinesView {
    let own = BTreeMap::from([(b.file.bank_hash, b.clone())]);
    LinesView { title, lines: (0..b.file.records.len() as u32).map(|i| Wave::read((b.file.bank_hash, i), &[&own])).collect() }
}

/// The language's WAD beside `vz`, opened on its own.
fn language_stack(vz: &Path, l: Language) -> Result<GameStack, String> {
    let p = qs::sibling_wad(vz, &format!("{}.wad", l.token()))?;
    GameStack::open(&[p]).map_err(|e| e.to_string())
}

/// A `load_sound`'s view: each soundbank it names, every cue playable; each wavebank it names (or the
/// item's level wavebank its stream is named for), every record a line.
fn load_sound_view(
    v: &mut View,
    (vz, game, shell): (&Path, &mut GameStack, &mut Option<GameStack>),
    (dir, contributions): (&Path, &[Contribution]),
    (soundbank, wavebank, stream): (Option<&str>, Option<&str>, Option<&str>),
) {
    // The item's level wavebanks, by bank hash, each streamed one from its `add_level` stream.
    let mut level_waves: BTreeMap<u32, Bank> = BTreeMap::new();
    let mut level_entries: Vec<(u32, u32)> = Vec::new();
    for c in contributions {
        let Contribution::LevelAsset { asset_type, name, .. } = c else { continue };
        if mercs2_formats::types::type_by_name(asset_type).map(|(t, _)| t) != Some(TYPE_HASH_WAVEBANK) {
            continue;
        }
        let entry = asset_hash(name);
        match level_table(dir, contributions, entry, TYPE_HASH_WAVEBANK) {
            Some(Ok(body)) => match WavebankFile::parse(&body) {
                Ok(file) => {
                    let pws = match &file.stream_name {
                        None => Ok(None),
                        Some(sn) => level_stream(dir, contributions, entry)
                            .map(Some)
                            .ok_or_else(|| format!("wavebank {name:?} streams from {sn}, and no add_level of the item has a stream named for it")),
                    };
                    level_entries.push((entry, file.bank_hash));
                    level_waves.insert(file.bank_hash, Bank { file, pws });
                }
                Err(e) => v.problems.push(format!("level wavebank {name:?}: {e}")),
            },
            Some(Err(e)) => v.problems.push(e),
            None => {}
        }
    }

    if let Some(name) = soundbank {
        for language in languages_of(vz, name) {
            let entry = qs::entry_name(name, language);
            match level_table(dir, contributions, asset_hash(&entry), TYPE_HASH_SOUNDBANK) {
                Some(Ok(body)) => match Soundbank::parse(&body) {
                    Ok(sb) => v.banks.push(BankView {
                        title: format!("{entry} in the item's level rows"),
                        rows: rows(Some((&sb, &[&level_waves])), None, None),
                        target: None,
                        built: Ok(()),
                    }),
                    Err(e) => v.problems.push(format!("level soundbank {entry:?}: {e}")),
                },
                Some(Err(e)) => v.problems.push(e),
                None => {
                    let read = match language {
                        Some(l) => language_stack(vz, l).and_then(|mut st| qs::carried_banks(&mut st, shell, name, language)),
                        None => qs::carried_banks(game, shell, name, None),
                    };
                    match read {
                        Err(e) => v.problems.push(e),
                        Ok(carried) => {
                            for cb in carried {
                                let gw = game_waves(vz, cb.carrier, cb.wavebanks);
                                v.banks.push(BankView {
                                    title: format!("{entry} in {}", cb.carrier.wad()),
                                    rows: rows(Some((&cb.soundbank, &[&gw, &level_waves])), None, None),
                                    target: None,
                                    built: Ok(()),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(name) = wavebank {
        for language in languages_of(vz, name) {
            let entry = qs::entry_name(name, language);
            let h = asset_hash(&entry);
            if let Some(b) = level_entries.iter().find(|(e, _)| *e == h).and_then(|(_, bank)| level_waves.get(bank)) {
                v.lines.push(lines_of(format!("{entry} in the item's level rows"), b));
                continue;
            }
            let read = match language {
                Some(l) => language_stack(vz, l).and_then(|mut st| qs::carried_wavebanks(&mut st, shell, name, language)),
                None => qs::carried_wavebanks(game, shell, name, None),
            };
            match read {
                Err(e) => v.problems.push(e),
                Ok(found) => {
                    for (carrier, file) in found {
                        for b in game_waves(vz, carrier, BTreeMap::from([(file.bank_hash, file)])).values() {
                            v.lines.push(lines_of(format!("{entry} in {}", carrier.wad()), b));
                        }
                    }
                }
            }
        }
    }
    if let Some(name) = stream {
        match level_entries.iter().find(|(e, _)| *e == m2(name)).and_then(|(_, b)| level_waves.get(b)) {
            Some(b) => v.lines.push(lines_of(format!("{name}.pws: the item's level wavebank {name}"), b)),
            None => v.problems.push(format!("no level_asset wavebank of the item is named {name:?}, the entry its stream {name}.pws is named for")),
        }
    }
}

/// The sound editors' reads and choices.
#[derive(Default)]
pub struct Sound {
    /// What each item builds and shows, by item folder, with what it was computed from.
    after: RefCell<HashMap<PathBuf, (String, Loading<ItemSound>)>>,
    /// The row each bank view has open, by item folder, contribution and bank.
    open: RefCell<HashMap<(PathBuf, usize, usize), u32>>,
    /// Each drawn wave's waveform, by its id, decoded on a worker the first time it is drawn.
    peaks: RefCell<HashMap<u32, Loading<Vec<[i16; 2]>>>>,
    /// What the Depot's player is playing, as the app last read it.
    pub playing: Cell<Option<Status>>,
}

/// What the sound editors ask of the page.
pub enum Ask {
    /// Play, pause or resume the wave the player knows as this id.
    Play(u32),
    /// Open a row of a bank view.
    Open { item: PathBuf, index: usize, bank: usize, guid: u32 },
    /// Rewrite contribution `index` of the item.
    Edit { index: usize, c: Box<Contribution> },
}

impl Sound {
    /// Forget what was read from the game (a different stack).
    pub fn forget_game(&self) {
        self.after.borrow_mut().clear();
        self.peaks.borrow_mut().clear();
    }

    /// `w`'s waveform: `None` while it decodes.
    fn peaks_of(&self, w: &Wave) -> Option<Result<Rc<Vec<[i16; 2]>>, String>> {
        let mut map = self.peaks.borrow_mut();
        let entry = map.entry(w.id).or_insert_with(|| {
            let w = w.clone();
            spawn(move || w.pcm().map(|p| peaks(&p.samples)))
        });
        poll(entry)
    }

    /// What `item` builds and shows: `None` while it is read, or when it holds no sound contribution.
    pub fn after(&self, game: &[PathBuf], item: &Item) -> Option<Result<Rc<ItemSound>, String>> {
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        if !contributions.iter().any(of) {
            return None;
        }
        let mut stamp = format!("{game:?}|{contributions:?}");
        let mut files: Vec<&Path> = Vec::new();
        for c in &contributions {
            match c {
                Contribution::AddSound { cues, .. } | Contribution::ReplaceSoundBank { cues, .. } => files.extend(cues.iter().map(|c| c.wave.as_path())),
                Contribution::ReplaceSoundCue { cue, .. } => files.push(&cue.wave),
                Contribution::ReplaceSoundWave { wave, .. } => files.push(wave),
                Contribution::LevelAsset { rungs, .. } => files.extend(rungs.iter().map(|r| r.file.as_path())),
                Contribution::AddLevel { streams, .. } => files.extend(streams.iter().map(|s| s.as_path())),
                _ => {}
            }
        }
        for f in files {
            let t = std::fs::metadata(item.dir.join(f)).and_then(|m| m.modified()).ok();
            stamp.push_str(&format!("|{t:?}"));
        }
        let mut map = self.after.borrow_mut();
        if map.get(&item.dir).is_none_or(|(s, _)| *s != stamp) {
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            map.insert(item.dir.clone(), (stamp, spawn(move || after(&wads, &dir, &contributions))));
        }
        let (_, entry) = map.get_mut(&item.dir)?;
        poll(entry)
    }

    /// The wave the player knows as `id`, in any item read.
    pub fn wave(&self, id: u32) -> Option<Wave> {
        self.after.borrow().values().find_map(|(_, l)| match l {
            Loading::Ready(Ok(s)) => s.wave(id).cloned(),
            _ => None,
        })
    }

    fn open_row(&self, key: &(PathBuf, usize, usize)) -> Option<u32> {
        self.open.borrow().get(key).copied()
    }

    /// Open `guid`'s row of a bank view.
    pub fn set_open(&self, item: PathBuf, index: usize, bank: usize, guid: u32) {
        self.open.borrow_mut().insert((item, index, bank), guid);
    }
}

/// A hash's name: one of `known` when it is the hash's, else the names table's, else the hash.
fn name_of(hash: u32, known: &[&str], names: Option<&NameTable>) -> String {
    known
        .iter()
        .find(|n| asset_hash(n) == hash)
        .map(|n| n.to_string())
        .or_else(|| names.and_then(|t| t.reverse(hash)).map(str::to_string))
        .unwrap_or_else(|| format!("0x{hash:08X}"))
}

/// The names the contribution writes.
fn contribution_names(c: &Contribution) -> Vec<&str> {
    match c {
        Contribution::AddSound { cues, .. } | Contribution::ReplaceSoundBank { cues, .. } => cues.iter().map(|c| c.name.as_str()).collect(),
        Contribution::ReplaceSoundCue { cue, .. } => vec![cue.name.as_str()],
        Contribution::ReplaceSoundWave { line, .. } => vec![line.as_str()],
        _ => Vec::new(),
    }
}

/// What the rows of one contribution's preview are drawn with.
struct Ctx<'a> {
    s: &'a Sound,
    item: &'a Item,
    index: usize,
    c: &'a Contribution,
    /// A `replace_sound_bank`: a game cue it does not declare is dropped.
    replacing: bool,
    /// The sides the kind has: no game side for a new bank (`add_sound`), no build side for a bank it
    /// only loads (`load_sound`).
    shipped: bool,
    yours: bool,
    known: Vec<&'a str>,
    playing: Option<Status>,
    names: Option<&'a NameTable>,
}

/// A play button and the wave's waveform, the playhead on it while it plays.
fn wave_row(ui: &mut egui::Ui, w: &Wave, cx: &Ctx<'_>, asks: &mut Vec<Ask>) {
    ui.horizontal(|ui| {
        let state = cx.playing.filter(|s| s.clip == w.id);
        let peaks = match &w.source {
            Err(e) => Some(Err(e.clone())),
            Ok(_) => cx.s.peaks_of(w),
        };
        if peaks.is_none() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        let ok = matches!(peaks, Some(Ok(_)));
        let (r, resp) = ui.allocate_exact_size(egui::vec2(22.0, WAVE_H), egui::Sense::click());
        let col = if !ok {
            theme::FAINT
        } else if resp.hovered() {
            theme::BRASS
        } else {
            theme::TX
        };
        let p = ui.painter();
        p.rect_filled(r, egui::CornerRadius::same(3), theme::G3);
        let c = r.center();
        if state.is_some_and(|s| !s.paused) {
            for dx in [-3.5, 1.5] {
                p.rect_filled(egui::Rect::from_min_size(egui::pos2(c.x + dx, c.y - 5.0), egui::vec2(2.5, 10.0)), egui::CornerRadius::ZERO, col);
            }
        } else {
            p.add(egui::Shape::convex_polygon(
                vec![egui::pos2(c.x - 3.5, c.y - 5.5), egui::pos2(c.x + 5.0, c.y), egui::pos2(c.x - 3.5, c.y + 5.5)],
                col,
                egui::Stroke::NONE,
            ));
        }
        if ok && resp.on_hover_text("Play").clicked() {
            asks.push(Ask::Play(w.id));
        }
        let width = (ui.available_width() - 150.0).max(80.0);
        let (wr, wresp) = ui.allocate_exact_size(egui::vec2(width, WAVE_H), egui::Sense::click());
        let p = ui.painter();
        p.rect_filled(wr, egui::CornerRadius::same(3), theme::G0);
        let note = |text: &str, col| {
            p.text(wr.left_center() + egui::vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, text, theme::font(&egui::TextStyle::Small), col);
        };
        match &peaks {
            None => note("decoding\u{2026}", theme::FAINT),
            Some(Ok(pk)) if !pk.is_empty() => {
                let mid = wr.center().y;
                let half = wr.height() / 2.0 - 2.0;
                let step = wr.width() / pk.len() as f32;
                for (i, [lo, hi]) in pk.iter().enumerate() {
                    let x = wr.left() + (i as f32 + 0.5) * step;
                    let y0 = mid - f32::from(*hi) / 32768.0 * half;
                    let y1 = mid - f32::from(*lo) / 32768.0 * half;
                    p.line_segment([egui::pos2(x, y0), egui::pos2(x, y1.max(y0 + 1.0))], egui::Stroke::new(step.max(1.0), theme::DIM));
                }
            }
            Some(Ok(_)) => note("no samples", theme::FAINT),
            Some(Err(e)) => note(e, theme::BAD),
        }
        if let Some(s) = state {
            let x = wr.left() + wr.width() * (s.position_s / s.length_s.max(1e-6)).clamp(0.0, 1.0);
            p.line_segment([egui::pos2(x, wr.top()), egui::pos2(x, wr.bottom())], egui::Stroke::new(1.5, theme::BRASS));
        }
        if ok && wresp.clicked() {
            asks.push(Ask::Play(w.id));
        }
        let clip = w.clip.map(|c| name_of(c, &cx.known, cx.names)).unwrap_or_else(|| format!("0x{:08X} #{}", w.at.0, w.at.1));
        ui.label(egui::RichText::new(format!("{:.2} s", w.seconds)).monospace().color(theme::DIM));
        ui.add(egui::Label::new(egui::RichText::new(clip).small().color(theme::FAINT)).truncate());
    });
}

/// One side of a row: its facts, its waves, and what it reaches that is not read.
fn side_waves(ui: &mut egui::Ui, label: &str, s: Option<&CueSide>, none: &str, cx: &Ctx<'_>, asks: &mut Vec<Ask>) {
    ui.horizontal(|ui| {
        ui.add_sized([62.0, 16.0], egui::Label::new(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), theme::FAINT)));
        let text = match s {
            None => none.to_string(),
            Some(s) => {
                // Retail writes -1 as the length of a cue whose sounds loop (audio_code_map.md §11.4).
                let length = if s.length_s == -1.0 { "loops".to_string() } else { format!("{:.2} s", s.length_s) };
                let mut facts = vec![length, format!("{} group(s)", s.groups), format!("{} wave(s)", s.waves.len())];
                if s.multi_track {
                    facts.push("multi-track".into());
                }
                facts.join(" \u{b7} ")
            }
        };
        ui.label(egui::RichText::new(text).small().color(if s.is_some() { theme::DIM } else { theme::FAINT }));
    });
    if let Some(s) = s {
        for w in &s.waves {
            wave_row(ui, w, cx, asks);
        }
        for (b, g) in &s.foreign {
            ui.label(egui::RichText::new(format!("group {g} of soundbank 0x{b:08X}, which is not read")).small().color(theme::FAINT));
        }
    }
}

/// The value rows of a cue, shipped beside yours, each column only when the kind has that side; a
/// value of yours that differs is marked.
fn values_grid(ui: &mut egui::Ui, cx: &Ctx<'_>, shipped: Option<&SoundCue>, yours: Option<&SoundCue>) {
    type Get = fn(&SoundCue) -> String;
    let rows: [(&str, Get); 14] = [
        ("group_gain_db", |c| format!("{:.3}", c.group_gain_db)),
        ("cue_gain_db", |c| format!("{:.3}", c.cue_gain_db)),
        ("pitch_semitones", |c| format!("{:.3}", c.pitch_semitones)),
        ("positional", |c| c.positional.to_string()),
        ("min_distance", |c| c.min_distance.to_string()),
        ("max_distance", |c| c.max_distance.to_string()),
        ("distance_exponent", |c| c.distance_exponent.to_string()),
        ("doppler_scale", |c| c.doppler_scale.to_string()),
        ("start_limit", |c| c.start_limit.to_string()),
        ("sound_id", |c| format!("0x{:08X}", c.sound_id)),
        ("priority", |c| c.priority.to_string()),
        ("group_20", |c| c.group_20.to_string()),
        ("cue_16", |c| c.cue_16.to_string()),
        ("clip_hash", |c| format!("0x{:08X}", c.clip_hash)),
    ];
    let columns = 1 + usize::from(cx.shipped) + usize::from(cx.yours);
    egui::Grid::new(ui.next_auto_id()).num_columns(columns).spacing([14.0, 2.0]).striped(true).show(ui, |ui| {
        for (h, on) in [("VALUE", true), ("SHIPPED", cx.shipped), ("YOURS", cx.yours)] {
            if on {
                ui.label(theme::disp_text(h, theme::style_eyebrow(), theme::FAINT));
            }
        }
        ui.end_row();
        for (name, get) in rows {
            let (a, b) = (shipped.map(get), yours.map(get));
            let differs = matches!((&a, &b), (Some(x), Some(y)) if x != y);
            ui.label(egui::RichText::new(name).monospace().color(theme::DIM));
            if cx.shipped {
                ui.label(egui::RichText::new(a.unwrap_or_else(|| "\u{2014}".into())).monospace().color(theme::TX));
            }
            if cx.yours {
                ui.label(egui::RichText::new(b.unwrap_or_else(|| "\u{2014}".into())).monospace().color(if differs { theme::BRASS } else { theme::TX }));
            }
            ui.end_row();
        }
    });
}

/// Whether two sides of a cue are one cue: the same length, values and waves.
fn same(a: &CueSide, b: &CueSide) -> bool {
    a.length_s.to_bits() == b.length_s.to_bits()
        && a.groups == b.groups
        && a.values == b.values
        && a.foreign == b.foreign
        && a.waves.iter().map(|w| w.at).eq(b.waves.iter().map(|w| w.at))
}

/// What a row is: the cue the build replaces, adds, drops or leaves as it is, or the game's alone.
fn row_state(r: &CueRow, replacing: bool) -> (&'static str, egui::Color32) {
    match (&r.shipped, &r.yours) {
        (Some(a), Some(b)) if same(a, b) => ("unchanged", theme::DIM),
        (Some(_), Some(_)) => ("replaced", theme::BRASS),
        (None, Some(_)) => ("new", theme::GOOD),
        (Some(_), None) if replacing => ("dropped", theme::BAD),
        (Some(_), None) => ("shipped", theme::DIM),
        (None, None) => ("", theme::FAINT),
    }
}

/// The preview of contribution `index` of `item`.
pub fn editor(ui: &mut egui::Ui, s: &Sound, item: &Item, index: usize, game: &[PathBuf], names: Option<&NameTable>) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return asks };
    if !of(c) {
        return asks;
    }
    theme::section(ui, "Preview", Some(c.kind()), true, |ui| {
        let after = match s.after(game, item) {
            None => {
                ui.label(egui::RichText::new("Reading the game's banks and building\u{2026}").color(theme::DIM));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Some(Err(e)) => {
                theme::field_note(ui, FieldState::Bad, &e);
                return;
            }
            Some(Ok(a)) => a,
        };
        let Some(v) = after.views.get(&index) else { return };
        let cx = Ctx {
            s,
            item,
            index,
            c,
            replacing: matches!(c, Contribution::ReplaceSoundBank { .. }),
            shipped: !matches!(c, Contribution::AddSound { .. }),
            yours: !matches!(c, Contribution::LoadSound { .. }),
            known: contribution_names(c),
            playing: s.playing.get(),
            names,
        };
        if cx.playing.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        for p in &v.problems {
            theme::field_note(ui, FieldState::Bad, p);
        }
        if let Some(l) = &v.line {
            theme::eyebrow(ui, "The line");
            for (label, w) in [("shipped", Ok(&l.shipped)), ("yours", l.yours.as_ref())] {
                ui.add_sized([62.0, 16.0], egui::Label::new(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), theme::FAINT)));
                match w {
                    Ok(w) => wave_row(ui, w, &cx, &mut asks),
                    Err(e) => theme::field_note(ui, FieldState::Bad, e),
                }
            }
            ui.add_space(6.0);
        }
        for (bi, b) in v.banks.iter().enumerate() {
            ui.push_id(("bank", bi), |ui| bank_view(ui, &cx, (bi, b), &mut asks));
        }
        for (li, l) in v.lines.iter().enumerate() {
            ui.push_id(("lines", li), |ui| {
                ui.horizontal(|ui| {
                    theme::eyebrow(ui, &l.title);
                    ui.label(egui::RichText::new(format!("{} line(s)", l.lines.len())).small().color(theme::FAINT));
                });
                let row_h = WAVE_H + ui.spacing().item_spacing.y;
                theme::scroll_vertical().max_height(LIST_H).auto_shrink([false, true]).show_rows(ui, row_h, l.lines.len(), |ui, range| {
                    for w in &l.lines[range] {
                        wave_row(ui, w, &cx, &mut asks);
                    }
                });
            });
        }
    });
    asks
}

/// The `replace_sound_cue` that replaces `guid` of the bank with `values` (its wave kept): the cue
/// named by `name` when that is the guid's, else by the guid.
fn replacing_cue(c: &Contribution, guid: u32, name: &str, mut values: SoundCue) -> Option<Contribution> {
    let Contribution::ReplaceSoundCue { bank, language, category, cue } = c else { return None };
    values.name = if asset_hash(name) == guid { name.to_string() } else { format!("0x{guid:08X}") };
    values.wave = cue.wave.clone();
    Some(Contribution::ReplaceSoundCue { bank: bank.clone(), language: *language, category: category.clone(), cue: values })
}

fn bank_view(ui: &mut egui::Ui, cx: &Ctx<'_>, (bi, b): (usize, &BankView), asks: &mut Vec<Ask>) {
    ui.horizontal_wrapped(|ui| {
        theme::eyebrow(ui, &b.title);
        let facts: Vec<String> = ["replaced", "new", "dropped", "unchanged", "shipped"]
            .into_iter()
            .filter_map(|st| {
                let n = b.rows.iter().filter(|r| row_state(r, cx.replacing).0 == st).count();
                (n > 0).then(|| format!("{n} {st}"))
            })
            .collect();
        ui.label(egui::RichText::new(facts.join(" \u{b7} ")).small().color(theme::FAINT));
    });
    if let Err(e) = &b.built {
        theme::field_note(ui, FieldState::Bad, e);
    }
    let open = cx.s.open_row(&(cx.item.dir.clone(), cx.index, bi)).or(b.target).or_else(|| b.rows.first().map(|r| r.guid));
    // The target first, then the rows the build changes, then the rest.
    let mut order: Vec<&CueRow> = b.rows.iter().collect();
    order.sort_by_key(|r| (Some(r.guid) != b.target, !matches!(row_state(r, cx.replacing).0, "replaced" | "new")));
    theme::scroll_vertical().id_salt(("rows", bi)).max_height(LIST_H).auto_shrink([false, true]).show(ui, |ui| {
        for (ri, r) in order.iter().enumerate() {
            let (state, col) = row_state(r, cx.replacing);
            let is_open = open == Some(r.guid);
            let name = name_of(r.guid, &cx.known, cx.names);
            ui.push_id(r.guid, |ui| {
                egui::Frame::NONE
                    .fill(if is_open { theme::G1 } else { theme::row_stripe(ri % 2 == 1) })
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let (mr, mresp) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::click());
                            let c = mr.center();
                            let tri = if is_open {
                                vec![egui::pos2(c.x - 4.0, c.y - 2.0), egui::pos2(c.x + 4.0, c.y - 2.0), egui::pos2(c.x, c.y + 3.0)]
                            } else {
                                vec![egui::pos2(c.x - 2.0, c.y - 4.0), egui::pos2(c.x + 3.0, c.y), egui::pos2(c.x - 2.0, c.y + 4.0)]
                            };
                            ui.painter().add(egui::Shape::convex_polygon(tri, theme::DIM, egui::Stroke::NONE));
                            let label = ui.add(egui::Label::new(egui::RichText::new(&name).color(theme::TX)).sense(egui::Sense::click()));
                            if mresp.clicked() || label.clicked() {
                                asks.push(Ask::Open { item: cx.item.dir.clone(), index: cx.index, bank: bi, guid: r.guid });
                            }
                            let hash = format!("0x{:08X}", r.guid);
                            if name != hash {
                                ui.label(egui::RichText::new(hash).monospace().small().color(theme::FAINT));
                            }
                            if !state.is_empty() {
                                ui.label(egui::RichText::new(state).small().color(col));
                            }
                            if Some(r.guid) == b.target {
                                ui.label(egui::RichText::new("the cue it replaces").small().color(theme::BRASS));
                            }
                            if let (Contribution::ReplaceSoundCue { .. }, Some(sh)) = (cx.c, &r.shipped) {
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let label = if Some(r.guid) == b.target { "Use shipped values" } else { "Replace this cue" };
                                    let resp = ui
                                        .add_enabled(sh.values.is_some(), egui::Button::new(label))
                                        .on_disabled_hover_text("It plays more than one wave; a replace_sound_cue carries one.");
                                    if resp.clicked() {
                                        if let Some(new) = sh.values.clone().and_then(|v| replacing_cue(cx.c, r.guid, &name, v)) {
                                            asks.push(Ask::Edit { index: cx.index, c: Box::new(new) });
                                        }
                                    }
                                });
                            }
                        });
                        if is_open {
                            if cx.shipped {
                                side_waves(ui, "shipped", r.shipped.as_ref(), "the game's bank has no such cue", cx, asks);
                            }
                            if cx.yours {
                                let none = if b.built.is_err() {
                                    "not built"
                                } else if cx.replacing {
                                    "dropped: not among your cues"
                                } else {
                                    "as the game ships it"
                                };
                                side_waves(ui, "yours", r.yours.as_ref(), none, cx, asks);
                            }
                            let (a, y) = (r.shipped.as_ref().and_then(|x| x.values.as_ref()), r.yours.as_ref().and_then(|x| x.values.as_ref()));
                            if a.is_some() || y.is_some() {
                                values_grid(ui, cx, a, y);
                            }
                        }
                    });
            });
        }
        if b.rows.is_empty() && b.built.is_ok() {
            ui.label(egui::RichText::new("no cue").color(theme::FAINT));
        }
    });
    ui.add_space(6.0);
}

/// The inspector's card: what the item's sound build ships, and what each session's loader loads.
pub fn after_card(ui: &mut egui::Ui, s: &Sound, game: &[PathBuf], item: &Item) {
    let Some(after) = s.after(game, item) else {
        if item.file.as_ref().is_ok_and(|f| f.contributions.iter().any(of)) {
            theme::card(ui, "Build", None, |ui| {
                ui.label(egui::RichText::new("Building\u{2026}").color(theme::DIM));
            });
        }
        return;
    };
    theme::card(ui, "Build", Some("qm build"), |ui| match after.as_ref().map(|a| &a.build) {
        Err(e) | Ok(Err(e)) => theme::field_note(ui, FieldState::Bad, e),
        Ok(Ok(b)) => {
            for ship in &b.ships {
                theme::kv(ui, &ship.to, egui::RichText::new(&ship.path).monospace());
                for (h, what) in &ship.tables {
                    ui.label(egui::RichText::new(format!("0x{h:08X}  {what}")).monospace().small().color(theme::DIM));
                }
            }
            if b.ships.is_empty() {
                ui.label(egui::RichText::new("ships no sound table").color(theme::DIM));
            }
            for (session, lua) in &b.loaders {
                ui.add_space(4.0);
                theme::eyebrow(ui, &format!("{} loader", session.token()));
                theme::code_block(ui, lua.trim_end());
            }
            for line in &b.log {
                ui.label(egui::RichText::new(line).small().color(theme::FAINT));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_audio::encode::{build_general, CueBodySpec, CueDef, GroupFormSpec, GroupHeadParams, GroupSpec, Pcm16, TablesSpec, WaveSpec};

    /// A soundbank whose cue `<name>_i` plays its group `i`, which plays wave `i` of its wavebank.
    fn bank(name: &str, cues: usize) -> (Soundbank, WavebankFile) {
        let t = build_general(&TablesSpec {
            name: name.into(),
            hash: m2(name),
            waves: (0..cues)
                .map(|i| WaveSpec { clip_hash: 0x100 + i as u32, pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![i as i16 * 100; 64] } })
                .collect(),
            groups: (0..cues)
                .map(|i| GroupSpec {
                    sound_id: 1,
                    category: "ui".into(),
                    head: GroupHeadParams { unknown_10: 1.0, unknown_14: 0, min_distance: 1.0, max_distance: 2.0, unknown_20: 0.0, distance_exponent: 1.0, doppler_scale: 0.0 },
                    form: GroupFormSpec::Single { wave: i, gain: 1.0, unknown_30: 0.0, weight: 1.0 },
                })
                .collect(),
            cues: (0..cues)
                .map(|i| CueDef { name: format!("{name}_{i}"), guid: m2(&format!("{name}_{i}")), byte_06: 1, gain: 1.0, body: CueBodySpec::SingleTrack { group: i, unknown_16: 0 } })
                .collect(),
        })
        .expect("builds");
        (t.soundbank, t.wavebank)
    }

    fn waves(wb: WavebankFile) -> BTreeMap<u32, Bank> {
        BTreeMap::from([(wb.bank_hash, Bank { file: wb, pws: Ok(None) })])
    }

    /// The rows are yours in order, then the game's cues yours lacks; each side plays the wave its
    /// group names, decoded from the table, and a cue of one wave carries its values.
    #[test]
    fn rows_pair_the_cues_by_guid_and_read_each_sides_waves() {
        let (game, gw) = bank("b", 3);
        let (mut yours, yw) = bank("b", 1);
        yours.cues[0].guid = m2("b_2");
        let (gw, yw) = (waves(gw), waves(yw));
        let rs = rows(Some((&game, &[&gw])), Some((&yours, &[&yw])), None);
        let guids: Vec<u32> = rs.iter().map(|r| r.guid).collect();
        assert_eq!(guids, vec![m2("b_2"), m2("b_0"), m2("b_1")]);
        assert_eq!(row_state(&rs[0], true).0, "replaced");
        assert_eq!(row_state(&rs[1], true).0, "dropped");
        assert_eq!(row_state(&rs[1], false).0, "shipped");
        let shipped = rs[0].shipped.as_ref().unwrap();
        assert_eq!(shipped.waves.len(), 1);
        assert_eq!(shipped.waves[0].clip, Some(0x102));
        assert_eq!(shipped.waves[0].pcm().unwrap().samples, vec![200; 64]);
        assert_eq!(rs[0].yours.as_ref().unwrap().waves[0].pcm().unwrap().samples, vec![0; 64]);
        assert_eq!(shipped.values.as_ref().map(|v| v.clip_hash), Some(0x102));
        let only = BTreeSet::from([m2("b_1")]);
        assert_eq!(rows(Some((&game, &[&gw])), None, Some(&only)).len(), 1);
    }

    /// A wave whose wavebank is not read, or whose record is past its end, says so and does not play.
    #[test]
    fn a_wave_nothing_holds_names_what_is_missing() {
        let (_, wb) = bank("b", 1);
        let w = waves(wb);
        let missing = Wave::read((0xDEAD, 0), &[&w]);
        assert!(missing.source.as_ref().err().is_some_and(|e| e.contains("0x0000DEAD")));
        let past = Wave::read((m2("b"), 5), &[&w]);
        assert!(past.source.as_ref().err().is_some_and(|e| e.contains("record 5")));
        assert!(past.pcm().is_err());
    }

    #[test]
    fn peaks_fold_each_column_to_its_min_and_max() {
        let s: Vec<i16> = (0..320).map(|i| if i % 2 == 0 { -i } else { i }).collect();
        let p = peaks(&s);
        assert_eq!(p.len(), PEAKS);
        assert_eq!(p[0], [0, 1]);
        assert_eq!(p[159], [-318, 319]);
        assert!(peaks(&[]).is_empty());
    }

    /// A row whose sides are one cue is unchanged; a moved wave makes it replaced.
    #[test]
    fn a_row_whose_sides_are_one_cue_is_unchanged() {
        let (game, gw) = bank("b", 2);
        let gw = waves(gw);
        let rs = rows(Some((&game, &[&gw])), Some((&game, &[&gw])), None);
        assert!(rs.iter().all(|r| row_state(r, false).0 == "unchanged"));
        let mut moved = game.clone();
        moved.cues.swap(0, 1);
        (moved.cues[0].guid, moved.cues[1].guid) = (game.cues[0].guid, game.cues[1].guid);
        let rs = rows(Some((&game, &[&gw])), Some((&moved, &[&gw])), None);
        assert!(rs.iter().all(|r| row_state(r, false).0 == "replaced"));
    }

    /// A stream's file is its alias, with the language before `.pws` for a `vo_` stream of a
    /// language's copy.
    #[test]
    fn a_vo_stream_reads_its_languages_file() {
        assert_eq!(stream_file("vo_stream.pws", Carrier::Language(Language::English)).unwrap(), "vo_stream.english.pws");
        assert_eq!(stream_file("music.pws", Carrier::Vz).unwrap(), "music.pws");
        assert_eq!(stream_file("MUSIC.PWS", Carrier::Language(Language::German)).unwrap(), "MUSIC.PWS");
        assert!(stream_file("music", Carrier::Vz).is_err());
    }

    /// "Replace this cue" names the cue by its name when the name is the guid's, else by the guid,
    /// carries the game's values and keeps the author's wave.
    #[test]
    fn replacing_a_cue_seeds_it_from_the_game_and_keeps_the_wave() {
        let mut values = crate::audioroute::retail_cue(&bank("b", 1).0.cues[0], &bank("b", 1).0.groups[0], 0x100);
        values.start_limit = 9;
        let c = Contribution::ReplaceSoundCue {
            bank: "b".into(),
            language: None,
            category: "ui".into(),
            cue: SoundCue { wave: "src/mine.wav".into(), ..values.clone() },
        };
        let Some(Contribution::ReplaceSoundCue { cue, .. }) = replacing_cue(&c, m2("b_0"), "b_0", values.clone()) else { panic!("a cue") };
        assert_eq!((cue.name.as_str(), cue.wave.to_str(), cue.start_limit), ("b_0", Some("src/mine.wav"), 9));
        let Some(Contribution::ReplaceSoundCue { cue, .. }) = replacing_cue(&c, m2("b_0"), "0xDEAD", values) else { panic!("a cue") };
        assert_eq!(cue.name, format!("0x{:08X}", m2("b_0")));
    }
}

/// Against the game named by `.mercs2-local.toml`: each sound kind's preview from qm's own build, the
/// built waves decoded back to the authored samples, the game's decoded from their tables and `.pws`.
#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use mercs2_audio::encode::Pcm16;

    fn vz() -> PathBuf {
        mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
    }

    /// A second of a 440 Hz tone at 22,050 Hz.
    fn tone() -> Pcm16 {
        let samples = (0..22_050).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / 22_050.0).sin() * 8000.0) as i16).collect();
        Pcm16 { channels: 1, sample_rate: 22_050, samples }
    }

    /// An item folder holding the tone as `src/mine.wav`.
    fn item(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depotsound_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/mine.wav"), mercs2_audio::wav::write_pcm16_wav(&tone()).unwrap()).unwrap();
        d
    }

    /// Of the first channel of interleaved `s`: mean absolute step over RMS, about 1.4 for noise and
    /// far less for a recorded sound.
    fn roughness(s: &[i16], channels: u8) -> f64 {
        let s: Vec<f64> = s.iter().step_by(usize::from(channels.max(1))).map(|x| f64::from(*x)).collect();
        let rms = (s.iter().map(|x| x * x).sum::<f64>() / s.len() as f64).sqrt();
        let step = s.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (s.len() - 1) as f64;
        step / rms
    }

    fn cue(name: String) -> SoundCue {
        SoundCue {
            name,
            wave: "src/mine.wav".into(),
            group_gain_db: 0.0,
            cue_gain_db: 0.0,
            pitch_semitones: 0.0,
            positional: false,
            min_distance: 1.0,
            max_distance: 50.0,
            distance_exponent: 1.0,
            doppler_scale: 0.0,
            start_limit: 1,
            sound_id: 1,
            priority: 1.0,
            group_20: 0.0,
            cue_16: 0,
            clip_hash: 0x1234_5678,
        }
    }

    /// The guid of the first cue of `bank` that plays one wave.
    fn a_cue_of(bank: &str) -> u32 {
        let mut game = GameStack::open(&[vz()]).unwrap();
        let carried = qs::carried_banks(&mut game, &mut None, bank, None).unwrap_or_else(|e| panic!("{e}"));
        let sb = &carried[0].soundbank;
        sb.cues
            .iter()
            .find(|c| matches!(cue_groups(&c.body).as_slice(), [(b, g)] if *b == sb.bank_hash && sb.groups[usize::from(*g)].waves().len() == 1))
            .unwrap_or_else(|| panic!("{bank} has no one-wave cue"))
            .guid
    }

    fn view(dir: &Path, c: Contribution) -> (ItemSound, View) {
        let mut s = after(&[vz()], dir, &[c]).unwrap_or_else(|e| panic!("{e}"));
        if let Err(e) = &s.build {
            panic!("the build refused: {e}");
        }
        let v = s.views.remove(&0).expect("a view of contribution 0");
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        (s, v)
    }

    fn samples(w: &Wave) -> Vec<i16> {
        w.pcm().unwrap_or_else(|e| panic!("{e}")).samples
    }

    /// A cue of a bank retail Lua loads (`ui_hud`, carried by vz.wad and shell.wad) and of one the
    /// engine loads (`veh_jeep`): in each WAD's copy the cue replaced plays the authored samples from
    /// the build's tables, the game's cue decodes from its own, and every other cue is unchanged.
    #[test]
    fn a_replaced_cue_plays_the_authored_wave_beside_the_games() {
        for (bank, wads) in [("ui_hud", 2), ("veh_jeep", 1)] {
            let dir = item(bank);
            let guid = a_cue_of(bank);
            eprintln!("{bank}: replacing cue 0x{guid:08X}");
            let c = Contribution::ReplaceSoundCue { bank: bank.into(), language: None, category: "ui".into(), cue: cue(format!("0x{guid:08X}")) };
            let (_, v) = view(&dir, c);
            assert_eq!(v.banks.len(), wads, "{bank}: one view per WAD that carries it");
            for b in &v.banks {
                assert_eq!(b.built, Ok(()), "{bank} {}", b.title);
                assert_eq!(b.target, Some(guid));
                let target = b.rows.iter().find(|r| r.guid == guid).expect("the target's row");
                assert_eq!(row_state(target, false).0, "replaced");
                let yours = target.yours.as_ref().unwrap();
                assert_eq!(yours.waves.len(), 1);
                assert_eq!(samples(&yours.waves[0]), tone().samples, "{bank} {}: the built wave is the authored one", b.title);
                let shipped = target.shipped.as_ref().unwrap();
                assert!(!samples(&shipped.waves[0]).is_empty());
                assert!(shipped.values.is_some(), "a one-wave cue carries its values");
                let others = b.rows.iter().filter(|r| r.guid != guid);
                assert!(others.clone().count() > 0);
                assert!(others.clone().all(|r| row_state(r, false).0 == "unchanged"), "{bank} {}", b.title);
            }
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    /// A replaced line of `vo_stream.english`: the game's record decodes from `vo_stream.english.pws`
    /// as a recorded sound, not noise; the build's is the authored samples; and each cue that plays the
    /// line is listed, moved to the new wave.
    #[test]
    fn a_replaced_line_plays_the_streamed_line_beside_the_authored_one() {
        let english = qs::sibling_wad(&vz(), "English.wad").unwrap_or_else(|e| panic!("{e}"));
        let mut game = GameStack::open(&[vz(), english]).unwrap();
        let carried = qs::carried_banks(&mut game, &mut None, "vo_Jen", Some(Language::English)).unwrap_or_else(|e| panic!("{e}"));
        let at = carried[0].soundbank.groups[0].waves()[0];
        let clip = carried[0].wavebanks[&at.wavebank].records[at.index as usize].clip_hash;
        let dir = item("line");
        let c = Contribution::ReplaceSoundWave { wavebank: "vo_stream".into(), language: Some(Language::English), line: format!("0x{clip:08X}"), wave: "src/mine.wav".into() };
        let (_, v) = view(&dir, c);
        let line = v.line.as_ref().expect("the line");
        let pcm = line.shipped.pcm().unwrap_or_else(|e| panic!("{e}"));
        let shipped = pcm.samples;
        assert!(shipped.len() > 1000, "{} samples", shipped.len());
        let r = roughness(&shipped, pcm.channels);
        eprintln!("line 0x{clip:08X}: {} samples, {} Hz, {} ch, roughness {r:.3}", shipped.len(), pcm.rate, pcm.channels);
        assert!(r < 0.4, "the streamed line decodes as noise: roughness {r}");
        assert_eq!(samples(line.yours.as_ref().unwrap_or_else(|e| panic!("{e}"))), tone().samples);
        assert!(!v.banks.is_empty());
        for b in &v.banks {
            assert!(!b.rows.is_empty() && b.rows.iter().all(|r| row_state(r, false).0 == "replaced"), "{}", b.title);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A replaced bank lists the one cue it declares as replaced and every other cue of the game's
    /// bank as dropped.
    #[test]
    fn a_replaced_bank_names_the_cues_it_drops() {
        let dir = item("bank");
        let guid = a_cue_of("ui_hud");
        let c = Contribution::ReplaceSoundBank { bank: "ui_hud".into(), language: None, category: "ui".into(), cues: vec![cue(format!("0x{guid:08X}"))] };
        let (_, v) = view(&dir, c);
        assert_eq!(v.banks.len(), 2, "ui_hud in vz.wad and shell.wad");
        for b in &v.banks {
            assert!(b.rows.len() > 1, "{}", b.title);
            let replaced: Vec<u32> = b.rows.iter().filter(|r| row_state(r, true).0 == "replaced").map(|r| r.guid).collect();
            assert_eq!(replaced, vec![guid], "{}", b.title);
            let dropped = b.rows.iter().filter(|r| row_state(r, true).0 == "dropped").count();
            assert_eq!(dropped, b.rows.len() - 1, "{}", b.title);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A new bank plays the authored samples from the tables the build ships, and the gameplay loader
    /// loads its wavebank and soundbank.
    #[test]
    fn a_new_bank_plays_the_authored_wave_and_its_loader_loads_it() {
        let dir = item("add");
        let c = Contribution::AddSound { bank: "depot_test_bank".into(), category: "ui".into(), cues: vec![cue("depot_test_cue".into())], load_in: vec![LoadSession::Gameplay] };
        let (s, v) = view(&dir, c);
        let rows = &v.banks[0].rows;
        assert_eq!(rows.len(), 1);
        assert_eq!(row_state(&rows[0], false).0, "new");
        assert_eq!(samples(&rows[0].yours.as_ref().unwrap().waves[0]), tone().samples);
        let b = s.build.as_ref().unwrap();
        let (session, lua) = &b.loaders[0];
        assert_eq!(*session, LoadSession::Gameplay);
        assert!(lua.contains("MrxSoundBanks.LoadWaveBank(\"depot_test_bank\")") && lua.contains("MrxSoundBanks.LoadSoundBank(\"depot_test_bank\")"), "{lua}");
        assert!(b.ships.iter().any(|sh| sh.to == "overlay" && sh.tables.len() == 3));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A loaded soundbank of the game lists its cues, whose waves stream from `music.pws` and decode
    /// as recorded sound; the gameplay loader loads it by name.
    #[test]
    fn a_loaded_bank_plays_its_streamed_waves() {
        let dir = item("load");
        let c = Contribution::LoadSound { soundbank: Some("music".into()), wavebank: None, stream: None, sessions: vec![LoadSession::Gameplay] };
        let (s, v) = view(&dir, c);
        let w = v
            .banks
            .iter()
            .flat_map(|b| &b.rows)
            .flat_map(|r| r.shipped.iter())
            .flat_map(|sd| &sd.waves)
            .find(|w| w.source.as_ref().is_ok_and(|(_, pws)| pws.is_some()))
            .expect("a wave streamed from a .pws");
        let pcm = w.pcm().unwrap_or_else(|e| panic!("{e}"));
        let r = roughness(&pcm.samples, pcm.channels);
        eprintln!("music wave 0x{:08X}: {:.1} s, {} Hz, {} ch, roughness {r:.3}", w.clip.unwrap_or(0), w.seconds, pcm.rate, pcm.channels);
        assert!(r < 0.4, "the streamed music decodes as noise: roughness {r}");
        let lua = &s.build.as_ref().unwrap().loaders[0].1;
        assert!(lua.contains("MrxSoundBanks.LoadSoundBank(\"music\")"), "{lua}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
