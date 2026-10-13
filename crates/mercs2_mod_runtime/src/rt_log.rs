//! m2-sdk.dll's own log, `m2-sdk.log` beside the DLL (the game root): what the runtime resolved, hooked and
//! refused, so a failure is never silent even when no mod logs it.

use crate::log;
use std::sync::Once;
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleHandleExA,
};

static OPEN: Once = Once::new();

fn own_module() -> HMODULE {
    let mut module: HMODULE = core::ptr::null_mut();
    unsafe {
        GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            own_module as *const u8,
            &mut module,
        )
    };
    module
}

pub(crate) fn line(text: &str) {
    OPEN.call_once(|| log::m2_log_init(own_module()));
    log::write_line(own_module(), text.as_bytes());
}
