//! FaceFX evaluation: an actor's face graph driven by an anim's curves, solved into local
//! transforms for its face bones — the PC solve `FUN_00686ce0` and the SDK code it calls.
//!
//! | Step | PC v1.0 | PS3 v1.00 |
//! |---|---|---|
//! | curve value at `t` (Hermite / linear) | `0x7341a0` | `0x892e68` |
//! | curve → node by exact name (`FxAnim::Link`, `FindNode`) | `0x72afb0`, `0x72d2c0` | — |
//! | node value (`GetValue`, vtable `+0x18`) | `0x729b80` | `0x89d3b8` |
//! | `linear` link function | `0x731950` | `0x8a15d0` |
//! | bone-pose contributions (master bone list link) | `0x72ea80` | — |
//! | per-bone blend and the pose write | `0x686ce0` | — |
//!
//! A curve whose name is no node of the actor is evaluated and its value goes nowhere, as in the
//! game (`FindNode` returns null and the write-back is skipped). The game also feeds nodes "user
//! values" from its registers (`0x7276f0`); nothing here sets a register, so no node has one.
//!
//! The output is the face bone's LOCAL transform in the clip's (Havok) space, replacing whatever
//! the body clip gives that bone: the game writes it over the bone's slot of the local pose buffer
//! with X mirrored (`0x6877de`–`0x68784d`).

use mercs2_formats::anim::QsTransform;
use mercs2_formats::facefx::{Actor, Anim, Curve, NodeKind};

/// `FLT_EPSILON` (`[0xbeaac0]`): a pose node lighter than this adds nothing.
const EPSILON: f32 = 1.192_092_9e-7;
/// Above this quaternion dot product the blend is a plain lerp (`[0xbead70]`).
const SLERP_BELOW: f32 = 0.55;

/// A quaternion in FaceFX's `w, x, y, z` order.
type Quat = [f32; 4];

fn qmul(a: Quat, b: Quat) -> Quat {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

fn dot(a: Quat, b: Quat) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// A curve's value at `t` (`0x7341a0`): held at the first and last key, Hermite (interpolation 0)
/// or linear (1) between keys, with the outgoing and incoming slopes scaled by the segment length.
pub fn curve_value(c: &Curve, t: f32) -> Result<f32, String> {
    let k = &c.keys;
    let Some(first) = k.first() else { return Ok(0.0) };
    let last = k[k.len() - 1];
    if t <= first.time() || k.len() == 1 {
        return Ok(first.value());
    }
    if t >= last.time() {
        return Ok(last.value());
    }
    let i = k.windows(2).position(|w| w[0].time() <= t && t < w[1].time()).expect("t lies between the first and last key");
    let ([t0, v0, _, out0], [t1, v1, in1, _]) = (k[i].0, k[i + 1].0);
    let dt = t1 - t0;
    match c.interpolation {
        0 => {
            let s = (t - t0) / dt;
            let (m0, m1) = (out0 * dt, in1 * dt);
            let a = 2.0 * v0 - 2.0 * v1 + m0 + m1;
            let b = 3.0 * v1 - 3.0 * v0 - 2.0 * m0 - m1;
            Ok(((a * s + b) * s + m0) * s + v0)
        }
        1 => Ok(v0 + (t - t0) / dt * (v1 - v0)),
        // The game returns 0 for any other interpolator; no retail curve has one.
        n => Err(format!("curve {:?}: interpolation {n} is neither Hermite (0) nor linear (1)", c.name)),
    }
}

struct Link {
    input: usize,
    params: Vec<f32>,
}

struct Node {
    min: f32,
    max: f32,
    sum: bool,
    links: Vec<Link>,
}

/// One bone-pose node's hold on a master bone (`0x72ea80`): its offset from the reference pose and
/// its absolute rotation.
struct Contribution {
    pose: usize,
    d_pos: [f32; 3],
    rot: Quat,
    d_scale: [f32; 3],
}

struct FaceBone {
    name: String,
    pos: [f32; 3],
    rot: Quat,
    scale: [f32; 3],
    contributions: Vec<Contribution>,
}

/// An actor ready to solve: its graph, with each link's input resolved, and its master bones with
/// their contributions.
pub struct FaceRig {
    names: Vec<String>,
    nodes: Vec<Node>,
    /// The bone-pose nodes, in face-graph order: the weight array's order.
    poses: Vec<usize>,
    bones: Vec<FaceBone>,
}

/// An anim bound to a rig: each curve with the node it writes, if the actor has one of its name.
pub struct BoundAnim<'a> {
    pub anim: &'a Anim,
    targets: Vec<Option<usize>>,
}

