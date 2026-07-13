# mercs2_physics

Silo 7 (scoreboard row 22) of the Mercenaries 2 reimplementation: the character controller, collision
queries and minimal rigid-body dynamics that stand in for the retail Havok world.

## What it is

This crate is the collision/physics backend of the reimplementation. It has two layers:

* **`StaticSoupPhysics`** — the concrete implementation of the `mercs2_core::physics_query::PhysicsQuery`
  seam, backed by a static world-space triangle soup (buildings, roads, terrain mesh) plus an optional
  regular-grid `Heightmap`. It answers the three seam queries with exact math:
  * `raycast` — Möller–Trumbore ray/triangle, nearest hit, normal oriented against the ray.
  * `closest_point` — point/triangle (Ericson §5.1.5), **signed** distance (negative when the query
    point is behind the nearest wound face).
  * `move_character` — swept capsule collide-and-slide (`linear_cast` / `move_swept`, conservative
    advancement so the capsule cannot tunnel a thin wall at any speed) followed by a ground snap
    within the step height.

  It also carries `ground_hit` (slope-limited walkable-ground probe) and `step_rigid_body`
  (semi-implicit Euler + impulse resolve for a spherical `RigidBody` — props/debris that fall, bounce
  and settle on the soup).
* **`CharacterController`** — the on-foot locomotion state machine (`OnGround` / `Jumping` / `InAir`)
  with gravity, jump impulse, air control, a slope limit (`max_slope_cos`) and a step-up limit
  (`step_height`). It talks to the world *only* through `&dyn PhysicsQuery`, so it runs against
  `StaticSoupPhysics` today and against the full Havok world later.

The `soup` module is a separate, lighter-weight API: free functions over a raw `&[[Vec3; 3]]` with no
owning world object, used by the engine's on-foot player controller (`mercs2_engine::player`) and the
camera boom (`mercs2_engine::camera`). Its broad phase culls by each triangle's **bounding box**, not by
distance to one vertex — that vertex-distance cull was the "fell through the floor after moving" bug
when a player stood in the middle of a large floor triangle.

The crate has no Lua surface of its own; it backs the `PhysicsQuery` seam that the vehicle / combat /
anim silos query. `mercs2_engine` re-exports it as `mercs2_engine::physics`.

## Where it comes from

Provenance as recorded in the source (`docs/reverse_engineer/physics_code_map.md`, silo plan
`docs/modernization/reimplementation_parallelization_plan.md` §3):

| Reimpl item | Retail oracle |
| --- | --- |
| `PhysicsQuery::raycast` | `hkpWorldRayCaster` / Pangea `CastRay` |
| `PhysicsQuery::closest_point` | `LthkpWorld::getClosestPoints` `FUN_008db880` + closest collector |
| `PhysicsQuery::move_character` | `hkpCharacterProxy` swept capsule (`FUN_0094f2c0`) / `HumanPhysics::Activate` `FUN_004255c0` |
| `CharacterController` state machine | `hkpCharacterContext` `FUN_0094d2e0` — OnGround (id 2, `FUN_0094ce90`), InAir (id 3, `FUN_0094d7b0`), Jumping (id 1, `FUN_00951ef0`) |
| `air_control` blend | `hkpCharacterStateInAir` air-control setter `FUN_0094d780` |
| `max_slope_cos` | proxy cinfo `maxSlopeCosine` at `+0xa4` (`FUN_0094dd30`) |
| `DEFAULT_GRAVITY` | `hkpWorldCinfo` gravity `hkVector4` (`FUN_008e2da0`) |
| `RigidBody` / `step_rigid_body` | `hkpRigidBody` (`FUN_008d4be0`) stepped by `hkpWorld::step` |
| triangle soup + `Heightmap` | `hkpMoppBvTreeShape` + `hkpSampledHeightFieldShape` world |
| `soup` camera raycast | the exe's `CameraCollisionCastRay` radius² probe |

The numeric constants marked `// CONFIRM-LIVE:` in source are **faithful defaults, not the exe's
values**: the per-frame integrator (`hkpWorld::step`) and the cinfo setup paths are VMX128/SSE and do
not statically decode in either build (`physics_code_map.md` §1/§10). They are to be pinned by reading
`hkpWorldCinfo` / the proxy cinfo / `hkpRigidBody` material fields live.

## Usage

Library only — no binaries. Build a `StaticSoupPhysics` over world geometry, hand it to a
`CharacterController`, and step:

