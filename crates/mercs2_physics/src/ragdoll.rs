//! Constrained multi-body ragdoll — the faithful replacement for the single-rigid-body death
//! stand-in (`mercs2_combat::ragdoll`).
//!
//! # What is recovered vs. what is a faithful default (honesty boundary)
//!
//! A full census of retail PC `vz.wad` (11 370 blocks, via `mercs2_formats` bin `ragdoll_probe`)
//! found the ragdoll physics is **not serialized**: **zero** `hkpRigidBody`, `hkpRagdollConstraintData`,
//! `hkpRagdollLimitsData`, `hkpRagdollMotorConstraintAtom`, `hkaRagdollInstance` or `hkaSkeletonMapper`
//! instances anywhere in the archive (the class *names* sit in the packfile classname tables but no
//! virtual fixup ever instantiates them). The engine builds the rigid-body chain + constraints
//! **procedurally at load**, Havok-side — matching the docs (`physics_havok_spec.md` §5 OPEN#1:
//! "the ECS `PhysicsActor` component carries only a 4-byte handle; the real body is Havok-side").
//!
//! What the WAD **does** ship — and what this module is built on — is the set of collider shapes the
//! ragdoll bodies attach to: the resident human/animation block (3185) carries exactly **11
//! `hkpCapsuleShape`** bodies, the classic humanoid ragdoll count. Their per-body radius + half-length
//! are decoded byte-exact by `mercs2_formats::havok` (layout `+16` radius / `+32,+48` endpoints,
//! verified against those 11 real instances) and baked into [`RagdollDef::human`] below. The bone
//! roster (`Bone_Hips`, `Bone_Chest`, `Bone_Head`, `Bone_L/RThigh`, `Bone_L/RShin`, `Bone_L/RBicep`,
//! `Bone_L/RForearm`) comes from the devkit bone-name constants (`animation-skeleton.md`), and each
//! bone's `pandemic_hash_m2` was confirmed present in every human character HIER in the WAD (12/12).
//!
//! The pieces that genuinely are **not** in the shipped data — per-body **mass** and the per-joint
//! **cone/twist limits** — are marked `// CONFIRM-LIVE:` and filled with standard anthropometric /
//! anatomical defaults (segment-mass fractions; joint ranges of motion). They are read live off a
//! loaded ragdoll (`RagdollController`) with x32dbg to pin exactly; the *shape* data driving them is
//! recovered.
//!
//! # The sim
//!
//! A small **XPBD** (extended position-based dynamics) articulated-body solver: one rigid body per
//! ragdoll bone (capsule collider, full orientation + inertia), ball-and-socket joints at the bone
//! origins, and per-joint swing-cone + twist angular limits — the parameterization of Havok's
//! `hkpRagdollConstraintData` (a twist-cone-plane constraint). Substepped for stability; collides
//! against the world through the shared [`PhysicsQuery`] seam (the same one the character controller
//! uses), so a killed NPC's limbs articulate and settle on the real terrain/geometry.
//!
//! `// CONFIRM-LIVE:` the exact integrator is Havok's VMX/SSE `hkpWorld::step`, which does not decode
//! (`physics_havok_spec.md` §5 OPEN#2 — "no numeric oracle for the solver internals"); this XPBD step
//! is a faithful modern equivalent gated on observable settling behaviour, not step-for-step math.

use mercs2_core::glam::{Quat, Vec3};
use mercs2_core::physics_query::PhysicsQuery;

use std::f32::consts::PI;

