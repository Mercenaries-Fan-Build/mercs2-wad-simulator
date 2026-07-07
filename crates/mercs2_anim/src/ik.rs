//! Two-bone foot-placement IK — the `hkaFootPlacementIkSolver` analog.
//!
//! Corpus anchor: `hkaFootPlacementIkSolver` ctor `FUN_009ef650` (`animation_code_map.md`); the
//! per-frame VMX solve body is vtable-dispatched (confirm-live), so the numeric solve here is the
//! standard analytic two-bone (law-of-cosines) IK Havok's foot-placement solver reduces to — hip /
//! knee / ankle three-joint chain, planted onto a ground height the caller supplies. Kept behind a
//! clean API so the ground query comes from the physics silo via [`PhysicsQuery`] (no leaf→leaf
//! edge). This positions the ankle; the ankle→toe orientation pass (aligning the foot to the
//! surface normal) is a follow-up — see `DEFERRED.md`.

use mercs2_core::glam::{Quat, Vec3};
use mercs2_core::PhysicsQuery;

/// The result of a two-bone solve: the new mid/end world positions and the world-space rotation
/// **deltas** to apply on top of the sampled pose (root delta on the upper bone, mid delta on the
/// lower). The engine composes these onto the thigh/shin world rotations after `sampleAndCombine`.
#[derive(Clone, Copy, Debug)]
pub struct IkResult {
    /// New world position of the mid joint (knee).
    pub mid: Vec3,
    /// New world position of the end effector (ankle) — reaches `target` when in range.
    pub end: Vec3,
    /// World-space rotation to apply to the upper bone (hip→knee).
    pub root_delta: Quat,
    /// World-space rotation to apply to the lower bone (knee→ankle), after `root_delta`.
    pub mid_delta: Quat,
    /// True if the target was beyond reach and the leg was extended straight toward it (clamped).
    pub clamped: bool,
}

fn safe_normalize(v: Vec3) -> Vec3 {
    let l = v.length();
    if l > 1e-6 {
        v / l
    } else {
        Vec3::ZERO
    }
}

fn angle_between(a: Vec3, b: Vec3) -> f32 {
    let d = safe_normalize(a).dot(safe_normalize(b)).clamp(-1.0, 1.0);
    d.acos()
}

/// Analytic two-bone IK. `root` (hip), `mid` (knee), `end` (ankle) are the current world positions;
/// `target` is where the ankle should land; `pole` biases the bend plane (e.g. the knee's forward
/// direction) so the knee flexes the anatomically-correct way. Bone lengths are preserved.
///
/// Faithful to the law-of-cosines solve Havok's foot-placement IK uses: rotate the hip so the
/// hip→ankle span matches the target distance, bend the knee to close the triangle, then swing the
/// whole chain so the ankle points at the target.
pub fn solve_two_bone(root: Vec3, mid: Vec3, end: Vec3, target: Vec3, pole: Vec3) -> IkResult {
    let l_upper = (mid - root).length(); // hip→knee
    let l_lower = (end - mid).length(); // knee→ankle
    let reach = l_upper + l_lower;

    let to_target = target - root;
    let dist_raw = to_target.length();
    let clamped = dist_raw > reach || dist_raw < (l_upper - l_lower).abs();
    let dist = dist_raw.clamp((l_upper - l_lower).abs() + 1e-4, reach - 1e-4);

    // Current interior angle at the hip (between hip→knee and hip→ankle) and desired angle.
    let a_hip_0 = angle_between(mid - root, end - root);
    let a_hip_1 = ((l_upper * l_upper + dist * dist - l_lower * l_lower)
        / (2.0 * l_upper * dist).max(1e-6))
    .clamp(-1.0, 1.0)
    .acos();

    // Current interior angle at the knee and desired angle.
    let a_knee_0 = angle_between(root - mid, end - mid);
    let a_knee_1 = ((l_upper * l_upper + l_lower * l_lower - dist * dist)
        / (2.0 * l_upper * l_lower).max(1e-6))
    .clamp(-1.0, 1.0)
    .acos();

    // Bend-plane normal: prefer the current chain's plane; fall back to the pole when the leg is
    // straight (degenerate cross product). This keeps the knee flexing toward `pole`.
    let mut bend_axis = (end - root).cross(mid - root);
    if bend_axis.length() < 1e-5 {
        bend_axis = (target - root).cross(pole - root);
    }
    if bend_axis.length() < 1e-5 {
        bend_axis = Vec3::X; // last-resort deterministic axis
    }
    let bend_axis = safe_normalize(bend_axis);

    // Rotation that swings the whole chain so hip→ankle points at the target.
    let swing_axis = {
        let a = (end - root).cross(target - root);
        if a.length() < 1e-5 {
            bend_axis
        } else {
            safe_normalize(a)
        }
    };
    let swing_angle = angle_between(end - root, target - root);

    let r_hip = Quat::from_axis_angle(bend_axis, a_hip_1 - a_hip_0);
    let r_knee = Quat::from_axis_angle(bend_axis, a_knee_1 - a_knee_0);
    let r_swing = Quat::from_axis_angle(swing_axis, swing_angle);

    // New positions: apply the hip bend + swing to the upper bone, then the knee bend to the lower.
    let root_delta = r_swing * r_hip;
    let new_mid = root + root_delta * (mid - root);
    let mid_delta = r_swing * r_knee; // lower-bone delta, expressed in world (after root_delta swing)
    let new_end = new_mid + (root_delta * (r_knee * (end - mid)));

    IkResult { mid: new_mid, end: new_end, root_delta, mid_delta, clamped }
}

