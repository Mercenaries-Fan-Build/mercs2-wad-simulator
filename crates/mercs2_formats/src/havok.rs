//! Little-endian Havok-5.5 packfile reader — the **read / extract** path for
//! PC-retail `PHY2` collision data.
//!
//! Relationship to the other Havok code in this workspace:
//! - [`ucfx_byteswap::havok`] is the PS3 **BE→LE byteswap converter** (its fixup
//!   parsers are big-endian; its job is to *rewrite* a packfile, not read it).
//! - This module is the **little-endian reader** for already-LE retail bodies:
//!   it walks the packfile structure (section headers → virtual/local fixups →
//!   class instances) and pulls out collision geometry — convex break-piece
//!   hulls (verts + plane equations), box shapes, MOPP/mesh references.
//!
//! It replaces the heuristic `tools/havok_extractor.py` (`longest_vec3_run`, a
//! byte-scan that yields denormal garbage). Proven against the Python reversal
//! on the resident2 up-crate: 6 hulls `[19,24,35,12,36,10]` (see tests).
//!
//! Packfile layout (HK 5.5.0-r1, 32-bit, searched magic — there is a u32 prefix):
//! `__classnames__` marks the start of three 48-byte section headers
//! (20-byte name + 7×u32 `[abs, lf, gf, vf, exp, imp, end]`); section bodies
//! follow the header table. In `__data__`: **virtual fixups** (`src, sec, cnoff`)
//! bind an object offset to its class name; **local fixups** (`src, dst`) relocate
//! the object's pointer fields (e.g. the hkArray data pointers).
//!
//! `hkpConvexVerticesShape`: `+64` m_rotatedVertices hkArray (FourVectors SoA —
//! `X[4]Y[4]Z[4]` = 4 verts per 48B block), `+76` m_numVertices, `+80`
//! m_planeEquations hkArray (hkVector4 `n.xyz, -support`), `+84` plane count.

use std::collections::{BTreeMap, HashMap};

/// 8-byte Havok packfile magic (palindromic per u32 word). Searched, not at 0.
pub const HAVOK_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

const CLASSNAMES: &[u8] = b"__classnames__";
const CONVEX: &str = "hkpConvexVerticesShape";
const SECTION_HDR: usize = 48;

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0
    }
}

#[inline]
fn u16_le(b: &[u8], o: usize) -> u16 {
    if o + 2 <= b.len() {
        u16::from_le_bytes([b[o], b[o + 1]])
    } else {
        0
    }
}

#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    if o + 4 <= b.len() {
        f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0.0
    }
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// A convex collision hull — one destructible break piece's shape.
/// `vertices` are the rotated hull vertices in model-local space; `planes` are
/// the face half-spaces `n·x + w ≤ 0` where `w = -max_v(n·v)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvexHull {
    pub vertices: Vec<[f32; 3]>,
    pub planes: Vec<[f32; 4]>,
}

/// A `hkpCapsuleShape` — the collider Havok uses for a ragdoll limb/torso body.
///
/// Layout **verified against 11 real instances** in retail `vz.wad` block 3185 (the resident
/// human/animation block) by `ragdoll_probe` and the `capsule_layout_*` tests below:
/// ```text
///  +16  f32       m_radius            (shared hkpConvexShape::m_radius)
///  +32  hkVector4 m_vertexA           (x,y,z ; .w duplicates m_radius)
///  +48  hkVector4 m_vertexB
/// ```
/// Every human-ragdoll capsule is a segment along **local Y**: `m_vertexA = (0,+h,0)`,
/// `m_vertexB = (0,-h,0)`, so it is fully described by `radius` + `half_len = |vertexA.y|`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capsule {
    pub radius: f32,
    pub vertex_a: [f32; 3],
    pub vertex_b: [f32; 3],
}

