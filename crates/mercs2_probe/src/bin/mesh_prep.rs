//! `mesh_prep` — turn a modder's high-poly OBJ into a **game-ready rigid GLB** for the
//! `add_model` path (`mercs2_formats::mesh_import::external_mesh_from_gltf`).
//!
//! The rigid injector wants exactly four streams — POSITION, NORMAL, TEXCOORD_0 and triangulated
//! indices — and it takes the glTF positions VERBATIM (no axis flip, no rescale: `add_model` calls
//! `inject_static_into_donor_block(.., scale = 1.0, ..)` with "the mesh carries its own transform").
//! `ExternalMesh` is documented as "baked to donor frame: Y-up, feet at Y=0". So a game-ready GLB
//! must be:
//!   * **Y-up** (deck faces +Y) — glTF up == game up == +Y,
//!   * resting on **Y = 0** (hull bottom on the ground plane), and
//!   * at **final metric scale** (game unit = 1 metre, proven in `docs/coordinate_systems.md`:
//!     the placement pipeline multiplies game units by 100 to reach UE centimetres).
//!
//! Because a rigid host group uses u16 indices (hard cap 65 535 verts), the mesh is decimated by
//! **vertex clustering**: quantise positions onto a grid, collapse each occupied cell to its
//! centroid, rebuild the faces, drop degenerate/zero-area tris. The cell size is auto-tuned to land
//! under a vertex budget. Normals are recomputed (clustering invalidates the source normals), a
//! dummy zero TEXCOORD_0 is emitted (the prop wears the donor's material this phase), and a
//! self-contained binary `.glb` is written by hand (JSON header + BIN chunk — no new dep).
//!
//! It can also render orthographic preview PNGs (front / side / top / three-quarter, with a 1 m
//! ground grid at Y=0 and a 1.8 m human-height reference bar) so orientation and scale can be
//! *seen*, and `--verify` re-reads the GLB through the real rigid reader to prove Phase-2 ingest.
//!
//! ```text
//! mercs2_probe (bin mesh_prep) --in boat.obj --out boat.glb \
//!     --up z --fit-length 13.0 --budget 55000 --render preview --verify
//! ```

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

// ------------------------------------------------------------------ geometry

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

struct Mesh {
    pos: Vec<V3>,
    tris: Vec<[u32; 3]>,
}

impl Mesh {
    fn bbox(&self) -> (V3, V3) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }
}

// ------------------------------------------------------------------ OBJ read

/// Minimal OBJ reader. Keeps positions only (normals are recomputed after decimation, UVs are
/// replaced by a dummy stream). `f` lines may be POLYGONS — fan-triangulated `v0,vi,vi+1`.
/// OBJ indices are 1-based; negative indices are relative to the current vertex count.
fn read_obj(path: &Path) -> Result<Mesh, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut pos: Vec<V3> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();

    let parse_idx = |tok: &str, count: usize| -> Option<u32> {
        // token is "v", "v/vt", "v/vt/vn" or "v//vn" — we want the FIRST (position) field.
        let first = tok.split('/').next()?;
        let raw: i64 = first.parse().ok()?;
        let idx = if raw < 0 {
            count as i64 + raw
        } else {
            raw - 1
        };
        if idx < 0 || idx as usize >= count {
            None
        } else {
            Some(idx as u32)
        }
    };

    for line in text.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("v ") {
            let mut it = rest.split_whitespace();
            let x = it.next().and_then(|s| s.parse().ok());
            let y = it.next().and_then(|s| s.parse().ok());
            let z = it.next().and_then(|s| s.parse().ok());
            if let (Some(x), Some(y), Some(z)) = (x, y, z) {
                pos.push([x, y, z]);
            } else {
                return Err(format!("bad vertex line: {line}"));
            }
        } else if let Some(rest) = line.strip_prefix("f ") {
            let verts: Vec<u32> = rest
                .split_whitespace()
                .filter_map(|t| parse_idx(t, pos.len()))
                .collect();
            if verts.len() >= 3 {
                for i in 1..verts.len() - 1 {
                    tris.push([verts[0], verts[i], verts[i + 1]]);
                }
            }
        }
    }
    if pos.is_empty() {
        return Err(format!("{}: no vertices", path.display()));
    }
    if tris.is_empty() {
        return Err(format!("{}: no faces", path.display()));
    }
    Ok(Mesh { pos, tris })
}

