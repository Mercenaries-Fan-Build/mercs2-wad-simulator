//! Which cues can play which waves, from the tables alone — for tools that name or inventory waves
//! without decoding audio (the engine's [`crate::AudioEngine::resolve_cue`] needs the waves resident).
//!
//! The chain is the one the engine follows: a `sounddb` entry names `(soundbank, cue index)`; the
//! soundbank cue plays one group (single-track) or, per sound of every track, one of several groups
//! (multi-track); a group plays one of its waves `(wavebank, index)`. Every group of every given
//! soundbank is listed, whether or not a cue reaches it.

use std::collections::{BTreeMap, HashMap};

use crate::soundbank::{CueBody, Soundbank};
use crate::sounddb::SoundDb;

/// One way a wave is used: in `slot` of group `group_index` of `soundbank`, reached by `cues`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaveUse {
    /// The soundbank holding the group.
    pub soundbank: u32,
    /// The group's index there.
    pub group_index: u16,
    /// The wave's position in the group's wave list.
    pub slot: usize,
    /// Guids of the cues that can play the group, in sounddb order (empty when no cue reaches it).
    pub cues: Vec<u32>,
}

/// Why routing stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// A sounddb entry's cue index is past its soundbank's cues.
    CueIndexOutOfRange { soundbank: u32, index: u32 },
    /// A sounddb entry's guid differs from the soundbank cue it indexes.
    GuidMismatch { entry: u32, soundbank_cue: u32 },
    /// A cue names a group index past its soundbank's groups.
    GroupIndexOutOfRange { soundbank: u32, index: u16 },
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::CueIndexOutOfRange { soundbank, index } => {
                write!(f, "route: cue index {index} is past soundbank 0x{soundbank:08X}'s cues")
            }
            RouteError::GuidMismatch { entry, soundbank_cue } => {
                write!(f, "route: sounddb entry 0x{entry:08X} lands on soundbank cue 0x{soundbank_cue:08X}")
            }
            RouteError::GroupIndexOutOfRange { soundbank, index } => {
                write!(f, "route: group index {index} is past soundbank 0x{soundbank:08X}'s groups")
            }
        }
    }
}

impl std::error::Error for RouteError {}

/// Every wave's uses, plus the cue references that leave the given tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routing {
    /// `(wavebank, wave index)` → its uses.
    pub waves: BTreeMap<(u32, u32), Vec<WaveUse>>,
    /// `(cue guid, soundbank)` for entries or cue sounds naming a soundbank that was not given.
    pub missing_soundbanks: Vec<(u32, u32)>,
}

impl Routing {
    /// The first cue that can play a wave, if any.
    pub fn first_cue(&self, wavebank: u32, index: u32) -> Option<u32> {
        self.waves.get(&(wavebank, index))?.iter().find_map(|u| u.cues.first().copied())
    }

    /// The first use of a wave (the group and slot to qualify a name with), if any.
    pub fn first_use(&self, wavebank: u32, index: u32) -> Option<&WaveUse> {
        self.waves.get(&(wavebank, index))?.first()
    }
}

/// Route every entry of `sounddbs` through `soundbanks`.
pub fn route(sounddbs: &[SoundDb], soundbanks: &[Soundbank]) -> Result<Routing, RouteError> {
    let banks: HashMap<u32, &Soundbank> = soundbanks.iter().map(|b| (b.bank_hash, b)).collect();
    // (soundbank, group) → cue guids reaching it.
    let mut reached: HashMap<(u32, u16), Vec<u32>> = HashMap::new();
    let mut missing = Vec::new();
    let mut reach = |guid: u32, bank: u32, group: u16, missing: &mut Vec<(u32, u32)>| -> Result<(), RouteError> {
        let Some(b) = banks.get(&bank) else {
            missing.push((guid, bank));
            return Ok(());
        };
        if group as usize >= b.groups.len() {
            return Err(RouteError::GroupIndexOutOfRange { soundbank: bank, index: group });
        }
        let cues = reached.entry((bank, group)).or_default();
        if !cues.contains(&guid) {
            cues.push(guid);
        }
        Ok(())
    };
    for db in sounddbs {
        for e in &db.cues {
            let Some(b) = banks.get(&e.bank_hash) else {
                missing.push((e.guid, e.bank_hash));
                continue;
            };
            let cue = b
                .cues
                .get(e.cue_index as usize)
                .ok_or(RouteError::CueIndexOutOfRange { soundbank: e.bank_hash, index: e.cue_index })?;
            if cue.guid != e.guid {
                return Err(RouteError::GuidMismatch { entry: e.guid, soundbank_cue: cue.guid });
            }
            match &cue.body {
                CueBody::SingleTrack { soundbank, group_index, .. } => {
                    reach(e.guid, *soundbank, *group_index, &mut missing)?
                }
                CueBody::MultiTrack(m) => {
                    for entry in m.tracks.iter().flat_map(|t| t.sounds.iter()).flat_map(|s| s.entries.iter()) {
                        reach(e.guid, entry.soundbank, entry.group_index, &mut missing)?;
                    }
                }
            }
        }
    }
    let mut routing = Routing { waves: BTreeMap::new(), missing_soundbanks: missing };
    for b in soundbanks {
        for (g, group) in b.groups.iter().enumerate() {
            let cues = reached.get(&(b.bank_hash, g as u16)).cloned().unwrap_or_default();
            for (slot, w) in group.waves().iter().enumerate() {
                routing.waves.entry((w.wavebank, w.index)).or_default().push(WaveUse {
                    soundbank: b.bank_hash,
                    group_index: g as u16,
                    slot,
                    cues: cues.clone(),
                });
            }
        }
    }
    Ok(routing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{encode_bank, BankSpec, CueSpec, Pcm16, UI_PDA_OPEN_CUE, UI_PDA_OPEN_GROUP};
    use mercs2_formats::hash::pandemic_hash_m2 as m2;

    #[test]
    fn routes_each_wave_to_the_cue_that_plays_it() {
        let cue = |name: &str| CueSpec {
            name: name.to_string(),
            category: "ui".to_string(),
            sound_id: m2(name),
            clip_hash: m2(name),
            pcm: Pcm16 { channels: 1, sample_rate: 22050, samples: vec![1; 8] },
            group: UI_PDA_OPEN_GROUP,
            cue: UI_PDA_OPEN_CUE,
        };
        let enc = encode_bank(&BankSpec { name: "b".into(), cues: vec![cue("zeta"), cue("alpha")] }).unwrap();
        let db = SoundDb::parse(&enc.sounddb).unwrap();
        let sb = Soundbank::parse(&enc.soundbank).unwrap();
        let r = route(std::slice::from_ref(&db), std::slice::from_ref(&sb)).unwrap();
        // The sounddb is guid-sorted, the waves are in cue order: the third field must be followed.
        assert_eq!(r.first_cue(m2("b"), 0), Some(m2("zeta")));
        assert_eq!(r.first_cue(m2("b"), 1), Some(m2("alpha")));
        assert!(r.missing_soundbanks.is_empty());

        let alone = route(&[db], &[]).unwrap();
        assert_eq!(alone.missing_soundbanks.len(), 2, "entries naming an absent soundbank are listed");
    }
}
