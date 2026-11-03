//! The billboard-tree render loops' sites, found in the game image without per-build addresses, for the
//! BUG-014 fix (`docs/fixpack/bug_register.md`): the tree Z-prepass loop never pushes a world matrix, so its
//! packets draw with whatever matrix the previous draw pushed. The shadow tree loop pushes the identity
//! first; the fix detours the Z-prepass loop and does the same before it runs. Byte signatures over `.text`
//! in the form of [`crate::lua_sites::Pattern`], cross-checked against the frame stores
//! ([`crate::profiler_sites::FrameStores`]) and the world-matrix push ([`profiler_sites::Site::MatrixIndex`]).
//! Roles, from the decompilation of retail v1.1 (v1.0 addresses in brackets):
//!
//! | site | VA v1.1 \[v1.0\] | what it is |
//! |---|---|---|
//! | [`Site::TreeShadowLoop`] | `0x004A3E10` \[`0x004A3DB0`\] | `void __cdecl (void)`: `i = counter++`; copies the identity (`FUN_00836120`, `EAX` = destination, `ECX` = source, 64 bytes) to the matrix table's entry `(parity · cap + i)`; `matrix index = i`; `draw record = 1`; then draws every tree batch |
//! | [`Site::TreeZPassLoop`] | `0x004A3EA0` \[`0x004A3E40`\] | `void __cdecl (void)`, reads no register it did not set: draws every tree batch's Z-prepass (`FUN_004A1D70`, `FUN_004A1F30`, packet class 3). It pushes no matrix. Its only caller (`FUN_00468EA0`) calls it after each object's draw and, for objects flagged for one, its occlusion box (`FUN_0046EDF0`), which pushes the box's matrix and sets the matrix index |
//!
//! So the Z-prepass packets carry the matrix index (`0x01163740` in v1.1) last pushed before the loop: in the
//! frame captures of BUG-014, an occlusion box's. The index is the only box-specific state that reaches them.

use crate::exe_image::Image;
use crate::lua_sites::Pattern;
use crate::profiler_sites::{self, FrameStores, ProfilerSitesError};

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    TreeShadowLoop,
    TreeZPassLoop,
}

/// Offsets in the [`Site::TreeShadowLoop`] pattern of the matrix counter (read `mov ax, [moffs]`, written
/// back `mov [moffs], ax`), the parity (`mov eax, [moffs]`), the per-parity capacity (`imul eax, eax,
/// imm32`), the table (`add eax, imm32`), the identity (`mov ecx, imm32`), the copy's `call rel32`, the
/// matrix index (`mov [moffs], si`), the batch count (`cmp [moffs], esi`) and the draw record (`mov word
/// [moffs], 1`).
const SHADOW_COUNTER_AT: usize = 2;
const SHADOW_COUNTER_STORE_AT: usize = 17;
const SHADOW_PARITY_AT: usize = 22;
const SHADOW_CAP_AT: usize = 28;
const SHADOW_TABLE_AT: usize = 42;
const SHADOW_IDENTITY_AT: usize = 47;
const SHADOW_COPY_CALL_AT: usize = 51;
const SHADOW_INDEX_AT: usize = 59;
const SHADOW_BATCHES_AT: usize = 67;
const SHADOW_RECORD_AT: usize = 74;
/// Offset in the [`Site::TreeZPassLoop`] pattern of the batch count (`mov eax, [moffs]`).
const ZPASS_BATCHES_AT: usize = 1;
/// From the [`profiler_sites::Site::MatrixIndex`] site: its copy's `call rel32`, its `mov [matrix index],
/// di` (`66 89 3d moffs`) and its `mov word [draw record], 1` (`66 c7 05 moffs 01 00`).
const PUSH_COPY_CALL_AT: usize = 73;
const PUSH_INDEX_AT: usize = 87;
const PUSH_RECORD_AT: usize = 98;
/// A world-matrix table entry: a 4×4 `f32` matrix.
pub const MATRIX_BYTES: u32 = 0x40;
/// The identity, as the 16 `f32` words of a 4×4 matrix.
const IDENTITY: [u32; 16] = {
    let one = 0x3f80_0000;
    [one, 0, 0, 0, 0, one, 0, 0, 0, 0, one, 0, 0, 0, 0, one]
};

