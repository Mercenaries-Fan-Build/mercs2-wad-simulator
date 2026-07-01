//! Drive `model_inject` to build the pmc_hum_cesium model block from CesiumMan.glb
//! injected into the obama_faithful4 donor. Parses the GLB minimally (JSON + BIN
//! chunks, no external crate), bakes the bind-pose transform (Z_UP * Armature) so
//! the mesh stands Y-up feet-at-0, applies uniform scale, V-flips UVs, and emits
//! the model block. DO NOT deploy.
//!
//! Usage: inject_cesium <CesiumMan.glb> <donor.bin> <out_model.bin>

use mercs2_formats::ffcs::read_u32_le;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mannequin::BodyMap;
use mercs2_formats::model_inject::{
    inject_multi_into_donor_block, read_f16_le, ExternalMesh, MtrlRepoint,
};
use mercs2_formats::retarget::{build_retarget_table, quantise_weights, remap_joints};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

// Named mattias bone hashes (rainbow-table resolved) used to resolve the donor
// BodyMap by NAME (not hard-coded indices). Mirrors build_mannequin.rs.
const H_HIPS: u32 = 0x24C5009C; // Bone_Hips
const H_CHEST: u32 = 0x4C7733ED; // Bone_Chest
const H_HEAD: u32 = 0x705C4508; // Bone_Head
const H_LBICEP: u32 = 0xB2C9CE63; // Bone_LBicep
const H_RBICEP: u32 = 0x20F635D9; // Bone_RBicep
const H_LFOREARM: u32 = 0xBEFC09A2; // Bone_LForearm
const H_RFOREARM: u32 = 0x23F6F598; // Bone_RForearm
const H_LTHIGH: u32 = 0x76853D12; // Bone_LThigh
const H_RTHIGH: u32 = 0xC2299AC4; // Bone_RThigh
const H_LSHIN: u32 = 0xA76C9842; // Bone_LShin
const H_RSHIN: u32 = 0x0163705C; // Bone_RShin

// mattias_v2 measured model-space Y extent (NOT obama's 1.8343).
const DONOR_HEIGHT: f32 = 1.847;

// mattias_v2 host drawing groups (stride-40 / DECL64, skinned). The injected mesh
// is split across these two via the multi-group splitter (geometry HOST / capacity
// targets). These are the GEOMETRY-HOST set only — distinct from the weight
// SAMPLE set below (M2 conflated the two, which caused the A-pose ghost).
const TARGET_GROUPS: [usize; 2] = [2, 6];

// cesium_skin diffuse hash (single material -> one texture).
const CESIUM_SKIN: u32 = 0xdd4d410d;
// mattias_v2 donor diffuse hashes (grp2 + grp6 materials) -> cesium_skin.
// grp2 -> material 2 diffuse 0xf66b8f19 (head/face); grp6 -> material 6 diffuse
// 0x63c031b5 (torso/arms). Each occurs twice (LOD0 3-tex + LOD1 1-tex twin), both
// the same logical material — global value-scan repoint hits only these, no clobber.
const MATTIAS_DIFFUSE: [u32; 2] = [0xf66b8f19, 0x63c031b5];

