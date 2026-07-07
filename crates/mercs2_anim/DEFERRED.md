# mercs2_anim — deferred improvements

Backlog for the animation runtime. Every entry is tagged with whether it blocks a *faithful* reimpl
(`[faithful-blocker: yes]` = the retail game does this and we don't yet; `no` = polish/optimization
beyond what the exe demonstrably does or a cross-silo dependency).

---

## Ragdoll — physics rigid-body driven
`[faithful-blocker: yes]` (but gated on another silo)

Ragdoll consumes physics rigid bodies + constraints from **silo 7** (`mercs2_physics`,
`docs/reverse_engineer/physics_code_map.md`). The Havok anchor is `hkaRagdollInstance` / the
`hkpRigidBody` chain the death/impact path activates. This crate must NOT build it (no leaf→leaf
edge, and there are no rigid bodies to drive yet). When silo 7 lands `PhysicsQuery`-adjacent
rigid-body access, add a `Ragdoll` component + a system that: (1) on trigger, seeds body transforms
from the current `SkinPalette`; (2) each tick reads body transforms back into the bone locals
(blend `animated → ragdoll` over a short window); (3) exposes a get-up blend back to animation.

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
cues — a separate curve-eval path from skeletal clips. Out of scope for the Wave-1 body-animation
runtime; add a `facefx` module when the FaceFX curve format is decoded.

## Perf — precomputed state→clip acceleration structure
`[faithful-blocker: no]`

`ClipPicker` precomputes a flat `(ActionRow, clip)` table per character and `resolve_indexed` does a
linear scan picking the most-specific match. Fine for a handful of humans; if thousands of NPCs
animate, key the entries by `(stance, action)` into a hash bucket first.
</content>
