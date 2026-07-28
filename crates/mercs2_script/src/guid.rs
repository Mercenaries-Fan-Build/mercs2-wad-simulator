//! [`Guid`] — the engine-handle type as it crosses the Lua boundary: **lightuserdata**, like retail.
//!
//! # Why this type exists
//!
//! Retail does not hand GUIDs to Lua as numbers. Its universal argument reader `FUN_0059FF50`
//! accepts **only Lua type tags 2 (lightuserdata) and 7 (full userdata)** and returns `0` for
//! anything else without raising (`docs/reverse_engineer/inventory_equipment_code_map.md` §4.5), and
//! the push side matches: `Player.GetAnyCharacter` builds its `0xF0000000` sentinel as
//! `mov dword [eax], 0xf0000000` / `mov dword [eax+4], 2` — value plus **type tag 2**, i.e.
//! lightuserdata (`docs/reverse_engineer/player_code_map.md` §10.7).
//!
//! The shipped scripts are written against that and **type-check on it**. In the vendored corpus
//! (`corpus/mercs2-luacd/src/`) there are 114 `"userdata"` comparisons — roughly 54 of the form
//! `type(u) == "userdata"` (a gate that must PASS before the code proceeds) and roughly 60 of the
//! form `type(u) ~= "userdata"` (a guard that BAILS). While our bindings returned Lua integers,
//! every one of the first kind was dead and every one of the second kind fired. Two concrete
//! casualties:
//!
//! - `corpus/mercs2-luacd/src/resident/mrxsupportdesignatorsatellite.lua:61,72` bails on its first
//!   line, so the satellite designator never starts.
//! - `corpus/mercs2-luacd/src/resident/mrxguiinterface.lua:413,458,492,557,1090,1104,1436` drops its
//!   entire net-sync fan-out.
//!
//! # The contract
//!
//! - **Out (`IntoLua`)**: a non-zero GUID becomes [`Value::LightUserData`]; `0` becomes `nil`.
//!   `0` is the engine's "no such handle" and the scripts' `if not uGuid then` depends on it being
//!   falsey — a lightuserdata (even a NULL one) is truthy in Lua, so 0 must never be pushed as one.
//! - **In (`FromLua`)**: accepts lightuserdata (the real shape), `nil`/absent → [`Guid::NONE`], and
//!   — **transitionally** — integers and numbers, so the conversion can land binding-by-binding
//!   without a flag day while some namespaces still return raw integers. Anything else is `NONE`
//!   rather than an error, which is exactly what `FUN_0059FF50` does (return 0, never raise).
//!
//! That last point is what preserves the **nil-handle contract** documented at the top of
//! [`crate::bindings`]: a handle miss must push `nil`, not raise, because scripts chain
//! `Vehicle.GetRiders(Pg.GetGuidByName(sName))` and let the nil fall through. `Guid` absorbs `nil`
//! into `NONE` for exactly that reason, so `fn(u: Guid)` is as tolerant as the `Option<i64>` it
//! replaces. (`Option<Guid>` also works — mlua maps `nil` → `None` before ever calling us — but a
//! bare `Guid` says "0 means none" in one place instead of two.)
//!
//! # Pointer width
//!
//! mlua's [`LightUserData`] wraps a `*mut c_void`, so a GUID has to fit in a pointer. On the 64-bit
//! targets this workspace builds for that is lossless for the full `u64`. On a 32-bit target it is
//! not, so [`Guid::into_lua`] converts through `usize::try_from` and **raises rather than
//! truncating** — see [`Guid::as_ptr`]. In practice nothing trips it: `mercs2_core::GuidMap` mints
//! from `FIRST_DYNAMIC_GUID` (`0x1000_0000`) upward and retail's own handles were 32-bit pointers,
//! so every GUID the game can observe fits in 32 bits.

use std::ffi::c_void;

use mlua::{Error as LuaError, FromLua, IntoLua, Lua, LightUserData, Result as LuaResult, Value};

/// An engine object handle as seen by Lua.
///
/// This is a **boundary type only**. [`crate::EngineHost`] still speaks plain `u64`; nothing behind
/// the trait knows this type exists. Convert at the edge with [`Guid::raw`] / [`Guid::from`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Guid(pub u64);

