//! Author a UCFX **static model** container FROM SCRATCH — no donor.
//!
//! The modding deep-dive rated "author a model UCFX from scratch" the hardest,
//! unsolved tier. This module emits a complete, minimal **static (non-destructible)**
//! model container from first principles: header + the flattened descriptor tree
//! (`u2` = #siblings-after, `u3` = #children) + all chunk bodies + `CSUM` trailer.
//! Nothing existing is copied or overridden; the caller mints a NEW asset hash and
//! ships it via `smuggler --inject-extra 0x<hash>:19:<file>`.
//!
//! Full byte spec: `docs/ucfx_model_from_scratch.md`. A static prop deliberately
//! OMITS every destruction chunk (SEGM/PHY2/STAM/SWIT/NODE/STAT/CHDR/CEXE), so
//! there is no twin-PRMT state pair and none of the `0x00478E43` class of crash.
//!
//! Reuses the shared primitives (`to_strip`, `f16_le`) and `crc32_mercs2` rather
//! than reinventing them.

use crate::crc32::crc32_mercs2;
use crate::model_inject::{f16_le, to_strip};

/// The exact 32-byte stride-20 static vertex declaration (POSITION FLOAT16_4 @0,
/// TEXCOORD FLOAT16_2 @8, NORMAL FLOAT16_4 @12, END). This IS the format — every
/// stride-20 static model carries these bytes.
pub const DECL20: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, // POSITION FLOAT16_4 @0
    0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00, // TEXCOORD FLOAT16_2 @8
    0x00, 0x00, 0x0c, 0x00, 0x10, 0x00, 0x03, 0x00, // NORMAL   FLOAT16_4 @12
    0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, // END
];

/// Default material-preamble bytes (shader id + standard color/emissive/specular
/// float defaults). A format constant, not a donor's identity.
const MTRL_PREAMBLE: [u8; 104] = [
    0x85, 0x47, 0x16, 0x0a, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x41, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
];

/// A complete 128-byte MTRL material record (the standard 3-texture form the prop/
/// building shader expects): `flag=(3<<16)|0x80`, then diffuse/specular/normal
/// texture hashes, then the float props. The three default hashes are base-resident
/// so the material always binds; the caller patches the diffuse slot to the model's
/// own texture. Emitting fewer than 3 slots leaves the shader's spec/normal
/// unbound → the 0x00858DB8 null-deref crash.
const MTRL_REC_TMPL: [u8; 128] = [
    0x80, 0x00, 0x03, 0x00, 0x61, 0x46, 0xe1, 0x68, 0xb8, 0xab, 0x68, 0x25,
    0x5b, 0xb3, 0x6c, 0xd8, 0xfe, 0xe1, 0xef, 0xca, 0xfc, 0x61, 0x5d, 0x3e,
    0x54, 0x28, 0x2f, 0x15, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x41,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
];

