//! `PlayerObject` — the retail per-slot player record (`player_code_map.md` §2.2).
//!
//! A **player is not a character.** The player is a ≥`0x465`-byte controller object living in the ECS
//! component container `0x00DF9B90` (which names itself `Players`), holding the slot index, the
//! viewport id, the attached character GUID, the control source, the reticle target, the boundary
//! callback and the seat/vehicle control locks. Everything *persistent* — cash, fuel, character,
//! upgrade, costume — lives on a different object entirely, the profile/economy singleton
//! ([`crate::profile`]). The two must never be merged: the profile is one global shared by both co-op
//! players, the player object is per-slot.
//!
//! Every field below carries the **offset** and the **VA of the instruction** it was read from, so each
//! row is a one-line re-read against the image.
//!
//! ## Two traps this module encodes deliberately
//!
//! 1. **Some retail bodies dereference twice.** `mov eax,[eax+8]` and *then* `[eax+0x4F5]`: `+0x08` is
//!    a pointer to a *boundary sub-object*, and the out-of-boundary / warning-zone bits live on **that**,
//!    not on the player. Modelled here as the nested [`BoundaryState`], not as player fields — the flat
//!    reading cost the map's own validation pass two rows of its table.
//! 2. **Confidence is per-field, not per-struct.** Rows the map rates **M** or **L** are marked in their
//!    doc comment. Fields whose only consumer is unimplemented (`+0x04`'s list head, `+0x1BC`'s bounded
//!    pass, `+0xBC`'s probe block) are present-and-inert rather than guessed at.

/// Minimum size of the retail player record. The largest field written straight off the resolved
/// object is `+0x464` (`0x005DFEA5: 88 88 64 04 00 00  mov byte [eax+0x464], cl`), so the record is at
/// least `0x465` bytes.
///
/// Carried as a *documented fact*, not as a layout contract — this crate models the record as ordinary
/// Rust fields, never `#[repr(C)]`.
pub const PLAYER_OBJECT_MIN_SIZE: usize = 0x465;

/// The sentinel `player+0x30` holds when the player has not joined. `IsJoined` is `+0x30 != -1`
/// (`FUN_006CDAC0` `0x006CDAD3: cmp dword [eax+0x30], -1`).
pub const NOT_JOINED: i32 = -1;

/// The widget-type hash `player+0x450` is compared against — `pandemic_hash_m2("PDA")`.
/// `0x005BA646: 81 B9 50 04 00 00 4E 75 62 FA  cmp dword [ecx+0x450], 0xfa62754e`.
pub const PDA_WIDGET_TYPE_HASH: u32 = 0xFA62_754E;

/// The out-of-boundary / warning-zone sub-object hanging off `player+0x08`.
///
/// **These are not player fields.** `GetOutBoundary` reaches them as
/// `0x005DC7D6 mov eax,[eax+8]` → `0x005DC7DD mov al,[eax+0x4f5]`; `IsInWarningZone` does the same at
/// `0x005DC8CD`/`0x005DC8D6`, and `IsBoundaryDeath` at `0x005DD0D2`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoundaryState {
    /// `sub+0x4F5` — the player is outside the play-area fence.
    pub out_of_bounds: bool,
    /// `sub+0x4F7` — the player is in the warning band before the fence.
    pub in_warning_zone: bool,
}

/// The reticle hit block (`player+0x11C`/`+0x124`/`+0x12C`), read by `GetTargetUnderReticle`
/// `FUN_005DD6B0`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReticleState {
    /// `+0x11C` — GUID of the object under the reticle, `0` for none. Confidence **H**.
    pub target: u64,
    /// `+0x124` — first word of the hit payload, pushed to Lua as the 2nd return. Confidence **M**
    /// (position is read; meaning is positional).
    pub payload: f32,
    /// `+0x12C` — third word (`0x005DD771 mov ecx,[eax+0x12c]`). Confidence **M**.
    pub payload2: f32,
}

