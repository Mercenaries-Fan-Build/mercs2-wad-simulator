//! `PlayerController` — third-person on-foot locomotion for the local player.
//!
//! # Honesty boundary — read this before treating anything here as recovered
//!
//! **This is a modern stand-in, not a decompiled body.** Retail's on-foot controller is Havok:
//! `HumanPhysics::Activate` (`FUN_004255C0`) builds 7 capsules + a phantom + an `hkpCharacterProxy`,
//! driven by the 5-state `hkpCharacterContext` machine — all of it mapped in
//! `docs/reverse_engineer/physics_code_map.md`, and **none of it implemented here**. Every constant
//! below is gameplay-derived from human scale, not read from the exe.
//!
//! What this *is*: a testable controller with the right shape (eased ground speed, collide-and-slide,
//! jump/gravity, buoyant swim, a walk/run/idle clip FSM) sized so the Havok-faithful version can
//! replace it behind the same [`LocomotionQuery`] seam without disturbing callers.
//!
//! # Why it lives in `mercs2_player`
//!
//! It was inside `mercs2_engine`'s world loop, then extracted into `mercs2_engine::player`. It belongs
//! with the player concern, and moving it here is what lets the engine
//! re-export the whole concern as one crate. Its three world reads — collision world, terrain
//! heightfield, watermap — go through [`LocomotionQuery`] rather than direct crate edges, because a leaf
//! crate may not depend on another leaf (the carve rule).

use std::f32::consts::PI;

use mercs2_core::glam::{Quat, Vec3};
use mercs2_core::{AnimState, Entity, LocomotionQuery, SwimConfig, SwimState, Transform, World};

/// Player locomotion clip hashes (the per-merc idle is resolved at load; the FSM switches between
/// these walk/run and the resolved idle).
pub const CLIP_IDLE: u32 = 0x24F8_C8E6;
pub const CLIP_WALK: u32 = 0x5368_2784;
pub const CLIP_RUN: u32 = 0x867B_166D;

// Locomotion feel tunables (human scale; the 1.0 s walk cycle strides ~2 m, so ~2 m/s keeps feet
// planted under FOOT_SYNC). **Gameplay-derived, not exe-recovered** — see the module docs.
pub const WALK_SPEED: f32 = 2.2; // m/s
pub const RUN_SPEED: f32 = 6.5; // m/s (Shift)
const TURN_RATE: f32 = 12.0; // rad/s exponential yaw damp toward the move direction
const ACCEL: f32 = 12.0; // m/s^2 easing toward a higher target speed
const DECEL: f32 = 16.0; // m/s^2 easing toward a lower target speed
const FOOT_SYNC: bool = true; // scale locomotion playback by current/target speed (0.8..1.2)
const PLAYER_RADIUS: f32 = 0.35;
const PLAYER_HEIGHT: f32 = 1.8;
const STEP: f32 = 0.5;
/// Jump/fall vertical dynamics. `GRAVITY` ≈ the game's human gravity; `JUMP_SPEED` gives a ~1 m hop
/// (v²/2g). Landing is caught by a downward probe reaching `LAND_PROBE` below the feet.
const GRAVITY: f32 = 18.0; // m/s²
const JUMP_SPEED: f32 = 6.0; // m/s launch (apex ≈ 1.0 m)
const LAND_PROBE: f32 = 4.0; // m — how far below the feet a landing surface is caught
/// Swim locomotion: planar swim speed, the chest-deep rest waterline the body floats to (feet this far
/// below the surface), and how fast buoyancy eases the body to that line.
const SWIM_SPEED: f32 = 2.6; // m/s
const SWIM_WATERLINE: f32 = 1.2; // m — feet depth at the floating rest line
const BUOYANCY_RATE: f32 = 4.0; // m/s vertical ease toward the waterline

