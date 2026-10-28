//! The retail runtime spawner — the 148-byte object `FUN_004E2840` builds from a
//! [`SpawnerRecord`] when the record's owner goes live or a tweak turns its group on — and the helpers
//! every family updater shares (`docs/reverse_engineer/population_spawner/shared_helpers.txt`; layout
//! table in `docs/reverse_engineer/population_spawner_code_map.md` §6). Each field names its retail
//! offset; each helper names its function.

use mercs2_core::glam::Vec3;
use mercs2_core::random::Lcg;
use mercs2_formats::spawner_record::SpawnerRecord;

use crate::adjust::{SpawnerAdjust, ADJUST_GROUPS};

/// `DAT_00B97EEC`: the percent-to-fraction scale init and the overrides apply to chance bytes.
const PERCENT: f32 = 0.01;

/// `SimpleSpawnerStateEnum`.
pub mod state {
    pub const OFF: u8 = 0;
    pub const ON: u8 = 1;
    pub const ACTIVATE_ON_DEATH: u8 = 2;
    pub const ACTIVATE_ON_DAMAGE: u8 = 3;
    pub const ACTIVATE_ON_SEGMENT_DAMAGE: u8 = 4;
    /// `__internal`: no further spawns.
    pub const DONE: u8 = 5;
}

/// `SimpleSpawnerTypeEnum`.
pub mod spawner_type {
    pub const SINGLE: u8 = 0;
    pub const SPEW: u8 = 1;
    pub const ONCE: u8 = 2;
    pub const SQUAD: u8 = 3;
    pub const SEAT: u8 = 4;
    pub const SEAT_ONCE: u8 = 5;
    pub const PARKING_SPACE: u8 = 6;
}

/// `SpawnerRadiusTypeEnum`.
pub mod radius_type {
    pub const HIBERNATION: u8 = 0;
    pub const CAMERA_2D: u8 = 1;
    pub const CAMERA_3D: u8 = 2;
    pub const PLAYER_2D: u8 = 3;
    pub const PLAYER_3D: u8 = 4;
    pub const INVERSE_HIBERNATION_CAMERA_2D: u8 = 5;
    pub const INVERSE_HIBERNATION_CAMERA_3D: u8 = 6;
}

/// `+0x93` flag bits (init `0x4E28E7..0x4E2944`).
pub mod flags {
    pub const IGNORE_MODEL: u8 = 0x01;
    pub const WINDOW: u8 = 0x02;
    /// Set by the unit-slot check once the unit's model type was checked (`0x4E3A5C`).
    pub const MODEL_TYPE_CHECKED: u8 = 0x04;
    pub const TRAFFIC: u8 = 0x08;
    pub const AI_PATROL: u8 = 0x10;
    pub const PATH: u8 = 0x20;
}

/// A door request `FUN_004E11F0` posts while a cycle is about to end (`FUN_004E57D0`, type 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoorRequest {
    pub owner: u32,
    pub door: u32,
}

/// `0x2A792A6B`: the door value the Window creator writes so window spawners never post door requests.
pub const NO_DOOR: u32 = 0x2A79_2A6B;

/// One runtime spawner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuntimeSpawner {
    /// `+0x00`: the record's local position (Window: offset by (0.35, −0.65, 0.25), `0x4E2B71`).
    pub position: [f32; 3],
    /// `+0x10`: the record's local rotation quaternion.
    pub rotation: [f32; 4],
    /// `+0x20`: the record's SpawnerName.
    pub spawner_name: u32,
    /// `+0x24`: the owner.
    pub owner: u32,
    /// `+0x28`: the record's HardPoint.
    pub hard_point: u32,
    /// `+0x2C`: the unit list.
    pub unit_list: u32,
    /// `+0x30`: the record's Door.
    pub door: u32,
    /// `+0x34`: the record's SquadName.
    pub squad_name: u32,
    /// `+0x3C`: the record's WakeupEffect.
    pub wakeup_effect: u32,
    /// `+0x48`: the node-health key — −1 for whole-object health (IgnoreModel and the NoModel/Path
    /// creators); the Window/Hardpoint creators set it from the HardPoint node.
    pub node_key: i32,
    /// `+0x4C`: the seat key, −1 when none.
    pub seat: i32,
    /// `+0x5C`: the spawn-cycle countdown (0 at init: the first spawn waits no cycle).
    pub cycle: f32,
    /// `+0x60`: the one-shot wakeup countdown.
    pub wakeup: f32,
    /// `+0x64` / `+0x68`: the evaluation throttle and the dt accumulated while throttled.
    pub throttle: f32,
    pub throttle_acc: f32,
    /// `+0x6C`: SecondsPerCycle.
    pub seconds_per_cycle: f32,
    /// `+0x70` / `+0x74`.
    pub escape_radius: f32,
    pub active_radius: f32,
    /// `+0x78` / `+0x7C` / `+0x80`: SkipPercentChance / ChanceNotActive / ChanceToDeactivate × 0.01.
    pub skip_chance: f32,
    pub not_active_chance: f32,
    pub deactivate_chance: f32,
    /// `+0x84`: PathSpacing at init (the Window updater reuses it as its arrival ramp).
    pub ramp: f32,
    /// `+0x88`: SpawnerRadiusType.
    pub radius_type: u8,
    /// `+0x89`: [`state`].
    pub state: u8,
    /// `+0x8A`: [`spawner_type`].
    pub spawner_type: u8,
    /// `+0x8B`: SpawnerGroup.
    pub group: u8,
    /// `+0x8D`: PathCount.
    pub path_count: u8,
    /// `+0x8E`: UnitListWeighting (the `FUN_004E4150` switch selector).
    pub weighting: u8,
    /// `+0x8F` / `+0x90`: MaxSkips / MaxNoSkips, clamped to at most 100.
    pub max_skips: i8,
    pub max_no_skips: i8,
    /// `+0x91` / `+0x92`: the skip and no-skip run counters.
    pub skip_run: i8,
    pub no_skip_run: i8,
    /// `+0x93`: [`flags`].
    pub flags: u8,
}

