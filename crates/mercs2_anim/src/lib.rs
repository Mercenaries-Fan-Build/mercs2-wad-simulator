//! `mercs2_anim` — Animation runtime.
//!
//! The faithful, data-driven human animation runtime, built on the already-solved wavelet/Havok
//! clip DECODE (`mercs2_formats::anim`, 168/168 tests) and the proven pose/skinning path. The exe is
//! the oracle. Three pieces:
//!
//! 1. **Selection** ([`select`]): the engine's real clip picker. `mercs2_formats::anim_select`
//!    parses `ActionTable`/`AnimationLookup`/`ASTO` out of the resident WAD block (RE'd + validated
//!    vs a live x32dbg capture — Chris idle `0xED37BC56`; `docs/modernization/human_animation_selection.md`);
//!    [`ClipPicker`] composes its two join halves into the forward `(character, StateKey) → clip`
//!    resolver. No hardcoded `CLIP_IDLE/WALK/RUN`.
//! 2. **Runtime** ([`controller`]): the [`HumanAnimationSet`] + [`AnimController`] ECS components and
//!    [`animation_system`] — per-entity clip state, fixed-tick time advance, crossfade blend
//!    (`hkaSkeletonUtils::blendPoses`), foot-lock/speed-scale — writing the `SkinPalette`.
//! 3. **IK** ([`ik`]): the [`FootPlacementIk`] two-bone foot-placement solver (`hkaFootPlacementIkSolver`
//!    analog), ground query supplied via `mercs2_core::PhysicsQuery`.
//!
//! The [`pose`] module is the `hkQsTransform` sample/compose/blend math, ported from
//! `mercs2_engine::pose` so this crate never depends on the renderer.
//!
//! # Module map
//!
//! | Module | Owns |
//! |---|---|
//! | [`select`] | [`ClipPicker`], [`StateKey`], [`ResolvedClip`] — the forward `(character, state) → clip` resolver. |
//! | [`controller`] | [`HumanAnimationSet`] + [`AnimController`] components, the [`AnimAssets`]/[`SampledPose`] asset seam, and [`animation_system`]. |
//! | [`pose`] | [`BoneRig`] and the `hkQsTransform` math (`bind_qs`/`model_poses`/`skin_palette`/`havok_palette*`/`qs_blend`/`clip_root_speed`). |
//! | [`ik`] | [`solve_two_bone`], [`FootPlacementIk`], [`LegChain`], [`IkResult`]. |
//!
//! Assets reach the runtime through the [`AnimAssets`] trait (rig / clip duration / sampled pose) and
//! ground through `mercs2_core::PhysicsQuery`, so the only dependencies are `mercs2_core` +
//! `mercs2_formats` — no renderer, no loader, no leaf→leaf edge to the physics system.
//!
//! **Ragdoll: the sim lands in `mercs2_physics`; the skeleton seam is [`ragdoll`] here.** The
//! constrained multi-body ragdoll (recovered WAD capsule bodies + XPBD joints) is
//! `mercs2_physics::ragdoll`; this crate contributes the physics-free glue ([`body_seeds`] /
//! [`write_back_model_pose`]) that snaps bodies onto the posed skeleton and reads the sim back into
//! the skin — no leaf→leaf edge. **FaceFX is DEFERRED** (evaluator `FUN_00686ce0` needs its curve
//! format decoded). Other known faithfulness gaps: the per-transition crossfade
//! table (`AnimationTransition 0xAB8FE34B` — the controller uses a fixed [`ANIM_BLEND_SEC`] instead),
//! the walk↔run locomotion blend space, and foot-IK surface-normal orientation + pelvis drop. All are
//! tracked in `DEFERRED.md`.

pub mod controller;
pub mod ik;
pub mod pose;
pub mod ragdoll;
pub mod select;

pub use controller::{
    animation_system, sample_controller_palette, AnimAssets, AnimController, HumanAnimationSet,
    SampledPose, ANIM_BLEND_SEC,
};
pub use ik::{solve_two_bone, FootPlacementIk, IkResult, LegChain};
pub use pose::BoneRig;
pub use ragdoll::{body_seeds, write_back_model_pose};
pub use select::{ClipPicker, ResolvedClip, StateKey};

// Re-export the clip decode + selection primitives this crate is built on, so downstream (the
// engine) can reach them through the anim crate.
pub use mercs2_formats::anim::{AnimClip, QsTransform};
pub use mercs2_formats::anim_select::AnimSelector;

#[cfg(test)]
mod tests {
    use super::*;