/// Configuration for planting a foot onto queried ground.
#[derive(Clone, Copy, Debug)]
pub struct FootPlacementIk {
    /// How far above the current ankle to start the downward ground probe.
    pub probe_up: f32,
    /// How far below the probe origin to search for ground.
    pub probe_down: f32,
    /// Desired ankle clearance above the ground contact (the ankle pivot height).
    pub ankle_height: f32,
    /// Only pull the foot DOWN to meet lower ground (true) or also push it UP onto higher ground
    /// (false). Locomotion usually plants down onto the mesh.
    pub down_only: bool,
}

impl Default for FootPlacementIk {
    fn default() -> Self {
        FootPlacementIk { probe_up: 0.5, probe_down: 1.5, ankle_height: 0.12, down_only: false }
    }
}

/// One leg's current world-space joint positions + the knee-forward pole hint.
#[derive(Clone, Copy, Debug)]
pub struct LegChain {
    pub hip: Vec3,
    pub knee: Vec3,
    pub ankle: Vec3,
    /// Direction the knee should flex toward (usually the character's forward).
    pub pole: Vec3,
}

impl FootPlacementIk {
    /// Query the ground under the ankle and, if found, solve the leg to plant the ankle at
    /// `ankle_height` above the contact. Returns `None` when no ground is within range (leg keeps
    /// its animated pose) or when the animated ankle is already at/below target under `down_only`.
    pub fn solve(&self, leg: LegChain, query: &dyn PhysicsQuery) -> Option<IkResult> {
        let origin = leg.ankle + Vec3::Y * self.probe_up;
        let hit = query.raycast(origin, -Vec3::Y, self.probe_up + self.probe_down)?;
        let ground_y = hit.point.y + self.ankle_height;
        if self.down_only && leg.ankle.y <= ground_y + 1e-4 {
            return None; // animated foot already at or below the ground; nothing to plant
        }
        let target = Vec3::new(leg.ankle.x, ground_y, leg.ankle.z);
        Some(solve_two_bone(leg.hip, leg.knee, leg.ankle, target, leg.pole))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::RayHit;

    /// A flat ground plane at a fixed Y that every downward ray hits.
    struct FlatGround {
        y: f32,
    }
    impl PhysicsQuery for FlatGround {
        fn raycast(&self, origin: Vec3, dir: Vec3, max: f32) -> Option<RayHit> {
            // Only handles downward rays for this test.
            if dir.y >= 0.0 {
                return None;
            }
            let dist = (origin.y - self.y) / (-dir.y);
            if dist < 0.0 || dist > max {
                return None;
            }
            Some(RayHit {
                point: Vec3::new(origin.x, self.y, origin.z),
                normal: Vec3::Y,
                distance: dist,
                entity: None,
            })
        }
        fn closest_point(&self, _p: Vec3, _m: f32) -> Option<mercs2_core::physics_query::ClosestPoint> {
            None
        }
        fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
            pos + delta
        }
    }

