//! mercs2_reassemble — reassemble the decompiled Pangea engine from the Ghidra corpus.
//!
//! Joins every attribution source we have (structured `docs/data/*_code_map.json`,
//! prose `docs/reverse_engineer/*_code_map.md`, `scripts/mercs2_annotations.json`,
//! and a reference scrape of the rest of the docs/memory corpus) onto the master
//! function list `output/_ghidra/all_functions_decomp.txt` (27k functions, keyed by
//! virtual address), then emits:
//!   * a regrouped decompiled-C source tree, one module per subsystem, and a sharded
//!     `_unclassified/` bucket for the residue;
//!   * `MANIFEST.json` / `MANIFEST.csv` — addr -> {system,name,tier,confidence,...};
//!   * `REVIEW_QUEUE.md` / `.csv` — the un-identified functions, ranked for manual review;
//!   * `COVERAGE.md` — per-subsystem coverage and the headline "how many still unmapped".
//!
//! The join key is the virtual address, normalised to `0x{:08x}`, so every source
//! (`FUN_00478120`, `0x478120`, `0x00478120`) collapses to one identity.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use serde_json::Value;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(about = "Reassemble the decompiled Mercs2 engine and flag the unmapped residue.")]
struct Args {
    /// Repo root. Defaults to auto-detect by walking up for output/_ghidra/all_functions_decomp.txt
    #[arg(long)]
    repo_root: Option<PathBuf>,
    /// Output directory (default: <repo>/output/engine_reassembled)
    #[arg(long)]
    out: Option<PathBuf>,
    /// Also scrape this extra directory (e.g. the .claude memory dir) for T2 references.
    #[arg(long)]
    extra_refs: Vec<PathBuf>,
}

// ----------------------------------------------------------------------------
// Master function record
// ----------------------------------------------------------------------------

struct Func {
    addr: u32,
    name: String, // as printed in the export (FUN_.., thunk_.., or a real symbol)
    size: u32,
    caller_count: u32,
    caller_addrs: Vec<u32>, // caller FUNCTION addresses (from the parenthesised FUN_ in callers=[])
    body: String,           // raw decompiled text
}

/// True if `name` is a real recovered symbol, not a Ghidra placeholder.
fn is_symbolic(name: &str) -> bool {
    !(name.starts_with("FUN_") || name.starts_with("thunk_FUN_") || name.starts_with("_FUN_"))
}

/// A Ghidra disassembly artifact, not a real function (bad instruction data / data-as-code).
fn is_artifact(body: &str) -> bool {
    body.contains("halt_baddata") || body.contains("Bad instruction")
}

/// The best available name: the master symbol if real, else a recovered (FID/RTTI/symbol) name.
fn best_name<'a>(f: &'a Func, primary: &'a HashMap<u32, Primary>) -> &'a str {
    if is_symbolic(&f.name) {
        return &f.name;
    }
    if let Some(p) = primary.get(&f.addr) {
        if let Some(nm) = &p.name {
            if is_symbolic(nm) {
                return nm;
            }
        }
    }
    &f.name
}

// ----------------------------------------------------------------------------
// Attribution: a raw Hit per source, reduced to one Primary per address.
// ----------------------------------------------------------------------------

#[derive(Clone)]
struct Hit {
    system: Option<String>,
    name: Option<String>,
    role: Option<String>,
    confidence: Option<String>,
    evidence: Option<String>,
    source: String,
    tier: u8, // 1 = subsystem-attributed, 2 = merely referenced
}

struct Primary {
    tier: u8,
    system: String,
    name: Option<String>,
    role: Option<String>,
    confidence: Option<String>,
    evidence: Option<String>,
    source: String,
    n_sources: usize,
}

/// A *probable* (not evidence-backed) subsystem assignment for a residue function.
struct Inferred {
    system: String,
    method: &'static str, // "locality-strong" | "locality-weak" | "callgraph"
    note: String,         // human-readable justification (bracketing anchors / vote)
}

fn conf_weight(c: &Option<String>) -> u8 {
    match c.as_deref() {
        Some("high") => 3,
        Some("med") | Some("medium") => 2,
        Some("confirm-live") => 2,
        Some("low") => 1,
        _ => 0,
    }
}

/// Prefer lower tier number (1 best), then higher confidence, then json over md.
fn hit_rank(h: &Hit) -> (u8, u8, u8) {
    let tier_score = 10 - h.tier;
    let json_bonus = u8::from(h.source.ends_with(".json"));
    (tier_score, conf_weight(&h.confidence), json_bonus)
}

