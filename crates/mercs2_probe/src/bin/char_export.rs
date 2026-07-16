//! `char_export` — export a rigged character + all its animation clips to glTF 2.0 (`.glb`).
//!
//! A community-usable dump of the playable heroes (Chris / Jennifer / Mattias): the assembled,
//! SKINNED body mesh, the named 100-bone skeleton, and every clip from the character's animgroup
//! block, wired as glTF `skins` + `animations`. Opens directly in Blender / Unity / Unreal / Maya.
//!
//! The engine's proven Havok pipeline does the work: `Model::flatten` gives skinned vertices
//! (pos/normal/uv + BLENDINDICES/BLENDWEIGHT) and the bind-pose rig (parent/world_bind/inv_bind/
//! local_bind); `AnimClip::sample_local(t)` samples a clip's per-track local `hkQsTransform`s, which
//! `sampleAndCombine` (replicated here from `pose::havok_locals`) folds onto the bind pose. A
//! `QsTransform` (translation / rotation-quat / scale) maps 1:1 onto a glTF TRS animation channel,
//! and the QS composition already uses the standard parent·child convention glTF expects — so the
//! only conversion is Havok's LEFT-handed space to glTF's RIGHT-handed one (negate Z + conjugate the
//! quaternion x,y + flip triangle winding).
//!
//! usage:
//!   char_export <0xMODELHASH> <0xANIMBLOCK> <outdir> --name NAME [--per-clip] [--combined]
//!   (default: both a combined <NAME>.glb and a clips/ folder of one-animation .glb files)

use std::collections::HashMap;
use std::io::Write;

use mercs2_engine::{game_world, mesh, model::Model, pose, render::ClipAnim, wad};
use mercs2_formats::anim::QsTransform;

// ── little-endian binary buffer with 4-byte alignment (glTF requires it) ─────────────────────
#[derive(Default)]
struct Buf(Vec<u8>);
impl Buf {
    fn align(&mut self) {
        while self.0.len() % 4 != 0 {
            self.0.push(0);
        }
    }
    /// Append `data`, returning (byteOffset, byteLength). Caller records a bufferView.
    fn put(&mut self, data: &[u8]) -> (usize, usize) {
        self.align();
        let off = self.0.len();
        self.0.extend_from_slice(data);
        (off, data.len())
    }
    fn put_f32(&mut self, v: &[f32]) -> (usize, usize) {
        let mut b = Vec::with_capacity(v.len() * 4);
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
        self.put(&b)
    }
}

// A glTF JSON is assembled as serde_json::Value arrays we push onto.
use serde_json::{json, Value};

