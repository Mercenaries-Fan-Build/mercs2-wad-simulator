//! The code sites m2-sdk.dll's Lua host hooks or calls, found in the game image without per-build
//! addresses. Every site resolves exactly once or resolution fails, naming each site's match count.
//!
//! Two methods:
//!
//! * **Byte signatures** over `.text` for the script host and the Lua core: `call rel32` targets and
//!   absolute addresses are `??`, so a build that relinks the same code elsewhere still matches. Roles,
//!   from the disassembly of `mercs2_unpacked.exe`:
//!
//!   | site | VA (unpacked) | what it is |
//!   |---|---|---|
//!   | [`Site::RegisterNamespaces`] | `0x005A2C40` | `__stdcall (host)`, `ret 4`: registers every engine namespace on `host+0x10` (the `lua_State`) |
//!   | [`Site::ScriptHostInit`] | `0x005A1760` | `__thiscall (host)`: creates the `lua_State` at `+0x10` and calls `RegisterNamespaces` (the `call` at pattern offset 71) |
//!   | [`Site::ScriptHostCtor`] | `0x005A1677` | the static constructor's stores: vtable → host, host → global; `mov eax, host; ret` |
//!   | [`Site::LuaLRegister`] | `0x0085F500` | `luaL_register`, `ECX = L`, `EAX = libname`, table pushed, caller pops |
//!   | [`Site::LuaSNewLStr`] | `0x0086A2B0` | `luaS_newlstr(L, str, len)`, cdecl, returns the interned `TString*` |
//!   | [`Site::LuaGErrorMsg`] | `0x0085EEA0` | `luaG_errormsg(L)` with `L` in `EAX`; `longjmp`s to the enclosing `lua_pcall` |
//!   | [`Site::FrameStep`] | `0x004C16E0` | `void (void)`: RunFrame's timestep compute, called once a frame from RunFrame alone (`0x4C1690` in v1.1). RunFrame itself is the profiler's to detour, and MinHook takes a target once |
//!
//! * **By name** from the `luaL_Reg` tables in `.rdata` (`{ const char* name; lua_CFunction func; }`): the
//!   row whose name pointer points at the exact string and whose function lies in `.text`. The base
//!   library gives [`Site::Print`] (the shared no-op log stub `33 C0 C3` that `print`, `Debug.Printf` and
//!   the stripped subsystem loggers all call), [`Site::LuaBType`], [`Site::LuaBLoadString`] and
//!   [`Site::LuaBPcall`]; the Gui table gives [`Site::CreateTextWidget`]. All are cdecl `lua_CFunction`s
//!   (`luaB_type` reads `L` from `[esp+8]` after `push esi`).

use crate::exe_image::{Image, Section};

/// The resolved sites, in table order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Site {
    RegisterNamespaces,
    ScriptHostInit,
    ScriptHostCtor,
    LuaLRegister,
    LuaSNewLStr,
    LuaGErrorMsg,
    Print,
    LuaBType,
    LuaBLoadString,
    LuaBPcall,
    CreateTextWidget,
    FrameStep,
}

