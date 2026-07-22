//! Weight TRANSFER: sample the shipped donor's skinning onto a conformed import.
//!
//! Why this exists. `char_skin` conforms foreign geometry onto a Mercs2 skeleton and carries the
//! source rig's own weights across. For a simple rig that is fine (`rigfig`: 9.7% rigid, shipped-
//! like). For a dense one it is not: 50 Cent's 119 joints include a facial rig and muscle/twist
//! helpers with no counterpart in a 116-bone Mercs2 skeleton, so several source joints legitimately
//! map to one target bone, their weights SUM, one dominates, and the vertex goes rigid. Result:
//! 85.4% of vertices bound to a single bone against 17.2%/6.7% in shipped characters — geometry
//! that looks right at rest and tears the moment a joint bends.
//!
//! No amount of better joint-mapping fixes that: the information is not representable on the target
//! rig. But it does not have to be re-derived, because the answer already ships. The donor mesh sits
//! on the SAME skeleton in the SAME bind pose (that is what conforming establishes) and measures
//! 82.8% multi-influence. So sample it: for each conformed vertex, take the weights retail uses at
//! that point in space.
//!
//! This is deliberately NOT the spatial nearest-neighbour weight transfer recorded as a TRAP in
//! `cj-foreign-model-import`. That failure transferred between rigs that did not share a bind pose
//! or proportions (CesiumMan Z-up vs Mattias Y-up), so "nearest vertex" crossed body parts. Here the
//! shared bind pose is a precondition, and it is MEASURED before use (`xfer_probe`): median distance
//! from a conformed vertex to the nearest donor vertex is 1.0% of body height, p99 5.5%.

use std::collections::HashMap;

/// One donor surface sample: where it is, which way it faces, and what retail binds it to.
pub struct DonorSample {
    pub pos: [f64; 3],
    /// Surface normal. Distance alone is not enough to identify the RIGHT donor point: where two
    /// body parts nearly touch — an arm hanging beside a torso — the closest donor vertex to an
    /// inner-arm point is often ON THE TORSO. Sampling it binds arm geometry to torso bones, and
    /// the limb fuses outward under animation: the character reads as THICKER than the source while
    /// the bind pose still looks perfect. Requiring the surfaces to face the same way rejects that.
    pub normal: [f64; 3],
    /// (global HIER bone, weight 0..1), already normalised, at most 4 entries.
    pub infl: Vec<(u32, f64)>,
}

/// Uniform grid over the donor samples. 222M brute-force pairs is affordable once in a probe but
/// not in a build step; a grid sized to the query radius makes it linear in practice.
struct Grid {
    cell: f64,
    min: [f64; 3],
    dims: [i64; 3],
    buckets: HashMap<i64, Vec<usize>>,
}

impl Grid {
    fn new(pts: &[DonorSample], cell: f64) -> Grid {
        let mut min = [f64::MAX; 3];
        let mut max = [f64::MIN; 3];
        for s in pts {
            for c in 0..3 {
                min[c] = min[c].min(s.pos[c]);
                max[c] = max[c].max(s.pos[c]);
            }
        }
        let dims = [
            (((max[0] - min[0]) / cell).ceil() as i64).max(1),
            (((max[1] - min[1]) / cell).ceil() as i64).max(1),
            (((max[2] - min[2]) / cell).ceil() as i64).max(1),
        ];
        let mut g = Grid { cell, min, dims, buckets: HashMap::new() };
        for (i, s) in pts.iter().enumerate() {
            let k = g.key(g.cell_of(s.pos));
            g.buckets.entry(k).or_default().push(i);
        }
        g
    }
    fn cell_of(&self, p: [f64; 3]) -> [i64; 3] {
        [
            (((p[0] - self.min[0]) / self.cell).floor() as i64).clamp(0, self.dims[0] - 1),
            (((p[1] - self.min[1]) / self.cell).floor() as i64).clamp(0, self.dims[1] - 1),
            (((p[2] - self.min[2]) / self.cell).floor() as i64).clamp(0, self.dims[2] - 1),
        ]
    }
    fn key(&self, c: [i64; 3]) -> i64 {
        (c[0] * 73_856_093) ^ (c[1] * 19_349_663) ^ (c[2] * 83_492_791)
    }
    /// Indices within `rings` cells of `p`, widening until something is found.
    fn near(&self, p: [f64; 3], out: &mut Vec<usize>) {
        let c = self.cell_of(p);
        for ring in 0..8i64 {
            out.clear();
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    for dz in -ring..=ring {
                        // only the shell of this ring after the first pass
                        if ring > 0 && dx.abs() != ring && dy.abs() != ring && dz.abs() != ring {
                            continue;
                        }
                        if let Some(b) = self.buckets.get(&self.key([c[0] + dx, c[1] + dy, c[2] + dz])) {
                            out.extend_from_slice(b);
                        }
                    }
                }
            }
            if !out.is_empty() && ring > 0 {
                return;
            }
        }
    }
}