/// The PDA-map-mode sub-object at `player+0x1A8`. Its own fields (`+0x30/34/38/3C/40/44/48/49`) live on
/// **it**, not on the player — reached by `SetPDAMapModeCallback` and `RequestPDAMapModeCancel`
/// (`0x005DB658`). Confidence **M**.
///
/// The spatial arguments are named from the shipped call site, which is the only thing that pins them:
/// `Player.SetPDAMapMode(owner, true, nX, nY + nStartZoom, nZ, nRadius, nStartZoom - nMinZoom,
/// nMaxZoom - nStartZoom, useMinigame)` — `mrxsupportdesignatorsatellite.lua:77`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PdaMapMode {
    /// Whether map mode is currently engaged (arg 2 of `SetPDAMapMode`).
    pub active: bool,
    /// Centre of the map view, world space (args 3–5).
    pub centre: [f32; 3],
    /// View radius (arg 6).
    pub radius: f32,
    /// Zoom range below and above the start zoom (args 7–8).
    pub zoom_below: f32,
    pub zoom_above: f32,
    /// Whether the targeting minigame is in play (arg 9).
    pub minigame: bool,
}

/// The satellite-scan sub-object at `player+0x1AC` (`+0x0C/14/28/2C/34` live on it). Reached by
/// `AddSatelliteScanTarget` and `SetSatelliteScanPaused`. Confidence **M**.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SatelliteScan {
    /// Whether scan mode is engaged (`SetSatelliteScanMode`).
    pub active: bool,
    /// Whether the scan is paused (`SetSatelliteScanPaused`).
    pub paused: bool,
    /// Targets added by `AddSatelliteScanTarget`, in call order.
    pub targets: Vec<u64>,
}

/// The survival-mode pair at `player+0x180`/`+0x198`, written through `FUN_006A2340`. Confidence **L**
/// — the *presence* of a pair is read; which half means what is positional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurvivalState {
    /// `+0x180`.
    pub enabled: bool,
    /// `+0x198`.
    pub secondary: bool,
}

/// The per-player vehicle-disguise sub-struct based at `player+0x3A8`.
///
/// ⚠ **This is only half of "disguise".** `VehicleDisguise` / `GetVehicleDisguiseState` are per-player
/// and reached through a **character** handle; `SetVehicleDisguise` / `GetVehicleDisguise` are a
/// *global* feature gate on a single byte `[0x01176106]` and do no lookup at all. See
/// [`crate::disguise`] — conflating the two is the mistake the map warns about.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerDisguise {
    /// `+0x3A8` — seeded with the attached character GUID on attach
    /// (`0x006A4314: 89 83 A8 03 00 00  mov [ebx+0x3a8], eax`, where `eax = [ebx+0x20]`), then used as
    /// the *base* by `GetVehicleDisguiseState` (`0x005E052B lea edi,[eax+0x3a8]`). Confidence **H**.
    pub base: u64,
    /// `+0x430` / `+0x434`, written by `VehicleDisguise` `FUN_005E02A0`. Confidence **M**.
    pub field_430: u32,
    pub field_434: u32,
    /// `+0x438` **bit 3** — `^= (v << 3 ^ cur) & 8` in `FUN_005E02A0`. Confidence **M**.
    pub flag_bit3: bool,
}

