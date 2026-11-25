//! `mopp-selectivity`: how many triangles a ray-like query is handed by a collider's `hkpMoppCode`, beside
//! the code [`mercs2_formats::phy2_moppswap`] computes over the same mesh and the count of triangles whose
//! box the query really overlaps.
//!
//! The colliders are every `PHY2` of the chosen blocks of a PC WAD, or every `PHY2` of a console archive
//! converted with its codes as the platform baked them (a PS3 archive's are its chunked codes). A query is
//! a segment walked as boxes of at most a metre through [`mopp::query_aabb`], the candidates of its boxes
//! united. That walk does not prune the 26-DOP splits Havok emits, so a Havok code's counts here are an
//! upper bound on what the game's walkers hand back for it.
//!
//! Three segments start from each sampled triangle's centroid, the triangle one of the mesh under the code:
//! a 2 m vertical probe through it, a 25 m level ray a metre above it, and a 60 m ray from 1.5 m above it
//! that rises or falls up to 5 m. The exact count of a segment is the triangles of that mesh whose box one
//! of the segment's boxes overlaps.
//!
//! The run fails (exit 1) on any code it could not measure, and on any code, source or recomputed, that
//! does not hand back a triangle of a segment's exact count.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::mopp::{self, MoppInfo};
use mercs2_formats::phy2_container::phy2_span_in_container;
use mercs2_formats::phy2_moppswap::{count_phy2_mopps, phy2_mopp_mesh, rebake_phy2_mopps, swap_phy2_mopp, SwapMode};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::collections::HashSet;
use std::path::Path;

const USAGE: &str = "usage: mercs2_probe mopp-selectivity <pc wad> (--blocks a,b,c | --match <path part> [--limit N]) [--anchors N]\n\
                     \x20      mercs2_probe mopp-selectivity --archive <big-endian archive> [--match <path part>] [--limit N] [--anchors N]";
const KINDS: [&str; 3] = ["2 m vertical probe", "25 m level ray", "60 m slanted ray"];
/// The integer span of a root frame: a root byte operand of `0xFF` at the root shift of 16.
const ROOT_SPAN: f32 = 16_711_680.0;

type Aabb = ([f32; 3], [f32; 3]);

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 11
    }
    fn unit(&mut self) -> f32 {
        (self.next() % 1_000_000) as f32 / 1_000_000.0
    }
}

/// The boxes a segment is walked as: pieces of at most `step`, each grown by a centimetre.
fn pieces(from: [f32; 3], to: [f32; 3], step: f32) -> Vec<Aabb> {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let n = ((len / step).ceil() as usize).max(1);
    (0..n)
        .map(|j| {
            let (t0, t1) = (j as f32 / n as f32, (j + 1) as f32 / n as f32);
            let (mut lo, mut hi) = ([0f32; 3], [0f32; 3]);
            for a in 0..3 {
                let (x, y) = (from[a] + d[a] * t0, from[a] + d[a] * t1);
                lo[a] = x.min(y) - 0.01;
                hi[a] = x.max(y) + 0.01;
            }
            (lo, hi)
        })
        .collect()
}

/// The keys a code hands back for a segment's boxes, and the opcodes its walks ran.
fn candidates(code: &[u8], info: &MoppInfo, boxes: &[Aabb]) -> (HashSet<u32>, usize) {
    let (mut seen, mut steps): (HashSet<u32>, usize) = (HashSet::new(), 0);
    for (lo, hi) in boxes {
        let (keys, n) = mopp::query_aabb_steps(code, info, *lo, *hi);
        seen.extend(keys);
        steps += n;
    }
    (seen, steps)
}

#[derive(Default, Clone, Copy)]
struct Sum {
    queries: usize,
    source: f64,
    rebaked: f64,
    exact: f64,
    source_steps: f64,
    rebaked_steps: f64,
}

impl Sum {
    fn add(&mut self, o: &Sum) {
        self.queries += o.queries;
        self.source += o.source;
        self.rebaked += o.rebaked;
        self.exact += o.exact;
        self.source_steps += o.source_steps;
        self.rebaked_steps += o.rebaked_steps;
    }
}

