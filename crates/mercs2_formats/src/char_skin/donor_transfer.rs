//! Rewrite a conformed `CharSkin`'s weights by SAMPLING the shipped donor, in one call.
//!
//! This is the step the CLI (`xfer_apply`) did and the workshop did NOT, and it is the whole
//! difference between a clean import and one whose arms tear. `char_skin::build_character` carries
//! the SOURCE rig's own weights across the bone map; for a dense or mismatched rig (a 119-bone
//! Unreal mannequin onto a 116-bone Pandemic skeleton) that map is fuzzy on the limbs and the arms
//! go rigid. Donor transfer throws those weights away and takes, at each conformed vertex, the
//! weights the RETAIL donor uses at that point in space — so a fuzzy arm mapping cannot reach the
//! result.
//!
//! It rewrites `skin_bytes` + `ranges` in place against a WHOLE-MODEL palette. That palette can
//! exceed the 48-slot per-group cap the game enforces, which is fine for PREVIEW and for the
//! multi-group injector (each group re-derives its own sub-palette); a single-group export must
//! still split. The settled parameters are the ones measured on 50 Cent: k=8, smooth 2 / lambda
//! 0.5, reach 1.15, axial 3.

use super::build::{build_palette_ranges, CharSkin};
use super::transfer::{
    adjacency, clamp_to_donor_reach, smooth_weights, transfer_weights_pruned, vertex_normals,
    DonorSample, TransferOpts,
};
use crate::model_cubeize::read_model_meshes;
use crate::skeleton::Skeleton;

/// Settled transfer knobs (see module doc). Defaults are what the deployed 50 Cent uses.
pub struct DonorTransferOpts {
    pub k: usize,
    pub smooth: usize,
    pub lambda: f64,
    pub reach: f64,
    pub axial: f64,
}
impl Default for DonorTransferOpts {
    fn default() -> Self {
        DonorTransferOpts { k: 8, smooth: 2, lambda: 0.5, reach: 1.15, axial: 3.0 }
    }
}

