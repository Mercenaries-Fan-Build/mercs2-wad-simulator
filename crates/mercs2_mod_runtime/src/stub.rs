//! The one detour on the shared log stub (`print` in the base library's `luaL_Reg` table; `xor eax, eax;
//! ret`, called by `print`, `Debug.Printf` and the stripped subsystem loggers). Two consumers share it:
//! the log listeners ([`crate::loghook`]) and the Lua host's tick ([`crate::lua_host`]). It always chains
//! to the trampoline, which holds another module's jump when that module hooked the stub first.

use crate::{hook, loghook, lua_host, rt_log, sites};
use core::ffi::{c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use mercs2_formats::lua_sites::Site;
use std::sync::Mutex;

type StubFn = unsafe extern "C" fn(l: *mut c_void) -> c_int;

static ORIG: AtomicUsize = AtomicUsize::new(0);
static ATTACHED: Mutex<bool> = Mutex::new(false);
pub(crate) static LOG_ON: AtomicBool = AtomicBool::new(false);
pub(crate) static TICK_ON: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn detour(l: *mut c_void) -> c_int {
    if LOG_ON.load(Ordering::Relaxed) {
        loghook::on_stub(l);
    }
    if TICK_ON.load(Ordering::Relaxed) {
        lua_host::tick(l as u32);
    }
    let orig: StubFn = unsafe { core::mem::transmute(ORIG.load(Ordering::Acquire)) };
    unsafe { orig(l) }
}

/// Attach the detour once the sites are known. Idempotent.
pub(crate) fn attach(sites: &mercs2_formats::lua_sites::LuaSites) -> Result<(), String> {
    let mut attached = ATTACHED.lock().expect("stub lock");
    if *attached {
        return Ok(());
    }
    let target = sites.va(Site::Print) as *mut c_void;
    if !unsafe { hook::attach(target, detour as *mut c_void, ORIG.as_ptr().cast()) } {
        return Err(format!("MinHook could not detour the log stub at {target:p}"));
    }
    *attached = true;
    rt_log::line(&format!("log stub detoured at {target:p}"));
    Ok(())
}

/// Attach now when the sites resolve now, else on a thread that waits for them (the caller may hold the
/// loader lock). The outcome of a deferred attach goes to m2-sdk.log.
pub(crate) fn attach_soon() -> Result<(), String> {
    match sites::now() {
        Ok(s) => attach(s),
        Err(_) => {
            std::thread::Builder::new()
                .name("m2_stub_attach".into())
                .spawn(|| match sites::wait().and_then(attach) {
                    Ok(()) => {}
                    Err(e) => rt_log::line(&format!("ERROR log stub not detoured: {e}")),
                })
                .map(|_| ())
                .map_err(|e| format!("could not start the thread that waits for the game's code: {e}"))
        }
    }
}
