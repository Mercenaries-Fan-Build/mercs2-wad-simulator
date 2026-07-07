//! `ObjectFilter` engine binding namespace — luaL_Reg table VA 0x00b98770, 16 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("ObjectFilter")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "ObjectFilter";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "ObjectFilter";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98770;

pub const REQUIRED: &[Required] = &[
    Required { name: "Create", corpus_calls: 20 },
    Required { name: "Copy", corpus_calls: 2 },
    Required { name: "SetFilter", corpus_calls: 15 },
    Required { name: "ClearFilter", corpus_calls: 0 },
    Required { name: "AddObject", corpus_calls: 7 },
    Required { name: "RemoveObject", corpus_calls: 8 },
    Required { name: "GetObjects", corpus_calls: 12 },
    Required { name: "ClearObjects", corpus_calls: 0 },
    Required { name: "UsePlayers", corpus_calls: 1 },
    Required { name: "SetAssociation", corpus_calls: 0 },
    Required { name: "ClearAssociation", corpus_calls: 0 },
    Required { name: "SetRelation", corpus_calls: 0 },
    Required { name: "ClearRelation", corpus_calls: 0 },
    Required { name: "Eval", corpus_calls: 1 },
    Required { name: "GetCoopPlayerGuid", corpus_calls: 2 },
    Required { name: "_GC", corpus_calls: 0 },
];

/// Object query filters, backed by the real `mercs2_core::ObjectFilterRegistry` on the host: a label
/// boolean-expression predicate (`"Hero||(China&&Vehicle)"`) + explicit include/exclude object sets +
/// a `UsePlayers` flag. `Create`/`Copy` mint handles; the mutators configure the registry filter;
/// `Eval`/`GetObjects` query it against the host's object label store. The filter-graph association/
/// relation cfuncs (0 shipped calls) remain unbacked (see burn-down).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real("Create", lua.create_function(move |_, _: MultiValue| Ok(h.borrow_mut().object_filter_create() as i64))?)?;
    let h = host.clone();
    b.real("Copy", lua.create_function(move |_, src: i64| Ok(h.borrow_mut().object_filter_copy(src as u64) as i64))?)?;

    // Configuration mutators → the registry filter.
    let h = host.clone();
    b.real("SetFilter", lua.create_function(move |_, (f, expr): (i64, String)| {
        h.borrow_mut().object_filter_set_expr(f as u64, &expr);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearFilter", lua.create_function(move |_, f: i64| { h.borrow_mut().object_filter_set_expr(f as u64, ""); Ok(()) })?)?;
    // AddObject(f, guid, bInclude) — bInclude defaults true (an explicit include).
    let h = host.clone();
    b.real("AddObject", lua.create_function(move |_, (f, guid, include): (i64, i64, Option<bool>)| {
        h.borrow_mut().object_filter_add(f as u64, guid as u64, include.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("RemoveObject", lua.create_function(move |_, (f, guid): (i64, i64)| {
        h.borrow_mut().object_filter_remove(f as u64, guid as u64);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearObjects", lua.create_function(move |_, f: i64| { h.borrow_mut().object_filter_clear(f as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("UsePlayers", lua.create_function(move |_, (f, on): (i64, Option<bool>)| {
        h.borrow_mut().object_filter_use_players(f as u64, on.unwrap_or(true));
        Ok(())
    })?)?;

    // Evaluators → query the registry filter.
    let h = host.clone();
    b.real("GetObjects", lua.create_function(move |_, (f, _which): (i64, Option<bool>)| {
        Ok(h.borrow().object_filter_objects(f as u64).into_iter().map(|g| g as i64).collect::<Vec<_>>())
    })?)?;
    let h = host.clone();
    b.real("Eval", lua.create_function(move |_, (f, guid): (i64, i64)| {
        Ok(h.borrow().object_filter_eval(f as u64, guid as u64))
    })?)?;
    let h = host.clone();
    b.real("_GC", lua.create_function(move |_, f: i64| { h.borrow_mut().object_filter_gc(f as u64); Ok(()) })?)?;

    b.real("GetCoopPlayerGuid", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // UNBACKED residue (0 shipped calls): filter-graph association/relation edges — a filter-to-filter
    // relation model not yet built. Honest no-ops (see burn-down).
    b.stub("SetAssociation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearAssociation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("SetRelation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearRelation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
