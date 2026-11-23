//! The display-mode sites, found in the game image without per-build addresses, for the BUG-016 fix
//! ([`docs/fixpack/bug_register.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/fixpack/bug_register.md)): the render state's constructor keeps a mode from
//! `IDirect3D9::EnumAdapterModes` only at 85 Hz or less and only near 4:3 or 16:9 (v1.0; v1.1 keeps
//! five aspect bands), and `RenderSystem::ApplySettings` falls back to 640×480 at 60 Hz when the
//! `Mercs2.ini` mode is not kept, then returns an error its caller never checks when that is not kept
//! either, and the renderer reads a NULL device. Byte signatures over `.text` in the form of
//! [`crate::lua_sites::Pattern`], cross-checked against each other. Roles, from the disassembly of retail
//! v1.1 (v1.0 addresses in brackets):
//!
//! | site | VA v1.1 \[v1.0\] | what it is |
//! |---|---|---|
//! | [`Site::Filter`] | `0x00754DE3` \[`0x00754BE9`\] | in the constructor's per-mode loop: skip when width < 640, height < 480 (`EDX`, `ECX`), then `mov esi, refresh; cmp esi, 0x55; ja skip` at `+0x24`, then the aspect test, then `keep` |
//! | [`Site::Keep`] | `0x00754E7B` \[`0x00754C80`\] | the loop's keep path: `push esi; push 0x20; push ecx; push edx` (refresh, bpp, height, width), the mode search on the state (`ECX` = `EBP` \[`EBX`\]) to drop a duplicate, then the record build. It reads nothing the aspect test writes (`EAX` is rewritten first; the test's stack stores are dead), and the aspect test leaves the x87 stack as it found it |
//! | [`Site::FirstSearch`] | `0x0074CCA7` \[`0x0074CA97`\] | `ApplySettings` searches the list for the settings' mode: pushes refresh, `(bpp index + 1) << 4`, height, width from the settings globals, `EAX` = an 8-byte out record, `ECX` = the pending state; `call` at `+0x24` |
//! | [`Site::Fallback`] | `0x0074CCEB` \[`0x0074CADB`\] | not found: `mov ecx, [pending state]`, pushes 60, 32, 480, 640, writes them to the settings globals, searches again (`call` at `+0x43`); not found again returns `D3DERR_NOTAVAILABLE` |
//! | [`Site::SetMode`] | `0x00755570` \[`0x00755380`\] | `stdcall (record lo, record hi)`: `ESI` = the pending state, its adapter `[esi]`, re-queries `EnumAdapterModes` by the record's index byte and fills the present parameters |
//! | [`Site::FindModeBody`] | `0x007556FB` \[`0x0075550B`\] | the mode search (`ECX` = state, `ESI` = count, `EAX` = out record, `ret 0x10`): the list is `[ecx + list]`; exact match on width, height, bpp and refresh, else `(-1, -1)`. Entered from the stub at the call target (`sub esp, 0xc; push ebx; push esi; jmp [slot]`) through SecuROM-moved code that loads the count (`mov esi, [ecx + count]; jmp body`) |
//!
//! The loop's index lives where the skip label's tail increments it: a stack slot in v1.0
//! (`mov eax, [esp+0x14]; add eax, 1`), `EBX` in v1.1 (`add ebx, 1`). The record keeps only its low byte,
//! and `SetMode` re-queries by that byte, so a mode at index 256 or later would be set as another mode.

use crate::exe_image::Image;
use crate::lua_sites::Pattern;

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    Filter,
    Keep,
    FirstSearch,
    Fallback,
    SetMode,
    FindModeBody,
}

