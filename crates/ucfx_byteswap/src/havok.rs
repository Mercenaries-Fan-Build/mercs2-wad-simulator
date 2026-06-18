//! Havok 5.5.0-r1 packfile BE→LE converter (32-bit / HK550).
//!
//! Faithful port of `tools/ucfx_be_to_le.py::_convert_havok_be_to_le` and
//! `tools/hk_class_layouts.py`. A wholesale u32 byte-swap of a Havok packfile
//! scrambles the ASCII `__classnames__` strings (the loader looks classes up by
//! name → `STATUS_OBJECT_NAME_NOT_FOUND` → AV). This converter is section-aware:
//! it swaps the header / section headers / classname signatures as u32, leaves
//! the classname strings and `__types__` raw, and converts `__data__` with
//! per-class field widths (u32 pointers/floats/enums, u16 refcounts/indices, u8
//! QuantizationFormat bytes and compressed bitstream buffers).
//!
//! The registry covers the animation classes (physics classes are not
//! registered, so for a `PHY2` collision packfile the `__data__` swap degenerates
//! to a u32 sweep + embedded-layoutRules repair — exactly matching Python).

use std::collections::HashMap;

use mercs2_formats::ffcs::read_u32_be;

pub const U32: u8 = 4;
pub const U16: u8 = 2;
pub const U8W: u8 = 1;

const HAVOK_VER: &[u8] = b"Havok-5.5.0-r1";
const HAVOK_DASH: &[u8] = b"Havok-";
const CLASSNAMES: &[u8] = b"__classnames__";
const SECTION_HDR_SIZE: usize = 48;

/// 8-byte Havok packfile magic — palindromic per u32 word, survives a swap.
const HAVOK_MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

// ── class layout registry (HK550, 32-bit; animation classes) ─────────────

pub struct ArrayInfo {
    pub ptr_off: Option<usize>,
    pub count_off: Option<usize>,
    pub elem_size: usize,
    pub elem_swap: u8,
}

pub enum SwapSpec {
    AllU32,
    Fields(&'static [(usize, u8)]),
}

pub struct ClassLayout {
    pub size: usize,
    pub swap: SwapSpec,
    pub arrays: &'static [(&'static str, ArrayInfo)],
}

// hkReferenceObject base appears inline in the per-class field lists below.
const HKA_INTERLEAVED_SWAP: &[(usize, u8)] = &[
    (0, U32), (4, U16), (6, U16),
    (8, U32), (12, U32), (16, U32), (20, U32), (24, U32), (28, U32), (32, U32),
    (36, U32), (40, U32), (44, U32), (48, U32),
];
const HKA_INTERLEAVED_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("transforms", ArrayInfo { ptr_off: Some(36), count_off: Some(40), elem_size: 48, elem_swap: U32 }),
    ("floats", ArrayInfo { ptr_off: Some(44), count_off: Some(48), elem_size: 4, elem_swap: U32 }),
];

const HKA_DELTA_SWAP: &[(usize, u8)] = &[
    (0, U32), (4, U16), (6, U16),
    (8, U32), (12, U32), (16, U32), (20, U32), (24, U32), (28, U32), (32, U32),
    (36, U32), (40, U32),
    (44, U8W), (45, U8W), (46, U8W), (47, U8W),
    (48, U32), (52, U32), (56, U32), (60, U32),
    (64, U32), (68, U32), (72, U32), (76, U32),
    (80, U32), (84, U32), (88, U32), (92, U32),
    (96, U32), (100, U32),
];
const HKA_DELTA_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("dataBuffer", ArrayInfo { ptr_off: Some(96), count_off: Some(100), elem_size: 1, elem_swap: U8W }),
];

const HKA_WAVELET_SWAP: &[(usize, u8)] = &[
    (0, U32), (4, U16), (6, U16),
    (8, U32), (12, U32), (16, U32), (20, U32), (24, U32), (28, U32), (32, U32),
    (36, U32), (40, U32),
    (44, U8W), (45, U8W), (46, U8W), (47, U8W),
    (48, U32), (52, U32), (56, U32), (60, U32),
    (64, U32), (68, U32), (72, U32), (76, U32),
    (80, U32), (84, U32), (88, U32), (92, U32),
];
const HKA_WAVELET_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("dataBuffer", ArrayInfo { ptr_off: Some(88), count_off: Some(92), elem_size: 1, elem_swap: U8W }),
];

