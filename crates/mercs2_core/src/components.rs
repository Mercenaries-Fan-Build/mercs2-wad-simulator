//! Core components. These are plain data — the systems (animation, render, physics, …) read and
//! write them. They deliberately mirror the original engine's model: an entity is a bag of
//! reflection-addressable components. Here we hand-type the hot-path components the sim actually
//! simulates; the long tail of the 220 native reflection classes will hang off a hash-keyed blob
//! component later, so Lua/ObjectScript can touch any of them the way the game does.

use glam::{Mat4, Quat, Vec3};

/// World transform in **canonical game space: left-handed, +Y up, +Z north, +X east**
/// (see docs/coordinate_systems.md — this is identical to the game's own basis, so the
/// asset-load transform is the identity). Stored as TRS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat, // xyzw
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    pub fn from_translation(t: Vec3) -> Self {
        Self {
            translation: t,
            ..Self::IDENTITY
        }
    }

    /// The 4x4 model matrix (scale, then rotate, then translate) in game space.
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Reference to a model asset by its WAD hash — the geometry + rig this entity renders as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelRef {
    pub model: u32,
}

/// The entity's health — the reimpl's stand-in for the engine's native `RuntimeHealth` component
/// (producer `FUN_004cfed0`).
///
/// **Field order here is ours and does NOT mirror the engine's.** Retail stores `{max, cur}`: read
/// first-hand from the disassembly, `Object.GetMaxHealth` (`0x005CC030`) loads `[rec + 0x00]` and
/// `Object.GetHealth` (`0x005CBDB0`) loads `[rec + 0x04]`. `object_assembly_model.md` records
/// `{cur, max}`, which is backwards. That costs nothing today — nothing memcpys a `RuntimeHealth`
/// blob into this struct — but anyone who later parses the component straight out of a WAD must use
/// the engine order, not this declaration order.
///
/// The destruction system reads [`fraction()`](Health::fraction) to drive each destructible node
/// through its state graph (pristine → damaged/on-fire → wreck), exactly as the game drives it from
/// damage messages against this component; the combat silo's damage applier writes `cur` and posts
/// `DamageMsg`/`DestroyMsg` off [`is_dead()`](Health::is_dead). `RuntimeNodeHealth` (per-node health,
/// for parts that die independently) will hang off the destructible component when we model
/// part-shedding.
///
/// **This is the single definition.** It lives in core rather than in a silo because damage
/// (`mercs2_combat`), destruction, and any health-bearing query are separate leaves that must agree
/// on one component type — a per-crate copy would make an entity damaged by combat invisible to
/// destruction. Being a *character* in the combat sense is "carries a `Health`", so the type is part
/// of the shared vocabulary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Health {
    pub cur: f32,
    pub max: f32,
}

impl Health {
    /// Full health: `cur == max`.
    pub fn new(max: f32) -> Self {
        Health { cur: max, max }
    }
    /// 0.0 = destroyed, 1.0 = full. Clamped; a zero/negative max reads as destroyed.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            0.0
        } else {
            (self.cur / self.max).clamp(0.0, 1.0)
        }
    }
    /// Dead once `cur` reaches zero. Note this is **not** `fraction() == 0.0`: a zero/negative `max`
    /// reads as fraction 0 while `cur` may still be positive, so the two are deliberately distinct.
    pub fn is_dead(&self) -> bool {
        self.cur <= 0.0
    }
}

impl Default for Health {
    fn default() -> Self {
        Health::new(100.0)
    }
}

/// Playback state for a bound animation clip. The animation system advances `time` each fixed
/// tick and samples the clip into a [`SkinPalette`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimState {
    pub clip: u32,
    pub time: f32,
    pub speed: f32,
    /// Crossfade source: the clip that was playing before the last switch. While `blend < 1`
    /// the animation system samples it too and blends toward `clip`.
    pub prev_clip: u32,
    /// Playback time within `prev_clip` (advances and wraps on its own duration during a fade).
    pub prev_time: f32,
    /// Crossfade progress 0..1 (weight of `clip` vs `prev_clip`); 1.0 = no fade active.
    pub blend: f32,
    pub playing: bool,
}

impl Default for AnimState {
    fn default() -> Self {
        Self {
            clip: 0,
            time: 0.0,
            speed: 1.0,
            prev_clip: 0,
            prev_time: 0.0,
            blend: 1.0,
            playing: false,
        }
    }
}

impl AnimState {
    /// A clip that starts playing from t=0 at normal speed.
    pub fn playing(clip: u32) -> Self {
        Self {
            clip,
            playing: true,
            ..Self::default()
        }
    }
}

