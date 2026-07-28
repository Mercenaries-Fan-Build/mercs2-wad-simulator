//! `mercs2_player` — the player concern: the per-slot player object, the persistent profile/economy
//! singleton, possession, and the disguise feature gate.
//!
//! **Silo 17** (`docs/modernization/reimplementation_parallelization_plan.md` §3 lists 16 silos and
//! routes `Player` cross-cutting; `wave0_seam_review.md:40` seam G overrode that and gave the namespace
//! its own crate — `Player` is the 2nd-highest-traffic Lua namespace at 107 cfuncs and spans economy +
//! player-controller, so it does not fold into vehicle or faction).
//! **Scoreboard row(s):** cross-cutting — there is no player row in the 32-row scoreboard.
//! **Code map:** `docs/reverse_engineer/player_code_map.md` (all 107 cfunc bodies read; §10 is a
//! requirements list written for this crate), with `save_serialize_code_map.md` owning the `.profile`
//! disk layout and `camera_code_map.md` the viewport arrays.
//! **Owned Lua namespace(s):** `Player` (107 cfuncs, `luaL_Reg` table `0x00B98FC0`).
//! **Not owned:** `Human.Inventory` — `inventory_equipment_code_map.md` §10 settles it with
//! `mercs2_combat`, because the state lives on the *character* entity and the player object carries no
//! weapon field at all. This crate owns possession and the profile, and reaches weapons through the
//! character GUID.
//!
//! # The one thing to understand first
//!
//! **A player is not a character, and there are two objects, not one.**
//!
//! | | [`PlayerObject`] | [`PlayerProfile`] |
//! |---|---|---|
//! | retail | the ≥`0x465`-byte record in the `Players` container `0x00DF9B90` | the singleton `[0x01176054]` |
//! | cardinality | one per occupied slot, **≤2** | exactly **one**, shared by the co-op pair |
//! | holds | slot, viewport, character link, control source, reticle, locks, modes | cash, fuel, capacity, character, upgrade, costume |
//! | lifetime | the session | persisted to `.profile` |
//!
//! Merging them is the single most consequential mistake available here: cash is not per-player, and a
//! viewport is not persistent.
//!
//! **Possession is the field [`PlayerObject::character`]** (`player+0x20`, written at `0x006A422E`) — not
//! a component added to the character entity. See [`possession`]'s module docs for the retracted reading.
//!
//! # Modules
//!
//! * [`object`] — [`PlayerObject`] and its sub-structs, every field carrying its offset and the VA of the
//!   instruction it was read from.
//! * [`roster`] — the `Players` container as a **scan, not an array**, plus the four independent player
//!   counts retail reports.
//! * [`profile`] — the profile/economy singleton, including the five setters that never arm the autosave.
//! * [`possession`] — `FUN_006A4060`: the possession write, the control-source clear, the disguise seed,
//!   and the cheat gate named by hash.
//! * [`boundary`] — the play-area fence, and the fact that mutating it is **server-authoritative**.
//! * [`pda`] — PDA map mode (nine arguments, not one) and satellite scan.
//! * [`disguise`] — the **two** disguise mechanisms wearing four similar names.
//! * [`callbacks`] — the registry that actually retains a Lua callback, replacing the eight
//!   registrations whose closures are currently destroyed at registration time.
//!
//! # Faithful quirks, deliberately preserved
//!
//! These look like bugs because they *are* bugs — shipped ones. Reproducing them is the fidelity bar;
//! each is logged in `DEFERRED.md` as `[faithful-blocker: no]` so the fix is queued, not smuggled in.
//!
//! * Five profile setters never OR the dirty flag, so changing cash / fuel capacity / character /
//!   costume / the costume roster **alone** leaves the profile un-autosaved
//!   ([`profile::NON_DIRTYING_SETTERS`]).
//! * `SetCash`/`SetFuel` accept an undocumented second boolean that suppresses the write entirely.
//! * [`roster::CURRENT_LOCAL_PLAYERS_CONST`] — `GetCurrentLocalPlayers` always returns `1.0`.
//!   Implementing it honestly diverges from retail on the split-screen path.
//! * [`roster::ANY_CHARACTER_SENTINEL`] — `GetAnyCharacter` does no lookup; it pushes a constant.
//! * There is **no** native 1-billion cash clamp and **no** native fuel-to-capacity clamp. Both are Lua
//!   soft-clamps in `MrxPmc`, and shipped scripts bypass them.

