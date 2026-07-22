//! Validation battery — faithful port of `mercs2-mesher/src/validate.js`.
//!
//! Five independent geometric checks plus static limits. Deliberately NOT a single score:
//! each check exists because a build passed everything before it and still failed on
//! screen. Reference numbers come from measuring shipped Mercs2 characters.

use super::build::CharSkin;
use super::mat::*;
use std::collections::HashMap;

/// Shipped-Mercs2 baseline for the bone-distance check.
pub const SHIPPED_BONE_DISTANCE: (f64, f64, f64) = (0.136, 0.124, 0.328); // mean, median, p95
/// Hard format ceiling: IBUF indices are u16 (`model_inject.rs:813`), so no single draw group
/// may exceed this many indices — and no mesh more than this many vertices.
pub const U16_INDEX_CEILING: usize = 65535;

/// Minimum share of vertices carrying 2+ bone influences, taken from shipped characters rather
/// than from how much of the source survived. Retail group-3 measurements (`mercs2_probe --bin
/// skin_census --group 3`): pmc_hum_mattias 82.8% multi / 17.2% rigid, pmc_hum_chris 93.3% / 6.7%,
/// both with ~20% of vertices at a full 4 influences. The floor sits below the lower observation,
/// not at it, so a legitimately blockier character is not failed for being blocky.
pub const SHIPPED_MULTI_INFLUENCE_MIN: f64 = 0.60;

/// Stale: 65535/6, i.e. the u16 ceiling divided by the flat 6.0 idx/tri the NAIVE
/// `model_inject::to_strip` costs. It is not a game limit and not a per-model limit. Kept only
/// so external callers still resolve; the `tris` check now measures the real encoder cost.
#[deprecated(note = "measure strip_index_cost() instead - the cap is per-group and encoder-dependent")]
pub const STRIP_TRI_CAP: usize = 10900;

/// Index cost of this triangle set under the adjacency strip encoder actually used to write
/// dense meshes (`to_strip_connected`), which chains shared-edge runs at ~1 idx/tri inside a run
/// and measures ~2.8 idx/tri over a whole character.
pub fn strip_index_cost(indices: &[u32]) -> usize {
    let tris: Vec<[u32; 3]> = indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    if tris.is_empty() {
        return 0;
    }
    crate::model_inject::to_strip_connected(&tris).len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Bad,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    pub value: f64,
    pub text: String,
    pub reference: &'static str,
    pub status: Status,
}

#[derive(Debug, Clone)]
pub struct Limit {
    pub id: &'static str,
    pub title: &'static str,
    pub ok: bool,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub checks: Vec<Check>,
    pub limits: Vec<Limit>,
    pub worst: Status,
}

