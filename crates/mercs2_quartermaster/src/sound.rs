//! Sound banks: `add_sound`, `replace_sound_bank` and `replace_sound_cue` lowered to the bank tables
//! the engine loads (`docs/reverse_engineer/audio_code_map.md` §11).
//!
//! A bank named `N` is three tables — soundbank (ASET type 21), sounddb (13) and wavebank (6) — each a
//! single-`data` UCFX container, all three entries of one block under the name hash `m2(N)` (§11.1).
//! A cue plays only once its bank is loaded, and retail loads banks by name from Lua
//! (`MrxSoundBanks.LoadWaveBank` / `LoadSoundBank`, `mrxsoundbootstrap.lua`, `mrxsound.lua`), so a
//! bank this module mints is registered with the mod loader of each session that loads it
//! ([`sound_registrations`]).
//!
//! * `add_sound` mints a new bank: one wave, one single-wave group and one single-track cue per
//!   authored cue ([`mercs2_audio::encode::encode_bank`]).
//! * `replace_sound_bank` ships a bank's soundbank and sounddb under the bank's own entry name, so the
//!   game's own load of that bank reads them. Its waves live in a wavebank of its own
//!   ([`override_wavebank_name`]), which the mod loader loads: a retail wavebank is shared — a group
//!   may play waves from another bank's wavebank (§11.4) — and the `vo_*` banks have none of their
//!   own at all (their waves stream from `vo_stream`, `mrxsoundbootstrap.lua:218-245`).
//! * `replace_sound_cue` forks the bank's soundbank and rewrites one cue to play a new group whose
//!   wave is in that same wavebank ([`mercs2_audio::encode::retarget_cue`]). The cue keeps its index,
//!   so the bank's own sounddb still routes to it and is not shipped.
//!
//! A `vo_*` bank is per language: retail Lua appends the language to its name before loading it
//! (`_GetLocalizedName`, `mrxsoundbanks.lua:80-87`), so its entry is `m2("<bank>.<language>")` and it
//! ships in that language's patch WAD. Any other bank ships to each level that both carries it and
//! loads it from a retail Lua call site ([`carrier_session`]): `vz.wad`'s gameplay banks to the
//! Shipment overlay, `shell.wad`'s front-end banks to the shell patch. An override's wavebank ships
//! to the level of each session that loads the bank, and that session's loader loads it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_audio::encode::{self, BankSpec, CueParams, CueSpec, GroupParams};
use mercs2_audio::soundbank::{GroupForm, Soundbank};
use mercs2_audio::sounddb::ASSET_TYPE_SOUNDDB;
use mercs2_audio::wave::{WaveRecord, WavebankFile};
use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::patch_wad::{AsetEntry, PatchBlock};
use mercs2_formats::types::{TYPE_HASH_SOUNDBANK, TYPE_HASH_WAVEBANK, TYPE_ID_SOUNDBANK, TYPE_ID_WAVEBANK};
use mercs2_formats::ucfx::{build_wrapped_entries, extract_data_chunk, WrappedEntry};

use crate::discover::LoadedShipment;
use crate::game::GameStack;
use crate::link::{Level, SoundBankRegistration};
use crate::manifest::{Contribution, Language, LoadSession, Manifest, SoundCue};

/// The sounddb's ASET type id (`mercs2_formats::aset_type_ids`: `0xE5273C14 → 13`).
pub const TYPE_ID_SOUNDDB: u32 = 13;

/// Whether retail Lua localizes this bank: `_GetLocalizedName` appends `.<language>` to a name whose
/// first three bytes are `vo_` (`string.sub(sAssetName, 1, 3) == "vo_"`, case-sensitive,
/// `mrxsoundbanks.lua:81-82`).
pub fn is_vo_bank(bank: &str) -> bool {
    bank.starts_with("vo_")
}

/// The entry name a bank's tables are registered under: `<bank>.<language>` for a localized bank,
/// the bank name otherwise.
pub fn entry_name(bank: &str, language: Option<Language>) -> String {
    match language {
        Some(l) => format!("{bank}.{}", l.token()),
        None => bank.to_string(),
    }
}

/// The wavebank that carries a Shipment's override waves for one bank entry. Its name does not start
/// with `vo_`, so the mod loader's `LoadWaveBank` loads it under exactly this name.
pub fn override_wavebank_name(shipment: &str, entry: &str) -> String {
    format!("qm_{shipment}_{entry}")
}

/// A gain in dB as the linear factor the tables carry: `10^(dB/20)`, rounded to `f32`.
pub fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// The PTHS path of the block a bank entry (or an override wavebank) ships in.
pub fn block_path(hash: u32) -> String {
    format!("blocks\\VZ\\mod_{hash:08x}.block")
}

/// The encoder's input for one authored cue: its WAV read through the strict reader, its dB fields
/// as linear gains, and its group and cue fields at their offsets.
pub fn cue_spec(category: &str, cue: &SoundCue, root: &Path) -> Result<CueSpec, String> {
    let path = root.join(&cue.wave);
    let bytes = std::fs::read(&path).map_err(|e| format!("cue {:?}: reading {}: {e}", cue.name, path.display()))?;
    let pcm = mercs2_audio::wav::read_pcm16_wav(&bytes)
        .map_err(|e| format!("cue {:?}: {} is not a usable WAV: {e}", cue.name, path.display()))?;
    let group_gain = db_to_gain(cue.group_gain_db);
    let cue_gain = db_to_gain(cue.cue_gain_db);
    for (field, value) in [
        ("group_gain_db", group_gain),
        ("cue_gain_db", cue_gain),
        ("pitch_semitones", cue.pitch_semitones),
        ("min_distance", cue.min_distance),
        ("max_distance", cue.max_distance),
        ("distance_exponent", cue.distance_exponent),
        ("doppler_scale", cue.doppler_scale),
        ("priority", cue.priority),
        ("group_20", cue.group_20),
    ] {
        if !value.is_finite() {
            return Err(format!(
                "cue {:?}: {field} gives {value}, which is not a finite f32 — write a finite number",
                cue.name
            ));
        }
    }
    Ok(CueSpec {
        name: cue.name.clone(),
        category: category.to_string(),
        sound_id: cue.sound_id,
        clip_hash: cue.clip_hash,
        pcm,
        group: GroupParams {
            unknown_10: cue.priority,
            unknown_14: u32::from(cue.positional),
            min_distance: cue.min_distance,
            max_distance: cue.max_distance,
            unknown_20: cue.group_20,
            distance_exponent: cue.distance_exponent,
            doppler_scale: cue.doppler_scale,
            gain: group_gain,
            unknown_30: cue.pitch_semitones,
            wave_weight: 1.0,
        },
        cue: CueParams { byte_06: cue.start_limit, gain: cue_gain, unknown_16: cue.cue_16 },
    })
}

