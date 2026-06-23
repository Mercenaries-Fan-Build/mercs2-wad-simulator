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

/// A collision shape recovered from a packfile.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// `hkpConvexVerticesShape` — a convex break-piece hull.
    Convex(ConvexHull),
    /// `hkpBoxShape` — half-extents (best-effort: m_halfExtents @ +16).
    Box { half_extents: [f32; 3] },
    /// `hkpMoppBvTreeShape` / `hkpMoppCode` — static non-convex mesh BV-tree.
    Mopp,
    /// `WpMeshShape16` — Pandemic 16-bit-indexed static collision mesh.
    Mesh,
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
}

/// Parse a Havok packfile that begins at `pk[0]` (i.e. `pk` starts at, or before,
/// the `__classnames__` section table). Reads little-endian (PC retail).
pub fn parse_packfile(pk: &[u8]) -> Result<Packfile, String> {
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

    // virtual fixups: object → class name. Decode each shape we recognise.
    let mut shapes = Vec::new();
    let mut class_counts: BTreeMap<String, u32> = BTreeMap::new();
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
        *class_counts.entry(cname.clone()).or_insert(0) += 1;
        let obj = data_pk + src;
        match cname.as_str() {
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
            "hkpMoppBvTreeShape" | "hkpMoppCode" => shapes.push(Shape::Mopp),
            "WpMeshShape16" => shapes.push(Shape::Mesh),
            other if other.contains("Shape") => shapes.push(Shape::Other(other.to_string())),
            _ => {} // non-shape class (WpArray, hkRootLevelContainer, …) — counted only
        }
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

    Ok(Packfile {
        version,
        size,
        shapes,
        class_counts,
    })
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

        assert!(pf.version.starts_with("Havok-5.5"), "version = {:?}", pf.version);
        assert_eq!(pf.class_counts.get(CONVEX), Some(&6), "six break-piece hulls");

        let counts: Vec<usize> = pf.hulls().map(|h| h.vertices.len()).collect();
        assert_eq!(counts, vec![19, 24, 35, 12, 36, 10], "hull vertex counts");
        let plane_counts: Vec<usize> = pf.hulls().map(|h| h.planes.len()).collect();
        assert_eq!(plane_counts, vec![12, 15, 22, 8, 22, 7], "hull plane counts");

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
}
