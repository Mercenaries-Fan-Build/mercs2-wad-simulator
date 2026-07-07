//! Combat ECS components — the live weapon/projectile/homing/explosion instances.
//!
//! These mirror the engine's `Runtime*` component pools (code map §6). The exe's `Runtime*` classes
//! are **runtime-state serializers** (ecs-01: no authored schema — they hold live spawn state), so the
//! reimpl models them as plain hot-path components carrying exactly the state the per-tick systems read
//! and write. Strides/hashes from the code map are recorded on each type for traceability; the Rust
//! layout is ours (the exe is the oracle for *behaviour*, not byte layout of a runtime instance).

use glam::Vec3;
use hecs::Entity;

use crate::stats::WeaponStats;

/// `RuntimeWeapon` (hash `0xec62e3a3`, exe stride `0x34`; registrar `FUN_0063dcf0`, instance
/// serializer `FUN_00666f00`/`006670e0`). The live weapon a character holds: its equipped stats plus
/// firing state (magazine, reload, fire-rate cooldown, trigger). One shot is produced by the firing
/// system when the trigger is down, the cooldown has elapsed, and the clip is non-empty.
#[derive(Clone, Debug)]
pub struct RuntimeWeapon {
    /// The owning actor (shooter). `RayHit`/`DamageMsg` attribute back to this entity.
    pub owner: Entity,
    /// The equipped gun's stats (from its `wpn_*` blob, or the exe defaults).
    pub stats: WeaponStats,
    /// Rounds currently in the magazine (`iClipAmmo`).
    pub clip_ammo: i32,
    /// Rounds in reserve (`MaxAmmoReserve` pool; consumed on reload).
    pub reserve_ammo: i32,
    /// Seconds until the next shot may fire (counts down; `<= 0` ⇒ ready). Seeded to `fire_interval`.
    pub fire_cooldown: f32,
    /// Trigger held this tick — the firing system's gate (set by input/AI, or Lua `Weapon`/fire).
    pub trigger_down: bool,
    /// A `SemiAutomatic`/`Burst` latch: the trigger must be released before it fires again.
    pub trigger_latched: bool,
    /// True while a reload is in progress (`bReloading`); no shots during a reload.
    pub reloading: bool,
    /// Seconds left in the current reload.
    pub reload_timer: f32,
    /// Homing lock state, if this is a lock-on launcher (`stats.homing.is_some()`).
    pub lock: HomingState,
    /// The muzzle in world space (where projectiles spawn / hitscans originate).
    pub muzzle: Vec3,
    /// Unit aim direction in world space.
    pub aim_dir: Vec3,
    /// Equipped as the character's primary (vs secondary) — backs `Weapon.IsPrimary` (code map §7).
    pub primary: bool,
    /// Infinite-ammo toggle — backs `Object.SetInfiniteAmmo` (code map §7). When set, firing consumes
    /// no clip/reserve rounds.
    pub infinite_ammo: bool,
}

impl RuntimeWeapon {
    /// A freshly-equipped weapon: full clip, full reserve, ready to fire.
    pub fn new(owner: Entity, stats: WeaponStats) -> Self {
        let clip = stats.clip_size.max(0);
        Self {
            owner,
            clip_ammo: clip,
            reserve_ammo: stats.max_ammo_reserve.max(0),
            fire_cooldown: 0.0,
            trigger_down: false,
            trigger_latched: false,
            reloading: false,
            reload_timer: 0.0,
            lock: HomingState::None,
            muzzle: Vec3::ZERO,
            aim_dir: Vec3::Z, // +Z north, canonical game space
            primary: true,
            infinite_ammo: false,
            stats,
        }
    }

    /// Whether a reload can begin (magazine not full and reserve available). Mirrors the
    /// `ReadyToReload` predicate role (code map §8.6; PC body unlocated → this is the faithful analog).
    pub fn can_reload(&self) -> bool {
        !self.reloading && self.clip_ammo < self.stats.clip_size && self.reserve_ammo > 0
    }
}

/// The homing lock state machine (code map §4.2 — `HomingLockStart→Update→Clear`). The FSM state codes
/// map to the exe's `local_44` lock-state selector: `Acquiring` emits `HomingLockStart` (2) on entry
/// and `HomingLockUpdate` (3) while holding; `None` after a `HomingLockClear` (1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HomingState {
    /// No target under the reticle.
    None,
    /// A target is held; `timer` counts down from `LockOnTime`. On reaching 0 → `Locked`.
    Acquiring { target: Entity, timer: f32 },
    /// Lock acquired; the launcher may fire a guided missile at `target`.
    Locked { target: Entity },
}

impl HomingState {
    /// The currently-held/locked target, if any.
    pub fn target(&self) -> Option<Entity> {
        match self {
            HomingState::None => None,
            HomingState::Acquiring { target, .. } | HomingState::Locked { target } => Some(*target),
        }
    }
}