/// One ragdoll body's static definition. Shape (`radius`/`half_len`) is **recovered** from the WAD
/// capsule; `mass` and the `cone`/`twist` limits are `// CONFIRM-LIVE:` anthropometric/anatomical
/// defaults (see module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagdollBodyDef {
    /// `pandemic_hash_m2` of the bone this body represents (identity into the animation skeleton —
    /// the retail "skeleton mapper" is trivial because each ragdoll body *is* a named skeleton bone).
    pub name_hash: u32,
    /// Parent body index in [`RagdollDef::bodies`], or `-1` for the root (pelvis / `Bone_Hips`).
    pub parent: i32,
    /// Body mass, kg. `// CONFIRM-LIVE:` Dempster body-segment fraction of an 80 kg human.
    pub mass: f32,
    /// Capsule radius (world units). **Recovered** from the WAD `hkpCapsuleShape`.
    pub radius: f32,
    /// Capsule half-length along the bone axis. **Recovered** from the WAD `hkpCapsuleShape`.
    pub half_len: f32,
    /// Swing-cone half-angle limit, degrees. `// CONFIRM-LIVE:` anatomical joint range of motion.
    pub cone_deg: f32,
    /// Twist limit about the bone axis, degrees. `// CONFIRM-LIVE:` anatomical.
    pub twist_deg: f32,
}

/// The topology + per-body shape/mass/limits of a ragdoll. Static; instantiated into a live
/// [`Ragdoll`] with [`Ragdoll::spawn`].
#[derive(Debug, Clone, PartialEq)]
pub struct RagdollDef {
    pub bodies: Vec<RagdollBodyDef>,
}

impl RagdollDef {
    /// The **recovered human ragdoll**: the 11 bodies retail attaches to the 11 `hkpCapsuleShape`
    /// colliders in `vz.wad` block 3185. Radii/half-lengths are byte-exact from the WAD (see
    /// `mercs2_formats::havok::human_ragdoll_capsules` and its `ragdoll_capsule_layout_*` tests);
    /// masses + joint limits are `// CONFIRM-LIVE:` anthropometric/anatomical defaults.
    ///
    /// Body order / parenting:
    /// `Hips(root) → Chest → {Head, LBicep→LForearm, RBicep→RForearm}`, `Hips → {LThigh→LShin,
    /// RThigh→RShin}`.
    pub fn human() -> Self {
        // name_hash = pandemic_hash_m2(bone) — confirmed present in every human HIER (ragdoll_probe).
        // (parent, mass_kg, radius, half_len, cone_deg, twist_deg)
        let b = |name_hash: u32, parent: i32, mass: f32, radius: f32, half_len: f32, cone_deg: f32, twist_deg: f32| {
            RagdollBodyDef { name_hash, parent, mass, radius, half_len, cone_deg, twist_deg }
        };
        RagdollDef {
            bodies: vec![
                // idx 0: Hips (root)
                b(0x24C5_009C, -1, 14.0, 0.0999, 0.0297, 0.0, 0.0),
                // idx 1: Chest (spine)          cone 25 / twist 30
                b(0x4C77_33ED, 0, 22.0, 0.1449, 0.0490, 25.0, 30.0),
                // idx 2: Head (neck)            cone 30 / twist 45
                b(0x705C_4508, 1, 6.5, 0.1700, 0.0188, 30.0, 45.0),
                // idx 3: L Bicep (shoulder)     cone 75 / twist 45
                b(0xB2C9_CE63, 1, 2.2, 0.0750, 0.0815, 75.0, 45.0),
                // idx 4: L Forearm (elbow)      cone 70 / twist 10
                b(0xBEFC_09A2, 3, 1.8, 0.0750, 0.0867, 70.0, 10.0),
                // idx 5: R Bicep (shoulder)
                b(0x20F6_35D9, 1, 2.2, 0.0750, 0.0929, 75.0, 45.0),
                // idx 6: R Forearm (elbow)
                b(0x23F6_F598, 5, 1.8, 0.0750, 0.0867, 70.0, 10.0),
                // idx 7: L Thigh (hip)          cone 60 / twist 40
                b(0x7685_3D12, 0, 8.0, 0.1194, 0.1148, 60.0, 40.0),
                // idx 8: L Shin (knee)          cone 70 / twist 10
                b(0xA76C_9842, 7, 3.7, 0.0914, 0.1088, 70.0, 10.0),
                // idx 9: R Thigh (hip)
                b(0xC229_9AC4, 0, 8.0, 0.1194, 0.1148, 60.0, 40.0),
                // idx 10: R Shin (knee)
                b(0x0163_705C, 9, 3.7, 0.0914, 0.1088, 70.0, 10.0),
            ],
        }
    }

