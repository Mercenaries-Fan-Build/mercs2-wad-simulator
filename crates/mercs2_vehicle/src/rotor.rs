//! `PgRotorBoneController` — the per-rotor bone controller that spins a rotor's hub and swaps its
//! solid blades for its motion-blur discs by rotor speed.
//!
//! The builder `FUN_00683D70` makes one controller (0x2C bytes, vtable `0x00BCD5F8`) per `Rotor`
//! record of the entity's template ([`mercs2_formats::rotor`]). Its update, slot 2 `0x00638000`
//! (x360 `0x824CDB70`, PS3 `0x001E8B28`, whose RTTI names the class):
//!
//! ```text
//! speed = HelicopterPhysics+0x3C                     // 0..1, via PhysicsActor vfunc +0x64
//! kill  = actor.u8[+0x15] | ctl.u8[+0x18]
//! angle = wrap_pi(angle + SpinDir*speed*dt*30.0)     // 30.0 @0x00B977D0
//! set_node(obj, node(RotorBlurOnBone),  speed > 0.3 && !kill)   // 0.3 @0x00B9B688
//! set_node(obj, node(RotorBlurOffBone), speed < 0.9 && !kill)   // 0.9 @0x00BB3E44
//! ```
//!
//! The compares are strict (`COMISS` + `JBE`): between 0.3 and 0.9 both meshes show, and a NaN speed
//! hides both. There is no hysteresis and no fade; the two nodes are re-recorded on every update
//! through `0x00472960` ([`NodeEnableRecords::set`]), and a node the model does not have is skipped.
//! Slot 5 `0x006381F0` turns the hub node's local transform by `angle` about its local +Y.
//!
//! The latch `+0x18` is set by `FUN_004B16B0` on the state machine's controller events `0xB4A26AA7`
//! and `0x6CCBB80A` ([`CMD_CONTROLLER_EVENT`](mercs2_formats::orchestrator::CMD_CONTROLLER_EVENT)) when
//! the event's node is the hub, or an ancestor or descendant of it; nothing clears it.

use mercs2_formats::orchestrator::{HierNode, NodeEnableRecords};
use mercs2_formats::rotor::RotorRecord;

/// Above this rotor speed the blur discs show (`0x00B9B688`).
pub const BLUR_ON_ABOVE: f32 = 0.3;
/// Below this rotor speed the blades show (`0x00BB3E44`).
pub const BLADES_BELOW: f32 = 0.9;
/// Hub angular rate at rotor speed 1, radians per second (`0x00B977D0`).
pub const SPIN_RATE: f32 = 30.0;
/// The controller events whose handler latches the rotor controllers (`FUN_004B16B0`).
pub const ROTOR_KILL_EVENTS: [u32; 2] = [0xB4A2_6AA7, 0x6CCB_B80A];

const PI: f32 = f32::from_bits(0x4049_0FDB); // 0x00B9285C
const INV_TWO_PI: f32 = f32::from_bits(0x3E22_F983); // 0x017D40B8
const TWO_PI: f32 = f32::from_bits(0x40C9_0FDB); // 0x00DFCB90

/// The update's angle wrap into `[-pi, pi)`: `a - floor((a + pi) / 2pi) * 2pi` (`FUN_00407840` floor).
pub fn wrap_pi(a: f32) -> f32 {
    a - ((a + PI) * INV_TWO_PI).floor() * TWO_PI
}

/// One rotor's controller state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotorController {
    /// `+0x1C`
    pub hub: u32,
    /// `+0x20`
    pub spin_dir: f32,
    /// `+0x24`
    pub blur_on: u32,
    /// `+0x28`
    pub blur_off: u32,
    /// `+0x14`, radians.
    pub angle: f32,
    /// `+0x18`: latched by a rotor kill event.
    pub killed: bool,
}

/// What one update shows: the blur discs, the blades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RotorShows {
    pub blur: bool,
    pub blades: bool,
}

impl RotorController {
    /// `FUN_00683D70`: a fresh controller, angle 0, not latched.
    pub fn new(r: &RotorRecord) -> RotorController {
        RotorController { hub: r.hub, spin_dir: r.spin_dir, blur_on: r.blur_on, blur_off: r.blur_off, angle: 0.0, killed: false }
    }

    /// `0x00638000`: advance the angle by `dt` seconds at rotor `speed` and say what shows.
    /// `actor_killed` is the physics actor's byte `+0x15`.
    pub fn update(&mut self, speed: f32, dt: f32, actor_killed: bool) -> RotorShows {
        self.angle = wrap_pi(self.spin_dir * speed * dt * SPIN_RATE + self.angle);
        self.shows(speed, actor_killed)
    }

