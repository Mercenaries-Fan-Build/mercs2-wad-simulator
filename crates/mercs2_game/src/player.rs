//! `PlayerController` — third-person player locomotion, extracted from the world loop.
//!
//! This logic was buried inside `world::run_scene_world_loading`'s 1600-line event-loop closure, so it
//! could not be tested. Pulled into a struct with a real `update` signature (input `mv` in, world
//! mutation out) so the eased speed / collide-and-slide / terrain ground-snap / walk-run-idle clip FSM
//! is **unit-testable without the GUI**. Behaviour is preserved verbatim from the closure.

use std::f32::consts::PI;

use mercs2_core::glam::{Quat, Vec3};
use mercs2_core::{AnimState, Entity, Transform, World};
use mercs2_engine::worldutil::HeightMap;

use crate::collision;

/// Player locomotion clip hashes (the per-merc idle is resolved at load; the FSM switches between
/// these walk/run and the resolved idle).
pub const CLIP_IDLE: u32 = 0x24F8_C8E6;
pub const CLIP_WALK: u32 = 0x5368_2784;
pub const CLIP_RUN: u32 = 0x867B_166D;

// Locomotion feel tunables (human scale; the 1.0 s walk cycle strides ~2 m, so ~2 m/s keeps feet
// planted under FOOT_SYNC).
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