fn read_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// 4x4 row-major matrix from a gltf column-major 16-array.
fn mat_from(m: &[f64]) -> [[f64; 4]; 4] {
    let mut r = [[0.0f64; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            r[i][j] = m[j * 4 + i];
        }
    }
    r
}
fn matmul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut r = [[0.0f64; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                r[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    r
}
fn xform(m: &[[f64; 4]; 4], p: [f32; 3], w: f64) -> [f32; 3] {
    let mut o = [0.0f64; 3];
    for i in 0..3 {
        o[i] = m[i][0] * p[0] as f64 + m[i][1] * p[1] as f64 + m[i][2] * p[2] as f64 + m[i][3] * w;
    }
    [o[0] as f32, o[1] as f32, o[2] as f32]
}

// ------- minimal JSON (just enough: objects, arrays, numbers, strings, bool) ----
// We avoid a JSON crate; the glTF JSON is well-formed and small.
#[derive(Debug, Clone)]
#[allow(dead_code)]
enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(HashMap<String, J>),
}
impl J {
    fn obj(&self) -> &HashMap<String, J> {
        match self {
            J::Obj(m) => m,
            _ => panic!("not obj"),
        }
    }
    fn arr(&self) -> &Vec<J> {
        match self {
            J::Arr(a) => a,
            _ => panic!("not arr"),
        }
    }
    fn num(&self) -> f64 {
        match self {
            J::Num(n) => *n,
            _ => panic!("not num"),
        }
    }
    fn get(&self, k: &str) -> Option<&J> {
        self.obj().get(k)
    }
}

struct P<'a> {
    b: &'a [u8],
    i: usize,
}
impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.b.len() && (self.b[self.i] as char).is_whitespace() {
            self.i += 1;
        }
    }
    fn val(&mut self) -> J {
        self.ws();
        match self.b[self.i] {
            b'{' => self.obj(),
            b'[' => self.arr(),
            b'"' => J::Str(self.string()),
            b't' => {
                self.i += 4;
                J::Bool(true)
            }
            b'f' => {
                self.i += 5;
                J::Bool(false)
            }
            b'n' => {
                self.i += 4;
                J::Null
            }
            _ => self.num(),
        }
    }
    fn obj(&mut self) -> J {
        let mut m = HashMap::new();
        self.i += 1;
        loop {
            self.ws();
            if self.b[self.i] == b'}' {
                self.i += 1;
                break;
            }
            let k = self.string();
            self.ws();
            self.i += 1; // ':'
            let v = self.val();
            m.insert(k, v);
            self.ws();
            if self.b[self.i] == b',' {
                self.i += 1;
            }
        }
        J::Obj(m)
    }
    fn arr(&mut self) -> J {
        let mut a = Vec::new();
        self.i += 1;
        loop {
            self.ws();
            if self.b[self.i] == b']' {
                self.i += 1;
                break;
            }
            a.push(self.val());
            self.ws();
            if self.b[self.i] == b',' {
                self.i += 1;
            }
        }
        J::Arr(a)
    }
    fn string(&mut self) -> String {
        self.i += 1; // opening quote
        let mut s = String::new();
        while self.b[self.i] != b'"' {
            if self.b[self.i] == b'\\' {
                self.i += 1;
                let c = self.b[self.i];
                s.push(match c {
                    b'n' => '\n',
                    b't' => '\t',
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    _ => c as char,
                });
            } else {
                s.push(self.b[self.i] as char);
            }
            self.i += 1;
        }
        self.i += 1;
        s
    }
    fn num(&mut self) -> J {
        let start = self.i;
        while self.i < self.b.len() {
            let c = self.b[self.i];
            if c == b'-' || c == b'+' || c == b'.' || c == b'e' || c == b'E' || c.is_ascii_digit() {
                self.i += 1;
            } else {
                break;
            }
        }
        let s = std::str::from_utf8(&self.b[start..self.i]).unwrap();
        J::Num(s.parse().unwrap())
    }
}