impl Site {
    pub const ALL: [Site; 2] = [Site::TreeShadowLoop, Site::TreeZPassLoop];

    pub fn name(self) -> &'static str {
        match self {
            Site::TreeShadowLoop => "TreeShadowLoop",
            Site::TreeZPassLoop => "TreeZPassLoop",
        }
    }

    /// The byte signature, as space-separated hex with `??` wildcards.
    fn pattern(self) -> &'static str {
        match self {
            // mov ax,[counter]; push ebx; push esi; movzx esi,ax; add ax,1; mov [counter],ax; mov eax,[parity];
            // imul eax,eax,<cap>; movzx ecx,si; add eax,ecx; shl eax,6; push edi; add eax,<table>;
            // mov ecx,<identity>; call <copy>; mov [matrix index],si; xor esi,esi; cmp [batches],esi;
            // mov word [draw record],1; jbe
            Site::TreeShadowLoop => {
                "66 a1 ?? ?? ?? ?? 53 56 0f b7 f0 66 05 01 00 66 a3 ?? ?? ?? ?? a1 ?? ?? ?? ?? 69 c0 ?? ?? ?? ?? \
                 0f b7 ce 03 c1 c1 e0 06 57 05 ?? ?? ?? ?? b9 ?? ?? ?? ?? e8 ?? ?? ?? ?? 66 89 35 ?? ?? ?? ?? \
                 33 f6 39 35 ?? ?? ?? ?? 66 c7 05 ?? ?? ?? ?? 01 00 76"
            }
            // mov eax,[batches]; push ebx; push esi; xor esi,esi; test eax,eax; push edi; jbe <second loop>;
            // xor ebx,ebx; mov eax,[batch array]; lea edi,[ebx+eax]; call <Z-prepass draw>
            Site::TreeZPassLoop => "a1 ?? ?? ?? ?? 53 56 33 f6 85 c0 57 76 ?? 33 db a1 ?? ?? ?? ?? 8d 3c 03 e8 ?? ?? ?? ??",
        }
    }
}

/// What the BUG-014 fix needs: the loop to detour and everything the shadow loop's identity push touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeSites {
    /// The tree Z-prepass loop, `void __cdecl (void)`: the hook target.
    pub zpass_loop: u32,
    /// The tree shadow loop, whose identity push the fix repeats.
    pub shadow_loop: u32,
    /// The 64-byte identity matrix the shadow loop copies.
    pub identity: u32,
    /// The 64-byte matrix copy both pushes call (`EAX` = destination, `ECX` = source).
    pub matrix_copy: u32,
    /// The `u16` matrix index the packet producer stores in each packet (`+0x38`).
    pub matrix_index: u32,
    /// The `u16` the producer stores beside it (`+0x3A`): an index into the frame's 32-byte draw-record
    /// table, which both tree identity pushes and the matrix push set to 1.
    pub draw_record: u32,
    /// The tree batch count both loops iterate.
    pub batches: u32,
    /// The world-matrix counter (`u16`), the parity, the table and its entries per parity, as
    /// [`FrameStores`] names them.
    pub matrices: u32,
    pub parity: u32,
    pub matrix_table: u32,
    pub matrix_cap: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeSitesError {
    NoSection(&'static str),
    /// Each site's match count, when any is not exactly one.
    Matches(Vec<(Site, usize)>),
    /// The frame stores and the matrix push did not resolve.
    Profiler(ProfilerSitesError),
    /// The tree loops do not name the stores the frame-store sites name, or each other's.
    Inconsistent { site: u32, why: String },
}

impl std::fmt::Display for TreeSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            TreeSitesError::Matches(m) => write!(
                f,
                "tree sites did not resolve to exactly one match each: {}",
                m.iter().map(|(s, n)| format!("{}={n} match(es)", s.name())).collect::<Vec<_>>().join(", ")
            ),
            TreeSitesError::Profiler(e) => write!(f, "the frame stores the tree sites are checked against: {e}"),
            TreeSitesError::Inconsistent { site, why } => write!(f, "the tree sites at {site:#x} are inconsistent: {why}"),
        }
    }
}

