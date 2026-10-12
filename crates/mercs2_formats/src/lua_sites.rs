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
}

impl Site {
    pub const ALL: [Site; 11] = [
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
    va: [u32; 11],
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
    let mut va = [0u32; 11];
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