/// The skinning palette: one bone matrix per bone (row-major, as the shader consumes it). This is
/// the hand-off between the sim spine (which fills it) and the render system (which uploads it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinPalette {
    pub mats: Vec<[[f32; 4]; 4]>,
}

/// Per-entity destruction state — the runtime half of the model's `SWIT` state machine.
///
/// Retail drives destruction as **health → damage *messages* → a per-node state machine →
/// `SHOW`/`HIDE` over HIER subtrees → the node-enable table the draw gate's clause 3 reads**
/// (`docs/reverse_engineer/state_machine_destruction_code_map.md`; `docs/modernization/vehicle_model_spec.md` §5).
/// This component is the state that survives between ticks; `mercs2_destruction` owns the system
/// that advances it, and the render layer consumes [`node_enable`](Destructible::node_enable).
///
/// **`delivered` is monotonic, and that is the whole point.** Deriving the machine's position from
/// the *current* health fraction each tick walks it backwards when health is restored — a shed door
/// would reattach. Retail delivers a message once; the machine only moves forward. Keeping the
/// delivered set here, rather than recomputing it from health, is what makes repair behave.
/// The governing model is **not** stored here — it is read from the entity's [`ModelRef`], so the
/// two can never disagree. An entity with a `Destructible` but no `ModelRef` is simply skipped: it
/// has no geometry to gate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Destructible {
    /// Damage-message hashes delivered so far, deduped. **Never shrinks.**
    pub delivered: Vec<u32>,
    /// Chosen state index per switch node (parallel to the machine's `nodes`).
    pub chosen: Vec<usize>,
    /// Draw-gate clause 3: per-HIER-node enable flags (the engine's `OBJ+0x2a0`).
    pub node_enable: Vec<bool>,
}

impl Destructible {

    /// Record a delivered damage message. Returns `true` if this one is new — i.e. if the machine
    /// may have moved and the enable table needs recomputing.
    pub fn deliver(&mut self, msg: u32) -> bool {
        if self.delivered.contains(&msg) {
            return false;
        }
        self.delivered.push(msg);
        true
    }

    /// Whether HIER node `node` currently draws. Mirrors the engine's clause 3: a negative node
    /// index means "not governed", which always draws; an empty table means the machine has not run
    /// yet, which also draws (a pristine object renders before it is ever damaged).
    pub fn draws(&self, node: i32) -> bool {
        if node < 0 || self.node_enable.is_empty() {
            return true;
        }
        self.node_enable.get(node as usize).copied().unwrap_or(true)
    }
}

// ===== Humanoid =====
//
// The humanoid vocabulary lives in core (not in `mercs2_player`) because every human in the world is
// one of these and only **two** of them are ever player-driven: the retail player container
// (`0x00DF9B90`) holds at most 2 records, and possession is expressed by *adding a component to the
// character entity* (`FUN_006A4060` adds via container `0x00DF9B10`, removes via its vtable `+0x64`),
// never by making the human a facet of a player. So `Human` is the base and [`PlayerControlled`] is
// the annotation — mirroring the engine, and keeping `mercs2_ai` / `mercs2_anim` / `mercs2_combat`
// free of an edge to the player crate (the carve rule: leaf crates depend on core + formats only).
//
// Code map: `docs/reverse_engineer/player_code_map.md` §5; the `Human` Lua surface is the 21-cfunc
// table at VA `0x00B99EF0` (`mercs2_script::bindings::human`).

/// Wildcard for a [`HumanState`] key column — "matches any row value".
///
/// Not a sentinel in the usual sense: `0x27DE_7135` is literally `pandemic_hash_m2("*")`. The retail
/// tables store the hash of an asterisk, so a wildcard column is an ordinary interned name that
/// happens to read `*` (verified 2026-07-26; it fills 1012 of 1020 `AimState` cells in the shipped
/// `ActionTable`). `mercs2_formats::anim_select::NONE_SENTINEL` holds the same number under a less
/// accurate name. Duplicated here because `mercs2_core` deliberately depends on nothing but
/// `hecs` + `glam`.
pub const ANY_STATE: u32 = 0x27DE_7135;

/// Marker: this entity is a humanoid — hero, merc, soldier or civilian alike. Systems that act on
/// people (`mercs2_ai` goals, `mercs2_anim` selection, `mercs2_combat` hit reactions) query for this
/// rather than asking who is driving the entity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Human;