const HKA_ANIMATION_CONTAINER_SWAP: &[(usize, u8)] = &[
    (0, U32), (4, U32), (8, U32), (12, U32), (16, U32),
    (20, U32), (24, U32), (28, U32), (32, U32), (36, U32),
];

const HKA_SKELETON_SWAP: &[(usize, u8)] = &[
    (0, U32), (4, U32), (8, U32), (12, U32), (16, U32),
    (20, U32), (24, U32), (28, U32), (32, U32),
];
const HKA_SKELETON_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("parentIndices", ArrayInfo { ptr_off: Some(4), count_off: Some(8), elem_size: 2, elem_swap: U16 }),
    ("bones", ArrayInfo { ptr_off: Some(12), count_off: Some(16), elem_size: 8, elem_swap: U32 }),
    ("transforms", ArrayInfo { ptr_off: Some(20), count_off: Some(24), elem_size: 48, elem_swap: U32 }),
];

const HKA_ANNOTATION_TRACK_SWAP: &[(usize, u8)] = &[(0, U32), (4, U32), (8, U32)];
const HKA_ANNOTATION_SWAP: &[(usize, u8)] = &[(0, U32), (4, U32)];
const HKA_BONE_SWAP: &[(usize, u8)] = &[(0, U32), (4, U32)];

const HKA_ANIMATION_BINDING_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("transformTrackToBoneIndices", ArrayInfo { ptr_off: None, count_off: None, elem_size: 2, elem_swap: U16 }),
    ("floatTrackToFloatSlotIndices", ArrayInfo { ptr_off: None, count_off: None, elem_size: 2, elem_swap: U16 }),
];

// hkpMoppCode (physics): 48-byte struct + inline u8 MOPP bytecode buffer at +48.
// The m_data buffer is a u8 ARRAY — must NOT be u32-swapped (a blind sweep reverses
// every 4 bytes → the whole MOPP collision tree is scrambled).
const HKP_MOPP_CODE_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("m_data", ArrayInfo { ptr_off: Some(32), count_off: Some(36), elem_size: 1, elem_swap: U8W }),
];

// WpMeshShape16 (physics): Pandemic custom 16-bit-indexed collision mesh.
// Two u16 index arrays (a blind u32 sweep transposes each pair → wrong triangles).
const WP_MESH_SHAPE16_ARRAYS: &[(&str, ArrayInfo)] = &[
    ("indices_b", ArrayInfo { ptr_off: Some(80), count_off: Some(84), elem_size: 2, elem_swap: U16 }),
    ("indices_c", ArrayInfo { ptr_off: Some(88), count_off: Some(92), elem_size: 2, elem_swap: U16 }),
];

fn lookup_class(name: &str) -> Option<ClassLayout> {
    let layout = match name {
        "hkRootLevelContainer" => ClassLayout { size: 12, swap: SwapSpec::AllU32, arrays: &[] },
        "hkaAnimationContainer" => ClassLayout { size: 40, swap: SwapSpec::Fields(HKA_ANIMATION_CONTAINER_SWAP), arrays: &[] },
        "hkaSkeleton" => ClassLayout { size: 36, swap: SwapSpec::Fields(HKA_SKELETON_SWAP), arrays: HKA_SKELETON_ARRAYS },
        "hkaInterleavedUncompressedAnimation" | "hkaInterleavedSkeletalAnimation" =>
            ClassLayout { size: 52, swap: SwapSpec::Fields(HKA_INTERLEAVED_SWAP), arrays: HKA_INTERLEAVED_ARRAYS },
        "hkaDeltaCompressedAnimation" | "hkaDeltaCompressedSkeletalAnimation" | "hkaDeltaSkeletalAnimation" =>
            ClassLayout { size: 104, swap: SwapSpec::Fields(HKA_DELTA_SWAP), arrays: HKA_DELTA_ARRAYS },
        "hkaWaveletCompressedAnimation" | "hkaWaveletCompressedSkeletalAnimation" | "hkaWaveletSkeletalAnimation" =>
            ClassLayout { size: 96, swap: SwapSpec::Fields(HKA_WAVELET_SWAP), arrays: HKA_WAVELET_ARRAYS },
        "hkaAnimationBinding" => ClassLayout { size: 28, swap: SwapSpec::AllU32, arrays: HKA_ANIMATION_BINDING_ARRAYS },
        "hkaAnnotationTrack" => ClassLayout { size: 12, swap: SwapSpec::Fields(HKA_ANNOTATION_TRACK_SWAP), arrays: &[] },
        "hkaAnnotation" => ClassLayout { size: 8, swap: SwapSpec::Fields(HKA_ANNOTATION_SWAP), arrays: &[] },
        "hkaBone" => ClassLayout { size: 8, swap: SwapSpec::Fields(HKA_BONE_SWAP), arrays: &[] },
        // physics (PHY2 collision packfiles)
        "hkpMoppCode" => ClassLayout { size: 48, swap: SwapSpec::AllU32, arrays: HKP_MOPP_CODE_ARRAYS },
        "WpMeshShape16" => ClassLayout { size: 8, swap: SwapSpec::AllU32, arrays: WP_MESH_SHAPE16_ARRAYS },
        _ => return None,
    };
    Some(layout)
}