/// The float-property tail of a material record (everything after the flag word +
/// texture hashes): standard 1.0 color / spec defaults. Format constant.
#[allow(dead_code)]
const MTRL_REC_PROPS: [u8; 112] = [
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x41, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// SKINNED material preamble (104 B): shader `0x406b230e` = the human-skin shader
/// (the static/building shader `0x0a164785` in `MTRL_PREAMBLE` does NOT skin and
/// null-derefs at material bind — the 0x00858DB8 crash on SetOutfit). Shared engine
/// shader, required for any skinned model.
const SKINNED_MTRL_PREAMBLE: [u8; 104] = [
    0x0e, 0x23, 0x6b, 0x40, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x99, 0x99, 0x99, 0x3f, 0x99, 0x99, 0x99, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x41,
    0x00, 0x00, 0x00, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
];

/// SKINNED material record (124 B): flag `(3<<16)|0x98` (skinned materials use 0x98,
/// not the static 0x80), then diffuse/specular/normal. Caller patches the diffuse.
const SKINNED_MTRL_REC: [u8; 128] = [
    0x98, 0x00, 0x03, 0x00, 0xb7, 0x70, 0xd5, 0x47, 0xca, 0x09, 0x0f, 0x60,
    0x85, 0xca, 0xc5, 0xf3, 0x56, 0xcd, 0x2f, 0x32, 0x70, 0x84, 0x9d, 0x95,
    0xa0, 0xec, 0xb6, 0x19, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0xcd, 0xcc, 0x8c, 0x3f, 0xcd, 0xcc, 0x8c, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x0c, 0x42,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3f,
    0x00, 0x00, 0x80, 0x3f, 0x00, 0x00, 0x00, 0x00,
];

/// Build the SKINNED MTRL (human-skin shader + 3-texture skinned record). Diffuse
/// patched to the model's own texture; spec/normal keep resolvable defaults.
fn build_skinned_mtrl(diffuse_hash: u32) -> Vec<u8> {
    // ONE complete 128-byte material = preamble[104] + the 24-byte record head
    // (flags/count/3 hashes/trailing shader ref). Mtrl_Parse reads exactly 128 bytes
    // per material; emitting 104+128=232 is 1.8 materials and the parser reads the
    // second (garbage) one off the end → 0x00858DB8. All three texture slots point at
    // the caller's RESIDENT texture (the template spec/normal aren't resident).
    let mut b = Vec::with_capacity(128);
    b.extend_from_slice(&SKINNED_MTRL_PREAMBLE);
    let mut rec = SKINNED_MTRL_REC;
    rec[4..8].copy_from_slice(&diffuse_hash.to_le_bytes()); // diffuse
    rec[8..12].copy_from_slice(&diffuse_hash.to_le_bytes()); // specular
    rec[12..16].copy_from_slice(&diffuse_hash.to_le_bytes()); // normal
    b.extend_from_slice(&rec[..24]); // record HEAD only → 104+24 = 128 total
    debug_assert_eq!(b.len(), 128);
    b
}

/// Input: an owned triangle mesh in engine space (Y-up, metres). Positions/normals/
/// uvs are parallel arrays; `tris` indexes them.
#[derive(Debug, Clone, Default)]
pub struct StaticMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tris: Vec<[u32; 3]>,
}

/// One node in the chunk tree.
enum Node {
    Leaf { tag: [u8; 4], body: Vec<u8> },
    Cont { tag: [u8; 4], children: Vec<Node> },
}

struct Row {
    tag: [u8; 4],
    u0: u32,
    size: u32,
    u2: u32,
    u3: u32,
}

/// Flatten a sibling list depth-first, assigning each direct child's `u2`
/// (#siblings after it) and appending leaf bodies (16-byte aligned) to `data`.
fn flatten(children: &[Node], rows: &mut Vec<Row>, data: &mut Vec<u8>) {
    let n = children.len();
    for (i, child) in children.iter().enumerate() {
        let u2 = (n - 1 - i) as u32;
        match child {
            Node::Leaf { tag, body } => {
                while data.len() % 16 != 0 {
                    data.push(0);
                }
                let u0 = data.len() as u32;
                data.extend_from_slice(body);
                rows.push(Row { tag: *tag, u0, size: body.len() as u32, u2, u3: 0 });
            }
            Node::Cont { tag, children: kids } => {
                let idx = rows.len();
                rows.push(Row { tag: *tag, u0: 0xFFFF_FFFF, size: 0, u2, u3: kids.len() as u32 });
                flatten(kids, rows, data);
                // (u3 already set from kids.len(); u2 set above)
                let _ = idx;
            }
        }
    }
}

fn leaf(tag: &[u8; 4], body: Vec<u8>) -> Node {
    Node::Leaf { tag: *tag, body }
}
fn cont(tag: &[u8; 4], children: Vec<Node>) -> Node {
    Node::Cont { tag: *tag, children }
}

fn u32b(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}
fn f32b(v: f32) -> [u8; 4] {
    v.to_le_bytes()
}