/// Offsets in the [`Site::Filter`] pattern of the width and height `jb` (each `0f 82 rel32`), the
/// `cmp esi, 0x55` and its `ja` (`0f 87 rel32`).
const FILTER_WIDTH_JB_AT: usize = 10;
const FILTER_HEIGHT_JB_AT: usize = 26;
const FILTER_CMP_AT: usize = 36;
const FILTER_JA_AT: usize = 39;
/// Offsets in the [`Site::Keep`] pattern of the `mov ecx, <state>` ModRM byte, the search's `call rel32`
/// and the duplicate test's `jne` (`0f 85 rel32`).
const KEEP_STATE_MODRM_AT: usize = 10;
const KEEP_CALL_AT: usize = 11;
const KEEP_JNE_AT: usize = 24;
/// Offsets in the [`Site::FirstSearch`] pattern of the settings it reads (bpp index, refresh, height,
/// width: each a `moffs32`) and its `call rel32`.
const FIRST_BPP_AT: usize = 1;
const FIRST_REFRESH_AT: usize = 7;
const FIRST_HEIGHT_AT: usize = 14;
const FIRST_WIDTH_AT: usize = 26;
const FIRST_CALL_AT: usize = 36;
/// Offsets in the [`Site::Fallback`] pattern of the pending state (`mov ecx, [moffs]`), the four settings
/// stores (`mov dword [moffs], imm32`: width, height, bpp index, refresh) and its `call rel32`.
const FALLBACK_STATE_AT: usize = 2;
const FALLBACK_STORES_AT: [usize; 4] = [29, 39, 49, 59];
const FALLBACK_CALL_AT: usize = 67;
/// Offset in the [`Site::SetMode`] pattern of the pending state (`mov esi, [moffs]`). The next instruction,
/// `mov eax, [esi]` (`8b 06`), reads the adapter at offset 0 of the state.
const SET_MODE_STATE_AT: usize = 8;
/// Offset in the [`Site::FindModeBody`] pattern of the list's offset in the state (`mov ecx, [ecx + disp32]`).
const BODY_LIST_AT: usize = 16;
/// The mode search's entry stub, and how far past its start the body is.
const STUB: &str = "83 ec 0c 53 56 ff 25";
const STUB_BYTES: u32 = 11;
/// The SecuROM-moved code that loads the count and jumps to the body: `mov esi, [ecx + disp32]; jmp rel32`.
const FORWARDER: &str = "8b b1 ?? ?? ?? ?? e9 ?? ?? ?? ??";
/// The skip label's tail in each build: increments the index and loops.
const TAIL_STACK: &str = "8b 44 24 ?? 83 c0 01 3b 44 24 ?? 89 44 24 ?? 0f 82";
const TAIL_EBX: &str = "83 c3 01 3b 5c 24 ?? 89 5c 24 ?? 0f 82";

/// The retail refresh cap the filter's `cmp esi, imm8` holds.
pub const RETAIL_REFRESH_CAP: u8 = 0x55;
/// The bytes of `cmp esi, 0x55; ja skip` that a filter patch replaces.
pub const FILTER_PATCH_BYTES: usize = 9;
/// The settings' bpp index for 32 bits per pixel (`(index + 1) << 4`).
pub const BPP_INDEX_32: u32 = 1;

impl Site {
    pub const ALL: [Site; 6] = [Site::Filter, Site::Keep, Site::FirstSearch, Site::Fallback, Site::SetMode, Site::FindModeBody];

