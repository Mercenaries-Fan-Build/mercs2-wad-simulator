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

/// One donor surface sample: where it is, and what retail binds it to.
pub struct DonorSample {
    pub pos: [f64; 3],
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
    // Cell ~2% of body height: comfortably above the measured 1.0% median spacing, so the first
    // ring almost always holds a match, and small enough that buckets stay short.
    let grid = Grid::new(donor, (body_height * 0.02).max(1e-4));
    let trust = body_height * 0.05;

    let mut per_vertex = Vec::with_capacity(targets.len());
    let mut dists = Vec::with_capacity(targets.len());
    let mut far = 0usize;
    let mut cand = Vec::new();

    for t in targets {
        grid.near(*t, &mut cand);
        // k nearest within the candidate set
        let mut best: Vec<(f64, usize)> = Vec::with_capacity(cand.len());
        for &i in cand.iter() {
            let s = &donor[i].pos;
            let d2 = (t[0] - s[0]).powi(2) + (t[1] - s[1]).powi(2) + (t[2] - s[2]).powi(2);
            best.push((d2, i));
        }
        best.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
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
        // top 4, renormalised — the format carries exactly four influences per vertex
        infl.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        infl.truncate(4);
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