/// The retail player object — the ≥[`PLAYER_OBJECT_MIN_SIZE`]-byte controller record in the `Players`
/// container. One per occupied slot; see [`crate::roster::PlayerRoster`] for how it is resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerObject {
    // ===== Identity =====
    /// `+0x1C` — **the player's own GUID**: the handle every script passes to `Player.*` / `Object.*`.
    /// Returned verbatim by `GetPrimaryPlayer` (`0x005DD8A0`), `GetSecondaryPlayer` (`0x005DD900`),
    /// `GetLocalPlayer` (`0x005DE0B0`), `GetAllPlayers`, `DestroyPlayer` and `TeleportCamera`. **H**.
    pub guid: u64,
    /// `+0x2C` — **the player index (0..1), the roster key.** Matched by `FUN_006CDAF0`
    /// (`cmp [ecx+0x2c], ebp`) and read by `GetPlayerId` (`0x005DDD02`) and `IsLocal`. **H**.
    pub slot: u8,
    /// `+0x28` — local id (`GetLocalId` `0x005DE06C mov edi,[eax+0x28]`). **H**.
    pub local_id: i32,
    /// `+0x30` — **join / viewport id; [`NOT_JOINED`] (`-1`) means not joined.** Counted by
    /// `FUN_006CDAC0`, read by `IsJoined`, `IsRemote`, `GetViewport(Id)` and `FUN_00714230`. **H**.
    pub viewport: i32,
    /// `+0x58` — **remote flag**: 0 = local, non-zero = remote. `IsLocal` (`0x005DDE9E cmp byte
    /// [eax+0x58], bl`) is `+0x30 != -1` **and** `+0x58 == 0`; `IsRemote` (`0x005DDF41`) is
    /// `+0x30 != -1` **and** `+0x58 != 0`. **H**.
    ///
    /// ⚠ The *writer* is still open (map §9.1 S2): `BindToLocal`/`BindToRemote`/`Unbind` all delegate
    /// to SecuROM VM stubs, so which one assigns this is inferred from the three predicates above.
    // CONFIRM-LIVE (§9.1 S2): HW write watchpoint on <playerObj>+0x58, then join a second player.
    pub remote: bool,

    // ===== Possession =====
    /// `+0x20` — **the attached character GUID; this field IS the possession link.** Written directly
    /// at `0x006A422E: 89 43 20  mov [ebx+0x20], eax` and read by `GetCharacter` (`0x005DA870`),
    /// `GetPrimaryCharacter` (`0x005DD960`), `GetSecondaryCharacter` (`0x005DD9E0`),
    /// `GetLocalCharacter`, and both roster ticks. `0` = unpossessed. **H**.
    ///
    /// The engine does *not* mark the character with a component — see
    /// `mercs2_core::PlayerControlled`'s docs for the retraction.
    pub character: u64,
    /// `+0x24` — **control-source GUID**: a `SeatLink` key (container `0x00DF8188`) whose entity carries
    /// a `Controller*` component, i.e. the ridden vehicle; `0` when on foot. `GetSeat` (`0x005DA940`)
    /// returns it to Lua raw; `GetControlledObject` (`0x005DAA20`) uses it as a key and falls back to
    /// [`character`](Self::character) when zero; `GetControlBindingType` (`0x005DD430`) probes six
    /// `Controller*` containers with it; `Object.IsPlayerControlled` (`FUN_005CDFF0`) tests against it.
    /// **H**.
    ///
    /// ⚠ **Only the *clear* is faithful.** `FUN_006A4060` zeroes it on attach (`0x006A4279`); the write
    /// that sets it to a ridden vehicle is not statically reachable (map §9.1 S1 — every candidate was
    /// disassembled and ruled out). The seat/ride system pushes it in via
    /// [`crate::PlayerWorld::set_control_source`], which is a seam, not a recovered call.
    // CONFIRM-LIVE (§9.1 S1): one-shot bp at 0x005DA9F7 to capture playerObj, then a HW write
    // watchpoint on +0x24, then walk into a vehicle.
    pub control_source: u64,

    // ===== Mode / gating =====
    /// `+0x66` — mode/state byte. **Only the comparison `== 4` is known**: `IsBoundaryDeath`
    /// (`0x005DD0C3 cmp byte [esi+0x66], 4`) and the roster tick `FUN_006A1880` (`0x006A1896`) test it.
    /// Confidence **M**.
    ///
    /// The live lead for a name is the Xbox build's player mode machine — `PgPlayerPDAMapMode`,
    /// `PgPlayerBinocularsMode`, `PgPlayerHumanMode`, `PgPlayerEnterSeatMode`, `PgPlayerSeatedMode`
    /// (PPC strings 7507–7533); `PgPlayerPDAMapMode @825666a8` has a decompiled body in
    /// `docs/mercs2-pdb-analysis/gui-hud.md:244`. **No name is invented here.**
    pub mode: PlayerMode,
    /// `+0x1B4` — cinematic-mode **counter** (not a flag): `InCinematicMode` is
    /// `0x005DC146 cmp dword [eax+0x1b4], 0`. **H**.
    pub cinematic_depth: i32,
    /// `+0x244` / `+0x245` — the input-enabled pair (`SetInputEnabled` `0x005DC364` / `0x005DC36A`).
    /// **H**.
    pub input_enabled: bool,
    pub input_enabled_secondary: bool,
    /// `+0x461` — **wait-for-in-game latch. Set to 1 only, never cleared here**
    /// (`0x005DF1C4: C6 80 61 04 00 00 01  mov byte [eax+0x461], 1`). Reached through a **character**
    /// handle. **H**.
    pub wait_for_in_game: bool,
    /// `+0x463` — in-PMC flag (`SetInPmc` `0x005DFD95`). **H**.
    pub in_pmc: bool,
    /// `+0x464` — aim mode (`SetAimMode` `0x005DFEA5`). **H**.
    pub aim_mode: u8,
    /// `+0x158` — grapple-enabled byte (`SetGrappleEnabled` `0x005DFC85`). **H**.
    pub grapple_enabled: bool,
    /// `+0x199` — health-clamp byte (`SetHealthClamp` `0x005DC5C5`). **H**.
    pub health_clamp: bool,
    /// `+0x19C` — scope **refcount** (+1 per enable, −1 per disable), driven by `FUN_006A21E0`, which
    /// also sets `[player+0x1B8]->[+0x10] = 1`. Confidence **M**. Refcounted, not boolean: two enables
    /// need two disables.
    pub scope_refcount: i32,
    /// The survival-mode pair (`+0x180`/`+0x198`). Confidence **L**.
    pub survival: SurvivalState,
    /// `+0x45D`/`+0x45E`/`+0x45F` — **three seat movement locks, each defaulting to 1** when the Lua
    /// argument is absent (`SetSeatMovementLocks` `0x005DD295`–`0x005DD2A1`). **H**.
    pub seat_locks: [bool; 3],
    /// `+0x460` — vehicle controls lock (`SetVehicleControlsLock` `0x005DD3F2`). **H**.
    pub vehicle_controls_lock: bool,
    /// `+0x45C` — tick gate, read by the roster tick `FUN_006A1880` (`0x006A18C2`/`0x006A18CF`).
    /// Confidence **M**.
    pub tick_gate: bool,
    /// `SetSwimmingSearchRadius`'s scalar. Zero shipped call sites; kept so the cfunc has somewhere
    /// real to land rather than a discarded argument.
    pub swim_search_radius: f32,

    // ===== Sub-objects =====
    /// `+0x08` → the boundary sub-object. See [`BoundaryState`] for the double-deref trap.
    pub boundary: BoundaryState,
    /// `+0x11C`/`+0x124`/`+0x12C` — the reticle block.
    pub reticle: ReticleState,
    /// `+0x1A8` → the PDA-map-mode sub-object.
    pub pda_map: PdaMapMode,
    /// `+0x1AC` → the satellite-scan sub-object.
    pub satellite: SatelliteScan,
    /// `+0x3A8`/`+0x430`/`+0x434`/`+0x438` — the per-player disguise sub-struct.
    pub disguise: PlayerDisguise,

    // ===== Handles held for the UI / spawn layers =====
    /// `+0x390` — **this player's PDA widget id**, written by `_GuiInternal.SetPlayerPDAWidget`
    /// (`FUN_005BA500` `0x005BA5E1 mov [ecx+0x390], eax`). **H**.
    pub pda_widget: u64,
    /// `+0x398` — GPS slot, cleared by `ClearGPS` → `FUN_006A0FB0` (which reads `+0x390` first).
    /// Confidence **M**.
    pub gps_slot: u64,
    /// `+0x148` — retry position (`GetRetryPosition` `0x005DF2B7 lea edi,[eax+0x148]`). Confidence
    /// **M**. `None` until a checkpoint sets one, so the binding pushes nil and the shipped
    /// `if not uX` flow stays authentic.
    pub retry_position: Option<[f32; 3]>,
    /// `+0x454` — target-marker field (`GetAllTargetMarkerPos` `0x005DF320 mov edi,[eax+0x454]`).
    /// Confidence **M**; the *list* it heads is not modelled, so this stays empty.
    pub target_markers: Vec<[f32; 3]>,
    /// `+0x450` — widget-type hash, compared against [`PDA_WIDGET_TYPE_HASH`]. **H**.
    pub widget_type: u32,

    // ===== Present-and-inert: read in retail, no implemented consumer =====
    /// `+0x04` — head of a list walked `while (p) p = *(p+8)` in `FUN_006A4060` (`0x006A417E`).
    /// Confidence **L**; the walk's callee `FUN_006A4370` is unimplemented, so this stays 0 rather
    /// than being guessed at.
    pub list_head: u64,
    /// `+0x1BC` — attachment/child count bounding a second pass in `FUN_006A4060` (`0x006A41F7`,
    /// `0x006A42F8`). Confidence **L**; same reasoning as [`list_head`](Self::list_head).
    pub attachment_count: u32,
}

