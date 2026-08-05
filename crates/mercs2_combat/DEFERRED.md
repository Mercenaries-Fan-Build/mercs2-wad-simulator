# mercs2_combat — deferred improvements

Non-blocking improvements intentionally left for a later pass. Each is tagged `[faithful-blocker: no]`
— omitting it does NOT make the current behaviour less faithful to the exe oracle; it is scope/quality,
not correctness. The exe's per-hit ballistic/explosion solver math — long the documented **wall** — is
now **recovered from the sibling "WildStar" engine** (`docs/reverse_engineer/saboteur_damage_solver_symbol_map.md`);
what remains is confirming its constants against the Mercs2 prototype body (see the Damage/explosion
section below), not a blind live capture.

## wpn_* stat data

- **Reflection schemas recovered; per-weapon stat SOURCE is the remaining confirm** `[faithful-blocker: no]`
  — the exact slot→named-field order + types + defaults for the five gun-stat classes are now recovered
  first-hand from the schema-declarator bodies (`FUN_0065ca70` WeaponProjectileBase, `FUN_0065cc50`
  WeaponScatter, `FUN_0065dc00` ProjectilePhysics, `FUN_0065d6e0` Explosive, `FUN_0065d930`
  HomingWeapon) and encoded in `stats::schema`. `WeaponStats::apply_component_record(class_hash, words)`
  decodes a genuine component record to NAMED per-weapon stats.
  **⚠ Correction (verified 2026-08-04, retail `vz.wad` LE):** the `0x787c0871`
  (`= pandemic_hash_m2("weapon")`) sub-objects that `parse_weapon_block` enumerates are **NOT** the
  `WeaponProjectileBase`/`WeaponScatter`/… stat components — they are the weapon's **scene-graph nodes**.
  Extracting the real weapon-def data chunks (sniperrifle 8 / combatrifle 10 / shotgun 8 / rocketlauncher
  2 / antiair 1 records — matching the documented counts) shows every record shares a node header
  (`0, flag, 0.96, flag, near, far, 1, 1, 1, …`), back-refs the weapon's own asset name-hash (e.g.
  `0x071faae2` for sniperrifle) as an owner pointer, and carries `name_hash,child_index` slot pairs — a
  render/attach graph (LOD near/far, tint, muzzle hardpoints). The proof they are not the stat classes:
  in the whole weapon-def data chunk the stat class-hashes (`0xeb505c8b` …) appear **0×**,
  `RateOfFire = 120.0` appears **0×**, and `iRoundsPerReload`/`FirstMagazine = -1` (`0xffffffff`, two
  guaranteed `WeaponProjectileBase` fields) appear **0×**. So the per-weapon stat *values* are not in
  this chunk; they live in a `vz_state` weapon overlay or a hand-rolled `.rdata` table
  (`combat_vehicle_economy_gaps.md`, `data-defaults.md §1.4`), OR the reflection stream is delta-encoded
  and the field-mask is unread. The prior memory note [[weapon-definitions-wpn-blocks]] read those node
  words as "stat floats at fixed offsets" and observed real per-weapon diffs — but those are diffs in
  LOD/transform node values, and the note itself admitted the offsets "misalign after ~0x100".
  Naming node words as stats would invent numbers, which this loader refuses to do.
  **CONFIRM-LIVE (the per-weapon stat source):** x32dbg BP on the `WeaponProjectileBase` `CopyFromStream`
  (`PTR_CopyFromStream_00bbe328`) / `FUN_0064a600` while a `wpn_*` block loads, read the freshly-written
  `0x28`-byte record, and cross-ref its `iClipSize`/`RateOfFire` against `stats::schema`. Until then a
  non-overriding weapon genuinely uses the **declarator-recovered defaults** (`iClipSize 30`,
  `MaxAmmoReserve 60`, `iBulletsPerShot 1`, `RateOfFire 120`, `MaxAimAngleAi 15`, scatter 1.5, etc.).

- **The other two ASET entries** `[faithful-blocker: no]` — a `wpn_*` block's entries [1] `sounddb`
  (`0xe5273c14`) and [2] `wavebank` (`0xf753f6d0`) are the weapon's audio; parsing them belongs to the
  audio system, not combat.

## Damage / explosion

- **The exe's exact ballistic/explosion solver math** `[faithful-blocker: WILDSTAR-recovered, verify vs Mercs2]`
  — no longer a wall. Recovered from the sibling engine (The Saboteur / "WildStar" Xbox 360 devkit):
  `WSDamageable::ApplyDamage` = `health -= amount * damageScale`; `WSExplosion::CreateExplosion` falloff =
  linear `(radius - dist)/radius` to the nearest box point, point-blank = 1.0; deferred + staggered apply
  (`dist × 1/30`, 1.5 s lifetime); force floor 200; 7-bone ragdoll spread
  (`docs/reverse_engineer/saboteur_damage_solver_symbol_map.md`). `damage::apply_hit` /
  `detonate_explosion` now implement that shape (`// WILDSTAR:` comments); the falloff, `DamageKey`
  taxonomy, and event contract are faithful. **Residual (verify vs Mercs2):** the exact numeric constants
  are WildStar's — confirm against the Mercs2 Jul-08 prototype body (`ApplyExplosionToBodies` /
  `ApplyDamageToNodeHealth`, decompilable, no SecuROM) via the Havok-AABB-phantom anchor, and pin the
  Mercs2 two-tier Primary/Node health split.