// ── byte-level helpers ───────────────────────────────────────────────────

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn find_sub_from(hay: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if start >= hay.len() {
        return None;
    }
    find_sub(&hay[start..], needle).map(|p| p + start)
}

#[inline]
fn put_u32_swapped(be: &[u8], out: &mut [u8], off: usize) {
    if off + 4 <= be.len() {
        out[off] = be[off + 3];
        out[off + 1] = be[off + 2];
        out[off + 2] = be[off + 1];
        out[off + 3] = be[off];
    }
}

#[inline]
fn put_u16_swapped(be: &[u8], out: &mut [u8], off: usize) {
    if off + 2 <= be.len() {
        out[off] = be[off + 1];
        out[off + 1] = be[off];
    }
}

// ── __classnames__ / fixup parsing ───────────────────────────────────────

/// `{name_offset_in_cn_section: class_name}` (offset of the name string itself,
/// i.e. after the 4-byte signature + 1 flag byte).
fn parse_classnames_be(be: &[u8], cn_abs: usize, cn_end: usize) -> HashMap<u32, String> {
    let mut names = HashMap::new();
    let mut p = cn_abs;
    let body_end = (cn_abs + cn_end).min(be.len());
    while p + 5 <= body_end {
        let sig = read_u32_be(be, p);
        if sig == 0xFFFF_FFFF {
            break;
        }
        let rel_off = (p - cn_abs) as u32;
        let mut q = p + 5;
        while q < body_end && be[q] != 0 {
            q += 1;
        }
        let name = String::from_utf8_lossy(&be[p + 5..q]).into_owned();
        names.insert(rel_off + 5, name);
        p = q + 1;
    }
    names
}

/// `(src_offset_in_data, section_index, classname_offset)`
fn parse_virtual_fixups_be(be: &[u8], da_abs: usize, vf_off: u32, da_end: u32) -> Vec<(u32, u32, u32)> {
    let mut fixups = Vec::new();
    let mut p = da_abs + vf_off as usize;
    let end = (da_abs + da_end as usize).min(be.len());
    while p + 12 <= end {
        let src = read_u32_be(be, p);
        let sec = read_u32_be(be, p + 4);
        let cn_off = read_u32_be(be, p + 8);
        if src == 0xFFFF_FFFF {
            break;
        }
        fixups.push((src, sec, cn_off));
        p += 12;
    }
    fixups
}

/// `(src_offset_in_data, dst_offset_in_data)`
fn parse_local_fixups_be(be: &[u8], da_abs: usize, lf_off: u32, gf_off: u32) -> Vec<(u32, u32)> {
    let mut fixups = Vec::new();
    let mut p = da_abs + lf_off as usize;
    let end = (da_abs + gf_off as usize).min(be.len());
    while p + 8 <= end {
        let src = read_u32_be(be, p);
        let dst = read_u32_be(be, p + 4);
        if src == 0xFFFF_FFFF {
            break;
        }
        fixups.push((src, dst));
        p += 8;
    }
    fixups
}