/// Influences below this share are dropped before renormalising. Weights ship as u8/255, so a
/// contribution under ~4% is a couple of quantisation steps — too small to shape the surface, but
/// enough of them together visibly inflate it under linear-blend skinning.
pub const MIN_WEIGHT: f64 = 0.04;

/// Minimum cos(angle) between a target point's normal and a donor sample's for the sample to be
/// considered the same surface. 0.0 rejects anything facing away — enough to separate an arm from
/// the torso it rests against, without discarding legitimate curvature.
pub const NORMAL_MIN_DOT: f64 = 0.0;

/// Clamp each bone's influence to the reach it actually has in the DONOR.
///
/// Spatial sampling copies a bone's weight to whatever geometry sits at that point, but it cannot
/// know how far that bone is *supposed* to reach. Where the import's surface stands off further from
/// a joint than the donor's does, the copied weight is applied at a longer lever arm and the same
/// rotation throws the vertex proportionally further.
///
/// Measured on 50 Cent's face, which is where small joints live: retail's own vertices weighted to
/// bone 41 sit close enough that a 20 degree rotation moves them 2.5 mm, while the import's sit far
/// enough to move 10.9 mm — **4.4x** retail, and 2-3x on the rest of the facial cluster. Those bones
/// carry only ~2% of the region's weight, so the error is invisible in any average (the head band
/// has the LOWEST rms of the whole model) while being concentrated on eyes, jaw and mouth, where a
/// centimetre of unwanted travel is the difference between a face and a smear.
///
/// So bound it by evidence: for each bone, measure how far it reaches in the donor, and drop
/// influence beyond that. `quantile` picks the reach (0.99 ignores a few stray donor vertices),
/// `margin` allows for the import legitimately sitting a little proud of the donor surface. Bones
/// whose reach cannot be measured are left alone rather than guessed at.
pub fn clamp_to_donor_reach(
    per_vertex: &mut [Vec<(u32, f64)>],
    targets: &[[f64; 3]],
    donor: &[DonorSample],
    bone_pos: &[[f64; 3]],
    quantile: f64,
    margin: f64,
) -> usize {
    // reach[b] = distance from bone b beyond which the donor gives it no meaningful weight
    let mut per_bone: HashMap<u32, Vec<f64>> = HashMap::new();
    for s in donor {
        for &(b, w) in &s.infl {
            // Ignore crumbs: a 1% influence does not establish that a bone reaches that far.
            if w < 0.05 {
                continue;
            }
            let Some(p) = bone_pos.get(b as usize) else { continue };
            let d = ((s.pos[0] - p[0]).powi(2) + (s.pos[1] - p[1]).powi(2) + (s.pos[2] - p[2]).powi(2))
                .sqrt();
            per_bone.entry(b).or_default().push(d);
        }
    }
    let mut reach: HashMap<u32, f64> = HashMap::new();
    for (b, mut ds) in per_bone {
        ds.sort_by(|a, x| a.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
        let i = ((ds.len() as f64 - 1.0) * quantile).round() as usize;
        reach.insert(b, ds[i.min(ds.len() - 1)] * margin);
    }

    let mut clamped = 0usize;
    let mut dominant_hits = 0usize;
    for (vi, infl) in per_vertex.iter_mut().enumerate() {
        let Some(t) = targets.get(vi) else { continue };
        let before = infl.len();
        // Which bone carries this vertex? Losing it is categorically different from losing a crumb.
        let dom = infl
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|x| x.0);
        let keep: Vec<(u32, f64)> = infl
            .iter()
            .copied()
            .filter(|&(b, _)| {
                let (Some(r), Some(p)) = (reach.get(&b), bone_pos.get(b as usize)) else {
                    return true; // no evidence either way - leave it
                };
                let d = ((t[0] - p[0]).powi(2) + (t[1] - p[1]).powi(2) + (t[2] - p[2]).powi(2)).sqrt();
                d <= *r
            })
            .collect();
        if let Some(d) = dom {
            if !keep.iter().any(|&(b, _)| b == d) {
                dominant_hits += 1;
            }
        }
        // Never strip a vertex bare: an unweighted vertex collapses to the origin under skinning,
        // which is far worse than an over-long lever arm. Keep the original in that case.
        if keep.is_empty() {
            continue;
        }
        *infl = keep;
        if infl.len() != before {
            clamped += 1;
        }
        let s: f64 = infl.iter().map(|x| x.1).sum();
        if s > 0.0 {
            for x in infl.iter_mut() {
                x.1 /= s;
            }
        }
    }
    if dominant_hits > 0 {
        eprintln!(
            "  clamp WARNING: {dominant_hits} vertices lost their DOMINANT bone              (gross motion, not a crumb)"
        );
    }
    clamped
}

