//! The `ChatterSet` component's sites, found in the game image without per-build addresses, for a
//! `Human.SetChatterSet(character, name)` binding on PC: PS3 v1.03 registers it (`FUN_005300F8`), PC registers
//! none, and the Blow It Up Again DLC calls it on every outfit change. Byte signatures over `.text` in the form of
//! [`crate::lua_sites::Pattern`]. Roles, from the disassembly of retail v1.1 (v1.0 addresses in brackets):
//!
//! | site | VA v1.1 \[v1.0\] | what it is |
//! |---|---|---|
//! | [`Site::FieldWriter`] | `0x0065BD30` | One component's field writer; every component with one 4-byte field has one of this shape. It names the component (`mov edx, <name>`), saves the count at container `+0x1C`, adds the record (`EDI` = container `+0x18`, `push &value; push guid; call add`, the callee pops both), and when the count changed bumps container `+0x48` and, when the flag byte is set, marks the entity's component bit (`push (short)[container+4]; push guid; call mask; add esp, 8`). The one naming `"ChatterSet"` is the site |
//! | [`Site::Lookup`] | `0x00587D80` \[`0x005857E0`\] | A component's record for an entity (`ECX` = container, `EAX` = guid; returns the record or 0 in `EAX`): bit 3 of container `+8` picks one of two probes |
//!
//! `Human.SetChatterSet` on PS3 looks the record up and writes the name's `pandemic_hash_m2` into it; with no
//! record it adds one and does the field writer's bookkeeping.

use crate::exe_image::Image;
use crate::lua_sites::Pattern;

/// The component's name and its `pandemic_hash_m2`.
pub const COMPONENT: &str = "ChatterSet";
pub const COMPONENT_HASH: u32 = 0x949a_1e44;

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    FieldWriter,
    Lookup,
}

/// Offsets in the [`Site::FieldWriter`] pattern of the component's name (`mov edx, imm32`), the count (read
/// `mov esi, [moffs]`, compared again `cmp esi, [moffs]`), the add's base (`mov edi, imm32`), the add's
/// `call rel32`, the bump counter (`add dword [moffs], 1`), the flag (`cmp byte [moffs], 0`), the mask index
/// (`movsx edx, word [moffs]`) and the mask's `call rel32`.
const WRITER_NAME_AT: usize = 13;
const WRITER_COUNT_AT: usize = 65;
const WRITER_ADD_BASE_AT: usize = 76;
const WRITER_ADD_CALL_AT: usize = 80;
const WRITER_COUNT_AGAIN_AT: usize = 87;
const WRITER_BUMP_AT: usize = 95;
const WRITER_FLAG_AT: usize = 102;
const WRITER_INDEX_AT: usize = 112;
const WRITER_MASK_CALL_AT: usize = 118;
/// Where each global sits in the container: the add's base, the count, the bump counter, the mask index.
pub const CONTAINER_ADD_BASE: u32 = 0x18;
pub const CONTAINER_COUNT: u32 = 0x1c;
pub const CONTAINER_BUMP: u32 = 0x48;
pub const CONTAINER_MASK_INDEX: u32 = 0x04;

impl Site {
    pub const ALL: [Site; 2] = [Site::FieldWriter, Site::Lookup];