/// One raw frame of on-foot intent.
///
/// Bundling the three input channels is what lets [`PlayerController::update`] take four parameters
/// instead of nine — the old signature carried its own `#[allow(clippy::too_many_arguments)]`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocomotionInput {
    /// Planar input direction × magnitude.
    pub move_dir: Vec3,
    pub sprint: bool,
    /// Raw Jump-button state. The controller rising-edge-latches it, so holding it hops once.
    pub jump: bool,
}

/// The third-person player: locomotion state + the entity it drives.
///
/// `walk_speed`/`run_speed`/`dur_walk`/`dur_run`/`foot`/`idle`/`has_run`/`entity` are filled when the
/// avatar loads (the ground speeds are derived from each clip's baked root stride so the model advances
/// exactly as fast as its feet).
pub struct PlayerController {
    pub pos: Vec3,
    pub yaw: f32,
    pub speed: f32,
    pub move_dir: Vec3,
    /// Origin-to-lowest-vertex feet offset (so the avatar stands ON the ground sample).
    pub foot: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub dur_walk: f32,
    pub dur_run: f32,
    pub has_run: bool,
    pub idle: u32,
    /// Swimming locomotion clip (shared human swim anim, resolved at load). `0` = none loaded → the
    /// locomotion clips are used as a fallback while swimming.
    pub swim_clip: u32,
    pub entity: Option<Entity>,
    /// Vertical velocity (m/s) for jump/fall, and whether the feet are on the ground this frame.
    pub vel_y: f32,
    pub grounded: bool,
    /// Rising-edge latch for the jump button (jump fires once per press, not while held).
    jump_latch: bool,
    /// Swim FSM state driven by the watermap (feet-depth → OnLand/Wading/Swimming/Submerged).
    pub swim: SwimState,
}

impl PlayerController {
    /// A controller at `spawn_pos`, facing +Z, idle. Clip durations/speeds/foot/entity are filled when
    /// the player avatar loads.
    pub fn new(spawn_pos: Vec3) -> Self {
        PlayerController {
            pos: spawn_pos,
            yaw: 0.0,
            speed: 0.0,
            move_dir: Vec3::new(0.0, 0.0, 1.0),
            foot: 0.0,
            walk_speed: WALK_SPEED,
            run_speed: RUN_SPEED,
            dur_walk: 1.0,
            dur_run: 1.0,
            has_run: false,
            idle: CLIP_IDLE,
            swim_clip: 0,
            entity: None,
            vel_y: 0.0,
            grounded: true,
            jump_latch: false,
            swim: SwimState::OnLand,
        }
    }