pub mod boundary;
pub mod callbacks;
pub mod components;
pub mod disguise;
pub mod locomotion;
pub mod object;
pub mod pda;
pub mod possession;
pub mod profile;
pub mod roster;
pub mod system;

pub use boundary::{Boundary, BoundarySet, NetAuthority};
pub use callbacks::{CallbackArg, CallbackFire, CallbackId, CallbackRegistry, CallbackSlot};
pub use components::{ControllerPlayer, GrappleParameters, ModelMixerProfile, VehicleDisguiseScale, CONTROL_BINDING_TYPES};
pub use disguise::DisguiseRequest;
pub use locomotion::{LocomotionInput, PlayerController, CLIP_IDLE, CLIP_RUN, CLIP_WALK, RUN_SPEED, WALK_SPEED};
pub use object::{
    BoundaryState, PdaMapMode, PlayerDisguise, PlayerMode, PlayerObject, ReticleState, SatelliteScan,
    SurvivalState, NOT_JOINED, PDA_WIDGET_TYPE_HASH, PLAYER_OBJECT_MIN_SIZE,
};
pub use pda::PdaMapModeRequest;
pub use possession::{CheatFlags, CHEAT_FLAG_HASHES};
pub use profile::{PlayerProfile, NON_DIRTYING_SETTERS};
pub use system::{is_boundary_death_due, player_roster_system};
pub use roster::{
    PlayerRoster, ANY_CHARACTER_SENTINEL, CURRENT_LOCAL_PLAYERS_CONST, MAX_LOCAL_PLAYERS_CONST,
    PLAYERS_CONTAINER_HASH, PLAYERS_CONTAINER_VA, REPORTED_MAX_PLAYERS, ROSTER_CAP,
};

/// The global vehicle-disguise feature gate — the single byte `[0x01176106]`.
///
/// ⚠ **"Disguise" is two mechanisms wearing four similar names**, and this is the *first*:
/// `Set/GetVehicleDisguise` do **no lookup at all** and read/write this byte
/// (`0x005E0100 mov byte [0x1176106], dl`; `0x005E0131 mov bl, byte [0x1176106]`). It is also read by
/// `Object.IsDisguised` (`FUN_005CEF20`), and the Lua guards on it —
/// `if not Player.GetVehicleDisguise() then return end` (`wiftutorialvehicledisguise.lua:26`).
/// So it is a **global gate for the whole disguise system**, not a per-player setting.
///
/// The *second* mechanism is per-player and reached through a **character** handle:
/// `VehicleDisguise` / `GetVehicleDisguiseState`, whose state is [`PlayerDisguise`] on the player
/// record. Conflating the two is the mistake the code map explicitly warns about.
pub const DISGUISE_GATE_VA: u32 = 0x0117_6106;

