//! Load-phase triggers (`m2/m2_loadtrigger.h`): callbacks fired once when the world load first reaches a
//! phase of loadprobe's ladder, matched against the shared log stream.

use crate::loghook;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use mercs2_formats::load_ladder as ladder;
use std::ffi::CString;
use std::sync::OnceLock;

/// Called with the phase whose marker appeared, which is at or past the registered target.
pub type PhaseCb = unsafe extern "C" fn(reached_idx: c_int, ud: *mut c_void);

const MAX_TRIGGERS: usize = 32;

/// `cb` is published last; null means the slot is claimed but not ready.
struct Trigger {
    target: AtomicI32,
    cb: AtomicPtr<c_void>,
    ud: AtomicPtr<c_void>,
    fired: AtomicBool,
}

static TRIGGERS: [Trigger; MAX_TRIGGERS] = [const {
    Trigger {
        target: AtomicI32::new(0),
        cb: AtomicPtr::new(core::ptr::null_mut()),
        ud: AtomicPtr::new(core::ptr::null_mut()),
        fired: AtomicBool::new(false),
    }
}; MAX_TRIGGERS];
static TRIGGER_COUNT: AtomicI32 = AtomicI32::new(0);
static MAX_PHASE: AtomicI32 = AtomicI32::new(-1);
static LISTENER_ADDED: AtomicBool = AtomicBool::new(false);
static NAMES: OnceLock<Vec<CString>> = OnceLock::new();

/// Fire once, on whichever thread gets there first.
fn fire_once(t: &Trigger, reached: i32) {
    if t.fired.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
        let cb = t.cb.load(Ordering::Acquire);
        if !cb.is_null() {
            let cb: PhaseCb = unsafe { core::mem::transmute(cb) };
            unsafe { cb(reached, t.ud.load(Ordering::Relaxed)) };
        }
    }
}

/// Register `cb` for the load reaching `target_idx` or later. A phase already reached fires at once.
/// 0 for a null `cb`, an index outside the ladder, or a full table (32).
#[unsafe(no_mangle)]
pub extern "C" fn m2_loadtrigger_on_phase(target_idx: c_int, cb: Option<PhaseCb>, ud: *mut c_void) -> c_int {
    let Some(cb) = cb else { return 0 };
    if usize::try_from(target_idx).map_or(true, |i| i >= ladder::count()) {
        return 0;
    }
    let slot = TRIGGER_COUNT.fetch_add(1, Ordering::AcqRel);
    let Some(t) = usize::try_from(slot).ok().and_then(|s| TRIGGERS.get(s)) else { return 0 };
    t.target.store(target_idx, Ordering::Relaxed);
    t.ud.store(ud, Ordering::Relaxed);
    t.fired.store(false, Ordering::Relaxed);
    t.cb.store(cb as *mut c_void, Ordering::Release);
    let reached = MAX_PHASE.load(Ordering::Acquire);
    if reached >= target_idx {
        fire_once(t, reached);
    }
    1
}

/// The highest phase reached so far, -1 before any.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loadtrigger_current_phase() -> c_int {
    MAX_PHASE.load(Ordering::Acquire)
}

/// The phase's name, or `"?"` for an index outside the ladder.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loadtrigger_phase_name(idx: c_int) -> *const c_char {
    let names = NAMES.get_or_init(|| {
        (0..ladder::count() as i32)
            .map(|i| CString::new(ladder::phase_name(i).expect("index inside the ladder")).expect("no NUL in a phase name"))
            .collect()
    });
    usize::try_from(idx).ok().and_then(|i| names.get(i)).map_or(c"?".as_ptr(), |n| n.as_ptr())
}

unsafe extern "C" fn on_log_line(msg: *const c_char, _ud: *mut c_void) {
    let Some(matched) = ladder::match_phase(unsafe { CStr::from_ptr(msg) }.to_bytes()) else { return };
    let matched = matched as i32;
    if MAX_PHASE.fetch_max(matched, Ordering::AcqRel) >= matched {
        return;
    }
    let n = (TRIGGER_COUNT.load(Ordering::Acquire).max(0) as usize).min(MAX_TRIGGERS);
    for t in &TRIGGERS[..n] {
        if !t.cb.load(Ordering::Acquire).is_null() && t.target.load(Ordering::Relaxed) <= matched {
            fire_once(t, matched);
        }
    }
    // The ladder's last phase: no later line can fire anything, and a trigger registered from now on
    // fires at registration.
    if matched as usize + 1 == ladder::count() {
        loghook::m2_loghook_set_active(Some(on_log_line), core::ptr::null_mut(), 0);
    }
}

/// Subscribe the one process-wide listener and start the log source. Idempotent.
#[unsafe(no_mangle)]
pub extern "C" fn m2_loadtrigger_install() -> c_int {
    if LISTENER_ADDED.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok()
        && loghook::m2_loghook_add_listener(Some(on_log_line), core::ptr::null_mut()) == 0
    {
        LISTENER_ADDED.store(false, Ordering::Release);
        return 0;
    }
    loghook::m2_loghook_install()
}
