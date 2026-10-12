//! The shader registry (`m2/m2_shader.h`): the detour on FUN_0084f130 that registers every queued entry
//! after the game's own shaders, through FUN_0085ac90 as the game does. Validation, the queue and the
//! admission decision are `mercs2_formats::shader_registry`.

use crate::{hook, log};
use core::cell::UnsafeCell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::{rt_log, sites};
use mercs2_formats::shader_registry::{self as shader, Class, Entry, PIXEL_CLASSES, PS_CAPACITY, ShaderQueue, Stage, Status, VS_CAPACITY};
use mercs2_formats::shader_sites::ShaderSites;
use std::sync::Mutex;
use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleHandleExA,
};

/// FUN_0084f130: `__thiscall(renderer)`, one stack argument, `ret 4`. The detour hands back EAX unchanged.
type RegisterAllFn = unsafe extern "thiscall" fn(renderer: *mut c_void, arg: u32) -> u32;
/// FUN_0085ace0: `__thiscall(record)`, `ret`.
type PsBaseCtorFn = unsafe extern "thiscall" fn(record: *mut u8);
/// FUN_0085ac90: `__thiscall(record, name, sho, class)`, `ret 0xc`.
type RegisterFn = unsafe extern "thiscall" fn(record: *mut u8, name: *const c_char, sho: *const c_char, cls: c_int);

static QUEUE: Mutex<ShaderQueue> = Mutex::new(ShaderQueue::new());
static ORIG_REGISTER_ALL: AtomicUsize = AtomicUsize::new(0);
static HOOKED: AtomicBool = AtomicBool::new(false);
static PASS_STARTED: AtomicBool = AtomicBool::new(false);

const RECORD_ALIGN: usize = 16;
const fn aligned(n: usize) -> usize {
    (n + RECORD_ALIGN - 1) & !(RECORD_ALIGN - 1)
}
/// Every record the registry builds takes one pool slot, so the pool capacities bound the storage.
const ARENA_SIZE: usize =
    PS_CAPACITY as usize * aligned(shader::PS_RECORD_MAX as usize) + VS_CAPACITY as usize * aligned(shader::VS_RECORD_MAX as usize);

/// Records live for the process, as the game's static records do. Only the registry pass touches it,
/// on the game thread, with `QUEUE` held.
#[repr(C, align(16))]
struct Arena(UnsafeCell<[u8; ARENA_SIZE]>);
unsafe impl Sync for Arena {}
static ARENA: Arena = Arena(UnsafeCell::new([0; ARENA_SIZE]));
static ARENA_USED: AtomicUsize = AtomicUsize::new(0);

fn s() -> &'static ShaderSites {
    sites::shaders_now().expect("the registry detour runs only after the shader sites resolved")
}

fn read_u32(va: u32) -> u32 {
    unsafe { (va as *const u32).read_volatile() }
}

fn registry_started(s: &ShaderSites) -> bool {
    PASS_STARTED.load(Ordering::Acquire) || read_u32(s.ps_count) != 0 || read_u32(s.vs_count) != 0
}

/// FUN_0085ade0 takes the record in EAX, writes through it, and clobbers ECX.
unsafe fn call_vs_base_ctor(record: *mut u8) {
    unsafe {
        core::arch::asm!(
            "call {f}",
            f = in(reg) s().vs_base_ctor,
            inout("eax") record => _,
            out("ecx") _,
            out("edx") _,
        );
    }
}

/// A zeroed record of the family's size, built by its stage's base constructor and given the family's
/// vtable as resolved in this image; `None` once the arena is used up.
fn new_record(family: i32, info: &shader::FamilyInfo) -> Option<*mut u8> {
    let size = aligned(info.size as usize);
    let used = ARENA_USED.load(Ordering::Relaxed);
    if ARENA_SIZE - used < size {
        return None;
    }
    ARENA_USED.store(used + size, Ordering::Relaxed);
    let record = unsafe { ARENA.0.get().cast::<u8>().add(used) };
    unsafe {
        core::ptr::write_bytes(record, 0, info.size as usize);
        match info.stage {
            Stage::Pixel => {
                let ctor: PsBaseCtorFn = core::mem::transmute(s().ps_base_ctor as usize);
                ctor(record);
            }
            Stage::Vertex => call_vs_base_ctor(record),
        }
        record.cast::<u32>().write(s().vtable[family as usize]);
    }
    Some(record)
}