impl<'a> BoundAnim<'a> {
    /// Curves that drive a node, and curves whose name is no node of the actor.
    pub fn bound_and_unbound(&self) -> (Vec<&'a str>, Vec<&'a str>) {
        let names = |bound: bool| {
            self.anim.curves.iter().zip(&self.targets).filter(|(_, t)| t.is_some() == bound).map(|(c, _)| c.name.as_str()).collect()
        };
        (names(true), names(false))
    }
}

/// A face bone's solved local transform, by its name.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceBonePose {
    pub name: String,
    pub local: QsTransform,
}

impl FaceRig {
    pub fn new(actor: &Actor) -> Result<FaceRig, String> {
        let names: Vec<String> = actor.nodes.iter().map(|n| n.name.clone()).collect();
        let find = |n: &str| names.iter().position(|m| m == n);
        let mut nodes = Vec::with_capacity(actor.nodes.len());
        for n in &actor.nodes {
            let sum = match n.input_operation {
                0 => true,
                1 => false,
                op => return Err(format!("node {:?}: input operation {op} is neither sum (0) nor multiply (1)", n.name)),
            };
            let mut links = Vec::new();
            for l in &n.links {
                // Only `linear` is spelled in the retail archives; another function's name is not
                // guessed.
                if l.function != "linear" {
                    return Err(format!("node {:?}: link function {:?} is not evaluated", n.name, l.function));
                }
                let input = find(&l.input).ok_or_else(|| format!("node {:?}: link from {:?}, no such node", n.name, l.input))?;
                links.push(Link { input, params: l.params.clone() });
            }
            nodes.push(Node { min: n.min, max: n.max, sum, links });
        }
        let poses: Vec<usize> =
            actor.nodes.iter().enumerate().filter(|(_, n)| matches!(n.kind, NodeKind::BonePose(_))).map(|(i, _)| i).collect();
        let bones = actor
            .master_bones
            .iter()
            .map(|m| {
                let b = &m.bone;
                let contributions = poses
                    .iter()
                    .enumerate()
                    .flat_map(|(k, &ni)| {
                        let NodeKind::BonePose(pb) = &actor.nodes[ni].kind else { unreachable!() };
                        pb.iter().filter(|p| p.name == b.name).map(move |p| Contribution {
                            pose: k,
                            d_pos: [p.position[0] - b.position[0], p.position[1] - b.position[1], p.position[2] - b.position[2]],
                            rot: p.rotation,
                            d_scale: [p.scale[0] - b.scale[0], p.scale[1] - b.scale[1], p.scale[2] - b.scale[2]],
                        })
                    })
                    .collect();
                FaceBone { name: b.name.clone(), pos: b.position, rot: b.rotation, scale: b.scale, contributions }
            })
            .collect();
        Ok(FaceRig { names, nodes, poses, bones })
    }

    /// The face bones' names, in master-bone order.
    pub fn bone_names(&self) -> impl Iterator<Item = &str> {
        self.bones.iter().map(|b| b.name.as_str())
    }

