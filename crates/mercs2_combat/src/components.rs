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
/// applies radial damage/force to bodies within its radius, **deferred and staggered by distance** over
/// its life (the recovered `WSExplosion::CreateExplosion` → `Update` cadence — near victims first), then
/// despawns. The applier is `crate::damage` (recovered sibling-engine solver).
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
    /// Whether this blast has gathered its victim list yet (the `CreateExplosion` pass runs once, on the
    /// first tick, then `Update` drains the list). Named `applied` for compatibility.
    pub applied: bool,
    /// Age since detonation (`WSExplosion.timer@0x1c`); the blast frees itself at
    /// [`wildstar::LIFETIME_SECS`](crate::damage::wildstar::LIFETIME_SECS).
    pub life: f32,
    /// The gathered, distance-staggered victim queue (empty until `applied`); drained by
    /// [`crate::damage::update_explosion`].
    pub victims: Vec<crate::damage::PendingBlastVictim>,
}

impl RuntimeExplosion {
    /// A freshly-detonated blast: victims un-gathered, timer at zero.
    pub fn new(
        owner: Option<Entity>,
        pos: Vec3,
        stats: crate::stats::ExplosiveStats,
        damage_key: crate::damage::DamageKey,
    ) -> Self {
        Self { owner, pos, stats, damage_key, applied: false, life: 0.0, victims: Vec::new() }
    }
}

// `Health` is **not** defined here. It is the shared `RuntimeHealth {cur,max}` analog owned by
// `mercs2_core` (producer `FUN_004cfed0`) — damage, destruction and any health-bearing query must
// agree on one component type, so each site imports `mercs2_core::Health` directly rather than
// through a combat-local alias.

/// A character's weapon loadout — the **`RuntimeInventory`** record
/// (`inventory_equipment_code_map.md` §2.1), carried on the *character* entity.
///
/// Eleven dwords `+0x00 … +0x28` plus a flags dword at `+0x2C`, closing exactly on the registrar's
/// static record size of `0x30` with no unaccounted offset.
///
/// # What this replaced, and why it could not stand
///
/// The previous model was `{ weapons: Vec<WeaponStats>, equipped: usize }`. Three things were wrong
/// with it, each load-bearing:
///
/// 1. **One `equipped` index cannot represent retail.** A human has an equipped primary **and** an
///    equipped secondary **and** possibly a vehicle weapon *simultaneously*, so `GetPrimaryWeapon` and
///    `GetSecondaryWeapon` were made mutually exclusive by construction.
/// 2. **Storing `WeaponStats` by value made the returned values non-entities.** Shipped Lua calls
///    `Object.GetParent(w)`, `Weapon.GetReserveAmmo(w)`, `Object.SetPosition(w, …)` and
///    `Object.HasLabel(w, "Grenade")` on whatever `GetAllWeapons` hands back — they must be real ECS
///    entities, and their ammo must live on the weapon, not be copied into the human's record.
/// 3. **The secondary carousel is three rungs deep, not two.** `FUN_00527C70` rotates
///    `+0x04 ← +0x10 ← +0x14`, so a two-field model gives the rotation the wrong third rung.
///
/// # Naming confidence
///
/// The offsets and the *semantics* are **H** (read on the PC side). Most of the field *names* are **M**:
/// they come from positionally joining the Xbox debug build's literal pool against the PC evidence.
/// The one place that join was previously wrong is recorded on [`last_last_secondary`](Self::last_last_secondary).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuntimeInventory {
    /// `+0x00` — equipped **primary** weapon (`GetPrimaryWeapon` `0x005BEA61`). `None` = empty slot.
    pub equipped_primary: Option<Entity>,
    /// `+0x04` — equipped **secondary** (`GetSecondaryWeapon` `0x005BEBE1`; rotated by `FUN_00527C70`).
    pub equipped_secondary: Option<Entity>,
    /// `+0x08` — equipped **vehicle/mounted** weapon (`GetVehicleWeapon` `0x005BED4D`, no fallback).
    pub equipped_vehicle: Option<Entity>,
    /// `+0x0C` — last-equipped primary, i.e. the *other* primary slot. `GetPrimaryWeapon` falls back to
    /// it at `0x005BEA67`.
    pub last_primary: Option<Entity>,
    /// `+0x10` — last-equipped secondary; `GetSecondaryWeapon`'s fallback at `0x005BEBE8`.
    pub last_secondary: Option<Entity>,
    /// `+0x14` — **last-*last*-equipped secondary**: the third rung of the secondary carousel.
    ///
    /// ⚠ Earlier revisions of the code map named this the pickup holding-pen. That is retracted, and the
    /// PC side settles it independently of the Xbox name join: a slot `FUN_00527C70` **rotates through
    /// on every secondary swap** cannot be a pickup pen. The pen is [`pending_pickup`](Self::pending_pickup)
    /// at `+0x18`.
    pub last_last_secondary: Option<Entity>,
    /// `+0x18` — equipment waiting to be picked up (`FUN_0051B1E0`, zeroed at `0x0051B972`).
    pub pending_pickup: Option<Entity>,
    /// `+0x1C` — ammo prop (`FUN_0051B1E0` `0x0051B45F`).
    pub ammo_prop: Option<Entity>,
    /// `+0x20` — the mounted/emplaced weapon currently in use. **Non-zero ⇒ the human is on a turret**,
    /// and every loadout mutator detaches first. `EnableWeapons` zeroes it.
    pub weapon_in_use: Option<Entity>,
    /// `+0x24` — pending equip action: set by the draw path, tested and cleared by the equip tick.
    pub current_equip_action: u32,
    /// `+0x28` — weapon visibility, enum-valued. A newtype rather than an enum because only one
    /// comparison (`cmp eax,2`) is recovered; inventing variant names would be a fabrication.
    pub weapon_visibility: WeaponVisibility,
    /// `+0x2C` — the flags dword. See [`InventoryFlags`].
    pub flags: InventoryFlags,
}

