//! The `SpawnerAdjust` record — a per-owner tweak of the owner's spawners, group by group
//! (`docs/reverse_engineer/population_spawner/activation_and_adjust.txt` §A–C).
//!
//! `Ai.TweakAttachedSpawners(owner, {…})` (`FUN_005A4C40`) and `…InGroup(owner, group, {…})`
//! (`FUN_005A4D10`) parse their table into a fresh record ([`SpawnerAdjust::from_tweak`],
//! `FUN_005A4980`) and merge it into the record stored on the owner ([`SpawnerAdjust::merge`],
//! `FUN_004E2CC0`). The stored record decides whether a spawner record may become a runtime spawner
//! ([`activation_roll`], `FUN_004E2480`) and overrides fields of the owner's runtime spawners
//! (`FUN_004E3630` / `FUN_004E35B0`, [`crate::runtime_spawner::RuntimeSpawner::apply_adjust`]).

use mercs2_core::random::Lcg;
use mercs2_formats::spawner_record::SpawnerRecord;

/// Groups a tweak can address (`FUN_004E2480` / `FUN_004E3630` bound the group with `> 7`).
pub const ADJUST_GROUPS: usize = 8;

/// One owner's stored tweak (0x60 bytes in retail, container `0x017C0288`). Bit `g` of a mask says
/// the matching per-group value was given for group `g`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpawnerAdjust {
    /// `+0x00`: SpawnerState given.
    pub state_mask: u8,
    /// `+0x01`: ChanceNotActive given.
    pub chance_not_active_mask: u8,
    /// `+0x02`: SecondsPerCycle given.
    pub seconds_per_cycle_mask: u8,
    /// `+0x03`: SpawnList given.
    pub spawn_list_mask: u8,
    /// `+0x04`: ChanceSkip given.
    pub skip_mask: u8,
    /// `+0x05+g`: `SimpleSpawnerStateEnum` index (Off 0, On 1, Death 2, Damage 3, SegmentDamage 4).
    pub state: [u8; ADJUST_GROUPS],
    /// `+0x0D+g`: ChanceNotActive percent, `(u8)(i32)trunc(value)`.
    pub chance_not_active: [u8; ADJUST_GROUPS],
    /// `+0x15+g`: the skip percent the overrides apply — filled with the ChanceNotActive integer
    /// (retail parser bug, `0x5A4C1D`; the ChanceSkip value itself is read and discarded).
    pub skip: [u8; ADJUST_GROUPS],
    /// `+0x20+4g`.
    pub seconds_per_cycle: [f32; ADJUST_GROUPS],
    /// `+0x40+4g`: a `PopulationList` template handle (the name registry's answer for the SpawnList
    /// name; 0 when the name did not resolve).
    pub spawn_list: [u32; ADJUST_GROUPS],
}

/// The values one tweak call supplies, as `FUN_005A4980` reads them from its table arguments (last
/// argument first, first hit per key wins). `None` = the key was absent (or, for `state`, its string
/// named no state).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TweakValues {
    /// `SpawnerState`: the index of the state its name hashes to.
    pub state: Option<u8>,
    /// `ChanceNotActive`, as the number Lua gave.
    pub chance_not_active: Option<f32>,
    /// Whether `ChanceSkip` was given (its value is never used).
    pub chance_skip_given: bool,
    /// `SecondsPerCycle`.
    pub seconds_per_cycle: Option<f32>,
    /// `SpawnList`: the resolved list handle (0 when the name did not resolve — the mask is still set).
    pub spawn_list: Option<u32>,
}

impl SpawnerAdjust {
    /// The record one tweak call builds for the groups in `group_mask` (`FUN_005A4980`'s write loop,
    /// `0x5A4BD0..0x5A4C2C`).
    pub fn from_tweak(v: &TweakValues, group_mask: u8) -> SpawnerAdjust {
        let mut r = SpawnerAdjust::default();
        let cna = v.chance_not_active.map(|f| f.trunc() as i32 as u8);
        for g in 0..ADJUST_GROUPS {
            let b = 1u8 << g;
            if group_mask & b == 0 {
                continue;
            }
            if let Some(s) = v.state {
                r.state_mask |= b;
                r.state[g] = s;
            }
            if let Some(c) = cna {
                r.chance_not_active_mask |= b;
                r.chance_not_active[g] = c;
            }
            if let Some(s) = v.seconds_per_cycle {
                r.seconds_per_cycle_mask |= b;
                r.seconds_per_cycle[g] = s;
            }
            if let Some(l) = v.spawn_list {
                r.spawn_list_mask |= b;
                r.spawn_list[g] = l;
            }
            if v.chance_skip_given {
                r.skip_mask |= b;
                r.skip[g] = cna.unwrap_or(0);
            }
        }
        r
    }