struct Gltf {
    buf: Buf,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
}
impl Gltf {
    fn new() -> Self {
        Gltf { buf: Buf::default(), buffer_views: vec![], accessors: vec![] }
    }
    fn view(&mut self, off: usize, len: usize, target: Option<u32>) -> usize {
        let mut v = json!({"buffer":0,"byteOffset":off,"byteLength":len});
        if let Some(t) = target {
            v["target"] = json!(t);
        }
        self.buffer_views.push(v);
        self.buffer_views.len() - 1
    }
    /// f32 accessor (SCALAR/VEC2/VEC3/VEC4/MAT4), optionally with min/max (needed for POSITION).
    fn acc_f32(&mut self, data: &[f32], comps: usize, ty: &str, target: Option<u32>, minmax: bool) -> usize {
        let count = data.len() / comps;
        let (off, len) = self.buf.put_f32(data);
        let bv = self.view(off, len, target);
        let mut a = json!({"bufferView":bv,"componentType":5126,"count":count,"type":ty});
        if minmax {
            let mut mn = vec![f32::INFINITY; comps];
            let mut mx = vec![f32::NEG_INFINITY; comps];
            for c in data.chunks(comps) {
                for i in 0..comps {
                    mn[i] = mn[i].min(c[i]);
                    mx[i] = mx[i].max(c[i]);
                }
            }
            a["min"] = json!(mn);
            a["max"] = json!(mx);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
    fn acc_u16(&mut self, data: &[u16], comps: usize, ty: &str, target: Option<u32>) -> usize {
        let count = data.len() / comps;
        let mut b = Vec::with_capacity(data.len() * 2);
        for x in data {
            b.extend_from_slice(&x.to_le_bytes());
        }
        let (off, len) = self.buf.put(&b);
        let bv = self.view(off, len, target);
        self.accessors
            .push(json!({"bufferView":bv,"componentType":5123,"count":count,"type":ty}));
        self.accessors.len() - 1
    }
    fn acc_u8x4(&mut self, data: &[[u8; 4]]) -> usize {
        let mut b = Vec::with_capacity(data.len() * 4);
        for x in data {
            b.extend_from_slice(x);
        }
        let (off, len) = self.buf.put(&b);
        let bv = self.view(off, len, Some(34962));
        // JOINTS_0 as unsigned byte, VEC4
        self.accessors.push(
            json!({"bufferView":bv,"componentType":5121,"count":data.len(),"type":"VEC4"}),
        );
        self.accessors.len() - 1
    }
}

// ── LH(+Y up) → RH(+Y up): negate Z. Reflection across XY: point z→-z, quat (x,y,z,w)→(-x,-y,z,w). ──
fn conv_pos(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[1], -p[2]]
}
fn conv_quat(q: [f32; 4]) -> [f32; 4] {
    [-q[0], -q[1], q[2], q[3]]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let getflag = |f: &str| args.iter().any(|a| a == f);
    let getval = |f: &str| {
        args.iter().position(|a| a == f).and_then(|i| args.get(i + 1)).cloned()
    };
    let hexarg = |i: usize| -> Option<u32> {
        args.get(i).and_then(|a| a.strip_prefix("0x")).and_then(|h| u32::from_str_radix(h, 16).ok())
    };
    let model_hash = hexarg(1).unwrap_or(0x0BBA3066); // pmc_hum_mattias
    let anim_block = hexarg(2); // 0xBLOCKINDEX of the character's animgroup, or None = rig-matched
    let outdir = args.get(3).filter(|a| !a.starts_with("--")).cloned().unwrap_or_else(|| "output/char".into());
    let name = getval("--name").unwrap_or_else(|| format!("char_{model_hash:08X}"));
    let want_per_clip = getflag("--per-clip") || !getflag("--combined");
    let want_combined = getflag("--combined") || !getflag("--per-clip");

    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");
    let m = Model::load(&mut w, model_hash).expect("load model");
    let (verts, indices, draws, stats) = m.flatten();
    let rig = &stats.rig;
    let single_rung = m.rungs.len() == 1;

    // Destruction filter — the AUTHORITATIVE engine gate, not a heuristic. A vehicle ships its intact
    // body AND its wreck in one container; the SWIT state machine SHOW/Hides one at a time (the game
    // never co-renders them). `machine_node_enable` replays the pristine state's enter-script over the
    // HIER exactly as `render_state` clause (3) does — `NodeSeed::SwitchSlotsHidden` hides every switch
    // subtree until a SHOW re-enables it, so the wreck stays hidden and only the intact body survives.
    let keep_node: Vec<bool> = match &m.machine {
        Some(sm) => {
            use mercs2_formats::orchestrator as orch;
            // Full health = the pristine state; its chosen state indices drive the SHOW/Hide replay.
            let chosen = orch::node_states_for_health(sm, 1.0, 0.99);
            orch::machine_node_enable(sm, &m.hier, &chosen)
        }
        None => vec![true; rig.len()],
    };
    let hier: Vec<u32> = rig.iter().map(|b| b.name_hash).collect();
    eprintln!(
        "[char_export] {name}: 0x{model_hash:08X} — {} verts, {} draws, {} bones",
        verts.len(),
        draws.len(),
        rig.len()
    );

    // Clip set: every clip that belongs to this rig. `--static` skips it (vehicles have no clips of
    // their own — rig-matching would pull unrelated character clips that share generic bone hashes).
    let clips = if getflag("--static") {
        Vec::new()
    } else {
        load_all_clips(&mut w, &hier, anim_block)
    };
    eprintln!("[char_export] {} clips resolve & decode against this rig", clips.len());

    std::fs::create_dir_all(&outdir).ok();

    if want_combined {
        let path = format!("{outdir}/{name}.glb");
        write_glb(&path, &name, &verts, &indices, &draws, rig, &keep_node, single_rung, &clips);
        eprintln!("[char_export] -> {path}  ({} animations)", clips.len());
    }
    if want_per_clip {
        let cdir = format!("{outdir}/{name}_clips");
        std::fs::create_dir_all(&cdir).ok();
        for c in &clips {
            let cname = clip_label(c.name_hash);
            let path = format!("{cdir}/{cname}.glb");
            write_glb(&path, &name, &verts, &indices, &draws, rig, &keep_node, single_rung, std::slice::from_ref(c));
        }
        eprintln!("[char_export] -> {cdir}/  ({} individual clip files)", clips.len());
    }
}

/// All clips authored for this rig, decoded and bound. Uses the WAD clip index's rig selection so we
/// catch the character's whole animgroup, then decodes each. `anim_block` (if given) restricts to one
/// block so sibling characters' clips (same skeleton) don't bleed in.
fn load_all_clips(w: &mut wad::Wad, hier: &[u32], anim_block: Option<u32>) -> Vec<ClipAnim> {
    let idx = game_world::clip_index(w);
    let wants: Vec<u32> = if let Some(blk) = anim_block {
        idx.clips.iter().filter(|c| c.block as u32 == blk).map(|c| c.name_hash).collect()
    } else {
        idx.select_for_rig(hier, 512)
    };
    let mut out = Vec::new();
    for h in wants {
        for c in game_world::load_clips_for_rig(w, hier, &[h]).into_iter().flatten() {
            if c.clip.decoded && c.clip.num_frames >= 1 {
                out.push(c);
            }
        }
    }
    out
}

/// A readable file/animation name for a clip. Names aren't stored, so use the hex hash; the caller can
/// rename from footstep annotations later.
fn clip_label(h: u32) -> String {
    format!("clip_{h:08X}")
}

/// Replicated `pose::havok_locals`: bind local per bone, driven bones overwritten by the clip sample.
fn locals_for(
    rig: &[mesh::BoneRig],
    sample: &[QsTransform],
    track_to_hier: &[Option<usize>],
    ntt: usize,
) -> Vec<QsTransform> {
    let mut local = pose::bind_qs(rig);
    for (track, bone) in track_to_hier.iter().enumerate() {
        if track >= ntt {
            break;
        }
        if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
            if b < local.len() {
                let mut q = *qs;
                let n = (q.rotation.iter().map(|c| c * c).sum::<f32>()).sqrt();
                if n > 1e-6 {
                    for c in q.rotation.iter_mut() {
                        *c /= n;
                    }
                }
                local[b] = q;
            }
        }
    }
    local
}