    pub fn name(self) -> &'static str {
        match self {
            Site::FieldWriter => "FieldWriter",
            Site::Lookup => "Lookup",
        }
    }

    /// The byte signature, as space-separated hex with `??` wildcards.
    fn pattern(self) -> &'static str {
        match self {
            // push ebp; mov ebp,esp; and esp,-8; push ecx; push ebx; push esi; push edi; push 0; mov edx,<name>;
            // call <read>; mov edx,[a]; mov ecx,[b]; mov esi,[ecx]; mov [esp+0x10],eax; mov eax,[c]; add esp,4;
            // add eax,edx; cdq; push 1; push edx; push eax; mov eax,[esi+0x1c]; call eax; mov ebx,[ebp+8];
            // mov esi,[count]; lea ecx,[esp+0xc]; push ecx; push ebx; mov edi,<add base>; call <add>;
            // cmp esi,[count]; je; add dword [bump],1; cmp byte [flag],0; je; movsx edx,word [index]; push edx;
            // push ebx; call <mask>; add esp,8
            Site::FieldWriter => {
                "55 8b ec 83 e4 f8 51 53 56 57 6a 00 ba ?? ?? ?? ?? e8 ?? ?? ?? ?? 8b 15 ?? ?? ?? ?? 8b 0d ?? ?? ?? ?? \
                 8b 31 89 44 24 10 a1 ?? ?? ?? ?? 83 c4 04 03 c2 99 6a 01 52 50 8b 46 1c ff d0 8b 5d 08 \
                 8b 35 ?? ?? ?? ?? 8d 4c 24 0c 51 53 bf ?? ?? ?? ?? e8 ?? ?? ?? ?? 3b 35 ?? ?? ?? ?? 74 ?? \
                 83 05 ?? ?? ?? ?? 01 80 3d ?? ?? ?? ?? 00 74 ?? 0f bf 15 ?? ?? ?? ?? 52 53 e8 ?? ?? ?? ?? 83 c4 08"
            }
            // mov edx,[ecx+8]; shr edx,3; test dl,1; je; push esi; push edi; lea edi,[ecx+0x18]; mov esi,eax;
            // mov ecx,edi; call <probe>; mov edx,eax; test edx,edx; jl; mov eax,[edi+0x1c]; cmp dword [eax+edx*4],-1;
            // je
            Site::Lookup => {
                "8b 51 08 c1 ea 03 f6 c2 01 74 ?? 56 57 8d 79 18 8b f0 8b cf e8 ?? ?? ?? ?? 8b d0 85 d2 7c ?? \
                 8b 47 1c 83 3c 90 ff 74 ??"
            }
        }
    }
}

/// What a `Human.SetChatterSet` binding needs: the container's globals and the three engine functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatterSites {
    /// The `ChatterSet` component container.
    pub container: u32,
    /// The field writer the other sites were read from.
    pub writer: u32,
    /// `ECX` = container, `EAX` = guid → the record or 0 in `EAX`.
    pub lookup: u32,
    /// `EDI` = container + [`CONTAINER_ADD_BASE`]; stack `(guid, &value)`, popped by the callee; adds or
    /// overwrites the entity's record.
    pub add: u32,
    /// `cdecl (guid, (short) index)`: sets the entity's bit for the component.
    pub mask: u32,
    /// The byte gating the mask call.
    pub flag: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatterSitesError {
    NoSection(&'static str),
    /// The lookup's match count, when it is not exactly one.
    Lookup(usize),
    /// How many field writers matched, and how many of them name `ChatterSet`, when that is not one.
    Writer { writers: usize, named: usize },
    /// The `ChatterSet` writer's operands are not one container's globals.
    Inconsistent { site: u32, why: String },
}

impl std::fmt::Display for ChatterSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatterSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            ChatterSitesError::Lookup(n) => write!(f, "the component lookup matched {n} time(s), not once"),
            ChatterSitesError::Writer { writers, named } => {
                write!(f, "{writers} component field writer(s) matched, {named} of them name \"{COMPONENT}\"; need exactly one")
            }
            ChatterSitesError::Inconsistent { site, why } => write!(f, "the {COMPONENT} writer at {site:#x} is inconsistent: {why}"),
        }
    }
}