// ----------------------------------------------------------------------------
// Address scanning (no regex dependency)
// ----------------------------------------------------------------------------

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// Collect every function-address-shaped token in `s`:
///   * `FUN_<hex>` / `thunk_FUN_<hex>` (always a function)
///   * `0x<hex>` with 6-8 hex digits (validate against the master set at the call site)
/// Returns (addr, was_fun_prefixed).
fn scan_addrs(s: &str) -> Vec<(u32, bool)> {
    let cs: Vec<char> = s.chars().collect();
    let n = cs.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if i + 4 <= n && cs[i] == 'F' && cs[i + 1] == 'U' && cs[i + 2] == 'N' && cs[i + 3] == '_' {
            let mut j = i + 4;
            while j < n && is_hex(cs[j]) {
                j += 1;
            }
            if j > i + 4 {
                if let Ok(a) = u32::from_str_radix(&cs[i + 4..j].iter().collect::<String>(), 16) {
                    out.push((a, true));
                }
            }
            i = j.max(i + 1);
            continue;
        }
        if i + 2 <= n && cs[i] == '0' && (cs[i + 1] == 'x' || cs[i + 1] == 'X') {
            let mut j = i + 2;
            while j < n && is_hex(cs[j]) {
                j += 1;
            }
            let len = j - (i + 2);
            if (6..=8).contains(&len) {
                if let Ok(a) = u32::from_str_radix(&cs[i + 2..j].iter().collect::<String>(), 16) {
                    out.push((a, false));
                }
            }
            i = j.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

fn norm(a: u32) -> String {
    format!("0x{:08x}", a)
}

/// CSV-escape a field (demangled names contain commas and parens).
fn csv(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn sanitize(s: &str) -> String {
    let t: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let t = t.trim_matches('_').to_string();
    if t.is_empty() {
        "unknown".to_string()
    } else {
        t
    }
}

// ----------------------------------------------------------------------------
// Master parse
// ----------------------------------------------------------------------------

fn parse_master(path: &Path) -> Vec<Func> {
    let text = fs::read_to_string(path).expect("read master decomp");
    let mut funcs = Vec::new();
    let mut cur: Option<Func> = None;
    let mut body = String::new();
    for line in text.lines() {
        if line.starts_with("==== ") && line.contains("@0x") {
            if let Some(mut f) = cur.take() {
                f.body = std::mem::take(&mut body);
                funcs.push(f);
            }
            body.clear();
            cur = parse_header(line);
            continue;
        }
        if !line.is_empty() && line.chars().all(|c| c == '=') {
            continue; // separator banner
        }
        if cur.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(mut f) = cur.take() {
        f.body = body;
        funcs.push(f);
    }
    funcs
}

fn parse_header(line: &str) -> Option<Func> {
    let inner = line.trim_start_matches("==== ").trim_end_matches("====").trim();
    let at = inner.find("@0x")?;
    let name = inner[..at].trim().to_string();
    let rest = &inner[at + 3..];
    let addr_str: String = rest.chars().take_while(|c| is_hex(*c)).collect();
    let addr = u32::from_str_radix(&addr_str, 16).ok()?;
    let size = field_usize(rest, "size=").unwrap_or(0) as u32;
    let callers_str = between(rest, "callers=[", "]").unwrap_or("");
    let caller_count = callers_str.matches('(').count() as u32;
    // caller FUNCTION addr = the parenthesised FUN_ token in each `0xsite(FUN_owner)` entry.
    let mut caller_addrs = Vec::new();
    for (a, is_fun) in scan_addrs(callers_str) {
        if is_fun {
            caller_addrs.push(a);
        }
    }
    Some(Func { addr, name, size, caller_count, caller_addrs, body: String::new() })
}

fn field_usize(s: &str, key: &str) -> Option<usize> {
    let idx = s.find(key)? + key.len();
    let digits: String = s[idx..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn between<'a>(s: &'a str, a: &str, b: &str) -> Option<&'a str> {
    let start = s.find(a)? + a.len();
    let end = s[start..].find(b)? + start;
    Some(&s[start..end])
}

// ----------------------------------------------------------------------------
// JSON code-map walk
// ----------------------------------------------------------------------------

fn str_field(o: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(Value::String(s)) = o.get(*k) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

/// Normalise messy confidence strings ("high (body read)", "M", "H") to high/med/low.
fn norm_conf(s: &str) -> String {
    match s.trim().chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('h') => "high",
        Some('m') => "med",
        Some('l') => "low",
        Some('c') => "confirm-live",
        _ => "med",
    }
    .to_string()
}

fn walk_json(v: &Value, file_default: &str, source: &str, out: &mut Vec<(u32, Hit)>) {
    match v {
        Value::Object(o) => {
            // find the record's function address: try addr / ghidra_name / fn / va / address
            let addr = ["addr", "ghidra_name", "fn", "va", "address"]
                .iter()
                .find_map(|k| {
                    o.get(*k)
                        .and_then(|x| x.as_str())
                        .and_then(|s| scan_addrs(s).first().map(|(a, _)| *a))
                });
            if let Some(a) = addr {
                let system =
                    str_field(o, &["system", "subsystem"]).unwrap_or_else(|| file_default.to_string());
                let mut role = str_field(o, &["role", "desc", "description"]);
                if let Some(extra) = str_field(o, &["class", "layer"]) {
                    role = Some(match role {
                        Some(r) => format!("[{}] {}", extra, r),
                        None => extra,
                    });
                }
                out.push((
                    a,
                    Hit {
                        system: Some(system),
                        name: str_field(o, &["ghidra_name", "name"]),
                        role,
                        confidence: str_field(o, &["confidence", "conf"]).map(|c| norm_conf(&c)),
                        evidence: str_field(o, &["evidence"]),
                        source: source.to_string(),
                        tier: 1,
                    },
                ));
            }
            for (_, child) in o {
                walk_json(child, file_default, source, out);
            }
        }
        Value::Array(a) => {
            for child in a {
                walk_json(child, file_default, source, out);
            }
        }
        _ => {}
    }
}

// ----------------------------------------------------------------------------
// helpers
// ----------------------------------------------------------------------------

fn find_repo_root(explicit: Option<PathBuf>) -> PathBuf {
    if let Some(p) = explicit {
        return p;
    }
    let mut dir = std::env::current_dir().expect("cwd");
    loop {
        if dir.join("output/_ghidra/all_functions_decomp.txt").exists() {
            return dir;
        }
        if !dir.pop() {
            panic!("could not locate repo root — pass --repo-root");
        }
    }
}

/// Fold synonym subsystem names (same subsystem, different source spelling) onto one
/// canonical module name. Genuinely distinct-but-adjacent systems (shadow vs render_core,
/// region_cache vs population) are left separate. Raw system is kept in the manifest.
fn canon_system(s: &str) -> &str {
    match s {
        "vehicle" => "vehicles",
        "scheduler" => "scheduler_tick",
        "pimp" => "pimp_job_system",
        "lua_binding" => "scripting_host_binding",
        "fx" | "particle_fx_shadow" => "particle_fx",
        "population_update" | "spawner" | "spawn_worker" | "death" => "population_spawner",
        "sky_atmosphere" | "hdr_post" => "sky_post_hdr",
        "scaleform" => "scaleform_gfx",
        other => other,
    }
}

/// Library/utility modules produced purely from a function's recovered symbol name
/// (or the caller-spread heuristic). The shared layers underneath the gameplay subsystems.
const UTILITY_MODULES: &[&str] = &["crt", "cpp_runtime", "havok", "scaleform_gfx", "utility_shared"];

fn is_utility_module(m: &str) -> bool {
    UTILITY_MODULES.contains(&m)
}

/// Exact-match MSVC C runtime symbols (string/mem/stdio/math/startup).
const CRT_NAMES: &[&str] = &[
    "strlen", "strcpy", "strncpy", "strcat", "strncat", "strcmp", "strncmp", "stricmp", "strnicmp",
    "strchr", "strrchr", "strstr", "strpbrk", "strcspn", "strspn", "strtok", "strlwr", "strupr",
    "strdup", "strncnt", "memset", "memcpy", "memmove", "memcmp", "memchr", "malloc", "calloc",
    "realloc", "free", "_malloc", "_free", "_calloc", "_realloc", "sprintf", "_sprintf", "snprintf",
    "_snprintf", "vsprintf", "_vsprintf", "vsnprintf", "_vsnprintf", "printf", "fprintf", "vfprintf",
    "fputs", "fputc", "fwrite", "fread", "fopen", "fclose", "fflush", "fseek", "ftell", "sscanf",
    "atoi", "atol", "atof", "strtol", "strtoul", "strtod", "itoa", "_itoa", "ltoa", "ultoa",
    "qsort", "bsearch", "rand", "srand", "abort", "exit", "_exit", "doexit", "atexit", "_onexit",
    "_cinit", "_initterm", "_amsg_exit", "raise", "signal", "siglookup", "setSBCS", "_ismbblead",
    "wcslen", "wcscpy", "mbstowcs", "wcstombs", "toupper", "tolower", "isspace", "isdigit", "isalpha",
    "ceil", "floor", "fabs", "sqrt", "pow", "sin", "cos", "tan", "atan", "atan2", "asin", "acos",
    "exp", "log", "log10", "fmod", "frexp", "ldexp", "modf", "_ftol", "_ftol2", "_CIsqrt", "_CIpow",
];

/// MSVC C++ exception-handling / RTTI runtime symbols.
const CPP_RT_NAMES: &[&str] = &[
    "CatchIt", "CallCatchBlock", "BuildCatchObject", "FindHandler", "FindHandlerForForeignException",
    "AdjustPointer", "TypeMatch", "IsExceptionObjectToBeDestroyed", "TranslatorGuardHandler",
    "DestructExceptionObject", "ExFilterRethrow", "CreateFrameInfo", "FindAndUnlinkFrame",
    "_CxxThrowException", "__CxxFrameHandler", "__CxxFrameHandler3", "_purecall", "terminate",
    "unexpected", "_inconsistency", "type_info", "__RTDynamicCast", "__RTtypeid", "__RTCastToVoid",
    "_global_unwind", "_local_unwind", "_except_handler", "_XcptFilter", "_setjmp", "longjmp",
];

/// Scaleform GFx base-library class heads (shared by symbol + RTTI classifiers).
const GFX_HEADS: &[&str] = &[
    "GMatrix2D", "GColor", "GImage", "GString", "GArray", "GPtr", "GRefCountBase",
    "GRefCountBaseImpl", "GRefCountBaseNTS", "GRefCountImpl", "GMemory", "GAllocator", "GFxString",
    "GFile", "GZLibFile", "GBufferReader", "GRenderer", "GRendererD3D9", "GPoint", "GRect",
    "GList", "GHash", "GAtomicInt", "GStat", "GDataFile", "GNewOverrideBase", "GRangeAllocator",
    "GDebug", "GLock", "GMemoryHeap", "GSysAllocPaged",
];

/// Map an RTTI class/namespace name to a subsystem (evidence-grade — from Ghidra's RTTI).
/// Returns None for game classes (Pg*/Mrx*) that need a keyword match at the call site.
fn class_to_subsystem(c: &str) -> Option<&'static str> {
    if c.starts_with("hkp") { return Some("physics"); }
    if c.starts_with("hka") || c.starts_with("hkb") { return Some("animation"); }
    if c.starts_with("hk") { return Some("havok"); }
    if c.starts_with("GFx") || GFX_HEADS.contains(&c) || GFX_HEADS.iter().any(|h| c.starts_with(h)) {
        return Some("scaleform_gfx");
    }
    if c.starts_with("std") || c.contains("std::") || c.starts_with("type_info") || c.starts_with("MSVCP") {
        return Some("cpp_runtime");
    }
    if c.starts_with("MSVCR") { return Some("crt"); }
    if c.starts_with("WS2_32") || c.contains("WSOCK") { return Some("networking"); }
    if c.starts_with("DSOUND") { return Some("audio"); }
    if c.starts_with("DINPUT") { return Some("input"); }
    if c.starts_with("D3D") { return Some("render_core"); }
    if c.starts_with("Pal") { return Some("audio"); }
    const OS_DLLS: &[&str] = &[
        "KERNEL32", "USER32", "GDI32", "ole32", "OLE32", "ADVAPI32", "SHELL32", "WINMM", "ntdll",
        "NTDLL", "MSIMG32", "VERSION", "IMM32", "XINPUT", "COMCTL32", "OLEAUT32", "SHLWAPI",
    ];
    if OS_DLLS.iter().any(|k| c.starts_with(k)) { return Some("crt"); }
    None
}

/// Map a FID-recovered library symbol (MSVC/ATL/STL/openssl) to a subsystem module.
fn fid_name_to_subsystem(name: &str, library: &str) -> &'static str {
    let lib = library.to_ascii_lowercase();
    if lib.contains("openssl") || lib.contains("sodium") || lib.contains("crypto") {
        return "networking";
    }
    // ATL/MFC and mangled C++ / STL templates
    if name.contains("ATL") || name.contains("CString") || name.contains("CComCrit")
        || name.starts_with("??") || name.contains("@@")
    {
        return "cpp_runtime";
    }
    // everything else from the VS runtime is the C runtime
    "crt"
}

/// Binary-derived subsystem signals: substrings found in a function's body (imported API
/// names, Ghidra string-label globals `s_*`, and class-reference tokens) map to a subsystem.
/// Unlike locality/call-graph inference, the token is literally present in the compiled image,
/// so a decisive vote is evidence-grade (med confidence). Order-independent; votes accumulate.
/// (needle matched case-insensitively as a substring of body identifiers, subsystem)
const SIGNAL_RULES: &[(&str, &str)] = &[
    // render / D3D
    ("D3DX", "render_core"), ("Direct3DDevice", "render_core"), ("SetRenderState", "render_core"),
    ("DrawIndexedPrimitive", "render_core"), ("DrawPrimitive", "render_core"),
    ("CreateVertexShader", "render_core"), ("CreatePixelShader", "render_core"),
    ("SetTexture", "render_core"), ("_sho", "render_core"), ("Mtrl", "render_core"),
    ("VertexBuffer", "render_core"), ("IndexBuffer", "render_core"), ("Renderable", "render_core"),
    // audio
    ("DirectSound", "audio"), ("IDirectSound", "audio"), ("DSBUFFER", "audio"), ("waveOut", "audio"),
    ("EAXReverb", "audio"), ("EAXListener", "audio"), ("s_Sound", "audio"), ("s_Audio", "audio"),
    ("s_Voice", "audio"), ("s_Music", "audio"), ("sounddb", "audio"), ("PalSound", "audio"),
    // networking
    ("WSAStartup", "networking"), ("recvfrom", "networking"), ("sendto", "networking"),
    ("htons", "networking"), ("inet_", "networking"), ("closesocket", "networking"),
    ("s_Fesl", "networking"), ("XLSP", "networking"), ("s_Net", "networking"),
    // input
    ("DirectInput", "input"), ("IDirectInput", "input"), ("GetDeviceState", "input"),
    ("DIJOYSTATE", "input"), ("GetDeviceData", "input"), ("DIERR", "input"),
    // scripting
    ("lua_", "scripting_host_binding"), ("luaL_", "scripting_host_binding"),
    ("lua_State", "scripting_host_binding"), ("luaU_", "scripting_host_binding"),
    // physics (Havok hkp*/collision)
    ("hkpWorld", "physics"), ("hkpRigidBody", "physics"), ("hkpCharacter", "physics"),
    ("hkpConstraint", "physics"), ("hkContact", "physics"), ("hkMopp", "physics"),
    ("hkpShape", "physics"), ("hkpCollid", "physics"), ("hkSimulation", "physics"),
    ("hkpAction", "physics"), ("SampledHeightField", "physics"),
    // animation (Havok hka*/hkb*, FaceFX)
    ("hkaSkeleton", "animation"), ("hkaAnimat", "animation"), ("hkaPose", "animation"),
    ("hkbGetUp", "animation"), ("hkaRagdoll", "animation"), ("FaceFX", "animation"),
    ("referencePose", "animation"), ("s_hierarchy", "animation"), ("s_bones", "animation"),
    // generic havok utility (hkVector/hkMath/hkMemory/hkArray)
    ("hkVector", "havok"), ("hkMath", "havok"), ("hkMemory", "havok"), ("hkArray", "havok"),
    ("hkQsTransform", "havok"), ("hkQuaternion", "havok"), ("hkDefaultError", "havok"),
    // scaleform
    ("GFx", "gui_hud_scaleform"), ("GRenderer", "gui_hud_scaleform"), ("Scaleform", "gui_hud_scaleform"),
    ("GMatrix2D", "scaleform_gfx"), ("GColor", "scaleform_gfx"), ("GImage", "scaleform_gfx"),
    // jobs / threading
    ("pimpQueue", "pimp_job_system"), ("s_pimp", "pimp_job_system"), ("jobtype", "pimp_job_system"),
    // world content
    ("Water", "water"), ("Buoyancy", "water"),
    ("Hibernation", "world_streaming"), ("StreamBlock", "world_streaming"), ("CacheIn", "world_streaming"),
    ("Population", "population_spawner"), ("Spawner", "population_spawner"),
    ("decaltable", "decal"), ("s_Decal", "decal"),
    ("Atmosphere", "sky_post_hdr"), ("s_Cloud", "sky_post_hdr"), ("s_Bloom", "sky_post_hdr"),
    ("BlobShadow", "shadow"), ("ShadowMap", "shadow"),
    ("Faction", "faction_reputation"), ("mrxfaction", "faction_reputation"),
    ("Airstrike", "weapons_combat"), ("Projectile", "weapons_combat"), ("Homing", "weapons_combat"),
    ("s_Weapon", "weapons_combat"), ("Explosion", "weapons_combat"),
    ("Intersection", "road_graph_ai_driving"), ("RoadGraph", "road_graph_ai_driving"),
    ("Emitter", "particle_fx"), ("fxdict", "particle_fx"), ("Ribbon", "particle_fx"),
    ("s_Profile", "save_serialize"), ("SaveData", "save_serialize"),
];

/// Extract a subsystem vote from a function body. Returns (top_subsystem, top_votes, runner_up_votes).
fn body_signal(body: &str) -> Option<(String, u32, u32)> {
    let low = body.to_ascii_lowercase();
    let mut votes: HashMap<&str, u32> = HashMap::new();
    for (needle, sys) in SIGNAL_RULES {
        // count non-overlapping occurrences of the (lowercased) needle
        let nlow = needle.to_ascii_lowercase();
        let mut cnt = 0u32;
        let mut from = 0usize;
        while let Some(pos) = low[from..].find(&nlow) {
            cnt += 1;
            from += pos + nlow.len();
            if cnt >= 8 {
                break;
            }
        }
        if cnt > 0 {
            *votes.entry(sys).or_insert(0) += cnt;
        }
    }
    if votes.is_empty() {
        return None;
    }
    let mut v: Vec<(&&str, &u32)> = votes.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    let top = *v[0].1;
    let second = v.get(1).map(|x| *x.1).unwrap_or(0);
    Some((v[0].0.to_string(), top, second))
}

/// Classify a function into a library/utility module from its recovered symbol name.
/// Returns `Some((module, category))`; conservative — bare/ambiguous method names return None.
fn classify_lib(name: &str) -> Option<(&'static str, &'static str)> {
    if !is_symbolic(name) {
        return None;
    }
    // strip a leading destructor tilde and any Class:: qualification for prefix tests
    let base = name.trim_start_matches('~');
    let leaf = base.rsplit("::").next().unwrap_or(base);
    let head = base.split("::").next().unwrap_or(base);

    if CRT_NAMES.contains(&leaf) || CRT_NAMES.contains(&base) {
        return Some(("crt", "C runtime (string/mem/stdio/math/startup)"));
    }
    // MSVC-internal names Ghidra recovers but the whitelist doesn't enumerate:
    // triple-underscore internals (___sbh_*, ___crt*), 64-bit math helpers, /GS, /RTC.
    if base.starts_with("___")
        || base.starts_with("__sbh")
        || base.starts_with("_CRT")
        || base.starts_with("__crt")
        || base.starts_with("__security")
        || base.starts_with("_RTC")
        || base.starts_with("__RTC")
        || matches!(base,
            "__allmul" | "__alldiv" | "__aulldiv" | "__allrem" | "__aullrem" | "__allshr"
            | "__aullshr" | "__allshl" | "__aulldvrm" | "__alldvrm" | "__ftol" | "__ftol2"
            | "__ftol2_sse" | "__chkstk" | "__alloca_probe" | "__EH_prolog" | "__SEH_prolog"
            | "__SEH_epilog" | "__onexit" | "__dllonexit")
    {
        return Some(("crt", "MSVC C runtime internal / compiler helper"));
    }
    if base.contains("std::") || name.contains("std::") {
        return Some(("cpp_runtime", "C++ std / template runtime"));
    }
    if CPP_RT_NAMES.iter().any(|n| base == *n || leaf == *n || base.contains(*n)) {
        return Some(("cpp_runtime", "C++ exception-handling / RTTI runtime"));
    }
    if base == "exception" || leaf == "exception" || base.contains("bad_alloc") || base.contains("bad_cast")
    {
        return Some(("cpp_runtime", "C++ exception types"));
    }
    // Havok: hk<Upper>...
    let hb = head.as_bytes();
    if hb.len() > 2 && &head[..2] == "hk" && hb[2].is_ascii_uppercase() {
        return Some(("havok", "Havok middleware (hk*)"));
    }
    // Scaleform GFx base library: G<Upper> class family (whitelist heads).
    if GFX_HEADS.contains(&head) || (head.starts_with("GFx") && head.len() > 3) {
        return Some(("scaleform_gfx", "Scaleform GFx base library (G*)"));
    }
    None
}

fn json_file_default(stem: &str) -> String {
    match stem {
        "vehicle_code_map" => "vehicles",
        "audio_code_map" => "audio",
        "animation_code_map" => "animation",
        "particle_fx_shadow_code_map" => "particle_fx_shadow",
        "scaleform_gfx_function_map" => "gui_hud_scaleform",
        "road_graph_ai_driving_code_map" => "road_graph_ai_driving",
        other => other.trim_end_matches("_code_map"),
    }
    .to_string()
}

// ----------------------------------------------------------------------------
// main
// ----------------------------------------------------------------------------

fn main() {
    let args = Args::parse();
    let root = find_repo_root(args.repo_root.clone());
    let out_dir = args.out.clone().unwrap_or_else(|| root.join("output/engine_reassembled"));
    eprintln!("repo root : {}", root.display());
    eprintln!("out dir   : {}", out_dir.display());

    // 1. master list ---------------------------------------------------------
    let master_path = root.join("output/_ghidra/all_functions_decomp.txt");
    let funcs = parse_master(&master_path);
    eprintln!("master    : {} functions parsed", funcs.len());
    let master_set: HashMap<u32, usize> =
        funcs.iter().enumerate().map(|(i, f)| (f.addr, i)).collect();

    // 2. attribution hits ----------------------------------------------------
    let mut hits: HashMap<u32, Vec<Hit>> = HashMap::new();

    // 2a. structured JSON code maps
    let mut json_maps = Vec::new();
    for entry in WalkDir::new(root.join("docs/data")).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().map(|e| e == "json").unwrap_or(false) {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            if name.contains("code_map") || name.contains("function_map") {
                json_maps.push(p.to_path_buf());
            }
        }
    }
    for p in &json_maps {
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        let src = format!("docs/data/{}", p.file_name().unwrap().to_string_lossy());
        let file_default = json_file_default(&stem);
        match fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
            Some(v) => {
                let mut recs = Vec::new();
                walk_json(&v, &file_default, &src, &mut recs);
                eprintln!("json map  : {:<42} {} fns", src, recs.len());
                for (a, h) in recs {
                    hits.entry(a).or_default().push(h);
                }
            }
            None => eprintln!("json map  : {:<42} PARSE FAIL", src),
        }
    }

    // 2a-bis. Ghidra RTTI class map (func_class_map.csv from ExportFuncClass.java) — a function's
    // parent namespace / vtable-owning class is RTTI-derived, so class->subsystem is evidence-grade.
    let fcm = root.join("output/_ghidra/func_class_map.csv");
    if let Ok(text) = fs::read_to_string(&fcm) {
        let mut n = 0;
        let mut seen: HashSet<(u32, String)> = HashSet::new();
        for line in text.lines().skip(1) {
            // addr,class,name,source  (class/name may be quoted)
            let (addr_s, rest) = match line.split_once(',') {
                Some(x) => x,
                None => continue,
            };
            let a = match scan_addrs(addr_s).first() {
                Some((a, _)) => *a,
                None => continue,
            };
            // class = first field of rest, honouring quotes
            let class = if let Some(stripped) = rest.strip_prefix('"') {
                stripped.split('"').next().unwrap_or("").to_string()
            } else {
                rest.split(',').next().unwrap_or("").to_string()
            };
            if class.is_empty() {
                continue;
            }
            // strip template args for the subsystem decision
            let head: String = class.split('<').next().unwrap_or(&class).to_string();
            let (sys, conf) = match class_to_subsystem(&head) {
                Some(s) => (s.to_string(), "high"),
                None => match body_signal(&head) {
                    Some((s, _, _)) => (s, "med"), // Pg*/Mrx* game classes by keyword
                    None => continue,
                },
            };
            if !seen.insert((a, sys.clone())) {
                continue;
            }
            hits.entry(a).or_default().push(Hit {
                system: Some(sys),
                name: Some(head.clone()),
                role: Some(format!("RTTI class {}", class)),
                confidence: Some(conf.to_string()),
                evidence: Some(format!("Ghidra RTTI: member of {}", class)),
                source: "ghidra-rtti".to_string(),
                tier: 1,
            });
            n += 1;
        }
        eprintln!("rtti map  : {:<42} {} class attributions", "output/_ghidra/func_class_map.csv", n);
    } else {
        eprintln!("rtti map  : func_class_map.csv not found (run ExportFuncClass.java) — skipping");
    }

    // 2a-ter. FID byte-signature matches (fid_matches.csv from FidApplyExport.java). A FID hit is
    // an exact library-function body match => evidence-grade identity + a REAL name (e.g. the CRT
    // small-block allocator ___sbh_alloc_block). addr,fid_name,score,library,version.
    let fidp = root.join("output/_ghidra/fid_matches.csv");
    if let Ok(text) = fs::read_to_string(&fidp) {
        let mut n = 0;
        for line in text.lines().skip(1) {
            // addr,fid_name,score,library,version  (fid_name may be quoted, contains commas)
            let addr_s = line.split(',').next().unwrap_or("");
            let a = match scan_addrs(addr_s).first() {
                Some((a, _)) => *a,
                None => continue,
            };
            // parse fid_name honouring quotes; then score/library
            let after = &line[addr_s.len() + 1..];
            let (fid_name, tail) = if let Some(rest) = after.strip_prefix('"') {
                match rest.split_once('"') {
                    Some((nm, t)) => (nm.to_string(), t.trim_start_matches(',')),
                    None => continue,
                }
            } else {
                match after.split_once(',') {
                    Some((nm, t)) => (nm.to_string(), t),
                    None => (after.to_string(), ""),
                }
            };
            let mut tcols = tail.split(',');
            let score: f32 = tcols.next().unwrap_or("0").parse().unwrap_or(0.0);
            let library = tcols.next().unwrap_or("");
            let sys = fid_name_to_subsystem(&fid_name, library);
            hits.entry(a).or_default().push(Hit {
                system: Some(sys.to_string()),
                name: Some(fid_name.clone()),
                role: Some(format!("FID match: {} (score {:.0}, {})", fid_name, score, library)),
                confidence: Some(if score >= 50.0 { "high" } else { "med" }.to_string()),
                evidence: Some(format!("byte-signature FID: {} [{}]", fid_name, library)),
                source: "fid".to_string(),
                tier: 1,
            });
            n += 1;
        }
        eprintln!("fid map   : {:<42} {} byte-signature matches", "output/_ghidra/fid_matches.csv", n);
    } else {
        eprintln!("fid map   : fid_matches.csv not found (run FidApplyExport.java) — skipping");
    }

    // 2a-quater. Lua binding surface (mods/lua_trace_asi/reference/binding_map.json). Each luaL_Reg
    // entry pairs a real Lua name with a cfunc pointer in .rdata => evidence-grade name + the
    // scripting-host binding layer. cfunc addr is stored as an RVA or VA; disambiguate vs master.
    let bmp = root.join("mods/lua_trace_asi/reference/binding_map.json");
    if let Some(Value::Object(o)) =
        fs::read_to_string(&bmp).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        let base: u32 = o.get("provenance").and_then(|p| p.get("image_base")).and_then(|v| v.as_u64()).unwrap_or(0x40_0000) as u32;
        let mut n = 0;
        if let Some(Value::Array(tables)) = o.get("tables") {
            for t in tables {
                let group = t.get("group").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(Value::Array(entries)) = t.get("entries") {
                    for e in entries {
                        let name = match e.get("name").and_then(|v| v.as_str()) {
                            Some(s) => s,
                            None => continue,
                        };
                        let raw = match e.get("cfunc_rva").and_then(|v| v.as_u64()) {
                            Some(r) => r as u32,
                            None => continue,
                        };
                        // resolve: value may be a VA already, or an RVA needing +image_base
                        let addr = if master_set.contains_key(&raw) {
                            raw
                        } else if master_set.contains_key(&raw.wrapping_add(base)) {
                            raw.wrapping_add(base)
                        } else {
                            continue;
                        };
                        hits.entry(addr).or_default().push(Hit {
                            system: Some("scripting_host_binding".to_string()),
                            name: Some(name.to_string()),
                            role: Some(format!("Lua binding '{}' (group {})", name, group)),
                            confidence: Some("high".to_string()),
                            evidence: Some(format!("luaL_Reg entry '{}' -> cfunc (binding_map.json)", name)),
                            source: "lua-binding".to_string(),
                            tier: 1,
                        });
                        n += 1;
                    }
                }
            }
        }
        eprintln!("lua bind  : {:<42} {} cfunc names resolved", "binding_map.json", n);
    }

    // 2b. annotations (load-path map, 0x-keyed)
    let ann_path = root.join("scripts/mercs2_annotations.json");
    if let Some(Value::Object(o)) =
        fs::read_to_string(&ann_path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        let mut n = 0;
        for (k, val) in &o {
            if !k.starts_with("0x") {
                continue;
            }
            if let Some((a, _)) = scan_addrs(k).first().copied() {
                let ob = val.as_object();
                hits.entry(a).or_default().push(Hit {
                    system: Some("asset_load".to_string()),
                    name: ob.and_then(|o| str_field(o, &["name"])),
                    role: ob.and_then(|o| str_field(o, &["role"])),
                    confidence: ob.and_then(|o| str_field(o, &["confidence"])),
                    evidence: ob.and_then(|o| str_field(o, &["evidence"])),
                    source: "scripts/mercs2_annotations.json".to_string(),
                    tier: 1,
                });
                n += 1;
            }
        }
        eprintln!("json map  : {:<42} {} fns", "scripts/mercs2_annotations.json", n);
    }

    // 2c. prose subsystem code maps -> T1/med
    for entry in WalkDir::new(root.join("docs/reverse_engineer")).into_iter().filter_map(|e| e.ok()) {
        let p = entry.path();
        let fname = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        // subsystem code maps AND class/function maps (e.g. scaleform_gfx_class_map.md)
        let stem = if fname.ends_with("_code_map.md") {
            fname.trim_end_matches("_code_map.md")
        } else if fname.ends_with("_class_map.md") {
            fname.trim_end_matches("_class_map.md")
        } else if fname.ends_with("_function_map.md") {
            fname.trim_end_matches("_function_map.md")
        } else {
            continue;
        };
        let system = stem.to_string();
        let src = format!("docs/reverse_engineer/{}", fname);
        let text = match fs::read_to_string(p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let mut seen_here: HashSet<u32> = HashSet::new();
        for line in text.lines() {
            for (a, is_fun) in scan_addrs(line) {
                if !is_fun && !master_set.contains_key(&a) {
                    continue; // avoid DAT_/hash/global addresses fabricating functions
                }
                if !seen_here.insert(a) {
                    continue; // first mention per file wins the role snippet
                }
                let role = line.trim().chars().take(240).collect::<String>();
                hits.entry(a).or_default().push(Hit {
                    system: Some(system.clone()),
                    name: None,
                    role: Some(role),
                    confidence: Some("med".to_string()),
                    evidence: Some(format!("cited in {}", src)),
                    source: src.clone(),
                    tier: 1,
                });
            }
        }
    }

    // 2c-bis. symbol-name library classification (CRT / C++ runtime / Havok / Scaleform GFx).
    // A function literally named `strlen` IS the CRT — this is identity, not inference.
    let mut lib_n = 0usize;
    for f in &funcs {
        if let Some((module, cat)) = classify_lib(&f.name) {
            hits.entry(f.addr).or_default().push(Hit {
                system: Some(module.to_string()),
                name: Some(f.name.clone()),
                role: Some(cat.to_string()),
                confidence: Some("high".to_string()),
                evidence: Some(format!("recovered symbol `{}`", f.name)),
                source: "symbol-name".to_string(),
                tier: 1,
            });
            lib_n += 1;
        }
    }
    eprintln!("symbol lib: {} functions classified as CRT/C++rt/Havok/Scaleform by name", lib_n);

    // 2c-ter. binary-signal evidence — string-labels / imports / class-refs found IN the body.
    // A decisive vote (top >= 3 hits and >= 2x the runner-up) is evidence-grade (the token is in
    // the compiled image), so promote it to a tier-1 med attribution. The full signal map is
    // retained for corroborating the (weaker) locality/call-graph inference.
    let mut signals: HashMap<u32, (String, u32, u32)> = HashMap::new();
    let mut sig_ev = 0usize;
    let mut fesl_n = 0usize;
    for f in &funcs {
        // FESL (EA online backend) is unambiguous — one string ref is enough for networking.
        if f.body.contains("FESL") {
            hits.entry(f.addr).or_default().push(Hit {
                system: Some("networking".to_string()),
                name: None,
                role: Some("references FESL (EA online backend)".to_string()),
                confidence: Some("high".to_string()),
                evidence: Some("in-body FESL string reference".to_string()),
                source: "body-signal".to_string(),
                tier: 1,
            });
            fesl_n += 1;
        }
        if let Some((sys, top, second)) = body_signal(&f.body) {
            signals.insert(f.addr, (sys.clone(), top, second));
            if top >= 3 && top >= 2 * second.max(1) {
                hits.entry(f.addr).or_default().push(Hit {
                    system: Some(sys.clone()),
                    name: None,
                    role: Some(format!("binary signal: {} body tokens -> {}", top, sys)),
                    confidence: Some("med".to_string()),
                    evidence: Some(format!("{} in-body {} refs vs {} runner-up", top, sys, second)),
                    source: "body-signal".to_string(),
                    tier: 1,
                });
                sig_ev += 1;
            }
        }
    }
    eprintln!("body sig  : {} functions have a signal; {} decisive -> evidence; {} FESL->networking", signals.len(), sig_ev, fesl_n);

    // 2d. T2 reference scrape: the rest of docs/ + mods/ + extra dirs.
    let mut ref_dirs: Vec<PathBuf> = vec![root.join("docs"), root.join("mods")];
    ref_dirs.extend(args.extra_refs.iter().cloned());
    let already_t1: HashSet<u32> = hits
        .iter()
        .filter(|(_, hs)| hs.iter().any(|h| h.tier == 1))
        .map(|(a, _)| *a)
        .collect();
    let consumed_srcs: HashSet<String> = json_maps
        .iter()
        .map(|p| format!("docs/data/{}", p.file_name().unwrap().to_string_lossy()))
        .collect();
    let mut t2_new = 0usize;
    for d in &ref_dirs {
        if !d.exists() {
            continue;
        }
        for entry in WalkDir::new(d).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            let ext = p.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if !(ext == "md" || ext == "json" || ext == "txt") {
                continue;
            }
            let fname = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if fname.ends_with("_code_map.md") {
                continue;
            }
            let rel = p.strip_prefix(&root).unwrap_or(p).to_string_lossy().replace('\\', "/");
            if consumed_srcs.contains(&rel) {
                continue;
            }
            let text = match fs::read_to_string(p) {
                Ok(t) => t,
                Err(_) => continue,
            };
            for line in text.lines() {
                for (a, _is_fun) in scan_addrs(line) {
                    if !master_set.contains_key(&a) || already_t1.contains(&a) {
                        continue;
                    }
                    let e = hits.entry(a).or_default();
                    if e.iter().any(|h| h.tier == 2) {
                        continue;
                    }
                    e.push(Hit {
                        system: None,
                        name: None,
                        role: None,
                        confidence: None,
                        evidence: Some(format!("mentioned in {}", rel)),
                        source: rel.clone(),
                        tier: 2,
                    });
                    t2_new += 1;
                }
            }
        }
    }
    eprintln!("t2 refs   : {} new addresses merely referenced", t2_new);

    // 3. reduce to a primary attribution per address -------------------------
    let mut primary: HashMap<u32, Primary> = HashMap::new();
    for (a, hs) in &hits {
        let best = hs.iter().max_by_key(|h| hit_rank(h)).unwrap();
        primary.insert(
            *a,
            Primary {
                tier: best.tier,
                system: best.system.clone().unwrap_or_else(|| "referenced".to_string()),
                name: best.name.clone(),
                role: best.role.clone(),
                confidence: best.confidence.clone(),
                evidence: best.evidence.clone(),
                source: best.source.clone(),
                n_sources: hs.len(),
            },
        );
    }

    // 3-bis. caller-subsystem spread — for the UN-named cross-cutting utilities
    // (e.g. the tagged allocator). A function whose callers span many DISTINCT gameplay
    // subsystems is, by definition, a shared helper. Advisory only (not auto-assigned).
    let fn_sys: HashMap<u32, String> = primary
        .iter()
        .filter(|(_, p)| p.tier == 1)
        .map(|(a, p)| (*a, sanitize(canon_system(&p.system))))
        .collect();
    let mut spans: HashMap<u32, Vec<String>> = HashMap::new();
    for f in &funcs {
        let mut set: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in &f.caller_addrs {
            if let Some(sys) = fn_sys.get(c) {
                if !is_utility_module(sys) {
                    set.insert(sys.clone());
                }
            }
        }
        spans.insert(f.addr, set.into_iter().collect());
    }

    // 3-ter. classify the residue by address-locality + call-graph propagation.
    // Evidence-backed T1 functions are seeds; unlabeled functions inherit a *probable*
    // subsystem from their neighbours. Kept strictly separate from evidence (method field).
    let inferred = classify_residue(&funcs, &primary, &master_set);
    let inf_strong = inferred.values().filter(|i| i.method == "locality-strong").count();
    let inf_weak = inferred.values().filter(|i| i.method == "locality-weak").count();
    let inf_cg = inferred.values().filter(|i| i.method == "callgraph").count();
    eprintln!(
        "inferred  : {} functions ({} locality-strong, {} locality-weak, {} call-graph)",
        inferred.len(),
        inf_strong,
        inf_weak,
        inf_cg
    );

    // 4. emit ----------------------------------------------------------------
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("mkdir out");
    let uncl_dir = out_dir.join("_unclassified");
    fs::create_dir_all(&uncl_dir).expect("mkdir unclassified");

    let mut modules: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut uncl_shards: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let (mut t1, mut t2, mut t3) = (0usize, 0usize, 0usize);
    let mut per_system: BTreeMap<String, (usize, u64)> = BTreeMap::new();

    for (i, f) in funcs.iter().enumerate() {
        match primary.get(&f.addr) {
            Some(p) if p.tier == 1 => {
                t1 += 1;
                let m = sanitize(canon_system(&p.system));
                modules.entry(m.clone()).or_default().push(i);
                let e = per_system.entry(m).or_insert((0, 0));
                e.0 += 1;
                e.1 += f.size as u64;
            }
            Some(p) if p.tier == 2 => {
                t2 += 1;
                uncl_shards.entry(format!("seg_{:04x}", f.addr >> 16)).or_default().push(i);
            }
            _ => {
                t3 += 1;
                uncl_shards.entry(format!("seg_{:04x}", f.addr >> 16)).or_default().push(i);
            }
        }
    }

    // 4a. module source files
    for (m, idxs) in &modules {
        let mut w = BufWriter::new(fs::File::create(out_dir.join(format!("{}.c", m))).unwrap());
        let total: u64 = idxs.iter().map(|&i| funcs[i].size as u64).sum();
        writeln!(
            w,
            "/* ===== reassembled subsystem module: {m} =====\n * {} functions, {} bytes of code, sorted by address.\n * Generated by mercs2_reassemble from the Ghidra corpus + code maps.\n */\n",
            idxs.len(),
            total
        )
        .unwrap();
        let mut sorted = idxs.clone();
        sorted.sort_by_key(|&i| funcs[i].addr);
        for &i in &sorted {
            emit_fn(&mut w, &funcs[i], primary.get(&funcs[i].addr));
        }
    }

    // 4b. unclassified shards
    for (shard, idxs) in &uncl_shards {
        let mut w = BufWriter::new(fs::File::create(uncl_dir.join(format!("{}.c", shard))).unwrap());
        writeln!(
            w,
            "/* ===== UNCLASSIFIED region {shard} =====\n * {} functions with no subsystem attribution (tier 2 = referenced only, tier 3 = unmapped).\n * These are the manual-review surface. See REVIEW_QUEUE.md.\n */\n",
            idxs.len()
        )
        .unwrap();
        let mut sorted = idxs.clone();
        sorted.sort_by_key(|&i| funcs[i].addr);
        for &i in &sorted {
            emit_fn(&mut w, &funcs[i], primary.get(&funcs[i].addr));
        }
    }

    // 4c. off-image map addresses (attributed but not in the static image)
    let mut off_image: Vec<(u32, &Primary)> = primary
        .iter()
        .filter(|(a, _)| !master_set.contains_key(a))
        .map(|(a, p)| (*a, p))
        .collect();
    off_image.sort_by_key(|(a, _)| *a);

    write_manifest(&out_dir, &funcs, &primary, &off_image, &spans);
    write_review_queue(&out_dir, &funcs, &primary, &spans);
    write_coverage(&out_dir, funcs.len(), t1, t2, t3, &per_system, off_image.len(), modules.len());
    write_utility_report(&out_dir, &funcs, &primary, &per_system, &spans);
    write_classification(&out_dir, &funcs, &primary, &inferred, &signals);

    eprintln!(
        "\nDONE. tier1(identified)={} tier2(referenced)={} tier3(unmapped)={} off-image-map-addrs={}",
        t1,
        t2,
        t3,
        off_image.len()
    );
    eprintln!("wrote {} module(s) + {} unclassified shard(s)", modules.len(), uncl_shards.len());
    eprintln!("output: {}", out_dir.display());
}