/// Resolve the donor mattias `BodyMap` from its skeleton by name-hash + hierarchy
/// (no hard-coded indices). Mirrors `build_mannequin.rs` so the retarget targets
/// the SAME anatomically-verified bones the procedural mannequin used.
fn resolve_body_map(donor_block: &[u8]) -> BodyMap {
    let skel = Skeleton::from_block(donor_block).expect("extract donor skeleton");
    let h = skel.height();
    eprintln!("=== donor skeleton: {} bones, bind height {h:.4} ===", skel.bones.len());
    let r = |hash: u32, label: &str| {
        skel.by_hash(hash)
            .unwrap_or_else(|| panic!("bone {label} ({hash:#010x}) not found"))
    };
    let pelvis = r(H_HIPS, "pelvis");
    let chest = r(H_CHEST, "chest");
    let head = r(H_HEAD, "head");
    let upperarm_l = r(H_LBICEP, "upperarm_l");
    let upperarm_r = r(H_RBICEP, "upperarm_r");
    let forearm_l = r(H_LFOREARM, "forearm_l");
    let forearm_r = r(H_RFOREARM, "forearm_r");
    let thigh_l = r(H_LTHIGH, "thigh_l");
    let thigh_r = r(H_RTHIGH, "thigh_r");
    let shin_l = r(H_LSHIN, "shin_l");
    let shin_r = r(H_RSHIN, "shin_r");
    // hierarchy-derived (parent of head = neck; parent of bicep = clavicle;
    // forearm's farthest child = wrist/hand; shin's child = foot).
    let neck = skel.bones[head].parent as usize;
    let clav_l = skel.bones[upperarm_l].parent as usize;
    let clav_r = skel.bones[upperarm_r].parent as usize;
    let first_child = |b: usize| skel.bones.iter().position(|x| x.parent == b as i32);
    let wrist_of = |forearm: usize| -> usize {
        let elbow = skel.bones[forearm].world_pos();
        let mut best: Option<(usize, f32)> = None;
        for bb in &skel.bones {
            if bb.parent == forearm as i32 {
                let p = bb.world_pos();
                let dd = (p[0] - elbow[0]).powi(2)
                    + (p[1] - elbow[1]).powi(2)
                    + (p[2] - elbow[2]).powi(2);
                if best.map_or(true, |(_, bd)| dd > bd) {
                    best = Some((bb.index, dd));
                }
            }
        }
        best.map(|(i, _)| i).unwrap_or(forearm)
    };
    let hand_l = wrist_of(forearm_l);
    let hand_r = wrist_of(forearm_r);
    let foot_l = first_child(shin_l).expect("left foot");
    let foot_r = first_child(shin_r).expect("right foot");
    BodyMap {
        pelvis, chest, neck, head,
        clav_l, clav_r,
        upperarm_l, upperarm_r,
        forearm_l, forearm_r,
        hand_l, hand_r,
        thigh_l, thigh_r,
        shin_l, shin_r,
        foot_l, foot_r,
    }
}

