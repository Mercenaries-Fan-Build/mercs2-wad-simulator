//! Minimal GLB (binary glTF 2.0) reader — the glTF adapter for the FAITHFUL character
//! pipeline. [`load_char_glb`] lifts a rigged .glb into `mercs2_formats::char_skin`'s
//! [`CharGlbData`]: raw per-vertex POSITION/JOINTS_0/WEIGHTS_0, the full node graph, per-node
//! ROW-MAJOR world matrices and per-joint inverse-bind matrices.
//!
//! Deliberately self-contained (serde_json for the JSON chunk, hand-decoded accessors)
//! rather than reusing the workshop importer, which pulls in the whole engine/render stack.
//!
//! Scope: the accessor/component-type subset Blender's glTF exporter emits for a Mixamo
//! character — POSITION/NORMAL (VEC3 f32), TEXCOORD_0 (VEC2 f32), JOINTS_0 (VEC4 u8/u16),
//! WEIGHTS_0 (VEC4 f32/normalized u8), inverse-bind matrices (MAT4), indices (u16/u32).

use mercs2_formats::char_skin::CharGlbData;
use serde_json::Value;

/// Split a .glb container into its JSON chunk (parsed) and the binary buffer chunk.
fn split_glb(bytes: &[u8]) -> Result<(Value, Vec<u8>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"glTF" {
        return Err("not a GLB (bad magic)".into());
    }
    let ver = u32le(bytes, 4);
    if ver != 2 {
        return Err(format!("unsupported GLB version {ver} (need 2)"));
    }
    let mut json: Option<Value> = None;
    let mut bin: Option<Vec<u8>> = None;
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let clen = u32le(bytes, off) as usize;
        let ctype = u32le(bytes, off + 4);
        let start = off + 8;
        let end = start + clen;
        if end > bytes.len() {
            return Err("GLB chunk overruns file".into());
        }
        match ctype {
            0x4E4F_534A => {
                // "JSON"
                json = Some(
                    serde_json::from_slice(&bytes[start..end])
                        .map_err(|e| format!("glTF JSON parse: {e}"))?,
                );
            }
            0x004E_4942 => {
                // "BIN\0"
                bin = Some(bytes[start..end].to_vec());
            }
            _ => {}
        }
        off = end;
    }
    let json = json.ok_or("GLB has no JSON chunk")?;
    let bin = bin.ok_or("GLB has no BIN chunk (external buffers unsupported)")?;
    Ok((json, bin))
}

fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Raw bytes backing an accessor, plus the stride to walk it.
struct AccessorView<'a> {
    data: &'a [u8],
    count: usize,
    comp_type: u64,
    byte_stride: usize,
}

fn type_comps(t: &str) -> usize {
    match t {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        _ => 0,
    }
}

fn comp_size(ct: u64) -> usize {
    match ct {
        5120 | 5121 => 1, // byte / ubyte
        5122 | 5123 => 2, // short / ushort
        5125 => 4,        // uint
        5126 => 4,        // float
        _ => 0,
    }
}

fn accessor<'a>(json: &Value, bin: &'a [u8], idx: usize) -> Result<AccessorView<'a>, String> {
    let acc = &json["accessors"][idx];
    let bv_idx = acc["bufferView"]
        .as_u64()
        .ok_or("accessor without bufferView")? as usize;
    let count = acc["count"].as_u64().ok_or("accessor without count")? as usize;
    let comp_type = acc["componentType"].as_u64().ok_or("no componentType")?;
    let comps = type_comps(acc["type"].as_str().ok_or("no accessor type")?);
    let bv = &json["bufferViews"][bv_idx];
    let bv_off = bv["byteOffset"].as_u64().unwrap_or(0) as usize;
    let acc_off = acc["byteOffset"].as_u64().unwrap_or(0) as usize;
    let elem = comps * comp_size(comp_type);
    let stride = bv["byteStride"].as_u64().map(|s| s as usize).unwrap_or(elem);
    let start = bv_off + acc_off;
    Ok(AccessorView {
        data: &bin[start..],
        count,
        comp_type,
        byte_stride: stride,
    })
}