/// The player concern's runtime state: the ≤2-slot roster, the one profile singleton, and the globals
/// that are neither.
///
/// This is the facade the engine's script host owns and the `Player.*` binding bodies call. Method names
/// track their cfunc so the mapping is mechanical — the same convention `mercs2_audio::AudioEngine`
/// uses.
///
/// The **script host** owns this, not `GameplaySystems`, because Lua is its primary driver; the tick
/// reaches it by reference.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerWorld {
    /// The `Players` container.
    pub roster: PlayerRoster,
    /// The one profile/economy singleton.
    pub profile: PlayerProfile,
    /// The global disguise feature gate, `[0x01176106]` (see [`DISGUISE_GATE_VA`]).
    disguise_gate: bool,
    /// Config flags the attach path reads (`FUN_004C2C20` publishes these).
    pub cheats: CheatFlags,
    /// The play-area fence, per player slot. Indexed by slot rather than held on [`PlayerObject`] so the
    /// server-authority check has one place to live.
    pub boundaries: [BoundarySet; ROSTER_CAP],
    /// The retained-callback registry. The binding layer holds the `mlua::Function`s and maps them by
    /// [`CallbackId`]; this side only knows ids.
    pub callbacks: CallbackRegistry,
    /// The net-session role, published each frame from the session object. Boundary mutations early-out
    /// when this is [`NetAuthority::Client`].
    pub net: NetAuthority,
    /// `Player.GetPlayerStart` returns the **name** `"PlayerLocation_Start"`, not a transform — the
    /// engine does not resolve the spawn point, Lua does via `Pg.GetGuidByName`.
    ///
    /// ⚠ And it is only the **fallback**: `mrxplayer.lua:185-187` overrides it with
    /// `_tSpawnLocations[iPlayerId+1]` whenever that table is set, so the override is the authority.
    /// `SetPlayerStart` writes here.
    player_start: String,
    /// Queued `Player.TeleportCamera(uPlayer)` requests, drained by the engine's camera update via
    /// [`take_camera_teleports`](Self::take_camera_teleports).
    ///
    /// This is the **only** `Player` verb that still queues an intent rather than mutating state — and
    /// unlike the 24 verbs it replaces, it has a public accessor and a real consumer. Follows the
    /// existing `take_hero_teleport` convention on the script host.
    camera_teleports: Vec<u64>,
}

/// The literal `Player.GetPlayerStart` (`FUN_005DEC60`, 43 bytes) pushes:
/// `0x005DEC77 push 0x00D28A90` — the string `"PlayerLocation_Start"` — then `call 0x0085D9F0`.
pub const DEFAULT_PLAYER_START: &str = "PlayerLocation_Start";

impl PlayerWorld {
    /// An empty world: no players joined, a zeroed profile, disguise gate off.
    pub fn new() -> Self {
        PlayerWorld { player_start: DEFAULT_PLAYER_START.to_string(), ..Default::default() }
    }

    /// The single-player boot state: one joined local player in slot 0.
    pub fn single_player() -> Self {
        PlayerWorld { roster: PlayerRoster::single_player(), ..PlayerWorld::new() }
    }

    // ===== Disguise gate (§7) =====

    /// `Player.GetVehicleDisguise()` — reads the global gate byte. **Zero arguments** in the shipped
    /// call sites (`mrxguihudvehicledisguise.lua:130-133,158`), and the result is used as a plain
    /// boolean.
    pub fn vehicle_disguise_gate(&self) -> bool {
        self.disguise_gate
    }

    /// `Player.SetVehicleDisguise(bEnable)` — writes the global gate byte. Takes a bare bool
    /// (`mrxmissionflow.lua:733`), *not* a player handle.
    pub fn set_vehicle_disguise_gate(&mut self, on: bool) {
        self.disguise_gate = on;
    }

    // ===== Player start (§3.1, §8) =====

    /// `Player.GetPlayerStart()` — the spawn-point **name**. See [`player_start`](Self::player_start)'s
    /// field docs for why this is a fallback rather than the authority.
    pub fn player_start(&self) -> &str {
        &self.player_start
    }

    /// `Player.SetPlayerStart(name)` — override the spawn-point name.
    pub fn set_player_start(&mut self, name: impl Into<String>) {
        self.player_start = name.into();
    }

    // ===== The one intent queue =====

    /// `Player.TeleportCamera(uPlayer)` — queue a camera teleport for the engine to consume.
    pub fn teleport_camera(&mut self, player: u64) {
        self.camera_teleports.push(player);
    }