/// The `player+0x66` mode byte. A newtype rather than an enum, because **only one value is known** and
/// inventing names for the rest would be a fabrication ([[no-arbitrary-hashes]] applies to enum
/// variants too).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerMode(pub u8);

impl PlayerMode {
    /// The one value retail compares against: `IsBoundaryDeath` (`0x005DD0C3`) and the roster tick
    /// (`0x006A1896`) both test `byte [player+0x66] == 4`. What it *means* is open; that it gates
    /// boundary death and the tick is read.
    pub const BOUNDARY_DEATH: PlayerMode = PlayerMode(4);
}

impl Default for PlayerObject {
    fn default() -> Self {
        PlayerObject {
            guid: 0,
            slot: 0,
            local_id: 0,
            // Default is *not joined* — a freshly constructed record must not count toward
            // `GetCurrentPlayers`, which is exactly `+0x30 != -1`.
            viewport: NOT_JOINED,
            remote: false,
            character: 0,
            control_source: 0,
            mode: PlayerMode::default(),
            cinematic_depth: 0,
            // Retail's input pair is enabled until a script disables it; the shipped load-state flow
            // (`mrxstate.lua:105,149`) disables then re-enables around loads, so the resting state
            // has to be enabled or the first re-enable is a no-op against a wrong baseline.
            input_enabled: true,
            input_enabled_secondary: true,
            wait_for_in_game: false,
            in_pmc: false,
            aim_mode: 0,
            grapple_enabled: false,
            health_clamp: false,
            scope_refcount: 0,
            survival: SurvivalState::default(),
            // Each lock defaults to 1 in retail when the Lua argument is absent
            // (`SetSeatMovementLocks` 0x005DD295–0x005DD2A1).
            seat_locks: [true; 3],
            vehicle_controls_lock: false,
            tick_gate: true,
            swim_search_radius: 0.0,
            boundary: BoundaryState::default(),
            reticle: ReticleState::default(),
            pda_map: PdaMapMode::default(),
            satellite: SatelliteScan::default(),
            disguise: PlayerDisguise::default(),
            pda_widget: 0,
            gps_slot: 0,
            retry_position: None,
            target_markers: Vec::new(),
            widget_type: 0,
            list_head: 0,
            attachment_count: 0,
        }
    }
}