impl std::error::Error for TreeSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Resolve both loops in `img` and check them against the frame stores and the matrix push
/// ([`profiler_sites::resolve`]).
pub fn resolve(img: &Image) -> Result<TreeSites, TreeSitesError> {
    let p = profiler_sites::resolve(img).map_err(TreeSitesError::Profiler)?;
    resolve_against(img, &p.frame_stores(), p.va(profiler_sites::Site::MatrixIndex))
}

/// Both loops in `img`, checked against `stores` and the matrix push at `push`.
fn resolve_against(img: &Image, stores: &FrameStores, push: u32) -> Result<TreeSites, TreeSitesError> {
    let text = img.section(".text").ok_or(TreeSitesError::NoSection(".text"))?;
    let found: Vec<(Site, Vec<u32>)> = Site::ALL
        .iter()
        .map(|&s| {
            let hits = Pattern::parse(s.pattern()).expect("built-in pattern parses").find_all(text.bytes);
            (s, hits.iter().map(|&o| text.va + o as u32).collect())
        })
        .collect();
    if !found.iter().all(|(_, v)| v.len() == 1) {
        return Err(TreeSitesError::Matches(found.iter().map(|(s, v)| (*s, v.len())).collect()));
    }
    let (shadow_loop, zpass_loop) = (found[0].1[0], found[1].1[0]);
    let bytes = |v: u32, n: usize| text.bytes.get((v.wrapping_sub(text.va)) as usize..).and_then(|b| b.get(..n));
    let bad = |why: String| TreeSitesError::Inconsistent { site: shadow_loop, why };

    let s = bytes(shadow_loop, SHADOW_RECORD_AT + 4).expect("a matched pattern lies in .text");
    let matrices = u32_at(s, SHADOW_COUNTER_AT);
    if u32_at(s, SHADOW_COUNTER_STORE_AT) != matrices {
        return Err(bad(format!("the shadow loop reads the matrix counter at {matrices:#x} but writes {:#x}", u32_at(s, SHADOW_COUNTER_STORE_AT))));
    }
    for (what, have, want) in [
        ("matrix counter", matrices, stores.matrices),
        ("parity", u32_at(s, SHADOW_PARITY_AT), stores.parity),
        ("matrix capacity", u32_at(s, SHADOW_CAP_AT), stores.matrix_cap),
        ("matrix table", u32_at(s, SHADOW_TABLE_AT), stores.matrix_table),
    ] {
        if have != want {
            return Err(bad(format!("the shadow loop's {what} is {have:#x}, the frame stores' is {want:#x}")));
        }
    }
    let identity = u32_at(s, SHADOW_IDENTITY_AT);
    let call_target = |site: u32, at: usize| site.wrapping_add(at as u32 + 5).wrapping_add(u32_at(bytes(site, at + 5).expect("checked length"), at + 1));
    let matrix_copy = call_target(shadow_loop, SHADOW_COPY_CALL_AT);
    let matrix_index = u32_at(s, SHADOW_INDEX_AT);
    let batches = u32_at(s, SHADOW_BATCHES_AT);
    let draw_record = u32_at(s, SHADOW_RECORD_AT);
    if draw_record != matrix_index + 2 {
        return Err(bad(format!("the draw record {draw_record:#x} is not the u16 after the matrix index {matrix_index:#x}")));
    }

    let z = bytes(zpass_loop, ZPASS_BATCHES_AT + 4).expect("a matched pattern lies in .text");
    if u32_at(z, ZPASS_BATCHES_AT) != batches {
        return Err(bad(format!("the Z-prepass loop at {zpass_loop:#x} counts batches at {:#x}, the shadow loop at {batches:#x}", u32_at(z, ZPASS_BATCHES_AT))));
    }

    let p = bytes(push, PUSH_RECORD_AT + 9).ok_or_else(|| bad(format!("the matrix push at {push:#x} runs past .text")))?;
    if p[PUSH_COPY_CALL_AT] != 0xe8 || call_target(push, PUSH_COPY_CALL_AT) != matrix_copy {
        return Err(bad(format!(
            "the matrix push's +{PUSH_COPY_CALL_AT} is not a call to the shadow loop's copy {matrix_copy:#x}: {:02x?}",
            &p[PUSH_COPY_CALL_AT..PUSH_COPY_CALL_AT + 5]
        )));
    }
    if p[PUSH_INDEX_AT..PUSH_INDEX_AT + 3] != [0x66, 0x89, 0x3d] || u32_at(p, PUSH_INDEX_AT + 3) != matrix_index {
        return Err(bad(format!(
            "the matrix push's +{PUSH_INDEX_AT} is {:02x?}, not mov [{matrix_index:#x}], di",
            &p[PUSH_INDEX_AT..PUSH_INDEX_AT + 7]
        )));
    }
    if p[PUSH_RECORD_AT..PUSH_RECORD_AT + 3] != [0x66, 0xc7, 0x05] || u32_at(p, PUSH_RECORD_AT + 3) != draw_record || p[PUSH_RECORD_AT + 7..] != [0x01, 0x00] {
        return Err(bad(format!(
            "the matrix push's +{PUSH_RECORD_AT} is {:02x?}, not mov word [{draw_record:#x}], 1",
            &p[PUSH_RECORD_AT..PUSH_RECORD_AT + 9]
        )));
    }

    for (i, want) in IDENTITY.iter().enumerate() {
        let va = identity + 4 * i as u32;
        match img.u32_at(va) {
            Some(w) if w == *want => {}
            Some(w) => return Err(bad(format!("the identity {identity:#x} holds {w:#010x} at word {i}, not {want:#010x}"))),
            None => return Err(bad(format!("the identity's word {i} ({va:#x}) is not in the image"))),
        }
    }
    for (what, va) in [("the matrix index", matrix_index), ("the batch count", batches)] {
        if img.u32_at(va).is_none() {
            return Err(bad(format!("{what} {va:#x} is not in the image")));
        }
    }
    if bytes(matrix_copy, 1).is_none() {
        return Err(bad(format!("the matrix copy {matrix_copy:#x} is not in .text")));
    }
    Ok(TreeSites {
        zpass_loop,
        shadow_loop,
        identity,
        matrix_copy,
        matrix_index,
        draw_record,
        batches,
        matrices,
        parity: stores.parity,
        matrix_table: stores.matrix_table,
        matrix_cap: stores.matrix_cap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    /// Where the sites sit in the synthetic `.text` (at 0x401000), and the copy function.
    const SHADOW_AT: usize = 0x100;
    const ZPASS_AT: usize = 0x200;
    const PUSH_AT: usize = 0x300;
    const COPY_AT: usize = 0x400;

    /// What the two loops and the matrix push name.
    #[derive(Clone)]
    struct Loops {
        counter: u32,
        counter_store: u32,
        parity: u32,
        cap: u32,
        table: u32,
        identity: u32,
        copy_to: usize,
        index: u32,
        batches: u32,
        record: u32,
        zpass_batches: u32,
        push_copy_to: usize,
        push_index: u32,
        push_record: u32,
        /// The identity's 16 words as the `.data` holds them.
        identity_words: [u32; 16],
    }

    fn loops() -> Loops {
        Loops {
            counter: 0x403000,
            counter_store: 0x403000,
            parity: 0x403004,
            cap: 0x1400,
            table: 0x1000000,
            identity: 0x403100,
            copy_to: COPY_AT,
            index: 0x403010,
            batches: 0x403020,
            record: 0x403012,
            zpass_batches: 0x403020,
            push_copy_to: COPY_AT,
            push_index: 0x403010,
            push_record: 0x403012,
            identity_words: IDENTITY,
        }
    }

    fn stores() -> FrameStores {
        FrameStores {
            begin: 0x401800,
            packets: 0x403030,
            packet_cap: 0x2000,
            list_dwords: 0x403034,
            list_bytes: 0x50000,
            sidecar_bytes_used: 0x403038,
            sidecar_bytes: 0x20000,
            matrices: 0x403000,
            matrix_cap: 0x1400,
            parity: 0x403004,
            packet_array: 0x1200000,
            packet_bytes: 0x58,
            matrix_table: 0x1000000,
            list_base: 0x403040,
            sidecar_base: 0x403044,
            draw_program: 0x403048,
            draw_class: 0x40304c,
        }
    }

    fn concrete(s: Site, fill: u8) -> Vec<u8> {
        s.pattern().split_ascii_whitespace().map(|t| if t == "??" { fill } else { u8::from_str_radix(t, 16).unwrap() }).collect()
    }

    fn put(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// `call rel32` at `site + at` reaching `.text` offset `to`.
    fn put_call(b: &mut [u8], site: usize, at: usize, to: usize) {
        b[at] = 0xe8;
        put(b, at + 1, (to as i64 - (site + at + 5) as i64) as i32 as u32);
    }

    fn image(l: &Loops) -> Vec<u8> {
        let mut shadow = concrete(Site::TreeShadowLoop, 0x11);
        for (at, v) in [
            (SHADOW_COUNTER_AT, l.counter),
            (SHADOW_COUNTER_STORE_AT, l.counter_store),
            (SHADOW_PARITY_AT, l.parity),
            (SHADOW_CAP_AT, l.cap),
            (SHADOW_TABLE_AT, l.table),
            (SHADOW_IDENTITY_AT, l.identity),
            (SHADOW_INDEX_AT, l.index),
            (SHADOW_BATCHES_AT, l.batches),
            (SHADOW_RECORD_AT, l.record),
        ] {
            put(&mut shadow, at, v);
        }
        put_call(&mut shadow, SHADOW_AT, SHADOW_COPY_CALL_AT, l.copy_to);
        let mut zpass = concrete(Site::TreeZPassLoop, 0x22);
        put(&mut zpass, ZPASS_BATCHES_AT, l.zpass_batches);
        let mut push = vec![0x90; PUSH_RECORD_AT + 9];
        put_call(&mut push, PUSH_AT, PUSH_COPY_CALL_AT, l.push_copy_to);
        push[PUSH_INDEX_AT..PUSH_INDEX_AT + 3].copy_from_slice(&[0x66, 0x89, 0x3d]);
        put(&mut push, PUSH_INDEX_AT + 3, l.push_index);
        push[PUSH_RECORD_AT..PUSH_RECORD_AT + 3].copy_from_slice(&[0x66, 0xc7, 0x05]);
        put(&mut push, PUSH_RECORD_AT + 3, l.push_record);
        push[PUSH_RECORD_AT + 7..].copy_from_slice(&[0x01, 0x00]);
        let mut text = vec![0xcc; 0x500];
        for (at, b) in [(SHADOW_AT, shadow), (ZPASS_AT, zpass), (PUSH_AT, push)] {
            text[at..at + b.len()].copy_from_slice(&b);
        }
        let mut data = vec![0; 0x200];
        for (i, w) in l.identity_words.iter().enumerate() {
            put(&mut data, 0x100 + 4 * i, *w);
        }
        synth::pe(0x400000, &[(".text", 0x1000, text), (".data", 0x3000, data)])
    }

    fn resolve_loops(l: &Loops) -> Result<TreeSites, TreeSitesError> {
        resolve_against(&Image::from_file(&image(l)).unwrap(), &stores(), 0x400000 + 0x1000 + PUSH_AT as u32)
    }

    #[test]
    fn every_built_in_pattern_parses_and_its_offsets_name_their_operands() {
        for s in Site::ALL {
            Pattern::parse(s.pattern()).unwrap_or_else(|e| panic!("{}: {e:?}", s.name()));
        }
        let shadow: Vec<&str> = Site::TreeShadowLoop.pattern().split_whitespace().collect();
        assert_eq!(shadow.len(), SHADOW_RECORD_AT + 7);
        for (at, op) in [
            (SHADOW_COUNTER_AT, &["66", "a1"][..]),
            (SHADOW_COUNTER_STORE_AT, &["66", "a3"]),
            (SHADOW_PARITY_AT, &["a1"]),
            (SHADOW_CAP_AT, &["69", "c0"]),
            (SHADOW_TABLE_AT, &["05"]),
            (SHADOW_IDENTITY_AT, &["b9"]),
            (SHADOW_INDEX_AT, &["66", "89", "35"]),
            (SHADOW_BATCHES_AT, &["39", "35"]),
            (SHADOW_RECORD_AT, &["66", "c7", "05"]),
        ] {
            assert_eq!(shadow[at - op.len()..at], *op, "the opcode before the operand at {at}");
            assert_eq!(shadow[at..at + 4], ["??"; 4], "the operand at {at} is read, not matched");
        }
        assert_eq!(shadow[SHADOW_COPY_CALL_AT], "e8");
        let zpass: Vec<&str> = Site::TreeZPassLoop.pattern().split_whitespace().collect();
        assert_eq!(zpass[ZPASS_BATCHES_AT - 1], "a1");
    }

    #[test]
    fn the_loops_resolve_and_name_what_the_identity_push_touches() {
        let s = resolve_loops(&loops()).unwrap();
        assert_eq!(
            s,
            TreeSites {
                zpass_loop: 0x401200,
                shadow_loop: 0x401100,
                identity: 0x403100,
                matrix_copy: 0x401400,
                matrix_index: 0x403010,
                draw_record: 0x403012,
                batches: 0x403020,
                matrices: 0x403000,
                parity: 0x403004,
                matrix_table: 0x1000000,
                matrix_cap: 0x1400,
            }
        );
    }

    #[test]
    fn the_loops_must_name_the_frame_stores_the_matrix_push_and_each_other() {
        let check = |l: Loops| resolve_loops(&l).unwrap_err().to_string();
        let e = check(Loops { counter_store: 0x403008, ..loops() });
        assert!(e.contains("reads the matrix counter at 0x403000 but writes 0x403008"), "{e}");
        let e = check(Loops { counter: 0x403008, counter_store: 0x403008, ..loops() });
        assert!(e.contains("matrix counter is 0x403008, the frame stores' is 0x403000"), "{e}");
        let e = check(Loops { parity: 0x403008, ..loops() });
        assert!(e.contains("parity is 0x403008"), "{e}");
        let e = check(Loops { cap: 0x1000, ..loops() });
        assert!(e.contains("matrix capacity is 0x1000"), "{e}");
        let e = check(Loops { table: 0x1100000, ..loops() });
        assert!(e.contains("matrix table is 0x1100000"), "{e}");
        let e = check(Loops { record: 0x403014, push_record: 0x403014, ..loops() });
        assert!(e.contains("is not the u16 after the matrix index"), "{e}");
        let e = check(Loops { zpass_batches: 0x403024, ..loops() });
        assert!(e.contains("counts batches at 0x403024, the shadow loop at 0x403020"), "{e}");
        let e = check(Loops { push_copy_to: COPY_AT + 0x10, ..loops() });
        assert!(e.contains("is not a call to the shadow loop's copy 0x401400"), "{e}");
        let e = check(Loops { push_index: 0x403018, ..loops() });
        assert!(e.contains("not mov [0x403010], di"), "{e}");
        let e = check(Loops { push_record: 0x403016, ..loops() });
        assert!(e.contains("not mov word [0x403012], 1"), "{e}");
        let mut words = IDENTITY;
        words[3] = 0x3f80_0000;
        let e = check(Loops { identity_words: words, ..loops() });
        assert!(e.contains("holds 0x3f800000 at word 3, not 0x00000000"), "{e}");
        let e = check(Loops { identity: 0x500000, ..loops() });
        assert!(e.contains("is not in the image"), "{e}");
        let e = check(Loops { batches: 0x500000, zpass_batches: 0x500000, ..loops() });
        assert!(e.contains("the batch count 0x500000 is not in the image"), "{e}");
    }

    #[test]
    fn a_missing_or_doubled_loop_reports_its_count() {
        let empty = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64])]);
        let e = resolve_against(&Image::from_file(&empty).unwrap(), &stores(), 0x401000).unwrap_err();
        assert_eq!(e, TreeSitesError::Matches(vec![(Site::TreeShadowLoop, 0), (Site::TreeZPassLoop, 0)]));
        assert!(e.to_string().contains("TreeShadowLoop=0 match(es)"), "{e}");
        let mut text = concrete(Site::TreeShadowLoop, 0);
        text.extend(concrete(Site::TreeZPassLoop, 0));
        text.extend(concrete(Site::TreeZPassLoop, 0xff));
        let doubled = synth::pe(0x400000, &[(".text", 0x1000, text)]);
        let e = resolve_against(&Image::from_file(&doubled).unwrap(), &stores(), 0x401000).unwrap_err();
        assert_eq!(e, TreeSitesError::Matches(vec![(Site::TreeShadowLoop, 1), (Site::TreeZPassLoop, 2)]));
    }
}
