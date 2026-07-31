//! One rigged `.glb` + one donor block → the injected character block.
//!
//! This is the whole skinned-character lowering, in one place, because it has to be. The Workshop
//! previews it and the Quartermaster ships it; if those were two code paths, "what you saw is what
//! you get" would be a hope rather than a property. Everything here was previously spelled out
//! inline in `mercs2_workshop::app::faithful_char_skin`, where nothing else could reach it.
//!
//! The sequence, and why each step is not optional:
//!
//! 1. **Donor → `TargetSkeleton`.** The donor supplies the HIER rig the result must be posed onto.
//! 2. **`build_character`.** Re-poses the source mesh onto that rig and emits palette-relative
//!    `BLENDINDICES` plus the matching `INFO(56)` range table — the shipped format. Hand-authored
//!    global joint indices on a character group are simply wrong.
//! 3. **Donor transfer** (conditional). `build_character` carries the SOURCE rig's weights across
//!    the bone map, which is fuzzy on the limbs for a mismatched rig and tears the arms. Resampling
//!    the retail donor's own weights at each conformed vertex repairs that.
//! 4. **Host group choice.** The largest donor drawing group whose budget the mesh fits.
//! 5. **Inject.** Geometry rebuilt into a container the engine already accepts, name re-stamped.

use std::collections::HashMap;

use crate::char_skin::{
    build_character, donor_transfer, CharGlbData, CharSkin, TargetSkeleton,
};
use crate::model_inject::{
    drawing_group_caps, inject_character_into_donor_block, ExternalMesh, InjectStats, MtrlRepoint,
};
use crate::skeleton::Skeleton;

/// How to lower one character.
#[derive(Default)]
pub struct LowerOpts {
    /// Manual source-joint → target-HIER overrides, layered on top of the automap.
    ///
    /// For a rig whose convention has a hand-verified table (`SourceRig::CallOfDuty`,
    /// `ValveBiped`, `Pandemic`) the caller passes that table as FULL overrides; the generic
    /// automap misreads those namings. See `crate::retarget`.
    pub overrides: HashMap<usize, Option<u32>>,

    /// The source is already on the game's own rig, so **keep the author's weights**.
    ///
    /// Donor transfer exists to repair weights a fuzzy bone map mangled. On a model authored
    /// against the exported base kit there is no fuzzy map and its weights already ARE the retail
    /// ones — resampling them from the donor would discard every weight the author deliberately
    /// painted on new geometry and replace it with whatever the stock body does at that point in
    /// space. That is the one thing an authoring round-trip must never do.
    pub native_rig: bool,

    /// MTRL repoints applied to the host group, so the injected geometry wears its own skin rather
    /// than the donor's. Slot order is `0 = diffuse, 1 = SPECULAR, 2 = NORMAL`.
    pub repoints: Vec<MtrlRepoint>,
}

/// What the lowering produced, for a caller that wants to report on it.
pub struct Lowered {
    pub block: Vec<u8>,
    pub skin: CharSkin,
    pub stats: InjectStats,
    /// Donor-transfer outcome, or why it was skipped. Worth surfacing: a silently skipped transfer
    /// is the difference between clean arms and torn ones.
    pub transfer: String,
    /// The first donor drawing group hosting the mesh — `hosts[0]`, kept for terse logging.
    pub host_group: usize,
    /// Every donor drawing group the mesh was split across. More than one is normal: retail's own
    /// `pmc_hum_mattias` is 22 skinned groups, and no single group's budget holds a whole character.
    pub hosts: Vec<usize>,
    /// Everything the build wanted to tell the author, in one place.
    ///
    /// `build_character` and `donor_transfer` both populate `CharSkin::warnings`, and until this
    /// field existed **nothing on the shipping path read them** — including the one that says an
    /// extremity will be stranded in space. A warning nobody prints is a warning that was not
    /// issued.
    pub warnings: Vec<String>,
    /// Per-host vertex/index budget audit from the injector. Discarded into `_audits` before, which
    /// meant a group that only just fit and one that had room to spare read identically.
    pub audits: Vec<crate::model_inject::GroupBudgetAudit>,
    /// The skinning validation report. The Workshop ran this on its preview and the Quartermaster
    /// ran nothing, so the shipped path had strictly less checking than the preview of it.
    pub report: crate::char_skin::validate::Report,
}