/// The third-person player: locomotion state + the entity it drives. `walk_speed`/`run_speed`/
/// `dur_walk`/`dur_run`/`foot`/`idle`/`has_run`/`entity` are filled when the avatar loads (the ground
/// speeds are derived from each clip's baked root stride so the model advances exactly as fast as its
/// feet).
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
    pub swim: mercs2_water::SwimState,
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
            swim: mercs2_water::SwimState::OnLand,
        }
    }

    /// Advance one frame for planar move `mv` (input direction × magnitude): classify swim state from the
    /// `water` map, ease ground speed toward the walk/run/swim target, collide-and-slide against
    /// `collision`, apply jump/gravity (or buoyant float while swimming) for the vertical axis, turn
    /// toward motion, and drive the walk/run/idle clip FSM. `jump` is the raw Jump-button state (this
    /// controller rising-edge-latches it, so holding it hops once). Mutates the entity's
    /// `Transform`+`AnimState` in `world`.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        world: &mut World,
        mv: Vec3,
        sprint: bool,
        jump: bool,
        collision: &[[Vec3; 3]],
        hmap: Option<&HeightMap>,
        water: Option<&mercs2_water::Watermap>,
        interior: bool,
        dt: f32,
    ) {
        let swim_cfg = mercs2_water::SwimConfig::default();

        // --- Swim classification: feet depth below the water surface drives the OnLand→Submerged FSM. ---
        let feet_y = self.pos.y - self.foot;
        let depth = water
            .map(|wm| {
                let s = wm.sample(self.pos.x, self.pos.z);
                if s.is_water { s.surface_height - feet_y } else { -1.0 }
            })
            .unwrap_or(-1.0);
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
            let horiz = self.move_dir * self.speed * dt;
            if !collision.is_empty() {
                // Capsule collide-and-slide against walls; Y is owned below (jump/gravity/float), so the
                // ground probe here is disabled (follow_ground=false).
                self.pos = collision::move_character(
                    collision, self.pos, horiz, PLAYER_RADIUS, PLAYER_HEIGHT, STEP, false,
                );
            } else {
                self.pos += horiz;
            }
        }

        // --- Vertical axis: buoyant float while swimming, else jump + gravity onto the ground. ---
        if swimming {
            // Buoyancy: ease the feet toward a rest waterline (chest-deep) so the body floats at the
            // surface instead of sinking. No ground snap, no gravity while swimming.
            if let Some(wm) = water {
                let surface = wm.sample(self.pos.x, self.pos.z).surface_height;
                let target_y = surface - SWIM_WATERLINE + self.foot;
                self.pos.y += (target_y - self.pos.y).clamp(-BUOYANCY_RATE * dt, BUOYANCY_RATE * dt);
            }
            self.vel_y = 0.0;
            self.grounded = false;
        } else {
            // Ground height under the feet: terrain heightmap outdoors, a downward capsule probe indoors.
            let ground = if !interior {
                hmap.map(|hm| hm.height_at_near(self.pos.x, self.pos.z, self.pos.y - self.foot) + self.foot)
            } else {
                collision::ground_below(collision, self.pos, PLAYER_RADIUS, LAND_PROBE)
            };
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

    fn spawn_player(world: &mut World, pos: Vec3) -> Entity {
        world.spawn((Transform::from_translation(pos), AnimState::playing(CLIP_IDLE)))
    }

    /// Walking forward advances the player and switches the clip to WALK (open ground: no walls, no
    /// terrain — interior mode).
    #[test]
    fn walks_forward_and_plays_walk_clip() {
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        for _ in 0..120 {
            p.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, false, &[], None, None, true, 1.0 / 60.0);
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
        for _ in 0..60 {
            p.update(&mut world, Vec3::ZERO, false, false, &[], None, None, true, 1.0 / 60.0);
        }
        assert!(p.speed < 1e-2, "no input must decay speed to ~0, got {}", p.speed);
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, CLIP_IDLE);
    }

    /// Sprinting with a run clip available uses RUN and covers more ground than a walk in the same time.
    #[test]
    fn sprint_uses_run_clip_and_is_faster() {
        let mut world = World::new();
        let ew = spawn_player(&mut world, Vec3::ZERO);
        let er = spawn_player(&mut world, Vec3::ZERO);
        let mut walk = PlayerController::new(Vec3::ZERO);
        walk.entity = Some(ew);
        let mut run = PlayerController::new(Vec3::ZERO);
        run.entity = Some(er);
        run.has_run = true;
        for _ in 0..120 {
            walk.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, false, &[], None, None, true, 1.0 / 60.0);
            run.update(&mut world, Vec3::new(0.0, 0.0, 1.0), true, false, &[], None, None, true, 1.0 / 60.0);
        }
        assert_eq!(world.get::<&AnimState>(er).unwrap().clip, CLIP_RUN);
        assert!(run.pos.z > walk.pos.z, "sprint should cover more ground: run {} vs walk {}", run.pos.z, walk.pos.z);
    }

    /// A flat walkable floor of unit triangles at `y` spanning the origin (for jump/ground tests).
    fn flat_floor(y: f32) -> Vec<[Vec3; 3]> {
        let mut tris = Vec::new();
        for xi in -3..3 {
            for zi in -3..3 {
                let (x0, x1) = (xi as f32, xi as f32 + 1.0);
                let (z0, z1) = (zi as f32, zi as f32 + 1.0);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z0), Vec3::new(x1, y, z1)]);
                tris.push([Vec3::new(x0, y, z0), Vec3::new(x1, y, z1), Vec3::new(x0, y, z1)]);
            }
        }
        tris
    }

    /// A synthetic uniform water map: surface at `surface`, every cell wet (for swim tests). The wet
    /// mask uses the format's WET sentinel (255), not a bare 1.
    fn flat_water(surface: f32) -> mercs2_water::Watermap {
        let (w, h) = (4usize, 4usize);
        mercs2_water::Watermap::from_parts(w, h, 32.0, -48.0, -48.0, vec![surface; w * h], vec![255u8; w * h])
    }

    /// Pressing Jump launches the player off the floor (apex ≈ 1 m), then gravity returns them to the
    /// ground and re-grounds them. The button is edge-latched: holding it does not re-launch mid-air.
    #[test]
    fn jump_launches_and_lands() {
        let tris = flat_floor(0.0);
        let mut world = World::new();
        let e = spawn_player(&mut world, Vec3::ZERO);
        let mut p = PlayerController::new(Vec3::ZERO);
        p.entity = Some(e);
        // Settle on the floor first.
        p.update(&mut world, Vec3::ZERO, false, false, &tris, None, None, true, 1.0 / 60.0);
        assert!(p.grounded, "player should rest on the floor");
        // Jump (held): rises off the floor. Latch means the hold only launches once.
        let mut peak = p.pos.y;
        for _ in 0..24 {
            p.update(&mut world, Vec3::ZERO, false, true, &tris, None, None, true, 1.0 / 60.0);
            peak = peak.max(p.pos.y);
        }
        assert!(peak > 0.5, "jump should lift the player well off the floor; peak y = {peak}");
        // Release + fall: lands back on the floor, grounded again.
        for _ in 0..180 {
            p.update(&mut world, Vec3::ZERO, false, false, &tris, None, None, true, 1.0 / 60.0);
        }
        assert!(p.pos.y.abs() < 0.05, "player should land back on the floor; y = {}", p.pos.y);
        assert!(p.grounded, "player should be grounded after landing");
    }

    /// Dropped into deep water, the swim FSM leaves land (reaches an in-water state) and buoyancy floats
    /// the body up toward the surface waterline instead of sinking away.
    #[test]
    fn swims_and_floats_in_deep_water() {
        let water = flat_water(0.0); // surface at y = 0
        let mut world = World::new();
        let start = Vec3::new(0.0, -3.0, 0.0); // start submerged
        let e = spawn_player(&mut world, start);
        let mut p = PlayerController::new(start);
        p.entity = Some(e);
        for _ in 0..240 {
            p.update(&mut world, Vec3::ZERO, false, false, &[], None, Some(&water), false, 1.0 / 60.0);
        }
        assert!(p.swim.in_water(), "should be in water; swim state = {:?}", p.swim);
        let feet_y = p.pos.y - p.foot;
        assert!(
            feet_y > -SWIM_WATERLINE - 0.3 && feet_y < 0.3,
            "buoyancy should float the body to the surface waterline; feet_y = {feet_y}"
        );
    }

    /// While swimming, the animation FSM plays the resolved swim clip (not walk/run/idle) — on land it
    /// returns to the locomotion clips.
    #[test]
    fn swimming_plays_the_swim_clip() {
        const SWIM: u32 = 0x52CC_8375; // a resolved shared swim clip hash
        let water = flat_water(0.0);
        let mut world = World::new();
        let start = Vec3::new(0.0, -3.0, 0.0);
        let e = spawn_player(&mut world, start);
        let mut p = PlayerController::new(start);
        p.entity = Some(e);
        p.swim_clip = SWIM;
        // In deep water — moving or not — the swim clip plays.
        for _ in 0..60 {
            p.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, false, &[], None, Some(&water), false, 1.0 / 60.0);
        }
        assert!(p.swim.in_water());
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, SWIM, "swimming should play the swim clip");
        // Back on dry land (no watermap): the walk clip returns.
        for _ in 0..120 {
            p.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, false, &[], None, None, true, 1.0 / 60.0);
        }
        assert!(!p.swim.in_water());
        assert_eq!(world.get::<&AnimState>(e).unwrap().clip, CLIP_WALK, "on land the walk clip returns");
    }
}