    /// What the update shows at rotor `speed`, without turning the hub.
    pub fn shows(&self, speed: f32, actor_killed: bool) -> RotorShows {
        let kill = actor_killed || self.killed;
        RotorShows { blur: speed > BLUR_ON_ABOVE && !kill, blades: speed < BLADES_BELOW && !kill }
    }

    /// Record what `shows` into the object's node-enable records, for the blur and blade nodes the
    /// model has.
    pub fn record(&self, shows: RotorShows, hier: &[HierNode], records: &mut NodeEnableRecords) -> Result<(), String> {
        for (hash, on) in [(self.blur_on, shows.blur), (self.blur_off, shows.blades)] {
            if let Some(n) = hier.iter().find(|n| n.hash == hash) {
                records.set(n.index, on)?;
            }
        }
        Ok(())
    }

    /// `FUN_004B16B0`: latch on a rotor kill event whose node is this hub or an ancestor or descendant
    /// of it. Events other than [`ROTOR_KILL_EVENTS`], and nodes or hubs the model lacks, do nothing.
    pub fn on_controller_event(&mut self, event: u32, node: u32, hier: &[HierNode]) {
        if !ROTOR_KILL_EVENTS.contains(&event) {
            return;
        }
        let find = |h: u32| hier.iter().find(|n| n.hash == h).map(|n| n.index);
        let (Some(hub), Some(ev)) = (find(self.hub), find(node)) else { return };
        let above = |mut i: usize, a: usize| loop {
            match hier[i].parent {
                Some(p) if p == a => break true,
                Some(p) => i = p,
                None => break false,
            }
        };
        if hub == ev || above(hub, ev) || above(ev, hub) {
            self.killed = true;
        }
    }
}

/// Slot 5 `0x006381F0` over a skinning palette: turn the hub by `angle` about its own local +Y and
/// carry its subtree with it. The hub's world transform is `world_bind_hub · palette[hub]`; turning
/// its local transform turns its world transform to `R · world`, so every node of `subtree` (the hub
/// first) takes `palette · inv(world) · R · world`. The palette may already hold a sampled clip pose.
/// The order of the turn against the local rotation is INFERRED (the update multiplies an axis-angle
/// rotation built by `0x0088D400` into the transform's rotation part).
pub fn spin_subtree(palette: &mut [[[f32; 4]; 4]], world_bind_hub: &[[f32; 4]; 4], subtree: &[usize], angle: f32) -> Result<(), String> {
    use mercs2_formats::skeleton::{affine_inverse, mat4_mul};
    let hub = *subtree.first().ok_or("an empty rotor subtree")?;
    let n = palette.len();
    if let Some(&b) = subtree.iter().find(|&&b| b >= n) {
        return Err(format!("rotor node {b} is past the {n}-entry palette"));
    }
    let world = mat4_mul(world_bind_hub, &palette[hub]);
    let (s, c) = angle.sin_cos();
    let r = [[c, 0.0, -s, 0.0], [0.0, 1.0, 0.0, 0.0], [s, 0.0, c, 0.0], [0.0, 0.0, 0.0, 1.0]];
    let delta = mat4_mul(&affine_inverse(&world), &mat4_mul(&r, &world));
    for &b in subtree {
        palette[b] = mat4_mul(&palette[b], &delta);
    }
    Ok(())
}

/// `0x00BCD278`: the decay tick, 1/60 s.
const TICK: f32 = f32::from_bits(0x3C88_8889);
/// `0x00BEAA78`: a step longer than this does not decay tick by tick.
const LONG_STEP: f32 = f32::from_bits(0x4185_5556);
/// `0x00BEAA74`
const KEEP_RATE: f32 = f32::from_bits(0x3F7F_F972);
/// `0x00B92B3C`
const EPS_SCALE: f32 = f32::from_bits(0x3727_C5AC);
/// `0x00BEAA70`: 59.999996, the fractional tick's weight per second.
const TICKS_PER_SECOND: f32 = f32::from_bits(0x426F_FFFF);
/// `0x00BBC7EC`: spin-up per second of positive vertical control.
pub const SPIN_UP_PER_SECOND: f32 = 0.25;
/// `0x00B9B700`: the vertical control value that sets the speed outright.
const HOLD_INPUT: f32 = 5.0;
/// `0x00DFDDC4`
const HOLD_SPEED: f32 = f32::from_bits(0x3F73_3333);
/// `0x00BBB99C`: seconds of contact before the rotor runs down.
const CONTACT_GATE: f32 = 0.5;
/// `0x00B92B58`
const SLOW: f32 = f32::from_bits(0x3DCC_CCCD);
/// `0x00DFDDE0`
const RATE_PUSHED_DOWN: f32 = f32::from_bits(0x3F79_999A);
/// `0x00D2D838`
const RATE_IDLE: f32 = f32::from_bits(0x3F7D_70A4);
/// `0x00DFDDC4`, the idle rate below [`SLOW`].
const RATE_IDLE_SLOW: f32 = f32::from_bits(0x3F73_3333);
/// `0x00BEAA88`
const RATE_LOST: f32 = f32::from_bits(0x3F6C_CCCD);

