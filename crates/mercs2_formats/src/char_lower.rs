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

    // Host selection. Budgets are per group and count BONES as well as triangles, so "largest"
    // alone is not enough — and one group is frequently not enough either. Retail's own
    // `pmc_hum_mattias` is 22 skinned draw groups; a 6k-triangle import does not fit any single one
    // of them, which is why the single-group injector alone cannot ship a real character.
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

    let (block, stats) = if hosts.len() == 1 {
        inject_character_into_donor_block(
            donor_block,
            &mesh,
            &skin.ranges,
            hosts[0],
            &opts.repoints,
            new_name_hash,
        )?
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
                j[k] = palette.get(slot).copied().unwrap_or(0) as u8;
            }
        }
        let (b, _audits, s) = crate::model_inject::inject_character_multi_into_donor_block(
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
        (b, s)
    };

    Ok(Lowered {
        block,
        skin,
        stats,
        transfer,
        host_group: hosts[0],
        hosts,
    })
}