    /// Number of bodies.
    pub fn len(&self) -> usize {
        self.bodies.len()
    }
    /// Whether the def has no bodies.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
    /// Body index whose `name_hash` matches, if any.
    pub fn index_of(&self, name_hash: u32) -> Option<usize> {
        self.bodies.iter().position(|b| b.name_hash == name_hash)
    }
    /// The `name_hash` of each body, in body order (the read-back key set).
    pub fn bone_hashes(&self) -> Vec<u32> {
        self.bodies.iter().map(|b| b.name_hash).collect()
    }
}

/// The initial world pose of one ragdoll body — the current animated bone transform at the instant of
/// the `SetBodyToRagdoll` handoff (bodies are snapped onto the posed skeleton, *then* released).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySeed {
    pub position: Vec3,
    pub orientation: Quat,
    /// Optional initial linear velocity (e.g. the killing blast impulse / m, or the character's
    /// motion). Defaults to zero.
    pub velocity: Vec3,
}

impl BodySeed {
    pub fn at(position: Vec3, orientation: Quat) -> Self {
        Self { position, orientation, velocity: Vec3::ZERO }
    }
}

/// One live ragdoll rigid body (capsule): world center-of-mass + orientation + velocities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RagdollBody {
    /// World center of mass (the capsule is centred on the bone origin, so this tracks the bone).
    pub position: Vec3,
    /// World orientation.
    pub orientation: Quat,
    pub lin_vel: Vec3,
    pub ang_vel: Vec3,
    pub radius: f32,
    pub half_len: f32,
    inv_mass: f32,
    inv_inertia_local: Vec3,
    // solver scratch
    prev_pos: Vec3,
    prev_orient: Quat,
}

impl RagdollBody {
    /// The two capsule segment endpoints in world space (the collision/support points).
    pub fn endpoints(&self) -> [Vec3; 2] {
        let axis = self.orientation * Vec3::Y;
        [self.position + axis * self.half_len, self.position - axis * self.half_len]
    }
    /// World inverse-inertia applied to a world vector `w` (`R · Iinv_local · Rᵀ · w`).
    fn iinv_mul(&self, w: Vec3) -> Vec3 {
        let local = self.orientation.conjugate() * w;
        self.orientation * (local * self.inv_inertia_local)
    }
}

/// One ball-and-socket joint with swing-cone + twist limits (a `hkpRagdollConstraintData` analog).
#[derive(Debug, Clone, Copy)]
struct Joint {
    child: usize,
    parent: usize,
    /// Joint point in each body's local frame (both map to the same world point at rest).
    anchor_child: Vec3,
    anchor_parent: Vec3,
    /// Rest relative orientation `q_parentᵀ · q_child`, the reference the angular limits measure from.
    rest_rel: Quat,
    cone_cos: f32,
    twist_rad: f32,
}

/// A live constrained multi-body ragdoll. Spawn from a posed skeleton, [`step`](Ragdoll::step) each
/// frame against a [`PhysicsQuery`], and read the bone transforms back into the skin.
#[derive(Debug, Clone)]
pub struct Ragdoll {
    pub bodies: Vec<RagdollBody>,
    joints: Vec<Joint>,
    name_hashes: Vec<u32>,
    /// Gravity (world units/s²). `// CONFIRM-LIVE:` the exe uses `hkpWorldCinfo`'s gravity vector.
    pub gravity: Vec3,
    /// Velocity damping per second (air drag; keeps the sim from ringing).
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// Restitution/friction on ground contact.
    pub ground_friction: f32,
    /// XPBD substeps per [`step`](Ragdoll::step) call.
    pub substeps: u32,
    settled: bool,
}