impl Site {
    pub const ALL: [Site; 12] = [
        Site::RegisterNamespaces,
        Site::ScriptHostInit,
        Site::ScriptHostCtor,
        Site::LuaLRegister,
        Site::LuaSNewLStr,
        Site::LuaGErrorMsg,
        Site::Print,
        Site::LuaBType,
        Site::LuaBLoadString,
        Site::LuaBPcall,
        Site::CreateTextWidget,
        Site::FrameStep,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Site::RegisterNamespaces => "ScriptHostRegisterNamespaces",
            Site::ScriptHostInit => "ScriptHostInit",
            Site::ScriptHostCtor => "ScriptHostCtor",
            Site::LuaLRegister => "luaL_register",
            Site::LuaSNewLStr => "luaS_newlstr",
            Site::LuaGErrorMsg => "luaG_errormsg",
            Site::Print => "print (log stub)",
            Site::LuaBType => "luaB_type",
            Site::LuaBLoadString => "luaB_loadstring",
            Site::LuaBPcall => "luaB_pcall",
            Site::CreateTextWidget => "CreateTextWidget",
            Site::FrameStep => "frame timestep",
        }
    }

    /// The `luaL_Reg` name a by-name site is registered under.
    fn reg_name(self) -> Option<&'static str> {
        match self {
            Site::Print => Some("print"),
            Site::LuaBType => Some("type"),
            Site::LuaBLoadString => Some("loadstring"),
            Site::LuaBPcall => Some("pcall"),
            Site::CreateTextWidget => Some("CreateTextWidget"),
            _ => None,
        }
    }

    /// The byte signature of a signature site, as space-separated hex with `??` wildcards.
    fn pattern(self) -> Option<&'static str> {
        Some(match self {
            // push ebx; push ebp; mov ebp,[esp+0xc] (host); push esi; mov esi,[ebp+0x10] (L); push edi;
            // three times: push <cfunc>; xor eax,eax; call lua_pushcclosure; luaD_call(L, top-8, 0) ...
            // then four nil pushes via [L+8], and `mov edi, <name>; mov ecx, LUA_GLOBALSINDEX; call`
            Site::RegisterNamespaces => {
                "53 55 8b 6c 24 0c 56 8b 75 10 57 68 ?? ?? ?? ?? 33 c0 e8 ?? ?? ?? ?? 8b 46 08 33 db 53 83 \
                 e8 08 50 56 e8 ?? ?? ?? ?? 8b 75 10 68 ?? ?? ?? ?? 33 c0 e8 ?? ?? ?? ?? 8b 4e 08 53 83 e9 \
                 08 51 56 e8 ?? ?? ?? ?? 8b 75 10 68 ?? ?? ?? ?? 33 c0 e8 ?? ?? ?? ?? 8b 56 08 53 83 ea 08 \
                 52 56 e8 ?? ?? ?? ?? 8b 75 10 8b 46 08 89 58 04 83 46 08 08 83 c4 30 bf ?? ?? ?? ?? b9 ee \
                 d8 ff ff e8"
            }
            // push esi; mov esi,ecx; cmp byte [esi+0xc42],0; jne <out>; push edi; mov byte [esi+0xc42],1;
            // call lua_open; push 0xc00; mov [esi+0x10],eax; mov [esi+0x14],eax; ...; call RegisterNamespaces
            Site::ScriptHostInit => {
                "56 8b f1 80 be 42 0c 00 00 00 0f 85 ?? ?? ?? ?? 57 c6 86 42 0c 00 00 01 e8 ?? ?? ?? ?? 68 \
                 00 0c 00 00 89 46 10 89 46 14 8d 46 28 6a 00 50 e8 ?? ?? ?? ?? 83 c4 0c 6a 10 8b fe e8 ?? \
                 ?? ?? ?? 56 c7 46 20 00 00 00 00 e8 ?? ?? ?? ??"
            }
            // mov [host], vtable; mov [global], host; movss [..], xmm0; movss [..], xmm0; mov eax, host; ret
            Site::ScriptHostCtor => {
                "c7 05 ?? ?? ?? ?? ?? ?? ?? ?? c7 05 ?? ?? ?? ?? ?? ?? ?? ?? f3 0f 11 05 ?? ?? ?? ?? f3 0f 11 \
                 05 ?? ?? ?? ?? b8 ?? ?? ?? ?? c3"
            }
            // push ebp; mov ebp,esp; and esp,-8; sub esp,0xc; push ebx; mov ebx,[ebp+8] (l); push esi;
            // push edi; mov edi,eax (libname); test edi,edi; mov esi,ecx (L); jz <no libname>; count rows
            Site::LuaLRegister => {
                "55 8b ec 83 e4 f8 83 ec 0c 53 8b 5d 08 56 57 8b f8 85 ff 8b f1 0f 84 b9 00 00 00 33 c9 39 \
                 0b 8b c3 89 4c 24 0c 74 0f 83 c0 08 83 c1 01 83 38 00 75 f5 89 4c 24 0c 51"
            }
            // h = len; step = (len>>5)+1; hash loop; G(L)->strt.hash[h & (size-1)]; compare ts->len (+0xc)
            Site::LuaSNewLStr => {
                "55 8b ec 83 e4 f8 83 ec 0c 53 56 57 8b 7d 10 8b c7 c1 e8 05 83 c0 01 3b f8 8b d7 89 7c 24 \
                 14 8b cf 72 22 8b 75 0c 0f b6 74 0e ff 8b da c1 e3 05 03 f3 8b da c1 eb 02 03 f3 2b c8 33 \
                 d6 3b c8 73 e2 89 54 24 14 8b 45 08 8b 40 10 8b 48 08 8b 00 83 e9 01 23 ca 8b 04 88 85 c0 \
                 89 44 24 10 0f 84 ?? ?? ?? ?? 39 78 0c"
            }
            // push esi; mov esi,eax (L); mov eax,[esi+0x60] (errfunc); test; push edi; jz <throw>;
            // errfunc must be a function (tt 6) else luaD_throw(L, LUA_ERRERR=5); move the message up
            Site::LuaGErrorMsg => {
                "56 8b f0 8b 46 60 85 c0 57 74 6f 8b 7e 20 03 f8 83 7f 04 06 74 0b 6a 05 56 e8 ?? ?? ?? ?? \
                 83 c4 08 8b 46 08 8b 48 f8 89 08 8b 50 fc 89 50 04 8b 46 08 8b 0f 83 e8 08 89 08 8b 57 04 \
                 89 50 04"
            }
            // push ecx; mov ecx,[…]; test ecx,ecx; movss xmm0,[…]; movss [esp],xmm0; jz …; mov eax,[…];
            // test eax,eax; jz +0x23; mov eax,[eax+4]; cmp eax,0x57B5E35A
            Site::FrameStep => {
                "51 8b 0d ?? ?? ?? ?? 85 c9 f3 0f 10 05 ?? ?? ?? ?? f3 0f 11 04 24 0f 84 ?? ?? ?? ?? a1 ?? ?? ?? ?? \
                 85 c0 74 23 8b 40 04 3d 5a e3 b5 57"
            }
            _ => return None,
        })
    }
}

/// Offset of the `lua_State*` in the script host (`mov [esi+0x10], eax` in [`Site::ScriptHostInit`],
/// `mov esi, [ebp+0x10]` in [`Site::RegisterNamespaces`]; both fixed bytes of the patterns).
pub const HOST_LUA_STATE: u32 = 0x10;
/// Offset of the host's "a `lua_State` exists" byte (`cmp byte [esi+0xC42], 0` in [`Site::ScriptHostInit`]).
pub const HOST_INITIALISED: u32 = 0xC42;
/// The vtable slot holding [`Site::ScriptHostInit`] (the system loop `FUN_004C0730` calls `+0x14` to start a
/// system).
pub const HOST_VTABLE_INIT_SLOT: u32 = 0x14;
/// Offset of the `call RegisterNamespaces` instruction inside the [`Site::ScriptHostInit`] pattern.
const INIT_CALL_AT: usize = 71;
/// The log stub's body: `xor eax, eax; ret`.
pub const LOG_STUB_BODY: [u8; 3] = [0x33, 0xc0, 0xc3];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternError {
    BadToken(String),
    Empty,
    LeadingWildcard,
}