impl Capsule {
    /// Half the distance between the two segment endpoints — the capsule's cylindrical half-length.
    pub fn half_len(&self) -> f32 {
        let d = [
            self.vertex_a[0] - self.vertex_b[0],
            self.vertex_a[1] - self.vertex_b[1],
            self.vertex_a[2] - self.vertex_b[2],
        ];
        0.5 * (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
}

/// A `WpMeshShape16` — Pandemic's 16-bit-indexed static collision mesh (buildings, terrain cells).
///
/// Layout **cracked empirically** (40/40 real vz.wad instances decode with score ≥ 0.9). Within the
/// packfile `pk` (`obj` = `data_pk + mesh_src`):
/// ```text
///  obj+32   u32        nsub                 subpart count (sane 1..=256)
///  obj+48   sp0        first subpart; quantization block:
///    sp0+0  3×f32      min[3]               dequant offset
///    sp0+16 3×f32      scale[3]             dequant scale
///  per subpart s (sp = sp0 + s*48):
///    sp+36  u32        acnt                 triangle count
///    lf[sp_src+32]                          → 16-bit INDEX array base (data-relative)
///      each tri = u16 a,b,c at ap+t*8 (+0,+2,+4); the +6 tail is material/pad
/// ```
/// **Vertices** are quantized `u16×3` (6 bytes each): `vert[k] = min[k] + u16 * scale[k]`. The vertex
/// pool lives in the PHY2 engine WRAPPER *beyond* the Havok packfile — it is NOT reachable via any
/// local/virtual fixup and NOT at a fixed offset from the packfile end (verified: its offset-from-end
/// ranges 0..494256 across instances), so its base is recovered by a bounded scan of the trailing region
/// (`// CONFIRM-LIVE: pool base found by scan`).
#[derive(Debug, Clone, PartialEq)]
pub struct MeshShape {
    /// Dequantized model-local vertices (`min + u16*scale`), indexed by the triangle triples.
    pub vertices: Vec<[f32; 3]>,
    /// Triangle index triples into `vertices`.
    pub indices: Vec<[u16; 3]>,
}

impl MeshShape {
    /// The world-space (model-local) XZ span of the mesh AABB — the terrain-vs-building discriminator the
    /// collision router uses (a full terrain cell spans ≈400 m in both X and Z; a building ≤ ~40 m).
    pub fn xz_span(&self) -> [f32; 2] {
        let (mut mnx, mut mnz, mut mxx, mut mxz) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for v in &self.vertices {
            mnx = mnx.min(v[0]);
            mxx = mxx.max(v[0]);
            mnz = mnz.min(v[2]);
            mxz = mxz.max(v[2]);
        }
        if self.vertices.is_empty() {
            [0.0, 0.0]
        } else {
            [mxx - mnx, mxz - mnz]
        }
    }
}

/// A collision shape recovered from a packfile.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// `hkpConvexVerticesShape` — a convex break-piece hull.
    Convex(ConvexHull),
    /// `hkpBoxShape` — half-extents (best-effort: m_halfExtents @ +16).
    Box { half_extents: [f32; 3] },
    /// `hkpCapsuleShape` — a swept-segment body collider (ragdoll limbs; `+16` radius, `+32/+48`
    /// endpoints). Verified against block 3185. See [`Capsule`].
    Capsule(Capsule),
    /// `hkpSphereShape` — `+16` m_radius (verified: 1.0964 / 0.1500 in vz.wad).
    Sphere { radius: f32 },
    /// `hkpMoppBvTreeShape` / `hkpMoppCode` — static non-convex mesh BV-tree.
    Mopp,
    /// `WpMeshShape16` — Pandemic 16-bit-indexed static collision mesh (dequantized verts + index
    /// triples). Empty `indices` means the decode did not resolve (treat as an undecoded static mesh).
    /// See [`MeshShape`].
    Mesh(MeshShape),
    /// Another `*Shape*` class we recognise by name but don't yet decode.
    Other(String),
}

/// A parsed Havok packfile: its version, byte size (from the section headers),
/// the collision shapes it contains, and a census of every class instance.
#[derive(Debug, Clone)]
pub struct Packfile {
    pub version: String,
    pub size: usize,
    pub shapes: Vec<Shape>,
    pub class_counts: BTreeMap<String, u32>,
}

impl Packfile {
    /// Iterate just the convex break-piece hulls, in packfile order.
    pub fn hulls(&self) -> impl Iterator<Item = &ConvexHull> {
        self.shapes.iter().filter_map(|s| match s {
            Shape::Convex(h) => Some(h),
            _ => None,
        })
    }