/// Class-aware BE→LE swap of the `__data__` section (mirrors
/// `_havok_swap_data_class_aware`). Always succeeds (Python returns True).
fn swap_data_class_aware(
    be: &[u8],
    out: &mut [u8],
    da_abs: usize,
    sections: &[[u32; 7]; 3],
    cn_abs: usize,
    cn_end: usize,
) {
    let da_lf = sections[2][1] as usize;
    let da_gf = sections[2][2];
    let da_vf = sections[2][3];
    let da_end = sections[2][6] as usize;
    if da_end == 0 {
        return;
    }

    let cn_names = parse_classnames_be(be, cn_abs, cn_end);
    let vfixups = parse_virtual_fixups_be(be, da_abs, da_vf, da_end as u32);
    let lfixups = parse_local_fixups_be(be, da_abs, da_lf as u32, da_gf);

    // object offset → class name
    let mut obj_map: Vec<(usize, String)> = Vec::new();
    for (src, _sec, cn_off) in &vfixups {
        if let Some(name) = cn_names.get(cn_off) {
            if !name.is_empty() {
                obj_map.push((*src as usize, name.clone()));
            }
        }
    }
    let lf_map: HashMap<u32, u32> = lfixups.iter().copied().collect();

    // no-swap regions: U8 element arrays (compressed bitstream buffers)
    let mut no_swap_regions: Vec<(usize, usize)> = Vec::new();
    for (obj_off, class_name) in &obj_map {
        let cls = match lookup_class(class_name) {
            Some(c) => c,
            None => continue,
        };
        for (_arr_name, arr) in cls.arrays {
            if arr.elem_swap == U8W {
                if let (Some(ptr_off), Some(count_off)) = (arr.ptr_off, arr.count_off) {
                    let ptr_src = (obj_off + ptr_off) as u32;
                    if let Some(&buf_dst) = lf_map.get(&ptr_src) {
                        let count_abs = da_abs + obj_off + count_off;
                        if count_abs + 4 <= be.len() {
                            let n = read_u32_be(be, count_abs) as usize;
                            if n < 0x0100_0000 {
                                no_swap_regions.push((buf_dst as usize, buf_dst as usize + n));
                            }
                        }
                    }
                }
            }
        }
    }
    no_swap_regions.sort();
    let in_no_swap = |off: usize| -> bool {
        for &(start, end) in &no_swap_regions {
            if start <= off && off < end {
                return true;
            }
            if start > off {
                break;
            }
        }
        false
    };

    // per-byte swap width over [0, da_end)
    let mut swap_width = vec![0u8; da_end];

    for (obj_off, class_name) in &obj_map {
        let obj_off = *obj_off;
        let cls = match lookup_class(class_name) {
            Some(c) => c,
            None => {
                // unknown class: mark only the first u32
                if obj_off + 4 <= da_end {
                    swap_width[obj_off] = U32;
                }
                continue;
            }
        };

        match &cls.swap {
            SwapSpec::AllU32 => {
                let mut i = 0;
                while i < cls.size.min(da_end.saturating_sub(obj_off)) {
                    swap_width[obj_off + i] = U32;
                    i += 4;
                }
            }
            SwapSpec::Fields(spec) => {
                for &(field_off, w) in *spec {
                    let abs_off = obj_off + field_off;
                    if abs_off < da_end {
                        swap_width[abs_off] = w;
                    }
                }
            }
        }

        for (_arr_name, arr) in cls.arrays {
            let (ptr_off, count_off) = match (arr.ptr_off, arr.count_off) {
                (Some(p), Some(c)) => (p, c),
                _ => continue,
            };
            let ptr_src = (obj_off + ptr_off) as u32;
            let buf_dst = match lf_map.get(&ptr_src) {
                Some(&d) => d as usize,
                None => continue,
            };
            let count_abs = da_abs + obj_off + count_off;
            if count_abs + 4 > be.len() {
                continue;
            }
            let count = read_u32_be(be, count_abs) as usize;
            if count > 0x0100_0000 {
                continue;
            }
            match arr.elem_swap {
                U8W => {}
                U32 => {
                    let total = count * arr.elem_size;
                    let mut i = 0;
                    while i < total {
                        let pos = buf_dst + i;
                        if pos < da_end {
                            swap_width[pos] = U32;
                        }
                        i += 4;
                    }
                }
                U16 => {
                    let total = count * arr.elem_size;
                    let mut i = 0;
                    while i < total {
                        let pos = buf_dst + i;
                        if pos < da_end {
                            swap_width[pos] = U16;
                        }
                        i += 2;
                    }
                }
                _ => {}
            }
        }
    }

    // default-fill: u32 over fixup-free body (skip no-swap buffers), then u32
    // over the fixup tables themselves.
    let mut off = 0;
    while off < da_lf {
        if swap_width[off] == 0 && !in_no_swap(off) {
            swap_width[off] = U32;
        }
        off += 4;
    }
    let mut off = da_lf;
    while off < da_end {
        if swap_width[off] == 0 {
            swap_width[off] = U32;
        }
        off += 4;
    }

    // emit
    let mut off = 0;
    while off < da_end {
        let w = swap_width[off];
        let abs_off = da_abs + off;
        match w {
            U32 => {
                put_u32_swapped(be, out, abs_off);
                off += 4;
            }
            U16 => {
                put_u16_swapped(be, out, abs_off);
                off += 2;
            }
            _ => {
                if abs_off < be.len() {
                    out[abs_off] = be[abs_off];
                }
                off += 1;
            }
        }
    }
}

