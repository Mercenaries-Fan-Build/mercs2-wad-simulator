# mercs2_combat — deferred improvements

Non-blocking improvements intentionally left for a later pass. Each is tagged `[faithful-blocker: no]`
— omitting it does NOT make the current behaviour less faithful to the exe oracle; it is scope/quality,
not correctness. True faithful blockers (the exe's exact per-hit ballistic/explosion solver math) are
the documented **confirm-live** gap and live in `docs/reverse_engineer/weapons_combat_code_map.md` §5/§8,
not here.

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

- **The exe's exact ballistic/explosion solver math** `[faithful-blocker: CONFIRM-LIVE]` — see the code
  map §5. `ApplyDamage*` / `UpdateExplosions` / `PhysicsCreateExplosion` / `ApplyExplosionToBodies` are
  string-only on both builds. `damage::apply_hit` / `damage::detonate_explosion` here are a faithful,
  clearly-marked modern **stand-in** (radius overlap + linear/quadratic distance falloff + the DamageKey
  taxonomy) that consumes the authored dropoff/radius fields; the exe's exact curve + mitigation is
  unread and is NOT claimed. Recover via a HW write-BP on the player's `RuntimeHealth.cur` (§8).

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