impl RuntimeSpawner {
    /// `FUN_004E2840`: a runtime spawner for `owner` from `record`. Takes exactly one float draw (the
    /// wakeup countdown), as retail does even when both wakeup bytes are 0 (`0x4E2AF3..0x4E2B58`).
    /// `ai_patrol` is whether the owner carries an `AiPatrol` component (container `0x017BC868`).
    pub fn init(record: &SpawnerRecord, owner: u32, ai_patrol: bool, rng: &mut Lcg) -> RuntimeSpawner {
        let byte_pct = |b: u8| f32::from(b as i8) * PERCENT;
        let clamp100 = |b: u8| (b as i8).min(100);
        let mut f = 0u8;
        if record.ignore_model != 0 {
            f |= flags::IGNORE_MODEL;
        }
        if record.window_spawner != 0 {
            f |= flags::WINDOW;
        }
        if record.traffic_spawner != 0 {
            f |= flags::TRAFFIC;
        }
        if (record.path_count as i8) > 0 {
            f |= flags::PATH;
        }
        // Path spawners set AiPatrol unconditionally (0x4E29C4); the others when the owner has one.
        if f & flags::PATH != 0 || ai_patrol {
            f |= flags::AI_PATROL;
        }
        let wakeup = f32::from(record.wakeup_fixed_countdown as i8)
            + rng.next_unit() * f32::from(record.wakeup_random_countdown as i8);
        let mut position = record.position;
        if f & flags::WINDOW != 0 {
            position[0] += 0.35;
            position[1] -= 0.65;
            position[2] += 0.25;
        }
        let whole_object = if f & flags::IGNORE_MODEL != 0 { -1 } else { 0 };
        RuntimeSpawner {
            position,
            rotation: record.rotation,
            spawner_name: record.spawner_name,
            owner,
            hard_point: record.hard_point,
            unit_list: record.unit_list,
            door: record.door,
            squad_name: record.squad_name,
            wakeup_effect: record.wakeup_effect,
            node_key: whole_object,
            seat: whole_object,
            cycle: 0.0,
            wakeup,
            throttle: 0.0,
            throttle_acc: 0.0,
            seconds_per_cycle: record.seconds_per_cycle,
            escape_radius: record.escape_radius,
            active_radius: record.active_radius,
            skip_chance: byte_pct(record.skip_percent_chance),
            not_active_chance: byte_pct(record.chance_not_active),
            deactivate_chance: byte_pct(record.chance_to_deactivate),
            ramp: record.path_spacing,
            radius_type: record.radius_type as u8,
            state: record.default_state as u8,
            spawner_type: record.spawner_type as u8,
            group: record.group as u8,
            path_count: record.path_count,
            weighting: record.unit_list_weighting as u8,
            max_skips: clamp100(record.max_skips),
            max_no_skips: clamp100(record.max_no_skips),
            skip_run: 0,
            no_skip_run: 0,
            flags: f,
        }
    }