/// A parsed byte pattern; `None` is a wildcard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern(Vec<Option<u8>>);

impl Pattern {
    pub fn parse(text: &str) -> Result<Self, PatternError> {
        let mut out = Vec::new();
        for tok in text.split_ascii_whitespace() {
            if tok == "??" {
                out.push(None);
            } else if tok.len() == 2 {
                out.push(Some(u8::from_str_radix(tok, 16).map_err(|_| PatternError::BadToken(tok.to_owned()))?));
            } else {
                return Err(PatternError::BadToken(tok.to_owned()));
            }
        }
        match out.first() {
            None => Err(PatternError::Empty),
            Some(None) => Err(PatternError::LeadingWildcard),
            Some(Some(_)) => Ok(Pattern(out)),
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn matches_at(&self, hay: &[u8], at: usize) -> bool {
        hay.get(at..at + self.0.len()).is_some_and(|w| w.iter().zip(&self.0).all(|(b, p)| p.is_none_or(|p| p == *b)))
    }

    /// Every offset in `hay` where the pattern matches.
    pub fn find_all(&self, hay: &[u8]) -> Vec<usize> {
        let first = self.0[0].expect("parse() rejects a leading wildcard");
        let mut hits = Vec::new();
        if hay.len() < self.0.len() {
            return hits;
        }
        let last_start = hay.len() - self.0.len();
        let mut i = 0;
        while i <= last_start {
            match hay[i..=last_start].iter().position(|&b| b == first) {
                None => break,
                Some(p) => {
                    let at = i + p;
                    if self.matches_at(hay, at) {
                        hits.push(at);
                    }
                    i = at + 1;
                }
            }
        }
        hits
    }
}

/// Where every site was found, and the script host's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LuaSites {
    va: [u32; 12],
    host: u32,
}

impl LuaSites {
    pub fn va(&self, s: Site) -> u32 {
        self.va[s as usize]
    }

    /// The script host singleton (`this` of [`Site::ScriptHostInit`]).
    pub fn host(&self) -> u32 {
        self.host
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LuaSitesError {
    NoSection(&'static str),
    /// Each site's match count, when any is not exactly one.
    Matches(Vec<(Site, usize)>),
    InitCallsElsewhere { init: u32, called: u32, walker: u32 },
    Ctor { ctor: u32, why: String },
    BaseLibrary(String),
    LogStub { va: u32, bytes: Vec<u8> },
}

impl std::fmt::Display for LuaSitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LuaSitesError::NoSection(s) => write!(f, "the image has no {s} section"),
            LuaSitesError::Matches(m) => write!(
                f,
                "Lua sites did not resolve to exactly one match each: {}",
                m.iter().map(|(s, n)| format!("{}={n} match(es)", s.name())).collect::<Vec<_>>().join(", ")
            ),
            LuaSitesError::InitCallsElsewhere { init, called, walker } => write!(
                f,
                "ScriptHostInit at {init:#x} calls {called:#x}, not ScriptHostRegisterNamespaces at {walker:#x}"
            ),
            LuaSitesError::Ctor { ctor, why } => write!(f, "ScriptHostCtor at {ctor:#x} is inconsistent: {why}"),
            LuaSitesError::BaseLibrary(why) => write!(f, "the base library's luaL_Reg table: {why}"),
            LuaSitesError::LogStub { va, bytes } => write!(
                f,
                "print at {va:#x} starts {:02x?}, neither the log stub `33 c0 c3` nor a detour's `e9` jump",
                bytes
            ),
        }
    }
}

impl std::error::Error for LuaSitesError {}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// The `luaL_Reg` rows in `rdata` named `name` whose function lies in `text`, as `(row VA, function VA)`.
fn reg_rows(rdata: &Section, text: &Section, name: &[u8]) -> Vec<(u32, u32)> {
    let mut needle = Vec::with_capacity(name.len() + 2);
    needle.push(0);
    needle.extend_from_slice(name);
    needle.push(0);
    let strings: Vec<u32> = rdata
        .bytes
        .windows(needle.len())
        .enumerate()
        .filter(|(_, w)| *w == &needle[..])
        .map(|(i, _)| rdata.va + i as u32 + 1)
        .collect();
    let mut rows = Vec::new();
    for at in (0..rdata.bytes.len().saturating_sub(7)).step_by(4) {
        let p = u32_at(rdata.bytes, at);
        if strings.contains(&p) {
            let f = u32_at(rdata.bytes, at + 4);
            if text.contains(f) {
                rows.push((rdata.va + at as u32, f));
            }
        }
    }
    rows
}

/// The sites only the Lua debugger (`lua_debug`) detours, resolved apart from [`LuaSites`] so a build
/// where one is missing loses the debugger, not every mod's Lua host. Roles, from the pc-v10 disassembly:
///
/// | site | VA (unpacked) | what it is |
/// |---|---|---|
/// | `undump` | `0x008711D0` | `luaU_undump(L, z, buff, name)`, cdecl, returns the chunk's main `Proto*` |
/// | `poscall` | `0x00868850` | `luaD_poscall`, L in `ECX`, first result in `EAX`, plain `ret` |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DebugSites {
    pub undump: u32,
    pub poscall: u32,
}