/// Classify the un-attributed residue with two inferred signals:
///   1. address-locality — TU-contiguous layout means an unlabeled function usually shares
///      the subsystem of the evidence anchors bracketing it. Both sides agree => strong.
///   2. call-graph propagation — otherwise, inherit the majority subsystem of the function's
///      callees/callers that are already labeled (evidence or locality). Iterated to a fixpoint.
fn classify_residue(
    funcs: &[Func],
    primary: &HashMap<u32, Primary>,
    master_set: &HashMap<u32, usize>,
) -> HashMap<u32, Inferred> {
    let n = funcs.len();
    // evidence seed system per function (canonical)
    let ev_sys = |a: u32| -> Option<String> {
        primary.get(&a).filter(|p| p.tier == 1).map(|p| sanitize(canon_system(&p.system)))
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| funcs[i].addr);
    let seed: Vec<Option<String>> = order.iter().map(|&i| ev_sys(funcs[i].addr)).collect();

    // nearest previous / next evidence seed (system + its address)
    let mut prev: Vec<Option<(String, u32)>> = vec![None; n];
    let mut last: Option<(String, u32)> = None;
    for k in 0..n {
        if let Some(s) = &seed[k] {
            last = Some((s.clone(), funcs[order[k]].addr));
        }
        prev[k] = last.clone();
    }
    let mut next: Vec<Option<(String, u32)>> = vec![None; n];
    let mut nx: Option<(String, u32)> = None;
    for k in (0..n).rev() {
        if let Some(s) = &seed[k] {
            nx = Some((s.clone(), funcs[order[k]].addr));
        }
        next[k] = nx.clone();
    }

    const GAP_WEAK: u32 = 0x2000; // 8 KB — a plausible same-region distance
    const GAP_STRONG: u32 = 0x8000; // both-sides-agree tolerates a larger run
    let mut inferred: HashMap<u32, Inferred> = HashMap::new();
    for k in 0..n {
        if seed[k].is_some() || is_artifact(&funcs[order[k]].body) {
            continue; // artifacts are not real functions — never infer a subsystem for them
        }
        let a = funcs[order[k]].addr;
        match (&prev[k], &next[k]) {
            (Some((ps, pa)), Some((ns, na))) => {
                let (dp, dn) = (a.wrapping_sub(*pa), na.wrapping_sub(a));
                if ps == ns {
                    if dp.min(dn) <= GAP_STRONG {
                        inferred.insert(a, Inferred {
                            system: ps.clone(),
                            method: "locality-strong",
                            note: format!("bracketed by {} on both sides (±{},{} B)", ps, dp, dn),
                        });
                    }
                } else {
                    // pick the closer anchor if within the weak gap
                    let (sys, d, other) = if dp <= dn { (ps, dp, ns) } else { (ns, dn, ps) };
                    if d <= GAP_WEAK {
                        inferred.insert(a, Inferred {
                            system: sys.clone(),
                            method: "locality-weak",
                            note: format!("nearest anchor {} ({} B); other side {}", sys, d, other),
                        });
                    }
                }
            }
            (Some((ps, pa)), None) => {
                let d = a.wrapping_sub(*pa);
                if d <= GAP_WEAK {
                    inferred.insert(a, Inferred {
                        system: ps.clone(),
                        method: "locality-weak",
                        note: format!("trails {} anchor by {} B (no next anchor)", ps, d),
                    });
                }
            }
            (None, Some((ns, na))) => {
                let d = na.wrapping_sub(a);
                if d <= GAP_WEAK {
                    inferred.insert(a, Inferred {
                        system: ns.clone(),
                        method: "locality-weak",
                        note: format!("precedes {} anchor by {} B (no prev anchor)", ns, d),
                    });
                }
            }
            (None, None) => {}
        }
    }

    // --- call-graph propagation for whatever locality left unlabeled ---
    // build complete callee edges by scanning bodies (header caller lists are capped at 12).
    let mut callees: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, f) in funcs.iter().enumerate() {
        for (a, is_fun) in scan_addrs(&f.body) {
            if is_fun && a != f.addr && master_set.contains_key(&a) {
                callees[i].push(a);
            }
        }
    }
    // callers = inverse
    let mut callers: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, cs) in callees.iter().enumerate() {
        for &c in cs {
            if let Some(&j) = master_set.get(&c) {
                callers[j].push(funcs[i].addr);
            }
        }
    }
    let sys_of = |a: u32, inf: &HashMap<u32, Inferred>| -> Option<String> {
        ev_sys(a).or_else(|| inf.get(&a).map(|x| x.system.clone()))
    };
    for _pass in 0..4 {
        let mut adds: Vec<(u32, String, usize)> = Vec::new();
        for (i, f) in funcs.iter().enumerate() {
            if ev_sys(f.addr).is_some() || inferred.contains_key(&f.addr) || is_artifact(&f.body) {
                continue;
            }
            let mut votes: HashMap<String, usize> = HashMap::new();
            for &c in callees[i].iter().chain(callers[i].iter()) {
                if let Some(s) = sys_of(c, &inferred) {
                    if !is_utility_module(&s) {
                        *votes.entry(s).or_insert(0) += 1;
                    }
                }
            }
            if let Some((s, v)) = votes.into_iter().max_by_key(|(_, v)| *v) {
                if v >= 2 {
                    adds.push((f.addr, s, v));
                }
            }
        }
        if adds.is_empty() {
            break;
        }
        for (a, s, v) in adds {
            inferred.insert(a, Inferred {
                system: s.clone(),
                method: "callgraph",
                note: format!("{}/neighbour majority = {}", v, s),
            });
        }
    }

    // SecuROM/unpacked-region fallback: functions above the normal image (>= 0x01000000) with no
    // other signal are the SecuROM DRM/VM/crypto layer, not a game subsystem. Honest region label.
    for f in funcs {
        if f.addr >= 0x0100_0000
            && ev_sys(f.addr).is_none()
            && !inferred.contains_key(&f.addr)
            && !is_artifact(&f.body)
        {
            inferred.insert(f.addr, Inferred {
                system: "securom_drm".to_string(),
                method: "securom-region",
                note: "in the SecuROM-unpacked image region (>=0x01000000), no game-subsystem signal".to_string(),
            });
        }
    }
    inferred
}