    /// Advance one frame: classify swim state from the water column, ease ground speed toward the
    /// walk/run/swim target, collide-and-slide, apply jump/gravity (or buoyant float while swimming),
    /// turn toward motion, and drive the walk/run/idle clip FSM. Mutates the entity's `Transform` +
    /// `AnimState` in `world`.
    ///
    /// `q` supplies the three world reads. Whether ground comes from a terrain heightfield or a capsule
    /// probe is the query's business, not this function's — which is why there is no `interior` flag.
    pub fn update(
        &mut self,
        world: &mut World,
        input: LocomotionInput,
        q: &dyn LocomotionQuery,
        dt: f32,
    ) {
        let LocomotionInput { move_dir: mv, sprint, jump } = input;
        let swim_cfg = SwimConfig::default();

        // --- Swim classification: feet depth below the water surface drives the OnLand→Submerged FSM. ---
        let feet_y = self.pos.y - self.foot;
        let depth = match q.water_column(self.pos.x, self.pos.z) {
            Some(c) => c.surface_height - feet_y,
            None => -1.0, // dry: the FSM's "above water" sentinel
        };
        self.swim = swim_cfg.advance(self.swim, depth);
        let swimming = self.swim.is_swimming();

        // --- Horizontal speed ramp: ease toward the swim/walk/run target (or 0). ---
        let target_sp = if mv != Vec3::ZERO {
            if swimming {
                SWIM_SPEED
            } else if sprint {
                self.run_speed
            } else {
                self.walk_speed
            }
        } else {
            0.0
        };
        let rate = if target_sp > self.speed { ACCEL } else { DECEL };
        self.speed += (target_sp - self.speed).clamp(-rate * dt, rate * dt);
        if mv != Vec3::ZERO {
            self.move_dir = mv.normalize();
        }
        let moving = self.speed > 1e-3;
        if moving {
            // Capsule collide-and-slide against walls; Y is owned below (jump/gravity/float), so the
            // query's swept move must not also snap to the ground.
            let horiz = self.move_dir * self.speed * dt;
            self.pos = q.move_character(self.pos, horiz, PLAYER_RADIUS, PLAYER_HEIGHT, STEP);
        }

        // --- Vertical axis: buoyant float while swimming, else jump + gravity onto the ground. ---
        if swimming {
            // Buoyancy: ease the feet toward a rest waterline (chest-deep) so the body floats at the
            // surface instead of sinking. No ground snap, no gravity while swimming.
            if let Some(c) = q.water_column(self.pos.x, self.pos.z) {
                let target_y = c.surface_height - SWIM_WATERLINE + self.foot;
                self.pos.y += (target_y - self.pos.y).clamp(-BUOYANCY_RATE * dt, BUOYANCY_RATE * dt);
            }
            self.vel_y = 0.0;
            self.grounded = false;
        } else {
            // Ground height under the feet. The `+ foot` re-application stays here: the query answers in
            // feet-space, the controller's `pos` is the entity origin.
            let feet = Vec3::new(self.pos.x, self.pos.y - self.foot, self.pos.z);
            let ground = q.ground_height(feet, PLAYER_RADIUS, LAND_PROBE).map(|g| g + self.foot);
            // Jump on the button's rising edge, only when grounded.
            if jump && !self.jump_latch && self.grounded {
                self.vel_y = JUMP_SPEED;
                self.grounded = false;
            }
            self.vel_y -= GRAVITY * dt;
            self.pos.y += self.vel_y * dt;
            match ground {
                Some(gy) if self.pos.y <= gy && self.vel_y <= 0.0 => {
                    // Landed (or standing): rest on the ground, cancel downward velocity.
                    self.pos.y = gy;
                    self.vel_y = 0.0;
                    self.grounded = true;
                }
                _ => self.grounded = false, // airborne (jumping/falling) or over a gap
            }
        }
        self.jump_latch = jump;

        let Some(e) = self.entity else { return };
        if let Ok(mut t) = world.get::<&mut Transform>(e) {
            t.translation = self.pos;
            if moving {
                // Smooth turning: exponential yaw damp toward the move direction, shortest arc.
                let target = self.move_dir.x.atan2(self.move_dir.z);
                let d = (target - self.yaw + PI).rem_euclid(2.0 * PI) - PI;
                self.yaw += d * (1.0 - (-TURN_RATE * dt).exp());
                t.rotation = Quat::from_rotation_y(self.yaw);
            }
        }
        // Run under Shift, walk while moving, idle otherwise. A switch crossfades from the old clip;
        // walk<->run carries the normalized cycle phase so the feet stay in step (idle restarts at 0).
        if let Ok(mut a) = world.get::<&mut AnimState>(e) {
            let want = if swimming && self.swim_clip != 0 {
                // In water the whole body swims (arm strokes / tread) — one shared swim clip covers both
                // stroking forward and treading in place.
                self.swim_clip
            } else if mv != Vec3::ZERO {
                if sprint && self.has_run { CLIP_RUN } else { CLIP_WALK }
            } else {
                self.idle
            };
            if a.clip != want {
                a.prev_clip = a.clip;
                a.prev_time = a.time;
                a.blend = 0.0;
                a.time = if a.clip == CLIP_WALK && want == CLIP_RUN {
                    a.time / self.dur_walk * self.dur_run
                } else if a.clip == CLIP_RUN && want == CLIP_WALK {
                    a.time / self.dur_run * self.dur_walk
                } else {
                    0.0
                };
                a.clip = want;
            }
            // Foot-slide reduction: playback rate tracks the eased speed.
            a.speed = if FOOT_SYNC && want != self.idle && target_sp > 0.0 {
                (self.speed / target_sp).clamp(0.8, 1.2)
            } else {
                1.0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::physics_query::{ClosestPoint, PhysicsQuery, RayHit};
    use mercs2_core::WaterColumn;

    /// A synthetic world for the controller: an optional flat floor and an optional flat water surface.
    ///
    /// Implementing the seam rather than reaching for `mercs2_physics` is the point — these tests prove
    /// the controller works against the *contract*, which is what makes the Havok-backed implementation
    /// substitutable later.
    struct TestWorld {
        floor: Option<f32>,
        water: Option<f32>,
    }

    impl PhysicsQuery for TestWorld {
        fn raycast(&self, _o: Vec3, _d: Vec3, _m: f32) -> Option<RayHit> {
            None
        }
        fn closest_point(&self, _p: Vec3, _m: f32) -> Option<ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            // Open ground: the horizontal move always succeeds, and Y is the caller's.
            pos + Vec3::new(delta.x, 0.0, delta.z)
        }
    }

    impl LocomotionQuery for TestWorld {
        fn ground_height(&self, _feet: Vec3, _r: f32, _probe: f32) -> Option<f32> {
            self.floor
        }
        fn water_column(&self, _x: f32, _z: f32) -> Option<WaterColumn> {
            self.water.map(|surface_height| WaterColumn { surface_height })
        }
    }

    fn open_air() -> TestWorld {
        TestWorld { floor: None, water: None }
    }
    fn flat_floor(y: f32) -> TestWorld {
        TestWorld { floor: Some(y), water: None }
    }
    fn deep_water(surface: f32) -> TestWorld {
        TestWorld { floor: None, water: Some(surface) }
    }

    fn spawn_player(world: &mut World, pos: Vec3) -> Entity {
        world.spawn((Transform::from_translation(pos), AnimState::playing(CLIP_IDLE)))
    }

    fn walk(dir: Vec3) -> LocomotionInput {
        LocomotionInput { move_dir: dir, sprint: false, jump: false }
    }

    // ---- Deliverable 7, test 1: the migration is behaviour-preserving ----
    /// Walking forward advances the player and switches the clip to WALK.
    #[test]
    fn walks_forward_and_plays_walk_clip() {
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        let q = flat_floor(0.0);
        for _ in 0..120 {
            p.update(&mut world, walk(Vec3::new(0.0, 0.0, 1.0)), &q, 1.0 / 60.0);
        }
        assert!(p.pos.z > 1.0, "player should walk forward; z = {}", p.pos.z);
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, CLIP_WALK);
    }

    /// No input → speed decays to zero and the clip returns to idle.
    #[test]
    fn idle_when_no_input() {
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        p.speed = 5.0; // moving
        let q = flat_floor(0.0);
        for _ in 0..60 {
            p.update(&mut world, walk(Vec3::ZERO), &q, 1.0 / 60.0);
        }
        assert!(p.speed < 1e-2, "no input must decay speed to ~0, got {}", p.speed);
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, CLIP_IDLE);
    }