    /// Bind `anim`'s curves to this actor's nodes by exact name (`FindNode`: the first match).
    pub fn bind<'a>(&self, anim: &'a Anim) -> BoundAnim<'a> {
        let targets = anim.curves.iter().map(|c| self.names.iter().position(|n| *n == c.name)).collect();
        BoundAnim { anim, targets }
    }

    /// The bone-pose nodes' names, in face-graph order.
    pub fn pose_names(&self) -> impl Iterator<Item = &str> {
        self.poses.iter().map(|&n| self.names[n].as_str())
    }

    /// Solve the face at anim time `t` (seconds). Past the anim's end ([`Anim::bounds`]) nothing is
    /// evaluated (`0x72af60`), so every node is 0 and the bones sit at the reference pose.
    pub fn solve(&self, bound: &BoundAnim, t: f32) -> Result<Vec<FaceBonePose>, String> {
        let mut track = vec![None; self.nodes.len()];
        if t <= bound.anim.bounds().1 {
            for (c, target) in bound.anim.curves.iter().zip(&bound.targets) {
                let v = curve_value(c, t)?;
                if let Some(n) = target {
                    track[*n] = Some(v);
                }
            }
        }
        Ok(self.solve_tracks(&track))
    }

    /// Solve the face with node `name`'s track at 1 and every other track unset: one pose at full
    /// weight (and whatever it drives through the graph's links).
    pub fn hold(&self, name: &str) -> Result<Vec<FaceBonePose>, String> {
        let n = self.names.iter().position(|m| m == name).ok_or_else(|| format!("no node {name:?}"))?;
        let mut track = vec![None; self.nodes.len()];
        track[n] = Some(1.0);
        Ok(self.solve_tracks(&track))
    }

    fn solve_tracks(&self, track: &[Option<f32>]) -> Vec<FaceBonePose> {
        let mut value = vec![None; self.nodes.len()];
        let weights: Vec<f32> = self.poses.iter().map(|&n| self.value(n, track, &mut value)).collect();
        self.bones.iter().map(|b| FaceBonePose { name: b.name.clone(), local: blend(b, &weights) }).collect()
    }

    /// The face bones at the reference pose (every node 0): what the solve gives past an anim's end.
    pub fn reference_pose(&self) -> Vec<FaceBonePose> {
        let weights = vec![0.0; self.poses.len()];
        self.bones.iter().map(|b| FaceBonePose { name: b.name.clone(), local: blend(b, &weights) }).collect()
    }

    /// A node's value (`0x729b80`), memoised for the frame.
    fn value(&self, n: usize, track: &[Option<f32>], value: &mut [Option<f32>]) -> f32 {
        if let Some(v) = value[n] {
            return v;
        }
        let node = &self.nodes[n];
        let v = if node.links.is_empty() {
            track[n]
        } else {
            let mut acc = if node.sum { 0.0 } else { 1.0 };
            for l in &node.links {
                let x = self.value(l.input, track, value);
                let y = match l.params.as_slice() {
                    [p0, p1] => x * p0 + p1,
                    [p0] => x * p0,
                    _ => x,
                };
                if node.sum {
                    acc += y;
                } else {
                    acc *= y;
                }
            }
            Some(track[n].map_or(acc, |t| t + acc))
        };
        let v = v.map_or(0.0, |v| v.min(node.max).max(node.min));
        value[n] = Some(v);
        v
    }
}