    /// Merge `tweak` into this stored record (`FUN_004E2CC0`): every mask bit the tweak sets is OR-ed
    /// in and its value overwrites; nothing is ever cleared.
    pub fn merge(&mut self, tweak: &SpawnerAdjust) {
        for g in 0..ADJUST_GROUPS {
            let b = 1u8 << g;
            if tweak.state_mask & b != 0 {
                self.state_mask |= b;
                self.state[g] = tweak.state[g];
            }
            if tweak.chance_not_active_mask & b != 0 {
                self.chance_not_active_mask |= b;
                self.chance_not_active[g] = tweak.chance_not_active[g];
            }
            if tweak.seconds_per_cycle_mask & b != 0 {
                self.seconds_per_cycle_mask |= b;
                self.seconds_per_cycle[g] = tweak.seconds_per_cycle[g];
            }
            if tweak.spawn_list_mask & b != 0 {
                self.spawn_list_mask |= b;
                self.spawn_list[g] = tweak.spawn_list[g];
            }
            if tweak.skip_mask & b != 0 {
                self.skip_mask |= b;
                self.skip[g] = tweak.skip[g];
            }
        }
    }

    /// Whether a merged record is kept on its owner (`0x4E2DFA..0x4E2E3C`): any non-state mask set, or
    /// any group whose state is given and non-zero.
    pub fn keep(&self) -> bool {
        self.chance_not_active_mask | self.seconds_per_cycle_mask | self.spawn_list_mask | self.skip_mask != 0
            || (0..ADJUST_GROUPS).any(|g| self.state_mask & (1 << g) != 0 && self.state[g] != 0)
    }

    /// Whether group `g`'s state is given and non-zero.
    pub fn group_on(&self, g: usize) -> bool {
        self.state_mask & (1 << g) != 0 && self.state[g] != 0
    }
}