```rust
use mercs2_core::glam::Vec3;
use mercs2_core::physics_query::PhysicsQuery;
use mercs2_physics::{CharacterController, CharacterInput, Heightmap, RigidBody, StaticSoupPhysics};

// A floor quad, wound +Y so it is walkable.
let tris = vec![
    [Vec3::new(-10.0, 0.0, -10.0), Vec3::new(-10.0, 0.0, 10.0), Vec3::new(10.0, 0.0, -10.0)],
    [Vec3::new(10.0, 0.0, -10.0), Vec3::new(-10.0, 0.0, 10.0), Vec3::new(10.0, 0.0, 10.0)],
];
let phys = StaticSoupPhysics::new(tris);
// Or with terrain: StaticSoupPhysics::with_heightmap(tris, Heightmap::new(ox, oz, cell, w, d, heights))

// The PhysicsQuery seam.
let hit = phys.raycast(Vec3::new(0.0, 5.0, 0.0), -Vec3::Y, 100.0).unwrap();
assert!((hit.distance - 5.0).abs() < 1e-3);

// The on-foot controller: walk +X for a second at 60 Hz.
let mut cc = CharacterController::new(Vec3::ZERO, 0.3, 1.8);
for _ in 0..60 {
    cc.step(&phys, CharacterInput { move_dir: Vec3::X, jump: false }, 1.0 / 60.0);
}

// Props / debris.
let mut body = RigidBody::new(Vec3::new(0.0, 5.0, 0.0), 0.5);
phys.step_rigid_body(&mut body, 1.0 / 60.0, Vec3::Y * mercs2_physics::DEFAULT_GRAVITY);
```

The direct-soup API (what the engine's player + camera use) takes the triangle list itself:

```rust
use mercs2_core::glam::Vec3;
use mercs2_physics::soup;

let tris: Vec<[Vec3; 3]> = vec![/* world-space collision triangles */];
let feet = soup::move_character(&tris, Vec3::ZERO, Vec3::X * 0.1, 0.4, 1.8, 0.5, true);
let ground = soup::ground_below(&tris, feet, 0.4, 4.0);          // landing probe
let boom = soup::raycast(&tris, feet, Vec3::Z, 5.0);             // camera boom
```

## Modules

* **(crate root)** — `StaticSoupPhysics` (the `PhysicsQuery` impl: raycast / closest_point /
  move_character / linear_cast / move_swept / ground_hit / step_rigid_body), `Heightmap`, `GroundHit`,
  `CharacterController` + `CharacterState` + `CharacterInput`, `RigidBody`, `DEFAULT_GRAVITY`,
  `DEFAULT_MAX_SLOPE_COS`.
* **`soup`** — direct-triangle-soup collision over `&[[Vec3; 3]]`, no world object: `ray_tri`,
  `raycast`, `move_character`, `ground_below`. Bbox-culled broad phase (large-triangle-safe). Folded
  here from `mercs2_game::collision` — the game owns content, the engine/physics owns the mechanism.

## Notes / gotchas

* **No broadphase.** Every query is a linear scan over the triangle list with a cheap proximity cull.
  The retail engine uses `hkpMoppBvTreeShape`. Fine at Wave-1 triangle counts; swap in a BVH when they
  grow.
* **The two culls have different tuning.** The crate-root `StaticSoupPhysics` culls by distance to a
  triangle's *first vertex* — it is tuned for the game's **small** world triangles (the tests build
  tiled floors for exactly this reason). The `soup` module culls by triangle **bbox** and is therefore
  large-triangle-safe. If you feed the root impl huge quads, the ground probe can miss them.
* **Static geometry only.** `RayHit::entity` / `ClosestPoint::entity` are always `None` — there is no
  owning entity for MOPP/heightfield geometry, and dynamic `hkpRigidBody` props are not in the query
  world yet.
* **`closest_point`'s sign is only meaningful for consistently-wound closed shells** (it is taken from
  the nearest triangle's wound normal). An open soup has no well-defined inside.
* **Walls vs floors.** A triangle is a *wall* when `|n.y| < 0.5` (normal more horizontal than
  vertical). Walls block and slide; floors are handled exclusively by the downward ground probe, which
  owns Y.
* **Climbing / Ladder-Flying are not implemented.** The retail machine registers two extra game states
  (ids 5 & 6, built inline in `HumanPhysics::Activate FUN_004255c0`); they are driven by ladder-volume
  game data, not physics geometry. See `DEFERRED.md` for the full deferred list.
* `RigidBody` is a sphere with no rotation/inertia tensor and no body↔body contacts (only
  body↔static soup).