fn emit_fn<W: Write>(w: &mut W, f: &Func, p: Option<&Primary>) {
    writeln!(w, "/* ------------------------------------------------------------").unwrap();
    write!(w, " * {} @{}  size={}  callers={}", f.name, norm(f.addr), f.size, f.caller_count).unwrap();
    match p {
        Some(p) => {
            let canon = canon_system(&p.system);
            let sys_disp = if canon == p.system {
                p.system.clone()
            } else {
                format!("{} (raw: {})", canon, p.system)
            };
            write!(
                w,
                "\n * system: {}   tier: T{}   confidence: {}   [{} source(s)]\n * source: {}",
                sys_disp,
                p.tier,
                p.confidence.as_deref().unwrap_or("-"),
                p.n_sources,
                p.source
            )
            .unwrap();
            if let Some(r) = &p.role {
                write!(w, "\n * role: {}", r).unwrap();
            }
            if let Some(e) = &p.evidence {
                write!(w, "\n * evidence: {}", e).unwrap();
            }
        }
        None => {
            write!(w, "\n * system: UNMAPPED   tier: T3").unwrap();
        }
    }
    writeln!(w, "\n * ------------------------------------------------------------ */").unwrap();
    writeln!(w, "{}", f.body.trim_end()).unwrap();
    writeln!(w).unwrap();
}

