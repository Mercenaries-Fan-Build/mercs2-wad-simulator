//! `m2_render.h`: the game's render sites a fix detours or writes, resolved in the running image
//! (`mercs2_formats::tree_sites`, `mercs2_formats::display_mode_sites`) so the fix arms on every build the
//! signatures match.

use crate::{rt_log, sites};
use core::cell::RefCell;
use core::ffi::{c_char, c_int};
use mercs2_formats::mod_runtime::LastError;
use std::ffi::CString;

/// `m2_tree_sites`: every address is the running image's.
#[repr(C)]
pub struct M2TreeSites {
    /// Set by the caller to `sizeof(m2_tree_sites)`, at least [`FIRST_SIZE`]; the DLL fills the
    /// [`FIRST_SIZE`] bytes it knows.
    size: u32,
    zpass_loop: usize,
    shadow_loop: usize,
    identity: usize,
    matrix_copy: usize,
    matrix_index: usize,
    draw_record: usize,
    batches: usize,
    matrices: usize,
    parity: usize,
    matrix_table: usize,
    matrix_cap: u32,
}

/// The struct's size when it was introduced (0.5.0): the least a caller may ask for.
const FIRST_SIZE: usize = core::mem::size_of::<M2TreeSites>();

thread_local! {
    static LAST_ERROR_SLOT: RefCell<Option<CString>> = const { RefCell::new(None) };
}
/// [`m2_render_last_error`]'s message, one per thread.
static LAST_ERROR: LastError = LastError::new(&LAST_ERROR_SLOT);

fn fail(msg: String) {
    rt_log::line(&format!("ERROR {msg}"));
    LAST_ERROR.set(&msg);
}

fn tree_sites(out: *mut M2TreeSites) -> Result<(), String> {
    if out.is_null() {
        return Err("out is NULL".into());
    }
    let size = unsafe { core::ptr::addr_of!((*out).size).read_unaligned() } as usize;
    if size < FIRST_SIZE {
        return Err(format!("out->size is {size}, less than the {FIRST_SIZE} bytes of m2_tree_sites"));
    }
    let t = sites::trees_wait()?;
    let filled = M2TreeSites {
        size: size as u32,
        zpass_loop: t.zpass_loop as usize,
        shadow_loop: t.shadow_loop as usize,
        identity: t.identity as usize,
        matrix_copy: t.matrix_copy as usize,
        matrix_index: t.matrix_index as usize,
        draw_record: t.draw_record as usize,
        batches: t.batches as usize,
        matrices: t.matrices as usize,
        parity: t.parity as usize,
        matrix_table: t.matrix_table as usize,
        matrix_cap: t.matrix_cap,
    };
    unsafe { core::ptr::copy_nonoverlapping((&filled as *const M2TreeSites).cast::<u8>(), out.cast::<u8>(), FIRST_SIZE) };
    Ok(())
}

/// Fill `out` with the billboard-tree render sites, resolving them first (waiting up to 30 s for SecuROM
/// to decrypt the game's code). 1 on success; 0 with [`m2_render_last_error`] set and `out` untouched.
/// Not from `DllMain`.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_tree_sites(out: *mut M2TreeSites) -> c_int {
    match tree_sites(out) {
        Ok(()) => 1,
        Err(e) => {
            fail(format!("m2_render_tree_sites: {e}"));
            0
        }
    }
}

/// `m2_display_mode_sites`: every address is the running image's.
#[repr(C)]
pub struct M2DisplayModeSites {
    /// Set by the caller to `sizeof(m2_display_mode_sites)`, at least [`MODES_FIRST_SIZE`].
    size: u32,
    refresh_cmp: usize,
    skip: usize,
    keep: usize,
    /// 1 when the loop's index is `EBX`; 0 when it is the dword at `[esp + index_stack_offset]`.
    index_in_ebx: u32,
    index_stack_offset: u32,
    find_mode: usize,
    first_search_call: usize,
    fallback_call: usize,
    set_mode: usize,
    pending_state: usize,
    settings_width: usize,
    settings_height: usize,
    settings_bpp_index: usize,
    settings_refresh: usize,
    list_offset: u32,
    count_offset: u32,
    adapter_offset: u32,
}

/// The struct's size when it was introduced (0.6.1): the least a caller may ask for.
const MODES_FIRST_SIZE: usize = core::mem::size_of::<M2DisplayModeSites>();