impl Guid {
    /// The engine's "no such handle". Surfaces to Lua as `nil`, never as a NULL lightuserdata.
    pub const NONE: Guid = Guid(0);

    /// The underlying handle, for handing to an [`crate::EngineHost`] method.
    #[inline]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Whether this is the 0/"not found" handle.
    #[inline]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Whether this names something.
    #[inline]
    pub const fn is_some(self) -> bool {
        self.0 != 0
    }

    /// `Some(raw)` for a real handle, `None` for 0 — for the host calls that want an `Option`.
    #[inline]
    pub const fn opt(self) -> Option<u64> {
        if self.0 == 0 {
            None
        } else {
            Some(self.0)
        }
    }

    /// Read a handle out of an already-materialised [`Value`] — the `FromLua` body, minus the `&Lua`
    /// that conversion never uses. Needed where a handle arrives inside a `Value` the binding is
    /// already matching on (e.g. `Ai.Goal`'s bare-handle-or-`{AIGuid=…}`-table argument).
    #[inline]
    pub fn from_value(value: &Value) -> Guid {
        match value {
            Value::LightUserData(ud) => Guid(ud.0 as usize as u64),
            // TRANSITIONAL: drop once every namespace that *returns* a handle returns `Guid`.
            // Negative integers cannot be handles, so they collapse to NONE rather than wrapping
            // around into a huge u64 that would alias a live object.
            Value::Integer(i) => Guid(u64::try_from(*i).unwrap_or(0)),
            Value::Number(n) => {
                if n.is_finite() && *n >= 0.0 && *n <= u64::MAX as f64 {
                    Guid(*n as u64)
                } else {
                    Guid::NONE
                }
            }
            // A full userdata is tag 7, which `FUN_0059FF50` also accepts; we mint none of our own,
            // so treat it as an unrecognised handle rather than reading through it.
            _ => Guid::NONE,
        }
    }

    /// The pointer a lightuserdata would carry, or an error if the handle does not fit one.
    ///
    /// `usize::try_from` is the only lossless door from `u64` to pointer width. On 64-bit it never
    /// fails; on a 32-bit target it fails for a handle above `u32::MAX`, and we surface that as a
    /// conversion error rather than silently dropping the high half — a truncated handle would
    /// alias a different object, which is worse than a loud failure.
    #[inline]
    pub fn as_ptr(self) -> LuaResult<*mut c_void> {
        match usize::try_from(self.0) {
            Ok(bits) => Ok(bits as *mut c_void),
            Err(_) => Err(LuaError::ToLuaConversionError {
                from: "Guid".to_string(),
                to: "lightuserdata",
                message: Some(format!(
                    "GUID {:#x} does not fit a {}-bit pointer; refusing to truncate",
                    self.0,
                    usize::BITS
                )),
            }),
        }
    }
}

impl From<u64> for Guid {
    #[inline]
    fn from(g: u64) -> Guid {
        Guid(g)
    }
}

impl From<Guid> for u64 {
    #[inline]
    fn from(g: Guid) -> u64 {
        g.0
    }
}

impl IntoLua for Guid {
    /// `0` → `nil` (the scripts' `if not uGuid`), everything else → lightuserdata (retail's tag 2).
    #[inline]
    fn into_lua(self, _lua: &Lua) -> LuaResult<Value> {
        if self.0 == 0 {
            return Ok(Value::Nil);
        }
        Ok(Value::LightUserData(LightUserData(self.as_ptr()?)))
    }
}

impl FromLua for Guid {
    /// Lightuserdata is the real shape; integers/numbers are accepted **transitionally** while the
    /// binding surface converts namespace by namespace; everything else (including `nil` and an
    /// absent argument) is [`Guid::NONE`], mirroring `FUN_0059FF50`'s "return 0, never raise".
    #[inline]
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Guid> {
        Ok(Guid::from_value(&value))
    }
}

