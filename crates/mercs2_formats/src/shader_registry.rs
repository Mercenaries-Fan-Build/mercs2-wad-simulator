//! The shader registry behind m2-sdk.dll's `m2_shader_*` (`m2/m2_shader.h`), minus the engine calls: the
//! family table, the pool key lookup, call-time validation and the queue, the registry pass's admission
//! decision, and the per-name outcome. The DLL supplies the engine calls and the detour on FUN_0084f130.

use crate::hash::pandemic_hash_m2_bytes;
use core::ffi::{CStr, c_char};

/// `m2_shader_status` (`m2/m2_shader_types.h`); the discriminants are the C enumerator values.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    ErrSignature = 1,
    ErrHook = 2,
    ErrTooLate = 3,
    ErrArgument = 4,
    ErrDuplicate = 5,
    ErrFamily = 6,
    ErrCapacity = 7,
}

const STATUS_NAMES: [&CStr; 8] = [
    c"M2_SHADER_OK",
    c"M2_SHADER_ERR_SIGNATURE",
    c"M2_SHADER_ERR_HOOK",
    c"M2_SHADER_ERR_TOO_LATE",
    c"M2_SHADER_ERR_ARGUMENT",
    c"M2_SHADER_ERR_DUPLICATE",
    c"M2_SHADER_ERR_FAMILY",
    c"M2_SHADER_ERR_CAPACITY",
];

/// The enumerator's spelling, or `None` for a value outside the enum.
pub fn status_name(s: i32) -> Option<&'static CStr> {
    usize::try_from(s).ok().and_then(|i| STATUS_NAMES.get(i).copied())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Pixel,
    Vertex,
}

pub struct FamilyInfo {
    pub name: &'static str,
    pub stage: Stage,
    /// VA of the family's 6-slot vtable in the cracked/unpacked exe; m2-sdk.dll uses the vtable
    /// [`crate::shader_sites`] resolves in the running image.
    pub vtable: u32,
    /// Record size in bytes.
    pub size: u32,
}

pub const FAMILY_COUNT: usize = 45;
pub const FAMILY_WATER_WAKE_PIXEL: i32 = 25;
pub const FAMILY_VERTEX: i32 = 26;

pub const PS_RECORD_MAX: u32 = 0x15C;
pub const VS_RECORD_MAX: u32 = 0x16C;
pub const PIXEL_CLASSES: usize = 4;
/// Slot counts of the game's two key tables (pool+0x2008 pixel, pool+0x6410 vertex). The inserts
/// (FUN_0085b7c0 / FUN_00632250) probe `& 0x7ff` / `& 0xff` without giving up, so an insert into a full
/// table never terminates.
pub const PS_CAPACITY: u32 = 0x800;
pub const VS_CAPACITY: u32 = 0x100;
/// FUN_0085ac90 copies the sho name to record+0xb and the class lives at +0x8c, so a sho name holds at
/// most 0x8c - 0xb - 1 = 128 characters.
pub const SHO_MAX_LEN: usize = 128;

macro_rules! fam {
    ($name:literal, $stage:ident, $vt:literal, $size:literal) => {
        FamilyInfo { name: $name, stage: Stage::$stage, vtable: $vt, size: $size }
    };
}

