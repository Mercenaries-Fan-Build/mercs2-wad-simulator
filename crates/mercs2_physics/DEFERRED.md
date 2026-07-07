# mercs2_physics — deferred work

Improvements deliberately left for the physics silo (row 22) or later Wave-1 passes. None of these is a
faithfulness blocker for the Wave-1 `PhysicsQuery` bridge (`StaticSoupPhysics`); each is tagged.

- **Swept character controller (linear cast).** `move_character` is depenetration-based: it applies the
  desired delta and pushes the capsule out of penetrated walls. This is faithful for per-frame moves
  smaller than the capsule radius, but a single larger delta can tunnel through thin geometry. The
  retail `hkpCharacterProxy` / `HumanLinearCastJob` does a swept linear cast. Replace when the Havok
  world lands. `[faithful-blocker: no]`

- **5-state character machine + slope limits.** The retail controller runs the OnGround/InAir/Jumping/…
  state machine (`HumanPhysics::Activate FUN_004255c0`) with slope and max-slope-angle handling. This
  bridge only does collide-and-slide + ground snap within `step`; gravity/jump/air state is owned by the
  caller (sim silo) for now. `[faithful-blocker: no]`

- **Broadphase (MOPP BV-tree).** Queries are a linear scan over the triangle soup with a cheap sphere
  cull. Retail uses `hkpMoppBvTreeShape`. Swap in a BV-tree/BVH when triangle counts grow.
  `[faithful-blocker: no]`

- **Dynamic bodies + entity attribution.** Only static world geometry is modelled, so `RayHit::entity`
  / `ClosestPoint::entity` are always `None`. Dynamic `hkpRigidBody` props/debris and per-shape entity
  ownership arrive with the physics silo. `[faithful-blocker: no]`

- **Closest-point sign for arbitrary soup.** `closest_point`'s inside/outside sign is taken from the
  nearest triangle's wound normal, which is only meaningful for consistently-wound (outward-facing)
  closed shells. An open soup has no well-defined inside; the real `getClosestPoints` uses the shape's
  own inside test. `[faithful-blocker: no]`

- **Heightfield fidelity.** The `Heightmap` is a bilinear regular grid standing in for
  `hkpSampledHeightFieldShape`; it does not model the retail tri-sampled height-field BV-tree or
  per-tile material. `[faithful-blocker: no]`
