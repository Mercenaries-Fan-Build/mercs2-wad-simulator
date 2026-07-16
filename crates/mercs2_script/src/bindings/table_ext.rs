//! `Table` engine binding namespace — the engine's table helpers.
//!
//! **Provenance.** `Table` is not defined anywhere in the decompiled Lua corpus, so it is an engine
//! (C) global, not a script-side helper. It does not appear in the live Surface-B binding trace
//! (`mods/lua_trace_asi/reference/binding_map.json`) either, so its `luaL_Reg` VA is **not yet
//! located** — recorded as 0 below rather than guessed. That is the only unknown here; the semantics
//! are pinned by the call sites.
//!
//! **The whole observed surface** is two calls, both inside `Widget:GetChildren`
//! (`docs/mercs2-luacd/src/resident/mrxguibase.lua:887-893`):
//!
//! ```lua
//! function Widget:GetChildren()
//!   local tIdList, nListSize = _GuiInternal.GetWidgetChildren(self.BasicData.uId)
//!   local tChildren = Table.Create(nListSize, 0)             -- (arraySize, hashSize)
//!   for nIndex, uId in pairs(tIdList) do
//!     Table.InsertI(tChildren, WidgetIdIndex[uId], nIndex)   -- (table, value, index)
//!   end
//!   return tChildren
//! end
//! ```
//!
//! That fixes both signatures. `Create(n, m)` is a presized-table constructor (the `lua_createtable`
//! sizing hint — Lua 5.1 has no `table.create`, so the engine exposed one); `InsertI` is "insert at
//! Index" — `t[i] = v`. The return value of `Create` is used as a plain table by the caller, so a
//! correctly-sized empty table is a faithful implementation; the presizing is a perf hint with no
//! observable semantics.
//!
//! Without this namespace `Table.Create` resolves to nil, `GetChildren()` returns nil, and every
//! `for _ in pairs(self:GetChildren())` in the GUI layer throws — which takes down any boot that
//! imports `MrxUtil` (i.e. the whole task framework).

use mlua::{Lua, Result as LuaResult, Table, Value};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "TableExt";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Table";
/// luaL_Reg table VA — NOT YET LOCATED (absent from the Surface-B trace). 0 = unknown, not "none".
pub const TABLE_VA: u32 = 0;

pub const REQUIRED: &[Required] = &[
    Required { name: "Create", corpus_calls: 2 },
    Required { name: "InsertI", corpus_calls: 2 },
];

pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Create(nArraySize, nHashSize) -> a new table, presized. The sizing is a hint (Lua's
    // `lua_createtable`); callers use the result as an ordinary table, so capacity is unobservable.
    b.real(
        "Create",
        lua.create_function(|lua, (narr, nrec): (Option<i64>, Option<i64>)| {
            lua.create_table_with_capacity(
                narr.unwrap_or(0).max(0) as usize,
                nrec.unwrap_or(0).max(0) as usize,
            )
        })?,
    )?;

    // InsertI(t, v, i) -> t[i] = v. "Insert at Index" — unlike `table.insert`, the index is explicit
    // and nothing shifts.
    b.real(
        "InsertI",
        lua.create_function(|_, (t, v, i): (Table, Value, i64)| {
            t.set(i, v)?;
            Ok(())
        })?,
    )?;

    b.install_global(GLOBAL)
}