/// One block of bank tables under one entry hash: a row per table, then the wrapped containers
/// ([`build_wrapped_entries`]), and one primary ASET row per table. A bank has no LOD chain, so both
/// rung halves carry the sentinel.
pub fn bank_block(entry: u32, tables: &[(u32, u32, &[u8])]) -> Result<PatchBlock, String> {
    let wrapped: Vec<WrappedEntry<'_>> = tables
        .iter()
        .map(|&(_, type_hash, payload)| WrappedEntry { name_hash: entry, type_hash, payload })
        .collect();
    let aset = tables
        .iter()
        .map(|&(type_id, _, _)| AsetEntry::new(entry, 0xFFFF_FFFF, 0x0000_FFFF, type_id))
        .collect();
    PatchBlock::from_decompressed(&build_wrapped_entries(&wrapped), block_path(entry), aset, None)
}

/// An `add_sound` bank: the three tables `encode_bank` builds from the cues, in one block of three
/// entries under `m2(bank)` — soundbank (21), sounddb (13), wavebank (6).
pub fn lower_add_sound(
    bank: &str,
    category: &str,
    cues: &[SoundCue],
    root: &Path,
    log: &mut Vec<String>,
) -> Result<PatchBlock, String> {
    let specs = cues.iter().map(|c| cue_spec(category, c, root)).collect::<Result<Vec<_>, _>>()?;
    let spec = BankSpec { name: bank.to_string(), cues: specs };
    let enc = encode::encode_bank(&spec).map_err(|e| format!("bank {bank:?}: {e}"))?;
    log.push(format!(
        "add_sound {bank} 0x{:08X}: {} cue(s); soundbank {} B, sounddb {} B, wavebank {} B",
        enc.bank_hash,
        cues.len(),
        enc.soundbank.len(),
        enc.sounddb.len(),
        enc.wavebank.len()
    ));
    bank_block(
        enc.bank_hash,
        &[
            (TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK, &enc.soundbank),
            (TYPE_ID_SOUNDDB, ASSET_TYPE_SOUNDDB, &enc.sounddb),
            (TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK, &enc.wavebank),
        ],
    )
}

/// The soundbanks the front end loads: `MrxSound.EnterShellState` (`shell/mrxsound.lua:9-14`,
/// called from `shell/mrxguishell.lua:505`) is the only bank-load call site among `shell.wad`'s 28
/// scripts.
pub const FRONT_END_SOUNDBANK_LOADS: &[&str] = &["ui_shell", "ui_hud", "music"];

/// The soundbanks gameplay loads by a literal name: `MrxSoundBootstrap.LoadBanks`
/// (`resident/mrxsoundbootstrap.lua:196-245`), the `LoadSoundBank` calls. `vo_*` banks load per
/// language ([`carrier_session`]).
pub const GAMEPLAY_SOUNDBANK_LOADS: &[&str] = &[
    "ambience",
    "amb_birds",
    "collision_shared",
    "destruction_shared",
    "fol_shared",
    "veh_shared",
    "wpn_shared",
    "building_destruct",
    "veh_support",
    "music",
    "ui_hud",
];

/// Whether `bank` is one of `names`, as the engine compares them: by name hash, which folds case.
fn named_in(bank: &str, names: &[&str]) -> bool {
    names.iter().any(|n| m2(n) == m2(bank))
}

/// Whether the `vz` level loads `bank` only through the front end's code it also carries: a bank of
/// [`FRONT_END_SOUNDBANK_LOADS`] that `LoadBanks` does not load (`ui_shell`). `vz.wad`'s copy of
/// `MrxSound.EnterShellState` (`resident/mrxsound.lua:9-14`) is reached only through
/// `GameBootstrap.Start` (`resident/gamebootstrap.lua:58-75`), which returns at once when
/// `Sys.FinishedShell()` is true (`:43-46`), as it is on the retail path from the main menu into
/// the game.
fn vz_front_end_only(bank: &str) -> bool {
    named_in(bank, FRONT_END_SOUNDBANK_LOADS) && !named_in(bank, GAMEPLAY_SOUNDBANK_LOADS)
}

/// The session in which a level that carries `bank` loads it, or `None` when that level never
/// loads it.
///
/// * [`Carrier::Vz`] — gameplay, except for a bank the `vz` level loads only through the front
///   end's code ([`vz_front_end_only`]). `LoadBanks` names 11 of `vz.wad`'s 76 soundbanks
///   ([`GAMEPLAY_SOUNDBANK_LOADS`]) and the front end's code names `ui_shell`; the other 64 have no
///   literal Lua load site in the corpus, and their overrides load in gameplay.
/// * [`Carrier::Shell`] — the front end, for a bank of [`FRONT_END_SOUNDBANK_LOADS`].
/// * [`Carrier::Language`] — gameplay: the `vo_*` banks load through `LoadBanks`
///   (`resident/mrxsoundbootstrap.lua:219-245`) and through `LoadTempBank` from data tables
///   (`resident/mrxbriefing.lua:510`, `resident/mrxstarter.lua:452`), each localized.
pub fn carrier_session(bank: &str, carrier: Carrier) -> Option<LoadSession> {
    match carrier {
        Carrier::Vz => (!vz_front_end_only(bank)).then_some(LoadSession::Gameplay),
        Carrier::Shell => named_in(bank, FRONT_END_SOUNDBANK_LOADS).then_some(LoadSession::FrontEnd),
        Carrier::Language(_) => Some(LoadSession::Gameplay),
    }
}

