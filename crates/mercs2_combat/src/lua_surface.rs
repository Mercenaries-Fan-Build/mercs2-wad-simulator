//! Engine-side bodies for the `Weapon` / `Airstrike` / inventory Lua surface (code map §7).
//!
//! # The binding seam
//! `mercs2_combat` deliberately does **not** depend on `mercs2_script` (the carve rule: a leaf sim
//! crate depends only on `mercs2_core` + `mercs2_formats`). So the *Lua bindings* live in
//! `mercs2_script` (`bindings/weapon.rs`, `bindings/airstrike.rs`), and this module provides the
//! **real engine-side bodies** those bindings call. The wiring, mirroring the original engine's
//! `Weapon.*` / `Airstrike.*` C-binding tables calling into the native weapon system:
//!
//! ```text
//!   game Lua ──calls──▶ Weapon.SetReserveAmmo(uWeapon, n)   [mercs2_script binding closure]
//!                          │  (closure holds the combat World via the EngineHost seam)
//!                          ▼
//!                       mercs2_combat::lua_surface::weapon_set_reserve_ammo(world, weapon, n)
//! ```
//!
//! These are the **real bodies** (no stubs): they read/write the `RuntimeWeapon` pool (§2.2/§2.3) and
//! spawn ordnance through the projectile/explosion path — exactly what the native cfuncs do. The
//! `mercs2_script` side supplies a thin `EngineHost`-style trait method per cfunc that forwards here;
//! that trait method is the only seam, and it is named for its cfunc.
//!
//! ## Covered surface (code map §7)
//! - **`Weapon.*`**: `GetClipAmmo`, `SetClipAmmo`, `GetReserveAmmo`, `SetReserveAmmo`, `IsDesignator`,
//!   `IsPrimary`.
//! - **`Human.Inventory.*`**: `SetAllWeapons`, `ResetWeapons`; **`Object.SetInfiniteAmmo`**.
//! - **`Airstrike.*`**: `SpawnOrdnance`, `ConeSpawn`, `Flyby`, `SpawnDirectedObject` (the ordnance
//!   spawns; the mission scripts `mrxstrategicmissile`/`mrxfuelairbomb`/`mrxsatclusterbomb` drive
//!   these). *Correction from the brief:* `Munitions` is a pickup entity script, not a namespace — the
//!   ordnance cfuncs are under `Airstrike.*` (code map §7).

use glam::Vec3;
use hecs::{Entity, World};

use crate::components::{Inventory, RuntimeProjectile, RuntimeWeapon};
use crate::damage::DamageKey;
use crate::stats::{ExplosiveStats, WeaponStats};

// ---------------------------------------------------------------------------
// Weapon.*  (RuntimeWeapon pool read/write — §2.2/§2.3)
// ---------------------------------------------------------------------------

/// `Weapon.GetClipAmmo` — rounds in the magazine (`nil`/None if the handle isn't a weapon).
pub fn weapon_get_clip_ammo(world: &World, weapon: Entity) -> Option<i32> {
    world.get::<&RuntimeWeapon>(weapon).ok().map(|w| w.clip_ammo)
}

/// `Weapon.SetClipAmmo` — set the magazine (clamped to `[0, iClipSize]`). Returns `true` on success.
pub fn weapon_set_clip_ammo(world: &mut World, weapon: Entity, n: i32) -> bool {
    if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(weapon) {
        let cap = w.stats.clip_size.max(0);
        w.clip_ammo = n.clamp(0, cap);
        true
    } else {
        false
    }
}

/// `Weapon.GetReserveAmmo` — carried reserve rounds.
pub fn weapon_get_reserve_ammo(world: &World, weapon: Entity) -> Option<i32> {
    world.get::<&RuntimeWeapon>(weapon).ok().map(|w| w.reserve_ammo)
}

/// `Weapon.SetReserveAmmo` — set the reserve (clamped to `[0, MaxAmmoReserve]`). Returns `true` on
/// success. This is the real body of the note's `SetReserveAmmo`.
pub fn weapon_set_reserve_ammo(world: &mut World, weapon: Entity, n: i32) -> bool {
    if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(weapon) {
        let cap = w.stats.max_ammo_reserve.max(0);
        w.reserve_ammo = n.clamp(0, cap);
        true
    } else {
        false
    }
}

/// `Weapon.IsDesignator` — is this a laser designator (paints airstrike targets, no direct damage)?
pub fn weapon_is_designator(world: &World, weapon: Entity) -> bool {
    world.get::<&RuntimeWeapon>(weapon).map(|w| w.stats.designator).unwrap_or(false)
}

/// `Weapon.IsPrimary` — is this equipped as the primary weapon?
pub fn weapon_is_primary(world: &World, weapon: Entity) -> bool {
    world.get::<&RuntimeWeapon>(weapon).map(|w| w.primary).unwrap_or(false)
}