/// `0x00453B30`: run `s` down toward `-0.0` at rate `r` per 1/60 s tick over `dt` seconds.
pub fn decay(r: f32, dt: f32, s: f32) -> f32 {
    let x1 = r * EPS_SCALE;
    if dt > LONG_STEP {
        return if r > KEEP_RATE { s } else { 0.0 };
    }
    let mut cur = s;
    let omr = 1.0 - r;
    let mut dt = dt;
    if dt > TICK {
        let eps2 = x1 * x1;
        loop {
            let d = -0.0 - cur;
            if eps2 > d * d {
                return 0.0;
            }
            cur = d * omr + cur;
            dt -= TICK;
            if !(dt > TICK) {
                break;
            }
        }
    }
    let frac = dt * TICKS_PER_SECOND;
    let s1 = (-0.0 - cur) * omr + cur;
    s1 * frac + (1.0 - frac) * cur
}

/// A helicopter's rotor speed, `HelicopterPhysics+0x3C` (0..1), and the state its step reads: what
/// [`RotorController::update`] is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeliRotor {
    /// `+0x3C`
    pub speed: f32,
    /// `+0x44`: 2 and 3 latch a forced run-down. 3 is set by an object the step looks up by key
    /// `0x2001` (not decoded) and is not modelled.
    pub mode: u32,
    /// `+0x48`: the main rotor is lost (message `0x840FD56A`).
    pub rotor_lost: bool,
    /// `+0x4C`: seconds of contact, to 1.
    pub contact_time: f32,
    /// `+0x50`: seconds without contact, to 1.
    pub air_time: f32,
}

impl HeliRotor {
    /// `FUN_00453090`: 0, or full speed when the vehicle is created with its crew aboard (`+0x68`).
    pub fn new(crewed: bool) -> HeliRotor {
        HeliRotor { speed: if crewed { 1.0 } else { 0.0 }, mode: 0, rotor_lost: false, contact_time: 0.0, air_time: 0.0 }
    }

    /// One physics step (applyAction `0x00453760`): the speed step `FUN_004517B0` on the previous
    /// step's mode and timers, then `FUN_004516A0`'s mode and contact timers. `vertical` is the last
    /// vertical-control command (`+0x28`, verb `0xE6FA6B1B`: -1..1, or 5.0), `contacts` the contact
    /// count of the listener at `+0xD8`, `actor_killed` the physics actor's byte `+0x15`.
    pub fn step(&mut self, dt: f32, vertical: f32, contacts: u32, actor_killed: bool) {
        if dt == 0.0 {
            return;
        }
        let rate = if self.rotor_lost {
            Some(RATE_LOST)
        } else if self.mode == 2 {
            Some(RATE_IDLE)
        } else if self.mode == 3 {
            Some(RATE_LOST)
        } else if vertical > 0.0 {
            self.speed = if vertical == HOLD_INPUT { HOLD_SPEED } else { dt * SPIN_UP_PER_SECOND + self.speed };
            None
        } else if !(self.contact_time > CONTACT_GATE) {
            None
        } else if 0.0 > vertical {
            Some(RATE_PUSHED_DOWN)
        } else if self.speed > SLOW {
            Some(RATE_IDLE)
        } else {
            Some(RATE_IDLE_SLOW)
        };
        if let Some(r) = rate {
            self.speed = decay(r, dt, self.speed);
        }
        if 0.0 > self.speed {
            self.speed = 0.0;
        } else if self.speed > 1.0 {
            self.speed = 1.0;
        }

        if self.mode != 2 && self.mode != 3 {
            self.mode = if self.speed == 0.0 || actor_killed { 0 } else { 1 };
        }
        let clamp01 = |v: f32| if 0.0 > v { 0.0 } else if v > 1.0 { 1.0 } else { v };
        if contacts > 0 {
            self.contact_time = clamp01(self.contact_time + dt);
            self.air_time = 0.0;
        } else {
            let v = dt + self.air_time;
            self.contact_time = 0.0;
            self.air_time = clamp01(v);
        }
    }