impl PlayerObject {
    /// A record for `slot`, joined into viewport `slot` and local. This is the shape the boot path
    /// creates: single-player is slot 0, viewport 0, `remote == false`.
    pub fn joined_local(slot: u8, guid: u64) -> Self {
        PlayerObject { guid, slot, local_id: slot as i32, viewport: slot as i32, ..Default::default() }
    }

    /// `Player.IsJoined` — `+0x30 != -1` (`FUN_006CDAC0` `0x006CDAD3`).
    pub fn is_joined(&self) -> bool {
        self.viewport != NOT_JOINED
    }

    /// `Player.IsLocal` — joined **and** `+0x58 == 0` (`0x005DDE9E`).
    pub fn is_local(&self) -> bool {
        self.is_joined() && !self.remote
    }

    /// `Player.IsRemote` — joined **and** `+0x58 != 0` (`0x005DDF41`).
    ///
    /// Note this is deliberately **not** `!is_local()`: an unjoined player is neither local nor
    /// remote. An earlier revision computed `IsRemote` as `!IsLocal`, which made every unknown GUID
    /// in the game answer `true`.
    pub fn is_remote(&self) -> bool {
        self.is_joined() && self.remote
    }

    /// `Player.InCinematicMode` — the `+0x1B4` counter is non-zero (`0x005DC146`).
    pub fn in_cinematic_mode(&self) -> bool {
        self.cinematic_depth != 0
    }