    pub fn name(self) -> &'static str {
        match self {
            Site::Filter => "Filter",
            Site::Keep => "Keep",
            Site::FirstSearch => "FirstSearch",
            Site::Fallback => "Fallback",
            Site::SetMode => "SetMode",
            Site::FindModeBody => "FindModeBody",
        }
    }

    /// The byte signature, as space-separated hex with `??` wildcards.
    fn pattern(self) -> &'static str {
        match self {
            // mov edx,[esp+0x28]; cmp edx,640; jb skip; mov ecx,[esp+0x2c]; cmp ecx,480; jb skip;
            // mov esi,[esp+0x30]; cmp esi,0x55; ja skip
            Site::Filter => {
                "8b 54 24 28 81 fa 80 02 00 00 0f 82 ?? ?? ?? ?? 8b 4c 24 2c 81 f9 e0 01 00 00 0f 82 ?? ?? ?? ?? \
                 8b 74 24 30 83 fe 55 0f 87 ?? ?? ?? ??"
            }
            // push esi; push 0x20; push ecx; push edx; lea eax,[esp+0x48]; mov ecx,<state>; call <search>;
            // mov ecx,[eax]; and ecx,[eax+4]; cmp ecx,-1; jne skip
            Site::Keep => "56 6a 20 51 52 8d 44 24 48 8b ?? e8 ?? ?? ?? ?? 8b 08 23 48 04 83 f9 ff 0f 85 ?? ?? ?? ??",
            // mov eax,[bpp]; mov edx,[refresh]; push edx; mov edx,[height]; add eax,1; shl eax,4; push eax;
            // mov eax,[width]; push edx; push eax; lea eax,[esp+0x28]; call <search>; mov ecx,[esp+0x18];
            // mov eax,[esp+0x1c]; mov edx,ecx; and edx,eax; cmp edx,-1; jne
            Site::FirstSearch => {
                "a1 ?? ?? ?? ?? 8b 15 ?? ?? ?? ?? 52 8b 15 ?? ?? ?? ?? 83 c0 01 c1 e0 04 50 a1 ?? ?? ?? ?? 52 50 \
                 8d 44 24 28 e8 ?? ?? ?? ?? 8b 4c 24 18 8b 44 24 1c 8b d1 23 d0 83 fa ff 75 ??"
            }
            // mov ecx,[pending]; add esp,4; push 60; push 32; push 480; push 640; lea eax,[esp+0x28];
            // mov [width],640; mov [height],480; mov [bpp],1; mov [refresh],60; call <search>;
            // mov ecx,[eax]; mov eax,[eax+4]
            Site::Fallback => {
                "8b 0d ?? ?? ?? ?? 83 c4 04 6a 3c 6a 20 68 e0 01 00 00 68 80 02 00 00 8d 44 24 28 \
                 c7 05 ?? ?? ?? ?? 80 02 00 00 c7 05 ?? ?? ?? ?? e0 01 00 00 c7 05 ?? ?? ?? ?? 01 00 00 00 \
                 c7 05 ?? ?? ?? ?? 3c 00 00 00 e8 ?? ?? ?? ?? 8b 08 8b 40 04"
            }
            // push ebx; mov ebx,[esp+8]; push esi; mov esi,[pending]; mov eax,[esi]; push edi;
            // mov edi,[esp+0x14]; push edi; push ebx; push eax; call
            Site::SetMode => "53 8b 5c 24 08 56 8b 35 ?? ?? ?? ?? 8b 06 57 8b 7c 24 14 57 53 50 e8",
            // push edi; or edi,-1; xor edx,edx; test esi,esi; mov [esp+0x14],edi; jbe; mov ecx,[ecx+list];
            // mov [esp+0xc],ecx; add ecx,2
            Site::FindModeBody => "57 83 cf ff 33 d2 85 f6 89 7c 24 14 76 ?? 8b 89 ?? ?? ?? ?? 89 4c 24 0c 83 c1 02",
        }
    }
}

/// Where the constructor's loop keeps the `EnumAdapterModes` index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeIndex {
    /// `[esp + disp8]` at the loop body's stack pointer (v1.0).
    Stack(u8),
    /// `EBX` (v1.1).
    Ebx,
}

/// The settings globals `ApplySettings` searches with and the fallback writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub width: u32,
    pub height: u32,
    /// `(bpp index + 1) << 4` is the bits per pixel; [`BPP_INDEX_32`] is 32.
    pub bpp_index: u32,
    pub refresh: u32,
}

/// What the BUG-016 fix needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayModeSites {
    /// `cmp esi, 0x55` in the constructor's loop, followed by `ja skip`: the [`FILTER_PATCH_BYTES`] bytes a
    /// patch replaces. `ESI` holds the mode's refresh rate there, `EDX` its width and `ECX` its height.
    pub refresh_cmp: u32,
    /// The loop's skip label: drops the mode and moves to the next index.
    pub skip: u32,
    /// The loop's keep path ([`Site::Keep`]).
    pub keep: u32,
    pub index: ModeIndex,
    /// The mode search's entry (the stub every search call targets).
    pub find_mode: u32,
    /// `ApplySettings`' search for the settings' mode: a `call rel32` to [`Self::find_mode`].
    pub first_search_call: u32,
    /// `ApplySettings`' search for 640×480 at 60 Hz: a `call rel32` to [`Self::find_mode`].
    pub fallback_call: u32,
    pub set_mode: u32,
    /// The global holding the render state `ApplySettings` is applying (the search's `ECX`).
    pub pending_state: u32,
    pub settings: Settings,
    /// Offsets in the render state of the mode list (a pointer to 8-byte records), its record count and
    /// the adapter ordinal.
    pub list_offset: u32,
    pub count_offset: u32,
    pub adapter_offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayModeSitesError {
    NoSection(&'static str),
    /// Each site's match count, when any is not exactly one.
    Matches(Vec<(Site, usize)>),
    /// The sites do not name the same search, state, settings or labels.
    Inconsistent { site: u32, why: String },
}

impl std::fmt::Display for DisplayModeSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayModeSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            DisplayModeSitesError::Matches(m) => write!(
                f,
                "display-mode sites did not resolve to exactly one match each: {}",
                m.iter().map(|(s, n)| format!("{}={n} match(es)", s.name())).collect::<Vec<_>>().join(", ")
            ),
            DisplayModeSitesError::Inconsistent { site, why } => write!(f, "the display-mode site at {site:#x} is inconsistent: {why}"),
        }
    }
}

