//! `m2_version_num` / `m2_version_string` (`m2/m2_version.h`): what the loaded DLL reports, which a mod
//! compares against the header it compiled with (`m2_abi_ok`).

use core::ffi::{c_char, c_int};

mod built {
    include!(concat!(env!("OUT_DIR"), "/version.rs"));
}

/// MAJOR * 10000 + MINOR * 100 + PATCH.
#[unsafe(no_mangle)]
pub extern "C" fn m2_version_num() -> c_int {
    built::NUM
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_version_string() -> *const c_char {
    built::STRING.as_ptr()
}
