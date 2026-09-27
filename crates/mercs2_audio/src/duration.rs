//! A cue's `+0x0C` length — the value `Sound.GetMaxDuration` returns — computed the way the authored
//! banks carry it.
//!
//! **The engine never computes it.** It reads it in one place, `FUN_005faca0` (FindCue, then the cue,
//! then `movss xmm0, [cue+0x0C]`, 0 when the cue is missing), which has two callers: the
//! `Sound.GetMaxDuration` binding (`0x005E3860`, pushes it to Lua as a number) and the VO line start
//! `FUN_00515c10` (`0x00515C50`: a length ≤ 0 is replaced by 5.0 s, `DAT_00B9B700`). Nothing in the
//! mixer, the instance update or the bank loader touches `+0x0C`. So the value is authored data, and
//! the rule below is the one the retail banks follow — measured, not read from engine code:
//!
//! * **−1.0** when anything the cue plays loops: a multi-wave group it reaches with a non-zero
//!   `+0x2C` byte, a track with a non-zero `+0x00` byte, or a non-zero multi-track `+0x10` byte. (These
//!   are loop counts: `FUN_00835060` loops the cue while its `+0x10` count is non-zero and
//!   `FUN_0083c070` a track while its `+0x00` count is, each decrementing unless it is `0xFF`; the
//!   group's `+0x2C` byte is copied to the sound instance at `+0x80` by `FUN_008369e0`.)
//! * otherwise the latest end among the cue's sounds (`start + the longest wave any of its groups can
//!   play`, in double precision) and its ramps and LFOs (automation kinds 0–3: `start + duration`,
//!   single precision), rounded to `f32`.
//!
//! A wave's length is `frames / rate` for an embedded record and `+0x10 / (rate × channels × 2)` for a
//! streamed one (the streamed record's `+0x10` counts decoded PCM16 bytes, not frames).
//!
//! Across every cue of `vz.wad` and `English.wad` (14,834) the rule reproduces 14,818
//! lengths bit for bit; `tests/retail_banks.rs` names the 16 `vz.wad` cues it does not (hand-set or
//! stale values, and six that differ by one unit in the last place).

use crate::multitrack::{Automation, MultiTrackCue};
use crate::soundbank::{CueBody, Group, GroupForm};
use crate::wave::{WaveData, WaveRecord};

/// Why a length could not be computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DurationError {
    /// A group the cue reaches is not available.
    GroupMissing { soundbank: u32, group: u16 },
    /// A wave a group names is not available.
    WaveMissing { wavebank: u32, index: u32 },
    /// A wave record's sample rate is 0.
    ZeroRate { wavebank: u32, index: u32 },
    /// The cue plays no wave and has no ramp or LFO, so it has no end.
    NothingToPlay,
}

impl std::fmt::Display for DurationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DurationError::GroupMissing { soundbank, group } => {
                write!(f, "cue length: group {group} of soundbank 0x{soundbank:08X} is not available")
            }
            DurationError::WaveMissing { wavebank, index } => {
                write!(f, "cue length: wave {index} of wavebank 0x{wavebank:08X} is not available")
            }
            DurationError::ZeroRate { wavebank, index } => {
                write!(f, "cue length: wave {index} of wavebank 0x{wavebank:08X} has sample rate 0")
            }
            DurationError::NothingToPlay => write!(f, "cue length: the cue plays nothing"),
        }
    }
}

impl std::error::Error for DurationError {}

/// A sound's start time and the `(soundbank, group)` pairs it can pick.
type SoundGroups = (f32, Vec<(u32, u16)>);

/// A wave record's length in seconds, as the authored lengths count it.
pub fn wave_length_s(rec: &WaveRecord) -> Option<f64> {
    if rec.sample_rate == 0 {
        return None;
    }
    let rate = f64::from(rec.sample_rate);
    Some(match rec.data {
        WaveData::Embedded(_) => f64::from(rec.frames) / rate,
        WaveData::Streamed { .. } => f64::from(rec.frames) / (rate * f64::from(rec.channels) * 2.0),
    })
}