/// Read a float component from an accessor element, honoring normalized int types.
fn read_comp_f32(v: &AccessorView, elem: usize, c: usize) -> f32 {
    let base = elem * v.byte_stride + c * comp_size(v.comp_type);
    let d = v.data;
    match v.comp_type {
        5126 => f32::from_le_bytes([d[base], d[base + 1], d[base + 2], d[base + 3]]),
        5121 => d[base] as f32 / 255.0,                       // normalized ubyte
        5123 => u16::from_le_bytes([d[base], d[base + 1]]) as f32 / 65535.0, // normalized ushort
        _ => 0.0,
    }
}

fn read_comp_u16(v: &AccessorView, elem: usize, c: usize) -> u16 {
    let base = elem * v.byte_stride + c * comp_size(v.comp_type);
    let d = v.data;
    match v.comp_type {
        5121 => d[base] as u16,
        5123 => u16::from_le_bytes([d[base], d[base + 1]]),
        5125 => u32::from_le_bytes([d[base], d[base + 1], d[base + 2], d[base + 3]]) as u16,
        _ => 0,
    }
}

fn read_index(v: &AccessorView, i: usize) -> u32 {
    let base = i * v.byte_stride;
    let d = v.data;
    match v.comp_type {
        5121 => d[base] as u32,
        5123 => u16::from_le_bytes([d[base], d[base + 1]]) as u32,
        5125 => u32::from_le_bytes([d[base], d[base + 1], d[base + 2], d[base + 3]]),
        _ => 0,
    }
}

/// 4x4 * 4x4 (column-vector convention: point' = M * point).
fn mat4_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
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