fn write_manifest(
    out_dir: &Path,
    funcs: &[Func],
    primary: &HashMap<u32, Primary>,
    off_image: &[(u32, &Primary)],
    spans: &HashMap<u32, Vec<String>>,
) {
    let span_of = |a: u32| spans.get(&a).map(|v| v.len()).unwrap_or(0);
    let mut w = BufWriter::new(fs::File::create(out_dir.join("MANIFEST.csv")).unwrap());
    writeln!(w, "addr,name,recovered_name,system,raw_system,tier,confidence,size,callers,caller_span,in_image,n_sources,source").unwrap();
    let mut idxs: Vec<usize> = (0..funcs.len()).collect();
    idxs.sort_by_key(|&i| funcs[i].addr);
    for &i in &idxs {
        let f = &funcs[i];
        let rec = best_name(f, primary);
        match primary.get(&f.addr) {
            Some(p) => writeln!(
                w,
                "{},{},{},{},{},T{},{},{},{},{},1,{},{}",
                norm(f.addr),
                csv(&f.name),
                csv(rec),
                sanitize(canon_system(&p.system)),
                sanitize(&p.system),
                p.tier,
                p.confidence.as_deref().unwrap_or(""),
                f.size,
                f.caller_count,
                span_of(f.addr),
                p.n_sources,
                p.source
            )
            .unwrap(),
            None => writeln!(
                w,
                "{},{},{},UNMAPPED,UNMAPPED,T3,,{},{},{},1,0,",
                norm(f.addr),
                csv(&f.name),
                csv(rec),
                f.size,
                f.caller_count,
                span_of(f.addr)
            )
            .unwrap(),
        }
    }
    for (a, p) in off_image {
        writeln!(
            w,
            "{},{},{},{},T{},{},,,0,0,{},{}",
            norm(*a),
            csv(p.name.as_deref().unwrap_or("")),
            sanitize(canon_system(&p.system)),
            sanitize(&p.system),
            p.tier,
            p.confidence.as_deref().unwrap_or(""),
            p.n_sources,
            p.source
        )
        .unwrap();
    }

    // JSON (one record per line inside an array)
    let mut jw = BufWriter::new(fs::File::create(out_dir.join("MANIFEST.json")).unwrap());
    writeln!(jw, "[").unwrap();
    let mut recs: Vec<Value> = Vec::with_capacity(funcs.len() + off_image.len());
    for &i in &idxs {
        let f = &funcs[i];
        recs.push(match primary.get(&f.addr) {
            Some(p) => serde_json::json!({
                "addr": norm(f.addr), "name": f.name, "recovered_name": best_name(f, primary),
                "system": sanitize(canon_system(&p.system)), "raw_system": sanitize(&p.system),
                "tier": p.tier, "confidence": p.confidence, "role": p.role,
                "evidence": p.evidence, "size": f.size, "callers": f.caller_count,
                "caller_span": span_of(f.addr), "in_image": true,
                "n_sources": p.n_sources, "source": p.source,
            }),
            None => serde_json::json!({
                "addr": norm(f.addr), "name": f.name, "system": "UNMAPPED",
                "tier": 3, "size": f.size, "callers": f.caller_count,
                "caller_span": span_of(f.addr), "in_image": true,
            }),
        });
    }
    for (a, p) in off_image {
        recs.push(serde_json::json!({
            "addr": norm(*a), "name": p.name,
            "system": sanitize(canon_system(&p.system)), "raw_system": sanitize(&p.system), "tier": p.tier,
            "confidence": p.confidence, "role": p.role, "evidence": p.evidence,
            "in_image": false, "n_sources": p.n_sources, "source": p.source,
        }));
    }
    let last = recs.len().saturating_sub(1);
    for (n, r) in recs.iter().enumerate() {
        let comma = if n < last { "," } else { "" };
        writeln!(jw, "  {}{}", serde_json::to_string(r).unwrap(), comma).unwrap();
    }
    writeln!(jw, "]").unwrap();
}

