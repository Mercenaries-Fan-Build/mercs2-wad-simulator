# mercs2_combat — deferred improvements

Non-blocking improvements intentionally left for a later pass. Each is tagged `[faithful-blocker: no]`
— omitting it does NOT make the current behaviour less faithful to the exe oracle; it is scope/quality,
not correctness. The exe's per-hit ballistic/explosion solver math — long the documented **wall** — is
now **recovered from the sibling "WildStar" engine** (`docs/reverse_engineer/saboteur_damage_solver_symbol_map.md`);
what remains is confirming its constants against the Mercs2 prototype body (see the Damage/explosion
section below), not a blind live capture.

## wpn_* stat data

- **Exact offset→field binding inside the weapon-def reflection blob** `[faithful-blocker: no]` — the
  loader (`stats::parse_weapon_block`) faithfully unwraps a `wpn_*` block down to the weapon-def UCFX
  `data` chunk and enumerates its `0x787c0871` (`= pandemic_hash_m2("weapon")`) sub-objects with their
  raw field words. It does **not** yet map each sub-object word to a named stat (RateOfFire@word-N …),
  because the field order is positional reflection replayed by the exe schema declarators
  (`FUN_0065ca70` et al., ecs-01 §schemas) and the byte offsets are not pinned by any on-disk name —
  the RE memory note [[weapon-definitions-wpn-blocks]] reached the same honest conclusion ("can't
  reliably name offsets by eyeballing"). Pinning it needs one x32dbg trace of the deserializer against a
  `wpn_*` block. Until then `WeaponStats` carries the **documented exe schema defaults** (real values
  from ecs-01, not invented): `iClipSize 30`, `MaxAmmoReserve 60`, `iBulletsPerShot 1`, `RateOfFire
  120`, `MaxAimAngleAi 15`, scatter/projectile/homing/explosive defaults. Per-weapon overrides are the
  confirm-live follow-up.

- **The other two ASET entries** `[faithful-blocker: no]` — a `wpn_*` block's entries [1] `sounddb`
  (`0xe5273c14`) and [2] `wavebank` (`0xf753f6d0`) are the weapon's audio; parsing them belongs to the
  audio silo, not combat.

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

- **Deferred + staggered blast application** `[faithful-blocker: no]` — the WildStar blast is not
  instantaneous: `WSExplosion::Update` runs the victim list over `wildstar::LIFETIME_SECS` (1.5 s),
  applying each victim at `dist × wildstar::STAGGER_SECS_PER_METER` (blast travels 30 u/s) and applying
  force (`ApplyHitForce` impulse / ragdoll spread) before damage. `detonate_explosion` applies the same
  *total* damage immediately; the timing + impulse are the physics-silo follow-up. Constants are named in
  `damage::wildstar` so the deferred system can consume them directly.

- **Explosion body-set query** `[faithful-blocker: no]` — `detonate_explosion` finds targets by an ECS
  spatial sweep over entities carrying a `Health` (the local `RuntimeHealth` analog) within the blast
  radius, with an optional `PhysicsQuery` line-of-sight raycast for cover. The exe's
  `PhysicsCreateExplosion` queries the Havok broadphase for `hkpRigidBody` overlap; the precise body set
  (and impulse application) lands with the physics silo. The gameplay-damage overlap is faithful.

- **RuntimeHealth ownership** `[faithful-blocker: no]` — damage lands on a local `Health {cur,max}`
  component (the stand-in for the destruction silo's `RuntimeHealth`, producer `FUN_004cfed0`) and posts
  `DamageMsg 0xC6507EE1` / `DestroyMsg 0x1ED7AD78` — the exact events the destruction FSM consumes (code
  map §5.3A). When the destruction silo lands, retarget the applier at its `RuntimeHealth` and drop the
  local `Health`. The event contract is already faithful.

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
