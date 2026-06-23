//! Stage-2 Havok collision extractor.
//!
//! Drop-in replacement for `tools/havok_extractor.py` (same CLI:
//! `havok_extract <blob> --out-dir <dir> [--emit-convex-obj]`). The Python tool
//! sliced Havok regions at a fixed 256 KiB cap and guessed convex hulls with a
//! `longest_vec3_run` byte-scan → denormal-garbage vertices. This binary uses
//! the exact little-endian packfile decoder in [`mercs2_formats::havok`]:
//! correct packfile bounds, real `hkpConvexVerticesShape` vertices + plane
//! equations, and a structured manifest the webapp can ingest.

use mercs2_formats::havok::{find_packfiles, ConvexHull, Shape};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let mut blob: Option<PathBuf> = None;
    let mut out_dir: Option<PathBuf> = None;
    let mut emit_obj = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out-dir" => out_dir = args.next().map(PathBuf::from),
            "--emit-convex-obj" => emit_obj = true,
            "--max-len" => {
                args.next(); // accepted for CLI parity; real packfile size is used
            }
            s if !s.starts_with("--") && blob.is_none() => blob = Some(PathBuf::from(s)),
            _ => {}
        }
    }
    let (Some(blob), Some(out_dir)) = (blob, out_dir) else {
        eprintln!("usage: havok_extract <blob> --out-dir <dir> [--emit-convex-obj]");
        return 2;
    };

    let data = match fs::read(&blob) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("havok_extract: read {}: {e}", blob.display());
            return 1;
        }
    };
    if let Err(e) = fs::create_dir_all(&out_dir) {
        eprintln!("havok_extract: mkdir {}: {e}", out_dir.display());
        return 1;
    }

    let packfiles = find_packfiles(&data);
    let mut slices = Vec::new();
    let mut hull_n = 0usize;
    for (i, (off, pf)) in packfiles.iter().enumerate() {
        let end = (off + pf.size).min(data.len());
        let binname = format!("havok_{i:04}_Havok.bin");
        let _ = fs::write(out_dir.join(&binname), &data[*off..end]);

        let mut shapes_json = Vec::new();
        let mut convex_obj: Option<String> = None;
        for shape in &pf.shapes {
            match shape {
                Shape::Convex(h) => {
                    let objname = format!("convex_hull_{hull_n:04}.obj");
                    let obj_ref = if emit_obj {
                        let _ = fs::write(out_dir.join(&objname), hull_obj(h));
                        Some(objname.clone())
                    } else {
                        None
                    };
                    convex_obj.get_or_insert_with(|| objname.clone());
                    shapes_json.push(json!({
                        "kind": "convex",
                        "index": hull_n,
                        "obj": obj_ref,
                        "vertices": h.vertices,
                        "planes": h.planes,
                    }));
                    hull_n += 1;
                }
                Shape::Box { half_extents } => {
                    shapes_json.push(json!({"kind": "box", "half_extents": half_extents}))
                }
                Shape::Mopp => shapes_json.push(json!({"kind": "mopp"})),
                Shape::Mesh => shapes_json.push(json!({"kind": "mesh"})),
                Shape::Other(name) => shapes_json.push(json!({"kind": "other", "class": name})),
            }
        }

        slices.push(json!({
            "file": binname,
            "offset": off,
            "tag": "Havok",
            "size_written": end - off,
            "version": pf.version,
            "preview": pf.version,
            "has_convex_hull": convex_obj.is_some(),
            "convex_hull_filename": convex_obj,
            "class_counts": pf.class_counts,
            "shapes": shapes_json,
        }));
    }

    let manifest = json!({
        "schema": "mercs2_havok/2",
        "extractor": "mercs2_formats::havok",
        "havok_slices": slices,
    });
    if let Err(e) = fs::write(
        out_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap_or_default(),
    ) {
        eprintln!("havok_extract: write manifest: {e}");
        return 1;
    }
    println!(
        "havok_extract: {} packfile(s), {} convex hull(s) from {}",
        packfiles.len(),
        hull_n,
        blob.display()
    );
    0
}

// ── OBJ emission ─────────────────────────────────────────────────────────────

/// A hull as a real OBJ: exact vertices + faces derived from the plane equations
/// (verts on each plane, fan-triangulated). Faces are advisory for viewing; the
/// vertices are the authoritative decode.
fn hull_obj(h: &ConvexHull) -> String {
    let mut s = String::from(
        "# Mercenaries 2 Havok convex hull — exact decode (mercs2_formats::havok)\no hull\n",
    );
    for v in &h.vertices {
        s += &format!("v {:.6} {:.6} {:.6}\n", v[0], v[1], v[2]);
    }
    for f in hull_faces(h) {
        s += &format!("f {} {} {}\n", f[0] + 1, f[1] + 1, f[2] + 1); // OBJ is 1-indexed
    }
    s
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt();
    if l > 1e-12 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Triangulate the hull's faces from its plane equations: for each plane, gather
/// the vertices lying on it (`n·v + w ≈ 0`), order them around the plane normal,
/// and fan-triangulate.
fn hull_faces(h: &ConvexHull) -> Vec<[usize; 3]> {
    let v = &h.vertices;
    if v.len() < 3 {
        return vec![];
    }
    let scale = v
        .iter()
        .flat_map(|p| p.iter().map(|c| c.abs()))
        .fold(1e-3f32, f32::max);
    // Hull verts are inset from their planes by the (uniform) convex radius, so a
    // face's verts sit at the plane's *maximum* `n·v + w`, not at 0. Select per
    // plane the verts within eps of that max — radius-agnostic.
    let eps = (1e-2 * scale).max(1e-4);
    let mut faces = Vec::new();
    for pl in &h.planes {
        let n = [pl[0], pl[1], pl[2]];
        let w = pl[3];
        let d: Vec<f32> = v.iter().map(|p| dot(n, *p) + w).collect();
        let m = d.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut idx: Vec<usize> = (0..v.len()).filter(|&i| d[i] >= m - eps).collect();
        if idx.len() < 3 {
            continue;
        }
        let nn = normalize(n);
        let seed = if nn[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let u = normalize(cross(nn, seed));
        let t = cross(nn, u);
        let mut c = [0.0f32; 3];
        for &i in &idx {
            for k in 0..3 {
                c[k] += v[i][k];
            }
        }
        for k in 0..3 {
            c[k] /= idx.len() as f32;
        }
        idx.sort_by(|&a, &b| {
            let ang = |i: usize| {
                let d = [v[i][0] - c[0], v[i][1] - c[1], v[i][2] - c[2]];
                dot(d, t).atan2(dot(d, u))
            };
            ang(a)
                .partial_cmp(&ang(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for k in 1..idx.len() - 1 {
            faces.push([idx[0], idx[k], idx[k + 1]]);
        }
    }
    faces
}