/// The length retail stores at a cue's `+0x0C`. `group` finds a group by `(soundbank, index)`;
/// `wave` finds a wave record by `(wavebank, index)`.
pub fn cue_length_s<'a>(
    body: &CueBody,
    group: impl Fn(u32, u16) -> Option<&'a Group>,
    wave: impl Fn(u32, u32) -> Option<&'a WaveRecord>,
) -> Result<f32, DurationError> {
    let (sounds, mtc): (Vec<SoundGroups>, Option<&MultiTrackCue>) = match body {
        CueBody::SingleTrack { soundbank, group_index, .. } => {
            (vec![(0.0, vec![(*soundbank, *group_index)])], None)
        }
        CueBody::MultiTrack(m) => (
            m.tracks
                .iter()
                .flat_map(|t| t.sounds.iter())
                .map(|s| (s.start_s, s.entries.iter().map(|e| (e.soundbank, e.group_index)).collect()))
                .collect(),
            Some(m),
        ),
    };
    let mut loops = mtc.is_some_and(|m| m.byte_10 != 0 || m.tracks.iter().any(|t| t.byte_00 != 0));
    let mut end = f64::NEG_INFINITY;
    for (start, entries) in &sounds {
        for &(sb, gi) in entries {
            let g = group(sb, gi).ok_or(DurationError::GroupMissing { soundbank: sb, group: gi })?;
            if let GroupForm::Multi(m) = &g.form {
                loops |= m.byte_2c != 0;
            }
            for w in g.waves() {
                let rec = wave(w.wavebank, w.index)
                    .ok_or(DurationError::WaveMissing { wavebank: w.wavebank, index: w.index })?;
                let len = wave_length_s(rec)
                    .ok_or(DurationError::ZeroRate { wavebank: w.wavebank, index: w.index })?;
                end = end.max(f64::from(*start) + len);
            }
        }
    }
    if loops {
        return Ok(-1.0);
    }
    if let Some(m) = mtc {
        let records = m
            .events
            .iter()
            .chain(m.curves.iter())
            .chain(m.tracks.iter().flat_map(|t| t.automation.iter()));
        for a in records {
            if let Automation::Ramp { start_s, duration_s, .. } | Automation::Lfo { start_s, duration_s, .. } = a {
                end = end.max(f64::from(start_s + duration_s));
            }
        }
    }
    if end == f64::NEG_INFINITY {
        return Err(DurationError::NothingToPlay);
    }
    Ok(end as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multitrack::{Sound, SoundEntry, Target, Track};
    use crate::soundbank::{GroupHead, MultiGroup, WaveRef};

    fn head() -> GroupHead {
        GroupHead {
            sound_id: 0,
            category: 0,
            unknown_10: 1.0,
            unknown_14: 0,
            min_distance: 1.0,
            max_distance: 2.0,
            unknown_20: 1.0,
            distance_exponent: 1.0,
            doppler_scale: 1.0,
        }
    }
    fn rec(frames: u32) -> WaveRecord {
        WaveRecord {
            clip_hash: 0,
            channels: 1,
            format: 2,
            sample_rate: 1000,
            frames,
            data: WaveData::Embedded(vec![0; frames as usize * 2]),
        }
    }
    fn multi(byte_2c: u8, waves: Vec<WaveRef>) -> Group {
        Group {
            head: head(),
            form: GroupForm::Multi(MultiGroup {
                byte_2c,
                selection: 1,
                byte_2f: 0,
                unknown_30: 0.0,
                unknown_34: 0.0,
                unknown_3c: 0.0,
                unknown_40: 0.0,
                word_48: 0,
                floats_4c: [0.0; 6],
                unknown_64: 0.0,
                waves,
            }),
        }
    }

    #[test]
    fn longest_wave_start_and_automation_end() {
        let recs = [rec(500), rec(2000)];
        let g = multi(0, vec![WaveRef { wavebank: 1, index: 0, weight: 0.5 }, WaveRef { wavebank: 1, index: 1, weight: 0.5 }]);
        let single = CueBody::SingleTrack { soundbank: 7, group_index: 0, unknown_16: 0 };
        let find_g = |_: u32, _: u16| Some(&g);
        let find_w = |_: u32, i: u32| recs.get(i as usize);
        assert_eq!(cue_length_s(&single, find_g, find_w), Ok(2.0), "the longest wave");

        let mut m = MultiTrackCue {
            byte_10: 0,
            sound_slots: 1,
            unknown_18: 1.0,
            unknown_1c: -1.0,
            unknown_20: -1.0,
            unknown_24: 0.0,
            events: vec![],
            curves: vec![],
            tracks: vec![Track {
                byte_00: 0,
                unknown_04: -1.0,
                unknown_08: -1.0,
                automation: vec![],
                sounds: vec![Sound {
                    slot: 0,
                    byte_01: 2,
                    byte_02: 2,
                    selection: 1,
                    start_s: 0.5,
                    entries: vec![SoundEntry { soundbank: 7, group_index: 0, unknown_06: 0, weight: 1.0 }],
                }],
            }],
            params: vec![],
        };
        assert_eq!(cue_length_s(&CueBody::MultiTrack(m.clone()), find_g, find_w), Ok(2.5), "start + wave");
        m.tracks[0].automation.push(Automation::Ramp {
            target: Target::Volume,
            start_s: 1.0,
            mode: 0,
            unknown_0c: 0,
            duration_s: 3.0,
            from: 1.0,
            to: 0.0,
        });
        assert_eq!(cue_length_s(&CueBody::MultiTrack(m.clone()), find_g, find_w), Ok(4.0), "a ramp ends later");
        m.tracks[0].byte_00 = 0xFF;
        assert_eq!(cue_length_s(&CueBody::MultiTrack(m), find_g, find_w), Ok(-1.0), "a looping track");

        let looping = multi(3, vec![WaveRef { wavebank: 1, index: 0, weight: 1.0 }]);
        assert_eq!(cue_length_s(&single, |_, _| Some(&looping), find_w), Ok(-1.0), "a looping group");
    }

    #[test]
    fn streamed_records_count_decoded_bytes() {
        let r = WaveRecord {
            clip_hash: 0,
            channels: 2,
            format: 4,
            sample_rate: 44100,
            frames: 4 * 44100,
            data: WaveData::Streamed { offset: 0, size: 1, word_1c: 0 },
        };
        assert_eq!(wave_length_s(&r), Some(1.0));
    }
}