// ------------------------------------------------------------- decimation

/// Vertex-cluster decimation. Quantise positions onto a uniform grid of `cell`-sized cells, collapse
/// every occupied cell to the CENTROID of the source verts that fell in it, remap the faces, drop
/// tris that became degenerate (two shared corners) or zero-area. Returns the decimated mesh.
fn cluster_decimate(m: &Mesh, cell: f32) -> Mesh {
    let inv = 1.0 / cell;
    let key = |p: &V3| -> (i64, i64, i64) {
        (
            (p[0] * inv).floor() as i64,
            (p[1] * inv).floor() as i64,
            (p[2] * inv).floor() as i64,
        )
    };
    // cell -> (representative index, accumulated sum, count)
    let mut cells: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut sum: Vec<V3> = Vec::new();
    let mut cnt: Vec<u32> = Vec::new();
    let mut remap: Vec<u32> = Vec::with_capacity(m.pos.len());
    for p in &m.pos {
        let k = key(p);
        let idx = *cells.entry(k).or_insert_with(|| {
            sum.push([0.0; 3]);
            cnt.push(0);
            (sum.len() - 1) as u32
        });
        let i = idx as usize;
        sum[i] = [sum[i][0] + p[0], sum[i][1] + p[1], sum[i][2] + p[2]];
        cnt[i] += 1;
        remap.push(idx);
    }
    let pos: Vec<V3> = sum
        .iter()
        .zip(&cnt)
        .map(|(s, &c)| {
            let c = c.max(1) as f32;
            [s[0] / c, s[1] / c, s[2] / c]
        })
        .collect();

    let mut tris: Vec<[u32; 3]> = Vec::with_capacity(m.tris.len());
    for t in &m.tris {
        let a = remap[t[0] as usize];
        let b = remap[t[1] as usize];
        let c = remap[t[2] as usize];
        if a == b || b == c || a == c {
            continue; // collapsed to a line/point
        }
        let area2 = cross(sub(pos[b as usize], pos[a as usize]), sub(pos[c as usize], pos[a as usize]));
        if dot(area2, area2) <= 1e-20 {
            continue; // zero-area
        }
        tris.push([a, b, c]);
    }
    Mesh { pos, tris }
}

/// Pick a cell size that lands the decimated vertex count under `budget` (and reasonably close to
/// it). Binary-searches cell size: larger cell => fewer verts. Returns (cell, decimated).
fn decimate_to_budget(m: &Mesh, budget: usize) -> (f32, Mesh) {
    let (lo, hi) = m.bbox();
    let diag = ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2) + (hi[2] - lo[2]).powi(2)).sqrt();
    // If already under budget, a tiny cell (near-lossless weld) is fine.
    let mut cell_lo = diag / 4096.0; // fine  -> many verts
    let mut cell_hi = diag / 4.0; // coarse -> few verts
    // Ensure the coarse bound actually undershoots the budget; grow it if not.
    for _ in 0..24 {
        let d = cluster_decimate(m, cell_hi);
        if d.pos.len() <= budget {
            break;
        }
        cell_hi *= 1.5;
    }
    // If even the finest weld is already under budget, use it.
    let fine = cluster_decimate(m, cell_lo);
    if fine.pos.len() <= budget {
        return (cell_lo, fine);
    }
    // Binary search for the smallest cell (most detail) that still fits the budget.
    let mut best_cell = cell_hi;
    let mut best = cluster_decimate(m, cell_hi);
    for _ in 0..40 {
        let mid = (cell_lo * cell_hi).sqrt(); // geometric midpoint
        let d = cluster_decimate(m, mid);
        if d.pos.len() <= budget {
            // fits: try to keep more detail (smaller cell)
            best_cell = mid;
            best = d;
            cell_hi = mid;
        } else {
            cell_lo = mid;
        }
        if (cell_hi / cell_lo) < 1.01 {
            break;
        }
    }
    (best_cell, best)
}