/// push ebp; mov ebp,esp; and esp,-8; mov ecx,[ebp+0x14] (name); mov al,[ecx]; sub esp,0x10; cmp al,'@';
/// push esi; push edi; je; cmp al,'='; je; cmp al,0x1b; jne; mov [esp+0x14],"binary string"; jmp;
/// add ecx,1; mov [esp+0x14],ecx; mov eax,[ebp+0xc]; mov ecx,[ebp+0x10]; mov esi,[ebp+8]
const UNDUMP: &str = "55 8b ec 83 e4 f8 8b 4d 14 8a 01 83 ec 10 3c 40 56 57 74 12 3c 3d 74 0e 3c 1b 75 0d c7 44 24 14 \
                      ?? ?? ?? ?? eb 07 83 c1 01 89 4c 24 14 8b 45 0c 8b 4d 10 8b 75 08";
/// push ecx; push esi; mov esi,ecx (L); test byte [esi+0x36],2 (hookmask & ret); je; call callrethooks;
/// mov edx,[esi+0x14] (ci); push ebx; push edi; lea edi,[edx-0x18]; mov [esi+0x14],edi (L->ci--); ...
const POSCALL: &str = "51 56 8b f1 f6 46 36 02 74 05 e8 ?? ?? ?? ?? 8b 56 14 53 57 8d 7a e8 89 7e 14 8b 5a 10 85 db \
                       8b 4a 04 8b 3f 89 7e 0c 8b 52 f4 89 56 18";

/// Resolve the debugger's sites: each pattern must match exactly once in `.text`.
pub fn resolve_debug(img: &Image) -> Result<DebugSites, String> {
    let text = img.section(".text").ok_or("the image has no .text section")?;
    let one = |name: &str, pattern: &str| -> Result<u32, String> {
        let hits = Pattern::parse(pattern).expect("built-in pattern parses").find_all(text.bytes);
        match hits[..] {
            [at] => Ok(text.va + at as u32),
            _ => Err(format!("{name} matched {} time(s), not once", hits.len())),
        }
    };
    Ok(DebugSites { undump: one("luaU_undump", UNDUMP)?, poscall: one("luaD_poscall", POSCALL)? })
}

/// The Lua C API functions behind m2-sdk.dll's function references (`m2_lua_ref`, `m2_lua_unref`,
/// `m2_lua_push_ref`, `m2_lua_pcall`), resolved apart from [`LuaSites`] like [`DebugSites`]. Every one is a
/// whole-function signature over the pc-v10 bytes, found 0x220 later in the v1.1 dump. Their register
/// arguments are MSVC's link-time-generated conventions, read from the disassembly:
///
/// | site | VA (unpacked) | what it is |
/// |---|---|---|
/// | `luaL_ref` | `0x0085FD50` | `luaL_ref(L, t)`: `L` in `EAX`, `t` pushed, caller pops; returns the ref in `EAX` (`-1`, `LUA_REFNIL`, for a nil, which it pops) |
/// | `luaL_unref` | `0x0085FE30` | `luaL_unref(L, LUA_REGISTRYINDEX, ref)` with the registry folded in (`mov ecx, 0xFFFFD8F0` before each call): `L` in `EDI`, `ref` pushed, caller pops; a `ref < 0` returns at once |
/// | `lua_rawgeti` | `0x0085DBB0` | `lua_rawgeti(L, idx, n)`: `L` in `EDI`, `idx` in `ECX`, `n` pushed, caller pops |
/// | `lua_pcall` | `0x0085DF50` | `lua_pcall(L, nargs, nresults, errfunc)`: `L` in `EAX`, `errfunc` in `ECX`, `nresults` in `EDI`, `nargs` pushed, caller pops; returns the status in `EAX` |
///
/// They belong together by their calls: all four call one `index2adr`, `luaL_ref`, `luaL_unref` and
/// `lua_rawgeti` one `luaH_getnum`, and `luaL_ref` and `luaL_unref` one `lua_rawseti`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefSites {
    /// `luaL_ref`.
    pub lual_ref: u32,
    /// `luaL_unref`.
    pub lual_unref: u32,
    pub lua_rawgeti: u32,
    pub lua_pcall: u32,
}

/// `luaL_ref`, entry to the final `ret`: abs_index; a nil at the top is popped and gives -1; `ref =
/// t[0]` (the free list), then `t[0] = t[ref]`, or `ref = #t + 1`; `t[ref] = value`. `??` are the
/// `call` targets and `luaO_nilobject`.
const LUAL_REF: &str = "53 55 8b 6c 24 0c 81 fd f0 d8 ff ff 56 57 8b f8 7e 0d 8b 47 08 2b 47 0c c1 f8 03 8d 6c 28 01 \
                        8b 47 08 83 c0 f8 3d ?? ?? ?? ?? 74 11 83 78 04 00 75 0b 89 47 08 83 c8 ff 5f 5e 5d 5b c3 \
                        8b cd 8b d7 e8 ?? ?? ?? ?? 8b c8 8b 31 33 c0 e8 ?? ?? ?? ?? 8b 10 8b 4f 08 89 11 8b 40 04 \
                        89 41 04 83 47 08 08 83 c9 ff 8b d7 e8 ?? ?? ?? ?? 83 47 08 f8 8b d8 85 db 8b cd 74 43 8b d7 \
                        e8 ?? ?? ?? ?? 8b c8 8b 31 8b c3 e8 ?? ?? ?? ?? 8b 4f 08 8b 10 89 11 8b 40 04 89 41 04 83 47 \
                        08 08 6a 00 8b cd 8b f7 e8 ?? ?? ?? ?? 83 c4 04 53 8b cd e8 ?? ?? ?? ?? 83 c4 04 8b c3 5f 5e \
                        5d 5b c3 e8 ?? ?? ?? ?? 8b d8 83 c3 01 53 8b cd 8b f7 e8 ?? ?? ?? ?? 83 c4 04 5f 5e 5d 8b c3 \
                        5b c3";