/// The sessions that load an override of `bank` when the game carries it in `carriers`: the
/// [`carrier_session`] of each.
pub fn override_sessions(bank: &str, carriers: &BTreeSet<Carrier>) -> BTreeSet<LoadSession> {
    carriers.iter().filter_map(|c| carrier_session(bank, *c)).collect()
}

/// The sessions retail Lua loads `bank` in, for the carriers the game ships it in: `vz.wad` and
/// `shell.wad` for any bank, the language for a `vo_*` one. Needs no game: it is what
/// [`override_sessions`] gives when every level that could carry the bank does.
pub fn retail_sessions(bank: &str, language: Option<Language>) -> BTreeSet<LoadSession> {
    let carriers: BTreeSet<Carrier> = match language {
        Some(l) => BTreeSet::from([Carrier::Language(l)]),
        None => BTreeSet::from([Carrier::Vz, Carrier::Shell]),
    };
    override_sessions(bank, &carriers)
}

/// The carriers of each overridden bank entry (`<bank>` or `<bank>.<language>`), as the game ships
/// them.
pub type CarrierMap = BTreeMap<String, BTreeSet<Carrier>>;

/// Every bank a mod loader must load for this Shipment: each `add_sound` bank (wavebank and
/// soundbank) in its `load_in` sessions, and each override wavebank (wavebank only) in the sessions
/// that load its bank ([`override_sessions`] of the entry's carriers in `carriers`), once.
///
/// Pure: the carriers are the game's answer, looked up by the caller ([`lower_overrides`]). An
/// override entry missing from `carriers`, or one no session loads, is an error.
pub fn sound_registrations(manifest: &Manifest, carriers: &CarrierMap) -> Result<Vec<SoundBankRegistration>, String> {
    let shipment = &manifest.shipment.name;
    let mut out: Vec<SoundBankRegistration> = Vec::new();
    for (index, c) in manifest.contributions.iter().enumerate() {
        let reg = match c {
            Contribution::AddSound { bank, load_in, .. } => SoundBankRegistration {
                shipment: shipment.clone(),
                bank: bank.clone(),
                soundbank: true,
                sessions: load_in.iter().copied().collect(),
            },
            Contribution::ReplaceSoundBank { bank, language, .. } | Contribution::ReplaceSoundCue { bank, language, .. } => {
                let entry = entry_name(bank, *language);
                let found = carriers.get(&entry).ok_or_else(|| {
                    format!("internal error: {shipment} contributions[{index}] overrides {entry:?}, whose carriers were not looked up")
                })?;
                let sessions = override_sessions(bank, found);
                if sessions.is_empty() {
                    return Err(no_session(&entry, found, &format!("{shipment} contributions[{index}] ({})", c.kind())));
                }
                SoundBankRegistration {
                    shipment: shipment.clone(),
                    bank: override_wavebank_name(shipment, &entry),
                    soundbank: false,
                    sessions,
                }
            }
            _ => continue,
        };
        if !out.contains(&reg) {
            out.push(reg);
        }
    }
    Ok(out)
}

/// Why an override of `entry`, which `carriers` carry, loads in no session.
fn no_session(entry: &str, carriers: &BTreeSet<Carrier>, who: &str) -> String {
    format!(
        "[M0218] {who}: {} carries a soundbank named {entry:?}, and no level that carries it loads it \
         in a session — the front end loads {} from shell.wad (shell/mrxsound.lua:9-14), and vz.wad \
         loads {entry:?} only through the front end's code it carries \
         (resident/gamebootstrap.lua:43-46), so an override of it is never heard",
        carriers.iter().map(|c| c.wad()).collect::<Vec<_>>().join(" and "),
        FRONT_END_SOUNDBANK_LOADS.join(", "),
    )
}

/// Check that every bank a loader loads in `level` has its wavebank in `blocks`, the blocks emitted
/// for that level: a `LoadWaveBank` of a name no mounted WAD holds times out after 20 s with no
/// error (`audio_code_map.md` §11.2), so a missing one is refused here, naming the bank and the
/// level.
pub fn check_loader_banks(level: Level, banks: &[&str], blocks: &[&PatchBlock]) -> Result<(), String> {
    for bank in banks {
        let hash = m2(bank);
        let shipped = blocks
            .iter()
            .any(|b| b.aset_entries.iter().any(|r| r.asset_hash == hash && r.u32_3 == TYPE_ID_WAVEBANK));
        if !shipped {
            return Err(format!(
                "internal error: the {} loader loads the wavebank {bank:?} (0x{hash:08X}), and no block \
                 emitted for {} carries it",
                match level {
                    Level::Vz => "gameplay",
                    Level::Shell => "front-end",
                },
                level.wad()
            ));
        }
    }
    Ok(())
}

