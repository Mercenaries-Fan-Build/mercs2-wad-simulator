//! Player ↔ character binding — `FUN_006A4060` and the bind/unbind cfuncs
//! (`player_code_map.md` §5).
//!
//! **Possession is a FIELD, not a component.** `Player.AttachToCharacter(iPlayerId, uCharacterGuid)`
//! (`FUN_005DE4E0`) resolves the slot through `FUN_006CDAF0` and calls
//! `FUN_006A4060(playerObj, charGuid)`, whose three writes are:
//!
//! ```text
//! 0x006A422E  mov [ebx+0x20], eax          ★ THE POSSESSION WRITE  (character)
//! 0x006A4279  mov [ebx+0x24], edi          clear the control source (edi = 0)
//! 0x006A4314  mov [ebx+0x3a8], eax         seed the disguise sub-struct with [ebx+0x20]
//! ```
//!
//! > **⚠ A retracted reading, recorded so it is not re-derived.** An earlier revision of the code map
//! > concluded that attaching *adds a control-marker component* to the character via container
//! > `0x00DF9B10`, and detaching removes it. That is **wrong**. `0x00DF9B10` names itself
//! > `CheatInfiniteAmmo` (`[0x00DF9B10] = 0x00BC3F48`, `[0x00BC3F48+0x34] = 0x00647B90`, whose body is
//! > `B8 <ptr> C3` → that string), its element is **one byte**, and the attach path visits it only to
//! > re-apply an *already active* cheat to the new body — with cheats off the branch never runs, so it
//! > cannot be what marks possession. Whether the engine marks the character player-driven at all beyond
//! > `player+0x20` is **still open** (map §9.1).
//!
//! ## Argument shape
//!
//! ⚠ `AttachToCharacter`, `DetachFromCharacter`, `BindToLocal`, `BindToRemote`, `Unbind`,
//! `CreatePlayer` and `DestroyPlayer` take a **slot index**, not a player GUID —
//! `mrxplayer.lua:587 Player.AttachToCharacter(iPlayerId, uCharacterGuid)`, and §5's
//! `obj = FUN_006CDAF0(idx)` confirms it. An earlier revision keyed its map on arg 1 as though it were a
//! player GUID, so writes land under key `0` while reads look up key `2`.

use crate::object::PlayerObject;
use crate::roster::PlayerRoster;

/// The five config flags `FUN_004C2C20` publishes and the attach path reads.
///
/// Each name was recovered by matching the pushed hash constant against a *real* candidate name with
/// `pandemic_hash_m2` — **none was invented** ([[no-arbitrary-hashes]]):
///
/// | field | global | hash | corroboration |
/// |---|---|---|---|
/// | `demo` | `DAT_01175F59` | `0x949A9B14` | also read by `Sys.IsDemoMode` `0x005E5679` |
/// | `godmode` | `DAT_01175F5A` | `0x40B39AC0` | Xbox debug-cheat-menu "God Mode" |
/// | `unkillable` | `DAT_01175F5B` | `0x4299D698` | Xbox "Demigod Mode" |
/// | `infammo` | `DAT_01175F5C` | `0xF2E44D84` | gates `Object.SetInfiniteAmmo` `0x005CE86D` |
/// | `showgodmode` | `DAT_01175F5D` | `0xE79B0021` | Xbox "Show God Mode Et Al" |
///
/// Note `demo` is **not a cheat** — it is the demo-mode flag, and an earlier gloss calling all five
/// "cheat toggles" was wrong about that one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CheatFlags {
    /// `DAT_01175F59` — demo mode. Not a cheat.
    pub demo: bool,
    /// `DAT_01175F5A` — god mode.
    pub godmode: bool,
    /// `DAT_01175F5B` — unkillable ("demigod").
    pub unkillable: bool,
    /// `DAT_01175F5C` — infinite ammo.
    pub infammo: bool,
    /// `DAT_01175F5D` — show-god-mode diagnostics.
    pub showgodmode: bool,
}

/// The name-hashes `FUN_004C2C20` pushes to look each flag up, in [`CheatFlags`] field order. Held so
/// the test below can prove each against the engine hash rather than trusting the table.
pub const CHEAT_FLAG_HASHES: [(&str, u32); 5] = [
    ("demo", 0x949A_9B14),
    ("godmode", 0x40B3_9AC0),
    ("unkillable", 0x4299_D698),
    ("infammo", 0xF2E4_4D84),
    ("showgodmode", 0xE79B_0021),
];

impl CheatFlags {
    /// The gate the attach path tests before re-applying the infinite-ammo cheat to the new body:
    /// `0x006A414F` / `0x006A4158` read `infammo || demo`.
    pub fn reapplies_infinite_ammo(&self) -> bool {
        self.infammo || self.demo
    }