/// `RuntimeInventory+0x28`. Only the `== 2` comparison is recovered (`FUN_0051C140` `0x0051C1DB`), so
/// this stays a newtype over the raw value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WeaponVisibility(pub u32);

/// `RuntimeInventory+0x2C` — the flags dword.
///
/// Bit names are **M** (positional, from the Xbox pool's packed line); the bit *semantics* are **H**,
/// read from the 19 gate sites.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InventoryFlags(pub u32);

impl InventoryFlags {
    /// `& 0x08` — **locked**. Every loadout mutator returns false immediately. See
    /// `crate::inventory::gate`.
    pub const LOCKED: u32 = 0x08;
    /// `| 0x04` — a draw is in flight. Cleared on failure (`&= 0xF9`).
    pub const DRAWING: u32 = 0x04;
    /// `| 0x02` — a holster/swap is in flight. Also cleared by the `&= 0xF9` failure path.
    pub const SWAPPING: u32 = 0x02;

    pub fn locked(self) -> bool {
        self.0 & Self::LOCKED != 0
    }

    pub fn set_locked(&mut self, on: bool) {
        if on {
            self.0 |= Self::LOCKED;
        } else {
            self.0 &= !Self::LOCKED;
        }
    }

    /// The `&= 0xF9` failure path: clear both in-flight bits, leaving `LOCKED` untouched.
    pub fn clear_in_flight(&mut self) {
        self.0 &= !(Self::DRAWING | Self::SWAPPING);
    }
}

/// The registrar's static record size for `RuntimeInventory` (`0x00645782`), agreeing with the live
/// descriptor. The eleven dwords plus the flags dword close exactly inside it.
pub const RUNTIME_INVENTORY_STRIDE: usize = 0x30;
/// The `RuntimeInventory` container global.
pub const RUNTIME_INVENTORY_CONTAINER: u32 = 0x017B_F3D8;

/// `Equipment` — the **slot-class tag, carried on the weapon**, not on the human
/// (`inventory_equipment_code_map.md` §3.1, container `0x017BCDB8`).
///
/// `Weapon.IsPrimary` must read this same field, or the `Weapon` and `Human.Inventory` namespaces
/// disagree about what a given weapon is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Equipment {
    pub class: EquipmentType,
}

