//! `PlayerProfile` — the persistent profile / economy singleton `[0x01176054]`
//! (`player_code_map.md` §4).
//!
//! **One global, not per-player.** Cash and fuel are shared across the co-op pair. This is the object
//! `save_serialize_code_map.md` owns as the save source (`+0x470`); the offsets below were recovered from
//! the six profile/economy cfunc bodies.
//!
//! **The native ceiling is `i32`, and the 1-billion cap is not here.** The getters do
//! `cvtsi2ss xmm0, <field>` (signed int → float) and `AddFuel` uses `jns` → reset-to-0, i.e. a signed
//! clamp; the setters store a raw dword. The `1e9` limit is a **Lua** soft-clamp in
//! `MrxPmc.AddCashQty`, and `mrxpmc.lua:474`/`:538` bypass it by calling `Player.AddCash`/`SetCash`
//! directly. A native clamp at `1e9` is therefore an *invention* — it is removed here, not preserved.
//!
//! Lua numbers are 32-bit floats, so cash beyond 2²⁴ is not integer-exact. That is inherited from the
//! host, not a bug to fix.
//!
//! ## Three faithful quirks this module reproduces
//!
//! 1. **Five setters never dirty the profile, and the dirty flag gates `autoSave`.** Proven, not
//!    conjectured — see [`PlayerProfile::autosave_due`].
//! 2. **`SetCash` and `SetFuel` take an undocumented optional second boolean that suppresses the write
//!    entirely.** No shipped script passes it, so it can only surprise a *new* caller.
//! 3. **`Add*` dirty on the delta, not on old-vs-new**, so `AddCash(0)` does not dirty while
//!    `AddCash(n)` dirties even when the clamp makes it a no-op.

/// The profile / economy singleton `[0x01176054]`. Offsets in each field's doc comment; the VA is the
/// instruction the offset was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerProfile {
    /// `+0x2C` — cash, **signed i32** (`SetCash` `0x005DF4FE`, `AddCash` `0x005DF585`).
    pub cash: i32,
    /// `+0x30` — fuel, signed i32 (`SetFuel` `0x005DF651`, `AddFuel` `0x005DF6C7`).
    pub fuel: i32,
    /// `+0x30C` — fuel capacity (`GetFuelCapacity` `0x005DF6E0`,
    /// `SetFuelCapacity` `0x005DF778 mov [ecx+0x30c], eax`).
    ///
    /// The `[300, 9999]` range is enforced in **Lua** (`MrxPmc.SetFuelCapacity`), not here.
    pub fuel_capacity: i32,
    /// `+0x61` — profile character, a byte (`SetProfileCharacter` `0x005DF828`).
    pub character: u8,
    /// `+0x62` — profile upgrade (`SetProfileUpgrade` `0x005DF8D3`).
    pub upgrade: u8,
    /// `+0x63` — profile costume (`SetProfileCostume` `0x005DF978`).
    pub costume: u8,
    /// `+0x25E` — available costumes, a **byte count** (`GetAvailableCostumes`
    /// `0x005DFB0B movzx edi, byte [eax+0x25e]`, `SetAvailableCostumes` `0x005DFB98`).
    ///
    /// ⚠ A *count*, not a list: `wifpmcinterior.lua:1408-1430` does `== 0`, `<= 1`, `+ 1`, `>= i` on
    /// the result, so returning a table makes the arithmetic throw.
    pub available_costumes: u8,
    /// `+0x11` — the **dirty flag**, OR-ed by the setters that bother to. Gates `autoSave`; see
    /// [`autosave_due`](Self::autosave_due).
    pub dirty: bool,
    /// `+0x25F` — a **second** gate on the autosave: it must be non-zero for the save to run
    /// (`FUN_00614540` `0x00614897 cmp byte [eax+0x25f], 0 / je`). Role inferred from position only,
    /// confidence **L**.
    pub autosave_enabled: bool,
}