    /// The gate on the sibling `FUN_005262D0` call: `godmode || demo`, plus `unkillable` passed
    /// separately (`0x006A4107`–`0x006A4147`).
    pub fn grants_invulnerability(&self) -> bool {
        self.godmode || self.demo
    }
}

/// `Player.AttachToCharacter(slot, character)` → `FUN_006A4060`.
///
/// Performs the three recovered writes in order: possession, clear-control-source, seed-disguise-base.
/// Returns `false` when the slot resolves to no record (retail's `FUN_006CDAF0` returns 0 and the body
/// is skipped).
///
/// The `CheatInfiniteAmmo` re-application is deliberately **not** modelled as a possession effect — see
/// the module docs. `cheats` is accepted so the decision point is visible and testable, and so a later
/// cheat system has the hook it needs.
pub fn attach_to_character(
    roster: &mut PlayerRoster,
    slot: u32,
    character: u64,
    cheats: CheatFlags,
) -> bool {
    let Some(p) = roster.get_mut(slot) else { return false };
    apply_attach(p, character, cheats);
    true
}

/// The body of the attach, split out so [`PlayerObject`]-level tests do not need a roster.
fn apply_attach(p: &mut PlayerObject, character: u64, _cheats: CheatFlags) {
    // 0x006A422E — the possession write.
    p.character = character;
    // 0x006A4279 — the control source is cleared unconditionally on attach. Whoever *sets* it lives in
    // the seat/ride subsystem and is not statically reachable (§9.1 S1), so clearing is the only half
    // of this field a faithful reimpl can produce on its own.
    p.control_source = 0;
    // 0x006A4314 — seed the disguise sub-struct's base with the freshly written character guid.
    p.disguise.base = character;
}

/// `Player.DetachFromCharacter(slot)` — clear the possession link.
///
/// Retail reaches this through the same `FUN_006A4060` with a zero `charGuid`: the `if (old && charGuid
/// != old)` teardown branch runs, then `[ebx+0x20]` is written with 0 and `[ebx+0x24]` cleared.
pub fn detach_from_character(roster: &mut PlayerRoster, slot: u32) -> bool {
    let Some(p) = roster.get_mut(slot) else { return false };
    apply_attach(p, 0, CheatFlags::default());
    true
}

/// `Player.BindToLocal(slot, localId)` — `FUN_005DE690`, which resolves the player and calls
/// `thunk_FUN_024EBC20`.
///
/// ⚠ **Two arguments**, not one (an earlier revision took one). And ⚠ the field writes are *inferred*:
/// all three of `BindToLocal`/`BindToRemote`/`Unbind` delegate to SecuROM VM stubs
/// (`[0x0245F5A0] = 0x024EBC20`, `[0x02458FB4] = 0x024F0270`, `[0x0245A1D4] = 0x024E3B40`), so what each
/// assigns is derived from the three predicates that read the fields — `IsJoined` = `+0x30 != -1`,
/// `IsLocal` = that and `+0x58 == 0`, `IsRemote` = that and `+0x58 != 0` (map §9.1 S2). The *semantics*
/// are confidence H; the *assignment* is the inference.
// CONFIRM-LIVE (§9.1 S2): one-shot bp at 0x005DE7A4 (BindToRemote's call) and diff +0x58 across it.
pub fn bind_to_local(roster: &mut PlayerRoster, slot: u32, local_id: i32) -> bool {
    let Some(p) = roster.get_mut(slot) else { return false };
    p.remote = false;
    p.local_id = local_id;
    // Joining is what makes `+0x30` stop being -1; a local bind takes the viewport matching its slot.
    if p.viewport == crate::object::NOT_JOINED {
        p.viewport = i32::from(p.slot);
    }
    true
}

/// `Player.BindToRemote(slot)` — `FUN_005DE690`'s sibling. Same inference caveat as
/// [`bind_to_local`].
// CONFIRM-LIVE (§9.1 S2).
pub fn bind_to_remote(roster: &mut PlayerRoster, slot: u32) -> bool {
    let Some(p) = roster.get_mut(slot) else { return false };
    p.remote = true;
    if p.viewport == crate::object::NOT_JOINED {
        p.viewport = i32::from(p.slot);
    }
    true
}