    /// Drain the queued camera teleports. Called by the engine's camera update once per tick.
    pub fn take_camera_teleports(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.camera_teleports)
    }

    // ===== The seat/ride seam (§9.1 S1) =====

    /// Set a player's control source to a `SeatLink` GUID — **a seam, not a recovered call.**
    ///
    /// The retail write that populates `player+0x24` is not statically reachable: every function that
    /// touches a `Players` or `SeatLink` global was disassembled and every `mov [reg+0x24]` hit turned
    /// out to be a local argument struct, a generic list-insert, or an unrelated ctor. What *is* known is
    /// that `+0x24` holds a `SeatLink` key whose entity carries a `Controller*` component, so the writer
    /// lives in the seat/ride subsystem.
    ///
    /// So the vehicle silo calls this on enter/exit rather than this crate depending on
    /// `mercs2_vehicle` — the carve rule forbids that edge, and an `EventBus` message would work equally
    /// well if a queue is ever wanted.
    ///
    /// Returns `false` if the slot resolves to no record.
    // CONFIRM-LIVE (§9.1 S1): capture playerObj via a one-shot bp at 0x005DA9F7 (GetSeat's
    // `mov eax,[eax+0x24]` — cold, safe), then a HW write watchpoint on +0x24, then walk into a vehicle.
    pub fn set_control_source(&mut self, slot: u32, seat_link: u64) -> bool {
        match self.roster.get_mut(slot) {
            Some(p) => {
                p.control_source = seat_link;
                true
            }
            None => false,
        }
    }

    /// `Object.IsPlayerControlled(guid)` — retail `FUN_005CDFF0` tests the queried guid against
    /// `player+0x24`, and the shipped Lua reads the result as a **handle**, not a boolean:
    /// `local uPlayer = Object.IsPlayerControlled(uDriver)` (`mrxhijack.lua:504`, `mrxvehicle.lua:565`),
    /// then feeds it straight into `Player.SetInputEnabled(...)`.
    ///
    /// Returns the controlling player's GUID, or `0` for none (which the binding pushes as nil).
    pub fn player_for_controlled_object(&self, guid: u64) -> u64 {
        self.roster
            .iter()
            .find(|p| p.controlled_object() == guid && guid != 0)
            .map(|p| p.guid)
            .unwrap_or(0)
    }

    /// Resolve [`ANY_CHARACTER_SENTINEL`] to a concrete character.
    ///
    /// `GetAnyCharacter` pushes the sentinel without looking anything up, and 223 shipped call sites
    /// feed it straight into `Object.*` / `Human.*`. Those namespaces are where the resolve happens, so
    /// this is the function they call — returning the local player's character, else the primary's,
    /// else `0`.
    pub fn resolve_any_character(&self) -> u64 {
        self.roster.local().or_else(|| self.roster.primary()).map(|p| p.character).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Deliverable 1, test 1: the two objects are independent ----
    /// Cash lives on the one profile, not per player — so it is visible identically from either slot,
    /// and creating or destroying a player cannot change it.
    #[test]
    fn the_profile_is_global_and_the_roster_is_per_slot() {
        let mut w = PlayerWorld::single_player();
        w.profile.set_cash(75_000, false);
        // Slot 1 joins as a second local player (create allocates; bind joins).
        w.roster.create(1);
        possession::bind_to_local(&mut w.roster, 1, 1);

        assert_eq!(w.roster.current_players(), 2);
        assert_eq!(w.profile.cash, 75_000, "a second player does not get a second wallet");

        w.roster.get_mut(1).unwrap().character = 0x99;
        assert_eq!(w.profile.cash, 75_000, "per-slot state cannot touch the profile");

        w.roster.destroy(1);
        assert_eq!(w.roster.current_players(), 1);
        assert_eq!(w.profile.cash, 75_000, "and losing a player does not lose the money");
    }

    // ---- Deliverable 1, test 2: the disguise gate is global, the per-player state is not ----
    /// The two disguise mechanisms are independent: flipping the global gate does not clear a player's
    /// disguise, and a player's disguise does not flip the gate.
    #[test]
    fn the_disguise_gate_and_per_player_disguise_are_independent() {
        let mut w = PlayerWorld::single_player();
        w.set_vehicle_disguise_gate(true);
        w.roster.get_mut(0).unwrap().disguise.flag_bit3 = true;

        w.set_vehicle_disguise_gate(false);
        assert!(
            w.roster.get(0).unwrap().disguise.flag_bit3,
            "the global gate must not clear per-player disguise state"
        );

        w.roster.get_mut(0).unwrap().disguise.flag_bit3 = false;
        assert!(!w.vehicle_disguise_gate(), "and per-player state must not flip the global gate");
    }

    // ---- Deliverable 1, test 3: the camera-teleport queue is drained, not accumulated ----
    /// The one surviving intent queue has a public accessor and empties on drain — the property the
    /// 24 `record_all` verbs it replaces did not have.
    #[test]
    fn the_camera_teleport_queue_drains() {
        let mut w = PlayerWorld::single_player();
        assert!(w.take_camera_teleports().is_empty());

        w.teleport_camera(0x2);
        w.teleport_camera(0x3);
        assert_eq!(w.take_camera_teleports(), vec![0x2, 0x3]);
        assert!(w.take_camera_teleports().is_empty(), "draining empties the queue");
    }

    // ---- Deliverable 1, test 4: the sentinel resolves ----
    /// `GetAnyCharacter`'s constant must resolve to the local player's character, or its 223 call sites
    /// break. An empty roster resolves to 0 (→ nil) rather than panicking.
    #[test]
    fn the_any_character_sentinel_resolves_to_the_local_character() {
        let mut w = PlayerWorld::new();
        assert_eq!(w.resolve_any_character(), 0, "no players -> nil, not a panic");

        w.roster = PlayerRoster::single_player();
        assert_eq!(w.resolve_any_character(), 0, "joined but unpossessed -> still nil");

        possession::attach_to_character(&mut w.roster, 0, 0xC0FFEE, CheatFlags::default());
        assert_eq!(w.resolve_any_character(), 0xC0FFEE);
    }

    // ---- Deliverable 1, test 5: IsPlayerControlled answers with a handle ----
    /// The shipped Lua uses the *result* as a player handle, so a bool-typed binding turns
    /// `SetInputEnabled(Object.IsPlayerControlled(u), false)` into `SetInputEnabled(true, false)`.
    #[test]
    fn player_for_controlled_object_returns_a_handle_not_a_flag() {
        let mut w = PlayerWorld::single_player();
        possession::attach_to_character(&mut w.roster, 0, 0xC0FFEE, CheatFlags::default());

        let player_guid = w.roster.get(0).unwrap().guid;
        assert_eq!(w.player_for_controlled_object(0xC0FFEE), player_guid, "on foot: the character");
        assert_eq!(w.player_for_controlled_object(0xBAD), 0, "an unrelated guid -> 0 -> nil");
        assert_eq!(w.player_for_controlled_object(0), 0, "guid 0 must never match");

        // Riding: the control source answers instead, and the character no longer does.
        w.set_control_source(0, 0xCAB);
        assert_eq!(w.player_for_controlled_object(0xCAB), player_guid);
        assert_eq!(w.player_for_controlled_object(0xC0FFEE), 0);
    }

    // ---- Deliverable 1, test 6: the player-start literal ----
    /// `GetPlayerStart` returns a name string, not a transform — and the reimpl must model the Lua
    /// override rather than hardcoding the literal.
    #[test]
    fn player_start_is_a_name_and_is_overridable() {
        let mut w = PlayerWorld::new();
        assert_eq!(w.player_start(), "PlayerLocation_Start", "the retail literal");
        w.set_player_start("PlayerLocation_Mission3");
        assert_eq!(w.player_start(), "PlayerLocation_Mission3", "Lua's _tSpawnLocations wins");
    }

    /// The scaffold's dependency edge still resolves. Kept from the scaffold, as `mercs2_audio` keeps
    /// its own.
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
    }
}

/// The `README.md` usage example, compiled and run so the documentation cannot drift from the API.
///
/// Keep this in sync with the README's `Usage` block — a doc example that does not compile is worse
/// than none, because a reader trusts it.
#[cfg(test)]
mod readme_example {
    use crate::{possession, CheatFlags, PlayerWorld};

    #[test]
    fn the_readme_usage_example_compiles_and_holds() {
        let mut w = PlayerWorld::single_player();
        possession::attach_to_character(&mut w.roster, 0, 0xC0FFEE, CheatFlags::default());

        assert_eq!(w.roster.get(0).unwrap().character, 0xC0FFEE);
        let player_guid = w.roster.get(0).unwrap().guid;
        assert_eq!(w.roster.by_character(0xC0FFEE).map(|p| p.guid), Some(player_guid));

        w.profile.set_cash(75_000, false);
        w.roster.create(1);
        assert_eq!(w.profile.cash, 75_000);

        assert!(!w.profile.autosave_due());
    }
}