#[allow(clippy::too_many_arguments)]
fn write_glb(
    path: &str,
    name: &str,
    verts: &[mesh::Vertex],
    indices: &[u32],
    draws: &[mesh::DrawGroup],
    rig: &[mesh::BoneRig],
    keep_node: &[bool],
    single_rung: bool,
    clips: &[ClipAnim],
) {
    let mut g = Gltf::new();
    let nb = rig.len();

    // A model ships 1-3 LOD rungs merged into `draws`; the SAME part appears at several detail levels
    // and overlaps ("two detail levels fighting for the same pixels"). Export only the finest tier
    // (lod_mask bit 0), plus always-visible groups (mask 0), so each part is emitted once. `load()`
    // has already cleared coarser bits per (node,tier), so this is exactly the closest-LOD model.
    // Also drop destruction break-pieces (keep_node) so only the pristine body is emitted.
    let draws: Vec<&mesh::DrawGroup> = draws
        .iter()
        .filter(|d| (single_rung || d.lod_mask & 1 != 0) && (d.node < 0 || keep_node.get(d.node as usize).copied().unwrap_or(true)))
        .collect();

    // Which draw group owns each vertex — a RIGID (node-mounted) group's verts are baked into that
    // bone's LOCAL space, so we lift them to model space via the bone's bind world transform and bind
    // them 100% to that bone. A SKINNED group's verts are already model-space with real joints/weights.
    // Unified: everything becomes a skinned vertex, so one skin + the animation path drives it all
    // (a turret/wheel bone rotating carries its rigid mesh exactly like the game does).
    let mut vert_group = vec![usize::MAX; verts.len()];
    for (gi, d) in draws.iter().enumerate() {
        for &vi in &indices[d.index_start as usize..(d.index_start + d.index_count) as usize] {
            vert_group[vi as usize] = gi;
        }
    }

    // ── mesh attributes (lifted to model space, converted to RH) ────────────────────────────
    let mut pos = Vec::with_capacity(verts.len() * 3);
    let mut nrm = Vec::with_capacity(verts.len() * 3);
    let mut uv = Vec::with_capacity(verts.len() * 2);
    let mut joints: Vec<[u8; 4]> = Vec::with_capacity(verts.len());
    let mut weights = Vec::with_capacity(verts.len() * 4);
    for (i, v) in verts.iter().enumerate() {
        let g = vert_group[i];
        let rigid_node = if g != usize::MAX && !draws[g].skinned && draws[g].node >= 0 {
            Some(draws[g].node as usize)
        } else {
            None
        };
        let (p, n, j, wt) = if let Some(node) = rigid_node.filter(|&n| n < nb) {
            // `flatten` already places rigid verts in MODEL space, so we do NOT re-apply world_bind
            // (that double-transforms and scatters the part). Just bind 100% to the mount bone so it
            // rides that bone under animation; at bind pose jointWorld·IBM == identity keeps it put.
            (v.pos, v.normal, [node as u8, 0, 0, 0], [1.0f32, 0.0, 0.0, 0.0])
        } else {
            let ws: f32 = v.weights.iter().map(|&x| x as f32).sum::<f32>();
            if ws <= 0.0 {
                // static/unbound vertex (node = -1): pin to the skeleton root so it stays put
                (v.pos, v.normal, [0u8, 0, 0, 0], [1.0f32, 0.0, 0.0, 0.0])
            } else {
                (
                    v.pos,
                    v.normal,
                    v.joints,
                    [
                        v.weights[0] as f32 / ws,
                        v.weights[1] as f32 / ws,
                        v.weights[2] as f32 / ws,
                        v.weights[3] as f32 / ws,
                    ],
                )
            }
        };
        let p = conv_pos(p);
        pos.extend_from_slice(&p);
        let nn = conv_pos(n);
        nrm.extend_from_slice(&nn);
        uv.extend_from_slice(&[v.uv[0], v.uv[1]]);
        joints.push(j);
        weights.extend_from_slice(&wt);
    }
    let a_pos = g.acc_f32(&pos, 3, "VEC3", Some(34962), true);
    let a_nrm = g.acc_f32(&nrm, 3, "VEC3", Some(34962), false);
    let a_uv = g.acc_f32(&uv, 2, "VEC2", Some(34962), false);
    let a_jnt = g.acc_u8x4(&joints);
    let a_wgt = g.acc_f32(&weights, 4, "VEC4", Some(34962), false);

    // ── primitives: one per draw group (winding flipped for the RH reflection) ──────────────
    let mut primitives = Vec::new();
    for d in draws {
        let s = d.index_start as usize;
        let e = s + d.index_count as usize;
        let mut idx: Vec<u16> = Vec::with_capacity(d.index_count as usize);
        for tri in indices[s..e].chunks(3) {
            if tri.len() == 3 {
                idx.push(tri[0] as u16);
                idx.push(tri[2] as u16); // swap 1<->2: reverse winding
                idx.push(tri[1] as u16);
            }
        }
        let a_idx = g.acc_u16(&idx, 1, "SCALAR", Some(34963));
        primitives.push(json!({
            "attributes": {"POSITION":a_pos,"NORMAL":a_nrm,"TEXCOORD_0":a_uv,"JOINTS_0":a_jnt,"WEIGHTS_0":a_wgt},
            "indices": a_idx,
            "material": 0
        }));
    }

    // ── skeleton nodes: bind-pose local TRS, converted to RH ────────────────────────────────
    let bind_local = pose::bind_qs(rig);
    let mut nodes: Vec<Value> = Vec::with_capacity(nb + 2);
    let mut children_of: Vec<Vec<usize>> = vec![vec![]; nb];
    for (b, br) in rig.iter().enumerate() {
        if br.parent >= 0 {
            children_of[br.parent as usize].push(b);
        }
    }
    // bone node index = bone index (0..nb). Mesh node = nb. Skin uses these joints.
    for b in 0..nb {
        let qs = bind_local[b];
        let t = conv_pos(qs.translation);
        let r = conv_quat(qs.rotation);
        let mut node = json!({
            "name": bone_name(rig[b].name_hash),
            "translation": t, "rotation": r, "scale": qs.scale,
        });
        if !children_of[b].is_empty() {
            node["children"] = json!(children_of[b]);
        }
        nodes.push(node);
    }
    let mesh_node = nodes.len();
    nodes.push(json!({"name": format!("{name}_mesh"), "mesh": 0, "skin": 0}));

    // scene roots: bone roots + the skinned mesh node
    let roots: Vec<usize> =
        (0..nb).filter(|&b| rig[b].parent < 0).chain(std::iter::once(mesh_node)).collect();
    let skel_root = (0..nb).find(|&b| rig[b].parent < 0).unwrap_or(0);

    // ── skin: inverse bind matrices ─────────────────────────────────────────────────────────
    // Derive IBM by composing the CONVERTED node bind transforms (exactly as glTF will) and
    // inverting. This guarantees jointWorld·IBM == identity at bind pose, so the mesh stays put —
    // no fragile hand-conversion of the game's inv_bind matrix.
    let mut bind_world: Vec<[f32; 16]> = vec![IDENT16; nb]; // column-major, glTF convention
    for b in 0..nb {
        let qs = bind_local[b];
        let local = qs_to_mat4(conv_pos(qs.translation), conv_quat(qs.rotation), qs.scale);
        bind_world[b] = if rig[b].parent >= 0 {
            mat4_mul(&bind_world[rig[b].parent as usize], &local)
        } else {
            local
        };
    }
    let mut ibm = Vec::with_capacity(nb * 16);
    for b in 0..nb {
        ibm.extend_from_slice(&mat4_invert(&bind_world[b]));
    }
    let a_ibm = g.acc_f32(&ibm, 16, "MAT4", None, false);
    let skins = json!([{
        "inverseBindMatrices": a_ibm,
        "joints": (0..nb).collect::<Vec<_>>(),
        "skeleton": skel_root
    }]);

    // ── animations ──────────────────────────────────────────────────────────────────────────
    let mut animations = Vec::new();
    for c in clips {
        let frames = c.clip.num_frames.max(1);
        let dur = c.clip.duration.max(1e-3);
        let dt = dur / frames.max(1) as f32;
        // times accessor (shared across channels of this clip)
        let times: Vec<f32> = (0..frames).map(|f| f as f32 * dt).collect();
        let a_time = g.acc_f32(&times, 1, "SCALAR", None, true);
        // per-bone TRS tracks over frames
        let mut t_out: Vec<Vec<f32>> = vec![Vec::with_capacity(frames * 3); nb];
        let mut r_out: Vec<Vec<f32>> = vec![Vec::with_capacity(frames * 4); nb];
        let mut s_out: Vec<Vec<f32>> = vec![Vec::with_capacity(frames * 3); nb];
        for f in 0..frames {
            let sample = c.clip.sample_local(f as f32 * dt);
            let local = locals_for(rig, &sample, &c.track_to_hier, c.num_transform_tracks);
            for b in 0..nb {
                let t = conv_pos(local[b].translation);
                let r = conv_quat(local[b].rotation);
                t_out[b].extend_from_slice(&t);
                r_out[b].extend_from_slice(&r);
                s_out[b].extend_from_slice(&local[b].scale);
            }
        }
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for b in 0..nb {
            let at = g.acc_f32(&t_out[b], 3, "VEC3", None, false);
            let ar = g.acc_f32(&r_out[b], 4, "VEC4", None, false);
            let asc = g.acc_f32(&s_out[b], 3, "VEC3", None, false);
            let si = samplers.len();
            samplers.push(json!({"input":a_time,"output":at,"interpolation":"LINEAR"}));
            channels.push(json!({"sampler":si,"target":{"node":b,"path":"translation"}}));
            samplers.push(json!({"input":a_time,"output":ar,"interpolation":"LINEAR"}));
            channels.push(json!({"sampler":samplers.len()-1,"target":{"node":b,"path":"rotation"}}));
            samplers.push(json!({"input":a_time,"output":asc,"interpolation":"LINEAR"}));
            channels.push(json!({"sampler":samplers.len()-1,"target":{"node":b,"path":"scale"}}));
        }
        animations.push(json!({"name": clip_label(c.name_hash), "samplers": samplers, "channels": channels}));
    }

    // ── assemble the glTF JSON ──────────────────────────────────────────────────────────────
    let root = json!({
        "asset": {"version":"2.0","generator":"mercs2 char_export"},
        "scene": 0,
        "scenes": [{"name": name, "nodes": roots}],
        "nodes": nodes,
        "meshes": [{"name": format!("{name}_mesh"), "primitives": primitives}],
        "skins": skins,
        "materials": [{"name":"body","pbrMetallicRoughness":{"baseColorFactor":[0.8,0.8,0.8,1.0],"metallicFactor":0.0,"roughnessFactor":0.9},"doubleSided":true}],
        "animations": animations,
        "accessors": g.accessors,
        "bufferViews": g.buffer_views,
        "buffers": [{"byteLength": g.buf.0.len()}],
    });
    write_glb_file(path, &root, &g.buf.0);
}

