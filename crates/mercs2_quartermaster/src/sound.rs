//! Sound banks: `add_sound`, `replace_sound_bank` and `replace_sound_cue` lowered to the bank tables
//! the engine loads (`docs/reverse_engineer/audio_code_map.md` §11).
//!
//! A bank named `N` is three tables — soundbank (ASET type 21), sounddb (13) and wavebank (6) — each a
//! single-`data` UCFX container, all three entries of one block under the name hash `m2(N)` (§11.1).
//! A cue plays only once its bank is loaded, and retail loads banks by name from Lua
//! (`MrxSoundBanks.LoadWaveBank` / `LoadSoundBank`, `mrxsoundbootstrap.lua`), so a bank this module
//! mints is registered with the mod loader ([`sound_registrations`]).
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
//! ships in that language's patch WAD. Any other bank ships where the game carries it: `vz.wad`'s go
//! to the Shipment overlay, `shell.wad`'s (the front end) to the shell patch — both, when both carry
//! it.

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
use crate::link::SoundBankRegistration;
use crate::manifest::{Contribution, Language, Manifest, SoundCue};

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

/// Every bank the mod loader must load for this Shipment: each `add_sound` bank (wavebank and
/// soundbank), and each override wavebank (wavebank only), once.
pub fn sound_registrations(manifest: &Manifest) -> Vec<SoundBankRegistration> {
    let shipment = &manifest.shipment.name;
    let mut out: Vec<SoundBankRegistration> = Vec::new();
    for c in &manifest.contributions {
        let reg = match c {
            Contribution::AddSound { bank, .. } => {
                SoundBankRegistration { shipment: shipment.clone(), bank: bank.clone(), soundbank: true }
            }
            Contribution::ReplaceSoundBank { bank, language, .. } | Contribution::ReplaceSoundCue { bank, language, .. } => {
                SoundBankRegistration {
                    shipment: shipment.clone(),
                    bank: override_wavebank_name(shipment, &entry_name(bank, *language)),
                    soundbank: false,
                }
            }
            _ => continue,
        };
        if !out.contains(&reg) {
            out.push(reg);
        }
    }
    out
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

/// Where a bank entry's override tables ship.
#[derive(Debug, Default)]
pub struct OverrideBlocks {
    /// The Shipment overlay (or, for the link, the link WAD): `vz.wad`'s banks and every override
    /// wavebank.
    pub overlay: Vec<PatchBlock>,
    /// The shell patch: banks `shell.wad` carries.
    pub shell: Vec<PatchBlock>,
    /// Each language's patch: that language's `vo_*` banks.
    pub language: BTreeMap<Language, Vec<PatchBlock>>,
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
/// Each Shipment's override waves for one bank entry form one wavebank
/// ([`override_wavebank_name`]), in contribution order. A bank's base soundbank is the
/// `replace_sound_bank` replacement when the set has one, the game's otherwise; every
/// `replace_sound_cue` on it is then applied in set order ([`encode::retarget_cue`]). A `vo_*` bank
/// is read from, and ships to, its language; any other bank from and to each of `vz.wad` (`game`)
/// and `shell.wad` that carries it. A bank no carrier has, or a cue its bank does not have, is an
/// error.
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
                Contribution::ReplaceSoundBank { bank, language, category, cues } => {
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

    let mut out = OverrideBlocks::default();
    let mut shell: Option<GameStack> = None;
    for (entry, e) in &entries {
        let entry_hash = m2(entry);
        let banks: Vec<&Op<'_>> = e.ops.iter().filter(|o| o.kind == OpKind::Bank).collect();
        if banks.len() > 1 {
            return Err(format!(
                "bank {entry:?} is replaced by {} — one replacement can take effect",
                banks.iter().map(|o| o.who.as_str()).collect::<Vec<_>>().join(" and ")
            ));
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

        // The carriers: the language for a vo_* bank; `vz.wad` and `shell.wad` for any other.
        let mut carriers: Vec<(Carrier, Soundbank)> = Vec::new();
        if let Some(language) = e.language {
            let base = carrier_soundbank(game, entry_hash)?.ok_or_else(|| missing(entry, &e.ops))?;
            carriers.push((Carrier::Language(language), base));
        } else {
            if let Some(base) = carrier_soundbank(game, entry_hash)? {
                carriers.push((Carrier::Vz, base));
            }
            if shell.is_none() {
                let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
                let path = sibling_wad(&vz, "shell.wad")?;
                shell = Some(GameStack::open(&[path]).map_err(|err| err.to_string())?);
            }
            if let Some(shell) = shell.as_mut() {
                if let Some(base) = carrier_soundbank(shell, entry_hash)? {
                    carriers.push((Carrier::Shell, base));
                }
            }
            if carriers.is_empty() {
                return Err(missing(entry, &e.ops));
            }
        }

        for (carrier, retail) in carriers {
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
            log.push(format!("override wavebank {name} 0x{hash:08X}: {} B", body.len()));
            out.overlay
                .push(bank_block(hash, &[(TYPE_ID_WAVEBANK, TYPE_HASH_WAVEBANK, &body)]).map_err(|err| format!("wavebank {name:?}: {err}"))?);
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
enum Carrier {
    Vz,
    Shell,
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

/// For each `replace_sound_bank` / `replace_sound_cue` of `manifest`, why its target is not in the
/// game: the bank is in no carrier (`game`, which holds the declared languages' WADs, and
/// `shell.wad` beside it), or the cue is not in the bank. A cue the same Shipment's
/// `replace_sound_bank` declares for that bank is in it.
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
        let mut carriers: Vec<Result<Option<Soundbank>, String>> = vec![carrier_soundbank(game, hash)];
        if language.is_none() {
            let shell = shell.get_or_insert_with(|| {
                let vz = game.paths().first().map(|p| p.to_path_buf()).ok_or("the game stack is empty")?;
                let path = sibling_wad(&vz, "shell.wad")?;
                GameStack::open(&[path]).map_err(|e| e.to_string())
            });
            match shell {
                Ok(stack) => carriers.push(carrier_soundbank(stack, hash)),
                Err(e) => {
                    out.push((index, format!("the front end's shell.wad cannot be read, so {bank:?} cannot be looked up in it: {e}")));
                    continue;
                }
            }
        }
        let mut found: Vec<Soundbank> = Vec::new();
        for carrier in carriers {
            match carrier {
                Ok(Some(sb)) => found.push(sb),
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