/// Render a handle the way a log line wants it: the decimal value, or the empty string for `NONE`.
///
/// The recorded-command log (`EngineHost::script_cmd`) stringifies its arguments, and a handle that
/// used to arrive as `Value::Integer` now arrives as `Value::LightUserData`. Without this the log
/// would silently lose every GUID it prints. See [`crate::bindings::stringify_arg`].
pub fn stringify_light_userdata(ud: LightUserData) -> String {
    (ud.0 as usize as u64).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole objective: a `type(u) == "userdata"` gate passes against a GUID we push, and the
    /// value survives a round trip back into a binding argument.
    #[test]
    fn guid_is_userdata_to_lua_and_round_trips() {
        let lua = Lua::new();
        let f = lua
            .create_function(|_, ()| Ok(Guid(0x1000_0007)))
            .unwrap();
        lua.globals().set("GetGuid", f).unwrap();
        let echo = lua.create_function(|_, g: Guid| Ok(g.raw() as i64)).unwrap();
        lua.globals().set("Echo", echo).unwrap();

        let ty: String = lua.load("return type(GetGuid())").eval().unwrap();
        assert_eq!(ty, "userdata", "retail pushes handles as lightuserdata (tag 2)");
        let back: i64 = lua.load("return Echo(GetGuid())").eval().unwrap();
        assert_eq!(back, 0x1000_0007);
    }

    /// `0` is `nil`, not a NULL lightuserdata — `if not uGuid then` has to see a falsey value.
    #[test]
    fn zero_is_nil_not_null_lightuserdata() {
        let lua = Lua::new();
        let f = lua.create_function(|_, ()| Ok(Guid::NONE)).unwrap();
        lua.globals().set("Miss", f).unwrap();
        let ty: String = lua.load("return type(Miss())").eval().unwrap();
        assert_eq!(ty, "nil");
        let falsey: bool = lua.load("if not Miss() then return true end return false").eval().unwrap();
        assert!(falsey);
    }

    /// The nil-handle contract: a missing/nil/garbage argument is `NONE`, never a raised error.
    #[test]
    fn absent_or_junk_argument_is_none_not_an_error() {
        let lua = Lua::new();
        let f = lua.create_function(|_, g: Guid| Ok(g.is_none())).unwrap();
        lua.globals().set("IsNone", f).unwrap();
        for src in [
            "return IsNone()",
            "return IsNone(nil)",
            "return IsNone(\"not a handle\")",
            "return IsNone({})",
            "return IsNone(-1)",
        ] {
            let r: bool = lua.load(src).eval().unwrap_or_else(|e| panic!("{src}: {e}"));
            assert!(r, "{src} should yield Guid::NONE, not raise");
        }
    }

    /// Lightuserdata is hashable and compares by pointer, so the corpus's 402 GUID-as-table-key
    /// sites keep working.
    #[test]
    fn guids_are_stable_table_keys_and_compare_by_value() {
        let lua = Lua::new();
        let f = lua.create_function(|_, ()| Ok(Guid(0x1000_0042))).unwrap();
        lua.globals().set("G", f).unwrap();
        let ok: bool = lua
            .load("local t = {} t[G()] = 'v' return t[G()] == 'v' and G() == G()")
            .eval()
            .unwrap();
        assert!(ok);
    }

    /// Transitional acceptance: an integer handle from a not-yet-converted namespace still reads.
    #[test]
    fn integers_are_still_accepted_while_the_surface_converts() {
        let lua = Lua::new();
        let f = lua.create_function(|_, g: Guid| Ok(g.raw() as i64)).unwrap();
        lua.globals().set("Echo", f).unwrap();
        let v: i64 = lua.load("return Echo(268435456)").eval().unwrap();
        assert_eq!(v, 0x1000_0000);
    }

    /// A `u64` handle survives the pointer round trip on this target. On 64-bit that is the full
    /// range; on 32-bit the high case is refused rather than truncated (see [`Guid::as_ptr`]).
    #[test]
    fn pointer_round_trip_is_lossless_for_every_handle_this_target_can_carry() {
        for g in [1u64, 2, 0x1000_0000, 0xF000_0000, u32::MAX as u64] {
            let ptr = Guid(g).as_ptr().expect("32-bit-safe handle must convert");
            assert_eq!(ptr as usize as u64, g);
        }
        let wide = Guid(0x0123_4567_89AB_CDEF);
        match wide.as_ptr() {
            Ok(p) => {
                assert_eq!(usize::BITS, 64, "only a 64-bit target may accept a wide handle");
                assert_eq!(p as usize as u64, wide.raw());
            }
            Err(e) => {
                assert!(usize::BITS < 64);
                assert!(format!("{e}").contains("refusing to truncate"));
            }
        }
    }
}