/// One master bone's pose under the pose weights (`0x686ce0`): position and scale add each
/// contribution's offset by its weight; the rotation composes each contribution's turn from the
/// reference, slerped (or lerped above a dot of 0.55) by its weight; then renormalised once.
fn blend(b: &FaceBone, weights: &[f32]) -> QsTransform {
    let (mut pos, mut scale) = (b.pos, b.scale);
    let conj_r: Quat = [b.rot[0], -b.rot[1], -b.rot[2], -b.rot[3]];
    let mut a = b.rot;
    let mut r = b.rot;
    for c in &b.contributions {
        let w = weights[c.pose];
        if w.abs() < EPSILON {
            continue;
        }
        for i in 0..3 {
            pos[i] += w * c.d_pos[i];
            scale[i] += w * c.d_scale[i];
        }
        let q = c.rot;
        let sum = [q[0] + r[0], q[1] + r[1], q[2] + r[2], q[3] + r[3]];
        let diff = [r[0] - q[0], r[1] - q[1], r[2] - q[2], r[3] - q[3]];
        if dot(sum, sum) < dot(diff, diff) {
            r = [-r[0], -r[1], -r[2], -r[3]];
        }
        let d = dot(q, r);
        let s: Quat = if d <= SLERP_BELOW {
            let th = d.acos();
            let (sa, sb, st) = ((w * th).sin(), ((1.0 - w) * th).sin(), th.sin());
            std::array::from_fn(|i| (sa * q[i] + sb * r[i]) / st)
        } else {
            std::array::from_fn(|i| w * q[i] + (1.0 - w) * r[i])
        };
        a = qmul(a, qmul(conj_r, s));
    }
    let len2 = dot(a, a);
    if (len2 - 1.0).abs() > EPSILON {
        let len = len2.sqrt();
        a = if len < EPSILON { [1.0, 0.0, 0.0, 0.0] } else { a.map(|v| v / len) };
    }
    // The pose buffer takes X mirrored: (−x, y, z) and the rotation as (x, y, z, w) = (−x, y, z, −w).
    QsTransform { translation: [-pos[0], pos[1], pos[2]], rotation: [-a[1], a[2], a[3], -a[0]], scale }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use mercs2_formats::types::{TYPE_HASH_FACEFX_ACTOR, TYPE_HASH_MODEL};

    /// The chunk of `name` with type `ty` from retail `vz.wad`: every ASET row of the name names a
    /// block, and the first of those holding the chunk gives it.
    fn chunk(name: &str, ty: u32) -> Vec<u8> {
        let path = mercs2_formats::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap_or_else(|e| panic!("{e}"));
        let mut f = std::fs::File::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let size = f.metadata().unwrap().len();
        let arch = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("{e}"));
        let h = mercs2_formats::hash::pandemic_hash_m2(name);
        for row in arch.aset.iter().filter(|a| a.asset_hash == h) {
            let block = mercs2_formats::sges::decompress_block(&mut f, &arch.indx, row.block_index()).unwrap_or_else(|e| panic!("{e}"));
            let spans = mercs2_formats::ucfx::block_chunk_spans(&block).unwrap_or_else(|e| panic!("{e}"));
            if let Some((_, span)) = spans.into_iter().find(|(e, _)| e.name_hash == h && e.type_hash == ty) {
                return block[span].to_vec();
            }
        }
        panic!("{name}: no chunk of type 0x{ty:08X} in {}", path.display())
    }

    /// With every pose at 0, the solve gives each of Mattias's 20 face bones its model's own HIER
    /// local (`+16`): the mirror the game applies on the write lands the FaceFX reference pose in
    /// the space the body clips are sampled in.
    #[test]
    fn the_reference_pose_is_the_models_face_bones() {
        let actor = mercs2_formats::facefx::parse_actor_container(&chunk("pmc_hum_mattias", TYPE_HASH_FACEFX_ACTOR))
            .unwrap_or_else(|e| panic!("{e}"));
        let hier = mercs2_formats::orchestrator::parse_hier(&chunk("pmc_hum_mattias", TYPE_HASH_MODEL));
        let rig = super::FaceRig::new(&actor.root).unwrap_or_else(|e| panic!("{e}"));
        let rest = rig.reference_pose();
        assert_eq!(rest.len(), 20);
        for p in rest {
            let h = mercs2_formats::hash::pandemic_hash_m2(&p.name);
            let n = hier.iter().find(|n| n.hash == h).unwrap_or_else(|| panic!("{} is not in the model's HIER", p.name));
            let m: [[f32; 4]; 4] = std::array::from_fn(|r| std::array::from_fn(|c| n.local[r * 4 + c]));
            let q = crate::pose::mat_to_qs(&m);
            let near = |a: &[f32], b: &[f32]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5);
            assert!(near(&p.local.translation, &q.translation), "{}: {:?} vs {:?}", p.name, p.local.translation, q.translation);
            assert!(near(&p.local.rotation, &q.rotation), "{}: {:?} vs {:?}", p.name, p.local.rotation, q.rotation);
        }
    }

    /// `Mattias.Greeting01` of his job-briefing set moves his jaw while it plays (sampled at 30 Hz)
    /// and leaves every face bone at the reference pose past its end. His jaw poses are small (the
    /// widest turns the jaw about 0.11 rad at full weight) and this line's curves stay under 0.85,
    /// so it turns the jaw about 0.04 rad at most; 0.02 is "moved", not noise.
    #[test]
    fn a_briefing_line_moves_the_jaw_and_ends_at_rest() {
        use mercs2_formats::types::TYPE_HASH_FACEFX_ANIMSET;
        let actor = mercs2_formats::facefx::parse_actor_container(&chunk("pmc_hum_mattias", TYPE_HASH_FACEFX_ACTOR))
            .unwrap_or_else(|e| panic!("{e}"));
        let set = mercs2_formats::facefx::parse_animset_container(&chunk("Global_Job_Briefing_Mattias", TYPE_HASH_FACEFX_ANIMSET))
            .unwrap_or_else(|e| panic!("{e}"));
        let anim = set.root.group.anims.iter().find(|a| a.name == "Mattias.Greeting01").expect("Mattias.Greeting01");
        let rig = super::FaceRig::new(&actor.root).unwrap_or_else(|e| panic!("{e}"));
        let bound = rig.bind(anim);
        let rest = rig.reference_pose();
        let jaw = rest.iter().position(|p| p.name == "bone_jaw").expect("bone_jaw");
        let end = anim.bounds().1;
        let mut most = 0.0f32;
        for i in 0..=(end * 30.0) as usize {
            let pose = rig.solve(&bound, i as f32 / 30.0).unwrap_or_else(|e| panic!("{e}"));
            let (a, b) = (pose[jaw].local.rotation, rest[jaw].local.rotation);
            let d = (a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]).abs().min(1.0);
            most = most.max(2.0 * d.acos());
        }
        assert!(most > 0.02, "the jaw turns at most {most} rad over {end} s");
        assert_eq!(rig.solve(&bound, end + 0.1).unwrap_or_else(|e| panic!("{e}")), rest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::facefx::{Bone, Key, MasterBone, Node as FxNode};

    fn bone(name: &str, pos: [f32; 3], rot: Quat) -> Bone {
        Bone { name: name.into(), position: pos, rotation: rot, scale: [1.0; 3] }
    }

    fn node(name: &str, kind: NodeKind) -> FxNode {
        FxNode { class: String::new(), name: name.into(), min: 0.0, max: 1.0, input_operation: 0, links: vec![], properties: vec![], kind }
    }

    /// A jaw whose "Open" pose drops it 1 along Y and turns it 90° about X.
    fn actor() -> Actor {
        let h = std::f32::consts::FRAC_1_SQRT_2;
        Actor {
            name: "A".into(),
            master_bones: vec![MasterBone { bone: bone("jaw", [1.0, 2.0, 3.0], [1.0, 0.0, 0.0, 0.0]), index: 0, weight: Some(1.0) }],
            nodes: vec![node("Open", NodeKind::BonePose(vec![bone("jaw", [1.0, 1.0, 3.0], [h, h, 0.0, 0.0])]))],
            groups: vec![],
            names: vec![],
            phoneme_map: None,
        }
    }

    fn anim(name: &str, keys: &[[f32; 4]]) -> Anim {
        Anim {
            name: "anim".into(),
            curves: vec![Curve { name: name.into(), interpolation: 0, keys: keys.iter().map(|k| Key(*k)).collect() }],
            f20: 0.0,
            f24: 0.0,
            bone_weights: vec![],
            s38: String::new(),
            s44: String::new(),
            u50: None,
        }
    }

    #[test]
    fn hermite_with_flat_slopes_eases_between_keys_and_holds_outside() {
        let c = &anim("x", &[[0.0, 0.0, 0.0, 0.0], [2.0, 1.0, 0.0, 0.0]]).curves[0];
        assert_eq!(curve_value(c, -1.0).unwrap(), 0.0);
        assert_eq!(curve_value(c, 1.0).unwrap(), 0.5);
        assert_eq!(curve_value(c, 0.5).unwrap(), 0.15625);
        assert_eq!(curve_value(c, 3.0).unwrap(), 1.0);
    }

    #[test]
    fn a_full_weight_pose_is_the_pose_mirrored_into_the_clip_space() {
        let a = actor();
        let rig = FaceRig::new(&a).unwrap();
        let an = anim("Open", &[[0.0, 1.0, 0.0, 0.0]]);
        let bound = rig.bind(&an);
        let p = &rig.solve(&bound, 0.0).unwrap()[0].local;
        assert_eq!(p.translation, [-1.0, 1.0, 3.0]);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let want = [-h, 0.0, 0.0, -h];
        assert!(p.rotation.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6), "{:?}", p.rotation);
        assert_eq!(rig.hold("Open").unwrap()[0].local, *p);
        assert_eq!(rig.pose_names().collect::<Vec<_>>(), ["Open"]);
    }

    #[test]
    fn half_weight_turns_halfway_and_past_the_end_is_the_reference_pose() {
        let a = actor();
        let rig = FaceRig::new(&a).unwrap();
        let an = anim("Open", &[[0.0, 0.5, 0.0, 0.0]]);
        let bound = rig.bind(&an);
        let p = &rig.solve(&bound, 0.0).unwrap()[0].local;
        assert_eq!(p.translation, [-1.0, 1.5, 3.0]);
        let (c, s) = ((std::f32::consts::PI / 8.0).cos(), (std::f32::consts::PI / 8.0).sin());
        let want = [-s, 0.0, 0.0, -c];
        assert!(p.rotation.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6), "{:?}", p.rotation);
        let rest = &rig.solve(&bound, 2.0).unwrap()[0].local;
        assert_eq!((rest.translation, rest.rotation), ([-1.0, 2.0, 3.0], [-0.0, 0.0, 0.0, -1.0]));
    }

    #[test]
    fn a_curve_with_no_node_drives_nothing() {
        let a = actor();
        let rig = FaceRig::new(&a).unwrap();
        let an = anim("Gaze Eye Yaw", &[[0.0, 1.0, 0.0, 0.0]]);
        let bound = rig.bind(&an);
        assert_eq!(bound.bound_and_unbound(), (vec![], vec!["Gaze Eye Yaw"]));
        assert_eq!(rig.solve(&bound, 0.0).unwrap()[0].local.translation, [-1.0, 2.0, 3.0]);
    }

    #[test]
    fn a_linked_node_takes_its_input_through_the_linear_function() {
        let mut a = actor();
        a.nodes[0].links.push(mercs2_formats::facefx::Link { input: "Driver".into(), function: "linear".into(), params: vec![0.5, 0.25] });
        a.nodes.push(node("Driver", NodeKind::Combiner));
        let rig = FaceRig::new(&a).unwrap();
        // Open = 0.5 · Driver(1.0) + 0.25 = 0.75 → the jaw drops 0.75.
        let an = anim("Driver", &[[0.0, 1.0, 0.0, 0.0]]);
        let p = &rig.solve(&rig.bind(&an), 0.0).unwrap()[0].local;
        assert_eq!(p.translation, [-1.0, 1.25, 3.0]);
        a.nodes[0].links[0].function = "quadratic".into();
        assert!(FaceRig::new(&a).err().unwrap().contains("\"quadratic\" is not evaluated"));
    }
}
