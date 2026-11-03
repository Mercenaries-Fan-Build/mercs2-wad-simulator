//! `m2_render.h`: the game's render sites a fix detours or writes, resolved in the running image
//! (`mercs2_formats::tree_sites`) so the fix arms on every build the signatures match.

use crate::{rt_log, sites};
use core::ffi::{c_char, c_int};
use std::ffi::CString;
use std::sync::Mutex;

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

static LAST_ERROR: Mutex<Option<CString>> = Mutex::new(None);

fn fail(msg: String) {
    rt_log::line(&format!("ERROR {msg}"));
    let c = CString::new(msg.replace('\0', " ")).expect("NULs replaced");
    *LAST_ERROR.lock().expect("last error lock") = Some(c);
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

/// The message of the last failure a render call recorded, or NULL. Valid until the next failure.
#[unsafe(no_mangle)]
pub extern "C" fn m2_render_last_error() -> *const c_char {
    LAST_ERROR.lock().expect("last error lock").as_ref().map_or(core::ptr::null(), |c| c.as_ptr())
}