/// Build the INFO (72 B) model header: flags + bbox + LOD/param defaults.
fn build_info(bmin: [f32; 3], bmax: [f32; 3]) -> Vec<u8> {
    let mut b = Vec::with_capacity(72);
    b.extend_from_slice(&u32b(0x39)); // flags
    for v in bmin {
        b.extend_from_slice(&f32b(v));
    }
    for v in bmax {
        b.extend_from_slice(&f32b(v));
    }
    // trailing param defaults (LOD table sizes / counts, verbatim constants)
    for w in [0x2b0u32, 4, 0x12, 4, 4, 0, 3] {
        b.extend_from_slice(&u32b(w));
    }
    b.extend_from_slice(&f32b(100.0)); // LOD dist
    b.extend_from_slice(&f32b(5.0));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(0x0004_0003));
    debug_assert_eq!(b.len(), 72);
    b
}

/// Build a single root HIER node (88 B): node hash = model hash, flags 0x10001,
/// parent -1, identity transform.
fn build_hier_root(model_hash: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(88);
    b.extend_from_slice(&u32b(model_hash));
    b.extend_from_slice(&u32b(0x0001_0001)); // flags
    b.extend_from_slice(&u32b(0xFFFF_FFFF)); // parent = root
    // 19 transform floats (identity 4x4-ish + 2 tail), matching the observed root.
    let xf: [f32; 19] = [
        0.0, 1.0, 0.0, -0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0,
        0.0,
    ];
    for v in xf {
        b.extend_from_slice(&f32b(v));
    }
    // wait: 3 u32 (12B) + 19 f32 (76B) = 88B
    debug_assert_eq!(b.len(), 88);
    b
}

/// Build a 1-material MTRL: 104 B preamble + one **3-texture** record (diffuse +
/// base-resident specular/normal defaults), the shape the standard shader expects.
/// The caller's `diffuse_hash` is patched into the diffuse slot; spec/normal keep
/// the resolvable defaults until the model ships its own maps.
fn build_mtrl(diffuse_hash: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(104 + 128);
    // ONE complete 128-byte material (preamble 104 + 24-byte record head). See
    // build_skinned_mtrl: Mtrl_Parse reads 128 bytes/material, so 104+128=232 is 1.8
    // materials → the parser reads a garbage second material off the end → 0x00858DB8.
    b.extend_from_slice(&MTRL_PREAMBLE);
    let mut rec = MTRL_REC_TMPL;
    rec[4..8].copy_from_slice(&diffuse_hash.to_le_bytes()); // diffuse
    rec[8..12].copy_from_slice(&diffuse_hash.to_le_bytes()); // specular
    rec[12..16].copy_from_slice(&diffuse_hash.to_le_bytes()); // normal
    b.extend_from_slice(&rec[..24]); // record HEAD only → 104+24 = 128 total
    debug_assert_eq!(b.len(), 128);
    b
}

/// Build the 60 B PRMG INFO: `1,1,0, group_hash, pad` then bounds@20
/// (center[3], radius, min[3], max[3]).
fn build_prmg_info(bmin: [f32; 3], bmax: [f32; 3], group_hash: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(60);
    b.extend_from_slice(&u32b(1));
    b.extend_from_slice(&u32b(1));
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(group_hash));
    b.extend_from_slice(&f32b(0.0)); // pad float @16
    let center = [
        (bmin[0] + bmax[0]) * 0.5,
        (bmin[1] + bmax[1]) * 0.5,
        (bmin[2] + bmax[2]) * 0.5,
    ];
    let radius = {
        let dx = (bmax[0] - bmin[0]) * 0.5;
        let dy = (bmax[1] - bmin[1]) * 0.5;
        let dz = (bmax[2] - bmin[2]) * 0.5;
        (dx * dx + dy * dy + dz * dz).sqrt()
    };
    for v in center {
        b.extend_from_slice(&f32b(v));
    }
    b.extend_from_slice(&f32b(radius));
    for v in bmin {
        b.extend_from_slice(&f32b(v));
    }
    for v in bmax {
        b.extend_from_slice(&f32b(v));
    }
    debug_assert_eq!(b.len(), 60);
    b
}