/// Compute every glb node's WORLD translation by composing node-local transforms
/// down the scene hierarchy. Supports `matrix` nodes and TRS (translation /
/// rotation-quaternion / scale) nodes. Returns one [x,y,z] per node index.
fn compute_node_worlds(j: &J) -> Vec<[f32; 3]> {
    let nodes = j.get("nodes").unwrap().arr();
    let n = nodes.len();
    // node-local 4x4 (row-major, row-vector convention to match xform()).
    let local: Vec<[[f64; 4]; 4]> = (0..n).map(|i| node_local_matrix(&nodes[i])).collect();
    // parent map
    let mut parent = vec![usize::MAX; n];
    for (i, nd) in nodes.iter().enumerate() {
        if let Some(J::Arr(ch)) = nd.get("children") {
            for c in ch {
                parent[c.num() as usize] = i;
            }
        }
    }
    // world = local @ parent_world (column-vector chain via repeated matmul up).
    let mut world: Vec<[[f64; 4]; 4]> = vec![[[0.0; 4]; 4]; n];
    // resolve in an order that guarantees parents first: iterate until stable.
    let mut done = vec![false; n];
    let mut remaining = n;
    while remaining > 0 {
        let mut progressed = false;
        for i in 0..n {
            if done[i] {
                continue;
            }
            let p = parent[i];
            if p == usize::MAX {
                world[i] = local[i];
                done[i] = true;
                remaining -= 1;
                progressed = true;
            } else if done[p] {
                world[i] = matmul(&world[p], &local[i]);
                done[i] = true;
                remaining -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break; // cycle / orphan guard
        }
    }
    world
        .iter()
        .map(|m| [m[0][3] as f32, m[1][3] as f32, m[2][3] as f32])
        .collect()
}

/// Node-local 4x4 from either a `matrix` (column-major 16) or TRS components.
/// Row-major output (translation in column 3) consistent with `mat_from`/`matmul`.
fn node_local_matrix(node: &J) -> [[f64; 4]; 4] {
    if let Some(J::Arr(m)) = node.get("matrix") {
        let v: Vec<f64> = m.iter().map(|x| x.num()).collect();
        return mat_from(&v);
    }
    let mut t = [0.0f64; 3];
    if let Some(J::Arr(a)) = node.get("translation") {
        for k in 0..3 {
            t[k] = a[k].num();
        }
    }
    let mut q = [0.0f64, 0.0, 0.0, 1.0]; // x,y,z,w
    if let Some(J::Arr(a)) = node.get("rotation") {
        for k in 0..4 {
            q[k] = a[k].num();
        }
    }
    let mut s = [1.0f64; 3];
    if let Some(J::Arr(a)) = node.get("scale") {
        for k in 0..3 {
            s[k] = a[k].num();
        }
    }
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    // rotation matrix (row-major)
    let rot = [
        [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w), 0.0],
        [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w), 0.0],
        [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y), 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut m = [[0.0f64; 4]; 4];
    for i in 0..3 {
        for k in 0..3 {
            m[i][k] = rot[i][k] * s[k];
        }
        m[i][3] = t[i];
    }
    m[3][3] = 1.0;
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let glb_path = &args[1];
    let donor_path = &args[2];
    let out_path = &args[3];

    let d = std::fs::read(glb_path).expect("read glb");
    assert_eq!(&d[0..4], b"glTF");
    let jlen = read_u32_le(&d, 12) as usize;
    let jtyp = read_u32_le(&d, 16);
    assert_eq!(jtyp, 0x4e4f534a, "chunk0 not JSON");
    let json_bytes = &d[20..20 + jlen];
    let mut p = P { b: json_bytes, i: 0 };
    let j = p.val();
    let bin_off = 20 + jlen;
    let _blen = read_u32_le(&d, bin_off) as usize;
    let btyp = read_u32_le(&d, bin_off + 4);
    assert_eq!(btyp, 0x004e4942, "chunk1 not BIN");
    let bin0 = bin_off + 8;

    let bvs = j.get("bufferViews").unwrap().arr();
    let accs = j.get("accessors").unwrap().arr();

    // accessor reader: returns Vec<Vec<f64>> (components per element)
    let read_acc = |ai: usize| -> Vec<Vec<f64>> {
        let a = &accs[ai];
        let bv = &bvs[a.get("bufferView").unwrap().num() as usize];
        let start = bin0
            + bv.get("byteOffset").map(|x| x.num()).unwrap_or(0.0) as usize
            + a.get("byteOffset").map(|x| x.num()).unwrap_or(0.0) as usize;
        let comp = a.get("componentType").unwrap().num() as u32;
        let (csz, is_float, is_u16) = match comp {
            5126 => (4usize, true, false),
            5123 => (2usize, false, true),
            5125 => (4usize, false, false),
            _ => panic!("componentType {comp}"),
        };
        let ncomp = match a.get("type").unwrap() {
            J::Str(s) => match s.as_str() {
                "SCALAR" => 1,
                "VEC2" => 2,
                "VEC3" => 3,
                "VEC4" => 4,
                _ => panic!("type"),
            },
            _ => panic!(),
        };
        let count = a.get("count").unwrap().num() as usize;
        let stride = bv
            .get("byteStride")
            .map(|x| x.num() as usize)
            .unwrap_or(csz * ncomp);
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let base = start + i * stride;
            let mut v = Vec::with_capacity(ncomp);
            for c in 0..ncomp {
                let o = base + c * csz;
                let val = if is_float {
                    read_f32(&d, o) as f64
                } else if is_u16 {
                    u16::from_le_bytes([d[o], d[o + 1]]) as f64
                } else {
                    read_u32_le(&d, o) as f64
                };
                v.push(val);
            }
            out.push(v);
        }
        out
    };

    // single mesh, single primitive
    let mesh0 = &j.get("meshes").unwrap().arr()[0];
    let prim = &mesh0.get("primitives").unwrap().arr()[0];
    let attrs = prim.get("attributes").unwrap();
    let pos_ai = attrs.get("POSITION").unwrap().num() as usize;
    let nrm_ai = attrs.get("NORMAL").unwrap().num() as usize;
    let uv_ai = attrs.get("TEXCOORD_0").unwrap().num() as usize;
    let idx_ai = prim.get("indices").unwrap().num() as usize;

    let pos_raw = read_acc(pos_ai);
    let nrm_raw = read_acc(nrm_ai);
    let uv_raw = read_acc(uv_ai);
    let idx_raw = read_acc(idx_ai);

    // bake bind-pose transform: node0 (Z_UP) * node1 (Armature)
    let nodes = j.get("nodes").unwrap().arr();
    let m0 = mat_from(
        &nodes[0]
            .get("matrix")
            .unwrap()
            .arr()
            .iter()
            .map(|x| x.num())
            .collect::<Vec<_>>(),
    );
    let m1 = mat_from(
        &nodes[1]
            .get("matrix")
            .unwrap()
            .arr()
            .iter()
            .map(|x| x.num())
            .collect::<Vec<_>>(),
    );
    let m = matmul(&m0, &m1);

    // transform positions (point) and normals (vector, then renormalise)
    let mut positions: Vec<[f32; 3]> = pos_raw
        .iter()
        .map(|v| xform(&m, [v[0] as f32, v[1] as f32, v[2] as f32], 1.0))
        .collect();
    let mut normals: Vec<[f32; 3]> = nrm_raw
        .iter()
        .map(|v| {
            let r = xform(&m, [v[0] as f32, v[1] as f32, v[2] as f32], 0.0);
            let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-8);
            [r[0] / l, r[1] / l, r[2] / l]
        })
        .collect();
    // V-flip UVs (donor convention)
    let uvs: Vec<[f32; 2]> = uv_raw
        .iter()
        .map(|v| [v[0] as f32, 1.0 - v[1] as f32])
        .collect();

    // feet to Y=0, then uniform scale to donor height
    let (mut ymin, mut ymax) = (f32::INFINITY, f32::NEG_INFINITY);
    for p in &positions {
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let height = ymax - ymin;
    let scale = DONOR_HEIGHT / height;
    eprintln!(
        "CesiumMan baked: Y height={height:.4} -> uniform scale={scale:.6} (donor {DONOR_HEIGHT})"
    );
    for p in positions.iter_mut() {
        p[0] *= scale;
        p[1] = (p[1] - ymin) * scale;
        p[2] *= scale;
    }
    // normals are direction only: rotation already applied, do NOT apply position scale
    let _ = &mut normals;

    let tris: Vec<[u32; 3]> = idx_raw
        .chunks(3)
        .map(|c| [c[0][0] as u32, c[1][0] as u32, c[2][0] as u32])
        .collect();

    eprintln!(
        "verts={} tris={} (indices={})",
        positions.len(),
        tris.len(),
        idx_raw.len()
    );

    // ---- SKELETON RETARGET (CesiumMan's NATIVE 19-joint rig) ----------------
    // ABANDONS spatial NN. CesiumMan ships per-vertex JOINTS_0/WEIGHTS_0 against a
    // clean named 19-joint rig. NN discarded that and reconstructed anatomically-
    // scrambled weights from donor proximity (bone 5 owning head+torso+legs).
    // Here we HONOUR the foreign rig: map each cesium joint -> the anatomically-
    // correct mattias bone BY NAME (the 19->95 table), then every vertex KEEPS its
    // native coherent weights and only has its 4 joint indices remapped. The donor
    // uses DIRECT GLOBAL bone indexing, so the table values are global bone indices.

    // 1) read cesium's skin: joint node list + per-vertex JOINTS_0/WEIGHTS_0.
    let skin0 = &j.get("skins").unwrap().arr()[0];
    let joint_nodes: Vec<usize> = skin0
        .get("joints")
        .unwrap()
        .arr()
        .iter()
        .map(|x| x.num() as usize)
        .collect();
    let joint_names: Vec<String> = joint_nodes
        .iter()
        .map(|&ni| match nodes[ni].get("name") {
            Some(J::Str(s)) => s.clone(),
            _ => format!("node{ni}"),
        })
        .collect();
    let j0_ai = attrs.get("JOINTS_0").unwrap().num() as usize;
    let w0_ai = attrs.get("WEIGHTS_0").unwrap().num() as usize;
    let j0_raw = read_acc(j0_ai); // VEC4 u16
    let w0_raw = read_acc(w0_ai); // VEC4 f32

    // 2) compute each cesium joint's WORLD position by chaining node-local
    //    transforms through the node-parent map (independent of the mesh bake;
    //    only RELATIVE distances drive the chain-rank, which is bake-invariant).
    let node_world = compute_node_worlds(&j);
    // torso-root = the lowest (smallest world-Y) joint whose name is torso-ish, the
    // pelvis reference. chain-rank(j) = distance from that root: torso joints rank
    // by spine height, limb joints by distance from the torso (shoulder<hand,
    // thigh<foot), which orders every chain root->tip regardless of glb joint order.
    let mut torso_root_y = f32::INFINITY;
    let mut torso_root = [0.0f32; 3];
    for (j, name) in joint_names.iter().enumerate() {
        let lc = name.to_lowercase();
        if lc.contains("torso") || lc.contains("spine") || lc.contains("pelvis") || lc.contains("hips") {
            let w = node_world[joint_nodes[j]];
            if w[1] < torso_root_y {
                torso_root_y = w[1];
                torso_root = w;
            }
        }
    }
    let jworld: Vec<[f32; 3]> = joint_nodes.iter().map(|&ni| node_world[ni]).collect();
    let chain_rank = |j: usize| -> f32 {
        let w = jworld[j];
        let dx = w[0] - torso_root[0];
        let dy = w[1] - torso_root[1];
        let dz = w[2] - torso_root[2];
        (dx * dx + dy * dy + dz * dz).sqrt()
    };

    // 3) resolve the donor BodyMap (by name-hash + hierarchy) then build the table.
    let donor_block = std::fs::read(donor_path).expect("read donor");
    let body_map = resolve_body_map(&donor_block);
    let (table, unclassified) = build_retarget_table(&joint_names, &body_map, &chain_rank);
    eprintln!("=== FOREIGN-RIG RETARGET: {} cesium joints -> mattias bones ===", joint_names.len());
    for (jj, name) in joint_names.iter().enumerate() {
        eprintln!("  j{jj:<2} {name:<30} -> mattias bone {}", table[jj]);
    }
    if !unclassified.is_empty() {
        eprintln!("  WARNING unclassified joints (defaulted to pelvis): {unclassified:?}");
    }

    // 4) per-vertex: remap the 4 JOINTS_0 indices through the table; quantise the
    //    native WEIGHTS_0 to u8x4 summing exactly to 255.
    let mut joints: Vec<[u8; 4]> = Vec::with_capacity(positions.len());
    let mut weights: Vec<[u8; 4]> = Vec::with_capacity(positions.len());
    for vi in 0..positions.len() {
        let j0 = [
            j0_raw[vi][0] as u16,
            j0_raw[vi][1] as u16,
            j0_raw[vi][2] as u16,
            j0_raw[vi][3] as u16,
        ];
        let w0 = [
            w0_raw[vi][0] as f32,
            w0_raw[vi][1] as f32,
            w0_raw[vi][2] as f32,
            w0_raw[vi][3] as f32,
        ];
        joints.push(remap_joints(j0, &table));
        weights.push(quantise_weights(w0));
    }
    // retarget report: distinct BLENDINDICES tuples + a small sample.
    {
        use std::collections::HashMap as HM;
        let mut hist: HM<[u8; 4], usize> = HM::new();
        for j in &joints {
            *hist.entry(*j).or_insert(0) += 1;
        }
        let mut top: Vec<_> = hist.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        eprintln!(
            "  retargeted {} distinct BLENDINDICES tuples across {} verts",
            hist.len(),
            joints.len()
        );
        for (k, c) in top.iter().take(8) {
            eprintln!("    idx {:?} x{}", k, c);
        }
        for &vi in &[0usize, positions.len() / 3, positions.len() / 2, positions.len() - 1] {
            let wsum: u32 = weights[vi].iter().map(|&w| w as u32).sum();
            eprintln!(
                "    sample vtx{}: idx={:?} wgt={:?} (sum={})",
                vi, joints[vi], weights[vi], wsum
            );
        }
    }

    let mesh = ExternalMesh {
        positions,
        normals,
        uvs,
        tris,
        joints,
        weights,
    };

    let cesium_skin = pandemic_hash_m2("cesium_skin");
    let pmc_hum_cesium = pandemic_hash_m2("pmc_hum_cesium");
    eprintln!("cesium_skin={cesium_skin:#010x} pmc_hum_cesium={pmc_hum_cesium:#010x}");

    let _ = cesium_skin; // CESIUM_SKIN const is the authoritative hash
    let repoints: Vec<MtrlRepoint> = MATTIAS_DIFFUSE
        .iter()
        .map(|&from| MtrlRepoint { from, to: CESIUM_SKIN })
        .collect();
    // M2: donor = mattias_v2; SPLIT across host drawing groups [2,6] (both
    // stride-40/decl-64 skinned) so every injected group is <= the donor original
    // on BOTH vertex and index count. Per-vertex BLENDINDICES/BLENDWEIGHT come
    // from the spatial NN transfer above (mattias's 95-bone GLOBAL skeleton).
    let (block, audits, stats) =
        inject_multi_into_donor_block(&donor_block, &mesh, &TARGET_GROUPS, &repoints, pmc_hum_cesium)
            .expect("inject multi");

    std::fs::write(out_path, &block).expect("write");
    eprintln!("=== inject stats (v2 multi-group split) ===");
    eprintln!(
        "total verts={} tris={}",
        stats.vertex_count, stats.triangle_count
    );
    eprintln!("=== PER-GROUP BUDGET AUDIT (injected vs donor ORIGINAL) ===");
    for a in &audits {
        let vok = a.injected_vc <= a.donor_vc;
        let iok = a.injected_ic <= a.donor_ic;
        eprintln!(
            "  grp{:>2}: vc {:>5}/{:<5} ({}<=)  ic {:>6}/{:<6} ({}<=)  tris={}",
            a.group,
            a.injected_vc,
            a.donor_vc,
            if vok { "OK " } else { "FAIL" },
            a.injected_ic,
            a.donor_ic,
            if iok { "OK " } else { "FAIL" },
            a.triangles
        );
        assert!(vok && iok, "group {} budget violated", a.group);
    }
    eprintln!("emptied (neutralised) groups: {:?}", stats.emptied_groups);
    for (f, t, c) in &stats.mtrl_repoints {
        eprintln!("  MTRL repoint {f:#010x} -> {t:#010x} x{c}");
    }
    eprintln!(
        "bbox min={:?} max={:?}",
        stats.bbox_min.map(|v| (v * 1000.0).round() / 1000.0),
        stats.bbox_max.map(|v| (v * 1000.0).round() / 1000.0)
    );
    eprintln!(
        "avg normal len={:.4} avg tangent len={:.4}",
        stats.avg_normal_len, stats.avg_tangent_len
    );
    eprintln!("wrote {out_path}: {} bytes", block.len());

    // quick re-parse: verify quantised normals/tangents in the emitted STRM
    verify_emitted(&block);
}