/// The faithful triangle→host partition: one source part per host group, split further wherever a
/// group's bone palette would overflow.
pub struct Partition {
    /// Triangles REORDERED by `(part, dominant bone)`. Index-parallel with [`Partition::tri_group`]
    /// — the injector reads the two positionally, so they must be handed over together.
    pub tris: Vec<[u32; 3]>,
    /// Host slot for each triangle of [`Partition::tris`].
    pub tri_group: Vec<usize>,
    /// Source part index behind each host slot. `seg_part.len()` is how many host groups the split
    /// actually needs, discovered rather than guessed.
    pub seg_part: Vec<usize>,
}

/// Partition a conformed character the way retail authors one.
///
/// Lifted from `mercs2_poc::bin::xfer_apply`, which shipped this before `char_lower` existed and is
/// where the measurements in these comments come from. It was trapped in a `fn main()`, not by any
/// type — every input is public `mercs2_formats` API.
///
/// **Sub-object first, then contiguous bone span.** Retail authors a character as many small draw
/// groups (shipped mattias: 22, bone counts 2/9/2/48/27/6/...), and their bone sets are near
/// CONTIGUOUS in HIER index — group 3 packs 48 bones into 48 slots over 5 runs, chris 45 into 45
/// over 7, with zero gap bridging. A group is a body REGION and HIER indices are hierarchical, so a
/// region is an index range. Ordering triangles by `(part, dominant bone)` makes any cut fall on
/// such a range.
///
/// **One part per group is required, not preferred.** A draw group carries exactly one material, so
/// a group spanning two parts cannot be textured — whichever material it names is wrong for one of
/// them. The even-triangle split straddles part boundaries by construction (50 Cent's parts are
/// 9173/6191/320 triangles against a 3921 chunk), which is why an import could never wear its own
/// textures no matter what was packed alongside it.
///
/// **The cut is on SLOTS, not distinct bones.** The injector re-derives each group's palette with
/// `build_palette_ranges`, which bridges small gaps between runs — so 46 distinct bones can occupy
/// 49 slots. Counting bones and hoping for headroom is how that surfaced as a late panic; count
/// what the injector counts.
pub fn faithful_partition(
    glb: &CharGlbData,
    skin: &CharSkin,
    global_joints: &[[u8; 4]],
) -> Partition {
    let dom_of = |tri: &[u32; 3]| -> u32 {
        let mut best = (0u8, u32::MAX);
        for &v in tri {
            let vi = v as usize;
            for c in 0..4 {
                let w = skin.skin_bytes[vi * 8 + 4 + c];
                if w > best.0 {
                    best = (w, global_joints[vi][c] as u32);
                }
            }
        }
        best.1
    };
    let mut part_of = vec![0usize; glb.tris.len()];
    for (pi, part) in glb.parts.iter().enumerate() {
        for t in part.tri_start..(part.tri_start + part.tri_count).min(part_of.len()) {
            part_of[t] = pi;
        }
    }
    let mut order: Vec<usize> = (0..glb.tris.len()).collect();
    order.sort_by_key(|&i| (part_of[i], dom_of(&glb.tris[i])));
    let tris: Vec<[u32; 3]> = order.iter().map(|&i| glb.tris[i]).collect();

    let slots_of = |bones: &[u8]| -> usize {
        let mut v: Vec<u32> = bones.iter().map(|&b| b as u32).collect();
        v.sort_unstable();
        v.dedup();
        crate::char_skin::build::build_palette_ranges(&v).2
    };
    let tri_bones = |t: &[u32; 3]| -> Vec<u8> {
        let mut s: Vec<u8> = Vec::new();
        for &v in t {
            let vi = v as usize;
            for c in 0..4 {
                if skin.skin_bytes[vi * 8 + 4 + c] > 0 && !s.contains(&global_joints[vi][c]) {
                    s.push(global_joints[vi][c]);
                }
            }
        }
        s
    };
    let mut seg_part: Vec<usize> = Vec::new();
    let mut cur_bones: Vec<u8> = Vec::new();
    let tri_group: Vec<usize> = order
        .iter()
        .map(|&i| {
            let pi = part_of[i];
            let tb = tri_bones(&glb.tris[i]);
            let mut merged = cur_bones.clone();
            for b in &tb {
                if !merged.contains(b) {
                    merged.push(*b);
                }
            }
            // Close the group when the next triangle would cross a part boundary, or push the
            // palette past what the injector will accept.
            let open_new = seg_part.last() != Some(&pi)
                || slots_of(&merged) > crate::char_skin::build::MAX_GROUP_BONES;
            if open_new {
                seg_part.push(pi);
                cur_bones = tb;
            } else {
                cur_bones = merged;
            }
            seg_part.len() - 1
        })
        .collect();

    Partition {
        tris,
        tri_group,
        seg_part,
    }
}

