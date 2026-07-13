# mercs2_anim

The human animation runtime for the Mercenaries 2 reimplementation — data-driven clip selection,
per-entity playback with crossfade, and two-bone foot-placement IK (Wave-1 silo 8, scoreboard row 20).

## What it is

A library crate. It sits on top of the already-solved wavelet/Havok clip **decode**
(`mercs2_formats::anim`) and turns it into a running animation system:

* **Selection** — `ClipPicker` composes the game's own `ActionTable` / `AnimationLookup` / `ASTO`
  tables into the forward resolver `(character, StateKey) → clip hash`. `StateKey` is the
  ActionTable's key columns (Stance / Action / AimState / Tandem / Seat / Target / ActionDirection),
  with a wildcard sentinel on either side; resolution picks the most-specific matching row. There are
  no hardcoded `CLIP_IDLE/WALK/RUN` constants — every clip comes out of the WAD data.
* **Runtime** — the `HumanAnimationSet` + `AnimController` ECS components and `animation_system`,
  a fixed-tick system that resolves the state to a clip, crossfades on clip changes, advances
  playback time (looping wrap / one-shot clamp, clamped by the lookup row's Min/MaxTimeScale),
  samples + blends the pose, and writes each entity's `mercs2_core::SkinPalette`.
* **Pose math** — `hkQsTransform` sample/compose/blend (`sampleAndCombine`,
  `transformLocalPoseToModelPose`, `hkaSkeletonUtils::blendPoses`), plus the in-place (foot-lock)
  variant that strips baked root locomotion so the entity `Transform` carries the ground motion.
* **IK** — `FootPlacementIk`, an analytic two-bone (law-of-cosines) leg solver that plants the ankle
  onto ground obtained through `mercs2_core::PhysicsQuery`.

Asset access is behind the `AnimAssets` trait (rig / clip duration / sampled pose), so the crate
drives entities without depending on the renderer or the loader. Its only dependencies are
`mercs2_core` and `mercs2_formats`.

**Ragdoll and FaceFX are NOT implemented** — see `DEFERRED.md`. (The `Cargo.toml` description still
lists them as crate content; they are backlog, not code.)

## Where it comes from

The exe is the oracle:

* The selection chain (`ActionTable 0x6802C321` → `AnimationLookup 0xE00B080C` → `ASTO`) was reverse
  engineered and validated against a live x32dbg capture — Chris's idle clip `0xED37BC56`. Spec:
  `docs/modernization/human_animation_selection.md`. The parse itself lives in
  `mercs2_formats::anim_select::AnimSelector`; this crate composes the two halves of the join into
  the forward direction the engine uses.
* The pose math is the Havok Animation 5.5 pipeline the retail engine runs, ported verbatim from the
  proven `mercs2_engine::pose` so this crate never depends on the renderer.
* The IK corpus anchor is `hkaFootPlacementIkSolver` ctor `FUN_009ef650` (`animation_code_map.md`).
  The per-frame VMX solve body is vtable-dispatched (confirm-live), so the numeric solve here is the
  standard analytic two-bone IK that solver reduces to.
* The three merc `CharacterName` keys are `pandemic_hash_m2(name)`: mattias `0x030E6C38`,
  chris `0xD64BB122`, jennifer `0xF3144C8E`.

## Usage

Build a picker from the resident WAD block that carries the AnimationLookup, then resolve a
gameplay state to a clip:

```rust
use mercs2_anim::{ClipPicker, StateKey};

// `resident` = the decompressed WAD block holding the animation tables
// (mercs2_formats::sges::decompress_block + ffcs::load_ffcs_archive).
let chris = ClipPicker::character_name("chris");
let picker = ClipPicker::from_resident_block(&resident, &[chris])
    .expect("block carries the AnimationLookup");

let resolved = picker.resolve_indexed(chris, StateKey::idle()).unwrap();
println!("clip {:#010x} looping={}", resolved.clip, resolved.looping);
```

Drive entities each fixed tick (the engine calls this from its `Schedule`):

```rust
use mercs2_anim::{animation_system, AnimAssets, AnimController, HumanAnimationSet};

// world: mercs2_core::World; assets: your impl of AnimAssets (rig / clip_duration / sample)
world.spawn((
    mercs2_core::ModelRef { model: model_hash },
    HumanAnimationSet::new(chris),      // state defaults to StateKey::idle()
    AnimController::default(),
));
animation_system(&mut world, Some(&picker), &assets, dt); // writes SkinPalette per entity
```

Tests:

```
VZ_WAD=/path/to/vz.wad cargo test -p mercs2_anim
```

The end-to-end test (`live_clip_picker_if_wad_present`) parses the retail `vz.wad`, resolves the
three mercs' idles, and asserts the live-captured Chris idle is reachable. It **skips** (stays green)
when the WAD is absent, so CI without retail data passes.

## Modules

| Module | Owns |
| --- | --- |
| `select` | `ClipPicker`, `StateKey`, `ResolvedClip` — the forward `(character, state) → clip` resolver over the parsed ActionTable/AnimationLookup/ASTO. |
| `controller` | `HumanAnimationSet` + `AnimController` components, `AnimAssets`/`SampledPose` asset seam, and the fixed-tick `animation_system`. |
| `pose` | `BoneRig` and the `hkQsTransform` math: `bind_qs`, `model_poses`, `skin_palette`, `havok_palette*`, `qs_blend`, `clip_root_speed`, `flatten`. |
| `ik` | `solve_two_bone`, `FootPlacementIk`, `LegChain`, `IkResult` — foot placement against a `PhysicsQuery` ground raycast. |

The clip-decode and selection primitives the crate is built on are re-exported for downstream users:
`AnimClip`, `QsTransform` (from `mercs2_formats::anim`) and `AnimSelector`
(from `mercs2_formats::anim_select`).

## Notes / gotchas

* `ClipPicker::resolve_indexed` takes `&self` but only sees characters that were pre-computed in the
  constructor; `resolve` takes `&mut self` and lazily builds a character's index. `animation_system`
  takes `&ClipPicker`, so pre-seed every character you intend to animate — entities whose character
  was not pre-computed simply keep their current clip.
* Crossfade duration is currently a fixed `ANIM_BLEND_SEC = 0.2 s`. The retail engine reads the
  `AnimationTransition` table (`0xAB8FE34B`) for per-transition durations/types; that is a listed
  faithfulness gap (`DEFERRED.md`).
* Selection returns one discrete clip per state — there is no walk↔run parametric blend space yet.
  `pose::clip_root_speed` (baked root locomotion → m/s) is the helper that driver will use.
* `foot_lock` (default `true`) strips the root bone's animated translation back to its bind local, so
  striding clips play in place and the entity `Transform` moves the character. Set it `false` for
  authored root motion.
* `FootPlacementIk` plants the ankle *position* only. Surface-normal foot orientation and the
  pelvis-drop pass are not implemented.
* Only `Stance=Upright (0x12C07B18)` and `Action=Fidget (0x0C0A7FA6)` are named constants; the rest of
  the state vocabulary is still raw hashes. Resolution matches by hash, so this is a readability gap,
  not a correctness one.
* All matrices are row-major / row-vector (`world = local · world_parent`), matching
  `mercs2_formats::skeleton`.