/// `Object.SetInfiniteAmmo(uChar, bEnable)` — the native infinite-ammo toggle (code map §7). Sets the
/// flag on the character's equipped `RuntimeWeapon`.
pub fn object_set_infinite_ammo(world: &mut World, weapon: Entity, enable: bool) -> bool {
    if let Ok(mut w) = world.get::<&mut RuntimeWeapon>(weapon) {
        w.infinite_ammo = enable;
        true
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Human.Inventory.*  (loadout)
// ---------------------------------------------------------------------------

/// `Human.Inventory.SetAllWeapons(uChar, {…})` — force a loadout (code map §7). Stores the weapon list
/// as an [`Inventory`] on `character` and equips slot 0 by inserting a [`RuntimeWeapon`] on the same
/// entity (owner = the character). Returns the equipped weapon entity (the character itself, which now
/// carries the `RuntimeWeapon`), or `None` for an empty list.
pub fn human_set_all_weapons(
    world: &mut World,
    character: Entity,
    weapons: Vec<WeaponStats>,
) -> Option<Entity> {
    if weapons.is_empty() {
        let _ = world.insert_one(character, Inventory { weapons, equipped: 0 });
        return None;
    }
    let equipped = weapons[0];
    let inv = Inventory { weapons, equipped: 0 };
    let _ = world.insert_one(character, inv);
    let _ = world.insert_one(character, RuntimeWeapon::new(character, equipped));
    Some(character)
}

/// `Human.Inventory.ResetWeapons(uChar)` — strip the loadout (empty inventory, remove the equipped
/// `RuntimeWeapon`).
pub fn human_reset_weapons(world: &mut World, character: Entity) {
    let _ = world.remove_one::<RuntimeWeapon>(character);
    let _ = world.insert_one(character, Inventory::default());
}

/// Equip inventory slot `slot` on `character` (swaps the active `RuntimeWeapon`). Returns `true` if the
/// slot exists.
pub fn human_equip_slot(world: &mut World, character: Entity, slot: usize) -> bool {
    let stats = {
        let Ok(mut inv) = world.get::<&mut Inventory>(character) else { return false };
        let Some(s) = inv.weapons.get(slot).copied() else { return false };
        inv.equipped = slot;
        s
    };
    let _ = world.insert_one(character, RuntimeWeapon::new(character, stats));
    true
}

// ---------------------------------------------------------------------------
// Airstrike.*  (ordnance spawns → projectile/explosion path)
// ---------------------------------------------------------------------------

/// `Airstrike.SpawnOrdnance(name, x,y,z, vx,vy,vz, "distance", dist, owner, cb, data)` — spawn one
/// falling ordnance as a [`RuntimeProjectile`] with an explosive warhead. The binding layer resolves
/// `name` → `(ExplosiveStats, DamageKey, damage)`; here we take them directly. `arm_distance` sets the
/// projectile lifetime so it detonates near the aim point (converted from the exe's `"distance"` fuze).
/// Returns the spawned ordnance entity.
#[allow(clippy::too_many_arguments)]
pub fn airstrike_spawn_ordnance(
    world: &mut World,
    owner: Entity,
    pos: Vec3,
    vel: Vec3,
    explosive: ExplosiveStats,
    key: DamageKey,
    direct_damage: f32,
    arm_distance: f32,
) -> Entity {
    // Convert the arm distance to a lifetime along the current speed (fuze), guarding zero speed.
    let speed = vel.length().max(1.0);
    let life = (arm_distance / speed).clamp(0.05, 30.0);
    world.spawn((RuntimeProjectile {
        owner,
        pos,
        vel,
        gravity: 9.81, // ordnance falls under gravity
        life,
        damage: direct_damage,
        damage_key: key,
        explosive: Some(explosive),
    },))
}

/// `Airstrike.ConeSpawn(...)` — spawn `count` ordnance in a downward cone about `dir`, spread by
/// `spread_deg`. Cluster-bomb / fuel-air pattern (`mrxsatclusterbomb`/`mrxfuelairbomb`). Returns the
/// spawned entities.
#[allow(clippy::too_many_arguments)]
pub fn airstrike_cone_spawn(
    world: &mut World,
    owner: Entity,
    origin: Vec3,
    dir: Vec3,
    speed: f32,
    spread_deg: f32,
    count: u32,
    explosive: ExplosiveStats,
    key: DamageKey,
    direct_damage: f32,
) -> Vec<Entity> {
    let dir = dir.normalize_or_zero();
    // Build a basis around `dir` to fan the cone.
    let up = if dir.dot(Vec3::Y).abs() > 0.99 { Vec3::X } else { Vec3::Y };
    let right = dir.cross(up).normalize_or_zero();
    let fwd = right.cross(dir).normalize_or_zero();
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count.max(1) {
        // Deterministic golden-angle spiral inside the cone.
        let t = if count <= 1 { 0.0 } else { i as f32 / (count as f32 - 1.0) };
        let ang = spread_deg.to_radians() * t;
        let phi = i as f32 * 2.399_963; // golden angle (rad)
        let offset = (right * phi.cos() + fwd * phi.sin()) * ang.tan();
        let v = (dir + offset).normalize_or_zero() * speed;
        out.push(airstrike_spawn_ordnance(world, owner, origin, v, explosive, key, direct_damage, speed));
    }
    out
}

/// `Airstrike.Flyby(...)` — a strafing/bombing run: drop `count` ordnance evenly along the segment
/// `from → to` (the plane's flight path), each falling straight down. Models the delivery drop; the
/// full `RuntimeAirstrikeAirplane` approach/egress is deferred (`DEFERRED.md`). Returns the spawned
/// ordnance entities.
#[allow(clippy::too_many_arguments)]
pub fn airstrike_flyby(
    world: &mut World,
    owner: Entity,
    from: Vec3,
    to: Vec3,
    drop_speed: f32,
    count: u32,
    explosive: ExplosiveStats,
    key: DamageKey,
    direct_damage: f32,
) -> Vec<Entity> {
    let mut out = Vec::with_capacity(count as usize);
    let n = count.max(1);
    for i in 0..n {
        let t = if n == 1 { 0.5 } else { i as f32 / (n as f32 - 1.0) };
        let drop = from.lerp(to, t);
        let vel = Vec3::new(0.0, -drop_speed, 0.0); // straight down
        out.push(airstrike_spawn_ordnance(world, owner, drop, vel, explosive, key, direct_damage, drop_speed * 20.0));
    }
    out
}

/// `Airstrike.SpawnDirectedObject(...)` — spawn one directed projectile toward an aim point (a guided
/// strategic missile's terminal segment, `mrxstrategicmissile`). Thin alias over
/// [`airstrike_spawn_ordnance`] with the velocity aimed at `target`.
#[allow(clippy::too_many_arguments)]
pub fn airstrike_spawn_directed_object(
    world: &mut World,
    owner: Entity,
    from: Vec3,
    target: Vec3,
    speed: f32,
    explosive: ExplosiveStats,
    key: DamageKey,
    direct_damage: f32,
) -> Entity {
    let dir = (target - from).normalize_or_zero();
    let dist = (target - from).length();
    airstrike_spawn_ordnance(world, owner, from, dir * speed, explosive, key, direct_damage, dist)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::event::EventBus;

    #[test]
    fn weapon_ammo_get_set_clamps() {
        let mut world = World::new();
        let ch = world.spawn(());
        let stats = WeaponStats::default(); // clip 30 / reserve 60
        world.spawn((RuntimeWeapon::new(ch, stats),));
        let we = world.iter().find_map(|e| {
            let ent = e.entity();
            world.get::<&RuntimeWeapon>(ent).ok().map(|_| ent)
        }).unwrap();

        assert_eq!(weapon_get_clip_ammo(&world, we), Some(30));
        assert!(weapon_set_clip_ammo(&mut world, we, 999)); // clamps to 30
        assert_eq!(weapon_get_clip_ammo(&world, we), Some(30));
        assert!(weapon_set_reserve_ammo(&mut world, we, 5));
        assert_eq!(weapon_get_reserve_ammo(&world, we), Some(5));
        assert!(object_set_infinite_ammo(&mut world, we, true));
        assert!(world.get::<&RuntimeWeapon>(we).unwrap().infinite_ammo);
    }

    #[test]
    fn set_all_weapons_equips_slot_zero() {
        let mut world = World::new();
        let ch = world.spawn(());
        let loadout = vec![WeaponStats::default(), WeaponStats::rocket_launcher()];
        let eq = human_set_all_weapons(&mut world, ch, loadout).unwrap();
        assert_eq!(eq, ch);
        assert!(world.get::<&RuntimeWeapon>(ch).is_ok());
        assert_eq!(world.get::<&Inventory>(ch).unwrap().weapons.len(), 2);
        // Equip the rocket in slot 1.
        assert!(human_equip_slot(&mut world, ch, 1));
        assert!(world.get::<&RuntimeWeapon>(ch).unwrap().stats.homing.is_some());
        // Reset strips it.
        human_reset_weapons(&mut world, ch);
        assert!(world.get::<&RuntimeWeapon>(ch).is_err());
    }

    #[test]
    fn airstrike_cone_spawns_ordnance() {
        let mut world = World::new();
        let mut bus = EventBus::new();
        let owner = world.spawn(());
        let ords = airstrike_cone_spawn(
            &mut world,
            owner,
            Vec3::new(0.0, 100.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            50.0,
            15.0,
            6,
            ExplosiveStats::default(),
            DamageKey::ExplosionLarge,
            0.0,
        );
        assert_eq!(ords.len(), 6);
        // Each is a live projectile.
        for e in &ords {
            assert!(world.get::<&RuntimeProjectile>(*e).is_ok());
        }
        // Run them a moment; nothing panics and they integrate.
        crate::projectile::projectile_system(&mut world, 1.0 / 60.0, &mut bus, None);
    }
}
