//! Attribution-wrapped calls into Lua — the reimpl-side of the seam-hardening doc's F5.
//!
//! # The problem
//!
//! Every native call *into* Lua runs script author code we do not control. In retail, an error
//! in one of those callbacks gets swallowed by a native `pcall` frame with no visible trace —
//! half the pain of Mercenaries 2 modding is that a mod stops working with no line to grep for.
//! See `docs/modding/lua_engine_seam_hardening.md#f5--pcall-swallowed-callback-errors` and the
//! failure classes it enumerates.
//!
//! # What this module does
//!
//! A single-call helper — [`call_attributed`] and its ergonomic method-call form on
//! [`LuaFnExt`] — that a call site wraps around `Function::call`. When the call throws it emits
//! a `[mod-crash] <site>: <error>` line through the active host's
//! [`EngineHost::log_mod_crash`](crate::EngineHost::log_mod_crash) sink **before** propagating
//! the error, so callers see the error land somewhere visible without any change to their own
//! error-handling.
//!
//! The active host is a **thread-local** ([`ACTIVE_HOST`]) installed by
//! [`crate::ScriptHost::register_engine`] at boot. This avoids threading a `&SharedHost`
//! through every internal fn — the crate has 14 scattered call-into-Lua sites and adding a
//! parameter to each one (plus their internal helpers) would cascade into a signature change
//! across the crate boundary. mlua's `Lua` is `!Send + !Sync`, so this crate is single-
//! threaded; the TL is the natural fit.
//!
//! # What it does NOT do (yet)
//!
//! - Attribute to a **specific Shipment**. That needs a module-source → shipment-id map
//!   populated by the mod loader; the loader is future work (`Mercs2.Mods.register`). Until
//!   then the site string ("`Event.TimerRelative`", "`Hud.MovieEnd`", …) is the attribution.
//! - Walk the Lua traceback. `debug.traceback` is available; a follow-up will fold it into the
//!   emitted message. For now the site + the propagated error carry enough for a modder to
//!   locate the failure in their editor.
//! - Quarantine the mod's callbacks. Also future work; the seam-hardening doc calls it out as
//!   part of the same F5 fix but it needs the shipment map first.
//!
//! # Retail-side counterpart
//!
//! `pmc_bb.dll`'s MinHook on `luaD_pcall` @ `0x00868AD0` (or `lua_pcall` @ `0x0085DF50`) is the
//! natural retail-side twin — same shape, different machinery. Both emit the same
//! `[mod-crash] …` line so a Shipment author sees one format regardless of which build ran.
//! **The seam-hardening doc previously named `FUN_004b2a50`** as the hook site; that address
//! is a 27-byte push-nil-return helper, not the C→Lua dispatch — corrected here.

use crate::SharedHost;
use mercs2_luac::rt::{FromLuaMulti, Function, IntoLuaMulti, Result as LuaResult};
use std::cell::RefCell;

thread_local! {
    /// The active host for this thread. Set once by
    /// [`crate::ScriptHost::register_engine`] at boot and left in place; the crate is
    /// single-threaded so no lifecycle management is required. `None` before registration
    /// (which is the shape unit tests without `register_engine` present) or between
    /// `unregister` / re-`register` — in that case [`call_attributed`] degrades to a plain
    /// `Function::call` with no logging side effect.
    static ACTIVE_HOST: RefCell<Option<SharedHost>> = const { RefCell::new(None) };
}

/// Install `host` as the active host for attribution logging in this thread. Idempotent — a
/// second call replaces the first. [`crate::ScriptHost::register_engine`] calls this on the
/// caller's behalf, so binding-surface consumers do not need to invoke it directly.
pub fn set_active_host(host: SharedHost) {
    ACTIVE_HOST.with(|slot| *slot.borrow_mut() = Some(host));
}

/// Clear the active host. Used by test scaffolding that wants a clean thread-local slate; not
/// used in production, where `set_active_host` is the last event of `register_engine`.
pub fn clear_active_host() {
    ACTIVE_HOST.with(|slot| *slot.borrow_mut() = None);
}

/// Call `f(args)` with attribution logging on failure. Returns whatever `f.call` returns; the
/// only observable side effect on the happy path is a virtual call through `Function::call`.
///
/// `site` is the arbitrary label a caller uses to identify **which** callback path this was —
/// "`Event.TimerRelative`", "`Hud.SetMovieEndCallback`", "`_MODULES._Init`", and so on. Kept as
/// `&'static str` because every site is a compile-time literal at the call point; letting a
/// caller build one dynamically would defeat the purpose.
///
/// Errors: bubbles the original `LuaResult` unchanged after logging. Callers using `?` continue
/// to see propagation semantics they already have; the log line is a side channel.
///
/// When no host is registered on this thread ([`ACTIVE_HOST`] is `None`), degrades to a plain
/// `f.call::<T>(args)` with no logging — matches the test shape where the crate is exercised
/// without a full host wire-up.
pub fn call_attributed<T>(
    f: &Function,
    args: impl IntoLuaMulti,
    site: &'static str,
) -> LuaResult<T>
where
    T: FromLuaMulti,
{
    match f.call::<T>(args) {
        Ok(v) => Ok(v),
        Err(e) => {
            emit_mod_crash(site, &e);
            Err(e)
        }
    }
}

/// Emit a `[mod-crash] <site>: <err>` line to the active host, if one is registered and not
/// already borrowed. `try_borrow_mut` rather than `borrow_mut`: a callback can (and does) call
/// synchronously back into a cfunc that holds `host.borrow_mut()`, and a straight `borrow_mut`
/// here would panic. When the host is busy we drop the log line — the caller still sees the
/// error propagate.
fn emit_mod_crash(site: &str, err: &mercs2_luac::rt::Error) {
    // The `.cloned()` lifts the shared handle out of the TL closure so the resulting `RefMut`
    // is not tied to a borrow of the TL slot — a nested lifetime the compiler cannot prove
    // safe. `Rc::clone` is a refcount bump; the flow works the same as the natural version.
    let host_opt = ACTIVE_HOST.with(|slot| slot.borrow().as_ref().cloned());
    let Some(host) = host_opt else { return };
    let msg = format!("[mod-crash] {site}: {err}");
    log_into(&host, &msg);
}

/// Take an owned host reference and try to log through it. Kept in its own function so the
/// `RefMut` borrow returned by `try_borrow_mut` is scoped tightly to this fn's body — inlining
/// it back into `emit_mod_crash` trips E0597 on the drop-order of the borrow against the local
/// `host` Rc, since `dyn EngineHost` is unsized and its `RefMut` destructor keeps the borrow
/// alive past the outer `if let`.
fn log_into(host: &SharedHost, msg: &str) {
    if let Ok(mut h) = host.try_borrow_mut() {
        h.log_mod_crash(msg);
    }
}

/// Method-call sugar for [`call_attributed`]. `f.call_attr(args, site)` reads more like the
/// native `f.call::<T>(args)` the call sites already use, and drops the `&f` line per site.
pub trait LuaFnExt {
    /// Ergonomic wrapper for [`call_attributed`]. See there for semantics.
    fn call_attr<T>(&self, args: impl IntoLuaMulti, site: &'static str) -> LuaResult<T>
    where
        T: FromLuaMulti;
}

impl LuaFnExt for Function {
    fn call_attr<T>(&self, args: impl IntoLuaMulti, site: &'static str) -> LuaResult<T>
    where
        T: FromLuaMulti,
    {
        call_attributed::<T>(self, args, site)
    }
}