/// Capsule principal inertia (diagonal, bone axis = local Y), split cylinder + spherical caps by
/// volume. Approximate but physically-scaled — enough for faithful tumbling.
fn capsule_inertia(mass: f32, r: f32, half_len: f32) -> Vec3 {
    let l = 2.0 * half_len; // cylinder length
    let vc = PI * r * r * l;
    let vs = (4.0 / 3.0) * PI * r * r * r;
    let v = (vc + vs).max(1e-9);
    let mc = mass * vc / v;
    let ms = mass * vs / v;
    // About the axis (Y):
    let iy = 0.5 * mc * r * r + (2.0 / 5.0) * ms * r * r;
    // Perpendicular (X, Z): cylinder + caps offset to the ends.
    let ix = mc * (l * l / 12.0 + r * r / 4.0)
        + ms * ((2.0 / 5.0) * r * r + (l * 0.5) * (l * 0.5) + 0.375 * r * l);
    Vec3::new(ix.max(1e-6), iy.max(1e-6), ix.max(1e-6))
}

impl Ragdoll {
    /// Spawn a ragdoll from a def and one [`BodySeed`] per body (in body order), snapping each body
    /// onto its current animated pose — the `WSHumanRagdoll::SetBodyToRagdoll` handoff. `seeds.len()`
    /// must equal `def.len()`.
    pub fn spawn(def: &RagdollDef, seeds: &[BodySeed]) -> Ragdoll {
        assert_eq!(seeds.len(), def.len(), "one seed per ragdoll body");
        let bodies: Vec<RagdollBody> = def
            .bodies
            .iter()
            .zip(seeds)
            .map(|(d, s)| RagdollBody {
                position: s.position,
                orientation: s.orientation.normalize(),
                lin_vel: s.velocity,
                ang_vel: Vec3::ZERO,
                radius: d.radius,
                half_len: d.half_len,
                inv_mass: if d.mass > 0.0 { 1.0 / d.mass } else { 0.0 },
                inv_inertia_local: {
                    let i = capsule_inertia(d.mass, d.radius, d.half_len);
                    Vec3::new(1.0 / i.x, 1.0 / i.y, 1.0 / i.z)
                },
                prev_pos: s.position,
                prev_orient: s.orientation.normalize(),
            })
            .collect();

        // Build joints at the child bone origins (the shared world point of body & parent at rest).
        let mut joints = Vec::new();
        for (i, d) in def.bodies.iter().enumerate() {
            if d.parent < 0 {
                continue;
            }
            let p = d.parent as usize;
            let (cb, pb) = (&bodies[i], &bodies[p]);
            let world_joint = cb.position; // the bone origin = the joint pivot
            let anchor_child = cb.orientation.conjugate() * (world_joint - cb.position);
            let anchor_parent = pb.orientation.conjugate() * (world_joint - pb.position);
            let rest_rel = (pb.orientation.conjugate() * cb.orientation).normalize();
            joints.push(Joint {
                child: i,
                parent: p,
                anchor_child,
                anchor_parent,
                rest_rel,
                cone_cos: (d.cone_deg.to_radians()).cos(),
                twist_rad: d.twist_deg.to_radians(),
            });
        }

        Ragdoll {
            bodies,
            joints,
            name_hashes: def.bone_hashes(),
            gravity: Vec3::new(0.0, -9.81, 0.0),
            linear_damping: 0.4,
            angular_damping: 0.6,
            ground_friction: 0.6,
            substeps: 10,
            settled: false,
        }
    }

    /// Spawn from a closure resolving each body's bone-hash to its current pose (returns `None` if any
    /// body's bone can't be posed). This is the seam a combat/anim death path drives: it reads the
    /// posed skeleton for the 11 ragdoll bones and hands them here.
    pub fn spawn_with(def: &RagdollDef, mut pose: impl FnMut(u32) -> Option<BodySeed>) -> Option<Ragdoll> {
        let mut seeds = Vec::with_capacity(def.len());
        for d in &def.bodies {
            seeds.push(pose(d.name_hash)?);
        }
        Some(Ragdoll::spawn(def, &seeds))
    }

    /// Number of bodies.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Whether every body has come to rest (all speeds below the settle threshold).
    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Read back each body's world transform keyed by its bone `name_hash` — what a combat/anim
    /// death system writes into the `SkinPalette` (the 11 driven bones; non-ragdoll bones follow
    /// their nearest ragdoll ancestor, resolved caller-side).
    pub fn bone_transforms(&self) -> Vec<(u32, Vec3, Quat)> {
        self.name_hashes
            .iter()
            .zip(&self.bodies)
            .map(|(&h, b)| (h, b.position, b.orientation))
            .collect()
    }