/// One row per record family, indexed by `m2_shader_family`. Each family is one vtable in the retail exe
/// whose slot [1] is FUN_0085ac90 and slot [2] the stage's load handler (0x0085b1a0 pixel, 0x0085af00
/// vertex).
static FAMILIES: [FamilyInfo; FAMILY_COUNT] = [
    fam!("pixel", Pixel, 0x00BE89EC, 0xFC),
    fam!("adaptive_luminance_pixel", Pixel, 0x00BAADD0, 0x10C),
    fam!("anti_aliasing_pixel", Pixel, 0x00BE85B4, 0x11C),
    fam!("billboard_tree_pixel", Pixel, 0x00BAC888, 0x104),
    fam!("blob_shadow_pixel", Pixel, 0x00BAB148, 0x114),
    fam!("blur_pixel", Pixel, 0x00BE8A58, 0x104),
    fam!("cloud_gen_pixel", Pixel, 0x00BAB59C, 0x11C),
    fam!("cloud_render_pixel", Pixel, 0x00BAB584, 0x14C),
    fam!("composite_pixel", Pixel, 0x00BAAEDC, 0x104),
    fam!("decal_pixel", Pixel, 0x00BAC614, 0x104),
    fam!("down_sample_pixel", Pixel, 0x00BAADE8, 0x104),
    fam!("fx_pixel", Pixel, 0x00BAC314, 0x104),
    fam!("hdr_flare_pixel", Pixel, 0x00BAADA0, 0x15C),
    fam!("motion_blur_pixel", Pixel, 0x00BAAE84, 0x11C),
    fam!("rain_pixel", Pixel, 0x00BAAF8C, 0x10C),
    fam!("ribbon_pixel", Pixel, 0x00BAC410, 0xFC),
    fam!("scaleform_cxform_pixel", Pixel, 0x00BAD158, 0x10C),
    fam!("scaleform_solid_color_pixel", Pixel, 0x00BAD170, 0x104),
    fam!("scaleform_strip_pixel", Pixel, 0x00BAD128, 0x10C),
    fam!("scaleform_text_texture_pixel", Pixel, 0x00BAD140, 0x104),
    fam!("shimmer_pixel", Pixel, 0x00BAAE5C, 0x10C),
    fam!("sky_pixel", Pixel, 0x00BAB604, 0x104),
    fam!("tone_mapping_pixel", Pixel, 0x00BAADB8, 0x13C),
    fam!("water_height_map_pixel", Pixel, 0x00BAB97C, 0x104),
    fam!("water_pixel", Pixel, 0x00BAC168, 0x10C),
    fam!("water_wake_pixel", Pixel, 0x00BAB964, 0x104),
    fam!("vertex", Vertex, 0x00BE8A04, 0x114),
    fam!("billboard_tree_instance_vertex", Vertex, 0x00BAC870, 0x11C),
    fam!("billboard_tree_vertex", Vertex, 0x00BAC858, 0x11C),
    fam!("blob_shadow_vertex", Vertex, 0x00BAB130, 0x12C),
    fam!("cloud_render_vertex", Vertex, 0x00BAB56C, 0x16C),
    fam!("decal_vertex", Vertex, 0x00BAC5FC, 0x124),
    fam!("fx_vertex", Vertex, 0x00BAC2FC, 0x114),
    fam!("mesh_combiner_vertex", Vertex, 0x00BAC6B0, 0x114),
    fam!("rain_vertex", Vertex, 0x00BAAF74, 0x12C),
    fam!("ribbon_vertex", Vertex, 0x00BAC3F8, 0x114),
    fam!("road_vertex", Vertex, 0x00BAC448, 0x12C),
    fam!("scaleform_glyph_vertex", Vertex, 0x00BAD110, 0x11C),
    fam!("scaleform_strip_vertex", Vertex, 0x00BAD0F8, 0x124),
    fam!("scrub_vertex", Vertex, 0x00BAC97C, 0x12C),
    fam!("sky_vertex", Vertex, 0x00BAB5EC, 0x11C),
    fam!("sun_vertex", Vertex, 0x00BAB61C, 0x124),
    fam!("terrain_mesh_vertex", Vertex, 0x00BACCD8, 0x124),
    fam!("water_vertex", Vertex, 0x00BAC150, 0x134),
    fam!("water_wake_vertex", Vertex, 0x00BAB94C, 0x11C),
];

/// The family's row, or `None` when `family` is not an enumerator.
pub fn family(family: i32) -> Option<&'static FamilyInfo> {
    usize::try_from(family).ok().and_then(|i| FAMILIES.get(i))
}

/// FUN_008242b0: the slot holding `key`, or `None`. Linear probe from `key % slots`, wrapping once,
/// stopping at an empty (0) slot. Key 0 always misses.
pub fn pool_find(key: u32, keys: &[u32]) -> Option<usize> {
    let slots = keys.len() as u32;
    if key == 0 || slots == 0 {
        return None;
    }
    let start = key % slots;
    for i in 0..slots {
        let slot = ((start + i) % slots) as usize;
        if keys[slot] == key {
            return Some(slot);
        }
        if keys[slot] == 0 {
            return None;
        }
    }
    None
}

/// `m2_shader_class`: two C string pointers the registry keeps for the life of the process.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Class {
    pub name: *const c_char,
    pub sho: *const c_char,
}

// The header requires both strings to stay valid and unchanged for the life of the process, so the
// registry may read them from any thread.
unsafe impl Send for Class {}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub family: i32,
    pub classes: usize,
    pub cls: [Class; PIXEL_CLASSES],
    pub key: [u32; PIXEL_CLASSES],
    /// The calling module, for its log line.
    pub owner: usize,
    pub outcome: Option<Status>,
}