const IDENT16: [f32; 16] = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

/// Transform a POINT by a row-major row-vector matrix (game HIER convention: `v' = v · M`,
/// translation in row 3). Used to lift a rigid part's node-local vertex into model space.
fn xform_point(v: [f32; 3], m: &[[f32; 4]; 4]) -> [f32; 3] {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0] + m[3][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1] + m[3][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2] + m[3][2],
    ]
}

/// Transform a DIRECTION (normal) — the rotation/scale 3×3 part only, no translation.
fn xform_dir(v: [f32; 3], m: &[[f32; 4]; 4]) -> [f32; 3] {
    [
        v[0] * m[0][0] + v[1] * m[1][0] + v[2] * m[2][0],
        v[0] * m[0][1] + v[1] * m[1][1] + v[2] * m[2][1],
        v[0] * m[0][2] + v[1] * m[1][2] + v[2] * m[2][2],
    ]
}

/// Build a column-major TRS matrix (glTF/standard column-vector: `p' = T·R·S·p`).
fn qs_to_mat4(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = q;
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    // rotation (column-major)
    let r = [
        1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0,
        2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0,
        2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0,
        0.0, 0.0, 0.0, 1.0,
    ];
    // scale columns, then set translation
    [
        r[0] * s[0], r[1] * s[0], r[2] * s[0], 0.0,
        r[4] * s[1], r[5] * s[1], r[6] * s[1], 0.0,
        r[8] * s[2], r[9] * s[2], r[10] * s[2], 0.0,
        t[0], t[1], t[2], 1.0,
    ]
}