    /// Advance the ragdoll by `dt` seconds against `phys`, running the XPBD substep loop (integrate →
    /// solve joints → solve angular limits → ground-collide → update velocities). Bodies articulate
    /// under their constraints and settle on the queried world geometry.
    pub fn step(&mut self, phys: &dyn PhysicsQuery, dt: f32) {
        if dt <= 0.0 || self.settled {
            return;
        }
        let n = self.substeps.max(1);
        let h = dt / n as f32;
        for _ in 0..n {
            self.integrate(h);
            // A few relaxation iterations resolve the coupled joint chain each substep.
            for _ in 0..2 {
                self.solve_joints();
                self.solve_angular_limits();
            }
            self.collide_ground(phys);
            self.update_velocities(h);
        }
        self.apply_damping(dt);
        self.update_settled();
    }

    // --- XPBD phases ---

    fn integrate(&mut self, h: f32) {
        let g = self.gravity;
        for b in &mut self.bodies {
            b.prev_pos = b.position;
            b.prev_orient = b.orientation;
            if b.inv_mass == 0.0 {
                continue;
            }
            b.lin_vel += g * h;
            b.position += b.lin_vel * h;
            // Orientation: q += 0.5 h (ω⊗q), renormalised.
            let wq = Quat::from_xyzw(b.ang_vel.x, b.ang_vel.y, b.ang_vel.z, 0.0) * b.orientation;
            b.orientation = Quat::from_xyzw(
                b.orientation.x + 0.5 * h * wq.x,
                b.orientation.y + 0.5 * h * wq.y,
                b.orientation.z + 0.5 * h * wq.z,
                b.orientation.w + 0.5 * h * wq.w,
            )
            .normalize();
        }
    }

    /// Ball-and-socket point constraints: bring each joint's two anchor points back together.
    fn solve_joints(&mut self) {
        for k in 0..self.joints.len() {
            let j = self.joints[k];
            let (i, p) = (j.child, j.parent);
            let (ra, rb, pa, pb);
            {
                let bi = &self.bodies[i];
                let bp = &self.bodies[p];
                ra = bi.orientation * j.anchor_child;
                rb = bp.orientation * j.anchor_parent;
                pa = bi.position + ra;
                pb = bp.position + rb;
            }
            let c = pa - pb;
            let clen = c.length();
            if clen < 1e-7 {
                continue;
            }
            let n = c / clen;
            let (wi, wj);
            {
                let bi = &self.bodies[i];
                let bp = &self.bodies[p];
                let ri = ra.cross(n);
                let rj = rb.cross(n);
                wi = bi.inv_mass + ri.dot(bi.iinv_mul(ri));
                wj = bp.inv_mass + rj.dot(bp.iinv_mul(rj));
            }
            let w = wi + wj;
            if w < 1e-12 {
                continue;
            }
            let corr = n * (clen / w); // full XPBD correction (compliance 0)
            // child i moves toward parent; parent p moves toward child.
            let (dphi_i, dphi_j);
            {
                let bi = &self.bodies[i];
                let bp = &self.bodies[p];
                dphi_i = -bi.iinv_mul(ra.cross(corr));
                dphi_j = bp.iinv_mul(rb.cross(corr));
            }
            let imi = self.bodies[i].inv_mass;
            let imj = self.bodies[p].inv_mass;
            self.bodies[i].position -= corr * imi;
            self.bodies[p].position += corr * imj;
            apply_rot(&mut self.bodies[i], dphi_i);
            apply_rot(&mut self.bodies[p], dphi_j);
        }
    }