// ------------------------------------------------------------- transform

/// One output-axis mapping: which input axis (0=x,1=y,2=z) and its sign.
#[derive(Clone, Copy)]
struct AxisMap {
    src: [usize; 3],
    sgn: [f32; 3],
}

impl AxisMap {
    fn identity() -> Self {
        AxisMap {
            src: [0, 1, 2],
            sgn: [1.0, 1.0, 1.0],
        }
    }
    /// Parse "x,z,-y" -> output.x=in.x, output.y=in.z, output.z=-in.y.
    fn parse(spec: &str) -> Result<Self, String> {
        let parts: Vec<&str> = spec.split(',').map(|s| s.trim()).collect();
        if parts.len() != 3 {
            return Err(format!("--axis-map needs 3 comma-separated axes, got {spec:?}"));
        }
        let mut src = [0usize; 3];
        let mut sgn = [1.0f32; 3];
        for (o, p) in parts.iter().enumerate() {
            let (s, letter) = if let Some(r) = p.strip_prefix('-') {
                (-1.0, r)
            } else {
                (1.0, p.strip_prefix('+').unwrap_or(p))
            };
            src[o] = match letter {
                "x" | "X" => 0,
                "y" | "Y" => 1,
                "z" | "Z" => 2,
                other => return Err(format!("bad axis {other:?} in --axis-map")),
            };
            sgn[o] = s;
        }
        Ok(AxisMap { src, sgn })
    }
    /// Convenience: given which INPUT axis is UP, build the map that rotates it onto +Y while
    /// keeping input X as output X (a single -90°/+90° rotation, preserving orientation).
    fn from_up(up: &str) -> Result<Self, String> {
        Ok(match up {
            "y" | "+y" | "Y" => Self::identity(),
            "-y" => Self::parse("x,-y,z")?,
            // Max Z-up -> Y-up: (x,y,z) -> (x, z, -y)
            "z" | "+z" | "Z" => Self::parse("x,z,-y")?,
            "-z" => Self::parse("x,-z,y")?,
            // X-up (rare): (x,y,z) -> (y, x, z) style — send +X to +Y.
            "x" | "+x" | "X" => Self::parse("y,x,z")?,
            "-x" => Self::parse("-y,x,z")?,
            other => return Err(format!("--up must be x|y|z|-x|-y|-z, got {other:?}")),
        })
    }
    fn apply(&self, p: V3) -> V3 {
        [
            self.sgn[0] * p[self.src[0]],
            self.sgn[1] * p[self.src[1]],
            self.sgn[2] * p[self.src[2]],
        ]
    }
}

// ------------------------------------------------------------- normals

/// Area-weighted vertex normals recomputed from the decimated topology.
fn recompute_normals(pos: &[V3], tris: &[[u32; 3]]) -> Vec<V3> {
    let mut n = vec![[0.0f32; 3]; pos.len()];
    for t in tris {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let fn_ = cross(sub(pos[b], pos[a]), sub(pos[c], pos[a])); // length ∝ 2·area
        for &i in &[a, b, c] {
            n[i] = [n[i][0] + fn_[0], n[i][1] + fn_[1], n[i][2] + fn_[2]];
        }
    }
    n.iter().map(|&v| norm(v)).collect()
}

// ------------------------------------------------------------- GLB writer

