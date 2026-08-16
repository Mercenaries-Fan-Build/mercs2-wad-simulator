//! `pool_locate` — for a multi-mesh PHY2 container, find WHERE each mesh's real quantized u16×3 vertex
//! pool lives (packfile region vs trailing wrapper), by scoring every candidate base in the WHOLE body
//! against THAT mesh's own indices + min/scale. Answers: are there N distinct high-scoring pools, and in
//! which region? This decides how to author the N-mesh wrapper. Reads vz.wad via local config.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::havok::{parse_packfile_raw, parse_phy2_body, Shape, HAVOK_MAGIC};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::path::Path;

fn arg(a: &[String], n: &str) -> Option<String> {
    a.iter().position(|x| x == n).and_then(|i| a.get(i + 1)).cloned()
}
fn phy2_span(container: &[u8]) -> Option<(usize, usize)> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let data_area_off = u32::from_le_bytes(container[4..8].try_into().ok()?) as usize;
    let n_desc = u32::from_le_bytes(container[16..20].try_into().ok()?) as usize;
    for i in 0..n_desc {
        let r = 20 + i * 20;
        if r + 20 > container.len() {
            break;
        }
        if &container[r..r + 4] != b"PHY2" {
            continue;
        }
        let row_u0 = u32::from_le_bytes(container[r + 4..r + 8].try_into().ok()?) as usize;
        let size = u32::from_le_bytes(container[r + 8..r + 12].try_into().ok()?) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue;
        }
        let start = if data_area_off > 0 { data_area_off + row_u0 } else { 8 + row_u0 };
        if start + size > container.len() {
            return None;
        }
        return Some((start, size));
    }
    None
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let block: u16 = arg(&args, "--block").and_then(|s| s.parse().ok()).unwrap_or(2612);
    let want: Option<u32> = arg(&args, "--model")
        .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok());

    let vz = game_paths::vz_wad(Path::new(".")).expect("vz.wad");
    let mut f = std::fs::File::open(&vz).expect("open");
    let size = f.metadata().unwrap().len();
    let ar = load_ffcs_archive(&mut f, size).expect("ffcs");
    let dec = decompress_block(&mut f, &ar.indx, block).expect("decompress");
    let (parsed, _i) = walk_decompressed_block(&dec, "wd");

    for (ci, c) in parsed.containers.iter().enumerate() {
        let nh = parsed.entries[ci].name_hash;
        if let Some(w) = want {
            if nh != w {
                continue;
            }
        }
        let Some((s, sz)) = phy2_span(c) else { continue };
        let body = &c[s..s + sz];
        let Ok(pf) = parse_phy2_body(body) else { continue };
        let off = body.windows(8).position(|w| w == HAVOK_MAGIC).unwrap();
        let raw = parse_packfile_raw(&body[off..]).unwrap();
        let pk_end = off + raw.size; // body-abs wrapper start
        println!("\n=== container[{ci}] 0x{nh:08X}  PHY2 {sz}B  packfile @+{off} size {} pkend@+{pk_end} wrapper {}B ===",
            raw.size, sz - pk_end);

        // For each decoded mesh we need min/scale + indices. The reader's MeshShape only exposes decoded
        // (already-dequantized) verts + indices; re-derive min/scale from the subpart in the packfile is
        // not exposed here, so instead re-quantize: we know the *dequantized* verts the reader produced,
        // but those may be a false lock. To find the TRUE pool we scan the whole body for a base whose
        // u16×3 pool, dequantized with the mesh's decoded min/scale (recovered from the decoded verts'
        // AABB / 0xffff), reproduces sane tris. Simpler + robust: score each candidate base by the SAME
        // <40m edge metric the reader uses, but do it per-mesh using the mesh's OWN index list and the
        // decoded min/scale, and report the TOP distinct bases across the whole body.
        let mesh_shapes: Vec<(usize, &mercs2_formats::havok::MeshShape)> = pf
            .shapes
            .iter()
            .enumerate()
            .filter_map(|(mi, sh)| match sh {
                Shape::Mesh(m) if !m.indices.is_empty() => Some((mi, m)),
                _ => None,
            })
            .collect();

        for (mi, m) in &mesh_shapes {
            // recover min/scale from decoded verts: min = componentwise min, scale = (max-min)/0xffff.
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for v in &m.vertices {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
            let scale = [
                (hi[0] - lo[0]) / 65535.0,
                (hi[1] - lo[1]) / 65535.0,
                (hi[2] - lo[2]) / 65535.0,
            ];
            let nverts = m.vertices.len();
            let getb = |pool: usize, v: usize| -> [f32; 3] {
                let o = pool + v * 6;
                [
                    lo[0] + u16::from_le_bytes([body[o], body[o + 1]]) as f32 * scale[0],
                    lo[1] + u16::from_le_bytes([body[o + 2], body[o + 3]]) as f32 * scale[1],
                    lo[2] + u16::from_le_bytes([body[o + 4], body[o + 5]]) as f32 * scale[2],
                ]
            };
            let edge = |p: [f32; 3], q: [f32; 3]| {
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            };
            let score = |pool: usize| -> f64 {
                let mut ok = 0usize;
                for t in &m.indices {
                    let (a, b, cc) = (getb(pool, t[0] as usize), getb(pool, t[1] as usize), getb(pool, t[2] as usize));
                    if edge(a, b) < 40.0 && edge(b, cc) < 40.0 && edge(a, cc) < 40.0 {
                        ok += 1;
                    }
                }
                ok as f64 / m.indices.len() as f64
            };
            // scan whole body for high-scoring bases; collect local maxima (report a few, with region).
            let end = body.len().saturating_sub(nverts * 6);
            let mut best: Vec<(usize, f64)> = Vec::new();
            let mut base = 0usize;
            while base <= end {
                let sc = score(base);
                if sc > 0.95 {
                    best.push((base, sc));
                }
                base += 2;
            }
            // de-dup adjacent bases (within 6 bytes) keeping the best.
            best.sort_by(|a, b| a.0.cmp(&b.0));
            let mut merged: Vec<(usize, f64)> = Vec::new();
            for (o, sc) in best {
                if let Some(last) = merged.last_mut() {
                    if o - last.0 < 8 {
                        if sc > last.1 {
                            *last = (o, sc);
                        }
                        continue;
                    }
                }
                merged.push((o, sc));
            }
            let region = |o: usize| if o >= pk_end { format!("WRAPPER+{}", o - pk_end) } else { format!("packfile+{o}") };
            println!(
                "  shape[{mi}] {} tris {} verts  min[{:.1},{:.1},{:.1}] scale[{:.4},{:.4},{:.4}]  →  {} candidate pool bases (score>0.95):",
                m.indices.len(), nverts, lo[0], lo[1], lo[2], scale[0], scale[1], scale[2], merged.len()
            );
            for (o, sc) in merged.iter().take(8) {
                println!("      base @body+{o:<7} [{}]  score {:.3}  (pool spans {}..{})",
                    region(*o), sc, o, o + nverts * 6);
            }
        }
    }
}