#[derive(Default)]
struct Totals {
    bodies: usize,
    without_code: usize,
    codes: usize,
    identical: usize,
    kinds: [Sum; 3],
    problems: Vec<String>,
}

/// Measure every code of one little-endian `PHY2` body, printing a line per code. `fitted` holds the body's
/// codes as the conversion recomputes them, each in the bytes of its source; without it a code is
/// recomputed with no limit on its length.
fn measure(label: &str, seed: u64, body: &[u8], fitted: Option<&[(Vec<u8>, MoppInfo)]>, anchors: usize, t: &mut Totals) {
    t.bodies += 1;
    let n = count_phy2_mopps(body);
    if n == 0 {
        t.without_code += 1;
        return;
    }
    let source = mopp::extract_mopp_with_info(body);
    if source.len() != n {
        t.problems.push(format!("{label}: {n} hkpMoppCode objects but {} readable codes", source.len()));
        return;
    }
    for i in 0..n {
        let (mesh, keys) = match phy2_mopp_mesh(body, i) {
            Ok(m) => m,
            Err(e) => {
                t.problems.push(format!("{label} code {i}: {e}"));
                continue;
            }
        };
        let (mut tri_boxes, mut centroids): (Vec<Aabb>, Vec<[f32; 3]>) = (Vec::new(), Vec::new());
        let mut bad = None;
        for tri in &mesh.indices {
            let (mut lo, mut hi, mut c) = ([f32::MAX; 3], [f32::MIN; 3], [0f32; 3]);
            for &vi in tri {
                let Some(v) = mesh.vertices.get(vi as usize) else {
                    bad = Some(vi);
                    break;
                };
                for a in 0..3 {
                    lo[a] = lo[a].min(v[a]);
                    hi[a] = hi[a].max(v[a]);
                    c[a] += v[a] / 3.0;
                }
            }
            tri_boxes.push((lo, hi));
            centroids.push(c);
        }
        if let Some(vi) = bad {
            t.problems.push(format!("{label} code {i}: a triangle names vertex {vi} of {}", mesh.vertices.len()));
            continue;
        }
        let rebaked = match fitted {
            Some(codes) => codes.to_vec(),
            None => match swap_phy2_mopp(body, i, SwapMode::Spatial) {
                Ok((b, _)) => mopp::extract_mopp_with_info(&b),
                Err(e) => {
                    t.problems.push(format!("{label} code {i}: {e}"));
                    continue;
                }
            },
        };
        if rebaked.len() != n {
            t.problems.push(format!("{label} code {i}: the recomputed body holds {} readable codes, not {n}", rebaked.len()));
            continue;
        }
        let ((sc, si), (rc, ri)) = (&source[i], &rebaked[i]);
        let walked = mopp::decode_chunked(sc);
        if let Some(e) = &walked.error {
            t.problems.push(format!("{label} code {i}: the source code does not walk: {e}"));
            continue;
        }
        t.codes += 1;
        if sc == rc {
            t.identical += 1;
        }
        // A code the unchunked walk cannot finish holds chunk jumps.
        let chunked = mopp::decode(sc).error.is_some();
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed ^ ((i as u64) << 48));
        let mut sums = [Sum::default(); 3];
        let (mut missed_source, mut missed_rebaked) = (0usize, 0usize);
        for _ in 0..anchors.min(centroids.len()) {
            let c = centroids[(rng.next() % centroids.len() as u64) as usize];
            let turn = rng.unit() * std::f32::consts::TAU;
            let (dx, dz) = (turn.cos(), turn.sin());
            let rise = (rng.unit() - 0.5) * 10.0;
            let segments = [
                pieces([c[0], c[1] + 1.0, c[2]], [c[0], c[1] - 1.0, c[2]], 0.5),
                pieces([c[0], c[1] + 1.0, c[2]], [c[0] + 25.0 * dx, c[1] + 1.0, c[2] + 25.0 * dz], 1.0),
                pieces([c[0], c[1] + 1.5, c[2]], [c[0] + 60.0 * dx, c[1] + 1.5 + rise, c[2] + 60.0 * dz], 1.0),
            ];
            for (k, boxes) in segments.iter().enumerate() {
                let ((s, s_steps), (r, r_steps)) = (candidates(sc, si, boxes), candidates(rc, ri, boxes));
                sums[k].source_steps += s_steps as f64;
                sums[k].rebaked_steps += r_steps as f64;
                // The triangles whose own box one of the segment's boxes overlaps: what any code must hand back.
                let (mut all_lo, mut all_hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                for (lo, hi) in boxes {
                    for a in 0..3 {
                        all_lo[a] = all_lo[a].min(lo[a]);
                        all_hi[a] = all_hi[a].max(hi[a]);
                    }
                }
                let over = |(tlo, thi): &Aabb, (qlo, qhi): &Aabb| (0..3).all(|a| tlo[a] <= qhi[a] && thi[a] >= qlo[a]);
                let exact: Vec<u32> = tri_boxes
                    .iter()
                    .zip(&keys)
                    .filter(|(b, _)| over(b, &(all_lo, all_hi)) && boxes.iter().any(|q| over(b, q)))
                    .map(|(_, &key)| key)
                    .collect();
                missed_source += exact.iter().filter(|key| !s.contains(key)).count();
                missed_rebaked += exact.iter().filter(|key| !r.contains(key)).count();
                sums[k].queries += 1;
                sums[k].source += s.len() as f64;
                sums[k].rebaked += r.len() as f64;
                sums[k].exact += exact.len() as f64;
            }
        }
        if missed_source > 0 {
            t.problems.push(format!("{label} code {i}: the source code missed {missed_source} overlapping triangle(s)"));
        }
        if missed_rebaked > 0 {
            t.problems.push(format!("{label} code {i}: the recomputed code missed {missed_rebaked} overlapping triangle(s)"));
        }
        print!(
            "{label} code {i}: {} triangles, {} -> {} bytes, chunked {chunked}, frame {:.1} -> {:.1} m, same bytes {}",
            keys.len(),
            sc.len(),
            rc.len(),
            si.scale * ROOT_SPAN,
            ri.scale * ROOT_SPAN,
            sc == rc
        );
        for (k, s) in sums.iter().enumerate() {
            let q = s.queries.max(1) as f64;
            print!(" | {:.1} -> {:.1} (exact {:.1})", s.source / q, s.rebaked / q, s.exact / q);
            t.kinds[k].add(s);
        }
        println!();
    }
}