/// Write a self-contained binary glTF (POSITION + NORMAL + TEXCOORD_0 + u32 indices, one primitive).
fn write_glb(path: &Path, pos: &[V3], nrm: &[V3], uv: &[[f32; 2]], tris: &[[u32; 3]]) -> Result<(), String> {
    let v = pos.len();
    let mut bin: Vec<u8> = Vec::with_capacity(v * 32 + tris.len() * 12);
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
        bin.extend_from_slice(&p[0].to_le_bytes());
        bin.extend_from_slice(&p[1].to_le_bytes());
        bin.extend_from_slice(&p[2].to_le_bytes());
    }
    let nrm_off = bin.len();
    for n in nrm {
        for c in n {
            bin.extend_from_slice(&c.to_le_bytes());
        }
    }
    let uv_off = bin.len();
    for t in uv {
        bin.extend_from_slice(&t[0].to_le_bytes());
        bin.extend_from_slice(&t[1].to_le_bytes());
    }
    let idx_off = bin.len();
    for t in tris {
        for i in t {
            bin.extend_from_slice(&i.to_le_bytes());
        }
    }
    let idx_count = tris.len() * 3;

    let json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "mercs2_probe mesh_prep" },
        "scene": 0,
        "scenes": [ { "nodes": [0] } ],
        "nodes": [ { "mesh": 0, "name": "prop" } ],
        "meshes": [ { "primitives": [ {
            "attributes": { "POSITION": 0, "NORMAL": 1, "TEXCOORD_0": 2 },
            "indices": 3,
            "mode": 4
        } ] } ],
        "buffers": [ { "byteLength": bin.len() } ],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0,       "byteLength": v * 12, "target": 34962 },
            { "buffer": 0, "byteOffset": nrm_off, "byteLength": v * 12, "target": 34962 },
            { "buffer": 0, "byteOffset": uv_off,  "byteLength": v * 8,  "target": 34962 },
            { "buffer": 0, "byteOffset": idx_off, "byteLength": idx_count * 4, "target": 34963 }
        ],
        "accessors": [
            { "bufferView": 0, "componentType": 5126, "count": v, "type": "VEC3",
              "min": [lo[0], lo[1], lo[2]], "max": [hi[0], hi[1], hi[2]] },
            { "bufferView": 1, "componentType": 5126, "count": v, "type": "VEC3" },
            { "bufferView": 2, "componentType": 5126, "count": v, "type": "VEC2" },
            { "bufferView": 3, "componentType": 5125, "count": idx_count, "type": "SCALAR" }
        ]
    });
    let mut json_bytes = serde_json::to_vec(&json).map_err(|e| e.to_string())?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out: Vec<u8> = Vec::with_capacity(total);
    out.extend_from_slice(&0x4654_6C67u32.to_le_bytes()); // "glTF"
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // "JSON"
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // "BIN\0"
    out.extend_from_slice(&bin);
    std::fs::write(path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

// ------------------------------------------------------------- rasterizer

/// A tiny orthographic software rasterizer: shaded z-buffered triangles + a 1 m ground grid at Y=0
/// and a 1.8 m human-height reference bar, so orientation and scale can be seen. `right`/`up` are
/// the screen axes (world dirs); the view direction is right×up.
#[allow(clippy::too_many_arguments)]
fn render_view(
    path: &Path,
    size: u32,
    pos: &[V3],
    nrm: &[V3],
    tris: &[[u32; 3]],
    right: V3,
    up: V3,
    label: &str,
) -> Result<(), String> {
    let right = norm(right);
    let up = norm(up);
    let view = norm(cross(right, up)); // depth axis (into screen = +view)
    let w = size as usize;
    let h = size as usize;
    let mut color = vec![24u8; w * h * 3]; // dark background
    let mut depth = vec![f32::MAX; w * h];

    // World bbox (include a 1.8 m reference and grid extent) to fit the ortho frame.
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    // Reference human stands just off the +X side of the hull.
    let ref_x = hi[0] + 1.5;
    for k in 0..3 {
        lo[k] = lo[k].min([ref_x, 0.0, 0.0][k]);
        hi[k] = hi[k].max([ref_x + 0.5, 1.8, 0.5][k]);
    }
    let center = [
        (lo[0] + hi[0]) * 0.5,
        (lo[1] + hi[1]) * 0.5,
        (lo[2] + hi[2]) * 0.5,
    ];
    let project = |p: V3| -> (f32, f32) {
        let d = sub(p, center);
        (dot(d, right), dot(d, up))
    };
    // ortho half-extent from projected span, with margin.
    let mut ext = 0.0f32;
    for p in [
        [lo[0], lo[1], lo[2]],
        [hi[0], lo[1], lo[2]],
        [lo[0], hi[1], lo[2]],
        [hi[0], hi[1], lo[2]],
        [lo[0], lo[1], hi[2]],
        [hi[0], lo[1], hi[2]],
        [lo[0], hi[1], hi[2]],
        [hi[0], hi[1], hi[2]],
    ] {
        let (u, v) = project(p);
        ext = ext.max(u.abs()).max(v.abs());
    }
    ext *= 1.1;
    let scale = (size as f32 * 0.5) / ext.max(1e-3);
    let to_px = |p: V3| -> (f32, f32, f32) {
        let (u, v) = project(p);
        let sx = w as f32 * 0.5 + u * scale;
        let sy = h as f32 * 0.5 - v * scale; // flip Y for image space
        let dep = dot(sub(p, center), view);
        (sx, sy, dep)
    };

    // Light: from upper-front.
    let light = norm([0.4, 0.8, 0.6]);

    let put = |x: i32, y: i32, dep: f32, rgb: [u8; 3], zbuf: bool, buf_c: &mut [u8], buf_d: &mut [f32]| {
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
            return;
        }
        let i = y as usize * w + x as usize;
        if zbuf {
            if dep < buf_d[i] {
                buf_d[i] = dep;
                buf_c[i * 3..i * 3 + 3].copy_from_slice(&rgb);
            }
        } else {
            buf_c[i * 3..i * 3 + 3].copy_from_slice(&rgb);
        }
    };

    // Draw the 1 m ground grid at Y=0 first (no z-test; it is scenery under the hull).
    let grid_min = lo[0].floor() as i32 - 1;
    let grid_max = hi[0].ceil() as i32 + 1;
    let grid_zmin = lo[2].floor() as i32 - 1;
    let grid_zmax = hi[2].ceil() as i32 + 1;
    let line = |a: V3, b: V3, rgb: [u8; 3], buf_c: &mut [u8], buf_d: &mut [f32]| {
        let (ax, ay, _) = to_px(a);
        let (bx, by, _) = to_px(b);
        let steps = ((bx - ax).abs().max((by - ay).abs()) as i32).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = (ax + (bx - ax) * t).round() as i32;
            let y = (ay + (by - ay) * t).round() as i32;
            put(x, y, 0.0, rgb, false, buf_c, buf_d);
        }
    };
    for gx in grid_min..=grid_max {
        line([gx as f32, 0.0, grid_zmin as f32], [gx as f32, 0.0, grid_zmax as f32], [50, 50, 60], &mut color, &mut depth);
    }
    for gz in grid_zmin..=grid_zmax {
        line([grid_min as f32, 0.0, gz as f32], [grid_max as f32, 0.0, gz as f32], [50, 50, 60], &mut color, &mut depth);
    }
    // 1.8 m human-height reference bar (bright green), off the +X side.
    line([ref_x, 0.0, 0.25], [ref_x, 1.8, 0.25], [40, 220, 40], &mut color, &mut depth);
    line([ref_x - 0.15, 1.8, 0.25], [ref_x + 0.15, 1.8, 0.25], [40, 220, 40], &mut color, &mut depth);
    line([ref_x - 0.15, 0.0, 0.25], [ref_x + 0.15, 0.0, 0.25], [40, 220, 40], &mut color, &mut depth);

    // Rasterize triangles (barycentric fill, z-buffered, Lambert + ambient).
    for t in tris {
        let p0 = pos[t[0] as usize];
        let p1 = pos[t[1] as usize];
        let p2 = pos[t[2] as usize];
        let fnrm = norm(cross(sub(p1, p0), sub(p2, p0)));
        let lambert = dot(fnrm, light).abs(); // abs: two-sided shading for open hulls
        let shade = 0.25 + 0.75 * lambert;
        // Base grey tinted slightly by facing to add relief.
        let base = 200.0 * shade;
        let rgb = [
            (base * 0.85).clamp(0.0, 255.0) as u8,
            (base * 0.9).clamp(0.0, 255.0) as u8,
            (base).clamp(0.0, 255.0) as u8,
        ];
        let (x0, y0, d0) = to_px(p0);
        let (x1, y1, d1) = to_px(p1);
        let (x2, y2, d2) = to_px(p2);
        let minx = x0.min(x1).min(x2).floor().max(0.0) as i32;
        let maxx = x0.max(x1).max(x2).ceil().min(w as f32 - 1.0) as i32;
        let miny = y0.min(y1).min(y2).floor().max(0.0) as i32;
        let maxy = y0.max(y1).max(y2).ceil().min(h as f32 - 1.0) as i32;
        let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
        if area.abs() < 1e-6 {
            continue;
        }
        let inv = 1.0 / area;
        for y in miny..=maxy {
            for x in minx..=maxx {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((x1 - px) * (y2 - py) - (x2 - px) * (y1 - py)) * inv;
                let w1 = ((x2 - px) * (y0 - py) - (x0 - px) * (y2 - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let dep = w0 * d0 + w1 * d1 + w2 * d2;
                put(x, y, dep, rgb, true, &mut color, &mut depth);
            }
        }
    }
    let _ = (nrm, label);
    write_png(path, w as u32, h as u32, &color)
}

fn write_png(path: &Path, w: u32, h: u32, rgb: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgb).map_err(|e| e.to_string())?;
    Ok(())
}