/// Laplacian smoothing of a transferred weight field over the target's own connectivity.
///
/// Sampling is a POINT operation: each target vertex asks the donor a question independently, so
/// nothing stops two neighbours a millimetre apart from picking different donor points and landing
/// on different bones. Most do agree, but the ones that do not are exactly the vertices that spike —
/// measured on the shipped donor, the worst-case deformation error stays pinned near 12% of body
/// height at every `k`, because more neighbours cannot fix a vertex whose whole neighbourhood was
/// sampled across a body-part boundary. Averaging each vertex's weights toward its mesh neighbours
/// removes the isolated disagreements while leaving genuine boundaries — where a whole run of
/// vertices agrees — where they are.
///
/// `adj` is the target's vertex adjacency (both directions). `lambda` is the blend toward the
/// neighbourhood mean per iteration; 1.0 replaces a vertex with its neighbours' average outright.
/// Results are renormalised and re-truncated to 4 influences, so the output stays encodable.
pub fn smooth_weights(
    per_vertex: &mut Vec<Vec<(u32, f64)>>,
    adj: &[Vec<u32>],
    iters: usize,
    lambda: f64,
) {
    for _ in 0..iters {
        let mut next: Vec<Vec<(u32, f64)>> = Vec::with_capacity(per_vertex.len());
        for (vi, own) in per_vertex.iter().enumerate() {
            let nb = adj.get(vi).map(|x| x.as_slice()).unwrap_or(&[]);
            if nb.is_empty() {
                next.push(own.clone());
                continue;
            }
            let mut acc: HashMap<u32, f64> = HashMap::new();
            for &(b, w) in own.iter() {
                *acc.entry(b).or_insert(0.0) += (1.0 - lambda) * w;
            }
            let share = lambda / nb.len() as f64;
            for &n in nb {
                for &(b, w) in per_vertex[n as usize].iter() {
                    *acc.entry(b).or_insert(0.0) += share * w;
                }
            }
            let mut infl: Vec<(u32, f64)> = acc.into_iter().collect();
            // Deterministic order: weight desc, then bone index, so a tie never depends on HashMap
            // iteration order (which would make a build unreproducible).
            infl.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
            });
            infl.truncate(4);
            let s: f64 = infl.iter().map(|x| x.1).sum();
            if s > 0.0 {
                for x in infl.iter_mut() {
                    x.1 /= s;
                }
            }
            next.push(infl);
        }
        *per_vertex = next;
    }
}

/// Vertex adjacency from a triangle list, both directions, deduplicated.
pub fn adjacency(vertex_count: usize, tris: &[[u32; 3]]) -> Vec<Vec<u32>> {
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); vertex_count];
    let mut add = |a: u32, b: u32, adj: &mut Vec<Vec<u32>>| {
        if (a as usize) < adj.len() && (b as usize) < adj.len() && a != b {
            adj[a as usize].push(b);
        }
    };
    for t in tris {
        for i in 0..3 {
            add(t[i], t[(i + 1) % 3], &mut adj);
            add(t[(i + 1) % 3], t[i], &mut adj);
        }
    }
    for v in adj.iter_mut() {
        v.sort_unstable();
        v.dedup();
    }
    adj
}

