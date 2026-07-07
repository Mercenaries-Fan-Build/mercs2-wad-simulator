//! `Event` engine binding namespace — luaL_Reg table VA 0x00b987f8, 4 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Event")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Event";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Event";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b987f8;

pub const REQUIRED: &[Required] = &[
    Required { name: "Create", corpus_calls: 688 },
    Required { name: "CreatePersistent", corpus_calls: 117 },
    Required { name: "Delete", corpus_calls: 654 },
    Required { name: "Post", corpus_calls: 72 },
];

/// Event-kind constants the boot scripts compare against, plus a **fake** `Event.Create` so scripts
/// register handlers without erroring. Phase 1 does not run the event loop, so `Create` returns a
/// distinct integer handle (a stub, not a real body); `CreatePersistent`/`Delete`/`Post` are for a
/// later silo once the event bus is wired (`event_bus_code_map.md`).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let _ = host;
    let mut b = NsBuilder::new(lua)?;

    // Event-kind enum values (not coverage-tracked — they are constants, not cfuncs).
    for (i, k) in [
        "ObjectHibernation",
        "TimerRelative",
        "TimerAbsolute",
        "ObjectDeath",
        "ObjectProximity",
        "Boundary",
        "ObjectPhysicsEvent",
    ]
    .iter()
    .enumerate()
    {
        b.value(k, (i + 1) as i64)?;
    }

    // Create(kind, params, fn, args) -> opaque handle. Distinct integer so scripts can store/compare.
    let counter = Rc::new(RefCell::new(0i64));
    b.stub(
        "Create",
        lua.create_function(move |_, _: MultiValue| {
            let mut n = counter.borrow_mut();
            *n += 1;
            Ok(*n)
        })?,
    )?;

    b.install_global(GLOBAL)
}