    /// `FUN_004B16B0` → actor slot `+0xEC` (`0x004397A0`): the controller events that reach the
    /// helicopter physics. `0xB4A26AA7` (main rotor destroyed) loses the rotor; `0x9CC8AF58` latches
    /// mode 2. Others do nothing to the speed.
    pub fn on_controller_event(&mut self, event: u32) {
        match event {
            0xB4A2_6AA7 => self.rotor_lost = true,
            0x9CC8_AF58 => self.mode = 2,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec() -> RotorRecord {
        RotorRecord {
            hub: 0xB366_B8C7,
            spin_dir: -1.0,
            first_blade: 0,
            min_angle: 0.0,
            max_angle: 0.0,
            blur_on: 0x4CC6_28FA,
            blur_off: 0x2C9F_4CEC,
        }
    }

    #[test]
    fn the_swap_is_two_strict_thresholds_with_both_between() {
        let mut c = RotorController::new(&rec());
        let at = |c: &mut RotorController, s: f32| c.update(s, 0.0, false);
        assert_eq!(at(&mut c, 0.0), RotorShows { blur: false, blades: true });
        assert_eq!(at(&mut c, 0.3), RotorShows { blur: false, blades: true }, "0.3 is not above 0.3");
        assert_eq!(at(&mut c, 0.5), RotorShows { blur: true, blades: true });
        assert_eq!(at(&mut c, 0.9), RotorShows { blur: true, blades: false }, "0.9 is not below 0.9");
        assert_eq!(at(&mut c, 1.0), RotorShows { blur: true, blades: false });
        assert_eq!(at(&mut c, f32::NAN), RotorShows { blur: false, blades: false });
        assert_eq!(c.update(1.0, 0.0, true), RotorShows { blur: false, blades: false }, "actor +0x15");
    }

    #[test]
    fn the_angle_advances_30_rad_per_second_at_full_speed_and_wraps() {
        let mut c = RotorController::new(&rec());
        c.update(1.0, 0.01, false);
        assert!((c.angle - -0.3).abs() < 1e-6, "SpinDir -1 turns backwards: {}", c.angle);
        c.angle = 3.0;
        c.spin_dir = 1.0;
        c.update(1.0, 0.01, false);
        assert!((c.angle - (3.3 - 2.0 * std::f32::consts::PI)).abs() < 1e-5, "{}", c.angle);
        assert!((-std::f32::consts::PI..std::f32::consts::PI).contains(&wrap_pi(std::f32::consts::PI)));
    }

    fn node(index: usize, hash: u32, parent: Option<usize>) -> HierNode {
        HierNode { index, hash, parent, local: [0.0; 16], bbox_min: [0.0; 3], bbox_max: [0.0; 3] }
    }

    #[test]
    fn records_go_to_the_blur_nodes_and_their_children_follow() {
        // 0 root, 1 hub, 2 blur-on, 3 blur-off, 4 a blade under blur-off.
        let hier = vec![
            node(0, 1, None),
            node(1, 0xB366_B8C7, Some(0)),
            node(2, 0x4CC6_28FA, Some(1)),
            node(3, 0x2C9F_4CEC, Some(1)),
            node(4, 5, Some(3)),
        ];
        let mut records = NodeEnableRecords { base: true, records: vec![None; 5] };
        let mut c = RotorController::new(&rec());
        let shows = c.update(0.0, 0.0, false);
        c.record(shows, &hier, &mut records).unwrap();
        assert_eq!(records.table(&hier).unwrap(), vec![true, true, false, true, true]);
        let shows = c.update(1.0, 0.0, false);
        c.record(shows, &hier, &mut records).unwrap();
        assert_eq!(records.table(&hier).unwrap(), vec![true, true, true, false, false], "the blade under blur-off hides");
    }

    #[test]
    fn decay_steps_whole_ticks_then_blends_the_fraction() {
        // One whole tick at 0.5: 1 -> (−1)(0.5) + 1 = 0.5; the remaining fraction is ~0.
        let one_tick = decay(0.5, TICK * 1.5, 1.0);
        assert!(one_tick < 0.5 && one_tick > 0.25, "{one_tick}");
        // Under a tick: only the fractional blend, no snap.
        let half = decay(0.5, TICK * 0.5, 1.0);
        assert!((half - 0.75).abs() < 1e-3, "{half}");
        // A step past the cut-off keeps a near-1 rate's value and zeroes any other.
        assert_eq!(decay(0.99999, 20.0, 0.7), 0.7);
        assert_eq!(decay(0.99, 20.0, 0.7), 0.0);
        // The snap: a value whose square is under (r·1e-5)² goes to 0 on a whole tick.
        assert_eq!(decay(0.99, TICK * 2.0, 1e-7), 0.0);
    }

    #[test]
    fn engine_on_spins_up_in_four_seconds_and_holds_in_the_air() {
        let mut h = HeliRotor::new(false);
        for _ in 0..240 {
            h.step(1.0 / 60.0, 1.0, 8, false);
        }
        assert!((h.speed - 1.0).abs() < 1e-3, "{}", h.speed);
        assert_eq!(h.mode, 1);
        // Airborne (no contacts), no input: the first step still reads the last step's contact time
        // and runs down once; from then on the speed holds.
        let mut air = h;
        air.step(1.0 / 60.0, 0.0, 0, false);
        assert!(air.speed < h.speed);
        assert_eq!(air.contact_time, 0.0);
        let lifted = air.speed;
        for _ in 0..120 {
            air.step(1.0 / 60.0, 0.0, 0, false);
        }
        assert_eq!(air.speed, lifted);
        // On the ground with no input: held until 0.5 s of contact, then it runs down at 0.99/tick.
        let mut ground = HeliRotor { contact_time: 0.0, ..h };
        for _ in 0..30 {
            ground.step(1.0 / 60.0, 0.0, 8, false);
        }
        assert_eq!(ground.speed, h.speed, "no decay before 0.5 s of contact");
        for _ in 0..600 {
            ground.step(1.0 / 60.0, 0.0, 8, false);
        }
        assert!(ground.speed < 0.01, "{}", ground.speed);
    }

    #[test]
    fn the_hold_input_and_a_lost_rotor() {
        let mut h = HeliRotor::new(false);
        h.step(0.1, 5.0, 0, false);
        assert_eq!(h.speed, HOLD_SPEED);
        h.on_controller_event(0xB4A2_6AA7);
        h.step(1.0, 1.0, 0, false);
        assert!(h.speed < HOLD_SPEED * 0.5, "a lost rotor runs down even under input: {}", h.speed);
    }

    #[test]
    fn spinning_a_rest_palette_turns_the_hub_subtree_about_its_own_y() {
        // Hub at world (2,0,0), identity bind, so palette = world for both nodes.
        let hub = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [2.0, 0.0, 0.0, 1.0]];
        let blade = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [3.0, 0.0, 0.0, 1.0]];
        let mut pal = vec![hub, blade];
        let ident = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        spin_subtree(&mut pal, &ident, &[0, 1], std::f32::consts::FRAC_PI_2).unwrap();
        // The hub stays put; the blade 1 m out along +X swings a quarter turn about the hub's Y.
        assert!((pal[0][3][0] - 2.0).abs() < 1e-5 && pal[0][3][2].abs() < 1e-5);
        assert!((pal[1][3][0] - 2.0).abs() < 1e-5, "{:?}", pal[1][3]);
        assert!((pal[1][3][2].abs() - 1.0).abs() < 1e-5, "{:?}", pal[1][3]);
    }

    #[test]
    fn a_kill_event_latches_on_the_hub_its_ancestors_and_descendants_only() {
        let hier = vec![node(0, 1, None), node(1, 0xA998_B636, Some(0)), node(2, 0xB366_B8C7, Some(1)), node(3, 9, Some(0))];
        for (event, node_hash, latched) in [
            (0xB4A2_6AA7, 0xA998_B636, true),  // ancestor of the hub
            (0x6CCB_B80A, 0xB366_B8C7, true),  // the hub
            (0xB4A2_6AA7, 9, false),           // an unrelated branch
            (0x1234_5678, 0xB366_B8C7, false), // not a rotor kill event
        ] {
            let mut c = RotorController::new(&rec());
            c.on_controller_event(event, node_hash, &hier);
            assert_eq!(c.killed, latched, "event 0x{event:08X} node 0x{node_hash:08X}");
        }
    }
}
