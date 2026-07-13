# mercs2_combat

Weapons and combat for the Mercenaries 2 reimplementation: `wpn_*` gun stats, the projectile lifecycle, the homing/lock-on FSM, and a damage/explosion applier.

## What it is

A leaf simulation crate (**silo 10**, scoreboard **row 26**) holding the whole per-frame combat pipeline as ECS components plus systems over a `hecs::World`:

* `WeaponSystem` — the per-frame driver. One `tick` sequences the combat passes in the exe's order: **homing lock FSM → firing → guided flight → projectiles → explosions**.
* Firing: fire-rate cooldown, `iClipSize` magazine / reload gating, `iBulletsPerShot`, hitscan-vs-projectile dispatch.
* Homing: the lock-on FSM (acquire / hold / lose), missile launch, and guided flight (cross-product steering + gravity bias + detonation timer).
* Projectiles: gravity → movement → swept raycast, direct hits, lifetime expiry, explosive fuzes.
* Damage: radius falloff, the `DamageKey` taxonomy, and `DamageMsg` / `DestroyMsg` posts into the destruction state machine.
* Impacts: an output channel of resolved-hit records (bullet hole / blood splatter / explosion mark) for the decal and particle consumers to spawn FX from.
* `wpn_*` stat loading and the engine-side bodies for the `Weapon` / `Airstrike` / inventory Lua cfuncs.

Hit tests go through `mercs2_core::PhysicsQuery` (silo 7); events go on the `mercs2_core` event bus. It depends only on `mercs2_core` + `mercs2_formats` — no leaf→leaf edge.

## Where it comes from

Derived from `docs/reverse_engineer/weapons_combat_code_map.md` and the ecs-01 component schemas. Faithful ports, read first-hand in the code map:

| Reimpl | Exe |
| --- | --- |
| `WeaponSystem::update` — layer-4 gameplay-system tick order | `FUN_0051cff0` |
| `homing::homing_lock_system` — lock FSM (states 1/2/3) | `FUN_0052dce0` |
| `homing::launch_missile` | `FUN_0052d120` |
| `homing::homing_flight_system` — guided-flight integration | `FUN_0052e1f0` |
| `projectile::projectile_system` | Xbox `Update::Gravity` → `Movement` → `Raycast` |

Weapon stats live in **26 `wpn_*` reflection blocks** in the WAD, not in Lua. `stats::parse_weapon_block` unwraps such a block → the weapon-def container (type-hash `0x9f8bca10`) → its UCFX `data` chunk → the `0x787c0871`-tagged (`= pandemic_hash_m2("weapon")`) sub-objects, endian-aware (retail PC `vz.wad` is LE, the Xbox/PS3 source BE).

Event identities (`HomingLockStart/Update/Clear`, `HomingLaunched`, `WeaponEvent`, `DamageMsg 0xC6507EE1`, `DestroyMsg 0x1ED7AD78`) are the verified hashes; a test asserts each equals `mercs2_formats::hash::pandemic_hash_m2(name)`.

**The one documented wall — confirm-live:** the exe's per-hit damage/explosion solver (`ApplyDamage*` / `UpdateExplosions` / `PhysicsCreateExplosion` / `ApplyExplosionToBodies`) is string-only/SecuROM on both builds and is **unread**. `damage` is a clearly-marked modern stand-in built from the *authored* dropoff/radius fields; every stand-in choice carries a `// CONFIRM-LIVE:` comment. Its outputs (`DamageMsg`/`DestroyMsg`) are the exe's known outputs. Also confirm-live: the `wpn_*` byte-offset → named-stat binding, so per-weapon stats currently fall back to the recovered exe schema defaults (`iClipSize 30`, `MaxAmmoReserve 60`, `RateOfFire 120`, …). See `DEFERRED.md`.

## Usage

Library crate. Equip a weapon, drive the system, drain the frame's impact FX records:

