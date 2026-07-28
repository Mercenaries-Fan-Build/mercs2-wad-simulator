//! `PlayerRoster` — the `Players` container and the accessors retail resolves slots through
//! (`player_code_map.md` §2.1, §2.3).
//!
//! **The roster is a scan, not an array.** Retail resolves a slot by linear-scanning the container and
//! matching `player+0x2C`, bounded by the live count `DAT_00DF9BA8` — it is not indexed by slot.
//! Modelling it as `[Option<PlayerObject>; 2]` would silently give the reimpl a shape the engine does
//! not have, and would also imply a capacity: the container's own capacity word `0x00DF9B9C` has **zero
//! references binary-wide**, so nothing bounds it there.
//!
//! **Three player counts, three independent answers.** This is the trap the map calls out (§2.3), and
//! it is the reason each has its own constant here rather than one shared number:
//!
//! | question | retail | answer |
//! |---|---|---|
//! | how many slots can the roster hold? | three compile-time immediates | [`ROSTER_CAP`] |
//! | what does `GetMaximumPlayers` report? | `DAT_017C0DD0`, enforced by nothing | [`REPORTED_MAX_PLAYERS`] |
//! | what does `GetMaximumLocalPlayers` report? | an `.rdata` float constant | [`MAX_LOCAL_PLAYERS_CONST`] |
//! | what does `GetCurrentLocalPlayers` report? | a *different* `.rdata` float, always | [`CURRENT_LOCAL_PLAYERS_CONST`] |
//! | how many players are actually joined? | a count of `+0x30 != -1` | [`PlayerRoster::current_players`] |

use crate::object::{PlayerObject, NOT_JOINED};

/// The `Players` component container `0x00DF9B90` (ctor `FUN_00A7C7D0`, vtable `PTR_FUN_00BC3FB8`).
///
/// It **names itself**: `[[0x00DF9B90]+0x34] = FUN_00647BA0`, whose body is `B8 <ptr> C3` → `"Players"`.
/// That `vtable+0x34` self-naming mechanism is how every container in this crate's docs was named,
/// rather than inferred from its registrar.
pub const PLAYERS_CONTAINER_VA: u32 = 0x00DF_9B90;

/// `pandemic_hash_m2("Players")` — the container's type hash.
///
/// ⚠ **Derived here, not quoted from the map.** `player_code_map.md` names the container (via the
/// `vtable+0x34` self-naming key) but never states a type hash for it; this is that name run through
/// the engine hash. The derivation is legitimate — the inventory map does state `RuntimeInventory`'s
/// hash and it matches the same computation — but it is inference, not a recovered constant. The test
/// below pins it against the hash function so a wrong hash impl fails loudly.
pub const PLAYERS_CONTAINER_HASH: u32 = 0x451C_2119;

/// The roster capacity. **Three** compile-time immediates in retail, all independent of any global:
/// `FUN_006CDAF0` rejects `index > 1` (`0x006CDAF0: cmp dword [esp+4],1 / ja`), `FUN_006CDAC0` loops
/// `i < 2` (`0x006CDADF: cmp esi,2 / jl`), and `FUN_006CD960` rejects local slots `>= 2`.
///
/// Deliberately **not** a read of `DAT_017C0DD0` — see [`REPORTED_MAX_PLAYERS`].
pub const ROSTER_CAP: usize = mercs2_core::MAX_PLAYERS;

/// What `Player.GetMaximumPlayers` (`FUN_005DDA60`) reports: it pushes `DAT_017C0DD0` verbatim, which
/// is `2` in the dump. **Nothing enforces it** — raising it does not widen the roster, because the cap
/// is the three immediates behind [`ROSTER_CAP`].
pub const REPORTED_MAX_PLAYERS: i64 = 2;

/// What `Player.GetMaximumLocalPlayers` (`FUN_005DDF90`) reports: an `.rdata` constant, not a query —
/// `0x005DDFAA: F3 0F 10 05 74 28 B9 00  movss xmm0, [0x00B92874]`, and `[0x00B92874] = 2.0f`.
pub const MAX_LOCAL_PLAYERS_CONST: f32 = 2.0;