/// Every sound block one Shipment's build emits, by where it ships, and the Shipment's sound
/// registrations: each `add_sound` bank's block ([`lower_add_sound`]) in the WAD of each session in
/// its `load_in`, and every override ([`lower_overrides`], [`OverrideScope::Shipment`]). `qm build`
/// ships exactly these; `qm link` lowers each Shipment of the set through here too, to check the
/// linked loaders against what the set's builds ship.
///
/// An override needs the game (`game`); without it the error names the contribution. An error
/// comes back with the index and kind of the contribution it is about.
pub fn lower_shipment_sound(
    s: &LoadedShipment,
    game: Option<&mut GameStack>,
    log: &mut Vec<String>,
) -> Result<OverrideBlocks, (usize, &'static str, String)> {
    let manifest = &s.manifest;
    let mut out = match manifest
        .contributions
        .iter()
        .enumerate()
        .find(|(_, c)| matches!(c, Contribution::ReplaceSoundBank { .. } | Contribution::ReplaceSoundCue { .. }))
    {
        Some((index, c)) => {
            let Some(game) = game else {
                return Err((index, c.kind(), "a sound override reads the bank it overrides from the game".into()));
            };
            lower_overrides(&[s], game, OverrideScope::Shipment, log).map_err(|m| (index, c.kind(), m))?
        }
        None => OverrideBlocks {
            registrations: sound_registrations(manifest, &CarrierMap::new()).map_err(|m| (0, "add_sound", m))?,
            ..OverrideBlocks::default()
        },
    };
    for (index, c) in manifest.contributions.iter().enumerate() {
        let Contribution::AddSound { bank, category, cues, load_in } = c else { continue };
        let block = lower_add_sound(bank, category, cues, &s.root, log).map_err(|m| (index, c.kind(), m))?;
        let sessions: BTreeSet<LoadSession> = load_in.iter().copied().collect();
        for session in &sessions {
            match Level::of(*session) {
                Level::Vz => out.overlay.push(block.clone()),
                Level::Shell => out.shell.push(block.clone()),
            }
        }
    }
    Ok(out)
}

/// The entry hash of every bank a `replace_sound_cue` in the set targets: `qm link` merges each into
/// one soundbank for the whole set, at [`block_path`] of the entry.
pub fn linked_sound_entries<'a>(manifests: impl IntoIterator<Item = &'a Manifest>) -> BTreeSet<u32> {
    manifests
        .into_iter()
        .flat_map(|m| m.contributions.iter())
        .filter_map(|c| match c {
            Contribution::ReplaceSoundCue { bank, language, .. } => Some(m2(&entry_name(bank, *language))),
            _ => None,
        })
        .collect()
}

/// Where a bank entry's override tables ship, and what the loaders load.
#[derive(Debug, Default)]
pub struct OverrideBlocks {
    /// The Shipment overlay (or, for the link, the link WAD): the gameplay banks `vz.wad` carries,
    /// and every override wavebank gameplay loads.
    pub overlay: Vec<PatchBlock>,
    /// The shell patch: the front-end banks `shell.wad` carries, and every override wavebank the
    /// front end loads.
    pub shell: Vec<PatchBlock>,
    /// Each language's patch: that language's `vo_*` banks.
    pub language: BTreeMap<Language, Vec<PatchBlock>>,
    /// Every Shipment's sound registrations, in `shipments` order: [`sound_registrations`] over the
    /// carriers found here.
    pub registrations: Vec<SoundBankRegistration>,
}

/// Which output [`lower_overrides`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideScope {
    /// One Shipment's build: every overridden bank's tables and the Shipment's override wavebanks.
    Shipment,
    /// `qm link` over the set: the one soundbank per bank that carries every `replace_sound_cue` of
    /// the set (with the replacement's sounddb when a `replace_sound_bank` replaces the bank).
    Link,
}

/// A game WAD beside the configured `vz.wad`, matched case-insensitively (`shell.wad`,
/// `English.wad`).
pub fn sibling_wad(vz: &Path, file: &str) -> Result<PathBuf, String> {
    let dir = vz.parent().ok_or_else(|| format!("{} has no parent folder", vz.display()))?;
    let entries = std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))?;
    for e in entries {
        let e = e.map_err(|e| format!("reading {}: {e}", dir.display()))?;
        if e.file_name().to_string_lossy().eq_ignore_ascii_case(file) {
            return Ok(e.path());
        }
    }
    Err(format!("{file} is not beside {} (looked in {})", vz.display(), dir.display()))
}

/// One override of one bank entry, in set order.
struct Op<'a> {
    shipment: &'a str,
    who: String,
    kind: OpKind,
    /// The cues' encoder input, each with its index in the Shipment's override wavebank.
    specs: Vec<(CueSpec, u32)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Bank,
    Cue,
}

/// Everything the set does to one bank entry.
struct EntryOps<'a> {
    bank: &'a str,
    language: Option<Language>,
    ops: Vec<Op<'a>>,
}

/// The soundbank a carrier WAD holds for `entry`, parsed.
fn carrier_soundbank(stack: &mut GameStack, entry: u32) -> Result<Option<Soundbank>, String> {
    if !stack.has_asset(entry, TYPE_ID_SOUNDBANK) {
        return Ok(None);
    }
    let container = stack
        .container_for_asset(entry, TYPE_HASH_SOUNDBANK, TYPE_ID_SOUNDBANK)
        .ok_or_else(|| format!("soundbank 0x{entry:08X} has an ASET row but its block does not read"))?;
    let body = extract_data_chunk(&container)
        .ok_or_else(|| format!("soundbank 0x{entry:08X}: its container has no data chunk"))?;
    Soundbank::parse(&body).map(Some).map_err(|e| format!("soundbank 0x{entry:08X}: {e}"))
}