/// Area-weighted vertex normals that treat vertices sharing a POSITION as one surface point.
///
/// An exported mesh splits a vertex wherever an attribute other than position is discontinuous — a
/// UV seam, a material change. Those copies are the same point on the surface, but each one only
/// sees the triangles on its own side of the seam, so plain per-index normals come out faceted along
/// every seam: a visible lighting crease down a character that has none. Welding by position first
/// and redistributing the shared normal afterwards is what makes derived normals match what the
/// exporter authored.
///
/// Positions are keyed at 1e-6, which is far below any real vertex spacing and far above f32 noise.
pub fn welded_vertex_normals(positions: &[[f64; 3]], tris: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let key = |p: &[f64; 3]| -> (i64, i64, i64) {
        (
            (p[0] * 1.0e6).round() as i64,
            (p[1] * 1.0e6).round() as i64,
            (p[2] * 1.0e6).round() as i64,
        )
    };
    let mut rep: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut canon = vec![0usize; positions.len()];
    for (i, p) in positions.iter().enumerate() {
        let k = key(p);
        let r = *rep.entry(k).or_insert(i);
        canon[i] = r;
    }
    let mut acc = vec![[0.0f64; 3]; positions.len()];
    for t in tris {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            continue;
        }
        let (p, q, r) = (positions[a], positions[b], positions[c]);
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        let f = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &vi in &[a, b, c] {
            let cv = canon[vi];
            for k in 0..3 {
                acc[cv][k] += f[k];
            }
        }
    }
    let mut out = vec![[0.0f64; 3]; positions.len()];
    for i in 0..positions.len() {
        let n = acc[canon[i]];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        out[i] = if l > 1e-12 { [n[0] / l, n[1] / l, n[2] / l] } else { [0.0, 0.0, 1.0] };
    }
    out
}

/// Area-weighted vertex normals from a triangle list.
///
/// Derived rather than read from the file on purpose. Both sides of the transfer must agree on
/// convention, and the two sides come from different authoring chains — a glTF exporter and a
/// de-stripped IBUF. Deriving both the same way from geometry makes them comparable; the caller
/// still checks the derived field against the stored one and flips if the winding disagrees.
pub fn vertex_normals(positions: &[[f64; 3]], tris: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let mut n = vec![[0.0f64; 3]; positions.len()];
    for t in tris {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            continue;
        }
        let (p, q, r) = (positions[a], positions[b], positions[c]);
        let u = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let v = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        // un-normalised cross = 2x triangle area, so bigger faces weigh more
        let f = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &vi in &[a, b, c] {
            for k in 0..3 {
                n[vi][k] += f[k];
            }
        }
    }
    for x in n.iter_mut() {
        let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
        if l > 1e-12 {
            for k in 0..3 {
                x[k] /= l;
            }
        }
    }
    n
}

/// Result of a transfer, ready to replace a `CharSkin`'s skinning.
pub struct Transferred {
    /// Per vertex, up to 4 (global bone, weight 0..1), normalised and sorted weight-desc.
    pub per_vertex: Vec<Vec<(u32, f64)>>,
    /// Vertices whose nearest donor sample was further than the trust radius — these were filled by
    /// widening the search rather than by a confident local match.
    pub far: usize,
    pub median_dist: f64,
}

/// Sample donor weights at each conformed position.
///
/// `k` nearest donor samples are blended by inverse distance rather than taking a single nearest.
/// A single nearest vertex reproduces the donor's own quantisation seams and can flip binding
/// across a limb boundary on one vertex; blending a few neighbours is what makes the result read as
/// a smooth field. Weights are accumulated PER BONE across the neighbourhood, so a target vertex
/// naturally ends up multi-influence wherever retail is multi-influence — which is the whole point.
pub fn transfer_weights(
    donor: &[DonorSample],
    targets: &[[f64; 3]],
    k: usize,
    body_height: f64,
) -> Transferred {
    transfer_weights_pruned(donor, targets, body_height, &TransferOpts { k, ..Default::default() })
}

/// Knobs for [`transfer_weights_pruned`]. A struct rather than positional arguments because every
/// one of these is swept against the self-test probe, and a seven-argument call is where a sweep
/// silently measures the wrong thing.
pub struct TransferOpts<'a> {
    /// Donor samples blended per target.
    pub k: usize,
    /// Influences below this share are dropped before renormalising. Measured to make deformation
    /// error monotonically WORSE on the shipped donor, so the default is off; kept because the
    /// palette cap sometimes forces a trade.
    pub min_weight: f64,
    /// Per-target surface normals, same order as `targets`. Empty disables the compatibility check.
    pub target_normals: &'a [[f64; 3]],
    /// Donor samples closer than this are ignored. Only for the self-test, where donor and target
    /// are the same mesh and a zero-distance self-match would make the measurement vacuous.
    pub exclude_radius: f64,
}