- ~~**Deferred + staggered blast application**~~ **DONE.** `RuntimeExplosion` now gathers its victim
  list once (`gather_explosion_victims` = `WSExplosion::CreateExplosion`), then `update_explosion`
  (= `WSExplosion::Update`, driven by `projectile::explosion_system`) counts each victim's
  `dist × wildstar::STAGGER_SECS_PER_METER` countdown down and applies force+damage as it elapses, over
  `wildstar::LIFETIME_SECS`. Near victims fire first; the ragdoll blast-impulse (floor 200) lands on the
  lethal frame. `detonate_explosion` is kept as the immediate all-at-once path (same *total* damage) for
  simple callers/tests.

- **Two-tier Primary/Node health split** `[faithful-blocker: no]` — modelled: `apply_hit`
  = `ApplyDamageToPrimaryHealth` (hull `Health`), `apply_node_hit` = `ApplyDamageToNodeHealth`
  (per-node `NodeHealth`; part nodes **tally** `hits` rather than killing the hull, matching the recovered
  `flags & 0x80` part-node behaviour). Which authored parts get a `NodeHealth` pool, and the
  `LookupNodeIdFromBodyId` body→node map, come from the destruction/vehicle asset data — the remaining
  wiring, not a solver gap.

- **The exact Mercs2 numeric constants** `[faithful-blocker: CONFIRM-LIVE]` — the *algorithm* is
  recovered and structurally confirmed for Mercs2 (Xenon prototype scope names), but the numbers
  (`1/30` stagger, `1.5 s` lifetime, force floor `200`, `amount × damageScale` per-target scale values,
  the `DamageKey × ModifierKey` matrix) are the sibling WildStar's — the Mercs2 numeric bodies are
  genuine VMX128 in the prototype and a BSim cross-fork match fell below the noise floor. Each is marked
  `// CONFIRM-LIVE:` in `damage.rs`. The check: HW write-BP on the player's `RuntimeHealth.cur`
  (`FUN_0066f220 → … → FUN_006696a0`, `cur@+0x04`, stride `0xc`) in x32dbg and read the applied delta.

- **Explosion body-set query** `[faithful-blocker: no]` — `detonate_explosion` finds targets by an ECS
  spatial sweep over entities carrying a `Health` (the local `RuntimeHealth` analog) within the blast
  radius, with an optional `PhysicsQuery` line-of-sight raycast for cover. The exe's
  `PhysicsCreateExplosion` queries the Havok broadphase for `hkpRigidBody` overlap; the precise body set
  (and impulse application) lands with the physics system. The gameplay-damage overlap is faithful.

- ~~**RuntimeHealth ownership**~~ **DONE.** The destruction system landed and produced no competing type:
  `Health` is now single-defined in `mercs2_core::components`, this crate imports it, and
  `mercs2_destruction` consumes `mercs2_core::{Health, Destructible}`. The seam item 5's first half is
  closed, and there was never a `RuntimeHealth` to retarget at.

- **Hoisting the inventory types to `mercs2_core`** `[faithful-blocker: no]` — `RuntimeInventory` /
  `Equipment` / `CarriedBy` live here, and the seam review anticipated hoisting them alongside
  `Health`. **Deliberately not done**, because that item's trigger has not fired: there is exactly one
  `struct Inventory`-shaped type in the workspace, and nothing outside this crate reads a loadout.
  Hoisting now would also drag the `Entity`-typed carry relation into the crate that "deliberately
  depends on nothing but hecs + glam", and would buy nothing at the binding seam — `mercs2_script`
  cannot name a `mercs2_combat` type either way, so the `EngineHost` signatures stay scalar regardless.
  **Trigger to revisit:** a second crate needing to query a loadout — most likely `mercs2_population`
  giving NPCs loadouts through the second apply path (`FUN_006F9260`), or a player save/load path.

- **The carry edge's `+0x04` object** `[faithful-blocker: no]` — retail's `RuntimeEquipmentLink`
  (`0x00DF9510`) edge carries a per-edge flag (bit `0x02` = equipped, modelled) plus a `+0x04` pointer
  whose target is unidentified (`inventory_equipment_code_map.md` §9.1). Modelled as absent.

- **`FUN_004F30D0`, the destroy primitive** `[faithful-blocker: no]` — genuinely VM-blocked, and static
  exhaustion was already performed (§9.2). "Despawn the entity and unregister the guid" is the stand-in;
  `inventory::drain_pending_destroy` is where it lives.

- **`FUN_005280A0`'s veto predicate** `[faithful-blocker: no]` — retail's destroy-all is not
  unconditional: that call can veto individual weapons. The predicate is unread, so
  `destroy_all_weapons` queues everything.

## Firing / equip

- **Equip / weapon-visibility state machine** `[faithful-blocker: no]` — `FUN_0051c200`'s equip/attach/
  detach + first-person-vs-world visibility switch (the `0x5429d8ec`… id family) is not modelled; only
  the fire/clip/reload/rate-of-fire leaf is. Equip is a HUD/animation concern for a later pass.

- **Scatter / spread sampling** `[faithful-blocker: no]` — `WeaponScatter` fields are loaded but the
  per-shot cone-spread RNG (`LowSkillScatter`/`ScatterMin`/`Max`/`CenterBias`) is applied as a simple
  symmetric cone; the exe's skill-weighted distribution is a refinement.

- **SecuROM-virtualised firing leaves** `[faithful-blocker: CONFIRM-LIVE]` — several `FUN_0051cff0`
  pooled-pass leaves route through the SecuROM VM dispatcher (`thunk_FUN_03410000`…, code map §2/§8).
  The driver structure is faithful; the virtualised commit is read live in the unpacked image.

## Lua surface

- **Airstrike flight / delivery model** `[faithful-blocker: no]` — `Airstrike.Flyby` spawns the airplane
  + ordnance instances and drops on the flight path; the full `RuntimeAirstrikeAirplane` (0xb0)
  approach/turn/egress path (ecs-01) is a later refinement. `SpawnOrdnance`/`ConeSpawn` (the projectile
  spawns) are faithful.