    /// `FUN_004E35B0` / `FUN_004E3630`: apply the owner's stored tweak for this spawner's group (a
    /// group past 7 is untouched).
    pub fn apply_adjust(&mut self, tw: &SpawnerAdjust) {
        let g = self.group as usize;
        if g >= ADJUST_GROUPS {
            return;
        }
        let b = 1u8 << g;
        if tw.state_mask & b != 0 {
            self.state = tw.state[g];
        }
        if tw.chance_not_active_mask & b != 0 {
            self.not_active_chance = f32::from(tw.chance_not_active[g]) * PERCENT;
        }
        if tw.skip_mask & b != 0 {
            self.skip_chance = f32::from(tw.skip[g]) * PERCENT;
        }
        if tw.seconds_per_cycle_mask & b != 0 {
            self.seconds_per_cycle = tw.seconds_per_cycle[g];
        }
        if tw.spawn_list_mask & b != 0 {
            self.unit_list = tw.spawn_list[g];
        }
    }

    /// `FUN_004E0C00`: the evaluation throttle. Returns `true` while throttled (the caller skips this
    /// spawner this frame). On re-arm it takes one float draw, sets the next interval to
    /// `0.1 + 3 U` and folds the accumulated time into `*dt`.
    pub fn throttle(&mut self, dt: &mut f32, rng: &mut Lcg) -> bool {
        if self.throttle > 0.0 {
            self.throttle -= *dt;
            self.throttle_acc += *dt;
            return true;
        }
        self.throttle = 0.1 + 3.0 * rng.next_unit();
        *dt += self.throttle_acc;
        self.throttle_acc = 0.0;
        false
    }

    /// `FUN_004E11F0`: the spawn-cycle countdown. Returns whether the cycle is still running and the
    /// door request posted while it is within 3 s of its end.
    pub fn cycle_countdown(&mut self, dt: f32) -> (bool, Option<DoorRequest>) {
        let mut door = None;
        if self.cycle > 0.0 {
            self.cycle -= dt;
            if self.cycle < 3.0 && self.door != 0 && self.door != NO_DOOR {
                door = Some(DoorRequest { owner: self.owner, door: self.door });
            }
        }
        (self.cycle > 0.0, door)
    }

    /// `FUN_004E2770`: whether this cycle is skipped. Forced skips and spawns by the run limits first;
    /// then, for a skip chance above 0, one float draw `U` and a skip when `U > chance`.
    pub fn skip_roll(&self, rng: &mut Lcg) -> bool {
        if self.max_no_skips > 0 && self.no_skip_run >= self.max_no_skips {
            return true;
        }
        if self.max_skips > 0 && self.skip_run >= self.max_skips {
            return false;
        }
        self.skip_chance > 0.0 && rng.next_unit() > self.skip_chance
    }

    /// `FUN_004E2800`: a skipped cycle.
    pub fn record_skip(&mut self) {
        self.no_skip_run = 0;
        if self.max_skips > 0 {
            self.skip_run = self.skip_run.wrapping_add(1);
        }
    }

    /// `FUN_004E2820`: a spawning cycle.
    pub fn record_spawn(&mut self) {
        self.skip_run = 0;
        if self.max_no_skips > 0 {
            self.no_skip_run = self.no_skip_run.wrapping_add(1);
        }
    }

    /// `FUN_004E26F0`: whether the spawner stops after this spawn (the caller then sets state 5): the
    /// ChanceToDeactivate roll (one draw when the chance is above 0), else Once / SeatOnce.
    pub fn deactivates_after_spawn(&self, rng: &mut Lcg) -> bool {
        if self.deactivate_chance > 0.0 && self.deactivate_chance > rng.next_unit() {
            return true;
        }
        matches!(self.spawner_type, spawner_type::ONCE | spawner_type::SEAT_ONCE)
    }

    /// `FUN_004E0FB0`: whether the spawner is dormant (does not spawn). `health` is the family's health
    /// argument (Window/Hardpoint: the health score; NoModel/Path: 1.0); `owner_health` is the owner's
    /// whole-object `(cur, max)`, if it has one.
    pub fn is_dormant(&self, health: f32, owner_health: Option<(f32, f32)>) -> bool {
        match self.state {
            state::ACTIVATE_ON_DEATH => health > 0.0,
            state::ACTIVATE_ON_DAMAGE => owner_health.is_some_and(|(cur, max)| cur >= max),
            state::ACTIVATE_ON_SEGMENT_DAMAGE => health == 1.0,
            _ => false,
        }
    }