    /// Iterate the `hkpCapsuleShape` bodies (ragdoll limb/torso colliders), in packfile order.
    pub fn capsules(&self) -> impl Iterator<Item = &Capsule> {
        self.shapes.iter().filter_map(|s| match s {
            Shape::Capsule(c) => Some(c),
            _ => None,
        })
    }
}

/// Recover the **human ragdoll body colliders** from a decompressed WAD block.
///
/// Faithful recovery, not fabrication: the retail PC `vz.wad` serializes **no** `hkpRigidBody`,
/// `hkpRagdollConstraintData`, `hkaRagdollInstance` or skeleton mapper (a full-WAD census found
/// **zero** instances — the ragdoll rigid-body chain + constraints are built procedurally at load,
/// Havok-side). What it *does* ship is the set of `hkpCapsuleShape` colliders the ragdoll bodies
/// attach to. The resident human/animation block (3185) carries exactly **11** — the classic
/// humanoid ragdoll body count (pelvis, spine, head, 2×upper-arm, 2×fore-arm, 2×thigh, 2×shin).
///
/// Returns every capsule in the first packfile that holds exactly 11 (the human ragdoll set), or all
/// capsules across all packfiles if none has 11. Empty when the block carries no capsule.
pub fn human_ragdoll_capsules(block: &[u8]) -> Vec<Capsule> {
    let mut all: Vec<Capsule> = Vec::new();
    for (_off, pf) in find_packfiles(block) {
        let caps: Vec<Capsule> = pf.capsules().copied().collect();
        if caps.len() == 11 {
            return caps;
        }
        all.extend(caps);
    }
    all
}

/// The structural skeleton of a parsed packfile, shared by every class decoder.
/// It is the output of the section-header + fixup walk that both the collision
/// decode ([`parse_packfile`]) and the animation decode (`crate::anim`) build on.
///
/// All offsets are absolute indices into the `pk` slice that was parsed.
#[derive(Debug, Clone)]
pub struct RawPackfile {
    /// Absolute offset of the `__data__` section body in the parsed slice.
    pub data_pk: usize,
    /// Total packfile byte size (max section `abs + end`).
    pub size: usize,
    /// Havok version string (e.g. `"Havok-5.5.0-r1"`).
    pub version: String,
    /// classname-body-relative name-string offset → class name.
    pub names: HashMap<usize, String>,
    /// Local fixups: object field-offset (relative to `data_pk`) → data offset
    /// (relative to `data_pk`). Resolve a pointer field with
    /// `data_pk + lf[&(obj_src + field_off)]`.
    pub lf: HashMap<usize, usize>,
    /// Virtual fixups in packfile order: `(object src offset relative to
    /// `data_pk`, class name)`.
    pub vfixups: Vec<(usize, String)>,
}

impl RawPackfile {
    /// Absolute offset of an object whose virtual-fixup `src` is `src`.
    #[inline]
    pub fn obj_abs(&self, src: usize) -> usize {
        self.data_pk + src
    }
    /// Resolve a pointer field at object-relative `obj_src + field_off` to an
    /// absolute offset via the local-fixup table (`None` if unrelocated / null).
    #[inline]
    pub fn resolve_ptr(&self, obj_src: usize, field_off: usize) -> Option<usize> {
        self.lf
            .get(&(obj_src + field_off))
            .map(|d| self.data_pk + d)
    }
}

/// Walk a Havok packfile's section headers, classname table and fixup tables
/// without decoding any class instances. This is the reusable structural pass
/// shared by the collision reader and the animation reader.
pub fn parse_packfile_raw(pk: &[u8]) -> Result<RawPackfile, String> {
    let sh = find_sub(pk, CLASSNAMES).ok_or("packfile missing __classnames__ section")?;
    if sh + 3 * SECTION_HDR > pk.len() {
        return Err("truncated section-header table".into());
    }
    // three section headers: 20-byte name + 7×u32 [abs, lf, gf, vf, exp, imp, end]
    let mut secs = [[0u32; 7]; 3];
    for (s, sec) in secs.iter_mut().enumerate() {
        for (k, field) in sec.iter_mut().enumerate() {
            *field = u32_le(pk, sh + s * SECTION_HDR + 20 + k * 4);
        }
    }
    let body0 = sh + 3 * SECTION_HDR; // section bodies start after the header table
    let cn_len = secs[0][1] as usize; // classname strings occupy the first lf bytes
    let data_pk = body0 + secs[0][6] as usize + secs[1][6] as usize; // __data__ body start
    let (d_lf, d_gf, d_vf, d_end) = (
        secs[2][1] as usize,
        secs[2][2] as usize,
        secs[2][3] as usize,
        secs[2][4] as usize,
    );
    let size = (0..3)
        .map(|i| secs[i][0] as usize + secs[i][6] as usize)
        .max()
        .unwrap_or(pk.len());

    // classnames: { offset-relative-to-classnames-body : class name }
    let mut names: HashMap<usize, String> = HashMap::new();
    let cn_end = (body0 + cn_len).min(pk.len());
    let mut p = body0;
    while p + 5 <= cn_end {
        if u32_le(pk, p) == 0xFFFF_FFFF {
            break;
        }
        let mut q = p + 5;
        while q < cn_end && pk[q] != 0 {
            q += 1;
        }
        if let Ok(name) = std::str::from_utf8(&pk[p + 5..q]) {
            if !name.is_empty() {
                names.insert(p + 5 - body0, name.to_string());
            }
        }
        p = q + 1;
    }

    // local fixups: object pointer field → data offset
    let mut lf: HashMap<usize, usize> = HashMap::new();
    let lf_end = (data_pk + d_gf).min(pk.len());
    let mut k = data_pk + d_lf;
    while k + 8 <= lf_end {
        let src = u32_le(pk, k);
        if src == 0xFFFF_FFFF {
            break;
        }
        lf.insert(src as usize, u32_le(pk, k + 4) as usize);
        k += 8;
    }

    // virtual fixups: object → class name.
    let mut vfixups = Vec::new();
    let vf_end = (data_pk + d_end).min(pk.len());
    let mut k = data_pk + d_vf;
    while k + 12 <= vf_end {
        let src = u32_le(pk, k) as usize;
        let cnoff = u32_le(pk, k + 8) as usize;
        if src == 0xFFFF_FFFF {
            break;
        }
        k += 12;
        let cname = names.get(&cnoff).cloned().unwrap_or_else(|| "?".into());
        vfixups.push((src, cname));
    }

    let version = find_sub(pk, b"Havok-")
        .map(|o| {
            let mut q = o;
            while q < pk.len() && pk[q] != 0 && pk[q].is_ascii_graphic() {
                q += 1;
            }
            String::from_utf8_lossy(&pk[o..q]).into_owned()
        })
        .unwrap_or_default();

    Ok(RawPackfile {
        data_pk,
        size,
        version,
        names,
        lf,
        vfixups,
    })
}

/// Decode ONE `WpMeshShape16` into dequantized model-local vertices + index triples.
///
/// `pk` is the parsed packfile slice — but it MUST extend past the packfile into the PHY2 engine
/// wrapper, because the quantized vertex pool lives THERE (beyond `raw.size`). [`parse_phy2_body`] and
/// [`find_packfiles`] pass `&buf[off..]`, which does. `obj_src` is the mesh object's virtual-fixup src.
///
/// Returns `None` (→ an undecoded static mesh, caller keeps the render fallback) if the object is
/// malformed or no vertex-pool base survives the acceptance guards. Two-stage recovery: the fast wrapper
/// scan (the validated 40/40 path), then a guarded whole-slice fallback for the pools it misses (retail c3
/// census: 77 undecoded → 28, zero regression). The vertex pool has NO packfile fixup — the only fixups on
/// the mesh object are the inline subpart pointer (`+28`, self-relative to `obj+48`) and the two u16 index
/// arrays (`+80`/`+88`); the quantized pool lives in the engine MOPP wrapper past the packfile with no fixed
/// offset — hence a content scan. See the FALLBACK block below and [`MeshShape`].
fn decode_mesh_shape16(pk: &[u8], raw: &RawPackfile, obj_src: usize) -> Option<MeshShape> {
    let obj = raw.data_pk + obj_src;
    let nsub = u32_le(pk, obj + 32) as usize;
    if nsub == 0 || nsub > 256 {
        return None;
    }
    let sp0 = obj + 48;
    let min = [f32_le(pk, sp0), f32_le(pk, sp0 + 4), f32_le(pk, sp0 + 8)];
    let scale = [f32_le(pk, sp0 + 16), f32_le(pk, sp0 + 20), f32_le(pk, sp0 + 24)];
    if !min.iter().chain(&scale).all(|v| v.is_finite()) {
        return None;
    }
    // 16-bit index triples, gathered across every subpart via each subpart's local-fixup index pointer.
    // `idx_ranges` records each subpart's index-array byte span so the fallback pool scan can REJECT any
    // candidate base that overlaps the index arrays (the classic false positive: index values are small
    // 0..N, so dequantising the index bytes as vertices clusters every vertex near `min` → all triangles
    // collapse to ~0 size → a spurious perfect edge-score. A real vertex pool never overlaps the indices).
    let mut indices: Vec<[u16; 3]> = Vec::new();
    let mut idx_ranges: Vec<(usize, usize)> = Vec::new();
    for s in 0..nsub {
        let sp = sp0 + s * 48;
        let sp_src = sp.checked_sub(raw.data_pk)?;
        let acnt = u32_le(pk, sp + 36) as usize;
        if acnt > 4_000_000 {
            return None; // defensive: real subparts are ≤ ~18k tris
        }
        let ap = raw.data_pk + *raw.lf.get(&(sp_src + 32))?;
        idx_ranges.push((ap, ap + acnt * 8));
        for t in 0..acnt {
            if ap + t * 8 + 6 > pk.len() {
                return None;
            }
            indices.push([
                u16_le(pk, ap + t * 8),
                u16_le(pk, ap + t * 8 + 2),
                u16_le(pk, ap + t * 8 + 4),
            ]);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let maxidx = indices.iter().map(|t| t[0].max(t[1]).max(t[2])).max().unwrap() as usize;
    let nverts = maxidx + 1;

    // Dequantize a vertex from a candidate pool base (pool holds u16×3, 6 bytes/vertex).
    let getb = |pool: usize, v: usize| -> [f32; 3] {
        let o = pool + v * 6;
        [
            min[0] + u16_le(pk, o) as f32 * scale[0],
            min[1] + u16_le(pk, o + 2) as f32 * scale[1],
            min[2] + u16_le(pk, o + 4) as f32 * scale[2],
        ]
    };
    let edge = |p: [f32; 3], q: [f32; 3]| {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    // Fraction of triangles whose three edges are all < 40 m — a well-placed pool yields sane tris.
    let score = |pool: usize| -> f64 {
        let mut ok = 0usize;
        for t in &indices {
            let (va, vb, vc) = (getb(pool, t[0] as usize), getb(pool, t[1] as usize), getb(pool, t[2] as usize));
            if edge(va, vb) < 40.0 && edge(vb, vc) < 40.0 && edge(va, vc) < 40.0 {
                ok += 1;
            }
        }
        ok as f64 / indices.len() as f64
    };

    // CONFIRM-LIVE: pool base found by scan. The quantized vertex pool is engine-wrapper data beyond the
    // packfile — NOT reachable via a fixup and NOT at a fixed offset from the packfile end (its
    // offset-from-end ranges 0..494256 across real instances), so the base is recovered by a bounded scan
    // of the trailing region. Cheap tris[0] pre-check + a 6-triangle medium check gate the full score;
    // stop early on a near-perfect base. Constrained: start at the packfile end (`raw.size`), keep the
    // best base only if its score > 0.9 (matches the probe's 40/40 acceptance).
    let trail = raw.size;
    let end = pk.len().saturating_sub(nverts * 6);
    if trail > end {
        return None; // no room for the pool inside the wrapper this slice carries
    }
    let t0 = indices[0];
    let probe: Vec<[u16; 3]> = indices.iter().take(6).copied().collect();
    let (mut best_score, mut best_base) = (0.0f64, None::<usize>);
    let mut base = trail;
    while base <= end {
        let (va, vb, vc) = (getb(base, t0[0] as usize), getb(base, t0[1] as usize), getb(base, t0[2] as usize));
        if edge(va, vb) > 0.0 && edge(va, vb) < 40.0 && edge(vb, vc) < 40.0 && edge(va, vc) < 40.0 {
            let mut good = true;
            for t in &probe {
                let (x, y, z) = (getb(base, t[0] as usize), getb(base, t[1] as usize), getb(base, t[2] as usize));
                if !(edge(x, y) < 40.0 && edge(y, z) < 40.0 && edge(x, z) < 40.0) {
                    good = false;
                    break;
                }
            }
            if good {
                let sc = score(base);
                if sc > best_score {
                    best_score = sc;
                    best_base = Some(base);
                    if sc > 0.999 {
                        break;
                    }
                }
            }
        }
        base += 2;
    }
    // FALLBACK — recover the ~1.3% of pools the wrapper scan misses (verified over the retail c3 census:
    // 77 undecoded WpMeshShape16 → 28, zero regression to the wrapper-scan successes). The misses fall in
    // two classes the wrapper scan cannot see: (1) TERRAIN-scale cell meshes whose pool sits at the very end
    // of the PHY2 chunk but whose FIRST triangle is a large (>40 m) cell edge, so the `tris[0]`/6-probe gate
    // above skips the correct base; (2) meshes whose pool lies BEFORE the packfile end. The fallback widens
    // the search to the whole slice with three guards that keep it honest (no fabricated geometry):
    //   • a SAMPLED cheap gate (12 evenly-spaced triangles, majority < 40 m, at least one non-degenerate
    //     edge) — admits terrain pools the strict `tris[0]` gate rejected, still cheap;
    //   • a SPREAD gate (used-vertex bbox diagonal in [3 m, 900 m]) — rejects the degenerate index-cluster
    //     false positive and any all-coincident window;
    //   • an INDEX-OVERLAP reject — the candidate pool must not overlap this mesh's own index arrays.
    // Wrapper region first (never changes a wrapper-scan success), then the whole slice. A mesh whose pool
    // cannot be located under these guards stays undecoded (empty) and is reported by the caller — the
    // remaining 28 are small building sub-meshes bound through the engine MOPP wrapper with no scan-locatable
    // pool, which we do NOT guess at.
    let pool = best_base.filter(|_| best_score > 0.9).or_else(|| {
        // Used-vertex bbox diagonal at a candidate base.
        let spread = |base: usize| -> f32 {
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for t in &indices {
                for &i in t.iter() {
                    let v = getb(base, i as usize);
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                }
            }
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt()
        };
        let overlaps_indices = |base: usize| -> bool {
            let e = base + nverts * 6;
            idx_ranges.iter().any(|&(a, b)| base < b && a < e)
        };
        // Sampled cheap pre-gate (≤12 triangles): ≥50% sane edges and not an all-coincident cluster.
        let step_s = (indices.len() / 12).max(1);
        let sample: Vec<[u16; 3]> = indices.iter().step_by(step_s).take(12).copied().collect();
        let cheap = |base: usize| -> bool {
            let (mut ok, mut any_extent) = (0usize, false);
            for t in &sample {
                let (a, b, c) = (getb(base, t[0] as usize), getb(base, t[1] as usize), getb(base, t[2] as usize));
                let (e0, e1, e2) = (edge(a, b), edge(b, c), edge(a, c));
                if e0 < 40.0 && e1 < 40.0 && e2 < 40.0 {
                    ok += 1;
                }
                if e0 > 0.05 || e1 > 0.05 || e2 > 0.05 {
                    any_extent = true;
                }
            }
            any_extent && ok * 2 >= sample.len()
        };
        let accept = |base: usize| -> Option<f64> {
            if overlaps_indices(base) || !cheap(base) {
                return None;
            }
            let sc = score(base);
            let sp = spread(base);
            (sc >= 0.9 && (3.0..=900.0).contains(&sp)).then_some(sc)
        };
        for lo in [trail, 0usize] {
            let (mut bsc, mut bb) = (0.0f64, None);
            let mut base = lo.min(end);
            while base <= end {
                if let Some(sc) = accept(base) {
                    if sc > bsc {
                        bsc = sc;
                        bb = Some(base);
                        if sc > 0.999 {
                            return Some(base);
                        }
                    }
                }
                base += 2;
            }
            if bb.is_some() {
                return bb;
            }
        }
        None
    })?;
    let vertices: Vec<[f32; 3]> = (0..nverts).map(|v| getb(pool, v)).collect();
    Some(MeshShape { vertices, indices })
}

/// Parse a Havok packfile that begins at `pk[0]` (i.e. `pk` starts at, or before,
/// the `__classnames__` section table). Reads little-endian (PC retail).
pub fn parse_packfile(pk: &[u8]) -> Result<Packfile, String> {
    let raw = parse_packfile_raw(pk)?;
    let data_pk = raw.data_pk;
    let lf = &raw.lf;
    let size = raw.size;

    // virtual fixups: object → class name. Decode each shape we recognise.
    let mut shapes = Vec::new();
    let mut class_counts: BTreeMap<String, u32> = BTreeMap::new();
    for (src, cname) in &raw.vfixups {
        let (src, cname) = (*src, cname.as_str());
        *class_counts.entry(cname.to_string()).or_insert(0) += 1;
        let obj = data_pk + src;
        match cname {
            CONVEX => {
                let nv = (u32_le(pk, obj + 76) as usize).min(4096);
                let vptr = data_pk + lf.get(&(src + 64)).copied().unwrap_or(0);
                let mut vertices = Vec::with_capacity(nv);
                for vi in 0..nv {
                    let bo = vptr + (vi / 4) * 48; // FourVectors SoA block
                    let l = (vi % 4) * 4;
                    vertices.push([
                        f32_le(pk, bo + l),
                        f32_le(pk, bo + 16 + l),
                        f32_le(pk, bo + 32 + l),
                    ]);
                }
                let pc = (u32_le(pk, obj + 84) as usize).min(4096);
                let pptr = data_pk + lf.get(&(src + 80)).copied().unwrap_or(0);
                let mut planes = Vec::with_capacity(pc);
                for pi in 0..pc {
                    let po = pptr + pi * 16;
                    planes.push([
                        f32_le(pk, po),
                        f32_le(pk, po + 4),
                        f32_le(pk, po + 8),
                        f32_le(pk, po + 12),
                    ]);
                }
                shapes.push(Shape::Convex(ConvexHull { vertices, planes }));
            }
            "hkpBoxShape" => shapes.push(Shape::Box {
                half_extents: [
                    f32_le(pk, obj + 16),
                    f32_le(pk, obj + 20),
                    f32_le(pk, obj + 24),
                ],
            }),
            "hkpCapsuleShape" => shapes.push(Shape::Capsule(Capsule {
                radius: f32_le(pk, obj + 16),
                vertex_a: [f32_le(pk, obj + 32), f32_le(pk, obj + 36), f32_le(pk, obj + 40)],
                vertex_b: [f32_le(pk, obj + 48), f32_le(pk, obj + 52), f32_le(pk, obj + 56)],
            })),
            "hkpSphereShape" => shapes.push(Shape::Sphere { radius: f32_le(pk, obj + 16) }),
            "hkpMoppBvTreeShape" | "hkpMoppCode" => shapes.push(Shape::Mopp),
            "WpMeshShape16" => shapes.push(Shape::Mesh(
                // Decode against `pk`, which extends past the packfile into the PHY2 wrapper where the
                // quantized vertex pool lives. A failed decode yields an EMPTY mesh so the caller still
                // treats it as an undecoded static mesh (keeps the render fallback) rather than silently
                // dropping the collider.
                decode_mesh_shape16(pk, &raw, src).unwrap_or(MeshShape { vertices: Vec::new(), indices: Vec::new() }),
            )),
            other if other.contains("Shape") => shapes.push(Shape::Other(other.to_string())),
            _ => {} // non-shape class (WpArray, hkRootLevelContainer, …) — counted only
        }
    }

    Ok(Packfile {
        version: raw.version,
        size,
        shapes,
        class_counts,
    })
}

/// ★Uniformly SCALE every convex collision hull in a `PHY2` body, in place.
///
/// Conforming a novel model into a donor container leaves the DONOR's collision behind. If the new
/// model is a different size, the vehicle you SEE and the volume bullets/impacts actually hit
/// disagree — a 2x-scaled tank keeps a half-size hit box and rides on a half-size hull.
///
/// Scaling a `hkpConvexVerticesShape` is exact and size-preserving:
///   * `m_rotatedVertices` (+64, FourVectors SoA: `X[4] Y[4] Z[4]` per 48-byte block) — scale every
///     component. Trailing lanes in the last block are padding (a repeat of the last vertex or 0),
///     and scaling them is harmless.
///   * `m_planeEquations` (+80, `hkVector4 { n.xyz, -support }`) — the normal stays UNIT, only the
///     support distance `w` scales. Scaling `n` would denormalise the half-space and break the
///     narrow-phase.
///
/// Byte size is unchanged, so the packfile's section headers/fixups stay valid and the surrounding
/// PHY2 header + trailing engine wrapper are untouched.
pub fn scale_phy2_hulls(body: &mut [u8], s: f32) -> Result<usize, String> {
    let off = find_sub(body, &HAVOK_MAGIC).ok_or("no embedded Havok packfile (legacy PHY2)")?;
    let raw = parse_packfile_raw(&body[off..])?;
    let (data_pk, lf) = (raw.data_pk, raw.lf);
    let mut scaled = 0usize;
    // Collect the writes first: `raw` borrows `body` immutably through the parse.
    let mut writes: Vec<(usize, f32)> = Vec::new();
    for (src, class) in &raw.vfixups {
        if class != CONVEX {
            continue;
        }
        let obj = off + data_pk + *src;
        let nv = (u32_le(body, obj + 76) as usize).min(4096);
        let vptr = off + data_pk + lf.get(&(*src + 64)).copied().unwrap_or(0);
        // Whole 48-byte SoA blocks, so the padding lanes scale with the real ones.
        let blocks = nv.div_ceil(4);
        for b in 0..blocks {
            for c in 0..12 {
                let o = vptr + b * 48 + c * 4;
                writes.push((o, f32_le(body, o) * s));
            }
        }
        let pc = (u32_le(body, obj + 84) as usize).min(4096);
        let pptr = off + data_pk + lf.get(&(*src + 80)).copied().unwrap_or(0);
        for p in 0..pc {
            // ONLY w (+12): the plane normal must stay unit-length.
            let o = pptr + p * 16 + 12;
            writes.push((o, f32_le(body, o) * s));
        }
        scaled += 1;
    }
    for (o, v) in writes {
        if o + 4 <= body.len() {
            body[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    Ok(scaled)
}

/// Parse a `PHY2` chunk body: the embedded Havok packfile is preceded by a u32
/// header prefix, so the magic is *searched* (mirrors `validate_phy2`). Returns
/// `Err` for a legacy PHY2 with no embedded packfile.
pub fn parse_phy2_body(body: &[u8]) -> Result<Packfile, String> {
    let off = find_sub(body, &HAVOK_MAGIC).ok_or("no embedded Havok packfile (legacy PHY2)")?;
    parse_packfile(&body[off..])
}

/// Find and parse every Havok packfile embedded in an arbitrary buffer (e.g. a
/// decompressed block or model container). Returns `(offset, packfile)` pairs,
/// skipping the bytes each packfile spans so overlapping magics aren't re-parsed.
pub fn find_packfiles(buf: &[u8]) -> Vec<(usize, Packfile)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(rel) = find_sub(&buf[at..], &HAVOK_MAGIC) {
        let off = at + rel;
        match parse_packfile(&buf[off..]) {
            Ok(pf) => {
                at = off + pf.size.max(8);
                out.push((off, pf));
            }
            Err(_) => at = off + 8,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression against the Python reversal on the resident2 up-crate
    /// (`0x81C71C96`): the destructible breaks into 6 pieces, so its PHY2
    /// packfile holds 6 `hkpConvexVerticesShape` hulls with these exact vertex
    /// counts and real O(1)-metre coordinates — NOT the heuristic's garbage.
    #[test]
    fn crate_phy2_decodes_six_break_piece_hulls() {
        let body = include_bytes!("../tests/fixtures/phy2_crate_le.bin");
        let pf = parse_phy2_body(body).expect("parse crate PHY2 body");

        assert!(
            pf.version.starts_with("Havok-5.5"),
            "version = {:?}",
            pf.version
        );
        assert_eq!(
            pf.class_counts.get(CONVEX),
            Some(&6),
            "six break-piece hulls"
        );

        let counts: Vec<usize> = pf.hulls().map(|h| h.vertices.len()).collect();
        assert_eq!(counts, vec![19, 24, 35, 12, 36, 10], "hull vertex counts");
        let plane_counts: Vec<usize> = pf.hulls().map(|h| h.planes.len()).collect();
        assert_eq!(
            plane_counts,
            vec![12, 15, 22, 8, 22, 7],
            "hull plane counts"
        );

        // first vertex of hull0 — real coordinates, not a denormal byte-scan hit.
        let v0 = pf.hulls().next().unwrap().vertices[0];
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(
            near(v0[0], -0.0783) && near(v0[1], -0.3693) && near(v0[2], 0.3575),
            "hull0 v0 = {v0:?}"
        );
        // every coordinate is plausibly within a couple of metres — the property
        // the heuristic violated (it emitted 1e-45 denormals and -2048).
        for h in pf.hulls() {
            for v in &h.vertices {
                assert!(
                    v.iter().all(|c| c.is_finite() && c.abs() < 8.0),
                    "implausible hull vertex {v:?}"
                );
            }
        }
    }

    #[test]
    fn legacy_phy2_without_packfile_errs() {
        assert!(parse_phy2_body(&[0u8; 64]).is_err());
    }

    #[test]
    fn find_packfiles_locates_the_crate_packfile() {
        let body = include_bytes!("../tests/fixtures/phy2_crate_le.bin");
        let found = find_packfiles(body);
        assert_eq!(found.len(), 1, "one embedded packfile");
        assert_eq!(found[0].1.hulls().count(), 6);
    }

    /// The 11 human-ragdoll `hkpCapsuleShape` bodies, decoded from the packfile carved out of retail
    /// `vz.wad` block 3185 (the resident human/animation block). Pins the `+16`/`+32`/`+48` layout
    /// against real bytes and records the exact recovered per-body radius + half-length.
    #[test]
    fn ragdoll_capsule_layout_decodes_from_block3185_fixture() {
        let body = include_bytes!("../tests/fixtures/ragdoll_capsules_le.bin");
        let pf = parse_packfile(body).expect("parse ragdoll capsule packfile");
        assert!(pf.version.starts_with("Havok-5.5"), "version {:?}", pf.version);
        assert_eq!(
            pf.class_counts.get("hkpCapsuleShape"),
            Some(&11),
            "the human ragdoll has 11 capsule bodies"
        );

        let caps = human_ragdoll_capsules(body);
        assert_eq!(caps.len(), 11);

        // Every capsule is a segment on local Y, symmetric about the origin (vertexA=+h, vertexB=-h).
        for c in &caps {
            assert!(c.vertex_a[0].abs() < 1e-4 && c.vertex_a[2].abs() < 1e-4, "A off-Y: {:?}", c.vertex_a);
            assert!(c.vertex_b[0].abs() < 1e-4 && c.vertex_b[2].abs() < 1e-4, "B off-Y: {:?}", c.vertex_b);
            assert!((c.vertex_a[1] + c.vertex_b[1]).abs() < 1e-4, "not centred: {:?}/{:?}", c.vertex_a, c.vertex_b);
            assert!(c.radius > 0.0 && c.radius < 0.5, "implausible radius {}", c.radius);
        }

        // Exact recovered (radius, half_len) multiset — the faithful per-body dimensions.
        let mut got: Vec<(f32, f32)> = caps.iter().map(|c| (c.radius, c.half_len())).collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut want: Vec<(f32, f32)> = vec![
            (0.1194, 0.1148), (0.0914, 0.1088), (0.1194, 0.1148), (0.0914, 0.1088),
            (0.0750, 0.0815), (0.0750, 0.0867), (0.0750, 0.0929), (0.0750, 0.0867),
            (0.0999, 0.0297), (0.1700, 0.0188), (0.1449, 0.0490),
        ];
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (g, w) in got.iter().zip(&want) {
            assert!((g.0 - w.0).abs() < 5e-4 && (g.1 - w.1).abs() < 5e-4, "capsule {g:?} != {w:?}");
        }

        // The head is the fattest, shortest body; the thighs are the thickest limbs.
        let head = caps.iter().max_by(|a, b| a.radius.partial_cmp(&b.radius).unwrap()).unwrap();
        assert!((head.radius - 0.1700).abs() < 5e-4, "head radius {}", head.radius);
    }

    /// Live decode of a real BUILDING `WpMeshShape16` from retail `vz.wad`. Block 767 carries a small
    /// building collider (~396 tris, ~25 m XZ span, verified by the meshshape probe). Confirms the port:
    /// non-empty index triples, dequantized verts within a sane building-scale bbox, and — the property
    /// the quantized-pool scan exists to guarantee — the vast majority of triangles have all edges < 40 m.
    /// SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn building_wpmesh16_decodes_live_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("skip: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("skip: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let dec = decompress_block(&mut f, &arch.indx, 767).expect("decompress block 767");

        // Every Havok packfile in the block; keep the decoded WpMeshShape16 meshes.
        let meshes: Vec<MeshShape> = find_packfiles(&dec)
            .into_iter()
            .flat_map(|(_off, pf)| pf.shapes.into_iter())
            .filter_map(|s| match s {
                Shape::Mesh(m) => Some(m),
                _ => None,
            })
            .collect();
        assert!(!meshes.is_empty(), "block 767 must carry at least one WpMeshShape16");

        // A BUILDING-scale mesh: non-empty, small XZ span, well-formed triangles.
        let bldg = meshes
            .iter()
            .find(|m| !m.indices.is_empty() && m.xz_span().iter().all(|&s| s < 300.0))
            .expect("a decoded building-scale mesh in block 767");
        assert!(bldg.indices.len() > 0, "building mesh has triangles: {}", bldg.indices.len());
        assert!(!bldg.vertices.is_empty(), "building mesh has vertices");

        // Verts finite and within a plausible model/world bbox; indices in range.
        let nv = bldg.vertices.len();
        for v in &bldg.vertices {
            assert!(v.iter().all(|c| c.is_finite()), "non-finite vertex {v:?}");
            assert!(v.iter().all(|c| c.abs() < 100_000.0), "implausibly far vertex {v:?}");
        }
        for t in &bldg.indices {
            assert!(t.iter().all(|&i| (i as usize) < nv), "index out of range {t:?} (nv={nv})");
        }
        // The dequantization is correct when nearly every triangle has sane (<40 m) edges.
        let edge = |p: [f32; 3], q: [f32; 3]| {
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        let sane = bldg
            .indices
            .iter()
            .filter(|t| {
                let (a, b, c) = (bldg.vertices[t[0] as usize], bldg.vertices[t[1] as usize], bldg.vertices[t[2] as usize]);
                edge(a, b) < 40.0 && edge(b, c) < 40.0 && edge(a, c) < 40.0
            })
            .count();
        let frac = sane as f64 / bldg.indices.len() as f64;
        assert!(frac > 0.9, "only {frac:.3} of building-mesh tris well-formed (pool base mis-scanned?)");
        eprintln!(
            "block 767 building WpMeshShape16: {} tris {} verts, XZ span {:?}, {:.1}% sane-edge tris",
            bldg.indices.len(), nv, bldg.xz_span(), frac * 100.0
        );
    }

    /// Live decode of a PREVIOUSLY-FAILING `WpMeshShape16` from retail `vz.wad`, exercising the guarded
    /// whole-slice FALLBACK. Block 826 container `0xF589CA67` carries a terrain-scale cell mesh (~4.5k verts)
    /// whose vertex pool sits at the very end of the PHY2 chunk with a large first triangle — the wrapper
    /// scan's `tris[0]`/6-probe gate skipped its correct base, so the old decoder returned an EMPTY mesh and
    /// the whole cell lost its authored collision. It must now decode: non-empty, finite, every vertex within
    /// the quantization range `[min, min + scale*65535]`, and the vast majority of triangles well-formed
    /// (< 40 m edges). SKIPS (stays green) when `vz.wad` is absent.
    #[test]
    fn recovered_terrain_wpmesh16_decodes_live_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        use crate::ucfx::{extract_chunk_body, parse_block_entry_table};
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("skip: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("skip: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let dec = decompress_block(&mut f, &arch.indx, 826).expect("decompress block 826");

        // Walk to container 0xF589CA67 exactly as the streaming loader does.
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut body: Option<Vec<u8>> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if end > dec.len() {
                break;
            }
            if e.name_hash == 0xF589_CA67 {
                body = extract_chunk_body(&dec[pos..end], b"PHY2");
                break;
            }
            pos = end;
        }
        let body = body.expect("block 826 container 0xF589CA67 carries a PHY2 chunk");

        // Recover this mesh object's own quantization block (min/scale @ obj+48/+64) for the range check.
        let off = find_sub(&body, &HAVOK_MAGIC).expect("embedded Havok packfile");
        let pk = &body[off..];
        let raw = parse_packfile_raw(pk).expect("parse packfile");
        let (mut min, mut scale) = ([0.0f32; 3], [0.0f32; 3]);
        for (src, cname) in &raw.vfixups {
            if cname == "WpMeshShape16" {
                let sp0 = raw.data_pk + src + 48;
                min = [f32_le(pk, sp0), f32_le(pk, sp0 + 4), f32_le(pk, sp0 + 8)];
                scale = [f32_le(pk, sp0 + 16), f32_le(pk, sp0 + 20), f32_le(pk, sp0 + 24)];
                break;
            }
        }

        // Decode and grab the terrain-scale mesh (the big one this test is about).
        let pf = parse_phy2_body(&body).expect("parse PHY2");
        let mesh = pf
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Mesh(m) if m.vertices.len() > 3000 => Some(m),
                _ => None,
            })
            .max_by_key(|m| m.vertices.len())
            .expect("block 826 must now decode its terrain-scale WpMeshShape16 (was empty → collision lost)");
        assert!(!mesh.indices.is_empty(), "recovered mesh has triangles");
        assert!(!mesh.vertices.is_empty(), "recovered mesh has vertices");

        // Every vertex lies within the quantization range [min, min + scale*65535], per-axis.
        let hi = [
            min[0] + scale[0] * 65535.0,
            min[1] + scale[1] * 65535.0,
            min[2] + scale[2] * 65535.0,
        ];
        for v in &mesh.vertices {
            for k in 0..3 {
                assert!(v[k].is_finite(), "non-finite vertex {v:?}");
                assert!(
                    v[k] >= min[k] - 1e-3 && v[k] <= hi[k] + 1e-3,
                    "vertex axis {k} = {} out of quant range [{}, {}]",
                    v[k],
                    min[k],
                    hi[k]
                );
            }
        }
        // Dequantization correct → nearly every triangle well-formed (< 40 m edges).
        let nv = mesh.vertices.len();
        for t in &mesh.indices {
            assert!(t.iter().all(|&i| (i as usize) < nv), "index out of range {t:?}");
        }
        let edge = |p: [f32; 3], q: [f32; 3]| {
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
        };
        let sane = mesh
            .indices
            .iter()
            .filter(|t| {
                let (a, b, c) = (mesh.vertices[t[0] as usize], mesh.vertices[t[1] as usize], mesh.vertices[t[2] as usize]);
                edge(a, b) < 40.0 && edge(b, c) < 40.0 && edge(a, c) < 40.0
            })
            .count();
        let frac = sane as f64 / mesh.indices.len() as f64;
        assert!(frac > 0.9, "only {frac:.3} of recovered terrain-mesh tris well-formed");
        eprintln!(
            "block 826 recovered WpMeshShape16: {} tris {} verts, XZ span {:?}, {:.1}% sane-edge",
            mesh.indices.len(), nv, mesh.xz_span(), frac * 100.0
        );
    }

    /// Live re-decode from the retail WAD: block 3185 yields exactly 11 ragdoll capsules matching the
    /// fixture. SKIPS (stays green) when `vz.wad` is absent — same pattern as the anim live test.
    #[test]
    fn ragdoll_capsules_live_from_vz_wad_if_present() {
        use crate::ffcs::load_ffcs_archive;
        use crate::sges::decompress_block;
        let Some(path) = crate::game_paths::vz_wad_from_env()
            .or_else(|| crate::game_paths::wad_from_local_config(std::path::Path::new(".")))
        else {
            return eprintln!("skip: vz.wad not found");
        };
        let Ok(mut f) = std::fs::File::open(&path) else {
            return eprintln!("skip: vz.wad not readable");
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");
        let dec = decompress_block(&mut f, &arch.indx, 3185).expect("decompress block 3185");
        let caps = human_ragdoll_capsules(&dec);
        assert_eq!(caps.len(), 11, "block 3185 must carry the 11-body human ragdoll");
        assert!(caps.iter().any(|c| (c.radius - 0.1700).abs() < 5e-4), "head capsule present");
    }
}