impl std::error::Error for DisplayModeSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The target of the `rel32` jump or call whose 4-byte operand is at `site + at`.
fn rel32_target(site: u32, b: &[u8], at: usize) -> u32 {
    site.wrapping_add(at as u32 + 4).wrapping_add(u32_at(b, at))
}

/// Resolve every site in `img` and check them against each other.
pub fn resolve(img: &Image) -> Result<DisplayModeSites, DisplayModeSitesError> {
    let text = img.section(".text").ok_or(DisplayModeSitesError::NoSection(".text"))?;
    let found: Vec<(Site, Vec<u32>)> = Site::ALL
        .iter()
        .map(|&s| {
            let hits = Pattern::parse(s.pattern()).expect("built-in pattern parses").find_all(text.bytes);
            (s, hits.iter().map(|&o| text.va + o as u32).collect())
        })
        .collect();
    if !found.iter().all(|(_, v)| v.len() == 1) {
        return Err(DisplayModeSitesError::Matches(found.iter().map(|(s, v)| (*s, v.len())).collect()));
    }
    let va = |s: Site| found[Site::ALL.iter().position(|&x| x == s).expect("site in ALL")].1[0];
    let bytes = |v: u32, n: usize| text.bytes.get((v.wrapping_sub(text.va)) as usize..).and_then(|b| b.get(..n));
    let in_text = |v: u32| v >= text.va && v < text.va + text.bytes.len() as u32;
    let bad = |site: u32, why: String| DisplayModeSitesError::Inconsistent { site, why };

    // The constructor's loop: the filter's three branches and the keep path's duplicate test all leave by
    // the same skip label.
    let filter = va(Site::Filter);
    let f = bytes(filter, FILTER_JA_AT + 6).expect("a matched pattern lies in .text");
    let skip = rel32_target(filter, f, FILTER_JA_AT + 2);
    for (what, at) in [("width", FILTER_WIDTH_JB_AT), ("height", FILTER_HEIGHT_JB_AT)] {
        let t = rel32_target(filter, f, at + 2);
        if t != skip {
            return Err(bad(filter, format!("the {what} test leaves for {t:#x}, the refresh test for {skip:#x}")));
        }
    }
    let keep = va(Site::Keep);
    if keep <= filter + FILTER_JA_AT as u32 + 6 || keep >= skip {
        return Err(bad(keep, format!("the keep path is not between the refresh test at {filter:#x} and the skip label {skip:#x}")));
    }
    let k = bytes(keep, KEEP_JNE_AT + 6).expect("a matched pattern lies in .text");
    let keep_skip = rel32_target(keep, k, KEEP_JNE_AT + 2);
    if keep_skip != skip {
        return Err(bad(keep, format!("the duplicate test leaves for {keep_skip:#x}, the filter for {skip:#x}")));
    }
    let find_mode = rel32_target(keep, k, KEEP_CALL_AT + 1);

    let tail = bytes(skip, 17).ok_or_else(|| bad(skip, "the skip label runs past .text".into()))?;
    let index = if Pattern::parse(TAIL_STACK).expect("built-in pattern parses").matches_at(tail, 0) {
        if tail[3] != tail[14] {
            return Err(bad(skip, format!("the tail reads the index at [esp+{:#x}] but writes [esp+{:#x}]", tail[3], tail[14])));
        }
        ModeIndex::Stack(tail[3])
    } else if Pattern::parse(TAIL_EBX).expect("built-in pattern parses").matches_at(tail, 0) {
        ModeIndex::Ebx
    } else {
        return Err(bad(skip, format!("the skip label's tail is neither form: {:02x?}", tail)));
    };
    // The keep path names the state by a register; it cannot be the one holding the index.
    let state_modrm = k[KEEP_STATE_MODRM_AT];
    if !matches!(state_modrm, 0xc8..=0xcf) {
        return Err(bad(keep, format!("the keep path's mov ecx has ModRM {state_modrm:#04x}, not a register")));
    }
    if index == ModeIndex::Ebx && state_modrm == 0xcb {
        return Err(bad(keep, "the keep path's state is EBX, which the tail increments as the index".into()));
    }

    // ApplySettings' two searches and the settings they use.
    let first = va(Site::FirstSearch);
    let a = bytes(first, FIRST_CALL_AT + 5).expect("a matched pattern lies in .text");
    let settings = Settings {
        width: u32_at(a, FIRST_WIDTH_AT),
        height: u32_at(a, FIRST_HEIGHT_AT),
        bpp_index: u32_at(a, FIRST_BPP_AT),
        refresh: u32_at(a, FIRST_REFRESH_AT),
    };
    let first_search_call = first + FIRST_CALL_AT as u32;
    let fallback = va(Site::Fallback);
    let b = bytes(fallback, FALLBACK_CALL_AT + 5).expect("a matched pattern lies in .text");
    let fallback_call = fallback + FALLBACK_CALL_AT as u32;
    let stores = FALLBACK_STORES_AT.map(|at| u32_at(b, at));
    let want = [settings.width, settings.height, settings.bpp_index, settings.refresh];
    for ((what, have), want) in ["width", "height", "bpp index", "refresh"].iter().zip(stores).zip(want) {
        if have != want {
            return Err(bad(fallback, format!("the fallback writes the {what} at {have:#x}, the first search reads {want:#x}")));
        }
    }
    if fallback <= first_search_call {
        return Err(bad(fallback, format!("the fallback is not after the first search's call at {first_search_call:#x}")));
    }
    for (what, site, t) in [
        ("first search", first_search_call, rel32_target(first, a, FIRST_CALL_AT + 1)),
        ("fallback", fallback_call, rel32_target(fallback, b, FALLBACK_CALL_AT + 1)),
    ] {
        if t != find_mode {
            return Err(bad(site, format!("the {what} calls {t:#x}, the keep path's search is {find_mode:#x}")));
        }
    }
    let pending_state = u32_at(b, FALLBACK_STATE_AT);
    let set_mode = va(Site::SetMode);
    let m = bytes(set_mode, SET_MODE_STATE_AT + 4).expect("a matched pattern lies in .text");
    if u32_at(m, SET_MODE_STATE_AT) != pending_state {
        return Err(bad(set_mode, format!("SetMode reads the state at {:#x}, the fallback at {pending_state:#x}", u32_at(m, SET_MODE_STATE_AT))));
    }
    for (what, v) in [("width", settings.width), ("height", settings.height), ("bpp index", settings.bpp_index), ("refresh", settings.refresh), ("pending state", pending_state)] {
        if img.u32_at(v).is_none() {
            return Err(bad(fallback, format!("the {what} {v:#x} is not in the image")));
        }
    }

    // The mode search: its stub, its body, and the moved code that loads the count.
    let stub = bytes(find_mode, STUB_BYTES as usize).ok_or_else(|| bad(find_mode, "the mode search is not in .text".into()))?;
    if !Pattern::parse(STUB).expect("built-in pattern parses").matches_at(stub, 0) {
        return Err(bad(find_mode, format!("the mode search's stub is {stub:02x?}, not sub esp,0xc; push ebx; push esi; jmp [slot]")));
    }
    let body = va(Site::FindModeBody);
    if body != find_mode + STUB_BYTES {
        return Err(bad(body, format!("the search body is not {STUB_BYTES} bytes past the stub {find_mode:#x}")));
    }
    let list_offset = u32_at(bytes(body, BODY_LIST_AT + 4).expect("a matched pattern lies in .text"), BODY_LIST_AT);
    let forwarders: Vec<u32> = Pattern::parse(FORWARDER)
        .expect("built-in pattern parses")
        .find_all(text.bytes)
        .into_iter()
        .map(|o| text.va + o as u32)
        .filter(|&v| bytes(v, 11).is_some_and(|w| rel32_target(v, w, 7) == body))
        .collect();
    let [forwarder] = forwarders[..] else {
        return Err(bad(body, format!("{} moved blocks load a count and jump to the search body, not one", forwarders.len())));
    };
    let count_offset = u32_at(bytes(forwarder, 6).expect("checked length"), 2);
    if count_offset != list_offset + 4 {
        return Err(bad(forwarder, format!("the count at state+{count_offset:#x} is not the dword after the list at +{list_offset:#x}")));
    }
    for v in [skip, keep, find_mode, set_mode] {
        if !in_text(v) {
            return Err(bad(v, "not in .text".into()));
        }
    }
    Ok(DisplayModeSites {
        refresh_cmp: filter + FILTER_CMP_AT as u32,
        skip,
        keep,
        index,
        find_mode,
        first_search_call,
        fallback_call,
        set_mode,
        pending_state,
        settings,
        list_offset,
        count_offset,
        adapter_offset: 0,
    })
}