impl<'a> Default for TransferOpts<'a> {
    fn default() -> Self {
        TransferOpts { k: 4, min_weight: MIN_WEIGHT, target_normals: &[], exclude_radius: 0.0 }
    }
}

/// As [`transfer_weights`], with the knobs exposed.
pub fn transfer_weights_pruned(
    donor: &[DonorSample],
    targets: &[[f64; 3]],
    body_height: f64,
    opts: &TransferOpts,
) -> Transferred {
    let TransferOpts { k, min_weight, target_normals, exclude_radius } = *opts;
    let excl2 = exclude_radius * exclude_radius;
    // Cell ~2% of body height: comfortably above the measured 1.0% median spacing, so the first
    // ring almost always holds a match, and small enough that buckets stay short.
    let grid = Grid::new(donor, (body_height * 0.02).max(1e-4));
    let trust = body_height * 0.05;

    let mut per_vertex = Vec::with_capacity(targets.len());
    let mut dists = Vec::with_capacity(targets.len());
    let mut far = 0usize;
    let mut cand = Vec::new();

    for (ti_idx, t) in targets.iter().enumerate() {
        grid.near(*t, &mut cand);
        // k nearest within the candidate set
        let mut best: Vec<(f64, usize)> = Vec::with_capacity(cand.len());
        for &i in cand.iter() {
            let s = &donor[i].pos;
            let d2 = (t[0] - s[0]).powi(2) + (t[1] - s[1]).powi(2) + (t[2] - s[2]).powi(2);
            if d2 < excl2 {
                continue;
            }
            best.push((d2, i));
        }
        best.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        // NORMAL COMPATIBILITY. Prefer donor samples facing the same way as this point; only fall
        // back to raw proximity if none qualify (a genuine gap, which inpainting-style methods fill
        // the same way). This is what stops an inner-arm vertex sampling the torso it rests against.
        if let Some(tn) = target_normals.get(ti_idx) {
            let compatible: Vec<(f64, usize)> = best
                .iter()
                .copied()
                .filter(|(_, i)| {
                    let dn = &donor[*i].normal;
                    tn[0] * dn[0] + tn[1] * dn[1] + tn[2] * dn[2] > NORMAL_MIN_DOT
                })
                .collect();
            if !compatible.is_empty() {
                best = compatible;
            }
        }
        best.truncate(k.max(1));

        let nearest = best.first().map(|b| b.0.sqrt()).unwrap_or(f64::MAX);
        dists.push(nearest);
        if nearest > trust {
            far += 1;
        }

        // inverse-distance blend, accumulated per bone
        let mut acc: HashMap<u32, f64> = HashMap::new();
        let mut wsum = 0.0;
        for (d2, i) in &best {
            let w = 1.0 / (d2.sqrt() + 1e-6);
            wsum += w;
            for (bone, bw) in &donor[*i].infl {
                *acc.entry(*bone).or_insert(0.0) += w * bw;
            }
        }
        let mut infl: Vec<(u32, f64)> = if wsum > 0.0 {
            acc.into_iter().map(|(b, w)| (b, w / wsum)).collect()
        } else {
            Vec::new()
        };
        // top 4, then PRUNE the tail, then renormalise.
        //
        // Blending k neighbours picks up a little weight from bones that merely happen to be near
        // in space — a bone on the far side of a limb, or across a joint. Those crumbs are what
        // inflate a silhouette: linear-blend skinning averages every contributing bone's transform,
        // so a vertex tugged weakly by several divergent bones sits proud of where any single bone
        // would put it. In game that reads as the character looking THICKER than the source model,
        // while the bind pose looks perfect (at bind every transform is identity, so weights cannot
        // show). Retail carries ~20-23% of vertices at a full four influences; blending k=4 without
        // pruning produced 58.3% in one group.
        //
        // The threshold is in the format's own terms: a weight is stored as u8/255, so anything
        // below MIN_WEIGHT rounds to a couple of quantisation steps and cannot be load-bearing.
        infl.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        infl.truncate(4);
        if let Some(&(_, top)) = infl.first() {
            // keep anything meaningful in absolute terms, and never drop the dominant bone
            infl.retain(|&(_, w)| w >= min_weight || w >= top);
        }
        let s: f64 = infl.iter().map(|x| x.1).sum();
        if s > 0.0 {
            for x in infl.iter_mut() {
                x.1 /= s;
            }
        }
        per_vertex.push(infl);
    }

    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median_dist = dists.get(dists.len() / 2).copied().unwrap_or(0.0);
    Transferred { per_vertex, far, median_dist }
}