/// # Safety
/// `p` is null or a NUL-terminated string.
unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

/// # Safety
/// Both pointers of `c` are null or NUL-terminated strings.
unsafe fn check_class(c: &Class) -> Status {
    let (Some(name), Some(sho)) = (unsafe { cstr(c.name) }, unsafe { cstr(c.sho) }) else {
        return Status::ErrArgument;
    };
    if name.is_empty() || sho.is_empty() {
        return Status::ErrArgument;
    }
    if !(sho.len() > 4 && sho.ends_with(b".sho")) || sho.len() > SHO_MAX_LEN {
        return Status::ErrArgument;
    }
    if pandemic_hash_m2_bytes(name) == 0 {
        return Status::ErrArgument;
    }
    Status::Ok
}

/// The queued entries. Every entry adds at least one name to its stage's registry, so the queue never holds
/// more entries of a stage than that stage's capacity.
#[derive(Default)]
pub struct ShaderQueue {
    entries: Vec<Entry>,
    pixel: u32,
    vertex: u32,
}

impl ShaderQueue {
    pub const fn new() -> Self {
        ShaderQueue { entries: Vec::new(), pixel: 0, vertex: 0 }
    }

    fn find_queued(&self, key: u32) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key[..e.classes].contains(&key))
    }

    /// Validate and queue one entry: FAMILY, then ARGUMENT, then DUPLICATE, then CAPACITY.
    ///
    /// # Safety
    /// `classes` is null or points at `count` classes whose pointers are null or NUL-terminated strings.
    unsafe fn queue(&mut self, family_id: i32, stage: Stage, classes: *const Class, count: usize, owner: usize) -> Status {
        if !family(family_id).is_some_and(|f| f.stage == stage) {
            return Status::ErrFamily;
        }
        if classes.is_null() {
            return Status::ErrArgument;
        }
        let given = unsafe { core::slice::from_raw_parts(classes, count) };
        for c in given {
            let st = unsafe { check_class(c) };
            if st != Status::Ok {
                return st;
            }
        }
        let empty = Class { name: core::ptr::null(), sho: core::ptr::null() };
        let mut e = Entry { family: family_id, classes: count, cls: [empty; PIXEL_CLASSES], key: [0; PIXEL_CLASSES], owner, outcome: None };
        for (i, c) in given.iter().enumerate() {
            e.cls[i] = *c;
            e.key[i] = pandemic_hash_m2_bytes(unsafe { cstr(c.name) }.expect("checked above"));
            if e.key[..i].contains(&e.key[i]) || self.find_queued(e.key[i]).is_some() {
                return Status::ErrDuplicate;
            }
        }
        match stage {
            Stage::Pixel if self.pixel >= PS_CAPACITY => return Status::ErrCapacity,
            Stage::Vertex if self.vertex >= VS_CAPACITY => return Status::ErrCapacity,
            Stage::Pixel => self.pixel += 1,
            Stage::Vertex => self.vertex += 1,
        }
        self.entries.push(e);
        Status::Ok
    }

    /// # Safety
    /// As [`ShaderQueue::queue`], with four classes.
    pub unsafe fn queue_pixel(&mut self, family: i32, classes: *const Class, owner: usize) -> Status {
        unsafe { self.queue(family, Stage::Pixel, classes, PIXEL_CLASSES, owner) }
    }

    /// # Safety
    /// As [`ShaderQueue::queue`], with one class.
    pub unsafe fn queue_vertex(&mut self, family: i32, cls: *const Class, owner: usize) -> Status {
        unsafe { self.queue(family, Stage::Vertex, cls, 1, owner) }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, index: usize) -> Option<&Entry> {
        self.entries.get(index)
    }

    /// Record the registry pass's result for the entry at `index`.
    pub fn record(&mut self, index: usize, outcome: Status) {
        if let Some(e) = self.entries.get_mut(index) {
            e.outcome = Some(outcome);
        }
    }

    /// OK or the recorded error for a name whose entry the registry pass reached; ARGUMENT for an empty or
    /// absent name, one never queued, or one the pass has not reached.
    pub fn outcome(&self, name: Option<&[u8]>) -> Status {
        let Some(name) = name.filter(|n| !n.is_empty()) else { return Status::ErrArgument };
        self.find_queued(pandemic_hash_m2_bytes(name)).and_then(|e| e.outcome).unwrap_or(Status::ErrArgument)
    }
}