fn write_review_queue(
    out_dir: &Path,
    funcs: &[Func],
    primary: &HashMap<u32, Primary>,
    spans: &HashMap<u32, Vec<String>>,
) {
    let span_of = |a: u32| spans.get(&a).map(|v| v.len()).unwrap_or(0);
    // non-T1 in-image functions, ranked by size*(1+callers)
    let mut rows: Vec<(&Func, u8, u64)> = Vec::new();
    for f in funcs {
        let tier = primary.get(&f.addr).map(|p| p.tier).unwrap_or(3);
        if tier == 1 {
            continue;
        }
        let score = f.size as u64 * (1 + f.caller_count as u64);
        rows.push((f, tier, score));
    }
    rows.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
    let t3: Vec<&(&Func, u8, u64)> = rows.iter().filter(|r| r.1 == 3).collect();
    let t2: Vec<&(&Func, u8, u64)> = rows.iter().filter(|r| r.1 == 2).collect();

    let mut w = BufWriter::new(fs::File::create(out_dir.join("REVIEW_QUEUE.md")).unwrap());
    writeln!(w, "# Manual-review queue — unmapped engine functions\n").unwrap();
    writeln!(
        w,
        "The functions we have **not** attributed to a subsystem, ranked by `size x (1+callers)` \
         (bigger + more-called = higher payoff to identify). Tier 3 = no mention anywhere; \
         Tier 2 = mentioned in the corpus but never assigned a subsystem.\n"
    )
    .unwrap();
    writeln!(w, "- **{}** functions still need identification (T2+T3)", rows.len()).unwrap();
    writeln!(w, "  - T3 (nothing known): **{}**", t3.len()).unwrap();
    writeln!(w, "  - T2 (referenced only): **{}**", t2.len()).unwrap();
    writeln!(w, "\nFull machine-readable list: `REVIEW_QUEUE.csv`.\n").unwrap();

    let dump = |w: &mut BufWriter<fs::File>, title: &str, rows: &[&(&Func, u8, u64)], lim: usize| {
        writeln!(w, "\n## {} — top {}\n", title, lim.min(rows.len())).unwrap();
        writeln!(w, "| # | addr | name | size | callers | score |").unwrap();
        writeln!(w, "|---|------|------|------|---------|-------|").unwrap();
        for (n, r) in rows.iter().take(lim).enumerate() {
            writeln!(
                w,
                "| {} | {} | {} | {} | {} | {} |",
                n + 1,
                norm(r.0.addr),
                r.0.name,
                r.0.size,
                r.0.caller_count,
                r.2
            )
            .unwrap();
        }
    };
    dump(&mut w, "Tier 3 (fully unmapped)", &t3, 200);
    dump(&mut w, "Tier 2 (referenced, unattributed)", &t2, 100);

    let mut c = BufWriter::new(fs::File::create(out_dir.join("REVIEW_QUEUE.csv")).unwrap());
    writeln!(c, "rank,addr,name,tier,size,callers,caller_span,score").unwrap();
    for (n, r) in rows.iter().enumerate() {
        writeln!(
            c,
            "{},{},{},T{},{},{},{},{}",
            n + 1,
            norm(r.0.addr),
            csv(&r.0.name),
            r.1,
            r.0.size,
            r.0.caller_count,
            span_of(r.0.addr),
            r.2
        )
        .unwrap();
    }
}