/// What `Player.GetCurrentLocalPlayers` (`FUN_005DDFD0`) reports: `0x005DDFEA movss xmm0, [0x00B9B664]`
/// where `[0x00B9B664] = 1.0f`. **It always returns 1.0, regardless of actual state.**
///
/// This is the dangerous one (map §10.10): implementing it *honestly* — counting local players —
/// diverges from retail on the split-screen path. Faithful means returning the constant.
pub const CURRENT_LOCAL_PLAYERS_CONST: f32 = 1.0;

/// `Player.GetAnyCharacter` (`FUN_005DE260`) performs **no lookup at all**: it pushes this constant as
/// lightuserdata (`0x005DE27A: C7 00 00 00 00 F0  mov dword [eax], 0xf0000000` /
/// `0x005DE280: mov dword [eax+4], 2`).
///
/// It is a **sentinel** meaning "whichever character", which downstream `Object.*` / `Human.*` calls
/// resolve. With 223 call sites it is the single most-used `Player` binding in the game, and modelling
/// it as a real query is wrong.
///
/// Sits far above `mercs2_core::FIRST_DYNAMIC_GUID` (`0x1000_0000`), so it can never collide with a
/// minted GUID. The resolve to a concrete character happens in the **engine host**
/// (`mercs2_engine::script_host::GameScriptHost::resolve_guid`), not in `GuidMap` — `GuidMap` has no
/// knowledge of this sentinel.
pub const ANY_CHARACTER_SENTINEL: u64 = 0xF000_0000;

/// The `Players` container: the occupied player records plus the slot/guid/character resolves retail
/// performs against them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerRoster {
    /// The **dense** container records — deliberately not slot-indexed, because retail scans and
    /// matches `+0x2C`. Order here is insertion order, exactly as the container's dense region is
    /// walked; nothing may assume `records[i].slot == i`.
    records: Vec<PlayerObject>,
}

impl PlayerRoster {
    /// An empty roster — no player has joined yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The single-player boot roster: one joined local player in slot 0 carrying
    /// `mercs2_core::LOCAL_PLAYER_GUID`.
    pub fn single_player() -> Self {
        PlayerRoster {
            records: vec![PlayerObject::joined_local(0, mercs2_core::LOCAL_PLAYER_GUID)],
        }
    }

    /// `Player.GetPlayer(i)` / the internal by-index accessor `FUN_006CDAF0` — **241 call sites**.
    ///
    /// The cap is checked **first, before the scan** (`0x006CDAF0: cmp dword [esp+4],1 / 77 ja`), so an
    /// out-of-range slot is rejected without touching the container. Then the records are scanned for
    /// a matching `+0x2C`.
    pub fn get(&self, slot: u32) -> Option<&PlayerObject> {
        if slot as usize >= ROSTER_CAP {
            return None;
        }
        self.records.iter().find(|p| u32::from(p.slot) == slot)
    }

    /// Mutable [`get`](Self::get), same cap-before-scan order.
    pub fn get_mut(&mut self, slot: u32) -> Option<&mut PlayerObject> {
        if slot as usize >= ROSTER_CAP {
            return None;
        }
        self.records.iter_mut().find(|p| u32::from(p.slot) == slot)
    }

    /// `Player.GetCurrentPlayers` — `FUN_006CDAC0`. Loops `0..2` **independently of
    /// [`get`](Self::get)'s cap** and counts records whose `+0x30 != -1`.
    pub fn current_players(&self) -> i64 {
        self.records.iter().filter(|p| p.viewport != NOT_JOINED).count() as i64
    }

    /// `FUN_006CD960` — local slot → player index, used by `GetLocalPlayer` / `GetLocalCharacter`.
    /// Rejects local slots `>= 2` with `-1` (`0x006CD961: cmp dword [esp+8],2 / 7C jl`), which is the
    /// third of the three compile-time caps.
    pub fn index_for_local(&self, local: i32) -> i32 {
        if local < 0 || local as usize >= ROSTER_CAP {
            return -1;
        }
        self.records
            .iter()
            .find(|p| p.is_local() && p.local_id == local)
            .map(|p| i32::from(p.slot))
            .unwrap_or(-1)
    }