/// The 64-byte skinned vertex declaration (DECL64, stride-40): POSITION f16x4@0,
/// TEXCOORD f16x2@8, COLOR bgra8@12, BLENDINDICES u8x4@16, BLENDWEIGHT u8x4n@20,
/// NORMAL f16x4@24, TANGENT f16x4@32. Verbatim from the reversed skinned format.
pub const DECL40: [u8; 64] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, // POSITION  FLOAT16_4 @0
    0x00, 0x00, 0x08, 0x00, 0x0f, 0x00, 0x05, 0x00, // TEXCOORD0 FLOAT16_2 @8
    0x00, 0x00, 0x0c, 0x00, 0x04, 0x00, 0x0a, 0x00, // COLOR     D3DCOLOR  @12
    0x00, 0x00, 0x10, 0x00, 0x05, 0x00, 0x02, 0x00, // BLENDIDX  UBYTE4    @16
    0x00, 0x00, 0x14, 0x00, 0x08, 0x00, 0x01, 0x00, // BLENDWGT  UBYTE4N   @20
    0x00, 0x00, 0x18, 0x00, 0x10, 0x00, 0x03, 0x00, // NORMAL    FLOAT16_4 @24
    0x00, 0x00, 0x20, 0x00, 0x10, 0x00, 0x06, 0x00, // TANGENT   FLOAT16_4 @32
    0xff, 0x00, 0x00, 0x00, 0x11, 0x00, 0x00, 0x00, // END
];

/// f16 tangent synthesised from the normal (unit, perpendicular). Rigid A-pose
/// doesn't need accurate tangents, but the shader binds the slot.
fn synth_tan(n: [f32; 3]) -> [f32; 3] {
    // pick an axis not parallel to n, cross to get a perpendicular
    let a = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let t = [
        n[1] * a[2] - n[2] * a[1],
        n[2] * a[0] - n[0] * a[2],
        n[0] * a[1] - n[1] * a[0],
    ];
    let l = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2]).sqrt().max(1e-6);
    [t[0] / l, t[1] / l, t[2] / l]
}

/// Encode a stride-40 DECL64 vertex buffer, rigid to bone 0 (BLENDINDICES all 0,
/// BLENDWEIGHT 0xFF,0,0,0 = weight 1.0 to bone 0).
fn encode_strm40(mesh: &StaticMesh) -> Vec<u8> {
    let n = mesh.positions.len();
    let mut vb = Vec::with_capacity(n * 40);
    const ONE: [u8; 2] = [0x00, 0x3c]; // f16 1.0
    for i in 0..n {
        let p = mesh.positions[i];
        let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
        let nrm = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
        let tan = synth_tan(nrm);
        vb.extend_from_slice(&f16_le(p[0]));
        vb.extend_from_slice(&f16_le(p[1]));
        vb.extend_from_slice(&f16_le(p[2]));
        vb.extend_from_slice(&ONE); // pos.w = 1.0
        vb.extend_from_slice(&f16_le(uv[0]));
        vb.extend_from_slice(&f16_le(uv[1]));
        vb.extend_from_slice(&[0xff, 0xff, 0xff, 0xff]); // COLOR white
        vb.extend_from_slice(&[0, 0, 0, 0]); // BLENDINDICES -> bone 0
        vb.extend_from_slice(&[0xff, 0, 0, 0]); // BLENDWEIGHT -> 1.0 to bone 0
        vb.extend_from_slice(&f16_le(nrm[0]));
        vb.extend_from_slice(&f16_le(nrm[1]));
        vb.extend_from_slice(&f16_le(nrm[2]));
        vb.extend_from_slice(&ONE);
        vb.extend_from_slice(&f16_le(tan[0]));
        vb.extend_from_slice(&f16_le(tan[1]));
        vb.extend_from_slice(&f16_le(tan[2]));
        vb.extend_from_slice(&ONE);
    }
    vb
}