/// `luaL_ref`'s calls: `index2adr` (twice), `luaH_getnum` (twice), `lua_rawseti` (three times).
const LUAL_REF_INDEX2ADR: [usize; 2] = [0x41, 0x7a];
const LUAL_REF_GETNUM: [usize; 2] = [0x4c, 0x85];
const LUAL_REF_RAWSETI: [usize; 3] = [0xa1, 0xac, 0xca];
/// `luaL_unref`, entry to `ret`: `if (ref >= 0) { t[ref] = t[0]; t[0] = ref; }` over the registry.
const LUAL_UNREF: &str = "53 8b 5c 24 08 85 db 7c 5c 56 b9 f0 d8 ff ff 8b d7 e8 ?? ?? ?? ?? 8b c8 8b 31 33 c0 e8 ?? ?? ?? \
                          ?? 8b 4f 08 8b 10 89 11 8b 40 04 89 41 04 83 47 08 08 53 b9 f0 d8 ff ff 8b f7 e8 ?? ?? ?? ?? \
                          8b 47 08 f3 0f 2a c3 f3 0f 11 00 c7 40 04 03 00 00 00 83 47 08 08 6a 00 b9 f0 d8 ff ff e8 ?? \
                          ?? ?? ?? 83 c4 08 5e 5b c3";
const LUAL_UNREF_INDEX2ADR: usize = 0x11;
const LUAL_UNREF_GETNUM: usize = 0x1c;
const LUAL_UNREF_RAWSETI: [usize; 2] = [0x3a, 0x5c];
/// `lua_rawgeti`, entry to `ret`: `setobj(L->top, luaH_getnum(hvalue(index2adr(L, idx)), n)); L->top++`.
const LUA_RAWGETI: &str = "56 8b d7 e8 ?? ?? ?? ?? 8b c8 8b 44 24 08 8b 31 e8 ?? ?? ?? ?? 8b 10 8b 4f 08 89 11 8b 40 04 89 \
                           41 04 83 47 08 08 5e c3";
const LUA_RAWGETI_INDEX2ADR: usize = 0x03;
const LUA_RAWGETI_GETNUM: usize = 0x10;
/// `lua_pcall`, entry to `ret`: errfunc through `index2adr`, `c.func = L->top - (nargs + 1)`, `c.nresults =
/// EDI`, `luaD_pcall(L, f_call, &c, savestack(c.func), errfunc)`, then `adjustresults`. `??` are the
/// `call` targets and `f_call`'s address.
const LUA_PCALL: &str = "55 8b ec 83 e4 f8 83 ec 0c 85 c9 56 8b f0 75 04 33 d2 eb 0c 8b d6 e8 ?? ?? ?? ?? 2b 46 20 8b d0 \
                         8b 45 08 8d 0c c5 08 00 00 00 8b 46 08 2b c1 89 44 24 08 2b 46 20 89 7c 24 0c 50 8d 44 24 0c 50 \
                         68 ?? ?? ?? ?? 8b c6 e8 ?? ?? ?? ?? 83 c4 0c 83 ff ff 75 0e 8b 4e 14 8b 76 08 3b 71 08 72 03 89 \
                         71 08 5e 8b e5 5d c3";
const LUA_PCALL_INDEX2ADR: usize = 0x16;
/// `push f_call`'s operand.
const LUA_PCALL_F_CALL: usize = 0x40;