/// Repair embedded packfile `layoutRules` (4 × u8 at magic+16) that the data
/// sweep reversed; copy verbatim from BE and set littleEndian = 1.
fn fix_embedded_layoutrules(be: &[u8], out: &mut [u8], start: usize, end: usize) {
    let region_end = end.min(be.len());
    let mut pos = start;
    loop {
        let m = match find_sub_from(be, &HAVOK_MAGIC, pos) {
            Some(m) if m < region_end => m,
            _ => break,
        };
        if m + 20 <= be.len() {
            out[m + 16..m + 20].copy_from_slice(&be[m + 16..m + 20]);
            out[m + 17] = 1;
        }
        pos = m + 8;
    }
}

/// Total size of the Havok packfile at the start of `be` (max section end),
/// so a containing chunk's trailing data can be handled separately. The three
/// 48-byte section headers each carry 7 u32 (`[abs, lf, gf, vf, exp, imp, end]`).
fn havok_packfile_size(be: &[u8]) -> Option<usize> {
    let ver = find_sub(be, HAVOK_VER).or_else(|| find_sub(be, HAVOK_DASH))?;
    let cn = find_sub_from(be, CLASSNAMES, ver)?;
    let mut total = 0usize;
    for i in 0..3 {
        let so = cn + i * SECTION_HDR_SIZE;
        if so + SECTION_HDR_SIZE > be.len() {
            return None;
        }
        let abs = read_u32_be(be, so + 20) as usize; // field 0
        let end = read_u32_be(be, so + 20 + 6 * 4) as usize; // field 6
        total = total.max(abs + end);
    }
    Some(total)
}