fn register_entry(e: &Entry, shader_level: u8) -> Status {
    let info = shader::family(e.family).expect("queued entries have a valid family");
    let (count_va, keys_va, slots) = match info.stage {
        Stage::Pixel => (s().ps_count, s().ps_keys, PS_CAPACITY),
        Stage::Vertex => (s().vs_count, s().vs_keys, VS_CAPACITY),
    };
    let keys = unsafe { core::slice::from_raw_parts(keys_va as *const u32, slots as usize) };
    let mut st = shader::admit(e, shader_level, read_u32(count_va), keys);
    let n = shader::registrations(e, shader_level) as usize;
    let mut records = [core::ptr::null_mut(); PIXEL_CLASSES];
    for r in records.iter_mut().take(n) {
        if st != Status::Ok {
            break;
        }
        match new_record(e.family, info) {
            Some(rec) => *r = rec,
            None => st = Status::ErrCapacity,
        }
    }
    if st == Status::Ok {
        let register: RegisterFn = unsafe { core::mem::transmute(s().register as usize) };
        for (c, &rec) in records.iter().enumerate().take(n) {
            unsafe { register(rec, e.cls[c].name, e.cls[c].sho, c as c_int) };
        }
    } else {
        let stage = match info.stage {
            Stage::Pixel => "pixel",
            Stage::Vertex => "vertex",
        };
        let mut line = format!("m2_shader: {stage} entry \"").into_bytes();
        line.extend_from_slice(unsafe { CStr::from_ptr(e.cls[0].name) }.to_bytes());
        line.extend_from_slice(format!("\" ({}) not registered: ", info.name).as_bytes());
        line.extend_from_slice(shader::status_name(st as i32).expect("a Status has a name").to_bytes());
        log::write_line(e.owner as HMODULE, &line);
    }
    st
}

unsafe extern "thiscall" fn detour_register_all(renderer: *mut c_void, arg: u32) -> u32 {
    PASS_STARTED.store(true, Ordering::Release);
    let orig: RegisterAllFn = unsafe { core::mem::transmute(ORIG_REGISTER_ALL.load(Ordering::Acquire)) };
    let ret = unsafe { orig(renderer, arg) };
    let shader_level = unsafe { (s().shader_level as *const u8).read_volatile() };
    let mut q = QUEUE.lock().expect("shader queue");
    for i in 0..q.len() {
        let e = *q.entry(i).expect("index below len");
        let st = register_entry(&e, shader_level);
        q.record(i, st);
    }
    ret
}

/// The sites resolve from the original bytes, so before the detour is installed; once hooked, only the
/// registry state is re-checked. Resolution does not wait: a mod calls this from its `DllMain`.
fn ensure_installed() -> Status {
    let s = match sites::shaders_now() {
        Ok(s) => s,
        Err(e) => {
            rt_log::line(&format!("ERROR shader registry: {e}"));
            return Status::ErrSignature;
        }
    };
    if HOOKED.load(Ordering::Acquire) {
        return if registry_started(s) { Status::ErrTooLate } else { Status::Ok };
    }
    if registry_started(s) {
        return Status::ErrTooLate;
    }
    let attached = unsafe {
        hook::attach(
            s.register_all as *mut c_void,
            detour_register_all as *mut c_void,
            ORIG_REGISTER_ALL.as_ptr().cast(),
        )
    };
    if !attached {
        return Status::ErrHook;
    }
    HOOKED.store(true, Ordering::Release);
    Status::Ok
}

/// The module holding `addr`, so a failed entry's log line lands in the calling mod's own log.
fn module_at(addr: usize) -> usize {
    let mut module: HMODULE = core::ptr::null_mut();
    unsafe {
        GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr as *const u8,
            &mut module,
        )
    };
    module as usize
}

unsafe extern "C" fn add_pixel_from(family: c_int, classes: *const Class, caller: usize) -> Status {
    let owner = module_at(caller);
    let st = ensure_installed();
    if st != Status::Ok {
        return st;
    }
    unsafe { QUEUE.lock().expect("shader queue").queue_pixel(family, classes, owner) }
}

unsafe extern "C" fn add_vertex_from(family: c_int, cls: *const Class, caller: usize) -> Status {
    let owner = module_at(caller);
    let st = ensure_installed();
    if st != Status::Ok {
        return st;
    }
    unsafe { QUEUE.lock().expect("shader queue").queue_vertex(family, cls, owner) }
}

/// `m2_shader_status m2_shader_add_pixel(m2_shader_family, const m2_shader_class[4])`, cdecl. The entry
/// passes its return address on as the caller.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_shader_add_pixel() {
    core::arch::naked_asm!(
        "push dword ptr [esp]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "call {f}",
        "add esp, 12",
        "ret",
        f = sym add_pixel_from,
    );
}

/// `m2_shader_status m2_shader_add_vertex(m2_shader_family, const m2_shader_class*)`, cdecl. The entry
/// passes its return address on as the caller.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_shader_add_vertex() {
    core::arch::naked_asm!(
        "push dword ptr [esp]",
        "push dword ptr [esp+12]",
        "push dword ptr [esp+12]",
        "call {f}",
        "add esp, 12",
        "ret",
        f = sym add_vertex_from,
    );
}

/// # Safety
/// `name` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn m2_shader_outcome(name: *const c_char) -> Status {
    let name = (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_bytes());
    QUEUE.lock().expect("shader queue").outcome(name)
}

#[unsafe(no_mangle)]
pub extern "C" fn m2_shader_status_name(s: c_int) -> *const c_char {
    shader::status_name(s).map_or(core::ptr::null(), CStr::as_ptr)
}