/// `FUN_004E2480`: whether `record` must NOT become a runtime spawner now. True when its list is 0;
/// otherwise the group's tweak state (or, with no tweak or a group past 7, the record's DefaultState)
/// must be on, and a ChanceNotActive roll then decides — the tweak's ChanceNotActive when a tweak exists
/// (the record's own is not rolled then; no roll at all if the tweak gives none), else the record's.
/// Draws from `rng` only where retail draws.
pub fn activation_roll(record: &SpawnerRecord, tweak: Option<&SpawnerAdjust>, rng: &mut Lcg) -> bool {
    if record.unit_list == 0 {
        return true;
    }
    let g = record.group as usize;
    match tweak.filter(|_| g < ADJUST_GROUPS) {
        None => {
            if record.default_state == 0 {
                return true;
            }
            let c = record.chance_not_active as i8;
            c > 0 && f32::from(c) * 0.01 > rng.next_unit()
        }
        Some(tw) => {
            let b = 1u8 << g;
            if tw.state_mask & b != 0 {
                if tw.state[g] == 0 {
                    return true;
                }
            } else if record.default_state == 0 {
                return true;
            }
            tw.chance_not_active_mask & b != 0 && f32::from(tw.chance_not_active[g]) * 0.01 > rng.next_unit()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(group: u32, default_state: u32, chance_not_active: u8) -> SpawnerRecord {
        SpawnerRecord {
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            spawner_name: 1,
            hard_point: 0,
            door: 0,
            unit_list: 0x8000_B001,
            squad_name: 0,
            seconds_per_cycle: 10.0,
            escape_radius: 0.0,
            active_radius: 100.0,
            path_spacing: 0.0,
            wakeup_effect: 0,
            unit_list_weighting: 1,
            radius_type: 1,
            default_state,
            spawner_type: 0,
            group,
            skip_percent_chance: 0,
            max_skips: 0,
            max_no_skips: 0,
            chance_not_active,
            chance_to_deactivate: 0,
            wakeup_fixed_countdown: 0,
            wakeup_random_countdown: 0,
            path_count: 0,
            ignore_model: 0,
            window_spawner: 0,
            traffic_spawner: 0,
        }
    }

    /// `dangerousbuilding.lua:163` + `:171`: `{SpawnerState="on", SpawnList=…}` then InGroup "ground"
    /// `{SpawnerState="off"}` — the 5 "On" records of the mid-mission capture: list mask 0xFF, state
    /// mask 0xFF, states [On, Off, On, On, On, On, On, On].
    #[test]
    fn turn_on_random_db_builds_the_captured_record() {
        let mut stored = SpawnerAdjust::default();
        stored.merge(&SpawnerAdjust::from_tweak(
            &TweakValues { state: Some(1), spawn_list: Some(0x8000_B001), ..TweakValues::default() },
            0xFF,
        ));
        stored.merge(&SpawnerAdjust::from_tweak(&TweakValues { state: Some(0), ..TweakValues::default() }, 1 << 1));
        assert_eq!(stored.state_mask, 0xFF);
        assert_eq!(stored.spawn_list_mask, 0xFF);
        assert_eq!(stored.state, [1, 0, 1, 1, 1, 1, 1, 1]);
        assert_eq!(stored.spawn_list, [0x8000_B001; 8]);
        assert!(stored.keep());
    }

    /// ChanceSkip's value is discarded and its slot takes the ChanceNotActive integer (0 without one).
    #[test]
    fn chance_skip_slot_holds_chance_not_active() {
        let r = SpawnerAdjust::from_tweak(
            &TweakValues { chance_not_active: Some(60.9), chance_skip_given: true, ..TweakValues::default() },
            1,
        );
        assert_eq!((r.chance_not_active_mask, r.chance_not_active[0]), (1, 60));
        assert_eq!((r.skip_mask, r.skip[0]), (1, 60));
        let r = SpawnerAdjust::from_tweak(&TweakValues { chance_skip_given: true, ..TweakValues::default() }, 1);
        assert_eq!((r.skip_mask, r.skip[0], r.chance_not_active_mask), (1, 0, 0));
    }

    /// An all-Off record with no other mask is not kept; a list mask alone keeps it.
    #[test]
    fn keep_rule() {
        let off = SpawnerAdjust::from_tweak(&TweakValues { state: Some(0), ..TweakValues::default() }, 0xFF);
        assert!(!off.keep());
        let list = SpawnerAdjust::from_tweak(&TweakValues { state: Some(0), spawn_list: Some(0), ..TweakValues::default() }, 0xFF);
        assert!(list.keep());
    }

    #[test]
    fn activation_roll_without_a_tweak_uses_the_record() {
        let mut rng = Lcg::new(1);
        assert!(activation_roll(&SpawnerRecord { unit_list: 0, ..record(0, 1, 0) }, None, &mut rng));
        assert!(activation_roll(&record(0, 0, 0), None, &mut rng), "DefaultState Off");
        let before = rng;
        assert!(!activation_roll(&record(0, 1, 0), None, &mut rng), "no ChanceNotActive → no draw, create");
        assert_eq!(rng, before);
        // 100 % not active: 1.0 > U for every U in [0, 1).
        assert!(activation_roll(&record(0, 1, 100), None, &mut rng));
        assert_ne!(rng, before, "the roll drew");
    }

    #[test]
    fn activation_roll_with_a_tweak_ignores_the_records_chance() {
        let mut rng = Lcg::new(1);
        let on = SpawnerAdjust::from_tweak(&TweakValues { state: Some(1), ..TweakValues::default() }, 0xFF);
        let before = rng;
        assert!(!activation_roll(&record(3, 0, 100), Some(&on), &mut rng), "tweak On overrides DefaultState and the record's 100 %");
        assert_eq!(rng, before, "no tweak ChanceNotActive → no draw");
        let off = SpawnerAdjust::from_tweak(&TweakValues { state: Some(0), ..TweakValues::default() }, 1 << 3);
        assert!(activation_roll(&record(3, 1, 0), Some(&off), &mut rng));
        // No state for the group → the record's DefaultState decides.
        let list_only = SpawnerAdjust::from_tweak(&TweakValues { spawn_list: Some(5), ..TweakValues::default() }, 0xFF);
        assert!(activation_roll(&record(3, 0, 0), Some(&list_only), &mut rng));
        assert!(!activation_roll(&record(3, 1, 0), Some(&list_only), &mut rng));
        // A group past 7 ignores the tweak.
        assert!(activation_roll(&record(8, 0, 0), Some(&on), &mut rng));
    }
}