/// Structurally convert a Havok 5.5 packfile from BE to LE.
pub fn convert_havok_be_to_le(be: &[u8]) -> Result<Vec<u8>, String> {
    if be.len() < 64 {
        return Err(format!("Havok packfile too short ({} bytes)", be.len()));
    }
    let ver_off = find_sub(be, HAVOK_VER)
        .or_else(|| find_sub(be, HAVOK_DASH))
        .ok_or_else(|| "Havok packfile missing version string".to_string())?;
    let cn_needle_off = find_sub_from(be, CLASSNAMES, ver_off)
        .ok_or_else(|| "Havok packfile missing __classnames__ section".to_string())?;

    // zero-init, then fill faithfully (mirrors the Python bytearray(len) init)
    let mut out = vec![0u8; be.len()];

    // 1. magic (palindromic) — copy
    out[0..8].copy_from_slice(&be[0..8]);
    // 2. header u32 fields [8,16)
    put_u32_swapped(be, &mut out, 8);
    put_u32_swapped(be, &mut out, 12);
    // 3. byte fields [16,20): copy, set littleEndian=1
    out[16..20].copy_from_slice(&be[16..20]);
    out[17] = 1;
    // 4. header i32 fields [20, ver_off) as u32
    let mut off = 20;
    while off < ver_off {
        put_u32_swapped(be, &mut out, off);
        off += 4;
    }
    // 5. version string + padding up to section headers: copy
    let sec_hdrs_off = cn_needle_off;
    out[ver_off..sec_hdrs_off].copy_from_slice(&be[ver_off..sec_hdrs_off]);

    // 6. three 48-byte section headers (20-byte name + 7 u32)
    let mut sections = [[0u32; 7]; 3];
    for i in 0..3 {
        let so = sec_hdrs_off + i * SECTION_HDR_SIZE;
        if so + SECTION_HDR_SIZE > be.len() {
            out[so..].copy_from_slice(&be[so..]);
            return Ok(out);
        }
        out[so..so + 20].copy_from_slice(&be[so..so + 20]);
        for k in 0..7 {
            let fo = so + 20 + k * 4;
            let v = read_u32_be(be, fo);
            sections[i][k] = v;
            put_u32_swapped(be, &mut out, fo);
        }
    }
    let sec_data_start = sec_hdrs_off + 3 * SECTION_HDR_SIZE;

    let cn_abs = sections[0][0] as usize;
    let cn_end = sections[0][6] as usize;
    let ty_abs = sections[1][0] as usize;
    let ty_end = sections[1][6] as usize;
    let da_abs = sections[2][0] as usize;
    let da_end = sections[2][6] as usize;

    // 7. __classnames__ body: u32 signatures + raw ASCII names
    if cn_end > 0 && cn_abs + cn_end <= be.len() {
        let body_end = cn_abs + cn_end;
        let mut p = cn_abs;
        while p + 5 <= body_end {
            let sig_be = read_u32_be(be, p);
            if sig_be == 0xFFFF_FFFF {
                put_u32_swapped(be, &mut out, p);
                p += 4;
                break;
            }
            put_u32_swapped(be, &mut out, p);
            out[p + 4] = be[p + 4]; // flag byte
            let mut q = p + 5;
            while q < body_end && be[q] != 0 {
                out[q] = be[q];
                q += 1;
            }
            if q < body_end {
                out[q] = 0;
                q += 1;
            }
            p = q;
        }
        if p < body_end {
            out[p..body_end].copy_from_slice(&be[p..body_end]);
        }
    }

    // 8. __types__: copy raw
    if ty_end > 0 && ty_abs + ty_end <= be.len() {
        out[ty_abs..ty_abs + ty_end].copy_from_slice(&be[ty_abs..ty_abs + ty_end]);
    }

    // 9. __data__: class-aware swap + embedded layoutRules repair
    if da_end > 0 && da_abs + da_end <= be.len() {
        swap_data_class_aware(be, &mut out, da_abs, &sections, cn_abs, cn_end);
        fix_embedded_layoutrules(be, &mut out, da_abs, da_abs + da_end);
    }

    // fill gap between section-header table and first section body
    let first_body = [cn_abs, ty_abs, da_abs]
        .into_iter()
        .filter(|&x| x > 0)
        .min()
        .unwrap_or(sec_data_start);
    if sec_data_start < first_body && first_body <= be.len() {
        out[sec_data_start..first_body].copy_from_slice(&be[sec_data_start..first_body]);
    }

    // fill trailing bytes after all sections
    let total_end = (cn_abs + cn_end).max(ty_abs + ty_end).max(da_abs + da_end);
    if total_end < be.len() {
        out[total_end..].copy_from_slice(&be[total_end..]);
    }

    Ok(out)
}