/// `Player.Unbind(slot)` — return the record to not-joined. Same inference caveat as
/// [`bind_to_local`].
// CONFIRM-LIVE (§9.1 S2).
pub fn unbind(roster: &mut PlayerRoster, slot: u32) -> bool {
    let Some(p) = roster.get_mut(slot) else { return false };
    p.viewport = crate::object::NOT_JOINED;
    p.remote = false;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every cheat-flag hash equals the engine hash of the name it was matched to. This is what keeps
    /// the table honest: a fabricated name would not hash to the constant the binary pushes.
    #[test]
    fn cheat_flag_hashes_match_their_names() {
        for (name, hash) in CHEAT_FLAG_HASHES {
            assert_eq!(
                mercs2_formats::hash::pandemic_hash_m2(name),
                hash,
                "hash for {name:?} does not match the constant pushed by FUN_004C2C20"
            );
        }
    }

    /// The two gates the attach path reads are different conjunctions, and `demo` appears in both —
    /// which is why treating all five flags as interchangeable "cheats" loses information.
    #[test]
    fn the_attach_gates_are_distinct() {
        let demo = CheatFlags { demo: true, ..Default::default() };
        assert!(demo.reapplies_infinite_ammo(), "demo implies infinite ammo re-apply");
        assert!(demo.grants_invulnerability(), "demo also implies invulnerability");

        let ammo = CheatFlags { infammo: true, ..Default::default() };
        assert!(ammo.reapplies_infinite_ammo());
        assert!(!ammo.grants_invulnerability(), "infammo alone does not grant invulnerability");

        let god = CheatFlags { godmode: true, ..Default::default() };
        assert!(!god.reapplies_infinite_ammo());
        assert!(god.grants_invulnerability());
    }

    /// The three recovered writes, in order — and specifically that attach **clears** the control
    /// source and **seeds** the disguise base, which the two obvious one-line implementations miss.
    #[test]
    fn attach_performs_all_three_writes() {
        let mut r = PlayerRoster::single_player();
        // Pretend the player was riding something before the attach.
        r.get_mut(0).unwrap().control_source = 0xDEAD;

        assert!(attach_to_character(&mut r, 0, 0x1234, CheatFlags::default()));
        let p = r.get(0).unwrap();
        assert_eq!(p.character, 0x1234, "0x006A422E: the possession write");
        assert_eq!(p.control_source, 0, "0x006A4279: attach clears the control source");
        assert_eq!(p.disguise.base, 0x1234, "0x006A4314: the disguise base is seeded from +0x20");
    }

    /// Re-attaching to a different character leaves no trace of the old link — the teardown branch.
    #[test]
    fn reattaching_replaces_the_old_link_entirely() {
        let mut r = PlayerRoster::single_player();
        attach_to_character(&mut r, 0, 0xAAA, CheatFlags::default());
        attach_to_character(&mut r, 0, 0xBBB, CheatFlags::default());
        let p = r.get(0).unwrap();
        assert_eq!(p.character, 0xBBB);
        assert_eq!(p.disguise.base, 0xBBB, "the disguise base tracks the current character");
        assert!(r.by_character(0xAAA).is_none(), "the old character no longer resolves to a player");
    }

    /// Detach clears possession, and an unpossessed record stops resolving by character.
    #[test]
    fn detach_clears_possession() {
        let mut r = PlayerRoster::single_player();
        attach_to_character(&mut r, 0, 0x1234, CheatFlags::default());
        assert!(r.by_character(0x1234).is_some());

        assert!(detach_from_character(&mut r, 0));
        assert_eq!(r.get(0).unwrap().character, 0);
        assert!(r.by_character(0x1234).is_none());
        assert!(r.by_character(0).is_none(), "character 0 must not resolve to the detached record");
    }

    /// An out-of-range slot is a `false`, not a panic — the binding turns it into a silent no-op, which
    /// is what `FUN_006CDAF0` returning 0 produces in retail.
    #[test]
    fn attach_to_a_bad_slot_is_a_no_op() {
        let mut r = PlayerRoster::single_player();
        assert!(!attach_to_character(&mut r, 7, 0x1234, CheatFlags::default()));
        assert_eq!(r.get(0).unwrap().character, 0, "the valid record is untouched");
    }

    /// The bind trio moves the two fields the three predicates read, and `unbind` restores
    /// not-joined — so `IsJoined`/`IsLocal`/`IsRemote` all answer correctly across the cycle.
    #[test]
    fn the_bind_trio_drives_the_three_predicates() {
        let mut r = PlayerRoster::new();
        // `create` takes a SLOT and leaves the record **not joined** — binding is what joins it.
        r.create(0);
        let p = r.get(0).expect("slot 0 exists");
        assert!(!p.is_joined() && !p.is_local() && !p.is_remote(), "created is not yet joined");

        assert!(bind_to_local(&mut r, 0, 1));
        let p = r.get(0).unwrap();
        assert!(p.is_joined() && p.is_local() && !p.is_remote());
        assert_eq!(p.local_id, 1, "BindToLocal takes a local id as its second argument");

        assert!(bind_to_remote(&mut r, 0));
        let p = r.get(0).unwrap();
        assert!(p.is_joined() && !p.is_local() && p.is_remote());

        assert!(unbind(&mut r, 0));
        assert!(!r.get(0).unwrap().is_joined());
    }
}