/// Resolve the function-reference sites: each pattern must match exactly once in `.text`, and their calls
/// must agree.
pub fn resolve_refs(img: &Image) -> Result<RefSites, String> {
    let text = img.section(".text").ok_or("the image has no .text section")?;
    let one = |name: &str, pattern: &str| -> Result<u32, String> {
        let hits = Pattern::parse(pattern).expect("built-in pattern parses").find_all(text.bytes);
        match hits[..] {
            [at] => Ok(text.va + at as u32),
            _ => Err(format!("{name} matched {} time(s), not once", hits.len())),
        }
    };
    let (lref, unref, rawgeti, pcall) =
        (one("luaL_ref", LUAL_REF), one("luaL_unref", LUAL_UNREF), one("lua_rawgeti", LUA_RAWGETI), one("lua_pcall", LUA_PCALL));
    let failed: Vec<String> = [&lref, &unref, &rawgeti, &pcall].iter().filter_map(|r| r.as_ref().err().cloned()).collect();
    if !failed.is_empty() {
        return Err(format!("the function-reference sites did not resolve: {}", failed.join(", ")));
    }
    let s = RefSites { lual_ref: lref?, lual_unref: unref?, lua_rawgeti: rawgeti?, lua_pcall: pcall? };

    // The target of the `call rel32` at `off` into the function at `f`.
    let target = |f: u32, off: usize| {
        let at = (f - text.va) as usize + off;
        f.wrapping_add(off as u32 + 5).wrapping_add(u32_at(text.bytes, at + 1))
    };
    let agree = |what: &str, calls: &[(&str, u32, usize)]| -> Result<u32, String> {
        let targets: Vec<(&str, u32)> = calls.iter().map(|&(name, f, off)| (name, target(f, off))).collect();
        let first = targets[0].1;
        if targets.iter().any(|&(_, t)| t != first) {
            let listed: Vec<String> = targets.iter().map(|(n, t)| format!("{n} -> {t:#x}")).collect();
            return Err(format!("the function-reference sites call different {what}: {}", listed.join(", ")));
        }
        if !text.contains(first) {
            return Err(format!("the function-reference sites call {what} at {first:#x}, outside .text"));
        }
        Ok(first)
    };
    agree(
        "index2adr",
        &[
            ("luaL_ref", s.lual_ref, LUAL_REF_INDEX2ADR[0]),
            ("luaL_ref", s.lual_ref, LUAL_REF_INDEX2ADR[1]),
            ("luaL_unref", s.lual_unref, LUAL_UNREF_INDEX2ADR),
            ("lua_rawgeti", s.lua_rawgeti, LUA_RAWGETI_INDEX2ADR),
            ("lua_pcall", s.lua_pcall, LUA_PCALL_INDEX2ADR),
        ],
    )?;
    agree(
        "luaH_getnum",
        &[
            ("luaL_ref", s.lual_ref, LUAL_REF_GETNUM[0]),
            ("luaL_ref", s.lual_ref, LUAL_REF_GETNUM[1]),
            ("luaL_unref", s.lual_unref, LUAL_UNREF_GETNUM),
            ("lua_rawgeti", s.lua_rawgeti, LUA_RAWGETI_GETNUM),
        ],
    )?;
    agree(
        "lua_rawseti",
        &[
            ("luaL_ref", s.lual_ref, LUAL_REF_RAWSETI[0]),
            ("luaL_ref", s.lual_ref, LUAL_REF_RAWSETI[1]),
            ("luaL_ref", s.lual_ref, LUAL_REF_RAWSETI[2]),
            ("luaL_unref", s.lual_unref, LUAL_UNREF_RAWSETI[0]),
            ("luaL_unref", s.lual_unref, LUAL_UNREF_RAWSETI[1]),
        ],
    )?;
    let f_call = u32_at(text.bytes, (s.lua_pcall - text.va) as usize + LUA_PCALL_F_CALL + 1);
    if !text.contains(f_call) {
        return Err(format!("lua_pcall at {:#x} passes f_call {f_call:#x}, outside .text", s.lua_pcall));
    }
    Ok(s)
}

