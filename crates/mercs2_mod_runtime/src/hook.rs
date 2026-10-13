//! `m2_hook_init` / `m2_hook_attach` (`m2/m2_hook.h`) over the one MinHook instance in the process.

use core::ffi::{c_int, c_void};

const MH_OK: i32 = 0;
const MH_ERROR_ALREADY_INITIALIZED: i32 = 1;

unsafe extern "system" {
    fn MH_Initialize() -> i32;
    fn MH_CreateHook(target: *mut c_void, detour: *mut c_void, original: *mut *mut c_void) -> i32;
    fn MH_EnableHook(target: *mut c_void) -> i32;
}

/// Initialise MinHook once; an already-initialised MinHook is success.
#[unsafe(no_mangle)]
pub extern "C" fn m2_hook_init() -> c_int {
    let st = unsafe { MH_Initialize() };
    c_int::from(st == MH_OK || st == MH_ERROR_ALREADY_INITIALIZED)
}

/// Create and enable a detour at `target`. MinHook writes the trampoline to `*orig` while creating the
/// hook, before it is enabled, so the detour never runs with `*orig` unset. `orig` may be null.
///
/// # Safety
/// `target` is the start of a function, `detour` a function of the same signature, `orig` null or
/// writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_hook_attach(target: *mut c_void, detour: *mut c_void, orig: *mut *mut c_void) -> c_int {
    c_int::from(unsafe { attach(target, detour, orig) })
}

/// # Safety
/// As [`m2_hook_attach`].
pub(crate) unsafe fn attach(target: *mut c_void, detour: *mut c_void, orig: *mut *mut c_void) -> bool {
    m2_hook_init() == 1
        && unsafe { MH_CreateHook(target, detour, orig) } == MH_OK
        && unsafe { MH_EnableHook(target) } == MH_OK
}