    /// Resolve the handle scripts pass around (`+0x1C`).
    ///
    /// A miss is `None`, and the binding layer must push **nil without raising** — `FUN_004B2A50` is
    /// literally `push nil; return 1`, and shipped scripts rely on `if Player.X(u) then`, so a reimpl
    /// that errors on a bad handle breaks working Lua.
    pub fn by_guid(&self, guid: u64) -> Option<&PlayerObject> {
        self.records.iter().find(|p| p.guid == guid)
    }

    /// Mutable [`by_guid`](Self::by_guid).
    pub fn by_guid_mut(&mut self, guid: u64) -> Option<&mut PlayerObject> {
        self.records.iter_mut().find(|p| p.guid == guid)
    }

    /// `FUN_006CDB70` = `GetPlayerForCharacter(charGuid)` — scan for `+0x20 == character`.
    ///
    /// **Confidence M**, and the reason matters: the retail body is a SecuROM VM stub
    /// (`jmp [0x0245F8CC]` → `push 0x24e8bda; call 0x1aaff10`), so it is named *behaviourally* — one
    /// register argument, the returned object's fields are all independently-established player fields,
    /// and the Lua splits 6/6 on handle type. Raising M→H needs the SecuROM recovery pipeline or a live
    /// `EAX` compare at `0x005E0527`.
    ///
    /// Four cfuncs resolve through this rather than the `Players` container, and therefore take a
    /// **character** handle, not a player handle: `IsBoundaryDeath`, `SetWaitForInGame`,
    /// `VehicleDisguise`, `GetVehicleDisguiseState`. Typing them as player handles fails silently.
    pub fn by_character(&self, character: u64) -> Option<&PlayerObject> {
        self.records.iter().find(|p| p.character != 0 && p.character == character)
    }

    /// Mutable [`by_character`](Self::by_character).
    pub fn by_character_mut(&mut self, character: u64) -> Option<&mut PlayerObject> {
        self.records.iter_mut().find(|p| p.character != 0 && p.character == character)
    }

    /// `Player.GetAllPlayers` — the GUIDs of every joined record, in container order.
    pub fn all_players(&self) -> Vec<u64> {
        self.records.iter().filter(|p| p.is_joined()).map(|p| p.guid).collect()
    }

    /// `Player.GetAllCharacters` — the attached character of every joined record, skipping the
    /// unpossessed.
    pub fn all_characters(&self) -> Vec<u64> {
        self.records
            .iter()
            .filter(|p| p.is_joined() && p.character != 0)
            .map(|p| p.character)
            .collect()
    }

    /// The primary player: slot 0. `GetPrimaryPlayer` (`FUN_005DD8A0`) returns its `+0x1C`.
    pub fn primary(&self) -> Option<&PlayerObject> {
        self.get(0)
    }

    /// The secondary player: slot 1 (`GetSecondaryPlayer` `FUN_005DD900`). `None` in single-player,
    /// which is why 143 `GetSecondaryCharacter` call sites are written to tolerate nil.
    pub fn secondary(&self) -> Option<&PlayerObject> {
        self.get(1)
    }

    /// The first local player — what `GetLocalPlayer` (`FUN_005DE0B0`, 107 call sites) resolves.
    pub fn local(&self) -> Option<&PlayerObject> {
        self.records.iter().find(|p| p.is_local())
    }

    /// `Player.CreatePlayer(iPlayerId)` — ensure a record exists for **slot** `slot`, joined and local.
    /// Returns its GUID, or `0` past the cap (which the binding pushes as nil).
    ///
    /// ⚠ The argument is a **slot index, not a GUID**: `mrxplayer.lua:114-118` is
    /// `for i = 0, Player.GetMaximumPlayers() - 1 do Player.CreatePlayer(i) end`.
    ///
    /// **Idempotent**, because that loop runs against a roster the boot may already have populated —
    /// re-creating an occupied slot returns the existing GUID and leaves its possession link alone. A
    /// non-idempotent version silently produced a second record whose `character` was 0, and
    /// `Player.GetCharacter` on it returned nil into `Human.Inventory.ReloadAll`.
    pub fn create(&mut self, slot: u32) -> u64 {
        if slot as usize >= ROSTER_CAP {
            return 0;
        }
        if let Some(p) = self.get(slot) {
            return p.guid;
        }
        // Created but **not joined**: `+0x30` stays [`NOT_JOINED`] until `BindToLocal`/`BindToRemote`
        // sets it. That split matters in single-player, where `MrxPlayer.Init` creates *both* slots but
        // only slot 0 is ever bound — so `GetAllPlayers` must yield one player, not two. Auto-joining
        // here put an unpossessed slot 1 into that list, and `GetCharacter` on it returned nil into
        // `Human.Inventory.ReloadAll`.
        let guid = Self::guid_for_slot(slot);
        self.records.push(PlayerObject {
            slot: slot as u8,
            guid,
            local_id: slot as i32,
            ..Default::default()
        });
        guid
    }