/// Re-parse the emitted block and, for each DRAWING group only (the injected
/// ones — neutralised donor groups keep their original bone indices and must NOT
/// be asserted), decode vertex 0's layout (sanity #3) and the quantised
/// normal/tangent lengths (v2-darkness check). Walks PRMG groups so it can tell
/// injected (draws) from neutralised (PRMT count 0).
fn verify_emitted(block: &[u8]) {
    let ulen = read_u32_le(block, 16) as usize;
    let ucfx = &block[20..20 + ulen];
    let data_off = read_u32_le(ucfx, 4) as usize;
    let ndesc = read_u32_le(ucfx, 16) as usize;

    // collect PRMG marker rows
    let prmg: Vec<usize> = (0..ndesc)
        .filter(|&i| {
            let ro = 20 + i * 20;
            &ucfx[ro..ro + 4] == b"PRMG" && read_u32_le(ucfx, ro + 4) == 0xFFFF_FFFF
        })
        .collect();

    let leaf_at = |i: usize| -> (usize, usize) {
        let ro = 20 + i * 20;
        (
            data_off + read_u32_le(ucfx, ro + 4) as usize,
            read_u32_le(ucfx, ro + 8) as usize,
        )
    };

    for (gi, &pr) in prmg.iter().enumerate() {
        let nxt = if gi + 1 < prmg.len() { prmg[gi + 1] } else { ndesc };
        let mut state = 0u8;
        let (mut strm_data, mut strm_stride, mut strm_n) = (None, 0usize, 0usize);
        let mut prmt: Option<usize> = None;
        for i in (pr + 1)..nxt {
            let ro = 20 + i * 20;
            let tag = &ucfx[ro..ro + 4];
            let cm = read_u32_le(ucfx, ro + 4) == 0xFFFF_FFFF;
            if tag == b"STRM" && cm {
                state = 1;
            } else if cm {
                state = 0;
            } else if state == 1 && tag == b"info" {
                let (o, _) = leaf_at(i);
                strm_stride = read_u32_le(ucfx, o + 4) as usize;
                strm_n = read_u32_le(ucfx, o + 8) as usize;
            } else if state == 1 && tag == b"data" {
                strm_data = Some(leaf_at(i));
            } else if tag == b"PRMT" && !cm {
                prmt = Some(i);
            }
        }
        // does this group draw? (any PRMT rec count > 0)
        let draws = prmt.map_or(false, |p| {
            let (o, sz) = leaf_at(p);
            (0..sz / 16).any(|r| read_u32_le(ucfx, o + r * 16 + 8) > 0)
        });
        if !draws || strm_stride != 40 {
            continue;
        }
        let Some((s, dsz)) = strm_data else { continue };
        let n = strm_n.min(dsz / 40);
        if n == 0 {
            continue;
        }
        let mut nl = 0.0f64;
        let mut tl = 0.0f64;
        // M2: confirm the injected STRM carries NON-uniform (transferred) skin
        // weights, not the rigid bone-0 fallback. Track distinct BLENDINDICES.
        use std::collections::HashSet;
        let mut distinct_idx: HashSet<[u8; 4]> = HashSet::new();
        let mut all_bone0 = true;
        for v in 0..n {
            let o = s + v * 40;
            nl += ((read_f16_le(ucfx, o + 24).powi(2)
                + read_f16_le(ucfx, o + 26).powi(2)
                + read_f16_le(ucfx, o + 28).powi(2)) as f64)
                .sqrt();
            tl += ((read_f16_le(ucfx, o + 32).powi(2)
                + read_f16_le(ucfx, o + 34).powi(2)
                + read_f16_le(ucfx, o + 36).powi(2)) as f64)
                .sqrt();
            let bi = [ucfx[o + 16], ucfx[o + 17], ucfx[o + 18], ucfx[o + 19]];
            distinct_idx.insert(bi);
            if bi != [0, 0, 0, 0] {
                all_bone0 = false;
            }
        }
        // vertex-0 layout decode (sanity #3) on an INJECTED group
        let pos_w = u16::from_le_bytes([ucfx[s + 6], ucfx[s + 7]]);
        let color = read_u32_le(ucfx, s + 12);
        let blendidx = [ucfx[s + 16], ucfx[s + 17], ucfx[s + 18], ucfx[s + 19]];
        let blendwgt = [ucfx[s + 20], ucfx[s + 21], ucfx[s + 22], ucfx[s + 23]];
        let wsum: u32 = blendwgt.iter().map(|&w| w as u32).sum();
        let nrm_w = u16::from_le_bytes([ucfx[s + 30], ucfx[s + 31]]);
        eprintln!(
            "  INJECTED grp(prmg#{gi}) {n} verts: vtx0 POS.w={pos_w:#06x}(want 3c00) \
             COLOR={color:#010x} BLENDIDX={blendidx:02x?} BLENDWGT={blendwgt:02x?}(sum={wsum}) \
             NRM.w={nrm_w:#06x}  distinct_idx={} all_bone0={all_bone0}  \
             normal avg={:.4} tangent avg={:.4}",
            distinct_idx.len(),
            nl / n as f64,
            tl / n as f64
        );
        assert_eq!(pos_w, 0x3c00, "POS.w must be 1.0 (0x3c00)");
        assert!(
            !all_bone0 && distinct_idx.len() > 1,
            "injected group must carry NON-uniform transferred BLENDINDICES (got all_bone0={all_bone0}, distinct={})",
            distinct_idx.len()
        );
    }
}