/// `RuntimeProjectile` (hash `0x9d2ab1a6`, exe stride `0xa0`; registrar `FUN_0063dda0`). A generic
/// ballistic projectile in flight — the per-tick system integrates velocity + gravity and raycasts the
/// swept segment for impact (`Update::Gravity`/`Update::Movement`/`Update::Raycast`, code map §3).
#[derive(Clone, Debug)]
pub struct RuntimeProjectile {
    /// The shooter, for damage attribution.
    pub owner: Entity,
    /// World-space position.
    pub pos: Vec3,
    /// World-space velocity (m/s).
    pub vel: Vec3,
    /// Gravity acceleration (+down, m/s²).
    pub gravity: f32,
    /// Seconds of life left; at `<= 0` the projectile self-detonates/despawns.
    pub life: f32,
    /// Damage this projectile deals on a direct hit.
    pub damage: f32,
    /// Damage taxonomy key.
    pub damage_key: crate::damage::DamageKey,
    /// If `Some`, on impact/expiry spawn a `RuntimeExplosion` with these params (explosive round).
    pub explosive: Option<crate::stats::ExplosiveStats>,
}

/// `RuntimeHomingWeapon` (hash `0xc09adb1b`, exe stride `0x54`; registrar `FUN_00645e30`, launch
/// `FUN_0052d120`). A guided missile in flight. The guided-flight system integrates it with
/// **cross-product steering toward the target + a gravity bias + a detonation/arm timer**, a direct
/// port of `FUN_0052e1f0` (code map §4.4).
#[derive(Clone, Debug)]
pub struct RuntimeHomingWeapon {
    /// The launching actor, for damage attribution.
    pub owner: Entity,
    /// The locked target this missile steers toward (`piVar1[0x11]` armed-target).
    pub target: Entity,
    /// World-space position.
    pub pos: Vec3,
    /// World-space velocity (m/s).
    pub vel: Vec3,
    /// Steering rate — how fast the velocity rotates toward the target (`TurnSpeed`, `DAT_00b92874`).
    pub turn_speed: f32,
    /// Gravity bias applied each tick (`DAT_00b9b664`), pulling the missile down.
    pub gravity: f32,
    /// Detonation proximity: within this distance of the target, detonate now.
    pub detonation_distance: f32,
    /// Arm/detonation timer (`piVar1[0x12]`), counts down by dt; at `<= 0` the missile detonates.
    pub arm_timer: f32,
    /// The warhead's blast params.
    pub explosive: crate::stats::ExplosiveStats,
    /// Damage key (typically `RocketLarge`).
    pub damage_key: crate::damage::DamageKey,
}

/// `RuntimeExplosion` (hash `0x5529dd38`, exe stride `0x40`; producer `FUN_0066ae30`). A live blast that
/// applies radial damage/force to bodies within its radius over its (short) life, then despawns. The
/// applier is the confirm-live stand-in (`crate::damage`).
#[derive(Clone, Debug)]
pub struct RuntimeExplosion {
    /// The instigator, for damage attribution.
    pub owner: Option<Entity>,
    /// Blast centre.
    pub pos: Vec3,
    /// Blast params (radius / force / damage / falloff).
    pub stats: crate::stats::ExplosiveStats,
    /// Damage taxonomy key.
    pub damage_key: crate::damage::DamageKey,
    /// Whether this blast has already applied its damage (a blast applies once, on its first tick).
    pub applied: bool,
    /// Remaining lifetime (s) for the visual/force to linger before despawn.
    pub life: f32,
}

/// A minimal health component — the **local stand-in** for the destruction silo's `RuntimeHealth
/// {cur,max}` (producer `FUN_004cfed0`). The damage applier writes `cur` and posts `DamageMsg`/
/// `DestroyMsg`; when the destruction silo lands, retarget the applier at its `RuntimeHealth` and drop
/// this (`DEFERRED.md`). Kept here so combat is testable without a leaf→leaf edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Health {
    pub cur: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { cur: max, max }
    }
    pub fn is_dead(&self) -> bool {
        self.cur <= 0.0
    }
}

/// A character's weapon loadout — the engine-side backing of `Human.Inventory.SetAllWeapons` (code map
/// §7). The `weapons` are the carried gun stats; index [`equipped`] is the active one (mirrored into a
/// [`RuntimeWeapon`] on the same entity when equipped). This is the combat-silo slice of the human
/// inventory; the full pickup/ammo-pack economy lives in the player/inventory silo.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    /// The carried weapons' stats, in slot order.
    pub weapons: Vec<crate::stats::WeaponStats>,
    /// Index into `weapons` of the currently-equipped weapon.
    pub equipped: usize,
}