    /// `FUN_004E38D0`: whether `pos` is in this spawner's active range of the camera anchors
    /// (`cameras`) or the local players (`players`) by SpawnerRadiusType, against ActiveRadius²;
    /// both comparisons inclusive. With no anchors the minimum distance is 0.
    pub fn in_active_range(&self, pos: Vec3, cameras: &[Vec3], players: &[Vec3]) -> bool {
        let r2 = self.active_radius * self.active_radius;
        let min = |anchors: &[Vec3], flat: bool| {
            anchors
                .iter()
                .map(|a| {
                    let d = *a - pos;
                    if flat {
                        d.x * d.x + d.z * d.z
                    } else {
                        d.length_squared()
                    }
                })
                .fold(None, |m: Option<f32>, d| Some(m.map_or(d, |m| m.min(d))))
                .unwrap_or(0.0)
        };
        match self.radius_type {
            radius_type::HIBERNATION => true,
            radius_type::CAMERA_2D => r2 >= min(cameras, true),
            radius_type::CAMERA_3D => r2 >= min(cameras, false),
            radius_type::PLAYER_2D => r2 >= min(players, true),
            radius_type::PLAYER_3D => r2 >= min(players, false),
            radius_type::INVERSE_HIBERNATION_CAMERA_2D => min(cameras, true) >= r2,
            radius_type::INVERSE_HIBERNATION_CAMERA_3D => min(cameras, false) >= r2,
            _ => r2 >= 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adjust::TweakValues;

    fn guard_tower() -> SpawnerRecord {
        SpawnerRecord {
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            spawner_name: 0x24CE_BC21,
            hard_point: 0xEBBF_5043,
            door: 0,
            unit_list: 0x8000_B00D,
            squad_name: 0,
            seconds_per_cycle: 10.0,
            escape_radius: 40.0,
            active_radius: 100.0,
            path_spacing: 0.0,
            wakeup_effect: 0,
            unit_list_weighting: 1,
            radius_type: 1,
            default_state: 1,
            spawner_type: 2,
            group: 3,
            skip_percent_chance: 0,
            max_skips: 0,
            max_no_skips: 0,
            chance_not_active: 0,
            chance_to_deactivate: 0,
            wakeup_fixed_countdown: 0,
            wakeup_random_countdown: 0,
            path_count: 0,
            ignore_model: 0,
            window_spawner: 0,
            traffic_spawner: 0,
        }
    }

    #[test]
    fn init_copies_the_record_and_takes_one_draw() {
        let mut rng = Lcg::new(7);
        let mut expect = rng;
        expect.next_unit();
        let s = RuntimeSpawner::init(&guard_tower(), 0x0013_C338, false, &mut rng);
        assert_eq!(rng, expect, "exactly one draw");
        assert_eq!((s.owner, s.unit_list, s.group, s.state, s.spawner_type, s.weighting), (0x0013_C338, 0x8000_B00D, 3, 1, 2, 1));
        assert_eq!((s.seconds_per_cycle, s.escape_radius, s.active_radius), (10.0, 40.0, 100.0));
        assert_eq!((s.cycle, s.wakeup, s.node_key, s.seat, s.flags), (0.0, 0.0, 0, 0, 0));
    }

    #[test]
    fn init_signs_clamps_and_flags() {
        let r = SpawnerRecord {
            skip_percent_chance: 0xF6, // −10 %
            max_skips: 120,
            max_no_skips: 0xFF, // −1: disabled
            wakeup_fixed_countdown: 4,
            wakeup_random_countdown: 0,
            ignore_model: 1,
            window_spawner: 1,
            path_count: 2,
            ..guard_tower()
        };
        let s = RuntimeSpawner::init(&r, 1, false, &mut Lcg::new(1));
        assert!((s.skip_chance + 0.1).abs() < 1e-6);
        assert_eq!((s.max_skips, s.max_no_skips), (100, -1));
        assert_eq!(s.wakeup, 4.0);
        assert_eq!(s.flags, flags::IGNORE_MODEL | flags::WINDOW | flags::PATH | flags::AI_PATROL);
        assert_eq!((s.node_key, s.seat), (-1, -1));
        assert_eq!(s.position, [1.35, 1.35, 3.25]);
    }

    #[test]
    fn throttle_rearms_and_folds_the_accumulated_time() {
        let mut rng = Lcg::new(3);
        let mut s = RuntimeSpawner::init(&guard_tower(), 1, false, &mut rng);
        let mut dt = 0.5;
        assert!(!s.throttle(&mut dt, &mut rng), "first evaluation runs");
        assert!((0.1..3.1).contains(&s.throttle));
        let mut held = 0.0;
        loop {
            let mut d = 0.05;
            if !s.throttle(&mut d, &mut rng) {
                assert!((d - (held + 0.05)).abs() < 1e-4, "dt carries everything since the last evaluation");
                break;
            }
            held += 0.05;
        }
    }

    #[test]
    fn cycle_countdown_posts_door_requests_near_the_end() {
        let mut s = RuntimeSpawner::init(&SpawnerRecord { door: 0x1234, ..guard_tower() }, 9, false, &mut Lcg::new(1));
        assert_eq!(s.cycle_countdown(1.0), (false, None), "cycle 0: not running, no request");
        s.cycle = 5.0;
        assert_eq!(s.cycle_countdown(1.0), (true, None));
        assert_eq!(s.cycle_countdown(1.5), (true, Some(DoorRequest { owner: 9, door: 0x1234 })));
        s.door = NO_DOOR;
        assert_eq!(s.cycle_countdown(1.0), (true, None));
    }

    #[test]
    fn skip_roll_run_limits_and_chance() {
        let mut rng = Lcg::new(5);
        let mut s = RuntimeSpawner::init(&guard_tower(), 1, false, &mut rng);
        let before = rng;
        assert!(!s.skip_roll(&mut rng), "chance 0 never skips");
        assert_eq!(rng, before, "and never draws");
        s.max_no_skips = 2;
        s.record_spawn();
        s.record_spawn();
        assert!(s.skip_roll(&mut rng), "forced skip after MaxNoSkips spawns");
        s.record_skip();
        assert_eq!((s.no_skip_run, s.skip_run), (0, 0), "MaxSkips 0 does not count skips");
        s.skip_chance = 1.0;
        assert!(!s.skip_roll(&mut rng), "U > 1.0 never");
    }

    #[test]
    fn deactivation_and_dormancy() {
        let mut rng = Lcg::new(2);
        let mut s = RuntimeSpawner::init(&guard_tower(), 1, false, &mut rng);
        assert!(s.deactivates_after_spawn(&mut rng), "Once");
        s.spawner_type = spawner_type::SINGLE;
        assert!(!s.deactivates_after_spawn(&mut rng));
        s.state = state::ACTIVATE_ON_DEATH;
        assert!(s.is_dormant(0.5, None) && !s.is_dormant(0.0, None));
        s.state = state::ACTIVATE_ON_DAMAGE;
        assert!(s.is_dormant(1.0, Some((200.0, 200.0))) && !s.is_dormant(1.0, Some((150.0, 200.0))) && !s.is_dormant(1.0, None));
        s.state = state::ACTIVATE_ON_SEGMENT_DAMAGE;
        assert!(s.is_dormant(1.0, None) && !s.is_dormant(0.9, None));
    }

    #[test]
    fn active_range_by_radius_type() {
        let mut s = RuntimeSpawner::init(&guard_tower(), 1, false, &mut Lcg::new(1));
        let cam = [Vec3::new(0.0, 500.0, 90.0)];
        let p = Vec3::ZERO;
        assert!(s.in_active_range(p, &cam, &[]), "Camera2D ignores height: 90 ≤ 100");
        s.radius_type = radius_type::CAMERA_3D;
        assert!(!s.in_active_range(p, &cam, &[]));
        s.radius_type = radius_type::INVERSE_HIBERNATION_CAMERA_3D;
        assert!(s.in_active_range(p, &cam, &[]));
        s.radius_type = radius_type::PLAYER_2D;
        assert!(s.in_active_range(p, &cam, &[]), "no players → distance 0");
        s.radius_type = radius_type::INVERSE_HIBERNATION_CAMERA_2D;
        assert!(!s.in_active_range(p, &[], &[]), "no cameras → 0 ≥ r² is false");
        s.radius_type = radius_type::HIBERNATION;
        assert!(s.in_active_range(p, &[], &[]));
    }

    #[test]
    fn adjust_overrides_the_groups_fields() {
        let mut s = RuntimeSpawner::init(&guard_tower(), 1, false, &mut Lcg::new(1));
        let tw = SpawnerAdjust::from_tweak(
            &TweakValues {
                state: Some(0),
                chance_not_active: Some(30.0),
                chance_skip_given: true,
                seconds_per_cycle: Some(20.0),
                spawn_list: Some(0x8000_B001),
            },
            1 << 3,
        );
        s.apply_adjust(&tw);
        assert_eq!((s.state, s.seconds_per_cycle, s.unit_list), (0, 20.0, 0x8000_B001));
        assert!((s.not_active_chance - 0.3).abs() < 1e-6 && (s.skip_chance - 0.3).abs() < 1e-6);
        let mut other = RuntimeSpawner::init(&SpawnerRecord { group: 2, ..guard_tower() }, 1, false, &mut Lcg::new(1));
        other.apply_adjust(&tw);
        assert_eq!(other.state, 1, "another group is untouched");
    }
}