/// The code a filter patch jumps to, assembled for address `at`: drop the mode when its refresh rate
/// (`ESI`) is above `max_refresh` or its index is above `max_index`, else go to the keep path without the
/// aspect test.
///
/// ```text
/// cmp esi, max_refresh            81 fe imm32
/// ja skip                         0f 87 rel32
/// cmp dword [esp+d], max_index    81 7c 24 d imm32   (ModeIndex::Stack(d))
/// cmp ebx, max_index              81 fb imm32        (ModeIndex::Ebx)
/// ja skip                         0f 87 rel32
/// jmp keep                        e9 rel32
/// ```
pub fn filter_code(s: &DisplayModeSites, at: u32, max_refresh: u32, max_index: u32) -> Vec<u8> {
    let mut c = Vec::with_capacity(32);
    let rel = |c: &Vec<u8>, to: u32| to.wrapping_sub(at + c.len() as u32 + 4).to_le_bytes();
    c.extend([0x81, 0xfe]);
    c.extend(max_refresh.to_le_bytes());
    c.extend([0x0f, 0x87]);
    let r = rel(&c, s.skip);
    c.extend(r);
    match s.index {
        ModeIndex::Stack(d) => c.extend([0x81, 0x7c, 0x24, d]),
        ModeIndex::Ebx => c.extend([0x81, 0xfb]),
    }
    c.extend(max_index.to_le_bytes());
    c.extend([0x0f, 0x87]);
    let r = rel(&c, s.skip);
    c.extend(r);
    c.push(0xe9);
    let r = rel(&c, s.keep);
    c.extend(r);
    c
}

