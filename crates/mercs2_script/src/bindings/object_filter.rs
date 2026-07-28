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

use crate::{Guid, SharedHost};
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

    // A filter handle is an engine handle like any other, so it leaves as lightuserdata (`crate::guid`)
    // and comes back through `Guid`'s `FromLua`.
    let h = host.clone();
    b.real("Create", lua.create_function(move |_, _: MultiValue| Ok(Guid(h.borrow_mut().object_filter_create())))?)?;
    let h = host.clone();
    b.real("Copy", lua.create_function(move |_, src: Guid| Ok(Guid(h.borrow_mut().object_filter_copy(src.raw()))))?)?;

    // Configuration mutators → the registry filter.
    let h = host.clone();
    b.real("SetFilter", lua.create_function(move |_, (f, expr): (Guid, String)| {
        h.borrow_mut().object_filter_set_expr(f.raw(), &expr);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearFilter", lua.create_function(move |_, f: Guid| { h.borrow_mut().object_filter_set_expr(f.raw(), ""); Ok(()) })?)?;
    let h = host.clone();
    // Arg 3 is **bExclude**, not bInclude — retail's add primitive clears the include bit when the
    // flag is set (Xbox `0x8247D5AC`: `cmplwi flag,0` → `andc` vs `or`), the PC omitted-arg default
    // is 0, and `mrxtaskobjective.lua` passes `true` from `RemoveTarget` to un-target. The default
    // coincides either way; every explicit argument was inverted before 2026-07-26.
    b.real("AddObject", lua.create_function(move |_, (f, guid, exclude): (Guid, Guid, Option<bool>)| {
        h.borrow_mut().object_filter_add(f.raw(), guid.raw(), exclude.unwrap_or(false));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("RemoveObject", lua.create_function(move |_, (f, guid): (Guid, Guid)| {
        h.borrow_mut().object_filter_remove(f.raw(), guid.raw());
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearObjects", lua.create_function(move |_, f: Guid| { h.borrow_mut().object_filter_clear(f.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("UsePlayers", lua.create_function(move |_, (f, on): (Guid, Option<bool>)| {
        h.borrow_mut().object_filter_use_players(f.raw(), on.unwrap_or(true));
        Ok(())
    })?)?;

    // Evaluators → query the registry filter.
    let h = host.clone();
    // The sequence this returns is iterated straight into other handle slots and then type-checked:
    // `mrxtaskobjectiveaction.lua:21` feeds each element to `Pg.AddContextAction`, and the same
    // objective's `_TargetActioned`/`_TargetDestroyed` gate on `type(uGuid) == "userdata"` (:31, :40)
    // before calling `RemoveTarget`. Integers made both gates fail closed.
    b.real("GetObjects", lua.create_function(move |_, (f, _which): (Guid, Option<bool>)| {
        Ok(h.borrow().object_filter_objects(f.raw()).into_iter().map(Guid).collect::<Vec<_>>())
    })?)?;
    let h = host.clone();
    b.real("Eval", lua.create_function(move |_, (f, guid): (Guid, Guid)| {
        Ok(h.borrow().object_filter_eval(f.raw(), guid.raw()))
    })?)?;
    let h = host.clone();
    b.real("_GC", lua.create_function(move |_, f: Guid| { h.borrow_mut().object_filter_gc(f.raw()); Ok(()) })?)?;

    // No second player in a single-player session → the "no such handle" GUID, which surfaces as nil.
    b.real("GetCoopPlayerGuid", lua.create_function(|_, _: MultiValue| Ok(Guid::NONE))?)?;

    // Filter-graph association/relation edges → recorded ObjectFilter commands (the filter-graph
    // relation model consumes them).
    super::record_all(&mut b, lua, host, "ObjectFilter", &[
        "SetAssociation", "ClearAssociation", "SetRelation", "ClearRelation",
    ])?;

    b.install_global(GLOBAL)
}
