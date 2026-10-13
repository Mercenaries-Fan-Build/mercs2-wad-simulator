//! The Lua argument readers (`m2/m2_luastack.h`) over process memory: `mercs2_formats::lua_stack` with every
//! read guarded by `VirtualQuery`.

use core::ffi::{c_char, c_int, c_void};
use mercs2_formats::lua_stack::{self, Mem};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY,
    PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY, VirtualQuery,
};

pub(crate) struct Process;

impl Mem for Process {
    fn readable_span(&self, addr: u32) -> u32 {
        if addr == 0 {
            return 0;
        }
        let mut mbi: MEMORY_BASIC_INFORMATION = unsafe { core::mem::zeroed() };
        if unsafe { VirtualQuery(addr as *const c_void, &mut mbi, size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
            return 0;
        }
        if mbi.State != MEM_COMMIT || mbi.Protect & (PAGE_NOACCESS | PAGE_GUARD) != 0 {
            return 0;
        }
        let readable = [
            PAGE_READONLY,
            PAGE_READWRITE,
            PAGE_WRITECOPY,
            PAGE_EXECUTE_READ,
            PAGE_EXECUTE_READWRITE,
            PAGE_EXECUTE_WRITECOPY,
        ];
        if !readable.contains(&(mbi.Protect & 0xFF)) {
            return 0;
        }
        let end = mbi.BaseAddress as usize + mbi.RegionSize;
        (end.saturating_sub(addr as usize)) as u32
    }

    fn read_u32(&self, addr: u32) -> u32 {
        unsafe { (addr as *const u32).read_unaligned() }
    }

    fn read_u8(&self, addr: u32) -> u8 {
        unsafe { (addr as *const u8).read() }
    }
}

/// `out` as a slice, or an empty one when it is null or `out_max` is not positive.
///
/// # Safety
/// `out` is null or writable for `out_max` bytes.
unsafe fn out_slice<'a>(out: *mut c_char, out_max: c_int) -> &'a mut [u8] {
    if out.is_null() || out_max <= 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(out.cast(), out_max as usize) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_nargs(l: *mut c_void) -> c_int {
    lua_stack::nargs(&Process, l as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_lua_arg_type(l: *mut c_void, idx0: c_int) -> c_int {
    lua_stack::arg_type(&Process, l as u32, idx0)
}

/// # Safety
/// `out` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_arg_number(l: *mut c_void, idx0: c_int, out: *mut f64) -> c_int {
    let Some(v) = lua_stack::arg_number(&Process, l as u32, idx0) else { return 0 };
    if !out.is_null() {
        unsafe { out.write(v) };
    }
    1
}

/// # Safety
/// `out` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_arg_bool(l: *mut c_void, idx0: c_int, out: *mut c_int) -> c_int {
    let Some(v) = lua_stack::arg_bool(&Process, l as u32, idx0) else { return 0 };
    if !out.is_null() {
        unsafe { out.write(c_int::from(v)) };
    }
    1
}

/// # Safety
/// `out` is null or writable for `out_max` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_arg_string(l: *mut c_void, idx0: c_int, out: *mut c_char, out_max: c_int) -> c_int {
    lua_stack::arg_string(&Process, l as u32, idx0, unsafe { out_slice(out, out_max) })
}

/// # Safety
/// `out` is null or writable for `out_max` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_lua_join_strings(l: *mut c_void, out: *mut c_char, out_max: c_int) -> c_int {
    lua_stack::join_strings(&Process, l as u32, unsafe { out_slice(out, out_max) })
}