    fn len(a: Vec3, b: Vec3) -> f32 {
        (b - a).length()
    }

    #[test]
    fn two_bone_reaches_lowered_target_preserving_lengths() {
        // A bent leg (knee forward in +X) so the bend plane is well-defined and the chain has slack.
        let hip = Vec3::new(0.0, 1.0, 0.0);
        let knee = Vec3::new(0.3, 0.5, 0.0);
        let ankle = Vec3::new(0.0, 0.1, 0.0);
        let l_up = len(hip, knee);
        let l_lo = len(knee, ankle);

        // Lower the target below the current ankle — within reach (|hip→target| < l_up + l_lo).
        let target = Vec3::new(0.05, -0.05, 0.0);
        let r = solve_two_bone(hip, knee, ankle, target, Vec3::Z);

        assert!(!r.clamped, "target must be within reach");
        assert!((r.end - target).length() < 1e-3, "ankle reaches the target: {:?}", r.end);
        // Bone lengths preserved.
        assert!((len(hip, r.mid) - l_up).abs() < 1e-3, "upper length preserved");
        assert!((len(r.mid, r.end) - l_lo).abs() < 1e-3, "lower length preserved");
        // The knee stays bent (not collinear) — a real pose, not a locked-straight leg.
        let knee_angle = angle_between(hip - r.mid, r.end - r.mid);
        assert!(knee_angle < std::f32::consts::PI - 0.05, "knee remains flexed");
    }

    #[test]
    fn overreach_clamps_and_straightens() {
        let hip = Vec3::new(0.0, 2.0, 0.0);
        let knee = Vec3::new(0.05, 1.0, 0.0);
        let ankle = Vec3::new(0.0, 0.0, 0.0);
        let reach = len(hip, knee) + len(knee, ankle);
        // Target far beyond reach.
        let target = hip + Vec3::new(0.0, -reach * 2.0, 0.0);
        let r = solve_two_bone(hip, knee, ankle, target, Vec3::Z);
        assert!(r.clamped, "beyond-reach target is clamped");
        // End is on the hip→target line at ~reach distance.
        assert!((len(hip, r.end) - reach).abs() < 1e-2, "leg extended to near-full reach");
    }

    #[test]
    fn foot_placement_plants_onto_ground() {
        let ik = FootPlacementIk { down_only: true, ..Default::default() };
        // Bent leg with the ankle floating above ground; plant it down onto the queried ground.
        let leg = LegChain {
            hip: Vec3::new(0.0, 1.0, 0.0),
            knee: Vec3::new(0.3, 0.5, 0.0),
            ankle: Vec3::new(0.0, 0.1, 0.0),
            pole: Vec3::Z,
        };
        let ground = FlatGround { y: -0.1 };
        let r = ik.solve(leg, &ground).expect("ground found, foot plants");
        let want_y = -0.1 + ik.ankle_height;
        assert!((r.end.y - want_y).abs() < 1e-3, "ankle planted at ground+height: {}", r.end.y);
        // No lateral drift (target kept ankle x/z).
        assert!(r.end.x.abs() < 1e-3 && r.end.z.abs() < 1e-3);
    }

    #[test]
    fn foot_placement_skips_when_already_below_ground_downonly() {
        let ik = FootPlacementIk { down_only: true, ..Default::default() };
        let leg = LegChain {
            hip: Vec3::new(0.0, 2.0, 0.0),
            knee: Vec3::new(0.05, 1.0, 0.0),
            ankle: Vec3::new(0.0, 0.0, 0.0),
            pole: Vec3::Z,
        };
        // Ground well above the ankle: down_only means don't push the foot up.
        let ground = FlatGround { y: 0.5 };
        assert!(ik.solve(leg, &ground).is_none());
    }
}
