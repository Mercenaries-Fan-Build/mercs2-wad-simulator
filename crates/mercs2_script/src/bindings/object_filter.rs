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

use std::cell::Cell;
use std::rc::Rc;

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

/// Object query/filter predicate handles. The engine backs a filter with a native `ObjectFilter`
/// object (a label/faction/type predicate + explicit include/exclude object set); we don't yet own
/// that native object, so filters are faithful empty predicates: `Create`/`Copy` hand back a stable
/// non-nil handle (so `local f = ObjectFilter.Create()` and `f = f or ObjectFilter.Create()` work),
/// the mutators are accepted no-ops, and the evaluators return an authentic empty/false/nil result.
/// A later silo backs this with a real host filter registry (see report — needs `object_filter_*`).
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Filter handles are opaque to the game; hand back a unique, stable, non-nil id per filter so the
    // game's `local f = ObjectFilter.Create()` / `f = f or ObjectFilter.Create()` control flow holds.
    let next = Rc::new(Cell::new(1i64));
    let n = next.clone();
    b.real("Create", lua.create_function(move |_, _: MultiValue| { let h = n.get(); n.set(h + 1); Ok(h) })?)?;
    let n = next.clone();
    b.real("Copy", lua.create_function(move |_, _: MultiValue| { let h = n.get(); n.set(h + 1); Ok(h) })?)?;

    // Evaluators — faithful empty predicate.
    b.real("GetObjects", lua.create_function(|_, _: MultiValue| Ok(Vec::<i64>::new()))?)?;
    b.real("Eval", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetCoopPlayerGuid", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // Mutators / configuration — accepted no-ops until the native filter object exists.
    b.stub("SetFilter", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearFilter", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("AddObject", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("RemoveObject", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearObjects", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("UsePlayers", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("SetAssociation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearAssociation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("SetRelation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ClearRelation", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("_GC", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
