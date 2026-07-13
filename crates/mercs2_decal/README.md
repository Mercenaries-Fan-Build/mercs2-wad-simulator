# mercs2_decal

Decals (scoreboard row 6) for the Mercenaries 2 reimplementation: the `decaltable` loader plus the
projected-decal instance pool and its lifetime bookkeeping. Render integration is a seam against
`mercs2_engine`.

## What it is

A pure-bookkeeping library — no GPU code. It owns the three halves of the decal system that the
engine (not the shader) is responsible for:

* **The `decaltable`** — the resident decal-material definition table. `DecalTable` is a keyed row
  store of `DecalDef`s (row key, base texture, `decalNormal` map, `decalParam` map, projection size,
  lifetime, `EnableSuperDecal` flag). Rows are addressed by `pandemic_hash_m2` of the material name,
  which is how the engine addresses them; `DecalType` (`BulletHole`, `Blood`, `Scorch`, `TireTrack`,
  `DamageShadow`) is the legible handle onto the recovered category set.
* **The instance pool** — `DecalPool` is the `CreateDecals` → `DecalsUpdate`/`DecalUnlock` runtime:
  a bounded slot array, spawn-prefers-free-slot, evict-oldest when full, per-frame aging, free on
  lifetime expiry, and a per-instance fade `alpha()`. Each `DecalInstance` carries the projection
  *inputs* (position, surface normal, tangent, size, super flag) that a draw pass consumes.
* **The ECS suppression tags** — `DisableDecals` (`0xff4533e5`) and `Disable3DDecals` (`0x69a0e0e4`),
  4-byte tags whose presence on an entity drops decal spawns against it.

`DecalWorld` is the pair the host holds and ticks (`table` + `pool`), world-global state in the same
shape as the AI crate's `AiWorld`.

## Where it comes from

Derived from `docs/reverse_engineer/decal_code_map.md` (the sky/decal/water PC code maps), as the
crate's own docs state:

* The `decaltable` is an ASET resident singleton, type-class hash **`0x3B0AABF8`**. The big ASET
  registrar `FUN_004bef00` registers it; its `GetTypeHash` vfn `FUN_004cb1b0` returns `0x3b0aabf8`,
  and its instance resolver `FUN_004cb1f0` allocates a **`0x400`**-byte resident block via
  `FUN_008242b0(0x400)`, stamping the resident flag `|0x4000` at `obj+0x16`. That block is
  `PgDecalTable` (`.data @0x9288b8`). These are encoded as `DECALTABLE_TYPE_HASH`,
  `DECALTABLE_RESIDENT_ALLOC`, `DECALTABLE_RESIDENT_FLAG`, `DECALTABLE_DATA_ADDR`.
* The two toggle components register via the Keystone-A two-function pattern:
  `DisableDecals` = registrar `FUN_00643bd0` (`PTR_00bc18d8`, stride 4) / deserializer `FUN_0063d060`;
  `Disable3DDecals` = registrar `FUN_00643c80` (`PTR_00bc1928`, stride 4) / deserializer `FUN_0063d0d0`.
* The material bind-slot names `decalNormal` (`.data @0xbac5d4`) and `decalParam` (`.data @0xbac5f0`)
  are recorded as constants for corpus x-ref; the maps themselves are data-only bind slots.

Per the code map's §0 boundary the *setup* half is statically recovered, while the
create/project/render/GC half has its profiler-marker strings stripped from the retail PC build and
is data/vtable-driven — so this crate implements the mechanism and leaves the numbers as loadable
data (see Notes).

## Usage

Library only — no binaries.

