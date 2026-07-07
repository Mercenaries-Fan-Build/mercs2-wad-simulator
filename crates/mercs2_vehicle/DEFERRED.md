# mercs2_vehicle — deferred improvements

Improvements that are **not** faithful to the retail exe (the oracle) live here. Each is tagged
with whether it blocks a faithful reimplementation.

## Backlog

- **Pilot every vehicle / ship (universal drivability).** The retail game gates which templates are
  drivable (seat data + `_CarPhysicsV2`/`_TankPhysics`/… actor presence). Making every ship/vehicle
  pilotable is a *mod feature*, not the oracle behaviour.
  `[faithful-blocker: no]`

- **Analytic (non-round-robin) wheel raycasts every frame.** The exe schedules ONE wheel/frame in
  the round-robin path (`FUN_0044d9b0`) when slow. Raycasting all wheels every frame would be
  higher fidelity to a *modern* sim but diverges from the oracle's amortised scheduler.
  `[faithful-blocker: no]`

- **Rapier/Havok-backed rigid body instead of the trait raycast.** We integrate a minimal chassis
  body against `&dyn PhysicsQuery` (the silo-7 seam). Swapping in a full physics engine solver is a
  later integration choice, not required for faithfulness.
  `[faithful-blocker: no]`

## Faithful blockers (must be resolved by confirm-live, tracked in the code map §5)

These are *not* improvements — they are unread oracle values. Marked `// CONFIRM-LIVE:` in source.

- Authored tuning defaults (MaxSpeed, suspension/friction constants, DonutBoost/DonutSidePower,
  gear/engine table) — field NAMES stripped on PC; recover by breaking `0x00449460` and diffing the
  0x18c block. `[faithful-blocker: yes — placeholders in `tuning.rs`]`
- The command-ID hash function (`0x3483DBF1` etc. are used verbatim; the *hash* that produced them
  is unknown). Does not block: the constants are what the ring compares. `[faithful-blocker: no]`
- Camera preset float layout + look-axis apply / pitch clamp (`FUN_0060f6d0` mode gate, string
  stripped). `[faithful-blocker: yes — placeholder pose math]`