    /// Swing-cone + twist limits on each joint's relative orientation.
    fn solve_angular_limits(&mut self) {
        for k in 0..self.joints.len() {
            let j = self.joints[k];
            let (i, p) = (j.child, j.parent);
            let q_child = self.bodies[i].orientation;
            let q_parent = self.bodies[p].orientation;
            // Relative orientation measured from the rest pose.
            let rel = (q_parent.conjugate() * q_child * j.rest_rel.conjugate()).normalize();
            // Swing-twist decomposition about the bone axis (local Y).
            let axis = Vec3::Y;
            let (swing, twist) = swing_twist(rel, axis);

            // Clamp swing to the cone.
            let mut target = rel;
            let swing_angle = 2.0 * swing.w.clamp(-1.0, 1.0).acos();
            let cone = j.cone_cos.clamp(-1.0, 1.0).acos();
            if swing_angle > cone {
                let sw_axis = Vec3::new(swing.x, swing.y, swing.z);
                let l = sw_axis.length();
                if l > 1e-6 {
                    let clamped = Quat::from_axis_angle(sw_axis / l, cone);
                    target = (clamped * twist).normalize();
                }
            }
            // Clamp twist.
            let twist_angle = 2.0 * twist.w.clamp(-1.0, 1.0).acos();
            let twist_angle = if twist_angle > PI { twist_angle - 2.0 * PI } else { twist_angle };
            if twist_angle.abs() > j.twist_rad {
                let sign = twist_angle.signum();
                let sw = extract_swing(&target, axis);
                let clamped_twist = Quat::from_axis_angle(axis, sign * j.twist_rad);
                target = (sw * clamped_twist).normalize();
            }
            if target == rel {
                continue;
            }
            // Correction rotation to apply to the pair: dq = target · relᵀ (small-angle → axis*angle).
            let dq = (target * rel.conjugate()).normalize();
            let mut v = Vec3::new(dq.x, dq.y, dq.z) * 2.0;
            if dq.w < 0.0 {
                v = -v;
            }
            // Rotate the correction into world (it was measured in the parent frame).
            let vw = q_parent * v;
            // Distribute by angular inverse mass; split half/half weighted.
            let wi = self.bodies[i].iinv_mul(vw).length();
            let wj = self.bodies[p].iinv_mul(vw).length();
            let sum = wi + wj;
            if sum < 1e-9 {
                continue;
            }
            let fi = wi / sum;
            let fj = wj / sum;
            apply_rot(&mut self.bodies[i], vw * fi);
            apply_rot(&mut self.bodies[p], -vw * fj);
        }
    }

    /// Push every capsule endpoint out of the world geometry (a positional point constraint against
    /// the ground under each end). Uses a short downward raycast — the shared `hkpWorldRayCaster` seam.
    fn collide_ground(&mut self, phys: &dyn PhysicsQuery) {
        for bi in 0..self.bodies.len() {
            let (r, com) = {
                let b = &self.bodies[bi];
                (b.radius, b.position)
            };
            let ends = self.bodies[bi].endpoints();
            for end in ends {
                // Probe from a little above the end straight down.
                let up = 0.5 + r;
                if let Some(hit) = phys.raycast(end + Vec3::Y * up, -Vec3::Y, up + 1.0) {
                    let ground_y = hit.point.y;
                    let n = if hit.normal.y.abs() > 1e-4 { hit.normal.normalize() } else { Vec3::Y };
                    let pen = (ground_y + r) - end.y; // >0 when the endpoint sphere pokes through
                    if pen > 0.0 {
                        let arm = end - com; // anchor offset from COM
                        let b = &self.bodies[bi];
                        let rn = arm.cross(n);
                        let w = b.inv_mass + rn.dot(b.iinv_mul(rn));
                        if w > 1e-9 {
                            let corr = n * (pen / w);
                            let dphi = b.iinv_mul(arm.cross(corr));
                            let imi = b.inv_mass;
                            self.bodies[bi].position += corr * imi;
                            apply_rot(&mut self.bodies[bi], dphi);
                        }
                    }
                }
            }
        }
    }

    fn update_velocities(&mut self, h: f32) {
        let inv_h = 1.0 / h;
        for b in &mut self.bodies {
            if b.inv_mass == 0.0 {
                continue;
            }
            b.lin_vel = (b.position - b.prev_pos) * inv_h;
            let dq = (b.orientation * b.prev_orient.conjugate()).normalize();
            let mut v = Vec3::new(dq.x, dq.y, dq.z) * 2.0;
            if dq.w < 0.0 {
                v = -v;
            }
            b.ang_vel = v * inv_h;
        }
    }