fn display_mode_sites(out: *mut M2DisplayModeSites) -> Result<(), String> {
    use mercs2_formats::display_mode_sites::ModeIndex;
    if out.is_null() {
        return Err("out is NULL".into());
    }
    let size = unsafe { core::ptr::addr_of!((*out).size).read_unaligned() } as usize;
    if size < MODES_FIRST_SIZE {
        return Err(format!("out->size is {size}, less than the {MODES_FIRST_SIZE} bytes of m2_display_mode_sites"));
    }
    let m = sites::modes_wait()?;
    let (index_in_ebx, index_stack_offset) = match m.index {
        ModeIndex::Ebx => (1, 0),
        ModeIndex::Stack(d) => (0, u32::from(d)),
    };
    let filled = M2DisplayModeSites {
        size: size as u32,
        refresh_cmp: m.refresh_cmp as usize,
        skip: m.skip as usize,
        keep: m.keep as usize,
        index_in_ebx,
        index_stack_offset,
        find_mode: m.find_mode as usize,
        first_search_call: m.first_search_call as usize,
        fallback_call: m.fallback_call as usize,
        set_mode: m.set_mode as usize,
        pending_state: m.pending_state as usize,
        settings_width: m.settings.width as usize,
        settings_height: m.settings.height as usize,
        settings_bpp_index: m.settings.bpp_index as usize,
        settings_refresh: m.settings.refresh as usize,
        list_offset: m.list_offset,
        count_offset: m.count_offset,
        adapter_offset: m.adapter_offset,
    };
    unsafe { core::ptr::copy_nonoverlapping((&filled as *const M2DisplayModeSites).cast::<u8>(), out.cast::<u8>(), MODES_FIRST_SIZE) };
    Ok(())
}

/// Fill `out` with the display-mode sites, resolving them first (waiting up to 30 s for SecuROM to decrypt the
/// game's code). 1 on success; 0 with [`m2_render_last_error`] set and `out` untouched. Not from `DllMain`.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_display_mode_sites(out: *mut M2DisplayModeSites) -> c_int {
    match display_mode_sites(out) {
        Ok(()) => 1,
        Err(e) => {
            fail(format!("m2_render_display_mode_sites: {e}"));
            0
        }
    }
}

fn display_mode_filter(at: usize, max_refresh: u32, max_index: u32, code: *mut u8, code_cap: u32, patch: *mut u8) -> Result<u32, String> {
    use mercs2_formats::display_mode_sites::{filter_code, filter_patch, FILTER_PATCH_BYTES};
    if code.is_null() || patch.is_null() {
        return Err("code or patch is NULL".into());
    }
    let m = sites::modes_resolved().ok_or("the display-mode sites have not resolved; call m2_render_display_mode_sites first")?;
    let c = filter_code(m, at as u32, max_refresh, max_index);
    if c.len() > code_cap as usize {
        return Err(format!("the code is {} bytes, more than code_cap {code_cap}", c.len()));
    }
    let p = filter_patch(m, at as u32);
    unsafe {
        core::ptr::copy_nonoverlapping(c.as_ptr(), code, c.len());
        core::ptr::copy_nonoverlapping(p.as_ptr(), patch, FILTER_PATCH_BYTES);
    }
    Ok(c.len() as u32)
}

/// Assemble the mode-list filter's replacement for address `at`: code that drops a mode whose refresh rate is
/// above `max_refresh` or whose enumeration index is above `max_index` and keeps every other mode without
/// the aspect test, into `code` (at most `code_cap` bytes); and into `patch` the 9 bytes that replace
/// `refresh_cmp` with a jump to `at`. Returns the code's length; 0 with [`m2_render_last_error`] set when the
/// sites have not resolved or `code_cap` is too small.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_display_mode_filter(at: *mut core::ffi::c_void, max_refresh: u32, max_index: u32, code: *mut u8, code_cap: u32, patch: *mut u8) -> u32 {
    match display_mode_filter(at as usize, max_refresh, max_index, code, code_cap, patch) {
        Ok(n) => n,
        Err(e) => {
            fail(format!("m2_render_display_mode_filter: {e}"));
            0
        }
    }
}

/// The position in the game's mode list (`count` 8-byte records at `list`) of the mode BUG-016 uses when the
/// ini mode is not listed: the `width`×`height` mode, else the desktop's (`desktop_width`×`desktop_height`;
/// 0×0 when unknown), each at its refresh rate closest to 60 Hz. -1 when the list has neither; sets
/// `*used_desktop` (when not NULL) to whether the desktop resolution was the one found.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_pick_display_mode(
    list: *const u8,
    count: u32,
    width: u32,
    height: u32,
    desktop_width: u32,
    desktop_height: u32,
    used_desktop: *mut c_int,
) -> c_int {
    use mercs2_formats::display_mode::{pick, records, Pick, RECORD_BYTES};
    if list.is_null() && count != 0 {
        fail("m2_render_pick_display_mode: list is NULL".into());
        return -1;
    }
    let bytes = if count == 0 { &[][..] } else { unsafe { core::slice::from_raw_parts(list, count as usize * RECORD_BYTES) } };
    let modes = records(bytes, count as usize).expect("the slice holds count records");
    let desktop = (desktop_width != 0 && desktop_height != 0).then_some((desktop_width, desktop_height));
    let p = pick(&modes, (width, height), desktop);
    if !used_desktop.is_null() {
        unsafe { *used_desktop = c_int::from(matches!(p, Some(Pick::Desktop(_)))) };
    }
    p.map_or(-1, |p| p.position() as c_int)
}

/// The message of the last failure a render call recorded on the calling thread, or NULL. Valid until that
/// thread's next failure; another thread's failure never touches it.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_last_error() -> *const c_char {
    LAST_ERROR.get()
}