/// Lower every `replace_sound_bank` and `replace_sound_cue` of `shipments` (in set order), for a
/// Shipment's own build or for the link ([`OverrideScope`]).
///
/// First, for every overridden bank entry, its carriers: the language for a `vo_*` bank; for any
/// other bank each of `vz.wad` (`game`) and `shell.wad` (opened beside it,
/// [`GameStack::open_sibling`]) that carries it. A bank no carrier has is an error, and so is one
/// no carrier loads from a retail Lua call site ([`carrier_session`]). The registrations of every
/// Shipment follow from them ([`sound_registrations`]).
///
/// Each Shipment's override waves for one bank entry form one wavebank
/// ([`override_wavebank_name`]), in contribution order. A bank's base soundbank is the
/// `replace_sound_bank` replacement when the set has one, the game's otherwise; every
/// `replace_sound_cue` on it is then applied in set order ([`encode::retarget_cue`]). The tables go
/// to each carrier that loads the bank; the Shipment scope's wavebanks go to the level of each
/// session that loads the bank (gameplay: the overlay; the front end: the shell patch). A cue its
/// bank does not have is an error.
pub fn lower_overrides(
    shipments: &[&LoadedShipment],
    game: &mut GameStack,
    scope: OverrideScope,
    log: &mut Vec<String>,
) -> Result<OverrideBlocks, String> {
    let mut entries: BTreeMap<String, EntryOps<'_>> = BTreeMap::new();
    let mut wavebanks: BTreeMap<(String, String), Vec<WaveRecord>> = BTreeMap::new();
    for s in shipments {
        let shipment = s.manifest.shipment.name.as_str();
        for (index, c) in s.manifest.contributions.iter().enumerate() {
            let (bank, language, category, cues, kind): (&str, Option<Language>, &str, Vec<&SoundCue>, OpKind) = match c {
                Contribution::ReplaceSoundBank { bank, language, category, cues, .. } => {
                    (bank, *language, category, cues.iter().collect(), OpKind::Bank)
                }
                Contribution::ReplaceSoundCue { bank, language, category, cue } => {
                    (bank, *language, category, vec![cue], OpKind::Cue)
                }
                _ => continue,
            };
            let who = format!("{shipment} contributions[{index}] ({})", c.kind());
            if is_vo_bank(bank) != language.is_some() {
                return Err(format!(
                    "{who}: bank {bank:?} {} — a `vo_*` bank is replaced per language, any other bank \
                     has none",
                    if language.is_some() { "is not a vo_* bank but declares a language" } else { "is a vo_* bank and declares no language" }
                ));
            }
            let entry = entry_name(bank, language);
            let records = wavebanks.entry((shipment.to_string(), entry.clone())).or_default();
            let mut specs = Vec::with_capacity(cues.len());
            for cue in cues {
                let spec = cue_spec(category, cue, &s.root).map_err(|m| format!("{who}: {m}"))?;
                let record = encode::wave_record(&format!("cue {:?}", cue.name), spec.clip_hash, &spec.pcm)
                    .map_err(|e| format!("{who}: {e}"))?;
                specs.push((spec, records.len() as u32));
                records.push(record);
            }
            entries
                .entry(entry)
                .or_insert_with(|| EntryOps { bank, language, ops: Vec::new() })
                .ops
                .push(Op { shipment, who, kind, specs });
        }
    }

    // The carriers of every entry, each with the soundbank it carries, whatever the scope.
    let mut shell: Option<GameStack> = None;
    let mut carried: BTreeMap<&str, Vec<(Carrier, Soundbank)>> = BTreeMap::new();
    for (entry, e) in &entries {
        let entry_hash = m2(entry);
        let mut found: Vec<(Carrier, Soundbank)> = Vec::new();
        if let Some(language) = e.language {
            let base = carrier_soundbank(game, entry_hash)?.ok_or_else(|| missing(entry, &e.ops))?;
            found.push((Carrier::Language(language), base));
        } else {
            if let Some(base) = carrier_soundbank(game, entry_hash)? {
                found.push((Carrier::Vz, base));
            }
            if shell.is_none() {
                shell = Some(game.open_sibling("shell.wad")?);
            }
            let stack = shell.as_mut().ok_or("internal error: shell.wad was not opened")?;
            if let Some(base) = carrier_soundbank(stack, entry_hash)? {
                found.push((Carrier::Shell, base));
            }
            if found.is_empty() {
                return Err(missing(entry, &e.ops));
            }
        }
        carried.insert(entry.as_str(), found);
    }
    let carriers: CarrierMap = carried
        .iter()
        .map(|(entry, found)| (entry.to_string(), found.iter().map(|(c, _)| *c).collect()))
        .collect();

    let mut out = OverrideBlocks::default();
    for s in shipments {
        out.registrations.extend(sound_registrations(&s.manifest, &carriers)?);
    }

    for (entry, e) in &entries {
        let entry_hash = m2(entry);
        let banks: Vec<&Op<'_>> = e.ops.iter().filter(|o| o.kind == OpKind::Bank).collect();
        if banks.len() > 1 {
            return Err(format!(
                "bank {entry:?} is replaced by {} — one replacement can take effect",
                banks.iter().map(|o| o.who.as_str()).collect::<Vec<_>>().join(" and ")
            ));
        }
        // Only the carriers that load the bank get its tables.
        let loading: Vec<(Carrier, Soundbank)> = carried[entry.as_str()]
            .iter()
            .filter(|(c, _)| carrier_session(e.bank, *c).is_some())
            .cloned()
            .collect();
        if loading.is_empty() {
            let who = e.ops.iter().map(|o| o.who.as_str()).collect::<Vec<_>>().join(" and ");
            return Err(no_session(entry, &carriers[entry], &who));
        }
        let has_cue = e.ops.iter().any(|o| o.kind == OpKind::Cue);
        if scope == OverrideScope::Link && !has_cue {
            continue;
        }
        // The replacement's own tables, its waves in the replacing Shipment's override wavebank.
        let replacement = match banks.first() {
            Some(op) => {
                let spec = BankSpec { name: e.bank.to_string(), cues: op.specs.iter().map(|(s, _)| s.clone()).collect() };
                let mut tables = encode::build_tables(&spec).map_err(|err| format!("{}: {err}", op.who))?;
                let wavebank = m2(&override_wavebank_name(op.shipment, entry));
                for (group, (_, index)) in tables.soundbank.groups.iter_mut().zip(&op.specs) {
                    match &mut group.form {
                        GroupForm::Single { wave, .. } => {
                            wave.wavebank = wavebank;
                            wave.index = *index;
                        }
                        GroupForm::Multi(_) => {
                            return Err(format!("{}: the encoder built a multi-wave group for a single-wave cue", op.who))
                        }
                    }
                }
                let sounddb = tables.sounddb.to_bytes().map_err(|err| format!("{}: {err}", op.who))?;
                Some((tables.soundbank, sounddb))
            }
            None => None,
        };

        for (carrier, retail) in loading {
            let mut soundbank = match &replacement {
                Some((sb, _)) => sb.clone(),
                None => retail,
            };
            for op in e.ops.iter().filter(|o| o.kind == OpKind::Cue) {
                let wavebank = m2(&override_wavebank_name(op.shipment, entry));
                for (spec, index) in &op.specs {
                    encode::retarget_cue(&mut soundbank, spec, wavebank, *index).map_err(|err| format!("{}: {err}", op.who))?;
                }
            }
            let sb = soundbank.to_bytes().map_err(|err| format!("bank {entry:?}: {err}"))?;
            let mut tables: Vec<(u32, u32, &[u8])> = vec![(TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK, &sb)];
            if let Some((_, db)) = &replacement {
                tables.push((TYPE_ID_SOUNDDB, ASSET_TYPE_SOUNDDB, db));
            }
            let block = bank_block(entry_hash, &tables).map_err(|err| format!("bank {entry:?}: {err}"))?;
            log.push(format!(
                "sound bank {entry} 0x{entry_hash:08X} → {}: {} op(s) from {}, soundbank {} B{}",
                carrier.label(),
                e.ops.len(),
                e.ops.iter().map(|o| o.shipment).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "),
                sb.len(),
                if replacement.is_some() { " + the replacement's sounddb" } else { "" }
            ));
            match carrier {
                Carrier::Vz => out.overlay.push(block),
                Carrier::Shell => out.shell.push(block),
                Carrier::Language(l) => out.language.entry(l).or_default().push(block),
            }
        }
    }

    if scope == OverrideScope::Shipment {
        for ((shipment, entry), records) in wavebanks {
            let name = override_wavebank_name(&shipment, &entry);
            let hash = m2(&name);
            let body = WavebankFile { bank_hash: hash, stream_name: None, records }
                .to_bytes()
                .map_err(|err| format!("wavebank {name:?}: {err}"))?;
            let sessions = override_sessions(entries[&entry].bank, &carriers[&entry]);
            for session in &sessions {
                let block = bank_block(hash, &[(TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK, &body)])
                    .map_err(|err| format!("wavebank {name:?}: {err}"))?;
                match Level::of(*session) {
                    Level::Vz => out.overlay.push(block),
                    Level::Shell => out.shell.push(block),
                }
            }
            log.push(format!(
                "override wavebank {name} 0x{hash:08X}: {} B → {}",
                body.len(),
                sessions.iter().map(|s| Level::of(*s).wad()).collect::<Vec<_>>().join(" and ")
            ));
        }
    }
    Ok(out)
}