    /// Sprinting with a run clip available uses RUN and covers more ground than a walk.
    #[test]
    fn sprint_uses_run_clip_and_is_faster() {
        let mut world = World::new();
        let ew = spawn_player(&mut world, Vec3::ZERO);
        let er = spawn_player(&mut world, Vec3::ZERO);
        let mut w = PlayerController::new(Vec3::ZERO);
        w.entity = Some(ew);
        let mut r = PlayerController::new(Vec3::ZERO);
        r.entity = Some(er);
        r.has_run = true;
        let q = flat_floor(0.0);
        let fwd = Vec3::new(0.0, 0.0, 1.0);
        for _ in 0..120 {
            w.update(&mut world, walk(fwd), &q, 1.0 / 60.0);
            r.update(&mut world, LocomotionInput { move_dir: fwd, sprint: true, jump: false }, &q, 1.0 / 60.0);
        }
        assert_eq!(world.get::<&AnimState>(er).unwrap().clip, CLIP_RUN);
        assert!(r.pos.z > w.pos.z, "sprint should cover more ground: run {} vs walk {}", r.pos.z, w.pos.z);
    }

    /// Pressing Jump launches the player off the floor, then gravity returns them. The button is
    /// edge-latched: holding it does not re-launch mid-air.
    #[test]
    fn jump_launches_and_lands() {
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        let q = flat_floor(0.0);

        p.update(&mut world, walk(Vec3::ZERO), &q, 1.0 / 60.0);
        assert!(p.grounded, "player should rest on the floor");

        let held = LocomotionInput { move_dir: Vec3::ZERO, sprint: false, jump: true };
        let mut peak = p.pos.y;
        for _ in 0..24 {
            p.update(&mut world, held, &q, 1.0 / 60.0);
            peak = peak.max(p.pos.y);
        }
        assert!(peak > 0.5, "jump should lift the player well off the floor; peak y = {peak}");

        for _ in 0..180 {
            p.update(&mut world, walk(Vec3::ZERO), &q, 1.0 / 60.0);
        }
        assert!(p.pos.y.abs() < 0.05, "player should land back on the floor; y = {}", p.pos.y);
        assert!(p.grounded, "player should be grounded after landing");
    }