/// Build the 56-byte SKIN PRMG INFO = bone palette. For a rigid bone-0 group the
/// palette maps local index 0 -> global HIER bone 0. Structure mirrors the
/// reversed layout: counts, group hash, then a single-bone range table.
fn build_skin_palette_info(group_hash: u32) -> Vec<u8> {
    let mut b = Vec::with_capacity(56);
    b.extend_from_slice(&u32b(1)); // record count
    b.extend_from_slice(&u32b(1)); // sub count
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(group_hash)); // group id hash
    b.extend_from_slice(&u32b(0)); // (second hash slot; 0 = none)
    b.extend_from_slice(&u32b(1)); // palette entry count = 1 bone
    b.extend_from_slice(&[0x00, 0x00, 0x01, 0x00]); // range: 1 bone starting at 0
    b.extend_from_slice(&u32b(0)); // bone 0 (global HIER index)
    b.extend_from_slice(&u32b(0));
    b.extend_from_slice(&u32b(0));
    while b.len() < 56 {
        b.push(0);
    }
    b.truncate(56);
    b
}

/// Author a complete SKINNED UCFX model container from scratch (rigid bone-0
/// A-pose). Single SKIN group + a single root HIER bone; DECL64 stride-40. Used
/// for a from-scratch wardrobe character. `model_hash` is the new asset hash.
pub fn build_skinned_model(
    mesh: &StaticMesh,
    model_hash: u32,
    diffuse_hash: u32,
) -> Result<Vec<u8>, String> {
    if mesh.positions.is_empty() || mesh.tris.is_empty() {
        return Err("empty mesh".into());
    }
    if mesh.positions.len() > 65534 {
        return Err(format!("vertex count {} exceeds u16", mesh.positions.len()));
    }
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &mesh.positions {
        for k in 0..3 {
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }
    let strip = to_strip(&mesh.tris);
    if strip.len() > 65534 {
        return Err(format!("strip length {} exceeds u16", strip.len()));
    }
    let vcount = mesh.positions.len() as u32;
    let strip_len = strip.len() as u32;
    let vb = encode_strm40(mesh);
    let mut ib = Vec::with_capacity(strip.len() * 2);
    for &x in &strip {
        ib.extend_from_slice(&(x as u16).to_le_bytes());
    }
    let mut strm_info = Vec::with_capacity(12);
    strm_info.extend_from_slice(&u32b(8)); // flag 8 = skinned
    strm_info.extend_from_slice(&u32b(40));
    strm_info.extend_from_slice(&u32b(vcount));
    let mut prmt = Vec::with_capacity(16);
    prmt.extend_from_slice(&u32b(0));
    prmt.extend_from_slice(&u32b(0));
    prmt.extend_from_slice(&(strip_len as u16).to_le_bytes());
    prmt.extend_from_slice(&0u16.to_le_bytes());
    prmt.extend_from_slice(&((vcount - 1) as u16).to_le_bytes());
    prmt.extend_from_slice(&(vcount as u16).to_le_bytes());

    // Tree: INFO, HIER(1 root bone), MTRL, GEOM{ INFO, INDX, SKIN{ INFO, PRMG{
    //   INFO(56 palette), STRM{info,decl64,data}, IBUF{info,data}, PRMT } } }.
    // NB: SKIN groups carry NO AREA chunk (unlike static MESH).
    let tree: Vec<Node> = vec![
        leaf(b"INFO", build_info(bmin, bmax)),
        leaf(b"HIER", build_hier_root(model_hash)),
        leaf(b"MTRL", build_skinned_mtrl(diffuse_hash)),
        cont(
            b"GEOM",
            vec![
                leaf(b"INFO", u32b(1).to_vec()),
                leaf(b"INDX", 0u16.to_le_bytes().to_vec()),
                cont(
                    b"SKIN",
                    vec![
                        leaf(b"INFO", u32b(1).to_vec()),
                        cont(
                            b"PRMG",
                            vec![
                                leaf(b"INFO", build_skin_palette_info(model_hash)),
                                cont(
                                    b"STRM",
                                    vec![
                                        leaf(b"info", strm_info),
                                        leaf(b"decl", DECL40.to_vec()),
                                        leaf(b"data", vb),
                                    ],
                                ),
                                cont(
                                    b"IBUF",
                                    vec![
                                        leaf(b"info", u32b(strip_len).to_vec()),
                                        leaf(b"data", ib),
                                    ],
                                ),
                                leaf(b"PRMT", prmt),
                            ],
                        ),
                    ],
                ),
            ],
        ),
    ];
    let mut rows: Vec<Row> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    flatten(&tree, &mut rows, &mut data);
    let ndesc = rows.len() as u32;
    let data_off = 20 + ndesc * 20;
    let mut out = Vec::with_capacity(data_off as usize + data.len() + 8);
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&u32b(data_off));
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&u32b(ndesc));
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&u32b(r.u0));
        out.extend_from_slice(&u32b(r.size));
        out.extend_from_slice(&u32b(r.u2));
        out.extend_from_slice(&u32b(r.u3));
    }
    out.extend_from_slice(&data);
    out.extend_from_slice(b"CSUM");
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(&u32b(crc));
    Ok(out)
}

