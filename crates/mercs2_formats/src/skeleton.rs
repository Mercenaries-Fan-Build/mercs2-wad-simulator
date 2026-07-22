//! Skeleton extraction from a UCFX model container's `HIER` chunk.
//!
//! REUSABLE (not cesium- or mannequin-specific): parse the 95-node (or N-node)
//! bone hierarchy of any skinned model donor and compute each bone's WORLD REST
//! transform/translation by chaining LOCAL transforms through the parent chain.
//!
//! HIER node layout (176 bytes, mirrors `tools/ucfx_skeleton_codec.py`):
//! ```text
//!  +0  u32  name_hash
//!  +4  u16  index_a (always 1)
//!  +6  u16  first_child (0xFFFF = leaf)
//!  +8  u16  parent      (0xFFFF = root)
//!  +10 u16  sibling     (0xFFFF = none)
//!  +12 u32  flags
//!  +16 4x4 f32 LOCAL transform   (row-major, affine, translation in row 3)
//!  +80 4x4 f32 inverse-bind      (row-major, affine)
//!  +144 vec3 tail_bbox_min + 1.0 pad
//!  +160 vec3 tail_bbox_max + 1.0 pad
//! ```
//!
//! World convention (verified against mattias_v2: root at origin, head ~1.66,
//! hands lateral ±0.46, symmetric): row-vector / row-major, so
//! `world(bone) = local(bone) @ world(parent)`, root's world = its local.

use crate::ffcs::read_u32_le;

pub const HIER_NODE_STRIDE: usize = 176;

/// One bone with its resolved world-rest transform.
#[derive(Debug, Clone)]
pub struct Bone {
    pub index: usize,
    pub name_hash: u32,
    pub parent: i32, // -1 = root
    /// World-rest 4x4 (row-major, translation in row 3), resolved by chaining the HIER records'
    /// LOCAL matrices (`+16`). This is the model's DEFAULT pose, which is not always its BIND pose.
    pub world: [[f32; 4]; 4],
    /// The BIND pose: inverse of the inverse-bind matrix the HIER record stores at `+80`, when that
    /// matrix is present and invertible. `None` falls back to [`Bone::world`].
    ///
    /// The two agree on most models and the distinction was missed for a long time, but they are
    /// NOT the same thing and shipped characters prove it. `pmc_hum_mattias_v2` matches on all 95
    /// nodes; `pmc_hum_jen` disagrees on 15 of 92; `pmc_hum_chris` on **54 of 94** — Chris's HIER
    /// locals hold a weapon-carry pose with the elbows bent 76 deg, while `+80` holds the ordinary
    /// hero bind (12.3 deg, hand at [0.435, 0.989, 0.020], identical to Mattias's).
    ///
    /// The mesh settles it: the distance from each bone to the centroid of the vertices it
    /// dominates has median 0.024 (mattias) / 0.025 (jen) / **0.123 (chris)** against `world`, and
    /// 0.024 / 0.025 / **0.027** against this field. Skinning maths is `Skin[b] = InvBind[b] *
    /// Pose[b]`, so `+80` is by definition the matrix the vertices were authored against — anything
    /// that binds geometry to bones (retarget, skin authoring, bundle export) must use THIS.
    pub bind_world: Option<[[f32; 4]; 4]>,
    /// The `+80`-derived bind BEFORE the staleness rejection, and whether it was rejected.
    ///
    /// Kept because the rejection is a JUDGEMENT, not a decode: it assumes a bone that lands on its
    /// parent cannot be a real bind. If the engine instead skins with the raw record, geometry
    /// conformed to the corrected pose is wrong in exactly the way that hides at rest and shows
    /// under animation. Retaining the raw value lets that be settled by measurement against shipped
    /// geometry rather than asserted.
    pub bind_world_raw: Option<[[f32; 4]; 4]>,
    pub bind_stale: bool,
}

