//! `ObjectState` engine binding namespace — luaL_Reg table VA 0x00b995b0, 9 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("ObjectState")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

const FNV1A_OFFSET_BASIS: u32 = 0x811C_9DC5;
const FNV1A_PRIME: u32 = 0x0100_0193;

/// Pandemic Mercs-2 string hash (FNV-1a, case-suppressed, `^0x2A` + `*prime` finalize). Same algorithm
/// as `mercs2_formats::hash::pandemic_hash_m2`, inlined because this crate stays dependency-light.
/// Verified vectors: `m2("texture") == 0xF011157A`, `m2("model") == 0x5B724250`.
fn pandemic_hash_m2(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut h = FNV1A_OFFSET_BASIS;
    for &byte in text.as_bytes() {
        h ^= (byte | 0x20) as u32;
        h = h.wrapping_mul(FNV1A_PRIME);
    }
    h ^= 0x2A;
    h.wrapping_mul(FNV1A_PRIME)
}

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "ObjectState";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "ObjectState";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b995b0;

pub const REQUIRED: &[Required] = &[
    Required { name: "SendMessage", corpus_calls: 0 },
    Required { name: "SendDamage", corpus_calls: 1 },
    Required { name: "SetState", corpus_calls: 0 },
    Required { name: "GetLinkGuid", corpus_calls: 7 },
    Required { name: "StartEmitter", corpus_calls: 14 },
    Required { name: "StopEmitter", corpus_calls: 14 },
    Required { name: "GetStringHash", corpus_calls: 3 },
    Required { name: "PrintStateMachine", corpus_calls: 0 },
    Required { name: "DebugStateMachine", corpus_calls: 0 },
];

/// Object state-machine + node FX driver. `GetStringHash` is a pure engine string hash (real). The
/// message/damage/state/emitter cfuncs drive the native FX + state machine we don't own yet, so they
/// are faithful no-ops; `GetLinkGuid` returns nil (no link) until the native state object exists.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Pure hash — fully faithful.
    b.real("GetStringHash", lua.create_function(|_, s: String| Ok(pandemic_hash_m2(&s) as i64))?)?;

    // Query — no linked object until the native state machine is backed.
    b.real("GetLinkGuid", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // SendDamage(target, amount) → apply damage to the target's health (returns whether it died).
    let h = host.clone();
    b.real("SendDamage", lua.create_function(move |_, (target, amount, _rest): (i64, f32, MultiValue)| {
        Ok(h.borrow_mut().object_send_damage(target as u64, amount))
    })?)?;

    // State-machine state + node emitters → real host state (the emitter's particle *rendering* is a
    // separate render pass; the active-emitter set + state name are engine state).
    let h = host.clone();
    b.real("SetState", lua.create_function(move |_, (guid, state): (i64, String)| {
        h.borrow_mut().object_sm_set_state(guid as u64, &state);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("StartEmitter", lua.create_function(move |_, (guid, name): (i64, String)| {
        h.borrow_mut().object_start_emitter(guid as u64, &name);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("StopEmitter", lua.create_function(move |_, (guid, name): (i64, String)| {
        h.borrow_mut().object_stop_emitter(guid as u64, &name);
        Ok(())
    })?)?;

    // Cross-object state-machine messaging → recorded ObjectState commands. The Print/Debug dumps are
    // genuine dev stubs (PC strips the debug menu).
    super::record_all(&mut b, lua, host, "ObjectState", &["SendMessage"])?;
    b.stub("PrintStateMachine", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("DebugStateMachine", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