/// Lower a rigged source mesh into `donor_block`, re-stamped as `new_name_hash`.
pub fn character_into_donor(
    donor_block: &[u8],
    glb: &CharGlbData,
    new_name_hash: u32,
    opts: &LowerOpts,
) -> Result<Lowered, String> {
    let skel = Skeleton::from_block(donor_block)
        .map_err(|e| format!("donor has no readable HIER skeleton: {e}"))?;
    let target = TargetSkeleton::from_skeleton(&skel);

    let mut skin = build_character(&glb.build_input(&target, None, opts.overrides.clone(), false))?;

    let transfer = if opts.native_rig {
        "native rig — kept the author's own weights (no donor transfer)".to_string()
    } else {
        match donor_transfer::apply_donor_transfer(
            &mut skin,
            &glb.tris,
            donor_block,
            &donor_transfer::DonorTransferOpts::default(),
        ) {
            Ok(msg) => msg,
            // Not fatal: the conform weights are usable, just fuzzier on the limbs. Say so rather
            // than failing the build or pretending it ran.
            Err(e) => format!("donor transfer SKIPPED ({e}); using conform weights"),
        }
    };

    // Host selection. `drawing_group_caps` reports `(ordinal, vertex_cap, triangle_cap)` — vertex
    // and index budgets only. It does NOT report bones, and this comment used to claim it did;
    // the bone limit is enforced inside the injector, per host, after the split is chosen.
    //
    // One group is frequently not enough: retail's own `pmc_hum_mattias` is 22 skinned draw groups,
    // and a 6k-triangle import fits none of them alone, which is why the single-group injector
    // cannot ship a real character by itself.
    let mut caps = drawing_group_caps(donor_block);
    caps.sort_by_key(|&(_, vcap, _)| std::cmp::Reverse(vcap));

    // Global HIER index per vertex influence — what the partitioner reasons about, and what the
    // multi-group injector expects in `mesh.joints`.
    let palette = crate::char_skin::expand_ranges(&skin.ranges);
    let mut global_joints: Vec<[u8; 4]> = Vec::with_capacity(skin.stats.verts);
    for vi in 0..skin.stats.verts {
        let mut g = [0u8; 4];
        for k in 0..4 {
            let slot = skin.skin_bytes[vi * 8 + k] as usize;
            // Both failures here used to be silent. A slot outside the palette became bone 0
            // (`GlobalSRT`, the root) — the "stranded extremity" the palette warning describes,
            // manufactured locally; and `as u8` wrapped for a donor with more than 255 HIER bones.
            let Some(&global) = palette.get(slot) else {
                return Err(format!(
                    "vertex {vi} influence {k} names palette slot {slot}, but the palette has {} — \
                     the skin and its range table disagree",
                    palette.len()
                ));
            };
            if global > 255 {
                return Err(format!(
                    "donor HIER bone {global} is past the 255 that BLENDINDICES can address; this \
                     donor needs a palette-relative multi-group split, not a global one"
                ));
            }
            g[k] = global as u8;
        }
        global_joints.push(g);
    }

    // THE SPLIT DECIDES THE HOST COUNT, not a triangle budget.
    //
    // This used to hand out "enough groups, biggest first, for the triangle budget to cover the
    // mesh", and passed `tri_group: None` so the injector split them evenly by triangle order. That
    // straddles source parts by construction, so every host group carried geometry from several
    // parts and therefore could not name a material that was right for all of it — the reason an
    // import could not wear its own textures. It also forced each group's palette to span nearly
    // the whole skeleton (measured 46-47 bones per group however many groups were used).
    //
    // The faithful partition discovers how many groups are needed instead: one per source part,
    // split further only where a palette would overflow.
    let part = faithful_partition(glb, &skin, &global_joints);
    let nseg = part.seg_part.len();
    if nseg > caps.len() {
        return Err(format!(
            "the split needs {nseg} host groups (one per source part, plus a cut wherever a bone \
             palette would overflow) but the donor has only {} drawing groups. Merge source parts, \
             or pick a donor with more groups.",
            caps.len()
        ));
    }
    // Biggest first — `grow` lifts the per-group vertex/index ceiling, so capacity ordering is a
    // preference rather than a constraint, but a bigger donor group still means less growth.
    let hosts: Vec<usize> = caps.iter().take(nseg).map(|&(ord, _, _)| ord).collect();

    // `grow` removes the DONOR's budget as a limit but not the format's: a group's index buffer is
    // u16, so no host can hold more than ~21.8k triangles however large the donor group was. Check
    // it here, where the source part is still nameable, rather than letting the injector report
    // "group 3 budget violated: ic 85381>65534" — true, and no use to whoever has to fix it.
    const MAX_TRIS_PER_GROUP: usize = 65534 / 3;
    let mut per_slot = vec![0usize; nseg];
    for &g in &part.tri_group {
        per_slot[g] += 1;
    }
    for (slot, &n) in per_slot.iter().enumerate() {
        if n > MAX_TRIS_PER_GROUP {
            let pi = part.seg_part[slot];
            let pname = glb
                .parts
                .get(pi)
                .map(|p| p.name.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("<unnamed>");
            return Err(format!(
                "source part {pi} ({pname}) puts {n} triangles in one draw group, past the \
                 {MAX_TRIS_PER_GROUP} a u16 index buffer can address. Decimate that part, or split \
                 it into several in the source file — a part cannot span draw groups, because a \
                 group carries exactly one material."
            ));
        }
    }

    let mesh = ExternalMesh {
        positions: skin.pos.clone(),
        // CONFORMED normals. The source glTF's field describes the PRE-conform surface and is
        // roughly orthogonal to the conformed one, which no geometry check would catch.
        normals: if skin.nrm.is_empty() {
            glb.normals.clone()
        } else {
            skin.nrm.clone()
        },
        uvs: glb.uvs.clone(),
        tris: glb.tris.clone(),
        joints: (0..skin.stats.verts)
            .map(|i| {
                [
                    skin.skin_bytes[i * 8],
                    skin.skin_bytes[i * 8 + 1],
                    skin.skin_bytes[i * 8 + 2],
                    skin.skin_bytes[i * 8 + 3],
                ]
            })
            .collect(),
        weights: (0..skin.stats.verts)
            .map(|i| {
                [
                    skin.skin_bytes[i * 8 + 4],
                    skin.skin_bytes[i * 8 + 5],
                    skin.skin_bytes[i * 8 + 6],
                    skin.skin_bytes[i * 8 + 7],
                ]
            })
            .collect(),
    };

    let (block, stats, audits) = if hosts.len() == 1 {
        // The single-host path hands the WHOLE-MODEL palette straight to `patch_skin_info56`, which
        // checks the run count and nothing else. The multi path gates per group; this one gated
        // nowhere, so a palette that had grown past what the format expresses shipped silently.
        // `donor_transfer` in particular REPLACES the palette after `build_character` has had its
        // say, so the only honest place to check is here, against the final numbers.
        let slots = skin.palette_slots;
        let bones = skin.stats.bones;
        if slots > crate::char_skin::build::MAX_PALETTE_SLOTS {
            return Err(format!(
                "the palette is {slots} slots, past the {} the engine's reader accepts — the group \
                 will not decode. Split it across more donor groups.",
                crate::char_skin::build::MAX_PALETTE_SLOTS
            ));
        }
        if bones > crate::char_skin::build::MAX_GROUP_BONES {
            return Err(format!(
                "the mesh weights {bones} distinct bones in one group, above the {} that is the \
                 measured ceiling across every skinned group in the shipped game. Split it across \
                 more donor groups.",
                crate::char_skin::build::MAX_GROUP_BONES
            ));
        }
        let (b, s) = inject_character_into_donor_block(
            donor_block,
            &mesh,
            &skin.ranges,
            hosts[0],
            &opts.repoints,
            new_name_hash,
        )?;
        (b, s, Vec::new())
    } else {
        // MULTI-GROUP. Each host gets its OWN palette and `INFO(56)` table, computed inside the
        // injector from the bones that group actually uses — so `mesh.joints` must carry **global**
        // donor HIER indices here, NOT the whole-model palette slots the single-group path wants.
        // Expanding the slots back through the model palette is what recovers them; skipping it
        // produces a model whose every group indexes the wrong bones.
        // Each host gets its OWN palette and `INFO(56)` table, computed inside the injector from the
        // bones that group actually uses — so `mesh.joints` carries GLOBAL donor HIER indices here,
        // not the whole-model palette slots the single-group path wants. `global_joints` was built
        // above for the partitioner and is the same expansion.
        let mut gmesh = mesh;
        gmesh.joints = global_joints;
        // The REORDERED triangles, index-parallel with `tri_group`. The injector reads the two
        // positionally, so handing over one without the other assigns triangles to the wrong hosts.
        gmesh.tris = part.tris;
        let (b, audits, s) = crate::model_inject::inject_character_multi_into_donor_block(
            donor_block,
            &gmesh,
            &hosts,
            &opts.repoints,
            new_name_hash,
            true, // grow: an import is denser than the donor; the packager recomputes page_count
            // The faithful partition: one source part per host, cut again wherever a palette would
            // overflow. The even split this replaces put geometry from several parts in one group,
            // and a draw group carries exactly ONE material — so no group could name a material
            // that was right for all of its geometry.
            Some(&part.tri_group),
            // Materials stay the donor's own per host (the injector preserves each group's PRMT
            // field 0 now). Per-part materials arrive with `textures:`, which needs its own blocks.
            None,
        )?;
        (b, s, audits)
    };

    // Validate what we are about to ship. The Workshop ran this on its preview and the Quartermaster
    // ran nothing, so the path that produced a WAD was checked LESS than the path that produced a
    // picture. Returned rather than enforced: `Report` grades, and which grades are fatal is the
    // caller's policy, not the lowering's.
    let report = crate::char_skin::validate::validate(
        &skin,
        &glb.vjoints,
        &glb.vweights,
        &glb.indices,
    );

    let mut warnings = skin.warnings.clone();
    for lim in report.limits.iter().filter(|l| !l.ok) {
        warnings.push(format!("{}: {}", lim.title, lim.text));
    }

    Ok(Lowered {
        block,
        skin,
        stats,
        transfer,
        host_group: hosts[0],
        hosts,
        warnings,
        audits,
        report,
    })
}