    /// The GUID a given slot's player record carries at `+0x1C`.
    ///
    /// Slot 0 is `mercs2_core::LOCAL_PLAYER_GUID`, which the rest of the engine already treats as the
    /// local player's handle; further slots follow it. Deterministic rather than minted from
    /// `GuidMap`, so a re-created slot keeps the handle any script is still holding.
    pub fn guid_for_slot(slot: u32) -> u64 {
        mercs2_core::LOCAL_PLAYER_GUID + u64::from(slot)
    }

    /// `Player.DestroyPlayer(iPlayerId)` — drop the record in **slot** `slot`.
    ///
    /// ⚠ Also a slot index, not a GUID (`mrxplayer.lua:121-126`, the mirror of [`create`](Self::create)).
    pub fn destroy(&mut self, slot: u32) {
        self.records.retain(|p| u32::from(p.slot) != slot);
    }

    /// `Player.ClearPlayerDB` — drop every record.
    pub fn clear_db(&mut self) {
        self.records.clear();
    }

    /// Iterate the dense records, container order — for the roster tick.
    pub fn iter(&self) -> impl Iterator<Item = &PlayerObject> {
        self.records.iter()
    }

    /// Mutable [`iter`](Self::iter).
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PlayerObject> {
        self.records.iter_mut()
    }

    /// How many records exist at all (joined or not) — the container's live count `DAT_00DF9BA8`,
    /// which both roster ticks open by reading.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the container holds no records.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The container's type hash is the engine hash of its self-reported name.
    #[test]
    fn container_hash_is_the_engine_hash_of_players() {
        assert_eq!(PLAYERS_CONTAINER_HASH, mercs2_formats::hash::pandemic_hash_m2("Players"));
    }

    /// `get` checks the cap **before** it scans, and finds a record by its `+0x2C` regardless of where
    /// it sits in the dense region — the two halves of `FUN_006CDAF0`.
    #[test]
    fn get_is_cap_checked_then_a_scan_by_slot() {
        let mut r = PlayerRoster::new();
        // Insert slot 1 FIRST, so a slot-indexed model would answer wrongly for both slots.
        r.records.push(PlayerObject::joined_local(1, 0xB));
        r.records.push(PlayerObject::joined_local(0, 0xA));

        assert_eq!(r.get(0).map(|p| p.guid), Some(0xA), "found by matching +0x2C, not by position");
        assert_eq!(r.get(1).map(|p| p.guid), Some(0xB));
        assert!(r.get(2).is_none(), "slot 2 is rejected by the cap before any scan");
        assert!(r.get(9999).is_none());
    }

    /// The four player counts are independent numbers, and conflating any two of them is a divergence.
    #[test]
    fn the_player_counts_are_independent() {
        let r = PlayerRoster::single_player();
        assert_eq!(r.current_players(), 1, "one joined record");
        assert_eq!(REPORTED_MAX_PLAYERS, 2, "GetMaximumPlayers reports a global nothing enforces");
        assert_eq!(ROSTER_CAP, 2, "the real cap is three compile-time immediates");
        // The faithful-but-surprising pair: both are .rdata constants, never queries.
        assert_eq!(MAX_LOCAL_PLAYERS_CONST, 2.0);
        assert_eq!(CURRENT_LOCAL_PLAYERS_CONST, 1.0);

        // ...and the constant does NOT track reality: an empty roster still reports 1.0 local player.
        let empty = PlayerRoster::new();
        assert_eq!(empty.current_players(), 0, "no joined records");
        assert_eq!(
            CURRENT_LOCAL_PLAYERS_CONST, 1.0,
            "GetCurrentLocalPlayers is a constant in retail — counting honestly would diverge"
        );
    }