// ------------------------------------------------------------------ main

struct Args {
    inp: PathBuf,
    out: PathBuf,
    budget: usize,
    cell: Option<f32>,
    scale: Option<f32>,
    fit_length: Option<f32>,
    axis: AxisMap,
    recenter_y: bool,
    center_xz: bool,
    render: Option<String>,
    render_size: u32,
    verify: bool,
}

fn parse_args() -> Result<Args, String> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let get = |name: &str| -> Option<String> {
        a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).cloned()
    };
    let has = |name: &str| a.iter().any(|x| x == name);
    let inp = get("--in").ok_or("missing --in <obj>")?;
    let out = get("--out").ok_or("missing --out <glb>")?;
    let axis = match (get("--axis-map"), get("--up")) {
        (Some(m), _) => AxisMap::parse(&m)?,
        (None, Some(u)) => AxisMap::from_up(&u)?,
        (None, None) => AxisMap::identity(),
    };
    Ok(Args {
        inp: PathBuf::from(inp),
        out: PathBuf::from(out),
        budget: get("--budget").and_then(|s| s.parse().ok()).unwrap_or(60_000),
        cell: get("--cell").and_then(|s| s.parse().ok()),
        scale: get("--scale").and_then(|s| s.parse().ok()),
        fit_length: get("--fit-length").and_then(|s| s.parse().ok()),
        axis,
        recenter_y: !has("--no-recenter-y"),
        center_xz: !has("--no-center-xz"),
        render: get("--render"),
        render_size: get("--render-size").and_then(|s| s.parse().ok()).unwrap_or(900),
        verify: has("--verify"),
    })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mesh_prep: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let raw = read_obj(&args.inp)?;
    let (rlo, rhi) = raw.bbox();
    println!(
        "read {}: {} verts, {} tris (triangulated), bbox [{:.4},{:.4},{:.4}]..[{:.4},{:.4},{:.4}]",
        args.inp.display(),
        raw.pos.len(),
        raw.tris.len(),
        rlo[0], rlo[1], rlo[2], rhi[0], rhi[1], rhi[2]
    );

    // 1) decimate
    let (cell, mut dec) = if let Some(c) = args.cell {
        (c, cluster_decimate(&raw, c))
    } else {
        decimate_to_budget(&raw, args.budget)
    };
    println!(
        "decimate: cell={cell:.5} -> {} verts, {} tris ({:.1}% verts, {:.1}% tris)",
        dec.pos.len(),
        dec.tris.len(),
        100.0 * dec.pos.len() as f32 / raw.pos.len() as f32,
        100.0 * dec.tris.len() as f32 / raw.tris.len() as f32
    );
    if dec.pos.len() > args.budget {
        return Err(format!(
            "decimated vertex count {} still exceeds budget {}",
            dec.pos.len(),
            args.budget
        ));
    }

    // 2) axis remap
    for p in &mut dec.pos {
        *p = args.axis.apply(*p);
    }

    // 3) scale (explicit, or fit-length on the longest HORIZONTAL axis after remap)
    let (blo, bhi) = dec.bbox();
    let horiz_len = (bhi[0] - blo[0]).max(bhi[2] - blo[2]);
    let s = if let Some(fl) = args.fit_length {
        fl / horiz_len.max(1e-6)
    } else {
        args.scale.unwrap_or(1.0)
    };
    if (s - 1.0).abs() > 1e-9 {
        for p in &mut dec.pos {
            *p = [p[0] * s, p[1] * s, p[2] * s];
        }
    }

    // 4) recenter: center X,Z at origin, drop min-Y to 0 (rest on ground)
    let (clo, chi) = dec.bbox();
    let cx = (clo[0] + chi[0]) * 0.5;
    let cz = (clo[2] + chi[2]) * 0.5;
    let dx = if args.center_xz { cx } else { 0.0 };
    let dz = if args.center_xz { cz } else { 0.0 };
    let dy = if args.recenter_y { clo[1] } else { 0.0 };
    for p in &mut dec.pos {
        *p = [p[0] - dx, p[1] - dy, p[2] - dz];
    }

    // 5) normals + dummy uv
    let nrm = recompute_normals(&dec.pos, &dec.tris);
    let uv = vec![[0.0f32, 0.0f32]; dec.pos.len()];

    let (flo, fhi) = dec.bbox();
    println!(
        "final (metres): {} verts, {} tris, scale x{s:.4}, bbox [{:.3},{:.3},{:.3}]..[{:.3},{:.3},{:.3}]  dims {:.2} x {:.2} x {:.2} (L×H×W of X/Y/Z)",
        dec.pos.len(),
        dec.tris.len(),
        flo[0], flo[1], flo[2], fhi[0], fhi[1], fhi[2],
        fhi[0] - flo[0], fhi[1] - flo[1], fhi[2] - flo[2]
    );

    // 6) write GLB
    write_glb(&args.out, &dec.pos, &nrm, &uv, &dec.tris)?;
    println!("wrote {}", args.out.display());

    // 7) preview renders
    if let Some(prefix) = &args.render {
        let mk = |name: &str| PathBuf::from(format!("{prefix}_{name}.png"));
        // front: look along -Z, screen = (X right, Y up)
        render_view(&mk("front"), args.render_size, &dec.pos, &nrm, &dec.tris, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], "front")?;
        // side: look along -X, screen = (Z right, Y up)
        render_view(&mk("side"), args.render_size, &dec.pos, &nrm, &dec.tris, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], "side")?;
        // top: look along -Y, screen = (X right, Z up)
        render_view(&mk("top"), args.render_size, &dec.pos, &nrm, &dec.tris, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], "top")?;
        // three-quarter: yaw ~35°, pitch ~25°
        let right = [0.82f32, 0.0, -0.57];
        let up = [-0.23f32, 0.9, -0.33];
        render_view(&mk("threeq"), args.render_size, &dec.pos, &nrm, &dec.tris, right, up, "threeq")?;
        println!("rendered {prefix}_{{front,side,top,threeq}}.png (1 m grid at Y=0, 1.8 m green human ref)");
    }

    // 8) verify through the real rigid reader
    if args.verify {
        match mercs2_formats::mesh_import::external_mesh_from_gltf(&args.out) {
            Ok(m) => {
                let ok = m.positions.len() <= 60_000;
                println!(
                    "verify external_mesh_from_gltf: OK — {} verts, {} tris, joints {}, weights {} [{}]",
                    m.positions.len(),
                    m.tris.len(),
                    m.joints.len(),
                    m.weights.len(),
                    if ok { "≤60000 ✓" } else { "OVER BUDGET ✗" }
                );
                if !ok {
                    return Err("verify: vertex count exceeds 60000".into());
                }
            }
            Err(e) => return Err(format!("verify external_mesh_from_gltf FAILED: {e}")),
        }
    }
    let _ = std::io::stdout().flush();
    Ok(())
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;

    fn tri_obj() -> String {
        // A quad (polygon) + a separate triangle, to exercise fan triangulation.
        "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\nv 2 0 0\nv 3 0 0\nv 3 1 0\nf 5 6 7\n".into()
    }

    #[test]
    fn obj_reads_and_fan_triangulates() {
        let dir = std::env::temp_dir().join("mesh_prep_test_obj.obj");
        std::fs::write(&dir, tri_obj()).unwrap();
        let m = read_obj(&dir).unwrap();
        assert_eq!(m.pos.len(), 7);
        // quad -> 2 tris, triangle -> 1 tri
        assert_eq!(m.tris.len(), 3);
    }

    #[test]
    fn clustering_stays_under_budget_and_drops_degenerates() {
        // A dense grid of points forming a strip of quads.
        let mut pos = Vec::new();
        for i in 0..40 {
            for j in 0..40 {
                pos.push([i as f32 * 0.01, j as f32 * 0.01, 0.0]);
            }
        }
        let mut tris = Vec::new();
        let at = |i: usize, j: usize| (i * 40 + j) as u32;
        for i in 0..39 {
            for j in 0..39 {
                tris.push([at(i, j), at(i + 1, j), at(i + 1, j + 1)]);
                tris.push([at(i, j), at(i + 1, j + 1), at(i, j + 1)]);
            }
        }
        let m = Mesh { pos, tris };
        let budget = 100;
        let (_cell, d) = decimate_to_budget(&m, budget);
        assert!(d.pos.len() <= budget, "verts {} > budget {budget}", d.pos.len());
        // no degenerate / zero-area triangles survive
        for t in &d.tris {
            assert!(t[0] != t[1] && t[1] != t[2] && t[0] != t[2], "degenerate tri {t:?}");
            let a = cross(sub(d.pos[t[1] as usize], d.pos[t[0] as usize]), sub(d.pos[t[2] as usize], d.pos[t[0] as usize]));
            assert!(dot(a, a) > 1e-20, "zero-area tri survived");
        }
    }

    #[test]
    fn bbox_preserved_within_a_cell() {
        let mut pos = Vec::new();
        for i in 0..60 {
            for j in 0..60 {
                pos.push([i as f32 * 0.05, j as f32 * 0.05, (i as f32 * 0.01).sin()]);
            }
        }
        let tris = vec![[0u32, 1, 2]];
        let m = Mesh { pos, tris };
        let (lo0, hi0) = m.bbox();
        let (cell, d) = decimate_to_budget(&m, 300);
        let (lo1, hi1) = d.bbox();
        for k in 0..3 {
            assert!((lo1[k] - lo0[k]).abs() <= cell * 2.0, "min axis {k} drifted");
            assert!((hi1[k] - hi0[k]).abs() <= cell * 2.0, "max axis {k} drifted");
        }
    }

    #[test]
    fn axis_map_z_up_to_y_up() {
        let m = AxisMap::from_up("z").unwrap();
        // input +Z (up) should land on +Y (up)
        let out = m.apply([0.0, 0.0, 1.0]);
        assert!((out[1] - 1.0).abs() < 1e-6, "z-up did not map to +Y: {out:?}");
        // input +Y should land on -Z
        let out2 = m.apply([0.0, 1.0, 0.0]);
        assert!((out2[2] + 1.0).abs() < 1e-6, "{out2:?}");
    }

    #[test]
    fn glb_roundtrips_through_the_rigid_reader() {
        let pos = vec![[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]];
        let tris = vec![[0u32, 1, 2], [1, 3, 2]];
        let nrm = recompute_normals(&pos, &tris);
        let uv = vec![[0.0f32, 0.0]; pos.len()];
        let path = std::env::temp_dir().join("mesh_prep_test_rt.glb");
        write_glb(&path, &pos, &nrm, &uv, &tris).unwrap();
        let m = mercs2_formats::mesh_import::external_mesh_from_gltf(&path).unwrap();
        assert_eq!(m.positions.len(), 4);
        assert_eq!(m.tris.len(), 2);
        assert!(m.joints.is_empty() && m.weights.is_empty());
    }
}