/// How many of the entry's classes the game registers: every pixel class when `shader_level` is non-zero,
/// only class 0 when it is zero; a vertex entry's one class.
pub fn registrations(e: &Entry, shader_level: u8) -> u32 {
    if e.classes == PIXEL_CLASSES && shader_level == 0 { 1 } else { e.classes as u32 }
}

/// The registry pass's decision for one entry against the live pool of its stage.
pub fn admit(e: &Entry, shader_level: u8, live_count: u32, keys: &[u32]) -> Status {
    if e.key[..e.classes].iter().any(|&k| pool_find(k, keys).is_some()) {
        return Status::ErrDuplicate;
    }
    let slots = keys.len() as u32;
    if live_count > slots || slots - live_count < registrations(e, shader_level) {
        return Status::ErrCapacity;
    }
    Status::Ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr::null;

    fn cls(name: &'static CStr, sho: &'static CStr) -> Class {
        Class { name: name.as_ptr(), sho: sho.as_ptr() }
    }

    fn glow() -> [Class; 4] {
        [
            cls(c"TestGlowFP", c"TestGlowFP.sho"),
            cls(c"TestGlowFP_pl", c"TestGlowFP_pl.sho"),
            cls(c"TestGlowFP_sl", c"TestGlowFP_sl.sho"),
            cls(c"TestGlowFP_pl_sl", c"TestGlowFP_pl_sl.sho"),
        ]
    }

    fn glow_vs() -> Class {
        cls(c"TestGlowVP", c"TestGlowVP.sho")
    }

    #[test]
    fn family_table() {
        let (mut pixel, mut vertex) = (0, 0);
        for f in 0..FAMILY_COUNT as i32 {
            let info = family(f).unwrap();
            assert!(!info.name.is_empty());
            assert!((0x00B0_5000..0x00BF_6000).contains(&info.vtable), ".rdata of the cracked/unpacked exe");
            match info.stage {
                Stage::Pixel => {
                    pixel += 1;
                    assert!(f <= FAMILY_WATER_WAKE_PIXEL);
                    assert!(info.size <= PS_RECORD_MAX);
                    assert!(info.size >= 0xFC, "FUN_0085ace0 writes through +0xf8");
                }
                Stage::Vertex => {
                    vertex += 1;
                    assert!(f >= FAMILY_VERTEX);
                    assert!(info.size <= VS_RECORD_MAX);
                    assert!(info.size >= 0x114, "FUN_0085ade0 writes through +0x110");
                }
            }
        }
        assert_eq!((pixel, vertex), (26, 19));
        assert!(family(-1).is_none());
        assert!(family(FAMILY_COUNT as i32).is_none());
        assert_eq!(family(0).unwrap().vtable, 0x00BE_89EC);
        assert_eq!(family(FAMILY_VERTEX).unwrap().vtable, 0x00BE_8A04);
        assert_eq!(family(22).unwrap().name, "tone_mapping_pixel");
        assert_eq!(family(22).unwrap().size, 0x13C);
        assert_eq!(family(44).unwrap().name, "water_wake_vertex");
        assert_eq!(family(12).unwrap().size, PS_RECORD_MAX);
        assert_eq!(family(30).unwrap().size, VS_RECORD_MAX);
    }

    #[test]
    fn status_names() {
        let want = [
            "M2_SHADER_OK",
            "M2_SHADER_ERR_SIGNATURE",
            "M2_SHADER_ERR_HOOK",
            "M2_SHADER_ERR_TOO_LATE",
            "M2_SHADER_ERR_ARGUMENT",
            "M2_SHADER_ERR_DUPLICATE",
            "M2_SHADER_ERR_FAMILY",
            "M2_SHADER_ERR_CAPACITY",
        ];
        for (i, w) in want.iter().enumerate() {
            assert_eq!(status_name(i as i32).unwrap().to_str().unwrap(), *w);
        }
        assert!(status_name(8).is_none());
        assert!(status_name(-1).is_none());
    }

    #[test]
    fn pool_lookup() {
        let mut keys = [0u32; 8];
        assert_eq!(pool_find(0x13, &keys), None);
        keys[3] = 0x13;
        assert_eq!(pool_find(0x13, &keys), Some(3));
        assert_eq!(pool_find(0x0B, &keys), None);
        keys[7] = 0x1F;
        keys[0] = 0x27;
        assert_eq!(pool_find(0x27, &keys), Some(0));
        assert_eq!(pool_find(0, &keys), None);
        let full = [4u32, 5, 6, 7];
        assert_eq!(pool_find(9, &full), None);
        assert_eq!(pool_find(7, &full), Some(3));
    }

    /// The C test's sequence, in order, against one queue: family checks, argument checks, duplicates,
    /// the 128-character edge, outcomes before and after the pass, admission, and the vertex capacity.
    #[test]
    fn queue_admit_and_outcome_sequence() {
        let mut q = ShaderQueue::new();
        let g = glow();
        let gv = glow_vs();
        unsafe {
            assert_eq!(q.queue_pixel(FAMILY_VERTEX, g.as_ptr(), 0), Status::ErrFamily);
            assert_eq!(q.queue_pixel(44, g.as_ptr(), 0), Status::ErrFamily);
            assert_eq!(q.queue_vertex(0, &gv, 0), Status::ErrFamily);
            assert_eq!(q.queue_vertex(FAMILY_WATER_WAKE_PIXEL, &gv, 0), Status::ErrFamily);
            assert_eq!(q.queue_pixel(FAMILY_COUNT as i32, g.as_ptr(), 0), Status::ErrFamily);
            assert_eq!(q.queue_vertex(-1, &gv, 0), Status::ErrFamily);
            assert_eq!(q.len(), 0);

            assert_eq!(q.queue_vertex(FAMILY_VERTEX, null(), 0), Status::ErrArgument);
            assert_eq!(q.queue_pixel(0, null(), 0), Status::ErrArgument);
            let bad = |name: *const c_char, sho: *const c_char| Class { name, sho };
            let n = c"TestArgVP".as_ptr();
            for c in [
                bad(null(), c"TestArgVP.sho".as_ptr()),
                bad(c"".as_ptr(), c"TestArgVP.sho".as_ptr()),
                bad(n, null()),
                bad(n, c"".as_ptr()),
                bad(n, c"TestArgVP.fx".as_ptr()),
                bad(n, c"TestArgVP.SHO".as_ptr()),
                bad(n, c".sho".as_ptr()),
            ] {
                assert_eq!(q.queue_vertex(FAMILY_VERTEX, &c, 0), Status::ErrArgument);
            }
            let mut long_sho = vec![b'a'; SHO_MAX_LEN + 1 - 4];
            long_sho.extend_from_slice(b".sho\0");
            let c = bad(n, long_sho.as_ptr().cast());
            assert_eq!(q.queue_vertex(FAMILY_VERTEX, &c, 0), Status::ErrArgument);
            let mut px = glow();
            px[3].sho = c"TestGlowFP_pl_sl".as_ptr();
            assert_eq!(q.queue_pixel(0, px.as_ptr(), 0), Status::ErrArgument);
            assert_eq!(q.len(), 0);

            let mut px = glow();
            px[2].name = c"testglowfp_PL".as_ptr();
            assert_eq!(q.queue_pixel(0, px.as_ptr(), 0), Status::ErrDuplicate);
            let mut px = glow();
            px[1].name = px[0].name;
            assert_eq!(q.queue_pixel(0, px.as_ptr(), 0), Status::ErrDuplicate);
            assert_eq!(q.len(), 0);

            assert_eq!(q.queue_pixel(5, g.as_ptr(), 0), Status::Ok);
            assert_eq!(q.queue_vertex(FAMILY_VERTEX, &gv, 0), Status::Ok);
            assert_eq!(q.len(), 2);
            assert_eq!(q.queue_pixel(0, g.as_ptr(), 0), Status::ErrDuplicate);
            let again = cls(c"TESTGLOWFP_SL", c"Other.sho");
            assert_eq!(q.queue_vertex(32, &again, 0), Status::ErrDuplicate);
            assert_eq!(q.queue_vertex(FAMILY_VERTEX, &gv, 0), Status::ErrDuplicate);
            assert_eq!(q.len(), 2);

            let mut long_ok = vec![b'b'; SHO_MAX_LEN - 4];
            long_ok.extend_from_slice(b".sho\0");
            let edge = Class { name: c"TestEdgeVP".as_ptr(), sho: long_ok.as_ptr().cast() };
            assert_eq!(q.queue_vertex(40, &edge, 0), Status::Ok);
            assert_eq!(q.len(), 3);
            let e = q.entry(0).unwrap();
            assert_eq!(e.family, 5);
            assert_eq!(e.classes, 4);
            assert_eq!(e.key[1], pandemic_hash_m2_bytes(b"TestGlowFP_pl"));
            assert!(e.outcome.is_none());
            assert!(q.entry(3).is_none());

            assert_eq!(q.outcome(Some(b"NeverQueuedFP")), Status::ErrArgument);
            assert_eq!(q.outcome(None), Status::ErrArgument);
            assert_eq!(q.outcome(Some(b"")), Status::ErrArgument);
            assert_eq!(q.outcome(Some(b"TestGlowFP")), Status::ErrArgument);

            let mut ps_keys = vec![0u32; PS_CAPACITY as usize];
            let vs_keys = vec![0u32; VS_CAPACITY as usize];
            let px_e = *q.entry(0).unwrap();
            let vx_e = *q.entry(1).unwrap();
            assert_eq!(registrations(&px_e, 1), 4);
            assert_eq!(registrations(&px_e, 0), 1);
            assert_eq!(registrations(&vx_e, 0), 1);
            assert_eq!(registrations(&vx_e, 1), 1);
            assert_eq!(admit(&px_e, 1, 242, &ps_keys), Status::Ok);
            assert_eq!(admit(&vx_e, 1, 129, &vs_keys), Status::Ok);
            assert_eq!(admit(&px_e, 1, PS_CAPACITY - 4, &ps_keys), Status::Ok);
            assert_eq!(admit(&px_e, 1, PS_CAPACITY - 3, &ps_keys), Status::ErrCapacity);
            assert_eq!(admit(&px_e, 0, PS_CAPACITY - 1, &ps_keys), Status::Ok);
            assert_eq!(admit(&px_e, 0, PS_CAPACITY, &ps_keys), Status::ErrCapacity);
            assert_eq!(admit(&vx_e, 0, VS_CAPACITY, &vs_keys), Status::ErrCapacity);
            let k = pandemic_hash_m2_bytes(b"TestGlowFP_pl_sl");
            ps_keys[(k % PS_CAPACITY) as usize] = k;
            assert_eq!(admit(&px_e, 1, 243, &ps_keys), Status::ErrDuplicate);
            assert_eq!(admit(&px_e, 0, 243, &ps_keys), Status::ErrDuplicate);
            assert_eq!(admit(&vx_e, 1, 129, &ps_keys[..VS_CAPACITY as usize]), Status::Ok);

            q.record(0, Status::Ok);
            q.record(1, Status::ErrDuplicate);
            assert_eq!(q.outcome(Some(b"TestGlowFP")), Status::Ok);
            assert_eq!(q.outcome(Some(b"testglowfp_pl_sl")), Status::Ok);
            assert_eq!(q.outcome(Some(b"TestGlowVP")), Status::ErrDuplicate);
            assert_eq!(q.outcome(Some(b"TestEdgeVP")), Status::ErrArgument);
            assert_eq!(q.outcome(Some(b"NeverQueuedFP")), Status::ErrArgument);

            let before = q.len();
            let names: Vec<std::ffi::CString> =
                (0..VS_CAPACITY - 2).map(|i| std::ffi::CString::new(format!("TestCapVP{i}")).unwrap()).collect();
            let shos: Vec<std::ffi::CString> =
                (0..VS_CAPACITY - 2).map(|i| std::ffi::CString::new(format!("TestCapVP{i}.sho")).unwrap()).collect();
            for (n, s) in names.iter().zip(&shos) {
                let c = Class { name: n.as_ptr(), sho: s.as_ptr() };
                assert_eq!(q.queue_vertex(FAMILY_VERTEX, &c, 0), Status::Ok);
            }
            assert_eq!(q.len(), before + VS_CAPACITY as usize - 2);
            let over = cls(c"TestCapVPOver", c"TestCapVPOver.sho");
            assert_eq!(q.queue_vertex(FAMILY_VERTEX, &over, 0), Status::ErrCapacity);
            let more = [
                cls(c"TestMoreFP", c"TestMoreFP.sho"),
                cls(c"TestMoreFP_pl", c"TestMoreFP_pl.sho"),
                cls(c"TestMoreFP_sl", c"TestMoreFP_sl.sho"),
                cls(c"TestMoreFP_pl_sl", c"TestMoreFP_pl_sl.sho"),
            ];
            assert_eq!(q.queue_pixel(11, more.as_ptr(), 0), Status::Ok);
        }
    }
}