    /// The three merc CharacterName keys are `pandemic_hash_m2(name)`.
    #[test]
    fn merc_character_names() {
        assert_eq!(ClipPicker::character_name("mattias"), 0x030E_6C38);
        assert_eq!(ClipPicker::character_name("chris"), 0xD64B_B122);
        assert_eq!(ClipPicker::character_name("jennifer"), 0xF314_4C8E);
    }

    #[cfg(feature = "retail")]
    mod retail {
        use super::*;

        /// Live end-to-end gate against retail `vz.wad`: parse the resident animation tables, resolve the
        /// three mercs' idles through the data-driven picker, and confirm the live-captured Chris idle
        /// clip (`0xED37BC56`) is reachable for Chris.
        ///
        /// Game-gated, built by the `retail` feature: reads the `vz.wad` named by the repo-root
        /// `.mercs2-local.toml` and fails if it is absent. Run with `cargo xtask retail-test`. This is a
        /// leaf crate and cannot reach `mercs2_engine::paths` (the carve rule), so discovery comes from
        /// `mercs2_formats` — the crate both already depend on.
        #[test]
        fn live_clip_picker_if_wad_present() {
            use mercs2_formats::anim_select::block_has_lookup;
            use mercs2_formats::aset_type_ids::type_id_for_type_hash;
            use mercs2_formats::ffcs::load_ffcs_archive;
            use mercs2_formats::hash::pandemic_hash_m2;
            use mercs2_formats::sges::decompress_block;

            let path = mercs2_formats::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
                .unwrap_or_else(|e| panic!("{e}"));
            let mut f = std::fs::File::open(&path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
            let size = f.metadata().unwrap().len();
            let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");

            // The AnimationLookup is an `animationtable` asset whose container name hash is
            // 0xE00B080C (the value `anim_select`'s `block_has_lookup` matches); its one ASET row names
            // its block. In retail vz.wad that is row 3055: type_id 11, block 3185, single-block.
            let lookup_hash: u32 = 0xE00B_080C;
            let animtable_type = type_id_for_type_hash(pandemic_hash_m2("animationtable"))
                .expect("animationtable has an ASET type_id");
            let rows: Vec<_> = arch.aset.iter().filter(|a| a.asset_hash == lookup_hash).collect();
            assert_eq!(
                rows.len(),
                1,
                "expected exactly one ASET row for AnimationLookup 0x{lookup_hash:08X} in {}, found {}",
                path.display(),
                rows.len()
            );
            let row = rows[0];
            assert_eq!(
                row.type_id, animtable_type,
                "AnimationLookup 0x{lookup_hash:08X} ASET row has type_id {}, not animationtable",
                row.type_id
            );
            assert!(row.is_single_block(), "AnimationLookup 0x{lookup_hash:08X} spans more than one block");
            let blk = row.block_index();
            let dec = decompress_block(&mut f, &arch.indx, blk)
                .unwrap_or_else(|e| panic!("decompress AnimationLookup block {blk}: {e}"));
            assert!(
                block_has_lookup(&dec),
                "block {blk}, named by the AnimationLookup ASET row, does not carry the AnimationLookup container"
            );

            let mattias = ClipPicker::character_name("mattias");
            let chris = ClipPicker::character_name("chris");
            let jennifer = ClipPicker::character_name("jennifer");
            let picker = ClipPicker::from_resident_block(&dec, &[mattias, chris, jennifer])
                .expect("resident block carries the AnimationLookup");

            // Per-merc idle is data-driven — each merc idles on its OWN clip (engine-path values,
            // human_animation_selection.md §10). The old hardcoded engine used Jennifer's for all.
            assert_eq!(picker.idle(mattias), Some(0x6EA8_8E00), "mattias idle");
            assert_eq!(picker.idle(chris), Some(0x835D_A06A), "chris idle");
            assert_eq!(picker.idle(jennifer), Some(0x24F8_C8E6), "jennifer idle");

            // The forward resolver maps the standing idle state to a clip for Chris.
            let r = picker
                .resolve_indexed(chris, StateKey::idle())
                .expect("Upright+Fidget resolves for chris");
            assert_ne!(r.clip, 0);

            // The live x32dbg-captured Chris idle clip is reachable through the data for Chris.
            let chris_clips = picker.selector().character_clips(chris);
            assert!(
                chris_clips.iter().any(|c| c.clip == 0xED37_BC56),
                "live-captured Chris idle 0xED37BC56 must be in Chris's resolved clip set"
            );
        }
    }
}
