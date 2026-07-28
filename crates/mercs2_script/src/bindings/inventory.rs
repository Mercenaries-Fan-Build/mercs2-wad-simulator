//! `Inventory` engine binding namespace — luaL_Reg table VA 0x00b99fa0, 9 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites across the base corpus **plus** `docs/mercs2-dlc-luacd/src` (the base-only recipe an earlier revision named is retracted — it undercounts by 75 files)). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_child("Human", "Inventory")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult, Value};

use crate::{Guid, SharedHost};
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Inventory";
/// The Lua global table this namespace installs as.
/// Retail nests this INSIDE the `Human` array as a marker-delimited sub-table
/// (`{"Inventory",0xFFFFFFFF}` @`0x00B99F98` … `{"Inventory",0xFFFFFFFE}` @`0x00B99FE8`), and the
/// game calls it `Human.Inventory.*` exclusively — 0 bare `Inventory.*` call sites in the whole Lua
/// corpus. Corrected 2026-07-26.
pub const GLOBAL: &str = "Human.Inventory";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99fa0;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetPrimaryWeapon", corpus_calls: 11 },
    Required { name: "GetSecondaryWeapon", corpus_calls: 6 },
    Required { name: "GetVehicleWeapon", corpus_calls: 0 },
    Required { name: "GetAllWeapons", corpus_calls: 32 },
    Required { name: "SetAllWeapons", corpus_calls: 34 },
    Required { name: "DropWeapon", corpus_calls: 17 },
    Required { name: "EquipWeapon", corpus_calls: 4 },
    Required { name: "ReloadAll", corpus_calls: 3 },
    Required { name: "DestroyAllWeapons", corpus_calls: 0 },
];

/// A human's weapon loadout, backed by `mercs2_combat::inventory`.
///
/// **Return shapes are the substance here.** Four of the nine cfuncs were pushing the wrong thing, and
/// shipped scripts branch on all four (`inventory_equipment_code_map.md` §10 item 5):
/// `SetAllWeapons`/`EquipWeapon`/`DropWeapon` push a **boolean**, `ReloadAll` pushes `true` or **nil**
/// when its second argument is absent, and `DestroyAllWeapons` pushes **nothing**.
///
/// A handle miss returns nil and does **not** raise — retail's arg reader (`FUN_0059FF50`) returns 0 for
/// anything it does not recognise rather than erroring.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // `0` → nil so the game's `if not w` control flow holds; non-zero → lightuserdata (`crate::guid`).
    fn guid_opt(g: u64) -> Guid {
        Guid(g)
    }

    let h = host.clone();
    b.real("GetPrimaryWeapon", lua.create_function(move |_, c: Guid| {
        Ok(guid_opt(h.borrow().inventory_primary(c.raw())))
    })?)?;
    let h = host.clone();
    b.real("GetSecondaryWeapon", lua.create_function(move |_, c: Guid| {
        Ok(guid_opt(h.borrow().inventory_secondary(c.raw())))
    })?)?;
    let h = host.clone();
    // ⚠ No fallback, unlike the other two slot getters.
    b.real("GetVehicleWeapon", lua.create_function(move |_, c: Guid| {
        Ok(guid_opt(h.borrow().inventory_vehicle_weapon(c.raw())))
    })?)?;
    let h = host.clone();
    // ⚠ **ONE** array table — primaries (equipped first) then secondaries — plus the optional 2nd arg.
    //
    // An earlier revision returned two Lua values and its comment claimed that was required. It is the
    // opposite of the oracle: §4.4 reads the epilogue as `FUN_005A1270(N, &L)` = `lua_createtable` +
    // N × `rawseti`, then **`return 1`** (`mov eax,1` @`0x005BF14E`); §7.3 shows the Lua side as a
    // single-value assignment iterated with `pairs`. Two returns made every shipped
    // `GetAllWeapons` → `SetAllWeapons` round trip silently drop its secondaries.
    b.real("GetAllWeapons", lua.create_function(move |lua, (c, exclude): (Guid, Option<bool>)| {
        let all = h.borrow().inventory_weapons(c.raw(), exclude.unwrap_or(false));
        lua.create_sequence_from(all.into_iter().map(Guid))
    })?)?;

    let h = host.clone();
    // ⚠ Argument 2 arrives as a **table of GUIDs or a bare GUID** — six shipped mission sites use the
    // bare form, and a table-only signature raises on them. Pushes a boolean.
    b.real("SetAllWeapons", lua.create_function(move |_, (c, weapons): (Guid, Option<Value>)| {
        let list: Vec<u64> = match weapons {
            Some(Value::Table(t)) => t
                .sequence_values::<Guid>()
                .filter_map(|v| v.ok())
                .map(Guid::raw)
                .collect(),
            // The bare form is a single handle — now lightuserdata, historically an integer.
            Some(other) => match Guid::from_value(&other) {
                Guid(0) => Vec::new(),
                g => vec![g.raw()],
            },
            None => Vec::new(),
        };
        Ok(h.borrow_mut().inventory_set_weapons(c.raw(), list))
    })?)?;
    let h = host.clone();
    b.real("EquipWeapon", lua.create_function(move |_, (c, w): (Guid, Guid)| {
        Ok(h.borrow_mut().inventory_equip(c.raw(), w.raw()))
    })?)?;
    let h = host.clone();
    b.real("DropWeapon", lua.create_function(move |_, (c, w): (Guid, Guid)| {
        Ok(h.borrow_mut().inventory_drop(c.raw(), w.raw()))
    })?)?;
    let h = host.clone();
    // Pushes **nothing** — the one mutator here that does not report.
    b.real("DestroyAllWeapons", lua.create_function(move |_, c: Guid| {
        h.borrow_mut().inventory_destroy_all(c.raw());
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ Argument 2 is **required**: retail bails and pushes nil without it, and the two shipped DLC
    // call sites were written against that bail.
    b.real("ReloadAll", lua.create_function(move |_, (c, arg2): (Guid, Option<bool>)| {
        Ok(h.borrow_mut().inventory_reload_all(c.raw(), arg2))
    })?)?;

    // Installed directly as a child of `Human`, matching retail's marker-delimited sub-table. This
    // replaces an earlier arrangement that installed a top-level `Inventory` global and then mirrored
    // it onto `Human.Inventory` — the mirror is unnecessary now, and the stray global was a name the
    // game never uses (0 bare `Inventory.*` call sites).
    b.install_child("Human", "Inventory")
}
