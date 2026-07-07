//! `mercs2_anim` — Animation runtime (Wave-1 silo 8, scoreboard row 20).
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
//! **Ragdoll is DEFERRED** — it needs physics rigid bodies from silo 7. See `DEFERRED.md`.

pub mod controller;
pub mod ik;
pub mod pose;
pub mod select;

pub use controller::{
    animation_system, sample_controller_palette, AnimAssets, AnimController, HumanAnimationSet,
    SampledPose, ANIM_BLEND_SEC,
};
pub use ik::{solve_two_bone, FootPlacementIk, IkResult, LegChain};
pub use pose::BoneRig;
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

    /// Live end-to-end gate against retail `vz.wad`: parse the resident animation tables, resolve the
    /// three mercs' idles through the data-driven picker, and confirm the live-captured Chris idle
    /// clip (`0xED37BC56`) is reachable for Chris. SKIPS (stays green) when the WAD is absent, so CI
    /// without the retail data passes. Run with `VZ_WAD=/path/to/vz.wad cargo test -p mercs2_anim`.
    #[test]
    fn live_clip_picker_if_wad_present() {
        use mercs2_formats::anim_select::block_has_lookup;
        use mercs2_formats::ffcs::load_ffcs_archive;
        use mercs2_formats::sges::decompress_block;

        let path = std::env::var("VZ_WAD").unwrap_or_else(|_| {
            "C:/Program Files (x86)/EA Games/Mercenaries 2 World in Flames/data/vz.wad".into()
        });
        let Ok(mut f) = std::fs::File::open(&path) else {
            eprintln!("skip: vz.wad not present at {path}");
            return;
        };
        let size = f.metadata().unwrap().len();
        let arch = load_ffcs_archive(&mut f, size).expect("ffcs archive");

        // The doc places the tables in resident block 3185; scan nearby as a fallback so a WAD
        // variant still resolves.
        let mut resident: Option<Vec<u8>> = None;
        for blk in std::iter::once(3185u16).chain(3180u16..3200u16) {
            if let Ok(dec) = decompress_block(&mut f, &arch.indx, blk) {
                if block_has_lookup(&dec) {
                    resident = Some(dec);
                    break;
                }
            }
        }
        let Some(dec) = resident else {
            eprintln!("skip: no AnimationLookup block found (WAD variant?)");
            return;
        };

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