impl std::error::Error for ChatterSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Resolve both sites in `img` and read the container's globals out of the `ChatterSet` field writer.
pub fn resolve(img: &Image) -> Result<ChatterSites, ChatterSitesError> {
    let text = img.section(".text").ok_or(ChatterSitesError::NoSection(".text"))?;
    let find = |s: Site| -> Vec<u32> {
        Pattern::parse(s.pattern()).expect("built-in pattern parses").find_all(text.bytes).iter().map(|&o| text.va + o as u32).collect()
    };
    let lookups = find(Site::Lookup);
    if lookups.len() != 1 {
        return Err(ChatterSitesError::Lookup(lookups.len()));
    }
    let writers = find(Site::FieldWriter);
    let named: Vec<u32> = writers
        .iter()
        .copied()
        .filter(|&w| {
            let b = &text.bytes[(w - text.va) as usize..];
            img.cstr_at(u32_at(b, WRITER_NAME_AT)) == Some(COMPONENT.as_bytes())
        })
        .collect();
    let [writer] = named[..] else {
        return Err(ChatterSitesError::Writer { writers: writers.len(), named: named.len() });
    };
    let b = &text.bytes[(writer - text.va) as usize..];
    let bad = |why: String| ChatterSitesError::Inconsistent { site: writer, why };
    let add_base = u32_at(b, WRITER_ADD_BASE_AT);
    let container = add_base.wrapping_sub(CONTAINER_ADD_BASE);
    for (what, at, offset) in [
        ("count", WRITER_COUNT_AT, CONTAINER_COUNT),
        ("count, compared again,", WRITER_COUNT_AGAIN_AT, CONTAINER_COUNT),
        ("bump counter", WRITER_BUMP_AT, CONTAINER_BUMP),
        ("mask index", WRITER_INDEX_AT, CONTAINER_MASK_INDEX),
    ] {
        let have = u32_at(b, at);
        let want = container.wrapping_add(offset);
        if have != want {
            return Err(bad(format!("its {what} is {have:#x}, not container {container:#x} + {offset:#x}")));
        }
    }
    let call_target = |at: usize| writer.wrapping_add(at as u32 + 5).wrapping_add(u32_at(b, at + 1));
    let (add, mask, flag) = (call_target(WRITER_ADD_CALL_AT), call_target(WRITER_MASK_CALL_AT), u32_at(b, WRITER_FLAG_AT));
    for (what, va) in [("the add", add), ("the mask setter", mask)] {
        if !text.contains(va) {
            return Err(bad(format!("{what} {va:#x} is not in .text")));
        }
    }
    for (what, va) in [("the container", container), ("the bump counter", container + CONTAINER_BUMP), ("the flag", flag)] {
        if img.u32_at(va).is_none() {
            return Err(bad(format!("{what} {va:#x} is not in the image")));
        }
    }
    Ok(ChatterSites { container, writer, lookup: lookups[0], add, mask, flag })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    /// Where the sites sit in the synthetic `.text` (at 0x401000).
    const WRITERS_AT: [usize; 2] = [0x100, 0x200];
    const LOOKUP_AT: usize = 0x300;
    const ADD_AT: usize = 0x400;
    const MASK_AT: usize = 0x500;
    /// The two writers' names in `.rdata` (at 0x405000).
    const NAMES: [u32; 2] = [0x405000, 0x405020];

    #[derive(Clone)]
    struct Writer {
        name: u32,
        container: u32,
        count: u32,
        count_again: u32,
        bump: u32,
        index: u32,
        flag: u32,
        add_to: usize,
        mask_to: usize,
    }

    fn writer(name: u32, container: u32) -> Writer {
        Writer {
            name,
            container,
            count: container + CONTAINER_COUNT,
            count_again: container + CONTAINER_COUNT,
            bump: container + CONTAINER_BUMP,
            index: container + CONTAINER_MASK_INDEX,
            flag: 0x403300,
            add_to: ADD_AT,
            mask_to: MASK_AT,
        }
    }

    fn concrete(s: Site, fill: u8) -> Vec<u8> {
        s.pattern().split_ascii_whitespace().map(|t| if t == "??" { fill } else { u8::from_str_radix(t, 16).unwrap() }).collect()
    }

    fn put(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn put_call(b: &mut [u8], site: usize, at: usize, to: usize) {
        put(b, at + 1, (to as i64 - (site + at + 5) as i64) as i32 as u32);
    }

    /// `.text` with two field writers (`ws`) and `lookups` copies of the lookup; `.rdata` with the names
    /// `"ChatterSet"` and `"Perception"`; `.data` at 0x403000.
    fn image(ws: &[Writer], lookups: usize) -> Vec<u8> {
        let mut text = vec![0xcc; 0x600];
        for (k, w) in ws.iter().enumerate() {
            let mut b = concrete(Site::FieldWriter, 0x11);
            put(&mut b, WRITER_NAME_AT, w.name);
            put(&mut b, WRITER_COUNT_AT, w.count);
            put(&mut b, WRITER_ADD_BASE_AT, w.container + CONTAINER_ADD_BASE);
            put_call(&mut b, WRITERS_AT[k], WRITER_ADD_CALL_AT, w.add_to);
            put(&mut b, WRITER_COUNT_AGAIN_AT, w.count_again);
            put(&mut b, WRITER_BUMP_AT, w.bump);
            put(&mut b, WRITER_FLAG_AT, w.flag);
            put(&mut b, WRITER_INDEX_AT, w.index);
            put_call(&mut b, WRITERS_AT[k], WRITER_MASK_CALL_AT, w.mask_to);
            text[WRITERS_AT[k]..WRITERS_AT[k] + b.len()].copy_from_slice(&b);
        }
        for k in 0..lookups {
            let b = concrete(Site::Lookup, 0x22);
            let at = LOOKUP_AT + k * 0x40;
            text[at..at + b.len()].copy_from_slice(&b);
        }
        let mut rdata = vec![0u8; 0x40];
        rdata[..10].copy_from_slice(b"ChatterSet");
        rdata[0x20..0x2a].copy_from_slice(b"Perception");
        synth::pe(0x400000, &[(".text", 0x1000, text), (".data", 0x3000, vec![0; 0x400]), (".rdata", 0x5000, rdata)])
    }

    fn resolve_file(file: &[u8]) -> Result<ChatterSites, ChatterSitesError> {
        resolve(&Image::from_file(file).unwrap())
    }

    #[test]
    fn every_built_in_pattern_parses_and_its_offsets_name_their_operands() {
        for s in Site::ALL {
            Pattern::parse(s.pattern()).unwrap_or_else(|e| panic!("{}: {e:?}", s.name()));
        }
        let w: Vec<&str> = Site::FieldWriter.pattern().split_whitespace().collect();
        for (at, op) in [
            (WRITER_NAME_AT, &["ba"][..]),
            (WRITER_COUNT_AT, &["8b", "35"]),
            (WRITER_ADD_BASE_AT, &["bf"]),
            (WRITER_COUNT_AGAIN_AT, &["3b", "35"]),
            (WRITER_BUMP_AT, &["83", "05"]),
            (WRITER_FLAG_AT, &["80", "3d"]),
            (WRITER_INDEX_AT, &["0f", "bf", "15"]),
        ] {
            assert_eq!(w[at - op.len()..at], *op, "the opcode before the operand at {at}");
            assert_eq!(w[at..at + 4], ["??"; 4], "the operand at {at} is read, not matched");
        }
        assert_eq!((w[WRITER_ADD_CALL_AT], w[WRITER_MASK_CALL_AT]), ("e8", "e8"));
        assert_eq!(crate::hash::pandemic_hash_m2(COMPONENT), COMPONENT_HASH);
    }

    #[test]
    fn the_chatter_set_writer_names_the_container_and_the_engine_functions() {
        let s = resolve_file(&image(&[writer(NAMES[1], 0x403100), writer(NAMES[0], 0x403200)], 1)).unwrap();
        assert_eq!(
            s,
            ChatterSites { container: 0x403200, writer: 0x401200, lookup: 0x401300, add: 0x401400, mask: 0x401500, flag: 0x403300 }
        );
    }

    #[test]
    fn the_writer_must_be_one_and_name_one_container() {
        let check = |ws: Vec<Writer>, lookups: usize| resolve_file(&image(&ws, lookups)).unwrap_err().to_string();
        let w = || writer(NAMES[0], 0x403200);
        assert!(check(vec![writer(NAMES[1], 0x403100)], 1).contains("1 component field writer(s) matched, 0 of them name"));
        assert!(check(vec![w(), writer(NAMES[0], 0x403100)], 1).contains("2 of them name"));
        assert!(check(vec![w()], 0).contains("matched 0 time(s)"));
        assert!(check(vec![w()], 2).contains("matched 2 time(s)"));
        let e = check(vec![Writer { count: 0x403240, ..w() }], 1);
        assert!(e.contains("its count is 0x403240, not container 0x403200 + 0x1c"), "{e}");
        let e = check(vec![Writer { count_again: 0x403240, ..w() }], 1);
        assert!(e.contains("compared again"), "{e}");
        let e = check(vec![Writer { bump: 0x403250, ..w() }], 1);
        assert!(e.contains("bump counter is 0x403250"), "{e}");
        let e = check(vec![Writer { index: 0x403208, ..w() }], 1);
        assert!(e.contains("mask index is 0x403208"), "{e}");
        let e = check(vec![Writer { add_to: 0x9000, ..w() }], 1);
        assert!(e.contains("the add") && e.contains("not in .text"), "{e}");
        let e = check(vec![Writer { flag: 0x700000, ..w() }], 1);
        assert!(e.contains("the flag 0x700000 is not in the image"), "{e}");
    }
}