/// Sample `donor_block`'s retail weights onto `cs` and rewrite its skinning. `tris` are the import's
/// triangles (for target normals + the smoothing graph). Returns a one-line summary for logging.
pub fn apply_donor_transfer(
    cs: &mut CharSkin,
    tris: &[[u32; 3]],
    donor_block: &[u8],
    opts: &DonorTransferOpts,
) -> Result<String, String> {
    let skel = Skeleton::from_block(donor_block)?;
    let height = {
        let ys: Vec<f64> = cs.posed.iter().map(|p| p[1]).collect();
        ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min)
    };
    if height <= 0.0 {
        return Err("import has zero height; cannot size the transfer grid".into());
    }

    // Donor surface samples: every skinned donor vertex with its GLOBAL bones and a geometric
    // normal re-wound to the stored winding (same as xfer_apply).
    let ucfx_len =
        u32::from_le_bytes(donor_block[16..20].try_into().map_err(|_| "short block")?) as usize;
    let meshes = read_model_meshes(&donor_block[20..20 + ucfx_len])?;
    let mut donor: Vec<DonorSample> = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        let mpos: Vec<[f64; 3]> =
            m.positions.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let mut dn = vertex_normals(&mpos, &m.tris);
        if !m.normals.is_empty() {
            let agree: f64 = (0..dn.len().min(m.normals.len()))
                .map(|i| {
                    let s = m.normals[i];
                    dn[i][0] * s[0] as f64 + dn[i][1] * s[1] as f64 + dn[i][2] * s[2] as f64
                })
                .sum();
            if agree < 0.0 {
                for x in dn.iter_mut() {
                    for k in 0..3 {
                        x[k] = -x[k];
                    }
                }
            }
        }
        for i in 0..mpos.len() {
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            let infl = (0..4)
                .filter(|&c| m.weights[i][c] > 0)
                .map(|c| (m.joints[i][c] as u32, m.weights[i][c] as f64 / tot))
                .collect();
            donor.push(DonorSample {
                pos: mpos[i],
                normal: dn.get(i).copied().unwrap_or([0.0; 3]),
                infl,
            });
        }
    }
    if donor.is_empty() {
        return Err("donor block carried no skinned vertices to sample".into());
    }

    let tnorm = vertex_normals(&cs.posed, tris);
    let bone_pos: Vec<[f64; 3]> = skel
        .bones
        .iter()
        .map(|b| {
            let p = b.bind_pos();
            [p[0] as f64, p[1] as f64, p[2] as f64]
        })
        .collect();
    let bone_parent: Vec<i32> = skel.bones.iter().map(|b| b.parent).collect();

    let mut t = transfer_weights_pruned(
        &donor,
        &cs.posed,
        height,
        &TransferOpts {
            k: opts.k,
            min_weight: 0.0,
            target_normals: &tnorm,
            exclude_radius: 0.0,
            bone_pos: &bone_pos,
            bone_parent: &bone_parent,
            axial_penalty: opts.axial,
        },
    );
    if opts.reach > 0.0 {
        clamp_to_donor_reach(&mut t.per_vertex, &cs.posed, &donor, &bone_pos, 0.99, opts.reach);
    }
    if opts.smooth > 0 {
        let adj = adjacency(cs.posed.len(), tris);
        smooth_weights(&mut t.per_vertex, &adj, opts.smooth, opts.lambda);
    }

    // ---- RIGID HAND (weight side) ----
    //
    // The conform already moves the hand as one rigid unit (`build.rs` §6c), but the sampled donor
    // weights still name the individual finger bones, so at runtime a clip would drive them apart and
    // shear the hand anyway. Measured on 50 Cent: the body conforms uniformly (p90 0.88x edge change,
    // 1.1% of edges disturbed) while the hand hit p90 1.57x, extremes 0.15x..8.13x, and 9.9% of edges
    // stretched past 2x or crushed below half — nine times the body's rate. The source hand is 146
    // verts total (~28 per finger) against the donor's 630, so there is no density to absorb shear and
    // no articulation worth having at that resolution. Fold every finger influence onto its hand bone:
    // the hand then deforms as a unit under any clip. Weapons are unaffected (they attach to the hand
    // bone, not the fingers).
    {
        let hier_of = |npc: u32| -> Option<u32> {
            let hash = super::npc84_name_hash(npc)?;
            skel.bones.iter().find(|b| b.name_hash == hash).map(|b| b.index as u32)
        };
        let mut finger_to_hand: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for (hand_npc, lo, hi) in [(46u32, 48u32, 62u32), (67, 69, 83)] {
            if let Some(hand) = hier_of(hand_npc) {
                for npc in lo..=hi {
                    if let Some(f) = hier_of(npc) {
                        finger_to_hand.insert(f, hand);
                    }
                }
            }
        }
        let mut folded = 0usize;
        for infl in t.per_vertex.iter_mut() {
            if !infl.iter().any(|(b, _)| finger_to_hand.contains_key(b)) {
                continue;
            }
            let mut acc: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
            for (b, w) in infl.iter() {
                *acc.entry(finger_to_hand.get(b).copied().unwrap_or(*b)).or_insert(0.0) += *w;
            }
            let mut v: Vec<(u32, f64)> = acc.into_iter().collect();
            v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            *infl = v;
            folded += 1;
        }
        if folded > 0 {
            eprintln!("  rigid hand: folded finger influences onto the hand bone for {folded} vertices");
        }
    }

    // Whole-model palette from the bones the transfer actually used.
    let mut used: Vec<u32> = t.per_vertex.iter().flatten().map(|x| x.0).collect();
    used.sort_unstable();
    used.dedup();
    let (ranges32, slot_of, slots) = build_palette_ranges(&used);

    let nv = cs.posed.len();
    let mut skin = vec![0u8; nv * 8];
    let mut multi = 0usize;
    for (vi, infl) in t.per_vertex.iter().enumerate() {
        // Quantise to 255 with the residual on the largest fractional part (build.rs policy).
        let scaled: Vec<(u8, f64)> =
            infl.iter().filter_map(|(b, w)| slot_of.get(b).map(|&s| (s, 255.0 * w))).collect();
        let mut q: Vec<(u8, i64)> = scaled.iter().map(|&(s, x)| (s, x.floor() as i64)).collect();
        let rem = 255 - q.iter().map(|p| p.1).sum::<i64>();
        let mut order: Vec<usize> = (0..scaled.len()).collect();
        order.sort_by(|&x, &y| {
            (scaled[y].1 - scaled[y].1.floor())
                .partial_cmp(&(scaled[x].1 - scaled[x].1.floor()))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in 0..rem.max(0) as usize {
            if q.is_empty() {
                break;
            }
            let idx = order[i % q.len()];
            q[idx].1 += 1;
        }
        if q.iter().filter(|p| p.1 > 0).count() > 1 {
            multi += 1;
        }
        for (i, (s, w)) in q.iter().take(4).enumerate() {
            skin[vi * 8 + i] = *s;
            skin[vi * 8 + 4 + i] = (*w).clamp(0, 255) as u8;
        }
    }

    cs.skin_bytes = skin;
    cs.ranges = ranges32.iter().map(|&(b, c)| (b as u16, c as u16)).collect();
    cs.palette_slots = slots;
    Ok(format!(
        "donor transfer: {} donor samples, {} bones / {slots} slots, {:.1}% multi-influence",
        donor.len(),
        used.len(),
        100.0 * multi as f64 / nv as f64
    ))
}
