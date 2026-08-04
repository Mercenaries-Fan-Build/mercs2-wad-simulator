# mercs2_physics — deferred work

Improvements deliberately left for the physics system or later passes. None of these is a
faithfulness blocker for the `PhysicsQuery` bridge (`StaticSoupPhysics`); each is tagged.

- **DONE (controller pass).** `move_character` is now a **swept linear cast** (conservative
  advancement, `StaticSoupPhysics::linear_cast` / `move_swept`) — tunnel-free at any speed — and a
  faithful **3-locomotion-state machine** (`CharacterController`: OnGround/InAir/Jumping per the
  recovered `hkpCharacterContext FUN_0094d2e0`) with gravity, jump, slope limit (`max_slope_cos`
  ≈ cinfo `maxSlopeCosine +0xa4`) and step-up (`step_height`) now live in this crate. A minimal
  `RigidBody` + `StaticSoupPhysics::step_rigid_body` (semi-implicit Euler + impulse resolve) covers
  props/debris. Remaining gaps below.

- **Climbing / Ladder-Flying states (5-state → full).** The retail machine registers two extra game
  states (ids 5 & 6, `PTR_FUN_00ba8f3c`/`PTR_FUN_00ba8f5c`, built inline in `HumanPhysics::Activate
  FUN_004255c0`). Not implemented — they are driven by ladder-volume *game* data, not physics geometry.
  Add when the ladder/climb game data lands. `[faithful-blocker: no]`

- **`// CONFIRM-LIVE:` integrator + tunables.** The per-frame integrator (`hkpWorld::step`) is
  VMX128/SSE and does not decode in either build (`physics_code_map.md` §10); the semi-implicit Euler
  (character *and* rigid body) is a faithful modern equivalent, not the exe's exact solver. Gravity
  (`DEFAULT_GRAVITY`), slope cosine (`DEFAULT_MAX_SLOPE_COS`), jump/move speeds, air control, and
  rigid-body restitution/friction/mass are faithful defaults tagged `// CONFIRM-LIVE:` in source —
  pin by reading `hkpWorldCinfo` / proxy-cinfo / `hkpRigidBody` material fields live (`physics_code_map.md`
  §9). `[faithful-blocker: no]`

- **Full rigid-body dynamics.** `RigidBody` is a sphere with no rotation/inertia tensor, no body↔body
  contacts (only body↔static soup), and a single deepest-contact resolve. The retail `hkpRigidBody`
  (`FUN_008d4be0`) has full motions + contact manifolds. Grow with the real broadphase/narrowphase.
  `[faithful-blocker: no]`

- **Broadphase (MOPP BV-tree).** Queries are a linear scan over the triangle soup with a cheap sphere
  cull. Retail uses `hkpMoppBvTreeShape`. Swap in a BV-tree/BVH when triangle counts grow.
  `[faithful-blocker: no]`

- **Dynamic bodies + entity attribution.** Only static world geometry is modelled, so `RayHit::entity`
  / `ClosestPoint::entity` are always `None`. Dynamic `hkpRigidBody` props/debris and per-shape entity
  ownership arrive with the physics system. `[faithful-blocker: no]`

- **Closest-point sign for arbitrary soup.** `closest_point`'s inside/outside sign is taken from the
  nearest triangle's wound normal, which is only meaningful for consistently-wound (outward-facing)
  closed shells. An open soup has no well-defined inside; the real `getClosestPoints` uses the shape's
  own inside test. `[faithful-blocker: no]`

- **Heightfield fidelity.** The `Heightmap` is a bilinear regular grid standing in for
  `hkpSampledHeightFieldShape`; it does not model the retail tri-sampled height-field BV-tree or
  per-tile material. `[faithful-blocker: no]`