/// Convert a `PHY2` mesh chunk: a u32 header prefix followed by an embedded
/// Havok packfile. Swap the prefix as u32, class-aware-convert the packfile.
/// If no packfile magic is present, fall back to a plain u32 sweep.
pub fn convert_phy2_be_to_le(be: &[u8]) -> Result<Vec<u8>, String> {
    match find_sub(be, &HAVOK_MAGIC) {
        Some(magic_off) => {
            let mut out = Vec::with_capacity(be.len());
            // header prefix: u32 fields (count, asset-hash, sizes …)
            let mut off = 0;
            while off + 4 <= magic_off {
                out.extend_from_slice(&[be[off + 3], be[off + 2], be[off + 1], be[off]]);
                off += 4;
            }
            if off < magic_off {
                out.extend_from_slice(&be[off..magic_off]); // ragged tail (rare)
            }
            // Bound the embedded packfile; a PHY2 chunk can carry engine
            // collision-wrapper data AFTER it that needs its own u32 swap.
            let rest = &be[magic_off..];
            let pf_size = havok_packfile_size(rest).unwrap_or(rest.len()).min(rest.len());
            let packfile = convert_havok_be_to_le(&rest[..pf_size])?;
            out.extend_from_slice(&packfile);
            // Trailing engine struct (hkpShape wrapper / collision metadata):
            // u32 offsets + f32 — a u32 sweep. Left raw it stays big-endian and
            // its self-offsets (e.g. 0x000004D8) read as 0xD8040000 → the engine
            // relocator computes base + 0xD8040000 → AV at 0x0248C13E.
            let mut t = pf_size;
            while t + 4 <= rest.len() {
                out.extend_from_slice(&[rest[t + 3], rest[t + 2], rest[t + 1], rest[t]]);
                t += 4;
            }
            if t < rest.len() {
                out.extend_from_slice(&rest[t..]); // ragged tail
            }
            Ok(out)
        }
        None => {
            // No embedded Havok packfile — legacy simple PHY2, u32 array.
            let mut out = be.to_vec();
            let n = out.len() / 4;
            for i in 0..n {
                out[i * 4..i * 4 + 4].reverse();
            }
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte-for-byte parity with the Python reference converter on a real
    /// `PHY2` collision packfile (resident2 mesh 0x85E40830). Physics classes
    /// are unregistered → exercises the full section-aware structure path
    /// (header / classnames-preserved / __data__ u32 sweep / layoutRules).
    #[test]
    fn phy2_resident2_matches_python() {
        let be = include_bytes!("../tests/fixtures/phy2_resident2_be.bin");
        let expected = include_bytes!("../tests/fixtures/phy2_resident2_le.bin");
        let got = convert_phy2_be_to_le(be).expect("PHY2 convert");
        assert_eq!(got.len(), expected.len(), "PHY2 output size must match");
        assert_eq!(&got, expected, "PHY2 Rust output must equal Python output");
        // The classname strings must be clean ASCII (the whole point).
        assert!(
            got.windows(22).any(|w| w == b"hkpConvexVerticesShape"),
            "PHY2 must contain unscrambled Havok class name"
        );
    }

    /// Byte-for-byte parity on an animation packfile (ks750 animgroup) whose
    /// `__data__` contains registered classes (hkaSkeleton/hkaBone/hkaWavelet…)
    /// — exercises the class-aware per-field widths (u16 indices, u8 buffers)
    /// and array no-swap regions.
    #[test]
    fn anim_ks750_matches_python() {
        let be = include_bytes!("../tests/fixtures/anim_ks750_be.bin");
        let expected = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        let got = convert_havok_be_to_le(be).expect("anim convert");
        assert_eq!(got.len(), expected.len(), "anim output size must match");
        assert_eq!(&got, expected, "anim Rust output must equal Python output");
    }

    /// A PHY2 chunk with engine collision-wrapper data AFTER the packfile:
    /// the packfile must be bounded (havok_packfile_size) and the trailing
    /// u32-swapped, so a BE self-offset 0xD8040000 becomes 0x000004D8 instead
    /// of relocating to an unmapped address (AV at 0x0248C13E).
    #[test]
    fn phy2_trailing_after_packfile_is_u32_swapped() {
        let pf_be = include_bytes!("../tests/fixtures/anim_ks750_be.bin");
        let pf_le = include_bytes!("../tests/fixtures/anim_ks750_le.bin");
        assert_eq!(havok_packfile_size(pf_be), Some(pf_be.len()), "packfile bound");

        let mut be = vec![0x00, 0x00, 0x00, 0x39, 0x12, 0x34, 0x56, 0x78]; // u32 header
        be.extend_from_slice(pf_be);
        be.extend_from_slice(&[0xD8, 0x04, 0x00, 0x00, 0xAA, 0xAA, 0xAA, 0xAA]); // BE trailing

        let mut want = vec![0x39, 0x00, 0x00, 0x00, 0x78, 0x56, 0x34, 0x12]; // header swapped
        want.extend_from_slice(pf_le);
        want.extend_from_slice(&[0x00, 0x00, 0x04, 0xD8, 0xAA, 0xAA, 0xAA, 0xAA]); // trailing swapped

        assert_eq!(convert_phy2_be_to_le(&be).unwrap(), want);
    }

    /// A PHY2 body with no embedded packfile falls back to a plain u32 sweep.
    #[test]
    fn phy2_without_magic_is_u32_sweep() {
        let be: [u8; 8] = [0x00, 0x00, 0x00, 0x39, 0x85, 0xE4, 0x08, 0x30];
        let got = convert_phy2_be_to_le(&be).expect("u32 sweep");
        assert_eq!(got, vec![0x39, 0x00, 0x00, 0x00, 0x30, 0x08, 0xE4, 0x85]);
    }
}