/// The recovered `EquipmentTypeEnum` values. Only these two are used by the loadout paths; the
/// unlabeled remainder of the enum is deliberately not modelled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EquipmentType {
    #[default]
    Primary,
    Secondary,
}

/// The `Equipment` container global.
pub const EQUIPMENT_CONTAINER: u32 = 0x017B_CDB8;

/// `CarriedBy` — the **carry relation edge, carried on the weapon** (retail's `RuntimeEquipmentLink`
/// container `0x00DF9510`, §3.2).
///
/// # Why the edge lives on the child
///
/// `hecs` has no relation primitive, and three shapes were possible:
///
/// * a `Vec<Entity>` on the human — rejected: it has nowhere to put the **per-edge flag**, and both
///   `GetAllWeapons` and `FUN_005283F0` consult that flag rather than the human's record;
/// * a side-table resource — rejected: `hecs` has no resources, so it would live on the host, which is
///   exactly the shadow table this change removes;
/// * the edge as a component on the weapon — **chosen**. Find-edge is an O(1) component get, and
///   "at most one holder per weapon" becomes structural rather than an invariant to police.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarriedBy {
    /// The human carrying this weapon.
    pub holder: Entity,
    /// The per-edge flags retail keeps on the link record. See [`CarriedBy::EQUIPPED`] and
    /// [`CarriedBy::EXCLUDED`].
    pub flags: u32,
    /// Insertion order, so `GetAllWeapons` can reproduce a stable sequence.
    pub seq: u32,
}

impl CarriedBy {
    /// Per-edge bit **`0x01`** — this edge is the equipped one. `GetAllWeapons` pushes it *first*
    /// (`0x005BEED8 test byte [esi+8], 1`), and `FUN_005283F0` uses it to decide equip-vs-stow for an
    /// already-attached weapon (`0x005284E3`/`0x00528540`). Written by `FUN_0052A3B0` (`&0xFE` at
    /// `0x0052A46E`/`0x0052A4B3`) and rebuilt by `FUN_00667210` (`&0xFE` at `0x00667366`).
    /// Confidence **H** (§3.2).
    pub const EQUIPPED: u32 = 0x01;

    /// Per-edge bit **`0x02`** — the **exclude** bit: edges carrying it are skipped when
    /// `GetAllWeapons`' second argument is true (`0x005BEE8F test byte [esi+8], 2`). It also
    /// short-circuits the holster path and gates `FUN_0052A3B0`'s push-down.
    ///
    /// ⚠ Confidence **H (behaviour) / OPEN (meaning)**. Its sole writer is `FUN_006FC280`
    /// (`or byte [edi+8],2` @`0x006FC2A5`) when a second object accessor returns, so it appears to
    /// track the presence of the edge's `+0x04` participant — but *what that object is* is the
    /// residual open item (§9.1). Do not name it for what it seems to mean.
    ///
    /// An earlier revision of this file called `0x02` "EQUIPPED" and attached bit `0x01`'s evidence
    /// to it — a wrong-bit attribution plus confidence inflation on a row the map marks open.
    pub const EXCLUDED: u32 = 0x02;

    pub fn is_equipped(&self) -> bool {
        self.flags & Self::EQUIPPED != 0
    }

    /// Whether `GetAllWeapons(uChar, true)` should skip this edge.
    pub fn is_excluded(&self) -> bool {
        self.flags & Self::EXCLUDED != 0
    }
}

/// The carry-relation container global.
pub const CARRY_RELATION_CONTAINER: u32 = 0x00DF_9510;

/// `PendingDestroy` — a weapon queued for destruction by `SetAllWeapons`/`DestroyAllWeapons`.
///
/// **The destroy is a deferred queue push, not a synchronous reap** (§4.9), and that deferral is what
/// makes the shipped snapshot-restore pattern legal: `mrxplayer.lua:661-724` destroys a loadout and
/// re-applies captured GUIDs, which a synchronous reap would invalidate mid-sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PendingDestroy;