/// Gameplay state of a humanoid: the authoritative source that the animation system's selection key
/// is *derived from*, and the backing store for the 21 `Human.*` cfuncs.
///
/// The three key columns are engine name-hashes (`m2(..)`), not enums, because the retail
/// `ActionTable` matches on hashes and treats [`ANY_STATE`] as a wildcard on either side of the
/// comparison. `mercs2_anim` reads these into its `StateKey`; it does not own them.
///
/// Field provenance — each flag is the state behind a named cfunc in the `Human` table:
/// `SetState` → `stance`/`action`, `Player.SetAimMode` → `aim_state`, `IsSwimming` → `swimming`,
/// `IsCarrying`/`Drop` → `carrying`, `IsGrappling`/`StopGrappling` → `grappling`,
/// `EnableWeapons`/`DisableWeapons` → `weapons_enabled`, `SetFireLock` → `fire_locked`,
/// `Knockdown` → `knocked_down`, `SetPreemptiveRagdoll` → `preemptive_ragdoll`,
/// `SetJostleEnabled` → `jostle_enabled`, `SetAllowCorpseCleanup` → `allow_corpse_cleanup`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanState {
    /// `ActionTable` `Stance` column (e.g. `Upright`). [`ANY_STATE`] = unset.
    pub stance: u32,
    /// `ActionTable` `Action` column (e.g. `Fidget`). [`ANY_STATE`] = unset.
    pub action: u32,
    /// `ActionTable` `AimState` column. [`ANY_STATE`] = unset.
    pub aim_state: u32,
    pub swimming: bool,
    pub carrying: bool,
    pub grappling: bool,
    /// Whether the human may use its weapons at all (`DisableWeapons` is the 2nd-most-called
    /// `Human` cfunc, 27 script call sites).
    pub weapons_enabled: bool,
    /// Locked out of *firing* while still holding a weapon — distinct from `weapons_enabled`.
    pub fire_locked: bool,
    pub knocked_down: bool,
    pub preemptive_ragdoll: bool,
    pub jostle_enabled: bool,
    pub allow_corpse_cleanup: bool,
}

impl Default for HumanState {
    /// An upright human with no action set, weapons usable, not locked, jostling on — the state a
    /// freshly-spawned human occupies before any `Human.*` call touches it.
    fn default() -> Self {
        HumanState {
            stance: ANY_STATE,
            action: ANY_STATE,
            aim_state: ANY_STATE,
            swimming: false,
            carrying: false,
            grappling: false,
            weapons_enabled: true,
            fire_locked: false,
            knocked_down: false,
            preemptive_ragdoll: false,
            jostle_enabled: true,
            allow_corpse_cleanup: true,
        }
    }
}

impl HumanState {
    /// `Human.SetState(guid, stance, action)` — the stance+action setter.
    pub fn set_state(&mut self, stance: u32, action: u32) {
        self.stance = stance;
        self.action = action;
    }

    /// Whether the human can fire right now: it must be allowed weapons *and* not fire-locked, and
    /// a knocked-down human cannot fire. The three are independent in the cfunc surface, so the
    /// conjunction lives here rather than being re-derived at each call site.
    pub fn can_fire(&self) -> bool {
        self.weapons_enabled && !self.fire_locked && !self.knocked_down
    }
}

/// The number of player slots the retail engine supports. **Not a tunable** — the cap is a compile-time
/// constant in **three** independent places (`FUN_006CDAF0` rejects `index > 1`; `FUN_006CDAC0` loops
/// `i < 2`; `FUN_006CD960` rejects local slots `>= 2`), and `Player.GetMaximumPlayers` merely *reports*
/// an unrelated global (`DAT_017C0DD0`) that enforces nothing. Raising the reported maximum does not
/// widen the roster. The `Players` container's own capacity word `0x00DF9B9C` has zero references
/// binary-wide, so nothing bounds it there either (`player_code_map.md` §2.3).
pub const MAX_PLAYERS: usize = 2;

/// Annotation: this humanoid is currently driven by a player rather than by AI.
///
/// **This is a reimpl convenience, not the engine's mechanism.** Retail's possession link is the single
/// field `player+0x20`, written at `0x006A422E` inside `FUN_006A4060` (`player_code_map.md` §5). An
/// earlier revision of this doc claimed `FUN_006A4060` marks the character by adding a component — that
/// reading was **retracted**: the container it touches (`0x00DF9B10`) names itself `CheatInfiniteAmmo`
/// and carries a 1-byte element, and the attach path only visits it to re-apply an *already active*
/// cheat to the new body (with cheats off the branch never runs). Whether the engine marks the
/// character player-driven at all beyond that field is still open (map §9.1).
///
/// So this component is a **denormalization `mercs2_player` maintains** so systems wanting only
/// player-driven humans can widen their query (`(&mut HumanState, &PlayerControlled)`) instead of
/// taking a dependency on the player crate. `mercs2_player` remains the owner of the slot↔character
/// pairing (`player+0x2C` / `player+0x20`); `slot` here is that reverse lookup made cheap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerControlled {
    /// Player slot, `0..MAX_PLAYERS`. Slot 0 is the primary/hero.
    pub slot: u8,
}