/// Column-major 4x4 multiply: `out = a · b` (column-vector convention).
fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

/// General 4x4 inverse (column-major). Falls back to identity on a singular matrix.
fn mat4_invert(m: &[f32; 16]) -> [f32; 16] {
    let mut inv = [0.0f32; 16];
    inv[0] = m[5]*m[10]*m[15] - m[5]*m[11]*m[14] - m[9]*m[6]*m[15] + m[9]*m[7]*m[14] + m[13]*m[6]*m[11] - m[13]*m[7]*m[10];
    inv[4] = -m[4]*m[10]*m[15] + m[4]*m[11]*m[14] + m[8]*m[6]*m[15] - m[8]*m[7]*m[14] - m[12]*m[6]*m[11] + m[12]*m[7]*m[10];
    inv[8] = m[4]*m[9]*m[15] - m[4]*m[11]*m[13] - m[8]*m[5]*m[15] + m[8]*m[7]*m[13] + m[12]*m[5]*m[11] - m[12]*m[7]*m[9];
    inv[12] = -m[4]*m[9]*m[14] + m[4]*m[10]*m[13] + m[8]*m[5]*m[14] - m[8]*m[6]*m[13] - m[12]*m[5]*m[10] + m[12]*m[6]*m[9];
    inv[1] = -m[1]*m[10]*m[15] + m[1]*m[11]*m[14] + m[9]*m[2]*m[15] - m[9]*m[3]*m[14] - m[13]*m[2]*m[11] + m[13]*m[3]*m[10];
    inv[5] = m[0]*m[10]*m[15] - m[0]*m[11]*m[14] - m[8]*m[2]*m[15] + m[8]*m[3]*m[14] + m[12]*m[2]*m[11] - m[12]*m[3]*m[10];
    inv[9] = -m[0]*m[9]*m[15] + m[0]*m[11]*m[13] + m[8]*m[1]*m[15] - m[8]*m[3]*m[13] - m[12]*m[1]*m[11] + m[12]*m[3]*m[9];
    inv[13] = m[0]*m[9]*m[14] - m[0]*m[10]*m[13] - m[8]*m[1]*m[14] + m[8]*m[2]*m[13] + m[12]*m[1]*m[10] - m[12]*m[2]*m[9];
    inv[2] = m[1]*m[6]*m[15] - m[1]*m[7]*m[14] - m[5]*m[2]*m[15] + m[5]*m[3]*m[14] + m[13]*m[2]*m[7] - m[13]*m[3]*m[6];
    inv[6] = -m[0]*m[6]*m[15] + m[0]*m[7]*m[14] + m[4]*m[2]*m[15] - m[4]*m[3]*m[14] - m[12]*m[2]*m[7] + m[12]*m[3]*m[6];
    inv[10] = m[0]*m[5]*m[15] - m[0]*m[7]*m[13] - m[4]*m[1]*m[15] + m[4]*m[3]*m[13] + m[12]*m[1]*m[7] - m[12]*m[3]*m[5];
    inv[14] = -m[0]*m[5]*m[14] + m[0]*m[6]*m[13] + m[4]*m[1]*m[14] - m[4]*m[2]*m[13] - m[12]*m[1]*m[6] + m[12]*m[2]*m[5];
    inv[3] = -m[1]*m[6]*m[11] + m[1]*m[7]*m[10] + m[5]*m[2]*m[11] - m[5]*m[3]*m[10] - m[9]*m[2]*m[7] + m[9]*m[3]*m[6];
    inv[7] = m[0]*m[6]*m[11] - m[0]*m[7]*m[10] - m[4]*m[2]*m[11] + m[4]*m[3]*m[10] + m[8]*m[2]*m[7] - m[8]*m[3]*m[6];
    inv[11] = -m[0]*m[5]*m[11] + m[0]*m[7]*m[9] + m[4]*m[1]*m[11] - m[4]*m[3]*m[9] - m[8]*m[1]*m[7] + m[8]*m[3]*m[5];
    inv[15] = m[0]*m[5]*m[10] - m[0]*m[6]*m[9] - m[4]*m[1]*m[10] + m[4]*m[2]*m[9] + m[8]*m[1]*m[6] - m[8]*m[2]*m[5];
    let det = m[0]*inv[0] + m[1]*inv[4] + m[2]*inv[8] + m[3]*inv[12];
    if det.abs() < 1e-12 {
        return IDENT16;
    }
    let idet = 1.0 / det;
    for x in inv.iter_mut() {
        *x *= idet;
    }
    inv
}