/// Emit the inferred (probable, non-evidence) classification: a separate `inferred/<system>.c`
/// tree (never mixed with evidence modules) + CLASSIFICATION.md/.csv + final coverage.
fn write_classification(
    out_dir: &Path,
    funcs: &[Func],
    primary: &HashMap<u32, Primary>,
    inferred: &HashMap<u32, Inferred>,
    signals: &HashMap<u32, (String, u32, u32)>,
) {
    let inf_dir = out_dir.join("inferred");
    fs::create_dir_all(&inf_dir).unwrap();
    // any tier-1 primary yields a system; verified sources are "evidence", the binary-token
    // vote is the weaker "signal" tier (binary-derived but heuristic — 28% conflict w/ evidence).
    let ev_sys = |a: u32| -> Option<String> {
        primary.get(&a).filter(|p| p.tier == 1).map(|p| sanitize(canon_system(&p.system)))
    };
    let is_signal = |a: u32| -> bool {
        primary.get(&a).map(|p| p.source == "body-signal").unwrap_or(false)
    };

    // group inferred funcs by system
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, f) in funcs.iter().enumerate() {
        if let Some(inf) = inferred.get(&f.addr) {
            groups.entry(inf.system.clone()).or_default().push(i);
        }
    }
    for (sys, idxs) in &groups {
        let mut w = BufWriter::new(fs::File::create(inf_dir.join(format!("{}.c", sys))).unwrap());
        writeln!(
            w,
            "/* ===== INFERRED module: {sys} =====\n * {} functions assigned by PROBABILITY \
             (address-locality / call-graph), NOT evidence. Verify before trusting.\n */\n",
            idxs.len()
        )
        .unwrap();
        let mut sorted = idxs.clone();
        sorted.sort_by_key(|&i| funcs[i].addr);
        for &i in &sorted {
            let f = &funcs[i];
            let inf = &inferred[&f.addr];
            writeln!(w, "/* ------------------------------------------------------------").unwrap();
            writeln!(
                w,
                " * {} @{}  size={}  callers={}\n * INFERRED system: {}   method: {}\n * basis: {}\n * ------------------------------------------------------------ */",
                f.name, norm(f.addr), f.size, f.caller_count, inf.system, inf.method, inf.note
            )
            .unwrap();
            writeln!(w, "{}\n", f.body.trim_end()).unwrap();
        }
    }

    // CLASSIFICATION.csv
    let mut c = BufWriter::new(fs::File::create(out_dir.join("CLASSIFICATION.csv")).unwrap());
    writeln!(c, "addr,name,status,system,method,signal,corrob,size,callers").unwrap();
    let mut idxs: Vec<usize> = (0..funcs.len()).collect();
    idxs.sort_by_key(|&i| funcs[i].addr);
    let (mut n_ev, mut n_sig, mut n_inf, mut n_none, mut n_art) = (0usize, 0usize, 0usize, 0usize, 0usize);
    let mut final_sys: BTreeMap<String, usize> = BTreeMap::new();
    // corroboration tallies: independent body signal vs the assigned label
    let (mut inf_agree, mut inf_conflict) = (0usize, 0usize);
    let (mut ev_agree, mut ev_conflict) = (0usize, 0usize);
    for &i in &idxs {
        let f = &funcs[i];
        let (status, sys, method) = if let Some(s) = ev_sys(f.addr) {
            *final_sys.entry(s.clone()).or_insert(0) += 1;
            if is_signal(f.addr) {
                n_sig += 1;
                ("signal", s, "body-signal")
            } else {
                n_ev += 1;
                ("evidence", s, "evidence")
            }
        } else if let Some(inf) = inferred.get(&f.addr) {
            n_inf += 1;
            *final_sys.entry(inf.system.clone()).or_insert(0) += 1;
            ("inferred", inf.system.clone(), inf.method)
        } else if is_artifact(&f.body) {
            n_art += 1;
            ("artifact", "ARTIFACT".to_string(), "-")
        } else {
            n_none += 1;
            ("unclassified", "UNCLASSIFIED".to_string(), "-")
        };
        let sig = signals.get(&f.addr).map(|(s, _, _)| s.clone());
        // "signal"-tier labels ARE the body signal, so the check isn't independent.
        let corrob = if status == "signal" {
            "self"
        } else {
            match &sig {
                Some(s) if *s == sys => {
                    match status {
                        "inferred" => inf_agree += 1,
                        "evidence" => ev_agree += 1,
                        _ => {}
                    }
                    "corroborated"
                }
                Some(s) if !s.is_empty() && sys != "UNCLASSIFIED" => {
                    match status {
                        "inferred" => inf_conflict += 1,
                        "evidence" => ev_conflict += 1,
                        _ => {}
                    }
                    "conflict"
                }
                _ => "-",
            }
        };
        writeln!(
            c,
            "{},{},{},{},{},{},{},{},{}",
            norm(f.addr),
            csv(&f.name),
            status,
            sys,
            method,
            sig.as_deref().unwrap_or(""),
            corrob,
            f.size,
            f.caller_count
        )
        .unwrap();
    }

    // CLASSIFICATION.md
    let total = funcs.len();
    let pct = |x: usize| 100.0 * x as f64 / total as f64;
    let mut w = BufWriter::new(fs::File::create(out_dir.join("CLASSIFICATION.md")).unwrap());
    writeln!(w, "# Full-corpus classification ({} functions)\n", total).unwrap();
    writeln!(w, "| Status | Meaning | Strength | Count | % |").unwrap();
    writeln!(w, "|--------|---------|----------|-------|---|").unwrap();
    writeln!(w, "| evidence | code-map / annotation / recovered symbol | verified | **{}** | {:.1}% |", n_ev, pct(n_ev)).unwrap();
    writeln!(w, "| signal | decisive in-body string/import/class token | binary-derived heuristic | **{}** | {:.1}% |", n_sig, pct(n_sig)).unwrap();
    writeln!(w, "| inferred | address-locality / call-graph / securom-region | probabilistic | **{}** | {:.1}% |", n_inf, pct(n_inf)).unwrap();
    writeln!(w, "| artifact | Ghidra bad-disassembly (not real code) | excluded | {} | {:.1}% |", n_art, pct(n_art)).unwrap();
    writeln!(w, "| unclassified | real code, no signal (deeper-RE core) | none | {} | {:.1}% |", n_none, pct(n_none)).unwrap();
    writeln!(
        w,
        "\n**{:.1}% of the corpus now carries a subsystem label.** Of that, {} are verified evidence \
         and {} are binary-derived signal ({:.1}% combined at binary-or-better strength); {} more are \
         probabilistic inference. {} are Ghidra disassembly artifacts (excluded); only **{} real functions \
         ({:.1}%)** remain the genuine unclassified core needing deeper/dynamic RE.\n",
        pct(n_ev + n_sig + n_inf),
        n_ev,
        n_sig,
        pct(n_ev + n_sig),
        n_inf,
        n_art,
        n_none,
        pct(n_none)
    )
    .unwrap();
    // corroboration: independent binary signal vs the assigned label
    let inf_checked = inf_agree + inf_conflict;
    let ev_checked = ev_agree + ev_conflict;
    writeln!(w, "## Corroboration — independent body signal vs assigned label\n").unwrap();
    writeln!(
        w,
        "An independent binary signal (in-body string-labels / imports / class-refs) was available \
         for **{}** functions. Where it overlaps an assigned label:\n",
        signals.len()
    )
    .unwrap();
    writeln!(w, "| Assigned by | Checked by signal | Agree | Conflict | Agreement |").unwrap();
    writeln!(w, "|-------------|-------------------|-------|----------|-----------|").unwrap();
    let pctf = |a: usize, t: usize| if t == 0 { 0.0 } else { 100.0 * a as f64 / t as f64 };
    writeln!(
        w,
        "| evidence | {} | {} | {} | {:.0}% |",
        ev_checked, ev_agree, ev_conflict, pctf(ev_agree, ev_checked)
    )
    .unwrap();
    writeln!(
        w,
        "| inferred | {} | {} | {} | {:.0}% |",
        inf_checked, inf_agree, inf_conflict, pctf(inf_agree, inf_checked)
    )
    .unwrap();
    writeln!(
        w,
        "\n- **{} inferred labels are independently corroborated** by a body signal — these are \
         the strongest promotion candidates (`corrob=corroborated` in CLASSIFICATION.csv).\n- \
         {} inferred labels **conflict** with their body signal — review these first (the \
         signal may be the truth).\n- The evidence agreement rate ({:.0}%) is a sanity check on \
         the signal-rule table itself.\n",
        inf_agree, inf_conflict, pctf(ev_agree, ev_checked)
    )
    .unwrap();

    writeln!(w, "## Final per-subsystem totals (evidence + inferred)\n").unwrap();
    writeln!(w, "| Subsystem | Functions |").unwrap();
    writeln!(w, "|-----------|-----------|").unwrap();
    let mut rows: Vec<_> = final_sys.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (s, cnt) in rows {
        writeln!(w, "| `{}` | {} |", s, cnt).unwrap();
    }
    writeln!(w, "\n> Inferred labels are probabilistic. See `inferred/<system>.c` for the basis of each, and CLASSIFICATION.csv (`method` column) to filter by confidence.").unwrap();
}