    /// `Player.GetControlledObject` — `+0x24` if set, else fall back to the character `+0x20`
    /// (`0x005DAB3B mov edi,[eax+0x20]`).
    pub fn controlled_object(&self) -> u64 {
        if self.control_source != 0 {
            self.control_source
        } else {
            self.character
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recovered minimum size, and the two defaults that are load-bearing rather than cosmetic.
    #[test]
    fn recovered_size_and_defaults() {
        assert_eq!(PLAYER_OBJECT_MIN_SIZE, 0x465, "largest field written off the object is +0x464");
        let p = PlayerObject::default();
        assert_eq!(p.viewport, NOT_JOINED, "a fresh record must not count as joined");
        assert!(!p.is_joined());
        assert_eq!(p.seat_locks, [true; 3], "each seat lock defaults to 1 in retail");
    }

    /// `IsLocal`/`IsRemote` are **both** conjunctions with joined-ness, so an unjoined player is
    /// neither. Computing `IsRemote` as `!IsLocal` (what an earlier revision did) makes every unjoined
    /// or unknown handle report remote.
    #[test]
    fn local_and_remote_are_both_gated_on_joined() {
        let mut p = PlayerObject::default();
        assert!(!p.is_local(), "unjoined is not local");
        assert!(!p.is_remote(), "unjoined is not remote either");
        assert_ne!(p.is_remote(), !p.is_local(), "IsRemote must not be the negation of IsLocal");

        p.viewport = 0;
        assert!(p.is_local());
        assert!(!p.is_remote());
        p.remote = true;
        assert!(!p.is_local());
        assert!(p.is_remote());
    }

    /// Cinematic mode is a **counter**, so nested enters need matching exits — a boolean model would
    /// let the inner exit cancel the outer enter.
    #[test]
    fn cinematic_mode_is_a_counter_not_a_flag() {
        let mut p = PlayerObject::default();
        assert!(!p.in_cinematic_mode());
        p.cinematic_depth += 1;
        p.cinematic_depth += 1;
        assert!(p.in_cinematic_mode());
        p.cinematic_depth -= 1;
        assert!(p.in_cinematic_mode(), "one exit must not clear a doubly-entered cinematic");
        p.cinematic_depth -= 1;
        assert!(!p.in_cinematic_mode());
    }

    /// `GetControlledObject` falls back to the character when the control source is clear — the
    /// on-foot case, which is also the only case a faithful reimpl can currently produce (§9.1 S1).
    #[test]
    fn controlled_object_falls_back_to_the_character() {
        let mut p = PlayerObject { character: 0x1234, ..Default::default() };
        assert_eq!(p.controlled_object(), 0x1234, "on foot, the controlled object is the character");
        p.control_source = 0xBEEF;
        assert_eq!(p.controlled_object(), 0xBEEF, "riding, it is the SeatLink key");
    }

    /// The PDA widget-type constant is the map's own cross-check: it must equal the engine hash of
    /// `"PDA"`, so a wrong hash implementation fails here rather than silently mismatching at runtime.
    #[test]
    fn pda_widget_hash_is_the_engine_hash_of_pda() {
        assert_eq!(PDA_WIDGET_TYPE_HASH, mercs2_formats::hash::pandemic_hash_m2("PDA"));
    }
}