/// The [`FILTER_PATCH_BYTES`] bytes that replace `cmp esi, 0x55; ja skip` at [`DisplayModeSites::refresh_cmp`]:
/// `jmp code`, then `nop`s.
pub fn filter_patch(s: &DisplayModeSites, code: u32) -> [u8; FILTER_PATCH_BYTES] {
    let mut p = [0x90; FILTER_PATCH_BYTES];
    p[0] = 0xe9;
    p[1..5].copy_from_slice(&code.wrapping_sub(s.refresh_cmp + 5).to_le_bytes());
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    /// The v1.1 bytes of every site, with its operands, at its v1.1 address, and the moved forwarder: read
    /// from the v1.1 dump (`dump-v11-securom-unpacked`).
    const V11: [(u32, &str); 9] = [
        (0x754de3, "8b 54 24 28 81 fa 80 02 00 00 0f 82 31 01 00 00 8b 4c 24 2c 81 f9 e0 01 00 00 0f 82 21 01 00 00 8b 74 24 30 83 fe 55 0f 87 14 01 00 00"),
        (0x754e7b, "56 6a 20 51 52 8d 44 24 48 8b cd e8 65 08 00 00 8b 08 23 48 04 83 f9 ff 0f 85 8b 00 00 00"),
        (0x754f24, "83 c3 01 3b 5c 24 24 89 5c 24 14 0f 82 47 fe ff ff"),
        (0x74cca7, "a1 30 b3 df 00 8b 15 34 b3 df 00 52 8b 15 2c b3 df 00 83 c0 01 c1 e0 04 50 a1 28 b3 df 00 52 50 8d 44 24 28 e8 20 8a 00 00 8b 4c 24 18 8b 44 24 1c 8b d1 23 d0 83 fa ff 75 79"),
        (0x74cceb, "8b 0d 8c 52 17 01 83 c4 04 6a 3c 6a 20 68 e0 01 00 00 68 80 02 00 00 8d 44 24 28 c7 05 28 b3 df 00 80 02 00 00 c7 05 2c b3 df 00 e0 01 00 00 c7 05 30 b3 df 00 01 00 00 00 c7 05 34 b3 df 00 3c 00 00 00 e8 bd 89 00 00 8b 08 8b 40 04"),
        (0x755570, "53 8b 5c 24 08 56 8b 35 8c 52 17 01 8b 06 57 8b 7c 24 14 57 53 50 e8"),
        (0x7556f0, "83 ec 0c 53 56 ff 25 b0 d1 4c 02"),
        (0x7556fb, "57 83 cf ff 33 d2 85 f6 89 7c 24 14 76 3e 8b 89 d4 05 00 00 89 4c 24 0c 83 c1 02"),
        (0x40a271, "8b b1 d8 05 00 00 e9 7f b4 34 00"),
    ];

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|t| u8::from_str_radix(t, 16).unwrap()).collect()
    }

    /// A `.text` from 0x401000 holding `sites`, and a `.data` holding the globals they name.
    fn image(sites: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut text = vec![0xcc; 0x356000];
        for (va, b) in sites {
            let at = (va - 0x401000) as usize;
            text[at..at + b.len()].copy_from_slice(b);
        }
        // .data spans the settings (0xdfb328..) and the pending state (0x117528c).
        synth::pe(0x400000, &[(".text", 0x1000, text), (".data", 0x9fb000, vec![0; 0x37b000])])
    }

    fn v11() -> Vec<(u32, Vec<u8>)> {
        V11.iter().map(|&(va, s)| (va, hex(s))).collect()
    }

    fn want_v11() -> DisplayModeSites {
        DisplayModeSites {
            refresh_cmp: 0x754e07,
            skip: 0x754f24,
            keep: 0x754e7b,
            index: ModeIndex::Ebx,
            find_mode: 0x7556f0,
            first_search_call: 0x74cccb,
            fallback_call: 0x74cd2e,
            set_mode: 0x755570,
            pending_state: 0x117528c,
            settings: Settings { width: 0xdfb328, height: 0xdfb32c, bpp_index: 0xdfb330, refresh: 0xdfb334 },
            list_offset: 0x5d4,
            count_offset: 0x5d8,
            adapter_offset: 0,
        }
    }

    #[test]
    fn every_built_in_pattern_parses_and_its_offsets_name_their_operands() {
        for s in Site::ALL {
            Pattern::parse(s.pattern()).unwrap_or_else(|e| panic!("{}: {e:?}", s.name()));
        }
        let tok = |s: Site| s.pattern().split_whitespace().map(str::to_owned).collect::<Vec<_>>();
        let f = tok(Site::Filter);
        assert_eq!(f.len(), FILTER_JA_AT + 6);
        assert_eq!(f[FILTER_CMP_AT..FILTER_CMP_AT + 3], ["83", "fe", "55"]);
        assert_eq!(f[FILTER_JA_AT..FILTER_JA_AT + 2], ["0f", "87"]);
        assert_eq!(f[FILTER_WIDTH_JB_AT..FILTER_WIDTH_JB_AT + 2], ["0f", "82"]);
        assert_eq!(f[FILTER_HEIGHT_JB_AT..FILTER_HEIGHT_JB_AT + 2], ["0f", "82"]);
        let k = tok(Site::Keep);
        assert_eq!(k[KEEP_STATE_MODRM_AT - 1], "8b");
        assert_eq!((k[KEEP_CALL_AT].as_str(), k[KEEP_JNE_AT].as_str(), k[KEEP_JNE_AT + 1].as_str()), ("e8", "0f", "85"));
        let a = tok(Site::FirstSearch);
        for (at, op) in [(FIRST_BPP_AT, &["a1"][..]), (FIRST_REFRESH_AT, &["8b", "15"]), (FIRST_HEIGHT_AT, &["8b", "15"]), (FIRST_WIDTH_AT, &["a1"])] {
            assert_eq!(a[at - op.len()..at], *op, "the opcode before the operand at {at}");
            assert_eq!(a[at..at + 4], ["??"; 4]);
        }
        assert_eq!(a[FIRST_CALL_AT], "e8");
        let b = tok(Site::Fallback);
        assert_eq!(b[FALLBACK_STATE_AT - 2..FALLBACK_STATE_AT], ["8b", "0d"]);
        for at in FALLBACK_STORES_AT {
            assert_eq!(b[at - 2..at], ["c7", "05"]);
            assert_eq!(b[at..at + 4], ["??"; 4]);
        }
        assert_eq!(b[FALLBACK_CALL_AT], "e8");
        let m = tok(Site::SetMode);
        assert_eq!(m[SET_MODE_STATE_AT - 2..SET_MODE_STATE_AT], ["8b", "35"]);
        assert_eq!(m[SET_MODE_STATE_AT + 4..SET_MODE_STATE_AT + 6], ["8b", "06"]);
        assert_eq!(tok(Site::FindModeBody)[BODY_LIST_AT - 2..BODY_LIST_AT], ["8b", "89"]);
    }

    #[test]
    fn the_v11_sites_resolve() {
        assert_eq!(resolve(&Image::from_file(&image(&v11())).unwrap()).unwrap(), want_v11());
    }

    #[test]
    fn a_stack_slot_index_is_read_from_the_tail() {
        let mut s = v11();
        s[2].1 = hex("8b 44 24 14 83 c0 01 3b 44 24 24 89 44 24 14 0f 82");
        let r = resolve(&Image::from_file(&image(&s)).unwrap()).unwrap();
        assert_eq!(r.index, ModeIndex::Stack(0x14));
        s[2].1 = hex("8b 44 24 14 83 c0 01 3b 44 24 24 89 44 24 18 0f 82");
        let e = resolve(&Image::from_file(&image(&s)).unwrap()).unwrap_err().to_string();
        assert!(e.contains("reads the index at [esp+0x14] but writes [esp+0x18]"), "{e}");
    }

    #[test]
    fn the_sites_must_name_the_same_labels_search_state_and_settings() {
        let check = |i: usize, at: usize, b: &[u8]| {
            let mut s = v11();
            s[i].1[at..at + b.len()].copy_from_slice(b);
            resolve(&Image::from_file(&image(&s)).unwrap()).unwrap_err().to_string()
        };
        let e = check(0, FILTER_WIDTH_JB_AT + 2, &[0x32, 0x01, 0, 0]);
        assert!(e.contains("the width test leaves for 0x754f25"), "{e}");
        let e = check(1, KEEP_JNE_AT + 2, &[0x8c, 0, 0, 0]);
        assert!(e.contains("the duplicate test leaves for 0x754f25"), "{e}");
        let e = check(1, KEEP_STATE_MODRM_AT, &[0xcb]);
        assert!(e.contains("the keep path's state is EBX"), "{e}");
        let e = check(4, FALLBACK_STORES_AT[2], &[0x38]);
        assert!(e.contains("writes the bpp index at 0xdfb338, the first search reads 0xdfb330"), "{e}");
        let e = check(3, FIRST_CALL_AT + 1, &[0x21]);
        assert!(e.contains("the first search calls 0x7556f1"), "{e}");
        let e = check(5, SET_MODE_STATE_AT, &[0x88]);
        assert!(e.contains("SetMode reads the state at 0x1175288, the fallback at 0x117528c"), "{e}");
        let e = check(8, 2, &[0xdc]);
        assert!(e.contains("is not the dword after the list"), "{e}");
        let e = check(8, 7, &[0x80]);
        assert!(e.contains("0 moved blocks load a count"), "{e}");
        let e = check(2, 0, &[0x90]);
        assert!(e.contains("neither form"), "{e}");
    }

    #[test]
    fn a_missing_or_doubled_site_reports_its_count() {
        let mut s = v11();
        s.push((0x500000, hex(V11[1].1)));
        s.remove(3);
        let e = resolve(&Image::from_file(&image(&s)).unwrap()).unwrap_err();
        assert_eq!(
            e,
            DisplayModeSitesError::Matches(vec![
                (Site::Filter, 1),
                (Site::Keep, 2),
                (Site::FirstSearch, 0),
                (Site::Fallback, 1),
                (Site::SetMode, 1),
                (Site::FindModeBody, 1),
            ])
        );
    }

    #[test]
    fn the_filter_code_drops_by_refresh_and_index_and_keeps_without_the_aspect_test() {
        let s = want_v11();
        let at = 0x1000_0000;
        let c = filter_code(&s, at, 255, 255);
        let rel = |end: u32, to: u32| to.wrapping_sub(at + end).to_le_bytes();
        let mut want = vec![0x81, 0xfe, 0xff, 0, 0, 0, 0x0f, 0x87];
        want.extend(rel(12, s.skip));
        want.extend([0x81, 0xfb, 0xff, 0, 0, 0, 0x0f, 0x87]);
        want.extend(rel(24, s.skip));
        want.push(0xe9);
        want.extend(rel(29, s.keep));
        assert_eq!(c, want);
        let c = filter_code(&DisplayModeSites { index: ModeIndex::Stack(0x14), ..s }, at, 255, 255);
        assert_eq!(c[12..20], [0x81, 0x7c, 0x24, 0x14, 0xff, 0, 0, 0]);
        assert_eq!(c.len(), 31);
        assert_eq!(c[22..26], rel(26, s.skip));
        assert_eq!(c[27..31], rel(31, s.keep));
    }

    #[test]
    fn the_filter_patch_jumps_to_the_code_and_pads_with_nops() {
        let s = want_v11();
        let p = filter_patch(&s, 0x1000_0000);
        assert_eq!(p[0], 0xe9);
        assert_eq!(u32::from_le_bytes(p[1..5].try_into().unwrap()), 0x1000_0000u32.wrapping_sub(0x754e07 + 5));
        assert_eq!(p[5..], [0x90; 4]);
    }
}
