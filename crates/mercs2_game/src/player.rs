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
    pub entity: Option<Entity>,
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
            entity: None,
        }
    }

    /// Advance one frame for planar move `mv` (input direction × magnitude): ease ground speed toward
    /// the walk/run target, collide-and-slide against `collision` (or terrain ground-snap when not
    /// `interior`), turn toward motion, and drive the walk/run/idle clip FSM on the entity's
    /// `AnimState`. Mutates the entity's `Transform`+`AnimState` in `world`.
    pub fn update(
        &mut self,
        world: &mut World,
        mv: Vec3,
        sprint: bool,
        collision: &[[Vec3; 3]],
        hmap: Option<&HeightMap>,
        interior: bool,
        dt: f32,
    ) {
        // Speed ramp: ease toward the walk/run target (or 0) so starts/stops/gait changes aren't instant.
        let target_sp = if mv != Vec3::ZERO {
            if sprint { self.run_speed } else { self.walk_speed }
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
                // Capsule controller: collide-and-slide + (interior) ground probe — mirrors the engine's
                // Havok capsule (MatchCapsuleToPose). Exterior Y comes from the terrain heightmap below.
                self.pos = collision::move_character(
                    collision, self.pos, horiz, PLAYER_RADIUS, PLAYER_HEIGHT, STEP, interior,
                );
            } else {
                self.pos += horiz;
            }
        }
        // Ground snap: feet follow the terrain heightmap (skipped for interior, floor at Y≈450).
        if !interior {
            if let Some(hm) = hmap {
                self.pos.y = hm.height_at_near(self.pos.x, self.pos.z, self.pos.y - self.foot) + self.foot;
            }
        }
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
            let want = if mv != Vec3::ZERO {
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
            p.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, &[], None, true, 1.0 / 60.0);
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
            p.update(&mut world, Vec3::ZERO, false, &[], None, true, 1.0 / 60.0);
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
            walk.update(&mut world, Vec3::new(0.0, 0.0, 1.0), false, &[], None, true, 1.0 / 60.0);
            run.update(&mut world, Vec3::new(0.0, 0.0, 1.0), true, &[], None, true, 1.0 / 60.0);
        }
        assert_eq!(world.get::<&AnimState>(er).unwrap().clip, CLIP_RUN);
        assert!(run.pos.z > walk.pos.z, "sprint should cover more ground: run {} vs walk {}", run.pos.z, walk.pos.z);
    }
}