impl PlayerControlled {
    /// The primary (slot 0) player — the hero in single-player.
    pub const PRIMARY: Self = PlayerControlled { slot: 0 };

    /// Whether this slot is within the engine's real roster cap ([`MAX_PLAYERS`]).
    pub fn is_valid(&self) -> bool {
        (self.slot as usize) < MAX_PLAYERS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::World;

    /// A fresh human is upright-unset, armed and unlocked — and `can_fire` composes the three
    /// independent gates rather than reading any single flag.
    #[test]
    fn default_human_is_armed_and_able_to_fire() {
        let mut s = HumanState::default();
        assert_eq!(s.stance, ANY_STATE);
        assert!(s.can_fire());

        s.fire_locked = true; // Human.SetFireLock
        assert!(!s.can_fire(), "fire lock alone must stop firing");
        s.fire_locked = false;

        s.weapons_enabled = false; // Human.DisableWeapons
        assert!(!s.can_fire(), "disabled weapons alone must stop firing");
        s.weapons_enabled = true;

        s.knocked_down = true; // Human.Knockdown
        assert!(!s.can_fire(), "knockdown alone must stop firing");
    }

    /// `Human.SetState(guid, stance, action)` writes both key columns and leaves aim alone.
    #[test]
    fn set_state_writes_both_key_columns() {
        const STANCE_UPRIGHT: u32 = 0x12C0_7B18;
        const ACTION_FIDGET: u32 = 0x0C0A_7FA6;
        let mut s = HumanState::default();
        s.set_state(STANCE_UPRIGHT, ACTION_FIDGET);
        assert_eq!((s.stance, s.action), (STANCE_UPRIGHT, ACTION_FIDGET));
        assert_eq!(s.aim_state, ANY_STATE, "SetState must not disturb AimState");
    }

    /// **The architectural invariant.** Humanity is the base; player-control is an annotation, so a
    /// system acting on people sees NPCs and player-driven humans alike, and one that wants only the
    /// latter widens its query — no crate-level relationship between the two.
    #[test]
    fn player_control_is_an_annotation_not_a_subtype() {
        let mut w = World::new();
        let _civilian = w.spawn((Human, HumanState::default()));
        let _soldier = w.spawn((Human, HumanState::default()));
        let hero = w.spawn((Human, HumanState::default(), PlayerControlled::PRIMARY));

        let all_humans = w.query::<(&Human, &HumanState)>().iter().count();
        assert_eq!(all_humans, 3, "every human is visible to a humanoid system");

        let driven: Vec<_> = w
            .query::<(&HumanState, &PlayerControlled)>()
            .iter()
            .map(|(e, (_, p))| (e, p.slot))
            .collect();
        assert_eq!(driven, vec![(hero, 0)], "only the annotated human is player-driven");
    }

    /// Detaching removes the marker (retail: container `0x00DF9B10` vtable `+0x64`) and leaves the
    /// human otherwise intact — the entity does not change type when possession ends.
    #[test]
    fn detach_removes_only_the_marker() {
        let mut w = World::new();
        let e = w.spawn((Human, HumanState::default(), PlayerControlled::PRIMARY));

        w.remove_one::<PlayerControlled>(e).expect("marker present");

        assert!(w.get::<&HumanState>(e).is_ok(), "the human survives detach");
        assert_eq!(w.query::<(&HumanState, &PlayerControlled)>().iter().count(), 0);
        assert_eq!(w.query::<(&Human, &HumanState)>().iter().count(), 1);
    }

    /// The roster cap is 2 and slots are validated against it, not against the reported maximum.
    #[test]
    fn roster_cap_is_two() {
        assert_eq!(MAX_PLAYERS, 2);
        assert!(PlayerControlled { slot: 0 }.is_valid());
        assert!(PlayerControlled { slot: 1 }.is_valid());
        assert!(!PlayerControlled { slot: 2 }.is_valid());
    }

    /// The wildcard must stay numerically identical to the ActionTable none-sentinel that
    /// `mercs2_formats` parses, since core cannot depend on that crate to share the constant.
    #[test]
    fn any_state_matches_the_actiontable_none_sentinel() {
        assert_eq!(ANY_STATE, 0x27DE_7135);
    }
}