impl Default for PlayerProfile {
    fn default() -> Self {
        PlayerProfile {
            cash: 0,
            fuel: 0,
            // `MrxPmc.Init` calls `SetFuelCapacity(300, …)`, so 300 is the shipped starting capacity —
            // but it arrives through Lua, so the native default here is 0 and Init is what sets it.
            fuel_capacity: 0,
            character: 0,
            upgrade: 0,
            costume: 0,
            available_costumes: 0,
            dirty: false,
            // `+0x25F` must be non-zero for autosave to run at all; the shipped game autosaves, so the
            // resting state is enabled. Confidence L, per the field's own doc.
            autosave_enabled: true,
        }
    }
}

impl PlayerProfile {
    /// Whether `FUN_00614540` would run the autosave: **both** gates must pass.
    ///
    /// ```text
    /// 0x0061488C  mov  eax, [0x01176054]
    /// 0x00614891  cmp  byte [eax + 0x11], 0     ; dirty?
    /// 0x00614895  je   0x006148CE               ;   not dirty -> skip the save
    /// 0x00614897  cmp  byte [eax + 0x25f], 0    ; autosave enabled?
    /// 0x006148C2  call 0x00634460               ; ★ THE SAVE
    /// ```
    ///
    /// It is the **only** absolute-addressed reader of `+0x11`: sweeping every reference to
    /// `0x01176054`, exactly one is followed by a `[+0x11]` test.
    pub fn autosave_due(&self) -> bool {
        self.dirty && self.autosave_enabled
    }

    /// `Player.SetCash(n [, suppress])` — `FUN_005DF480`.
    ///
    /// ⚠ **Two faithful quirks.** `suppress` is an undocumented optional Lua boolean that skips the
    /// store entirely (`0x005DF4EE: cmp byte [esp+0x10], 0 / 75 0C jne` jumps *past* the write); and
    /// `SetCash` is one of the **five setters that never OR the dirty flag** (`0x005DF4FE` is a bare
    /// `mov`), so changing cash alone leaves the profile un-autosaved.
    pub fn set_cash(&mut self, cash: i32, suppress: bool) {
        if suppress {
            return;
        }
        self.cash = cash;
        // Deliberately no `self.dirty = true` — see the doc comment. [faithful-blocker: no]
    }

    /// `Player.AddCash(n)` — `FUN_005DF510`.
    ///
    /// Dirties on the **delta** (`0x005DF567: test ecx,ecx / setne dl / or [eax+0x11],dl`), then clamps
    /// at zero (`0x005DF577`–`0x005DF585`). So `AddCash(0)` does not dirty, while `AddCash(n)` dirties
    /// even when the clamp makes the result a no-op.
    pub fn add_cash(&mut self, delta: i32) {
        if delta != 0 {
            self.dirty = true;
        }
        self.cash = self.cash.saturating_add(delta).max(0);
    }

    /// `Player.GetFuel` / `Player.SetFuel(n [, suppress])` — `FUN_005DF5D0`.
    ///
    /// `SetFuel` **does** dirty, and only on an actual change
    /// (`0x005DF64E: cmp [eax+0x30],ecx / setne dl / or [eax+0x11],dl`). `suppress` skips the store
    /// **and** the dirty OR (`0x005DF63E / 75 15 jne`).
    ///
    /// No clamp to capacity here — that is `mrxpmc.lua:114-115`'s job, and clamping natively is an
    /// invention.
    pub fn set_fuel(&mut self, fuel: i32, suppress: bool) {
        if suppress {
            return;
        }
        if self.fuel != fuel {
            self.dirty = true;
        }
        self.fuel = fuel;
    }

    /// `Player.AddFuel(n)` — `FUN_005DF670`. Same delta-dirty + clamp-at-zero shape as
    /// [`add_cash`](Self::add_cash) (`0x005DF6C7`–`0x005DF6D3`).
    pub fn add_fuel(&mut self, delta: i32) {
        if delta != 0 {
            self.dirty = true;
        }
        self.fuel = self.fuel.saturating_add(delta).max(0);
    }