    fn apply_damping(&mut self, dt: f32) {
        let lin = (1.0 - self.linear_damping * dt).clamp(0.0, 1.0);
        let ang = (1.0 - self.angular_damping * dt).clamp(0.0, 1.0);
        for b in &mut self.bodies {
            b.lin_vel *= lin;
            b.ang_vel *= ang;
        }
    }

    fn update_settled(&mut self) {
        const LIN: f32 = 0.08;
        const ANG: f32 = 0.15;
        self.settled = self
            .bodies
            .iter()
            .all(|b| b.lin_vel.length_squared() < LIN * LIN && b.ang_vel.length_squared() < ANG * ANG);
    }
}

/// Apply a small rotation-vector correction `dphi` (axis·angle, world) to a body's orientation.
fn apply_rot(b: &mut RagdollBody, dphi: Vec3) {
    if b.inv_mass == 0.0 || dphi.length_squared() < 1e-18 {
        return;
    }
    let dq = Quat::from_xyzw(dphi.x, dphi.y, dphi.z, 0.0) * b.orientation;
    b.orientation = Quat::from_xyzw(
        b.orientation.x + 0.5 * dq.x,
        b.orientation.y + 0.5 * dq.y,
        b.orientation.z + 0.5 * dq.z,
        b.orientation.w + 0.5 * dq.w,
    )
    .normalize();
}

/// Swing-twist decomposition of `q` about unit `axis`: returns `(swing, twist)` with `q = swing·twist`
/// and `twist` a rotation purely about `axis`.
fn swing_twist(q: Quat, axis: Vec3) -> (Quat, Quat) {
    let twist = extract_twist(&q, axis);
    let swing = (q * twist.conjugate()).normalize();
    (swing, twist)
}

fn extract_twist(q: &Quat, axis: Vec3) -> Quat {
    let p = Vec3::new(q.x, q.y, q.z).dot(axis) * axis; // project rotation axis onto `axis`
    let t = Quat::from_xyzw(p.x, p.y, p.z, q.w);
    let n = (t.x * t.x + t.y * t.y + t.z * t.z + t.w * t.w).sqrt();
    if n < 1e-9 {
        Quat::IDENTITY
    } else {
        Quat::from_xyzw(t.x / n, t.y / n, t.z / n, t.w / n)
    }
}