/// A WAD that carries a bank's tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Carrier {
    /// `vz.wad`, the gameplay level.
    Vz,
    /// `shell.wad`, the front end.
    Shell,
    /// A language's WAD: its `vo_*` banks.
    Language(Language),
}

impl Carrier {
    fn label(self) -> String {
        match self {
            Carrier::Vz => "overlay (vz.wad carries it)".into(),
            Carrier::Shell => "shell patch (shell.wad carries it)".into(),
            Carrier::Language(l) => format!("{}-patch", l.token()),
        }
    }

    /// The WAD's file name, for messages.
    pub fn wad(self) -> String {
        match self {
            Carrier::Vz => "vz.wad".into(),
            Carrier::Shell => "shell.wad".into(),
            Carrier::Language(l) => format!("{}.wad", l.token()),
        }
    }
}

fn missing(entry: &str, ops: &[Op<'_>]) -> String {
    format!(
        "[M0218] no game WAD carries a soundbank named {entry:?} (0x{:08X}), which {} overrides — \
         check the bank name (as the game's Lua loads it) and, for a vo_* bank, the language",
        m2(entry),
        ops.iter().map(|o| o.who.as_str()).collect::<Vec<_>>().join(" and ")
    )
}
/// Every cue guid the game's sound tables route: the cue entries of every sounddb in `game`.
pub fn game_cue_guids(game: &mut GameStack) -> Result<BTreeSet<u32>, String> {
    let mut guids = BTreeSet::new();
    for hash in game.asset_hashes(TYPE_ID_SOUNDDB) {
        let container = game
            .container_for_asset(hash, ASSET_TYPE_SOUNDDB, TYPE_ID_SOUNDDB)
            .ok_or_else(|| format!("sounddb 0x{hash:08X} has an ASET row but its block does not read"))?;
        let body = extract_data_chunk(&container)
            .ok_or_else(|| format!("sounddb 0x{hash:08X}: its container has no data chunk"))?;
        let db = mercs2_audio::sounddb::SoundDb::parse(&body).map_err(|e| format!("sounddb 0x{hash:08X}: {e}"))?;
        guids.extend(db.cues.iter().map(|c| c.guid));
    }
    Ok(guids)
}

/// Every cue guid the game routes in any language it has installed: the sounddbs of `game`, and of
/// each language WAD (`<token>.wad`, [`Language::ALL`]) in `vz.wad`'s folder that `game` does not
/// hold. Retail Lua loads the `vo_*` banks of the running language at boot
/// (`mrxsoundbootstrap.lua:219-245`), before the mod loader runs, so their cues answer first too.
pub fn installed_cue_guids(game: &mut GameStack) -> Result<BTreeSet<u32>, String> {
    let mut guids = game_cue_guids(game)?;
    let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
    let open: Vec<PathBuf> = game.paths().iter().map(|p| p.to_path_buf()).collect();
    let dir = vz.parent().ok_or_else(|| format!("{} has no parent folder", vz.display()))?;
    let mut files = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| format!("reading {}: {e}", dir.display()))? {
        files.push(e.map_err(|e| format!("reading {}: {e}", dir.display()))?.path());
    }
    for language in Language::ALL {
        let file = format!("{}.wad", language.token());
        let Some(path) = files
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&file)))
        else {
            continue;
        };
        if open.contains(path) {
            continue;
        }
        let mut stack = GameStack::open(std::slice::from_ref(path)).map_err(|e| e.to_string())?;
        guids.extend(game_cue_guids(&mut stack)?);
    }
    Ok(guids)
}