fn bone_name(h: u32) -> String {
    RAINBOW.with(|r| r.get(&h).cloned().unwrap_or_else(|| format!("bone_{h:08X}")))
}

thread_local! {
    static RAINBOW: HashMap<u32, String> = load_rainbow();
}
fn load_rainbow() -> HashMap<u32, String> {
    // Resolve bone-name hashes to the recovered names (repo rainbow table), so the skeleton exports
    // with real joint names. Best-effort: an unresolved bone falls back to its hex hash.
    let mut map = HashMap::new();
    for root in ["tools/rainbow_table.json", "../../tools/rainbow_table.json", "../../../tools/rainbow_table.json"] {
        if let Ok(txt) = std::fs::read_to_string(root) {
            if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                if let Some(o) = v.get("pandemic_hash_m2").and_then(|x| x.as_object()) {
                    for (k, names) in o {
                        if let (Ok(h), Some(n)) = (
                            u32::from_str_radix(k.trim_start_matches("0x"), 16),
                            names.get(0).and_then(|x| x.as_str()),
                        ) {
                            let l = n.to_lowercase();
                            if l.starts_with("bone") || l.starts_with("hp_") || l.starts_with("eff")
                                || l == "globalsrt" || l.contains("srt") || l.starts_with("dummy")
                            {
                                map.insert(h, n.to_string());
                            }
                        }
                    }
                }
            }
            break;
        }
    }
    map
}

fn write_glb_file(path: &str, json_root: &Value, bin: &[u8]) {
    let mut json = serde_json::to_vec(json_root).unwrap();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let mut binpad = bin.to_vec();
    while binpad.len() % 4 != 0 {
        binpad.push(0);
    }
    let total = 12 + 8 + json.len() + 8 + binpad.len();
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).expect("create glb"));
    f.write_all(b"glTF").unwrap();
    f.write_all(&2u32.to_le_bytes()).unwrap();
    f.write_all(&(total as u32).to_le_bytes()).unwrap();
    f.write_all(&(json.len() as u32).to_le_bytes()).unwrap();
    f.write_all(b"JSON").unwrap();
    f.write_all(&json).unwrap();
    f.write_all(&(binpad.len() as u32).to_le_bytes()).unwrap();
    f.write_all(b"BIN\0").unwrap();
    f.write_all(&binpad).unwrap();
}