```rust
use glam::Vec3;
use hecs::World;
use mercs2_combat::{Health, RuntimeWeapon, WeaponStats, WeaponSystem};
use mercs2_core::event::EventBus;
use mercs2_core::Transform;

let mut world = World::new();
let mut bus = EventBus::new();

let shooter = world.spawn(());
let _target = world.spawn((
    Transform::from_translation(Vec3::new(5.0, 0.0, 40.0)),
    Health::new(100.0),
));

// A lock-on launcher; WeaponStats::default() gives the recovered exe schema defaults.
let mut w = RuntimeWeapon::new(shooter, WeaponStats::rocket_launcher());
w.aim_dir = Vec3::new(5.0, 0.0, 40.0).normalize();
w.trigger_down = true;
world.spawn((w,));

let mut sys = WeaponSystem::default();
for _ in 0..600 {
    // `physics` is the PhysicsQuery seam; None ⇒ hitscans miss, projectiles fly to lifetime,
    // explosions still damage by ECS overlap.
    sys.tick(&mut world, 1.0 / 60.0, &mut bus, None);
    for impact in sys.take_impacts() {
        // feed the decal / particle consumers (impact.kind, impact.point, impact.normal)
        let _ = impact;
    }
}
```

Parsing a real `wpn_*` block:

```rust
use mercs2_combat::stats::parse_weapon_block;

// `block` = the raw wpn_* block bytes; `false` = little-endian (retail PC vz.wad).
if let Some(blob) = parse_weapon_block(block, false) {
    for sub in &blob.sub_objects {
        println!("sub-object @{:#x}: {} field words", sub.offset, sub.words.len());
        // sub.f32(i) / sub.i32(i) read a field word; the offset→name binding is confirm-live.
    }
}
```

`WeaponSystem::update(world, dt, bus, physics)` is the stateless entry point for callers that do not consume the impact channel.

## Modules

| Module | Owns |
| --- | --- |
| `components` | The live ECS instances: `RuntimeWeapon`, `RuntimeProjectile`, `RuntimeHomingWeapon`, `RuntimeExplosion`, `HomingState`, `Health`, `Inventory`. |
| `stats` | The `wpn_*` weapon-def blob parser and the authored stat structs (`WeaponStats`, `ExplosiveStats`, `HomingStats`, `FireType`, `WeaponSubObject`, `WeaponDefBlob`). |
| `firing` | Trigger → shot: rate-of-fire, magazine/reload, hitscan vs projectile spawn; defers homing launches. |
| `homing` | The lock-on FSM, missile launch, and guided-flight integration. |
| `projectile` | Projectile integration (gravity/movement/raycast) and the explosion aging pass. |
| `damage` | The confirm-live damage/explosion applier: `DamageKey`, `ExplosionSize`, `apply_hit`, `radius_falloff`, `detonate_explosion`. |
| `impact` | The `Impact`/`ImpactKind` output channel for decal + particle consumers. |
| `events` | The verified combat event name-hashes posted on the event bus. |
| `lua_surface` | Engine-side bodies for the `Weapon.*`, `Human.Inventory.*`, `Object.SetInfiniteAmmo` and `Airstrike.*` cfuncs. |

## Notes / gotchas

* **Do not read `damage` as recovered math.** The falloff curve and mitigation are a stand-in; only the taxonomy and the emitted messages are recovered. Anything marked `// CONFIRM-LIVE:` is a modern choice awaiting a live capture.
* **`WeaponStats` values are schema defaults, not per-weapon values.** The blob is parsed, but the positional field order is not pinned on disk, so naming offsets would mean inventing numbers. Sub-objects are exposed raw for the follow-up.
* Passing `physics: None` is legal and degrades predictably (hitscans miss; projectiles fly to lifetime; explosions still damage by ECS overlap) — it is how the crate ran before the physics silo landed.
* Explosion `Impact` normals are a fixed world-up (`+Y`, canonical game space) FX convention, not a measured surface; degenerate `RayHit` normals fall back to the negated travel direction.
* The Lua *bindings* live in `mercs2_script`; this crate only supplies the bodies they call, because a leaf sim crate may not depend on the script host.
* `Health` is a local stand-in for the destruction silo's `RuntimeHealth`; the event contract (`DamageMsg`/`DestroyMsg`) is already faithful, so retargeting later is a swap, not a rewrite.
* Equip/weapon-visibility, skill-weighted scatter sampling, and the full airstrike flight path are explicitly out of scope (`DEFERRED.md`, all non-blockers).
