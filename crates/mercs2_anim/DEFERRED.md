# mercs2_anim — deferred improvements

Backlog for the animation runtime. Every entry is tagged with whether it blocks a *faithful* reimpl
(`[faithful-blocker: yes]` = the retail game does this and we don't yet; `no` = polish/optimization
beyond what the exe demonstrably does or a cross-system dependency).

---

## Ragdoll — LANDED (sim in `mercs2_physics`; skeleton seam here)
`[faithful-blocker: no]` (was yes)

The constrained multi-body ragdoll now lives in **`mercs2_physics::ragdoll`** (recovered WAD capsule
bodies + XPBD joints/limits). This crate contributes the physics-free skeleton seam ([`ragdoll`]):
- `body_seeds(rig, model_pose, bone_hashes)` — the `SetBodyToRagdoll` snap: reads each ragdoll bone's
  current animated MODEL-space transform so a body spawns exactly on the posed skeleton.
- `write_back_model_pose(rig, model_pose, driven)` — reads the simulated transforms back into the
  driven bones' model matrices each frame.
No leaf→leaf edge: the seam speaks only bone name-hashes + `(pos, rot)`; the combat/death integrator
glues it to `mercs2_physics::Ragdoll::{spawn_with, step, bone_transforms}`.

**Residual (deferred):** the `animated → ragdoll` blend-in window and the get-up blend back to
animation (retail `hkbGetUpModifier`/catch-fall; the field defaults don't decode statically). The seam
above is what a blend layers on top of.

## Transition graph (`0xAB8FE34B`) — per-handle crossfade rules
`[faithful-blocker: yes]`

The controller currently crossfades on a fixed `ANIM_BLEND_SEC` (0.2 s). The retail engine reads the
`AnimationTransition` table (`FromHandle, ToSequence, TransitionType, TransitionDuration,
TransitionAnimation`, 497 rows) to pick a per-transition duration/type (and sometimes an intermediate
transition clip). Parse it out of the same resident block (it is a `0x207359C7` table like the others)
and feed `TransitionDuration` into `AnimController::set_clip` instead of the constant; honor
`TransitionType` (crossfade vs snap vs via-clip). Needs a public row-enumeration accessor on
`AnimSelector` (currently `mercs2_formats`-side) or a local parse.

## Full ActionTable state vocabulary
`[faithful-blocker: no]` (mechanical naming; resolution already works by hash)

`select` names `Stance=Upright (0x12C07B18)` and `Action=Fidget (0x0C0A7FA6)`. The remaining
Stance/Action/AimState/ActionDirection value hashes (Idle/Move/Run/Crouch, Front/Left/Right/Back,
aim states) are still raw hashes — resolution is correct regardless (it matches by hash), but named
constants would make gameplay code that builds a `StateKey` readable. Extend the constants in
`select.rs` as hashes are named (rainbow table / devkit strings).

## Locomotion blend space (walk↔run parametric blend)
`[faithful-blocker: yes]`

Right now selection picks a single discrete clip per state. The base game blends walk/run (and
strafe directions) by a speed parameter using the baked root speed (`pose::clip_root_speed`). Add a
locomotion blend node that samples two clips and blends by normalized speed, feeding
`havok_palette_blend_in_place`. The root-speed helper is already here; this is the parametric driver
on top.

## Foot IK — surface-normal foot orientation + pelvis drop
`[faithful-blocker: yes]`

`ik::FootPlacementIk` plants the ankle *position* onto queried ground. The retail foot-placement
solver also (a) rotates the foot to the surface normal (ankle→toe), and (b) lowers the pelvis when
neither foot can reach, so the higher foot stays planted on slopes/steps. Add an ankle-orientation
pass (align toe bone to `RayHit::normal`) and a pelvis-drop pre-pass across both legs.

## FaceFX facial animation
`[faithful-blocker: yes]`

FaceFX evaluator `FUN_00686ce0` (`animation_code_map.md`) drives face/lip-sync bones from audio
cues — a separate curve-eval path from skeletal clips. Out of scope for the body-animation
runtime; add a `facefx` module when the FaceFX curve format is decoded.

## Perf — precomputed state→clip acceleration structure
`[faithful-blocker: no]`

`ClipPicker` precomputes a flat `(ActionRow, clip)` table per character and `resolve_indexed` does a
linear scan picking the most-specific match. Fine for a handful of humans; if thousands of NPCs
animate, key the entries by `(stance, action)` into a hash bucket first.
</content>