fn tri_area(a: V3, b: V3, c: V3) -> f64 {
    0.5 * len(cross(sub(b, a), sub(c, a)))
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 * p).floor() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn sign(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

fn centroid_of(pts: &[[f64; 3]], dom: &[i64], hier: u32) -> Option<[f64; 3]> {
    let mut n = 0.0;
    let mut acc = [0.0f64; 3];
    for (i, p) in pts.iter().enumerate() {
        if dom[i] == hier as i64 {
            acc[0] += p[0];
            acc[1] += p[1];
            acc[2] += p[2];
            n += 1.0;
        }
    }
    if n > 0.0 {
        Some([acc[0] / n, acc[1] / n, acc[2] / n])
    } else {
        None
    }
}

/// Run the full battery over a completed [`CharSkin`]. `vjoints`/`vweights`/`indices` are
/// the same source arrays passed to `build_character`.
pub fn validate(
    cs: &CharSkin,
    vjoints: &[[u16; 4]],
    vweights: &[[f64; 4]],
    indices: &[u32],
) -> Report {
    let nv = cs.posed.len();
    let sk = &cs; // for readability
    let tgt = |h: u32| -> Option<[f64; 3]> { None.or_else(|| tgt_lookup(cs, h)) };

    // dominant target bone per vertex (highest weight), -1 if none.
    let mut dom = vec![-1i64; nv];
    for vi in 0..nv {
        let mut bw = -1.0;
        let mut best = -1i64;
        for k in 0..4 {
            let w = vweights[vi][k];
            if w > bw {
                if let Some(&h) = sk.full.get(&(vjoints[vi][k] as usize)) {
                    bw = w;
                    best = h as i64;
                }
            }
        }
        dom[vi] = best;
    }

    let mut checks: Vec<Check> = Vec::new();

    // ---- 1. bone distance ----
    {
        let mut d: Vec<f64> = Vec::new();
        for vi in 0..nv {
            if dom[vi] < 0 {
                continue;
            }
            if let Some(t) = tgt(dom[vi] as u32) {
                d.push(len(sub(cs.posed[vi], t)));
            }
        }
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = if d.is_empty() {
            0.0
        } else {
            d.iter().sum::<f64>() / d.len() as f64
        };
        // TWO-SIDED. This check used to pass on `mean < 2x shipped`, i.e. the SMALLER the better —
        // which rewards exactly the failure it should catch. A re-pose that snaps every vertex onto
        // its bone drives this toward zero while tearing the surface open: the mesher's own
        // translation-only re-pose scores 0.107 on 50 Cent (BETTER than the shipped 0.136) with
        // 8.4% of its edges stretched past 2x and a visible chasm between the shoulders. The value
        // is a proxy for how much flesh a bone carries, so the target is to MATCH the shipped
        // number, not to minimise it. Shipped heroes measure 0.146 / 0.128 / 0.127
        // (mattias_v2 / jen / chris).
        // Band: the three shipped heroes span 0.127..0.146, i.e. +/-7% about 0.136. 0.85x..1.45x
        // (0.116..0.197) is a generous envelope around that and still rejects both failure modes —
        // the mesher's snapped 0.107 and a 20%-over-scaled import's 0.198.
        let lo = SHIPPED_BONE_DISTANCE.0 * 0.85;
        let hi = SHIPPED_BONE_DISTANCE.0 * 1.45;
        let status = if mean >= lo && mean <= hi {
            Status::Ok
        } else if mean >= SHIPPED_BONE_DISTANCE.0 * 0.5 && mean < 0.4 {
            Status::Warn
        } else {
            Status::Bad
        };
        checks.push(Check {
            id: "bone-distance",
            title: "Bone distance",
            value: mean,
            text: format!(
                "mean {:.3} / median {:.3} / p95 {:.3}",
                mean,
                pct(&d, 0.5),
                pct(&d, 0.95)
            ),
            reference: "shipped: mean 0.136 / median 0.124 / p95 0.328 (match it — too LOW means the geometry was snapped onto the bones)",
            status,
        });
    }

    // ---- 2. triangle area ratio ----
    if !indices.is_empty() {
        let mut ratios: Vec<f64> = Vec::new();
        let mut collapsed = 0usize;
        let mut i = 0;
        while i + 2 < indices.len() {
            let (a, b, c) = (
                indices[i] as usize,
                indices[i + 1] as usize,
                indices[i + 2] as usize,
            );
            let a0 = tri_area(cs.cp[a], cs.cp[b], cs.cp[c]);
            let a1 = tri_area(cs.posed[a], cs.posed[b], cs.posed[c]);
            if a0 >= 1e-12 {
                let r = a1 / a0;
                ratios.push(r);
                if r < 0.3 {
                    collapsed += 1;
                }
            }
            i += 3;
        }
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = pct(&ratios, 0.5);
        let collapsed_pct = 100.0 * collapsed as f64 / ratios.len().max(1) as f64;
        let status = if collapsed_pct < 2.0 && med > 0.6 && med < 1.8 {
            Status::Ok
        } else if collapsed_pct < 10.0 {
            Status::Warn
        } else {
            Status::Bad
        };
        checks.push(Check {
            id: "area-ratio",
            title: "Triangle area",
            value: med,
            text: format!(
                "median {:.3}x, {:.1}% of triangles collapsed below 0.3x",
                med, collapsed_pct
            ),
            reference: "want median near 1.0 and <2% collapsed",
            status,
        });
    }

    // ---- 2b. edge stretch ----
    //
    // The check that was MISSING, and the only one that separated a torn build from an intact one
    // during this audit. Triangle AREA hides a tear: a triangle stretched 8x along one edge and
    // squashed across the other keeps its area, so the mesher's torn 50 Cent scored a clean
    // "median 1.000x". Per-EDGE length ratio cannot be cancelled that way.
    //
    // Some stretch is CORRECT — a donor thigh 0.35 m long on a 0.48 m target bone has to grow 1.38x
    // — so the gate is on the far tail (>2x, and the crushed <0.5x tail), not on the median.
    if !indices.is_empty() {
        let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
        let mut ratios: Vec<f64> = Vec::new();
        let mut i = 0;
        while i + 2 < indices.len() {
            let t = [indices[i], indices[i + 1], indices[i + 2]];
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if !seen.insert((a.min(b), a.max(b))) {
                    continue;
                }
                let (a, b) = (a as usize, b as usize);
                let l0 = len(sub(cs.cp[a], cs.cp[b]));
                if l0 > 1e-9 {
                    ratios.push(len(sub(cs.posed[a], cs.posed[b])) / l0);
                }
            }
            i += 3;
        }
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = ratios.len().max(1) as f64;
        let torn = 100.0 * ratios.iter().filter(|&&r| r > 2.0).count() as f64 / n;
        let crushed = 100.0 * ratios.iter().filter(|&&r| r < 0.5).count() as f64 / n;
        let worst = ratios.last().copied().unwrap_or(1.0);
        let status = if torn < 0.5 && crushed < 0.5 {
            Status::Ok
        } else if torn < 2.0 && crushed < 2.0 {
            Status::Warn
        } else {
            Status::Bad
        };
        checks.push(Check {
            id: "edge-stretch",
            title: "Edge stretch",
            value: torn,
            text: format!(
                "{torn:.2}% of edges stretched >2x, {crushed:.2}% crushed <0.5x, p99 {:.2}x, worst {worst:.1}x",
                pct(&ratios, 0.99)
            ),
            reference: "want <0.5% either tail; the mesher's translation-only re-pose scores 8.4% torn / 46x worst",
            status,
        });
    }

    // ---- 3. limb direction ----
    {
        const LIMBS: [(&str, u32, u32); 4] = [
            ("left arm", 43, 46),
            ("right arm", 64, 67),
            ("left leg", 6, 8),
            ("right leg", 10, 12),
        ];
        // LIMBS are CANONICAL NPC-84 indices; `dom`/`tgt` are in the donor's HIER, so resolve each
        // onto this donor by name-hash (else the check silently finds nothing on a HERO target).
        let resolve = |npc: u32| -> Option<u32> {
            let h = super::npc84_name_hash(npc)?;
            cs.skeleton_bones.iter().find(|b| b.name_hash == h).map(|b| b.i)
        };
        let mut rows: Vec<(&str, f64)> = Vec::new();
        for (label, npc_prox, npc_dist) in LIMBS {
            let (Some(prox), Some(dist)) = (resolve(npc_prox), resolve(npc_dist)) else {
                continue;
            };
            let (Some(cp), Some(cd)) = (
                centroid_of(&cs.posed, &dom, prox),
                centroid_of(&cs.posed, &dom, dist),
            ) else {
                continue;
            };
            let (Some(tp), Some(td)) = (tgt(prox), tgt(dist)) else {
                continue;
            };
            let mesh_dir = norm(sub(cd, cp));
            let bone_dir = norm(sub(td, tp));
            if len(mesh_dir) < 1e-6 || len(bone_dir) < 1e-6 {
                continue;
            }
            let deg = dot(mesh_dir, bone_dir).clamp(-1.0, 1.0).acos() * 180.0 / std::f64::consts::PI;
            rows.push((label, deg));
        }
        let worst = rows.iter().map(|r| r.1).fold(0.0, f64::max);
        let status = if rows.is_empty() {
            Status::Warn
        } else if worst < 10.0 {
            Status::Ok
        } else if worst < 25.0 {
            Status::Warn
        } else {
            Status::Bad
        };
        let text = if rows.is_empty() {
            "no limb bones weighted -- cannot check".to_string()
        } else {
            rows.iter()
                .map(|(l, d)| format!("{l} {:.1}°", d))
                .collect::<Vec<_>>()
                .join(", ")
        };
        checks.push(Check {
            id: "limb-direction",
            title: "Limb direction",
            value: worst,
            text,
            reference: "good is ~3°; the folded-arm bug read 28°",
            status,
        });
    }

    // ---- 4. bind-height chain ----
    {
        use super::automap::Origin;
        struct Entry {
            pos: [f64; 3],
            direct: bool,
        }
        let mut per_hier: HashMap<u32, Entry> = HashMap::new();
        let mut keys: Vec<usize> = cs.full.keys().copied().collect();
        keys.sort_unstable();
        for j in keys {
            let h = cs.full[&j];
            let Some(&p) = cs.srcp.get(&j) else { continue };
            let is_direct = matches!(cs.origin.get(&j), Some(Origin::Auto) | Some(Origin::Manual));
            match per_hier.get(&h) {
                Some(e) if !(is_direct && !e.direct) => {}
                _ => {
                    per_hier.insert(h, Entry { pos: p, direct: is_direct });
                }
            }
        }
        let mut bad: Vec<String> = Vec::new();
        let mut checked = 0usize;
        for b in &cs.skeleton_bones {
            if b.parent < 0 {
                continue;
            }
            let (Some(c), Some(p)) = (per_hier.get(&b.i), per_hier.get(&(b.parent as u32))) else {
                continue;
            };
            let (Some(ti), Some(tp)) = (tgt(b.i), tgt(b.parent as u32)) else {
                continue;
            };
            let tdy = ti[1] - tp[1];
            if tdy.abs() < 0.05 {
                continue;
            }
            checked += 1;
            let sdy = c.pos[1] - p.pos[1];
            if sign(sdy) != sign(tdy) && sdy.abs() > 0.02 {
                bad.push(b.name.clone());
            }
        }
        let status = if checked == 0 {
            Status::Warn
        } else if bad.is_empty() {
            Status::Ok
        } else if bad.len() <= 2 {
            Status::Warn
        } else {
            Status::Bad
        };
        let text = if checked == 0 {
            "no parent-child bone pairs to compare -- the retarget uses too few target bones"
                .to_string()
        } else if bad.is_empty() {
            format!("all {checked} vertical parent-child steps agree in direction")
        } else {
            format!("{}/{} parent-child steps run the WRONG WAY", bad.len(), checked)
        };
        checks.push(Check {
            id: "bind-chain",
            title: "Bind-height chain",
            value: bad.len() as f64,
            text,
            reference: "any inversion means bind positions came from mixed coordinate spaces",
            status,
        });
    }

    // ---- 5. overall height ----
    {
        // Measured from the three shipped hero meshes in their own bind pose: mattias_v2 1.847,
        // jen 1.850, chris 1.820 — all three within 3 cm of each other DESPITE bone extents of
        // 1.825 / 1.757 / 1.731, so the character height is a property of the game, not of which
        // skeleton you retarget onto. The old 1.5..2.2 gate passed a 2.26 m import silently.
        let h = cs.stats.height;
        let status = if (1.78..=1.90).contains(&h) {
            Status::Ok
        } else if (1.60..=2.10).contains(&h) {
            Status::Warn
        } else {
            Status::Bad
        };
        checks.push(Check {
            id: "height",
            title: "Character height",
            value: h,
            text: format!("{h:.3} m"),
            reference: "shipped meshes: 1.847 / 1.850 / 1.820 m (mattias_v2 / jen / chris)",
            status,
        });
    }

    // ---- static limits ----
    let limits = vec![
        Limit {
            id: "palette",
            title: "Palette slots",
            ok: cs.stats.palette_slots <= 46,
            text: format!("{} / 46", cs.stats.palette_slots),
        },
        Limit {
            id: "ranges",
            title: "Palette runs",
            ok: cs.stats.range_count <= 8,
            text: format!("{} / 8", cs.stats.range_count),
        },
        Limit {
            id: "tris",
            title: "Triangles",
            // The real ceiling is the u16 INDEX buffer (65535), not a triangle count. What a
            // model costs depends entirely on which encoder writes it: the naive per-triangle
            // `to_strip` burns a flat 6.0 idx/tri, while the adjacency `to_strip_connected`
            // measures ~2.8 on real characters. Measure this mesh rather than assume either.
            // Over-budget is not fatal: `inject_parts_into_donor_block` partitions a dense mesh
            // across several host groups, so this ceiling is PER GROUP, not per model.
            ok: strip_index_cost(indices) <= U16_INDEX_CEILING,
            text: {
                let ic = strip_index_cost(indices);
                let per = ic as f64 / cs.stats.tris.max(1) as f64;
                format!(
                    "{} tris -> {} idx ({:.2}/tri) / {} u16 ceiling{}",
                    cs.stats.tris,
                    ic,
                    per,
                    U16_INDEX_CEILING,
                    if ic > U16_INDEX_CEILING {
                        format!(" - needs {} groups", ic.div_ceil(U16_INDEX_CEILING))
                    } else {
                        String::new()
                    }
                )
            },
        },
        Limit {
            id: "influence",
            title: "Multi-bone influence",
            // Gate on the SHIPPED distribution, not on how much of the source survived. Retention
            // answers "did we lose weights in translation"; it cannot answer "is this riggable",
            // and it passed a build that was unusable. Measured on retail group 3 (skin_census):
            // mattias 82.8% multi / 17.2% rigid, chris 93.3% / 6.7%, and ~20% of vertices carry a
            // full 4 influences. An import at 14.6% multi / 85.4% rigid / 0.0% four-influence is
            // FAITHFUL but far too coarse — rigid chunks that survive bind pose and split the
            // moment a joint bends. That is the animation tearing, and the old check called it Ok.
            ok: SHIPPED_MULTI_INFLUENCE_MIN <= (cs.stats.multi_influence as f64 / cs.stats.verts.max(1) as f64),
            text: format!(
                "{:.1}% of verts have 2+ bones ({} of {}); shipped is {:.0}-{:.0}%. \
                 Retained {:.0}% of the source's multi-influence verts.",
                100.0 * cs.stats.multi_influence as f64 / cs.stats.verts.max(1) as f64,
                cs.stats.multi_influence,
                cs.stats.verts,
                100.0 * SHIPPED_MULTI_INFLUENCE_MIN,
                93.3,
                100.0 * cs.stats.influence_retained
            ),
        },
    ];

    let worst = if checks.iter().any(|c| c.status == Status::Bad) {
        Status::Bad
    } else if checks.iter().any(|c| c.status == Status::Warn) || limits.iter().any(|l| !l.ok) {
        Status::Warn
    } else {
        Status::Ok
    };
    let _ = sk;
    Report {
        checks,
        limits,
        worst,
    }
}

fn tgt_lookup(cs: &CharSkin, h: u32) -> Option<[f64; 3]> {
    cs.skeleton_bones.iter().find(|b| b.i == h).map(|b| b.pos)
}