impl Bone {
    /// World-rest (default-pose) translation.
    pub fn world_pos(&self) -> [f32; 3] {
        [self.world[3][0], self.world[3][1], self.world[3][2]]
    }
    /// BIND-pose translation — [`Bone::bind_world`] when the HIER carried one, else
    /// [`Bone::world_pos`]. Use this for anything that relates GEOMETRY to bones.
    pub fn bind_pos(&self) -> [f32; 3] {
        let m = self.bind_world.as_ref().unwrap_or(&self.world);
        [m[3][0], m[3][1], m[3][2]]
    }
}

#[derive(Debug, Clone)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

/// Distance between two row-major 4x4s' translation rows.
fn dist3(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> f32 {
    let (dx, dy, dz) = (a[3][0] - b[3][0], a[3][1] - b[3][1], a[3][2] - b[3][2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn read_f32(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn mat4_row_major(d: &[u8], o: usize) -> [[f32; 4]; 4] {
    let mut m = [[0.0f32; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            let v = read_f32(d, o + (r * 4 + c) * 4);
            m[r][c] = if v.is_finite() { v } else { 0.0 };
        }
    }
    m
}

/// `a @ b` for row-major 4x4 (row-vector convention).
fn matmul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    mat4_mul(a, b)
}

/// `a @ b` for row-major 4x4 (row-vector convention). Public for the
/// cross-skeleton re-pose driver, which composes `World_M[map(b)] @ InvBind_S[b]`.
pub fn mat4_mul(a: &[[f32; 4]; 4], b: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    let mut r = [[0.0f32; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                r[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    r
}

/// Determinant of the upper-left 3x3 of a row-major 4x4 — the invertibility test for a HIER
/// record's stored inverse-bind matrix.
pub fn mat3_det(m: &[[f32; 4]; 4]) -> f32 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Affine inverse of a row-major / row-vector 4x4 (the upper-left 3x3 linear part
/// inverted, then the translation re-projected). Robust for the rigid-ish bind
/// transforms here (rotation + optional uniform scale + translation). Falls back to
/// the transpose-based rigid inverse if the 3x3 is (near-)singular.
pub fn affine_inverse(m: &[[f32; 4]; 4]) -> [[f32; 4]; 4] {
    // linear 3x3 R (rows 0..3, cols 0..3); translation in row 3.
    let r = [
        [m[0][0], m[0][1], m[0][2]],
        [m[1][0], m[1][1], m[1][2]],
        [m[2][0], m[2][1], m[2][2]],
    ];
    let t = [m[3][0], m[3][1], m[3][2]];
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    let inv: [[f32; 3]; 3] = if det.abs() > 1e-12 {
        let id = 1.0 / det;
        [
            [
                (r[1][1] * r[2][2] - r[1][2] * r[2][1]) * id,
                (r[0][2] * r[2][1] - r[0][1] * r[2][2]) * id,
                (r[0][1] * r[1][2] - r[0][2] * r[1][1]) * id,
            ],
            [
                (r[1][2] * r[2][0] - r[1][0] * r[2][2]) * id,
                (r[0][0] * r[2][2] - r[0][2] * r[2][0]) * id,
                (r[0][2] * r[1][0] - r[0][0] * r[1][2]) * id,
            ],
            [
                (r[1][0] * r[2][1] - r[1][1] * r[2][0]) * id,
                (r[0][1] * r[2][0] - r[0][0] * r[2][1]) * id,
                (r[0][0] * r[1][1] - r[0][1] * r[1][0]) * id,
            ],
        ]
    } else {
        // rigid fallback: inverse of a pure-rotation 3x3 is its transpose
        [
            [r[0][0], r[1][0], r[2][0]],
            [r[0][1], r[1][1], r[2][1]],
            [r[0][2], r[1][2], r[2][2]],
        ]
    };
    // inverse translation = -t @ inv
    let it = [
        -(t[0] * inv[0][0] + t[1] * inv[1][0] + t[2] * inv[2][0]),
        -(t[0] * inv[0][1] + t[1] * inv[1][1] + t[2] * inv[2][1]),
        -(t[0] * inv[0][2] + t[1] * inv[1][2] + t[2] * inv[2][2]),
    ];
    [
        [inv[0][0], inv[0][1], inv[0][2], 0.0],
        [inv[1][0], inv[1][1], inv[1][2], 0.0],
        [inv[2][0], inv[2][1], inv[2][2], 0.0],
        [it[0], it[1], it[2], 1.0],
    ]
}

/// Transform a POINT (w=1) by a row-major / row-vector 4x4: `p' = [p,1] @ m`.
pub fn transform_point(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    [
        p[0] * m[0][0] + p[1] * m[1][0] + p[2] * m[2][0] + m[3][0],
        p[0] * m[0][1] + p[1] * m[1][1] + p[2] * m[2][1] + m[3][1],
        p[0] * m[0][2] + p[1] * m[1][2] + p[2] * m[2][2] + m[3][2],
    ]
}

/// Transform a DIRECTION (w=0, e.g. a normal) by the 3x3 linear part of a
/// row-major / row-vector 4x4: `n' = n @ R`. (For rigid-ish bind transforms the
/// linear part is rotation+uniform-scale, so this is correct up to a renormalise.)
pub fn transform_dir(m: &[[f32; 4]; 4], n: [f32; 3]) -> [f32; 3] {
    [
        n[0] * m[0][0] + n[1] * m[1][0] + n[2] * m[2][0],
        n[0] * m[0][1] + n[1] * m[1][1] + n[2] * m[2][1],
        n[0] * m[0][2] + n[1] * m[1][2] + n[2] * m[2][2],
    ]
}

impl Skeleton {
    /// Extract the skeleton from a full WAD model block (20-byte wrapper + UCFX +
    /// CSUM). Locates the first `HIER` leaf chunk, parses every node and resolves
    /// world-rest transforms in index order (parent[i] < i is guaranteed by the
    /// exporter, so a single forward pass suffices).
    pub fn from_block(container_block: &[u8]) -> Result<Skeleton, String> {
        if container_block.len() < 20 {
            return Err("block too small".into());
        }
        let ucfx_len = read_u32_le(container_block, 16) as usize;
        let ucfx = &container_block[20..20 + ucfx_len];
        if &ucfx[0..4] != b"UCFX" {
            return Err("payload is not UCFX".into());
        }
        let data_off = read_u32_le(ucfx, 4) as usize;
        let ndesc = read_u32_le(ucfx, 16) as usize;

        // find the first HIER leaf (u0 != container sentinel, size >= one node)
        let mut hier: Option<(usize, usize)> = None; // (abs_base, n_nodes)
        for i in 0..ndesc {
            let ro = 20 + i * 20;
            let tag = &ucfx[ro..ro + 4];
            let u0 = read_u32_le(ucfx, ro + 4);
            let size = read_u32_le(ucfx, ro + 8) as usize;
            if tag == b"HIER" && u0 != 0xFFFF_FFFF && size >= HIER_NODE_STRIDE {
                hier = Some((data_off + u0 as usize, size / HIER_NODE_STRIDE));
                break;
            }
        }
        let (base, n) = hier.ok_or("no HIER leaf chunk found")?;
        if base + n * HIER_NODE_STRIDE > ucfx.len() {
            return Err("HIER chunk out of range".into());
        }

        let mut name_hash = vec![0u32; n];
        let mut parent = vec![-1i32; n];
        let mut local = vec![[[0.0f32; 4]; 4]; n];
        let mut inv_bind: Vec<Option<[[f32; 4]; 4]>> = vec![None; n];
        for r in 0..n {
            let o = base + r * HIER_NODE_STRIDE;
            name_hash[r] = read_u32_le(ucfx, o);
            let p = u16::from_le_bytes([ucfx[o + 8], ucfx[o + 9]]);
            parent[r] = if p == 0xFFFF { -1 } else { p as i32 };
            local[r] = mat4_row_major(ucfx, o + 16);
            // `+80` is the record's INVERSE-BIND matrix (see `model_inject::append_hier_bones`,
            // which writes it there). Invert it back to a bind-pose world transform; skip anything
            // singular (a record that never carried one).
            let ib = mat4_row_major(ucfx, o + 80);
            inv_bind[r] = if mat3_det(&ib).abs() > 1e-12 {
                Some(affine_inverse(&ib))
            } else {
                None
            };
        }

        let mut world = vec![[[0.0f32; 4]; 4]; n];
        for r in 0..n {
            let p = parent[r];
            world[r] = if p < 0 || p as usize >= n {
                local[r]
            } else {
                matmul(&local[r], &world[p as usize])
            };
        }

        // Reject STALE `+80` records. Not every node carries its own inverse-bind: some hold an
        // ANCESTOR's matrix, which inverts to a bind that puts the bone exactly on its parent —
        // a zero-length bone. `pmc_hum_mattias` has 8 such nodes (the spine root, both clavicles,
        // the neck cluster). Trusting them conforms geometry as if those joints coincide: it
        // survives the rest pose and tears the mesh apart the moment animation separates them.
        //
        // The test is collapse-onto-parent, NOT difference-from-default — a genuinely posed bind
        // differs from the default pose legitimately (both of this model's hand bones do, and
        // Chris's whole arm chain does). Only a bone landing ON its parent while the default pose
        // gives it real length is impossible.
        //
        // Parents precede children in HIER order, so one forward pass resolves each bone against
        // its parent's ALREADY-VALIDATED bind.
        const COLLAPSE_EPS: f32 = 1e-3;
        let raw_bind = inv_bind.clone();
        let mut stale = vec![false; n];
        for r in 0..n {
            let p = parent[r];
            if p < 0 || p as usize >= n {
                continue;
            }
            let Some(ib) = inv_bind[r] else { continue };
            let pb = inv_bind[p as usize].unwrap_or(world[p as usize]);
            let d_bind = dist3(&ib, &pb);
            let d_default = dist3(&world[r], &world[p as usize]);
            if d_bind < COLLAPSE_EPS && d_default > COLLAPSE_EPS {
                inv_bind[r] = None; // stale — fall back to the chained default pose
                stale[r] = true;
            }
        }

        let bones = (0..n)
            .map(|r| Bone {
                index: r,
                name_hash: name_hash[r],
                parent: parent[r],
                world: world[r],
                bind_world: inv_bind[r],
                bind_world_raw: raw_bind[r],
                bind_stale: stale[r],
            })
            .collect();
        Ok(Skeleton { bones })
    }

    /// Find the bone index by exact name-hash, if present.
    pub fn by_hash(&self, h: u32) -> Option<usize> {
        self.bones.iter().position(|b| b.name_hash == h)
    }

    /// The bone's WORLD-REST (bind) 4x4 (row-major, row-vector convention).
    pub fn world_bind(&self, bone: usize) -> [[f32; 4]; 4] {
        self.bones[bone].world
    }

    /// The bone's INVERSE world-rest (bind) 4x4 — the affine inverse of
    /// [`Skeleton::world_bind`]. For the cross-skeleton re-pose this is the
    /// source bone's `InvBind_S` (maps a bind-space point into that bone's local
    /// frame).
    pub fn inv_world_bind(&self, bone: usize) -> [[f32; 4]; 4] {
        affine_inverse(&self.bones[bone].world)
    }

    /// Overall Y extent (feet→head) — the bind height sanity check.
    pub fn height(&self) -> f32 {
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for b in &self.bones {
            let y = b.world_pos()[1];
            lo = lo.min(y);
            hi = hi.max(y);
        }
        hi - lo
    }
}
