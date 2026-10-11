//! Per-module log files (`m2/m2_log.h`): `m2_log_init` opens `<module>.log` next to the module, keyed by
//! its HMODULE, so every mod writes its own file through the one shared DLL.

use core::ffi::{CStr, c_char, c_int};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, HMODULE, INVALID_HANDLE_VALUE, MAX_PATH};
use windows_sys::Win32::Storage::FileSystem::{
    CREATE_ALWAYS, CreateFileA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FlushFileBuffers, WriteFile,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameA;

const MAX_MODULES: usize = 32;
/// One line, CRLF included.
const LINE_MAX: usize = 1536;

#[link(name = "user32")]
unsafe extern "system" {
    fn wvsprintfA(out: *mut c_char, fmt: *const c_char, args: *const c_char) -> c_int;
}

/// `(HMODULE, HANDLE)` as addresses, so the table can live in a static.
static SLOTS: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

fn file_for(module: HMODULE) -> Option<HANDLE> {
    let slots = SLOTS.lock().expect("log table");
    slots.iter().find(|s| s.0 == module as usize).map(|s| s.1 as HANDLE)
}

/// The full path of `module` (null: the exe) as GetModuleFileNameA reports it, without the NUL.
pub(crate) fn module_file_name(module: HMODULE) -> Vec<u8> {
    let mut buf = [0u8; MAX_PATH as usize];
    let n = unsafe { GetModuleFileNameA(module, buf.as_mut_ptr(), MAX_PATH) } as usize;
    buf[..n.min(buf.len())].to_vec()
}

/// The directory of `module`, ending in its separator; empty when the path has none.
pub(crate) fn module_dir(module: HMODULE) -> Vec<u8> {
    let mut dir = module_file_name(module);
    let slash = dir.iter().rposition(|&b| b == b'\\').or_else(|| dir.iter().rposition(|&b| b == b'/'));
    dir.truncate(slash.map_or(0, |s| s + 1));
    dir
}

/// Build `<module dir>\<filename>` into `out`. The directory is truncated to fit; the file name is
/// appended only when all of it fits.
///
/// # Safety
/// `filename` is null or a NUL-terminated string; `out` is writable for `out_size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_module_path(module: HMODULE, filename: *const c_char, out: *mut c_char, out_size: c_int) {
    if out.is_null() || out_size <= 0 {
        return;
    }
    let out = unsafe { core::slice::from_raw_parts_mut(out.cast::<u8>(), out_size as usize) };
    let dir = module_dir(module);
    let n = dir.len().min(out.len() - 1);
    out[..n].copy_from_slice(&dir[..n]);
    out[n] = 0;
    let name: &[u8] = if filename.is_null() { b"" } else { unsafe { CStr::from_ptr(filename) }.to_bytes() };
    if n + name.len() < out.len() {
        out[n..n + name.len()].copy_from_slice(name);
        out[n + name.len()] = 0;
    }
}

/// Open `<module>.log` (the module's path with its extension replaced), truncating it. A second call for
/// the same module keeps the first handle.
#[unsafe(no_mangle)]
pub extern "C" fn m2_log_init(module: HMODULE) {
    if file_for(module).is_some() {
        return;
    }
    let mut path = module_file_name(module);
    if let Some(dot) = path.iter().rposition(|&b| b == b'.') {
        path.truncate(dot);
    }
    path.extend_from_slice(b".log\0");
    let file = unsafe {
        CreateFileA(
            path.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_READ,
            core::ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return;
    }
    let mut slots = SLOTS.lock().expect("log table");
    if slots.iter().any(|s| s.0 == module as usize) || slots.len() >= MAX_MODULES {
        drop(slots);
        unsafe { CloseHandle(file) };
        return;
    }
    slots.push((module as usize, file as usize));
}

/// Append `line` and CRLF to the module's log and flush; nothing when the module has no log open.
pub(crate) fn write_line(module: HMODULE, line: &[u8]) {
    let Some(file) = file_for(module) else { return };
    let mut buf = [0u8; LINE_MAX];
    let n = line.len().min(LINE_MAX - 2);
    buf[..n].copy_from_slice(&line[..n]);
    buf[n] = b'\r';
    buf[n + 1] = b'\n';
    let mut written = 0u32;
    unsafe {
        WriteFile(file, buf.as_ptr(), (n + 2) as u32, &mut written, core::ptr::null_mut());
        FlushFileBuffers(file);
    }
}

/// `m2_logf_mod(module, fmt, va)` with the variadic arguments as a `va_list`.
unsafe extern "C" fn logf_va(module: HMODULE, fmt: *const c_char, args: *const c_char) {
    if file_for(module).is_none() || fmt.is_null() {
        return;
    }
    let mut buf = [0u8; LINE_MAX];
    let len = unsafe { wvsprintfA(buf.as_mut_ptr().cast(), fmt, args) };
    if len <= 0 {
        return;
    }
    write_line(module, &buf[..(len as usize).min(LINE_MAX - 2)]);
}

/// `void m2_logf_mod(HMODULE module, const char* fmt, ...)`, cdecl: formats with `wvsprintfA` and writes
/// one line. The entry hands `logf_va` the module, the format and a pointer to the first variadic
/// argument, which is the `va_list` on x86.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_logf_mod() {
    core::arch::naked_asm!(
        "lea eax, [esp+12]",
        "push eax",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "call {logf}",
        "add esp, 12",
        "ret",
        logf = sym logf_va,
    );
}

/// Close the module's log and forget it.
#[unsafe(no_mangle)]
pub extern "C" fn m2_log_close_mod(module: HMODULE) {
    let mut slots = SLOTS.lock().expect("log table");
    let Some(i) = slots.iter().position(|s| s.0 == module as usize) else { return };
    let (_, file) = slots.swap_remove(i);
    drop(slots);
    unsafe { CloseHandle(file as HANDLE) };
}