    /// Unjoined records do not count, which is what makes the default `viewport == NOT_JOINED` matter.
    #[test]
    fn current_players_counts_only_joined_records() {
        let mut r = PlayerRoster::new();
        r.records.push(PlayerObject::joined_local(0, 0xA));
        r.records.push(PlayerObject { slot: 1, guid: 0xB, ..Default::default() });
        assert_eq!(r.current_players(), 1);
        assert_eq!(r.all_players(), vec![0xA], "GetAllPlayers skips the unjoined too");
    }

    /// `index_for_local` is the third compile-time cap, and reports `-1` rather than panicking.
    #[test]
    fn index_for_local_rejects_out_of_range_locals() {
        let r = PlayerRoster::single_player();
        assert_eq!(r.index_for_local(0), 0);
        assert_eq!(r.index_for_local(2), -1, "local slots >= 2 are rejected (FUN_006CD960)");
        assert_eq!(r.index_for_local(-1), -1);
        assert_eq!(PlayerRoster::new().index_for_local(0), -1, "no local player -> -1, not a panic");
    }

    /// `by_character` never matches an unpossessed record: two players with `character == 0` must not
    /// resolve to each other on a lookup for character 0.
    #[test]
    fn by_character_ignores_unpossessed_records() {
        let mut r = PlayerRoster::new();
        r.records.push(PlayerObject::joined_local(0, 0xA));
        r.records.push(PlayerObject::joined_local(1, 0xB));
        assert!(r.by_character(0).is_none(), "character 0 means unpossessed, not 'the first record'");

        r.get_mut(1).unwrap().character = 0x55;
        assert_eq!(r.by_character(0x55).map(|p| p.guid), Some(0xB));
    }

    /// `create`/`destroy` take a **slot index**, and `create` is **idempotent**.
    ///
    /// `mrxplayer.lua:114-118` loops `CreatePlayer(i)` over every slot at Init, against a roster the
    /// boot has already populated. A non-idempotent create added a second record with `character == 0`,
    /// and `GetCharacter` on it returned nil straight into `Human.Inventory.ReloadAll(nil, false)` —
    /// which aborted the gameplay-setup callback chain and stalled the boot in `STATE_WAITFORGAME`.
    #[test]
    fn create_is_slot_indexed_and_idempotent() {
        let mut r = PlayerRoster::single_player();
        // Slot 0 already exists and is possessing something.
        r.get_mut(0).unwrap().character = 0xC0FFEE;

        let g0 = r.create(0);
        assert_eq!(g0, PlayerRoster::guid_for_slot(0), "re-creating returns the existing handle");
        assert_eq!(r.len(), 1, "and does NOT add a second record");
        assert_eq!(r.get(0).unwrap().character, 0xC0FFEE, "nor disturb its possession link");

        let g1 = r.create(1);
        assert_eq!(g1, PlayerRoster::guid_for_slot(1));
        assert_eq!(r.len(), 2);
        assert_eq!(r.create(2), 0, "past the cap -> 0 -> Lua nil");

        // Every joined record has a real handle, so `GetAllPlayers` can never yield a 0.
        assert!(r.all_players().iter().all(|&g| g != 0));

        r.destroy(0);
        assert!(r.get(0).is_none(), "destroy takes a slot index too");
        assert_eq!(r.get(1).map(|p| p.guid), Some(g1), "and leaves the other slot alone");
    }

    /// The `GetAnyCharacter` sentinel is a value, and it must sit clear of the minted-GUID range or a
    /// real entity could shadow it.
    ///
    /// The range check is a **compile-time** assertion: if either constant ever moves such that they
    /// overlap, the crate should fail to build rather than fail a test run.
    #[test]
    fn any_character_sentinel_cannot_collide_with_a_minted_guid() {
        const _: () = assert!(
            ANY_CHARACTER_SENTINEL > mercs2_core::FIRST_DYNAMIC_GUID,
            "the GetAnyCharacter sentinel must sit above the dynamic GUID mint range"
        );
        assert_eq!(ANY_CHARACTER_SENTINEL, 0xF000_0000, "the constant retail pushes at 0x005DE27A");
    }
}