/// Resolve every site in `img` and check that they belong together.
pub fn resolve(img: &Image) -> Result<LuaSites, LuaSitesError> {
    let text = img.section(".text").ok_or(LuaSitesError::NoSection(".text"))?;
    let rdata = img.section(".rdata").ok_or(LuaSitesError::NoSection(".rdata"))?;

    let mut found: Vec<(Site, Vec<u32>, Vec<u32>)> = Vec::new();
    for s in Site::ALL {
        let (vas, rows) = match (s.pattern(), s.reg_name()) {
            (Some(p), _) => {
                let hits = Pattern::parse(p).expect("built-in pattern parses").find_all(text.bytes);
                (hits.iter().map(|&o| text.va + o as u32).collect(), Vec::new())
            }
            (None, Some(name)) => {
                let rows = reg_rows(rdata, text, name.as_bytes());
                (rows.iter().map(|r| r.1).collect(), rows.iter().map(|r| r.0).collect())
            }
            (None, None) => unreachable!("every site has a pattern or a registered name"),
        };
        found.push((s, vas, rows));
    }
    if !found.iter().all(|(_, v, _)| v.len() == 1) {
        return Err(LuaSitesError::Matches(found.iter().map(|(s, v, _)| (*s, v.len())).collect()));
    }
    let mut va = [0u32; 12];
    for (s, v, _) in &found {
        va[*s as usize] = v[0];
    }
    let row = |s: Site| found[s as usize].2[0];

    // ScriptHostInit's call lands on the walker.
    let init = va[Site::ScriptHostInit as usize];
    let init_off = (init - text.va) as usize;
    let rel = u32_at(text.bytes, init_off + INIT_CALL_AT + 1);
    let called = init.wrapping_add(INIT_CALL_AT as u32 + 5).wrapping_add(rel);
    let walker = va[Site::RegisterNamespaces as usize];
    if called != walker {
        return Err(LuaSitesError::InitCallsElsewhere { init, called, walker });
    }

    // The constructor: [host] = vtable; [global] = host; eax = host; vtable[+0x14] == ScriptHostInit.
    let ctor = va[Site::ScriptHostCtor as usize];
    let c = (ctor - text.va) as usize;
    let host = u32_at(text.bytes, c + 2);
    let vtable = u32_at(text.bytes, c + 6);
    let published = u32_at(text.bytes, c + 16);
    let returned = u32_at(text.bytes, c + 37);
    if published != host || returned != host {
        return Err(LuaSitesError::Ctor {
            ctor,
            why: format!("stores vtable at {host:#x} but publishes {published:#x} and returns {returned:#x}"),
        });
    }
    match img.u32_at(vtable.wrapping_add(HOST_VTABLE_INIT_SLOT)) {
        Some(f) if f == init => {}
        Some(f) => {
            return Err(LuaSitesError::Ctor {
                ctor,
                why: format!("vtable {vtable:#x} slot +{HOST_VTABLE_INIT_SLOT:#x} is {f:#x}, not ScriptHostInit {init:#x}"),
            });
        }
        None => return Err(LuaSitesError::Ctor { ctor, why: format!("vtable {vtable:#x} is not in the image") }),
    }

    // print, type, loadstring and pcall are rows of one table: the base library.
    let first = row(Site::LuaBLoadString).min(row(Site::LuaBPcall)).min(row(Site::Print)).min(row(Site::LuaBType));
    let last = row(Site::LuaBLoadString).max(row(Site::LuaBPcall)).max(row(Site::Print)).max(row(Site::LuaBType));
    let mut at = first;
    while at < last {
        let name = img.u32_at(at).filter(|&p| p != 0).and_then(|p| img.cstr_at(p));
        let func = img.u32_at(at + 4).filter(|&f| text.contains(f));
        if name.is_none() || func.is_none() {
            return Err(LuaSitesError::BaseLibrary(format!(
                "the row at {at:#x}, between the base functions' rows {first:#x}..{last:#x}, is not a named C function"
            )));
        }
        at += 8;
    }

    // The log stub, or a jump to a detour already installed on it.
    let print = va[Site::Print as usize];
    let head = img.bytes_at(print).map(|b| b[..b.len().min(3)].to_vec()).unwrap_or_default();
    if head != LOG_STUB_BODY && head.first() != Some(&0xe9) {
        return Err(LuaSitesError::LogStub { va: print, bytes: head });
    }

    Ok(LuaSites { va, host })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exe_image::synth;

    #[test]
    fn pattern_parser_is_strict() {
        assert_eq!(Pattern::parse(""), Err(PatternError::Empty));
        assert_eq!(Pattern::parse("?? 90"), Err(PatternError::LeadingWildcard));
        assert_eq!(Pattern::parse("9g"), Err(PatternError::BadToken("9g".into())));
        assert!(Pattern::parse("90 ?? c3").unwrap().matches_at(&[0x90, 0x12, 0xc3], 0));
        let p = Pattern::parse("ab ?? cd").unwrap();
        assert_eq!(p.find_all(&[0xab, 1, 0xcd, 0xab, 2, 0xcd, 0xab]), vec![0, 3]);
    }

    #[test]
    fn every_built_in_pattern_parses_and_every_site_has_one_method() {
        for s in Site::ALL {
            assert!(s.pattern().is_some() != s.reg_name().is_some(), "{}", s.name());
            if let Some(p) = s.pattern() {
                Pattern::parse(p).unwrap();
            }
        }
        let init: Vec<&str> = Site::ScriptHostInit.pattern().unwrap().split_whitespace().collect();
        assert_eq!(init[INIT_CALL_AT], "e8");
        assert_eq!(init.len(), INIT_CALL_AT + 5);
    }

    #[test]
    fn named_rows_need_the_exact_string_and_a_text_function() {
        // .rdata: "type\0typeX\0", then rows {"type" -> .text}, {"typeX" -> .text}, {"type" -> outside}.
        let mut rd = b"\0type\0typeX\0".to_vec();
        rd.resize(16, 0);
        let row = |name: u32, f: u32| [name.to_le_bytes(), f.to_le_bytes()].concat();
        rd.extend(row(0x402001, 0x401004));
        rd.extend(row(0x402006, 0x401008));
        rd.extend(row(0x402001, 0x500000));
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 16]), (".rdata", 0x2000, rd)]);
        let img = Image::from_file(&file).unwrap();
        let (text, rdata) = (img.section(".text").unwrap(), img.section(".rdata").unwrap());
        assert_eq!(reg_rows(rdata, text, b"type"), vec![(0x402010, 0x401004)]);
        assert_eq!(reg_rows(rdata, text, b"typeX"), vec![(0x402018, 0x401008)]);
        assert!(reg_rows(rdata, text, b"pcall").is_empty());
    }

    /// A pattern's bytes with every `??` zero.
    fn body(pattern: &str) -> Vec<u8> {
        pattern.split_whitespace().map(|t| if t == "??" { 0 } else { u8::from_str_radix(t, 16).unwrap() }).collect()
    }

    #[test]
    fn the_ref_patterns_have_their_calls_where_the_checks_read_them() {
        let tok = |p: &str| -> Vec<String> { p.split_whitespace().map(str::to_owned).collect() };
        let calls = |p: &str, offs: &[usize]| {
            let t = tok(p);
            for &o in offs {
                assert_eq!(t[o], "e8", "offset {o:#x}");
                assert!(t[o + 1..o + 5].iter().all(|b| b == "??"), "offset {o:#x}: the target is a wildcard");
            }
        };
        calls(LUAL_REF, &[LUAL_REF_INDEX2ADR[0], LUAL_REF_INDEX2ADR[1], LUAL_REF_GETNUM[0], LUAL_REF_GETNUM[1]]);
        calls(LUAL_REF, &LUAL_REF_RAWSETI);
        calls(LUAL_UNREF, &[LUAL_UNREF_INDEX2ADR, LUAL_UNREF_GETNUM, LUAL_UNREF_RAWSETI[0], LUAL_UNREF_RAWSETI[1]]);
        calls(LUA_RAWGETI, &[LUA_RAWGETI_INDEX2ADR, LUA_RAWGETI_GETNUM]);
        calls(LUA_PCALL, &[LUA_PCALL_INDEX2ADR]);
        assert_eq!(tok(LUA_PCALL)[LUA_PCALL_F_CALL], "68");
        // Whole functions, entry to the last `ret` (pc-v10 0x85FD50..0x85FE29, 0x85FE30..0x85FE97,
        // 0x85DBB0..0x85DBD8, 0x85DF50..0x85DFB7).
        for (p, len) in [(LUAL_REF, 0xd9), (LUAL_UNREF, 0x67), (LUA_RAWGETI, 0x28), (LUA_PCALL, 0x67)] {
            let b = body(p);
            assert_eq!((b.len(), b[b.len() - 1]), (len, 0xc3));
            Pattern::parse(p).unwrap();
        }
    }

    /// `.text` at 0x401000 holding the four functions, their calls aimed at `index2adr`, `luaH_getnum`,
    /// `lua_rawseti` and `f_call`, each a `ret` in the same section; `rawgeti_index2adr` overrides the
    /// one call of `lua_rawgeti` to `index2adr`.
    fn ref_image(rawgeti_index2adr: Option<u32>) -> (Vec<u8>, RefSites) {
        const TEXT: u32 = 0x401000;
        let (index2adr, getnum, rawseti, f_call) = (TEXT, TEXT + 1, TEXT + 2, TEXT + 3);
        let mut text = vec![0xc3, 0xc3, 0xc3, 0xc3];
        let mut place = |pattern: &str, calls: &[(usize, u32)], push: Option<(usize, u32)>| {
            text.resize(text.len().next_multiple_of(16), 0xcc);
            let at = TEXT + text.len() as u32;
            let mut b = body(pattern);
            for &(off, to) in calls {
                let rel = to.wrapping_sub(at + off as u32 + 5);
                b[off + 1..off + 5].copy_from_slice(&rel.to_le_bytes());
            }
            if let Some((off, v)) = push {
                b[off + 1..off + 5].copy_from_slice(&v.to_le_bytes());
            }
            text.extend(b);
            at
        };
        let lual_ref = place(
            LUAL_REF,
            &[
                (LUAL_REF_INDEX2ADR[0], index2adr),
                (LUAL_REF_INDEX2ADR[1], index2adr),
                (LUAL_REF_GETNUM[0], getnum),
                (LUAL_REF_GETNUM[1], getnum),
                (LUAL_REF_RAWSETI[0], rawseti),
                (LUAL_REF_RAWSETI[1], rawseti),
                (LUAL_REF_RAWSETI[2], rawseti),
            ],
            None,
        );
        let lual_unref = place(
            LUAL_UNREF,
            &[
                (LUAL_UNREF_INDEX2ADR, index2adr),
                (LUAL_UNREF_GETNUM, getnum),
                (LUAL_UNREF_RAWSETI[0], rawseti),
                (LUAL_UNREF_RAWSETI[1], rawseti),
            ],
            None,
        );
        let lua_rawgeti = place(
            LUA_RAWGETI,
            &[(LUA_RAWGETI_INDEX2ADR, rawgeti_index2adr.unwrap_or(index2adr)), (LUA_RAWGETI_GETNUM, getnum)],
            None,
        );
        let lua_pcall = place(LUA_PCALL, &[(LUA_PCALL_INDEX2ADR, index2adr)], Some((LUA_PCALL_F_CALL, f_call)));
        (synth::pe(0x400000, &[(".text", 0x1000, text)]), RefSites { lual_ref, lual_unref, lua_rawgeti, lua_pcall })
    }

    #[test]
    fn the_ref_sites_resolve_when_their_calls_agree() {
        let (file, want) = ref_image(None);
        assert_eq!(resolve_refs(&Image::from_file(&file).unwrap()), Ok(want));
    }

    #[test]
    fn a_ref_site_calling_another_index2adr_is_refused() {
        let (file, _) = ref_image(Some(0x401002));
        let e = resolve_refs(&Image::from_file(&file).unwrap()).unwrap_err();
        assert!(e.contains("call different index2adr") && e.contains("lua_rawgeti -> 0x401002"), "{e}");
    }

    #[test]
    fn an_image_without_the_ref_code_names_every_count() {
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64])]);
        let e = resolve_refs(&Image::from_file(&file).unwrap()).unwrap_err();
        for name in ["luaL_ref", "luaL_unref", "lua_rawgeti", "lua_pcall"] {
            assert!(e.contains(&format!("{name} matched 0 time(s), not once")), "{e}");
        }
    }

    #[test]
    fn an_image_without_the_code_reports_every_site() {
        let file = synth::pe(0x400000, &[(".text", 0x1000, vec![0xcc; 64]), (".rdata", 0x2000, vec![0; 64])]);
        let img = Image::from_file(&file).unwrap();
        let e = resolve(&img).unwrap_err();
        let LuaSitesError::Matches(m) = &e else { panic!("{e}") };
        assert_eq!(m.len(), Site::ALL.len());
        assert!(m.iter().all(|(_, n)| *n == 0));
        assert!(e.to_string().contains("luaB_pcall=0 match(es)"), "{e}");
    }
}
