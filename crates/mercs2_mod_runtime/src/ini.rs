//! `m2_ini_parse`, `m2_ini_bool`, `m2_ini_int` (`m2/m2_ini.h`); the parsing rules are `mercs2_formats::ini`.

use core::ffi::{CStr, c_char, c_int, c_void};
use mercs2_formats::ini;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileSize, INVALID_FILE_SIZE, OPEN_EXISTING,
    ReadFile,
};

/// Called once per `key = value` line, both trimmed and NUL-terminated.
pub type IniKv = unsafe extern "C" fn(ud: *mut c_void, key: *const c_char, value: *const c_char);

fn with_nul(s: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s);
    v.push(0);
    v
}

/// Read the file at `path` and call `cb` for each key. 1 when the file was read; 0 when it is missing,
/// unreadable, empty or larger than 1 MiB.
///
/// # Safety
/// `path` is null or a NUL-terminated string; `cb` is safe to call with `ud`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_ini_parse(path: *const c_char, cb: Option<IniKv>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    if path.is_null() {
        return 0;
    }
    let hf = unsafe {
        CreateFileA(
            path.cast(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if hf == INVALID_HANDLE_VALUE {
        return 0;
    }
    let size = unsafe { GetFileSize(hf, core::ptr::null_mut()) };
    if size == INVALID_FILE_SIZE || size == 0 || size as usize > ini::MAX_FILE {
        unsafe { CloseHandle(hf) };
        return 0;
    }
    let mut data = vec![0u8; size as usize];
    let mut read = 0u32;
    if unsafe { ReadFile(hf, data.as_mut_ptr(), size, &mut read, core::ptr::null_mut()) } == 0 {
        read = 0;
    }
    unsafe { CloseHandle(hf) };
    data.truncate(read as usize);
    ini::parse(&data, |k, v| {
        let (k, v) = (with_nul(k), with_nul(v));
        unsafe { cb(ud, k.as_ptr().cast(), v.as_ptr().cast()) };
    });
    1
}

/// 1 for `1`, `true`, `on`, `yes` in any case; 0 otherwise, a null `value` included.
///
/// # Safety
/// `value` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_ini_bool(value: *const c_char) -> c_int {
    if value.is_null() {
        return 0;
    }
    c_int::from(ini::bool_value(unsafe { CStr::from_ptr(value) }.to_bytes()))
}

/// The leading decimal integer of `value`, or `fallback` when it has none or is null.
///
/// # Safety
/// `value` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_ini_int(value: *const c_char, fallback: c_int) -> c_int {
    if value.is_null() {
        return fallback;
    }
    ini::int_value(unsafe { CStr::from_ptr(value) }.to_bytes(), fallback)
}