fn extract_swing(q: &Quat, axis: Vec3) -> Quat {
    (*q * extract_twist(q, axis).conjugate()).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Heightmap, StaticCollision};

    /// A flat floor made of small triangles centred on the origin (matches the small-triangle set
    /// the query culls are tuned for), for the ragdoll to settle on.
    fn floor(y: f32) -> Vec<[Vec3; 3]> {
        let mut out = Vec::new();
        let mut x = -3.0f32;
        while x < 3.0 {
            let mut z = -3.0f32;
            while z < 3.0 {
                let a = Vec3::new(x, y, z);
                let b = Vec3::new(x + 0.5, y, z);
                let c = Vec3::new(x + 0.5, y, z + 0.5);
                let d = Vec3::new(x, y, z + 0.5);
                out.push([a, c, b]);
                out.push([a, d, c]);
                z += 0.5;
            }
            x += 0.5;
        }
        out
    }

    /// Seed the human ragdoll standing upright: each bone at an anatomical height, identity rotation.
    fn upright_seeds(def: &RagdollDef, base_y: f32) -> Vec<BodySeed> {
        // Approximate standing bone heights (m) by body index (Hips..RShin).
        let ys = [
            base_y + 1.00, // hips
            base_y + 1.35, // chest
            base_y + 1.65, // head
            base_y + 1.45, // L bicep
            base_y + 1.20, // L forearm
            base_y + 1.45, // R bicep
            base_y + 1.20, // R forearm
            base_y + 0.70, // L thigh
            base_y + 0.35, // L shin
            base_y + 0.70, // R thigh
            base_y + 0.35, // R shin
        ];
        let xs = [0.0, 0.0, 0.0, 0.2, 0.25, -0.2, -0.25, 0.1, 0.1, -0.1, -0.1];
        def.bodies
            .iter()
            .enumerate()
            .map(|(k, _)| BodySeed::at(Vec3::new(xs[k], ys[k], 0.0), Quat::IDENTITY))
            .collect()
    }

    #[test]
    fn human_def_is_the_recovered_11_body_ragdoll() {
        let def = RagdollDef::human();
        assert_eq!(def.len(), 11);
        // Root is the pelvis; everything else chains to a valid parent with a lower index.
        assert_eq!(def.bodies[0].parent, -1, "hips is the root");
        for (i, b) in def.bodies.iter().enumerate().skip(1) {
            assert!(b.parent >= 0 && (b.parent as usize) < i, "body {i} parent {}", b.parent);
        }
        // The recovered head capsule is the fattest body.
        let head = def.bodies.iter().max_by(|a, b| a.radius.partial_cmp(&b.radius).unwrap()).unwrap();
        assert!((head.radius - 0.1700).abs() < 1e-4);
        // Total mass is a plausible human.
        let total: f32 = def.bodies.iter().map(|b| b.mass).sum();
        assert!((60.0..95.0).contains(&total), "total mass {total}");
    }

    #[test]
    fn spawn_builds_a_joint_per_non_root_body() {
        let def = RagdollDef::human();
        let rd = Ragdoll::spawn(&def, &upright_seeds(&def, 0.0));
        assert_eq!(rd.body_count(), 11);
        assert_eq!(rd.joints.len(), 10, "10 joints for 11 bodies (root has none)");
    }

    #[test]
    fn ragdoll_settles_on_the_ground_without_exploding() {
        let def = RagdollDef::human();
        let phys = StaticCollision::new(floor(0.0));
        // Drop the whole ragdoll from ~1.5 m so it falls, articulates, and settles.
        let mut rd = Ragdoll::spawn(&def, &upright_seeds(&def, 1.5));

        let mut steps = 0;
        while !rd.settled() && steps < 1200 {
            rd.step(&phys, 1.0 / 60.0);
            steps += 1;
            // Never explode: no body flies off to infinity or sinks far through the floor.
            for b in &rd.bodies {
                assert!(b.position.is_finite(), "NaN body position at step {steps}");
                assert!(b.position.length() < 50.0, "body flew away: {:?}", b.position);
                assert!(b.position.y > -1.0, "body sank through floor: y={}", b.position.y);
            }
        }
        assert!(rd.settled(), "ragdoll never settled ({steps} steps)");
        // Settled above the floor: every capsule's lowest point rests at/above ~ground.
        for b in &rd.bodies {
            let low = b.endpoints().iter().map(|e| e.y).fold(f32::INFINITY, f32::min) - b.radius;
            assert!(low > -0.15, "body rests below floor: low={low}");
            assert!(b.position.y < 2.0, "body did not fall: y={}", b.position.y);
        }
    }

    #[test]
    fn joints_stay_coherent_after_settling() {
        // After settling, connected bodies' joint anchors should still roughly coincide (the chain
        // didn't tear apart under the constraint solver).
        let def = RagdollDef::human();
        let phys = StaticCollision::with_heightmap(floor(0.0), Heightmap::new(-5.0, -5.0, 1.0, 11, 11, vec![0.0; 121]));
        let mut rd = Ragdoll::spawn(&def, &upright_seeds(&def, 1.2));
        for _ in 0..900 {
            rd.step(&phys, 1.0 / 60.0);
        }
        for j in &rd.joints {
            let bi = &rd.bodies[j.child];
            let bp = &rd.bodies[j.parent];
            let pa = bi.position + bi.orientation * j.anchor_child;
            let pb = bp.position + bp.orientation * j.anchor_parent;
            assert!((pa - pb).length() < 0.06, "joint separated by {}", (pa - pb).length());
        }
    }
}
