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

    let single = caps
        .iter()
        .filter(|&&(_, _, tricap)| skin.stats.tris as u32 <= tricap)
        .max_by_key(|&&(_, vcap, _)| vcap)
        .map(|&(ord, _, _)| ord);

    // Enough groups, biggest first, for the triangle budget to cover the mesh.
    let hosts: Vec<usize> = match single {
        Some(ord) => vec![ord],
        None => {
            let mut acc = 0u32;
            let mut picked = Vec::new();
            for &(ord, _, tricap) in &caps {
                if acc >= skin.stats.tris as u32 {
                    break;
                }
                acc = acc.saturating_add(tricap);
                picked.push(ord);
            }
            if acc < skin.stats.tris as u32 {
                return Err(format!(
                    "the donor's {} drawing groups hold {acc} triangles between them but the mesh \
                     has {} — decimate it or pick a larger donor",
                    caps.len(),
                    skin.stats.tris
                ));
            }
            picked
        }
    };

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
        let palette = crate::char_skin::expand_ranges(&skin.ranges);
        let mut gmesh = mesh;
        for (vi, j) in gmesh.joints.iter_mut().enumerate() {
            for k in 0..4 {
                let slot = skin.skin_bytes[vi * 8 + k] as usize;
                // Both failures here used to be silent. A slot outside the palette became bone 0
                // (`GlobalSRT`, the root) — which IS the "extremity stranded in space" the palette
                // warning describes, manufactured locally; and `as u8` wrapped for a donor with more
                // than 255 HIER bones, quietly aliasing every high bone onto a low one.
                let Some(&global) = palette.get(slot) else {
                    return Err(format!(
                        "vertex {vi} influence {k} names palette slot {slot}, but the palette has \
                         {} — the skin and its range table disagree",
                        palette.len()
                    ));
                };
                if global > 255 {
                    return Err(format!(
                        "donor HIER bone {global} is past the 255 that BLENDINDICES can address; \
                         this donor needs a palette-relative multi-group split, not a global one"
                    ));
                }
                j[k] = global as u8;
            }
        }
        let (b, audits, s) = crate::model_inject::inject_character_multi_into_donor_block(
            donor_block,
            &gmesh,
            &hosts,
            &opts.repoints,
            new_name_hash,
            true, // grow: an import is denser than the donor; the packager recomputes page_count
            // No explicit triangle→group map: this splits evenly by triangle order, which is the
            // proven behaviour. `CharGlbData::parts` carries the source's own sub-object partition
            // and is the faithful split (one material per group) — promoting it here is the known
            // next step, not something to improvise while the even split is what has shipped.
            None,
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