    /// `Player.SetFuelCapacity(n)` — `FUN_005DF720`. One of the **five non-dirtying setters**
    /// (`0x005DF778` is a bare `mov`).
    pub fn set_fuel_capacity(&mut self, cap: i32) {
        self.fuel_capacity = cap;
        // No dirty OR. [faithful-blocker: no]
    }

    /// `Player.SetProfileCharacter(n)` — `FUN_005DF7D0`. **Non-dirtying** (`0x005DF828`).
    pub fn set_character(&mut self, character: u8) {
        self.character = character;
    }

    /// `Player.SetProfileUpgrade(n)` — `FUN_005DF870`. **Does** dirty, and only on a change
    /// (`0x005DF8C3`–`0x005DF8D0`).
    pub fn set_upgrade(&mut self, upgrade: u8) {
        if self.upgrade != upgrade {
            self.dirty = true;
        }
        self.upgrade = upgrade;
    }

    /// `Player.SetProfileCostume(n)` — cfunc entry `FUN_005DF920` (§3 row 95). **Non-dirtying**
    /// (`0x005DF978` is a bare `mov`).
    pub fn set_costume(&mut self, costume: u8) {
        self.costume = costume;
    }

    /// `Player.SetAvailableCostumes(n)` — `FUN_005DFB40`. **Non-dirtying** (`0x005DFB98`).
    ///
    /// ⚠ Retail **truncates** into the byte (`mov byte [ecx+0x25e], al`), it does not clamp. So a
    /// negative or oversized argument wraps: `-1` becomes `0xFF` = 255. An earlier revision clamped to
    /// `[0, 255]`, which turns `-1` into 0 — the opposite end of the range.
    ///
    /// (That revision also claimed `SetAvailableCostumes(-1)` was a shipped call shape. It is not: the
    /// only site in the corpus is `xQ!L.lua:489`, passing `WifPmcInterior.GetAvailableCostumes()` =
    /// `_nAvailableCostumes or 1`. The truncation is still the faithful behaviour for any caller.)
    pub fn set_available_costumes(&mut self, n: i64) {
        self.available_costumes = n as u8;
    }
}