/// A node's local transform: an explicit `matrix` (glTF is column-major) or TRS.
fn node_local(node: &Value) -> [[f64; 4]; 4] {
    if let Some(mat) = node["matrix"].as_array() {
        // column-major 16-array -> [row][col]
        let m: Vec<f64> = mat.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect();
        let mut r = [[0.0f64; 4]; 4];
        for c in 0..4 {
            for row in 0..4 {
                r[row][c] = m[c * 4 + row];
            }
        }
        return r;
    }
    let t = node["translation"].as_array();
    let rot = node["rotation"].as_array();
    let s = node["scale"].as_array();
    let tv = |a: Option<&Vec<Value>>, i: usize, d: f64| {
        a.and_then(|v| v.get(i)).and_then(|x| x.as_f64()).unwrap_or(d)
    };
    let (tx, ty, tz) = (tv(t, 0, 0.0), tv(t, 1, 0.0), tv(t, 2, 0.0));
    let (qx, qy, qz, qw) = (tv(rot, 0, 0.0), tv(rot, 1, 0.0), tv(rot, 2, 0.0), tv(rot, 3, 1.0));
    let (sx, sy, sz) = (tv(s, 0, 1.0), tv(s, 1, 1.0), tv(s, 2, 1.0));
    // R (from unit quat) * S, then place translation.
    let (x2, y2, z2) = (qx + qx, qy + qy, qz + qz);
    let (xx, yy, zz) = (qx * x2, qy * y2, qz * z2);
    let (xy, xz, yz) = (qx * y2, qx * z2, qy * z2);
    let (wx, wy, wz) = (qw * x2, qw * y2, qw * z2);
    let rmat = [
        [1.0 - (yy + zz), xy - wz, xz + wy],
        [xy + wz, 1.0 - (xx + zz), yz - wx],
        [xz - wy, yz + wx, 1.0 - (xx + yy)],
    ];
    [
        [rmat[0][0] * sx, rmat[0][1] * sy, rmat[0][2] * sz, tx],
        [rmat[1][0] * sx, rmat[1][1] * sy, rmat[1][2] * sz, ty],
        [rmat[2][0] * sx, rmat[2][1] * sy, rmat[2][2] * sz, tz],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// Full ROW-MAJOR world matrix of every node (`world = world_parent · local`).
fn compute_node_world_mats(json: &Value) -> Vec<[f64; 16]> {
    let nodes = json["nodes"].as_array().cloned().unwrap_or_default();
    let n = nodes.len();
    let local: Vec<[[f64; 4]; 4]> = (0..n).map(|i| node_local(&nodes[i])).collect();
    let mut parent = vec![usize::MAX; n];
    for (i, node) in nodes.iter().enumerate() {
        if let Some(ch) = node["children"].as_array() {
            for c in ch {
                if let Some(ci) = c.as_u64() {
                    parent[ci as usize] = i;
                }
            }
        }
    }
    let mut world = vec![[[0.0f64; 4]; 4]; n];
    let mut done = vec![false; n];
    fn resolve(
        i: usize,
        parent: &[usize],
        local: &[[[f64; 4]; 4]],
        world: &mut [[[f64; 4]; 4]],
        done: &mut [bool],
    ) {
        if done[i] {
            return;
        }
        let p = parent[i];
        if p == usize::MAX {
            world[i] = local[i];
        } else {
            resolve(p, parent, local, world, done);
            world[i] = mat4_mul(&world[p], &local[i]);
        }
        done[i] = true;
    }
    for i in 0..n {
        resolve(i, &parent, &local, &mut world, &mut done);
    }
    world
        .iter()
        .map(|m| {
            let mut f = [0.0f64; 16];
            for r in 0..4 {
                for c in 0..4 {
                    f[r * 4 + c] = m[r][c];
                }
            }
            f
        })
        .collect()
}

/// Load a rigged .glb into [`CharGlbData`] for the faithful skinning writer. Reads mesh 0 /
/// primitive 0 only, matching the mesher's single-group scope.
pub fn load_char_glb(path: &str) -> Result<CharGlbData, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    let (json, bin) = split_glb(&bytes)?;
    let nodes = json["nodes"].as_array().cloned().unwrap_or_default();
    let nn = nodes.len();

    // node graph
    let mut node_parent = vec![-1i32; nn];
    let mut node_children = vec![Vec::new(); nn];
    let mut node_name = vec![String::new(); nn];
    for (i, node) in nodes.iter().enumerate() {
        node_name[i] = node["name"].as_str().unwrap_or("").to_string();
        if let Some(ch) = node["children"].as_array() {
            for c in ch {
                let ci = c.as_u64().unwrap() as usize;
                node_parent[ci] = i as i32;
                node_children[i].push(ci);
            }
        }
    }
    let node_world = compute_node_world_mats(&json);

    // EVERY skin, unified. A file may ship more than one: the CoD/Valve "Roze" rip binds its body
    // to a 103-joint CoD rig (skin0) and its face+hair to an 11-joint ValveBiped rig (skin1).
    // JOINTS_0 is an index into the PRIMITIVE'S OWN skin, so reading skin0 and concatenating every
    // primitive silently rebinds skin1's head onto skin0's joints 0..10 (root/spine) — 35% of that
    // model, wrong, with no error. Build one joint list across all skins and remap per primitive.
    let skins = json["skins"].as_array().cloned().unwrap_or_default();
    if skins.is_empty() {
        return Err("glb has no skin — not a rigged model".into());
    }
    let mut joint_nodes: Vec<usize> = Vec::new();
    let mut node_to_joint: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    // per-skin: local joint index -> unified joint index
    let mut skin_local_to_unified: Vec<Vec<usize>> = Vec::with_capacity(skins.len());
    let mut ibm: Vec<Option<[f64; 16]>> = Vec::new();
    for skin in &skins {
        let locals: Vec<usize> = skin["joints"]
            .as_array()
            .ok_or("skin has no joints")?
            .iter()
            .map(|j| j.as_u64().unwrap() as usize)
            .collect();
        let ibm_acc = match skin["inverseBindMatrices"].as_u64() {
            Some(i) => Some(accessor(&json, &bin, i as usize)?),
            None => None,
        };
        let mut map = Vec::with_capacity(locals.len());
        for (local, &node) in locals.iter().enumerate() {
            let uni = *node_to_joint.entry(node).or_insert_with(|| {
                joint_nodes.push(node);
                ibm.push(None);
                joint_nodes.len() - 1
            });
            // glTF MAT4 is COLUMN-major; convert to ROW-major (rm[r*4+c] = raw[c*4+r]).
            // First skin to define a joint's inverse-bind wins; later skins sharing that node
            // keep it rather than overwrite.
            if ibm[uni].is_none() {
                if let Some(a) = &ibm_acc {
                    let mut rm = [0.0f64; 16];
                    for c in 0..4 {
                        for r in 0..4 {
                            rm[r * 4 + c] = read_comp_f32(a, local, c * 4 + r) as f64;
                        }
                    }
                    ibm[uni] = Some(rm);
                }
            }
            map.push(uni);
        }
        skin_local_to_unified.push(map);
    }

    // mesh index -> the skin its node uses (a mesh drawn by a skinned node). Meshes with no
    // skinned node fall back to skin 0, matching the previous single-skin behaviour.
    let mut mesh_skin: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for node in &nodes {
        if let (Some(m), Some(s)) = (node["mesh"].as_u64(), node["skin"].as_u64()) {
            mesh_skin.insert(m as usize, s as usize);
        }
    }

    // Merge ALL meshes / primitives into one skinned stream — a character like 50 Cent ships its body,
    // head and accessories as SEPARATE meshes; reading only mesh0 would drop the head. All primitives of
    // a single-skin file index the same joint palette, so the merge is a straight concat with an index
    // offset. Non-skinned primitives are skipped (nothing for char_skin to re-pose).
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut vjoints: Vec<[u16; 4]> = Vec::new();
    let mut vweights: Vec<[f64; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let empty = Vec::new();
    for (mesh_ix, mesh) in json["meshes"].as_array().unwrap_or(&empty).iter().enumerate() {
        let remap = &skin_local_to_unified[*mesh_skin.get(&mesh_ix).unwrap_or(&0)];
        for prim in mesh["primitives"].as_array().unwrap_or(&empty) {
            let attrs = &prim["attributes"];
            let (Some(pi), Some(ji), Some(wi)) = (
                attrs["POSITION"].as_u64(),
                attrs["JOINTS_0"].as_u64(),
                attrs["WEIGHTS_0"].as_u64(),
            ) else {
                continue; // unskinned / no positions → not a char_skin input
            };
            let base = positions.len() as u32;
            let pos = accessor(&json, &bin, pi as usize)?;
            let n = pos.count;
            let na = attrs["NORMAL"].as_u64().map(|i| accessor(&json, &bin, i as usize)).transpose()?;
            let ta = attrs["TEXCOORD_0"].as_u64().map(|i| accessor(&json, &bin, i as usize)).transpose()?;
            let ja = accessor(&json, &bin, ji as usize)?;
            let wa = accessor(&json, &bin, wi as usize)?;
            for e in 0..n {
                positions.push([
                    read_comp_f32(&pos, e, 0) as f64,
                    read_comp_f32(&pos, e, 1) as f64,
                    read_comp_f32(&pos, e, 2) as f64,
                ]);
                normals.push(match &na {
                    Some(a) => [read_comp_f32(a, e, 0), read_comp_f32(a, e, 1), read_comp_f32(a, e, 2)],
                    None => [0.0, 0.0, 1.0],
                });
                uvs.push(match &ta {
                    Some(a) => [read_comp_f32(a, e, 0), read_comp_f32(a, e, 1)],
                    None => [0.0, 0.0],
                });
                // JOINTS_0 is skin-local; lift it into the unified joint list.
                let lift = |k: usize| -> u16 {
                    let local = read_comp_u16(&ja, e, k) as usize;
                    remap.get(local).copied().unwrap_or(0) as u16
                };
                vjoints.push([lift(0), lift(1), lift(2), lift(3)]);
                vweights.push([
                    read_comp_f32(&wa, e, 0) as f64,
                    read_comp_f32(&wa, e, 1) as f64,
                    read_comp_f32(&wa, e, 2) as f64,
                    read_comp_f32(&wa, e, 3) as f64,
                ]);
            }
            match prim["indices"].as_u64() {
                Some(ii) => {
                    let ia = accessor(&json, &bin, ii as usize)?;
                    for i in 0..ia.count {
                        indices.push(base + read_index(&ia, i));
                    }
                }
                None => indices.extend(base..base + n as u32),
            }
        }
    }
    if positions.is_empty() {
        return Err("glb has no skinned mesh primitive".into());
    }
    let tris: Vec<[u32; 3]> = indices.chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect();

    Ok(CharGlbData {
        positions,
        normals,
        uvs,
        tris,
        indices,
        vjoints,
        vweights,
        joint_nodes,
        node_parent,
        node_name,
        node_children,
        node_world,
        ibm,
    })
}