/// For each `replace_sound_bank` / `replace_sound_cue` of `manifest`, why its target is not in the
/// game: the bank is in no carrier (`game`, which holds the declared languages' WADs, and
/// `shell.wad` beside it), no carrier loads it from a retail Lua call site ([`carrier_session`]), or
/// the cue is not in the bank. A cue the same Shipment's `replace_sound_bank` declares for that bank
/// is in it.
pub fn override_target_problems(manifest: &Manifest, game: &mut GameStack) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut shell: Option<Result<GameStack, String>> = None;
    for (index, c) in manifest.contributions.iter().enumerate() {
        let (bank, language, cue) = match c {
            Contribution::ReplaceSoundBank { bank, language, .. } => (bank, *language, None),
            Contribution::ReplaceSoundCue { bank, language, cue, .. } => (bank, *language, Some(&cue.name)),
            _ => continue,
        };
        let entry = entry_name(bank, language);
        let hash = m2(&entry);
        let first = match language {
            Some(l) => Carrier::Language(l),
            None => Carrier::Vz,
        };
        let mut carriers: Vec<(Carrier, Result<Option<Soundbank>, String>)> = vec![(first, carrier_soundbank(game, hash))];
        if language.is_none() {
            let shell = shell.get_or_insert_with(|| game.open_sibling("shell.wad"));
            match shell {
                Ok(stack) => carriers.push((Carrier::Shell, carrier_soundbank(stack, hash))),
                Err(e) => {
                    out.push((index, format!("the front end's shell.wad cannot be read, so {bank:?} cannot be looked up in it: {e}")));
                    continue;
                }
            }
        }
        let mut found: Vec<Soundbank> = Vec::new();
        let mut found_in: BTreeSet<Carrier> = BTreeSet::new();
        for (carrier, soundbank) in carriers {
            match soundbank {
                Ok(Some(sb)) => {
                    found.push(sb);
                    found_in.insert(carrier);
                }
                Ok(None) => {}
                Err(e) => out.push((index, format!("bank {entry:?}: {e}"))),
            }
        }
        if found.is_empty() {
            out.push((
                index,
                format!(
                    "no game WAD carries a soundbank named {entry:?} (0x{hash:08X}) — check the bank \
                     name as the game's Lua loads it{}",
                    if language.is_some() { ", and the language" } else { "" }
                ),
            ));
            continue;
        }
        if override_sessions(bank, &found_in).is_empty() {
            let who = format!("contributions[{index}] ({})", c.kind());
            out.push((index, no_session(&entry, &found_in, &who).replacen("[M0218] ", "", 1)));
            continue;
        }
        let Some(cue) = cue else { continue };
        let guid = m2(cue);
        let replaced = manifest.contributions.iter().any(|other| match other {
            Contribution::ReplaceSoundBank { bank: b, language: l, cues, .. } => {
                entry_name(b, *l) == entry && cues.iter().any(|c| m2(&c.name) == guid)
            }
            _ => false,
        });
        if !replaced && found.iter().any(|sb| !sb.cues.iter().any(|c| c.guid == guid)) {
            out.push((
                index,
                format!(
                    "bank {entry:?} has no cue {cue:?} (0x{guid:08X}) — replace_sound_cue rewrites a \
                     cue the bank has; add a new cue with add_sound"
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// −4 dB and −6 dB give the gains retail `ui_PDA_Open_01_st` carries (group 70 and cue 57 of
    /// `ui_hud`), bit for bit.
    #[test]
    fn db_gains_reproduce_the_retail_bit_patterns() {
        assert_eq!(db_to_gain(-4.0).to_bits(), 0x3F21_866C);
        assert_eq!(db_to_gain(-6.0).to_bits(), 0x3F00_4DCE);
        assert_eq!(db_to_gain(0.0), 1.0);
    }

    #[test]
    fn voice_over_banks_are_named_per_language() {
        assert!(is_vo_bank("vo_mattias"));
        assert!(!is_vo_bank("VO_mattias"), "the Lua prefix test is case-sensitive");
        assert!(!is_vo_bank("ui_hud"));
        assert_eq!(entry_name("vo_mattias", Some(Language::English)), "vo_mattias.english");
        assert_eq!(entry_name("ui_hud", None), "ui_hud");
        assert_eq!(override_wavebank_name("my-mod", "vo_mattias.french"), "qm_my-mod_vo_mattias.french");
        assert!(!is_vo_bank(&override_wavebank_name("my-mod", "vo_mattias.french")));
    }

    fn manifest(contributions: &str) -> Manifest {
        crate::from_str(
            &format!("format: 2\nshipment: {{ name: s, version: 1.0.0, target: retail }}\ncontributions:\n{contributions}"),
            crate::Format::Yaml,
        )
        .expect("parses")
    }

    fn cue(indent: &str, name: &str) -> String {
        format!(
            "{indent}name: {name}\n{indent}wave: src/a.wav\n{indent}group_gain_db: 0\n{indent}cue_gain_db: 0\n\
             {indent}pitch_semitones: 0\n{indent}positional: false\n{indent}min_distance: 1\n{indent}max_distance: 2\n\
             {indent}distance_exponent: 1\n{indent}doppler_scale: 1\n{indent}start_limit: 0\n{indent}sound_id: 0\n\
             {indent}priority: 1\n{indent}group_20: 1\n{indent}cue_16: 0\n{indent}clip_hash: 0\n"
        )
    }

    fn carriers(rows: &[(&str, &[Carrier])]) -> CarrierMap {
        rows.iter().map(|(e, c)| (e.to_string(), c.iter().copied().collect())).collect()
    }

    fn sessions(list: &[LoadSession]) -> BTreeSet<LoadSession> {
        list.iter().copied().collect()
    }

    /// An added bank loads whole in its `load_in` sessions; the override wavebanks load once per
    /// bank entry, however many overrides share it, in the sessions their carriers load the bank
    /// in; and the link merges exactly the banks a replace_sound_cue targets.
    #[test]
    fn registrations_and_linked_entries() {
        let added = cue("        ", "mod_click");
        let m = manifest(&format!(
            "  - kind: add_sound\n    bank: mod_sounds\n    category: ui\n    load_in: [front_end, gameplay]\n    cues:\n      - {}\
             \x20 - kind: replace_sound_cue\n    bank: ui_hud\n    category: ui\n    cue:\n{}\
             \x20 - kind: replace_sound_cue\n    bank: ui_hud\n    category: ui\n    cue:\n{}\
             \x20 - kind: replace_sound_cue\n    bank: ui_shell\n    category: ui\n    cue:\n{}\
             \x20 - kind: replace_sound_bank\n    bank: vo_mattias\n    language: german\n    category: vo\n    cues:\n      - {}",
            &added[8..],
            cue("      ", "ui_PDA_Open_01_st"),
            cue("      ", "ui_PDA_Close_01_st"),
            cue("      ", "ui_shell_click"),
            &cue("        ", "line")[8..],
        ));
        let found = carriers(&[
            ("ui_hud", &[Carrier::Vz, Carrier::Shell]),
            ("ui_shell", &[Carrier::Vz, Carrier::Shell]),
            ("vo_mattias.german", &[Carrier::Language(Language::German)]),
        ]);
        let regs: Vec<(String, bool, BTreeSet<LoadSession>)> =
            sound_registrations(&m, &found).unwrap().into_iter().map(|r| (r.bank, r.soundbank, r.sessions)).collect();
        let both = sessions(&LoadSession::ALL);
        assert_eq!(
            regs,
            vec![
                ("mod_sounds".to_string(), true, both.clone()),
                ("qm_s_ui_hud".to_string(), false, both),
                ("qm_s_ui_shell".to_string(), false, sessions(&[LoadSession::FrontEnd])),
                ("qm_s_vo_mattias.german".to_string(), false, sessions(&[LoadSession::Gameplay])),
            ]
        );
        assert_eq!(linked_sound_entries([&m]), BTreeSet::from([m2("ui_hud"), m2("ui_shell")]));

        // An override whose carriers were not looked up is an internal error.
        let partial = carriers(&[("ui_hud", &[Carrier::Vz, Carrier::Shell])]);
        assert!(sound_registrations(&m, &partial).unwrap_err().contains("internal error"));
    }

    /// Each carrier gives its level's session: the front end loads `ui_shell`, `ui_hud` and `music`
    /// from `shell.wad`; gameplay loads `vz.wad`'s banks but `ui_shell`, which `vz.wad` loads only
    /// through the front end's code; a language loads its `vo_*` banks in gameplay. A bank no
    /// carrier loads has no session, and its registration is an M0218 error. Names compare by hash,
    /// so case is folded.
    #[test]
    fn sessions_follow_the_retail_load_sites() {
        let vz_shell: BTreeSet<Carrier> = [Carrier::Vz, Carrier::Shell].into();
        assert_eq!(override_sessions("ui_hud", &vz_shell), sessions(&LoadSession::ALL));
        assert_eq!(override_sessions("music", &vz_shell), sessions(&LoadSession::ALL));
        assert_eq!(override_sessions("ui_shell", &vz_shell), sessions(&[LoadSession::FrontEnd]));
        assert_eq!(override_sessions("UI_Shell", &vz_shell), sessions(&[LoadSession::FrontEnd]));
        assert_eq!(override_sessions("wpn_shared", &[Carrier::Vz].into()), sessions(&[LoadSession::Gameplay]));
        assert_eq!(override_sessions("ui_hud", &[Carrier::Vz].into()), sessions(&[LoadSession::Gameplay]));
        assert_eq!(override_sessions("ui_hud", &[Carrier::Shell].into()), sessions(&[LoadSession::FrontEnd]));
        assert_eq!(override_sessions("veh_tank", &[Carrier::Vz].into()), sessions(&[LoadSession::Gameplay]));
        assert!(override_sessions("ui_shell", &[Carrier::Vz].into()).is_empty(), "vz.wad's ui_shell is the front end's");
        assert_eq!(
            override_sessions("vo_mattias", &[Carrier::Language(Language::French)].into()),
            sessions(&[LoadSession::Gameplay])
        );
        assert_eq!(retail_sessions("ui_shell", None), sessions(&[LoadSession::FrontEnd]));
        assert_eq!(retail_sessions("ui_hud", None), sessions(&LoadSession::ALL));
        assert_eq!(retail_sessions("vo_mattias", Some(Language::English)), sessions(&[LoadSession::Gameplay]));

        let m = manifest(&format!(
            "  - kind: replace_sound_cue\n    bank: ui_shell\n    category: ui\n    cue:\n{}",
            cue("      ", "x")
        ));
        let err = sound_registrations(&m, &carriers(&[("ui_shell", &[Carrier::Vz])])).unwrap_err();
        assert!(err.starts_with("[M0218]") && err.contains("ui_shell") && err.contains("vz.wad"), "{err}");
    }

    /// A block with one row of `type_id` for `name`; the check reads rows, not the table.
    fn row_block(name: &str, type_id: u32, type_hash: u32) -> PatchBlock {
        bank_block(m2(name), &[(type_id, type_hash, b"table")]).unwrap()
    }

    /// The loader self-check: every bank a level's loader loads needs its wavebank among the blocks
    /// emitted for that level; a missing one names the bank and the level. A soundbank row of the
    /// same name is not a wavebank.
    #[test]
    fn the_missing_wavebank_self_check_fails() {
        let a = row_block("qm_s_ui_hud", TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK);
        let sb = row_block("qm_s_ui_shell", TYPE_ID_SOUNDBANK, TYPE_HASH_SOUNDBANK);
        check_loader_banks(Level::Vz, &["qm_s_ui_hud"], &[&a]).expect("present");
        check_loader_banks(Level::Shell, &[], &[]).expect("nothing to load");
        let err = check_loader_banks(Level::Shell, &["qm_s_ui_hud", "qm_s_ui_shell"], &[&a, &sb]).unwrap_err();
        assert!(err.contains("\"qm_s_ui_shell\"") && err.contains("shell.wad") && err.contains("front-end"), "{err}");
    }
}