fn summary(t: &Totals) {
    println!("\n{} PHY2 bodies ({} without a code), {} codes measured, {} of them the bytes of their own recompute", t.bodies, t.without_code, t.codes, t.identical);
    for (k, s) in t.kinds.iter().enumerate() {
        if s.queries == 0 {
            println!("  {}: no segments", KINDS[k]);
            continue;
        }
        let q = s.queries as f64;
        println!(
            "  {}: {} segments, mean candidates source {:.2}, recomputed {:.2}, exact {:.2} (source {:.2}x exact, recomputed {:.2}x); opcodes walked source {:.0}, recomputed {:.0}",
            KINDS[k],
            s.queries,
            s.source / q,
            s.rebaked / q,
            s.exact / q,
            s.source / s.exact.max(1e-9),
            s.rebaked / s.exact.max(1e-9),
            s.source_steps / q,
            s.rebaked_steps / q
        );
    }
    if !t.problems.is_empty() {
        println!("\n{} problems:", t.problems.len());
        for p in &t.problems {
            println!("  {p}");
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn number(args: &[String], name: &str, default: usize) -> Result<usize, String> {
    match flag(args, name) {
        Some(v) => v.parse().map_err(|_| format!("{name} takes a number, not {v:?}")),
        None => Ok(default),
    }
}

/// `limit` of `all`, evenly spaced.
fn spaced<T: Clone>(all: &[T], limit: usize) -> Vec<T> {
    if all.len() <= limit {
        return all.to_vec();
    }
    (0..limit).map(|j| all[j * all.len() / limit].clone()).collect()
}

fn pc_wad(wad: &Path, args: &[String], anchors: usize, t: &mut Totals) -> Result<(), String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let blocks: Vec<u16> = match (flag(args, "--blocks"), flag(args, "--match")) {
        (Some(list), None) => list.split(',').map(|s| s.trim().parse().map_err(|_| format!("--blocks: {s:?} is not a block index"))).collect::<Result<_, _>>()?,
        (None, Some(part)) => {
            let part = part.to_lowercase();
            let all: Vec<u16> = ar.paths.iter().enumerate().filter(|(_, p)| p.to_lowercase().contains(&part)).map(|(i, _)| i as u16).collect();
            let chosen = spaced(&all, number(args, "--limit", usize::MAX)?);
            println!("{} of {} blocks whose path holds {part:?}", chosen.len(), all.len());
            chosen
        }
        _ => return Err("a PC WAD takes one of --blocks and --match".into()),
    };
    for blk in blocks {
        let path = ar.paths.get(blk as usize).cloned().unwrap_or_default();
        let dec = decompress_block(&mut f, &ar.indx, blk).map_err(|e| format!("block {blk} ({path}): {e}"))?;
        let (parsed, _) = walk_decompressed_block(&dec, "mopp-selectivity");
        for (ci, c) in parsed.containers.iter().enumerate() {
            let Some((at, len)) = phy2_span_in_container(c) else { continue };
            let name = parsed.entries.get(ci).map_or(0, |e| e.name_hash);
            measure(&format!("{path} 0x{name:08X}"), ((blk as u64) << 16) ^ ci as u64, &c[at..at + len], None, anchors, t);
        }
    }
    Ok(())
}

fn console_archive(archive: &Path, args: &[String], anchors: usize, t: &mut Totals) -> Result<(), String> {
    use mercs2_formats::be_to_le::havok::convert_phy2_be_to_le_as_baked;
    use mercs2_formats::be_to_le::texture::container_rows;
    let part = flag(args, "--match").map(|p| p.to_lowercase());
    let chunks = crate::dlc_census::archive_chunks(archive, |_, _| true)?;
    let held: Vec<(usize, usize, usize)> = chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| part.as_ref().is_none_or(|p| c.block.to_lowercase().contains(p)))
        .filter_map(|(i, c)| {
            let rows = if c.body.len() > 20 && c.body.starts_with(b"XFCU") { container_rows(&c.body).ok()? } else { return None };
            rows.into_iter().find(|(tag, off, size)| tag == b"PHY2" && *size > 0 && off + size <= c.body.len()).map(|(_, off, size)| (i, off, size))
        })
        .collect();
    let chosen = spaced(&held, number(args, "--limit", usize::MAX)?);
    println!("{} of {} chunks that hold a PHY2", chosen.len(), held.len());
    for (i, off, size) in chosen {
        let c = &chunks[i];
        let label = format!("{} 0x{:08X}", c.block, c.name);
        let le = match convert_phy2_be_to_le_as_baked(&c.body[off..off + size]) {
            Ok(le) => le,
            Err(e) => {
                t.problems.push(format!("{label}: {e}"));
                continue;
            }
        };
        if count_phy2_mopps(&le) == 0 {
            measure(&label, 0, &le, None, anchors, t);
            continue;
        }
        match rebake_phy2_mopps(&le) {
            Ok(fitted) => measure(&label, ((i as u64) << 16) ^ c.name as u64, &le, Some(&mopp::extract_mopp_with_info(&fitted)), anchors, t),
            Err(e) => t.problems.push(format!("{label}: {e}")),
        }
    }
    Ok(())
}

pub fn run(args: &[String]) -> i32 {
    let mut t = Totals::default();
    let done = (|| {
        let anchors = number(args, "--anchors", 100)?;
        match (flag(args, "--archive"), args.first().filter(|a| !a.starts_with("--"))) {
            (Some(archive), None) => console_archive(Path::new(&archive), args, anchors, &mut t),
            (None, Some(wad)) => pc_wad(Path::new(wad), args, anchors, &mut t),
            _ => Err(USAGE.to_string()),
        }
    })();
    if let Err(e) = done {
        eprintln!("mopp-selectivity: {e}");
        return 2;
    }
    summary(&t);
    i32::from(!t.problems.is_empty())
}