/// The five setters that never OR the dirty flag `+0x11`, named exhaustively.
///
/// **An incomplete enumeration produces an incomplete fix**, which is why this is a list and not prose:
/// changing cash, fuel capacity, profile character, profile costume or the costume roster *alone* leaves
/// the profile un-autosaved. Previous revisions of the code map listed three of the five.
///
/// Reproduce: disassemble each setter body and grep for `or byte ptr [e?? + 0x11]`.
pub const NON_DIRTYING_SETTERS: [&str; 5] = [
    "SetCash",              // 0x005DF4FE — bare mov
    "SetFuelCapacity",      // 0x005DF778
    "SetProfileCharacter",  // 0x005DF828
    "SetProfileCostume",    // 0x005DF978
    "SetAvailableCostumes", // 0x005DFB98
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **The shipped autosave bug, as an executable claim.** Each of the five setters changes a value
    /// and leaves the profile un-autosaved; the two that dirty do trigger it.
    #[test]
    fn five_setters_change_state_without_arming_the_autosave() {
        // SetCash
        let mut p = PlayerProfile::default();
        p.set_cash(50_000, false);
        assert_eq!(p.cash, 50_000, "the value changed");
        assert!(!p.autosave_due(), "SetCash never ORs +0x11 — the shipped bug");

        // SetFuelCapacity
        let mut p = PlayerProfile::default();
        p.set_fuel_capacity(9999);
        assert_eq!(p.fuel_capacity, 9999);
        assert!(!p.autosave_due());

        // SetProfileCharacter
        let mut p = PlayerProfile::default();
        p.set_character(2);
        assert_eq!(p.character, 2);
        assert!(!p.autosave_due());

        // SetProfileCostume
        let mut p = PlayerProfile::default();
        p.set_costume(3);
        assert_eq!(p.costume, 3);
        assert!(!p.autosave_due());

        // SetAvailableCostumes
        let mut p = PlayerProfile::default();
        p.set_available_costumes(4);
        assert_eq!(p.available_costumes, 4);
        assert!(!p.autosave_due());

        // ...and it TRUNCATES into the byte rather than clamping: `-1` is `0xFF`, not `0`.
        let mut p = PlayerProfile::default();
        p.set_available_costumes(-1);
        assert_eq!(p.available_costumes, 255, "mov byte [ecx+0x25e], al — truncation, not a clamp");

        assert_eq!(NON_DIRTYING_SETTERS.len(), 5, "all five are named, not three");
    }

    /// The two setters that *do* dirty only do so on an actual change — compare-then-`setne`.
    #[test]
    fn the_dirtying_setters_only_dirty_on_a_real_change() {
        let mut p = PlayerProfile::default();
        p.set_fuel(0, false);
        assert!(!p.autosave_due(), "setting fuel to what it already was does not dirty");
        p.set_fuel(120, false);
        assert!(p.autosave_due(), "an actual change dirties");

        let mut p = PlayerProfile::default();
        p.set_upgrade(0);
        assert!(!p.autosave_due());
        p.set_upgrade(1);
        assert!(p.autosave_due());
    }

    /// `Add*` dirty on the **delta**, so a zero add is inert while a clamped-to-nothing add still
    /// dirties. Both halves are observable and both are faithful.
    #[test]
    fn add_dirties_on_the_delta_not_the_result() {
        let mut p = PlayerProfile::default();
        p.add_cash(0);
        assert!(!p.autosave_due(), "AddCash(0) does not dirty");

        let mut p = PlayerProfile::default();
        p.add_cash(-999); // already 0, so the clamp makes this a no-op...
        assert_eq!(p.cash, 0, "clamped at zero");
        assert!(p.autosave_due(), "...but a non-zero delta dirties regardless");
    }

    /// The undocumented suppress-bool is a **complete** no-op: it skips the store *and* the dirty OR.
    #[test]
    fn the_suppress_bool_skips_store_and_dirty() {
        let mut p = PlayerProfile::default();
        p.set_cash(12_345, true);
        assert_eq!(p.cash, 0, "the store is skipped");
        assert!(!p.dirty);

        let mut p = PlayerProfile::default();
        p.set_fuel(777, true);
        assert_eq!(p.fuel, 0, "the store is skipped");
        assert!(!p.dirty, "and so is the dirty OR");
    }

    /// The native domain is signed i32 with **no** 1-billion cap — that limit is Lua-side and shipped
    /// scripts bypass it. A reimpl that clamps natively diverges.
    #[test]
    fn the_native_cash_domain_is_i32_with_no_billion_cap() {
        let mut p = PlayerProfile::default();
        p.set_cash(2_000_000_000, false);
        assert_eq!(p.cash, 2_000_000_000, "no native 1e9 clamp — that is MrxPmc's job");
        p.set_cash(i32::MAX, false);
        assert_eq!(p.cash, i32::MAX, "the native ceiling is int32 max");
    }

    /// Fuel is **not** natively clamped to capacity, and both gates are needed for an autosave.
    #[test]
    fn fuel_is_not_clamped_to_capacity_and_autosave_needs_both_gates() {
        let mut p = PlayerProfile::default();
        p.set_fuel_capacity(300);
        p.set_fuel(5_000, false);
        assert_eq!(p.fuel, 5_000, "clamping fuel to capacity is mrxpmc.lua's job, not the engine's");

        assert!(p.autosave_due(), "dirty + enabled");
        p.autosave_enabled = false;
        assert!(!p.autosave_due(), "+0x25F is a second, independent gate");
    }
}