/// Utility/library review report — answers "how much of the un-attributed code is shared
/// utility?" It buckets the symbol-classified library modules and lists the un-named
/// functions whose callers span many subsystems (the shared helpers still to be named).
fn write_utility_report(
    out_dir: &Path,
    funcs: &[Func],
    primary: &HashMap<u32, Primary>,
    per_system: &BTreeMap<String, (usize, u64)>,
    spans: &HashMap<u32, Vec<String>>,
) {
    let mut w = BufWriter::new(fs::File::create(out_dir.join("UTILITY_REPORT.md")).unwrap());
    writeln!(w, "# Utility / shared-library review\n").unwrap();

    // 1. symbol-classified library modules
    let util_total: usize = UTILITY_MODULES.iter().filter_map(|m| per_system.get(*m)).map(|(c, _)| *c).sum();
    writeln!(
        w,
        "**{} functions** are shared library/utility code identified by recovered symbol name \
         (not subsystem-specific gameplay). Confirms the hypothesis: a large slice of the \
         un-attributed residue is CRT / C++ runtime / middleware, not engine logic.\n",
        util_total
    )
    .unwrap();
    writeln!(w, "| Utility module | Functions | Code bytes | What it is |").unwrap();
    writeln!(w, "|----------------|-----------|-----------|------------|").unwrap();
    let desc = |m: &str| match m {
        "crt" => "MSVC C runtime — string/mem/stdio/math/startup",
        "cpp_runtime" => "C++ exception-handling / RTTI / std runtime",
        "havok" => "Havok middleware (hk*) — mostly named in body comments only",
        "scaleform_gfx" => "Scaleform GFx base library (G* classes)",
        "utility_shared" => "cross-cutting helpers (caller-span heuristic)",
        _ => "",
    };
    for m in UTILITY_MODULES {
        if let Some((c, sz)) = per_system.get(*m) {
            writeln!(w, "| `{}` | {} | {} | {} |", m, c, sz, desc(m)).unwrap();
        }
    }

    // 2. un-named cross-cutting helpers, ranked by caller-subsystem span
    writeln!(
        w,
        "\n## Un-named shared helpers (caller-span heuristic)\n\nUn-named (`FUN_`) functions whose \
         callers span **>=3 distinct gameplay subsystems** — the shared helpers (allocators, \
         containers, math) not yet symbol-recovered. These are the highest-value manual-naming \
         targets: naming one explains many call sites.\n"
    )
    .unwrap();
    let mut cand: Vec<(&Func, usize, &Vec<String>)> = Vec::new();
    for f in funcs {
        let sys = primary.get(&f.addr).map(|p| sanitize(canon_system(&p.system)));
        let is_util_already = sys.as_deref().map(is_utility_module).unwrap_or(false);
        if !is_symbolic(&f.name) && !is_util_already {
            if let Some(sp) = spans.get(&f.addr) {
                if sp.len() >= 3 {
                    cand.push((f, sp.len(), sp));
                }
            }
        }
    }
    cand.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.caller_count.cmp(&a.0.caller_count)));
    writeln!(w, "| # | addr | size | callers | span | subsystems calling it |").unwrap();
    writeln!(w, "|---|------|------|---------|------|------------------------|").unwrap();
    for (n, (f, span, syslist)) in cand.iter().take(80).enumerate() {
        writeln!(
            w,
            "| {} | {} | {} | {} | {} | {} |",
            n + 1,
            norm(f.addr),
            f.size,
            f.caller_count,
            span,
            syslist.join(", ")
        )
        .unwrap();
    }
    writeln!(w, "\n_{} un-named helpers with span>=3 total._", cand.len()).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn write_coverage(
    out_dir: &Path,
    total: usize,
    t1: usize,
    t2: usize,
    t3: usize,
    per_system: &BTreeMap<String, (usize, u64)>,
    off_image: usize,
    n_modules: usize,
) {
    let mut w = BufWriter::new(fs::File::create(out_dir.join("COVERAGE.md")).unwrap());
    let pct = |n: usize| 100.0 * n as f64 / total as f64;
    writeln!(w, "# Reassembly coverage\n").unwrap();
    writeln!(w, "Master image: **{} functions**.\n", total).unwrap();
    writeln!(w, "| Tier | Meaning | Count | % |").unwrap();
    writeln!(w, "|------|---------|-------|---|").unwrap();
    writeln!(w, "| T1 | identified — assigned to a subsystem module | **{}** | {:.1}% |", t1, pct(t1)).unwrap();
    writeln!(w, "| T2 | referenced only — mentioned, not attributed | {} | {:.1}% |", t2, pct(t2)).unwrap();
    writeln!(w, "| T3 | unmapped — nothing known | {} | {:.1}% |", t3, pct(t3)).unwrap();
    writeln!(
        w,
        "\n**Headline:** {} functions reassembled into {} subsystem modules; \
         **{} still unmapped** for manual review (down from {}).\n",
        t1,
        n_modules,
        t2 + t3,
        total
    )
    .unwrap();
    writeln!(
        w,
        "Off-image map addresses (attributed but not in the static image — SecuROM/.securom thunks etc.): {}\n",
        off_image
    )
    .unwrap();
    writeln!(w, "## Per-subsystem module coverage\n").unwrap();
    writeln!(w, "| Subsystem module | Functions | Code bytes |").unwrap();
    writeln!(w, "|------------------|-----------|-----------|").unwrap();
    let mut rows: Vec<_> = per_system.iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    for (m, (c, sz)) in rows {
        writeln!(w, "| `{}` | {} | {} |", m, c, sz).unwrap();
    }
}