    /// Dropped into deep water, the swim FSM leaves land and buoyancy floats the body toward the
    /// surface waterline instead of sinking.
    #[test]
    fn swims_and_floats_in_deep_water() {
        let mut world = World::new();
        let start = Vec3::new(0.0, -3.0, 0.0);
        let e = spawn_player(&mut world, start);
        let mut p = PlayerController::new(start);
        p.entity = Some(e);
        let q = deep_water(0.0);
        for _ in 0..240 {
            p.update(&mut world, walk(Vec3::ZERO), &q, 1.0 / 60.0);
        }
        assert!(p.swim.in_water(), "should be in water; swim state = {:?}", p.swim);
        let feet_y = p.pos.y - p.foot;
        assert!(
            feet_y > -SWIM_WATERLINE - 0.3 && feet_y < 0.3,
            "buoyancy should float the body to the surface waterline; feet_y = {feet_y}"
        );
    }

    /// While swimming the FSM plays the resolved swim clip; back on land the locomotion clips return.
    #[test]
    fn swimming_plays_the_swim_clip() {
        const SWIM: u32 = 0x52CC_8375; // a resolved shared swim clip hash
        let mut world = World::new();
        let start = Vec3::new(0.0, -3.0, 0.0);
        let e = spawn_player(&mut world, start);
        let mut p = PlayerController::new(start);
        p.entity = Some(e);
        p.swim_clip = SWIM;

        let wet = deep_water(0.0);
        for _ in 0..60 {
            p.update(&mut world, walk(Vec3::new(0.0, 0.0, 1.0)), &wet, 1.0 / 60.0);
        }
        assert!(p.swim.in_water());
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, SWIM, "swimming should play the swim clip");

        let dry = flat_floor(-10.0);
        for _ in 0..120 {
            p.update(&mut world, walk(Vec3::new(0.0, 0.0, 1.0)), &dry, 1.0 / 60.0);
        }
        assert!(!p.swim.in_water());
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, CLIP_WALK, "on land the walk clip returns");
    }

    // ---- Deliverable 7, test 2: the seam's "no ground" contract ----
    /// `ground_height` returning `None` means a gap, and the controller must fall through it rather
    /// than treat the absence as y = 0.
    #[test]
    fn no_ground_means_falling() {
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        let q = open_air();
        for _ in 0..60 {
            p.update(&mut world, walk(Vec3::ZERO), &q, 1.0 / 60.0);
        }
        assert!(p.pos.y < -1.0, "with nothing to stand on the player falls; y = {}", p.pos.y);
        assert!(!p.grounded);
    }
}