/// Encode the stride-20 vertex buffer: f16 pos[3]+pad | f16 uv[2] | f16 nrm[3]+pad.
fn encode_strm20(mesh: &StaticMesh) -> Vec<u8> {
    let n = mesh.positions.len();
    let mut vb = Vec::with_capacity(n * 20);
    for i in 0..n {
        let p = mesh.positions[i];
        let uv = mesh.uvs.get(i).copied().unwrap_or([0.0, 0.0]);
        let nrm = mesh.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
        vb.extend_from_slice(&f16_le(p[0]));
        vb.extend_from_slice(&f16_le(p[1]));
        vb.extend_from_slice(&f16_le(p[2]));
        vb.extend_from_slice(&[0, 0]); // pad
        vb.extend_from_slice(&f16_le(uv[0]));
        vb.extend_from_slice(&f16_le(uv[1]));
        vb.extend_from_slice(&f16_le(nrm[0]));
        vb.extend_from_slice(&f16_le(nrm[1]));
        vb.extend_from_slice(&f16_le(nrm[2]));
        vb.extend_from_slice(&[0, 0]); // pad
    }
    vb
}

/// Author a complete static UCFX model container from scratch.
///
/// `model_hash` is the NEW asset hash (`pandemic_hash_m2(name)`); `diffuse_hash`
/// is the texture the single material samples (a shipped or global-resident hash).
pub fn build_static_model(
    mesh: &StaticMesh,
    model_hash: u32,
    diffuse_hash: u32,
) -> Result<Vec<u8>, String> {
    if mesh.positions.is_empty() || mesh.tris.is_empty() {
        return Err("empty mesh".into());
    }
    if mesh.positions.len() > 65534 {
        return Err(format!("vertex count {} exceeds u16", mesh.positions.len()));
    }
    // bounds
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &mesh.positions {
        for k in 0..3 {
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }

    // geometry buffers
    let strip = to_strip(&mesh.tris);
    if strip.len() > 65534 {
        return Err(format!("strip length {} exceeds u16", strip.len()));
    }
    let vcount = mesh.positions.len() as u32;
    let strip_len = strip.len() as u32;

    let vb = encode_strm20(mesh);
    let mut ib = Vec::with_capacity(strip.len() * 2);
    for &x in &strip {
        ib.extend_from_slice(&(x as u16).to_le_bytes());
    }
    // AREA: one f16 surface area per strip triangle (0 for degenerate joiners).
    let mut area = Vec::with_capacity(strip.len().saturating_sub(2) * 2);
    for k in 0..strip.len().saturating_sub(2) {
        let (a, b, c) = (strip[k] as usize, strip[k + 1] as usize, strip[k + 2] as usize);
        let ar = if a == b || b == c || a == c {
            0.0
        } else {
            let pa = mesh.positions[a];
            let pb = mesh.positions[b];
            let pc = mesh.positions[c];
            let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
            let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
            let cx = u[1] * v[2] - u[2] * v[1];
            let cy = u[2] * v[0] - u[0] * v[2];
            let cz = u[0] * v[1] - u[1] * v[0];
            0.5 * (cx * cx + cy * cy + cz * cz).sqrt()
        };
        area.extend_from_slice(&f16_le(ar));
    }

    // STRM info (flag=4, stride=20, count)
    let mut strm_info = Vec::with_capacity(12);
    strm_info.extend_from_slice(&u32b(4));
    strm_info.extend_from_slice(&u32b(20));
    strm_info.extend_from_slice(&u32b(vcount));

    // PRMT: single draw record covering the whole strip.
    let mut prmt = Vec::with_capacity(16);
    prmt.extend_from_slice(&u32b(0)); // material_index 0
    prmt.extend_from_slice(&u32b(0)); // start_index
    prmt.extend_from_slice(&(strip_len as u16).to_le_bytes());
    prmt.extend_from_slice(&0u16.to_le_bytes()); // base_vertex
    prmt.extend_from_slice(&((vcount - 1) as u16).to_le_bytes());
    prmt.extend_from_slice(&(vcount as u16).to_le_bytes());

    // ---- assemble the chunk tree ----
    let tree: Vec<Node> = vec![
        leaf(b"INFO", build_info(bmin, bmax)),
        leaf(b"HIER", build_hier_root(model_hash)),
        leaf(b"MTRL", build_mtrl(diffuse_hash)),
        cont(
            b"GEOM",
            vec![
                leaf(b"INFO", u32b(1).to_vec()), // 1 mesh group
                leaf(b"INDX", 0u16.to_le_bytes().to_vec()), // group 0 -> HIER node 0 (root)
                cont(
                    b"MESH",
                    vec![
                        leaf(b"INFO", u32b(1).to_vec()),
                        cont(
                            b"PRMG",
                            vec![
                                leaf(b"INFO", build_prmg_info(bmin, bmax, model_hash)),
                                cont(
                                    b"STRM",
                                    vec![
                                        leaf(b"info", strm_info),
                                        leaf(b"decl", DECL20.to_vec()),
                                        leaf(b"data", vb),
                                    ],
                                ),
                                cont(
                                    b"AREA",
                                    vec![
                                        leaf(b"info", u32b(strip_len.saturating_sub(2)).to_vec()),
                                        leaf(b"data", area),
                                    ],
                                ),
                                cont(
                                    b"IBUF",
                                    vec![
                                        leaf(b"info", u32b(strip_len).to_vec()),
                                        leaf(b"data", ib),
                                    ],
                                ),
                                leaf(b"PRMT", prmt),
                            ],
                        ),
                    ],
                ),
            ],
        ),
    ];

    // ---- flatten + emit ----
    let mut rows: Vec<Row> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    flatten(&tree, &mut rows, &mut data);

    let ndesc = rows.len() as u32;
    let data_off = 20 + ndesc * 20;

    let mut out = Vec::with_capacity(data_off as usize + data.len() + 8);
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&u32b(data_off));
    out.extend_from_slice(&[0u8; 8]); // reserved
    out.extend_from_slice(&u32b(ndesc));
    for r in &rows {
        out.extend_from_slice(&r.tag);
        out.extend_from_slice(&u32b(r.u0));
        out.extend_from_slice(&u32b(r.size));
        out.extend_from_slice(&u32b(r.u2));
        out.extend_from_slice(&u32b(r.u3));
    }
    debug_assert_eq!(out.len() as u32, data_off);
    out.extend_from_slice(&data);
    // CSUM trailer
    out.extend_from_slice(b"CSUM");
    let crc = crc32_mercs2(&out);
    out.extend_from_slice(&u32b(crc));
    Ok(out)
}