```rust
use mercs2_decal::{DecalWorld, DecalType};
use mercs2_core::glam::Vec3;

// Stock table (recovered categories, placeholder params) + default pool cap.
let mut decals = DecalWorld::new();

// CreateDecals: book a bullet hole at a surface hit point.
let hit    = Vec3::new(12.0, 1.5, -4.0);
let normal = Vec3::Y;   // projection axis
let tangent = Vec3::X;  // roll about the normal
let _slot = decals.spawn(DecalType::BulletHole, hit, normal, tangent);

// DecalsUpdate: age the pool once per fixed step; returns how many expired (DecalUnlock).
let _freed = decals.update(1.0 / 60.0);

// The render seam draws the live instances, multiplying by the fade alpha.
for inst in decals.iter_live() {
    let (_p, _n, _t) = (inst.position, inst.normal, inst.tangent);
    let (_size, _super_decal, _alpha) = (inst.size, inst.super_decal, inst.alpha());
}
```

Spawning against a specific surface entity honours the ECS suppression tags:

```rust
use mercs2_decal::{DecalWorld, DecalType, Disable3DDecals};
use mercs2_core::{World, glam::Vec3};

let mut world = World::new();
let surface = world.spawn((Disable3DDecals::default(),));
let mut decals = DecalWorld::new();

// Dropped -> None: the entity disables the projected decal pass.
assert!(decals
    .spawn_on_entity(&world, surface, DecalType::Blood, Vec3::ZERO, Vec3::Y, Vec3::X)
    .is_none());
```

A loader that has the retail numbers builds its own table and cap:

```rust
use mercs2_decal::{DecalDef, DecalTable, DecalType, DecalWorld};

let mut table = DecalTable::new();
table.insert(DecalDef { size: 0.15, lifetime: 45.0, ..DecalDef::placeholder(DecalType::BulletHole.hash()) });
let decals = DecalWorld::with(table, 512); // cap from tuning, not DEFAULT_POOL_CAP
```

## Modules

| module | owns |
|---|---|
| `table` | The `decaltable`: `DecalTable` / `DecalDef` / `DecalType` + the recovered ASET constants (`DECALTABLE_TYPE_HASH`, `DECALTABLE_RESIDENT_ALLOC`, `DECALTABLE_RESIDENT_FLAG`, `DECALTABLE_DATA_ADDR`, the two bind-slot param names). |
| `pool` | `DecalPool` / `DecalInstance`: the bounded `CreateDecals` / `DecalsUpdate` / `DecalUnlock` runtime — spawn, age, free, evict-oldest, fade alpha. |
| `components` | The `DisableDecals` / `Disable3DDecals` ECS toggle tags + their `suppresses_*` queries. |

The crate root additionally exports `DecalWorld`, the table+pool pair the host ticks.

## Notes / gotchas

* **The table numbers are not recovered.** The retail block is read via computed offsets inside
  stripped functions, never by name, so per-type texture handle / size / lifetime are `confirm-live`
  data. `DecalTable::stock()` seeds the recovered *categories* with `DecalDef::placeholder` params
  (size `1.0`, lifetime `30 s`) purely so the mechanism is exercisable — those are not retail values.
* **`DEFAULT_POOL_CAP` (256) is a reimpl default, not a recovered number.** The `0x400`-byte alloc
  sizes the *table* object, not the instance pool. The retail cap is `confirm-live`; pass it to
  `DecalPool::new` / `DecalWorld::with` when known.
* **The fade curve is a reimpl choice.** `DecalsUpdate` is stripped, so `alpha()` fades linearly over
  the trailing `FADE_FRACTION` (0.25) of an instance's lifetime.
* `lifetime <= 0` means **permanent**: the instance never ages out and is only removed by the pool's
  evict-oldest recycle.
* **The projection shader and the draw are not here.** `PgDecalVP` / `PgDecal2FP`, the
  `_pl` / `_sl` / `_pl_sl` / `_li` light permutations and the `DAT_00dfc345`-gated permutation
  selection are render-side and vtable-driven; this crate hands them the instance list.
* `DisableDecals`'s hash `0xff4533e5` also appears as a config token via
  `FUN_00826820(0xff4533e5, 0)` → global bool `DAT_01175c37`. The code map rates that low-confidence
  and the crate does not model it.
