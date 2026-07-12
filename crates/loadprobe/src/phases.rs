//! Static tables: the world-load milestone ladder, high-signal Lua prefixes, and the
//! known-crash-EIP map. Single source of truth — extend here as new markers appear.

/// One milestone phase. `matches` are substrings; a phase is "reached" if ANY of its
/// substrings appears in a line's message.
pub struct Phase {
    pub idx: usize,
    pub name: &'static str,
    pub matches: &'static [&'static str],
}

/// The ordered load → world → gameplay ladder. Order matches the observed chronology
/// of a deep load (`storage/pmc_blackbox-vanilla-boot-into-game.log`): the GlobalEnter
/// entity-construction burst (player/WAITFORGAME/act-staging) fires before mission-flow
/// + the WAITFORSTREAMING layer cycles, which precede the portal enables and GlobalExit.
pub static LADDER: &[Phase] = &[
    Phase { idx: 0,  name: "Process init",            matches: &["PMC Blackbox v3"] },
    Phase { idx: 1,  name: "Pool/hooks armed",        matches: &["render-instance pool initialized"] },
    Phase { idx: 2,  name: "Shell sound init",        matches: &["SoundShellBootstrap.Init"] },
    Phase { idx: 3,  name: "Shell init",              matches: &["Top of ShellBootstrap::Init()"] },
    Phase { idx: 4,  name: "Intro movies",            matches: &["Attempting to play movie", "Playing EA", "Playing Pandemic"] },
    Phase { idx: 5,  name: "Movies complete",         matches: &["All movies complete"] },
    Phase { idx: 6,  name: "Precache",                matches: &["StartPrecache()"] },
    Phase { idx: 7,  name: "Soundbanks ready",        matches: &["Shell music started"] },
    Phase { idx: 8,  name: "Shell exit",              matches: &["Shell exited"] },
    Phase { idx: 9,  name: "Game bootstrap",          matches: &["GameBootstrap - bailing because finished shell"] },
    Phase { idx: 10, name: "WORLD LOAD START",        matches: &["Loading vz level with vz masterscript"] },
    Phase { idx: 11, name: "Player spawn",            matches: &["CreatePlayerCharacter"] },
    Phase { idx: 12, name: "WAITFORGAME",             matches: &["STATE_WAITFORGAME (refcount="] },
    Phase { idx: 13, name: "GlobalEnter begin",       matches: &["GlobalEnter - Begin"] },
    Phase { idx: 14, name: "Act staging",             matches: &["Staging Act"] },
    Phase { idx: 15, name: "Mission flow data",       matches: &["Setting flow data ("] },
    Phase { idx: 16, name: "Streaming (WAITFORSTREAMING)", matches: &["STATE_WAITFORSTREAMING (refcount="] },
    Phase { idx: 17, name: "GlobalEnter complete",    matches: &["GlobalEnter - Complete"] },
    Phase { idx: 18, name: "World entities online",   matches: &["Enabling "] }, // + "portal", checked in report
    Phase { idx: 19, name: "Module/job imports",      matches: &["Dynamically imported module"] },
    Phase { idx: 20, name: "World fully loaded (GlobalExit)", matches: &["GlobalExit - Complete"] },
];

/// Phase index that marks the load fully completing into gameplay (GlobalExit complete).
pub const REACHED_WORLD_IDX: usize = 20;
/// Softer bar: reached world-entity construction (player created), even if it didn't finish.
pub const ENTERED_WORLD_IDX: usize = 11;

// The high-signal Lua prefixes ("###!,###,!!!,##@,@@@,***,=-=") and the routine
// sources ("lua,pool") are the CLI defaults (see main.rs `--signals` / `--routine`).

/// Debug instrumentation we added to pmc_bb.dll — the high-value diagnostic sources.
pub static INSTRUMENTATION: &[&str] = &["crash", "mtrl", "stall", "cc", "prmg-bw", "prmg-key", "prmg-key2", "seg"];

/// Init-noise sources (collapsed to a count + their ARMED/summary line).
pub static INIT_NOISE: &[&str] = &["blackbox", "compat", "lualog"];

/// Every source tag we know about. A source NOT in this set is surfaced as
/// "UNKNOWN SOURCE" — likely new instrumentation we haven't taught loadprobe about.
pub static KNOWN_SOURCES: &[&str] = &[
    "lua", "world", "pool", "blackbox", "compat", "lualog",
    "crash", "mtrl", "cc", "stall", "seg", "prmg", "prmg-bw", "prmg-key", "prmg-key2", "heap",
    "raw",
];

pub fn is_known_source(s: &str) -> bool {
    KNOWN_SOURCES.contains(&s)
}

/// Known crash EIP → suspected-subsystem label (ties to the memory notes).
/// `teardown` marks an EIP that is a process hard-close / teardown ARTIFACT, not a
/// spontaneous bug — the user force-killed the game and a worker faulted on the way out.
pub struct KnownEip {
    pub eip: u32,
    pub label: &'static str,
    pub teardown: bool,
}

pub static KNOWN_EIPS: &[KnownEip] = &[
    KnownEip { eip: 0x0061981F, label: "MTRL multi-material array overrun (FIXED 2026-06-16)", teardown: false },
    // 0x874E7D: the texture-streaming worker faulting on PROCESS TEARDOWN (hard-close).
    // EDI points at the english.wad path string mid-read; AV target=F011157A is the
    // texture sentinel. Confirmed by the user to be a manual hard-close, NOT a load bug.
    KnownEip { eip: 0x00874E7D, label: "texture-streaming worker fault on process teardown (HARD-CLOSE artifact, not a spontaneous bug)", teardown: true },
    KnownEip { eip: 0x0047AA5C, label: "PRMG null render-handle (record+4=0)", teardown: false },
    KnownEip { eip: 0x0047A7D8, label: "PRMG twin pass / binding-array", teardown: false },
    KnownEip { eip: 0x0084DD5B, label: "MTRL texture-handle overrun", teardown: false },
    KnownEip { eip: 0x004CC064, label: "render/texture-component pool NULL-fallback", teardown: false },
    KnownEip { eip: 0x007E045E, label: "ECS texture-component type-confusion", teardown: false },
    // 0x0085C8D0: texture-bind / null-surface fault in the wardrobe-preview (menu-open) path.
    // Neighbor of the 0x750BD9 null-DXT1 site and the 0x84DD5B tex-handle-overrun; reached via
    // model-instantiation frames 0x4A483B/0x479775/0x4796A9/0x471A83 → 0x84DDCB. AV READ target=0
    // with EAX = a texture handle/hash being looked up and not found. NOT a teardown artifact.
    KnownEip { eip: 0x0085C8D0, label: "texture-bind/null-surface fault on wardrobe preview (menu-open)", teardown: false },
    // 0x00478E43: mesh-geometry handler (cluster of 0x00478F2A/0x004719C0). AV READ with ECX/ESI =
    // "CSUM" (0x4D555343) — the per-piece geometry read of a DESTRUCTIBLE model runs off the end of
    // the replaced geometry into the container's CSUM trailer. Seen when a custom mesh is injected
    // into a destructible donor (SEGM/SWIT/STAT/CHDR/CEXE preserved) whose piece partitions no longer
    // match; fires at model instantiation (e.g. streaming a PMC building). Fix: use a non-destructible
    // donor or strip the destruction chunks. NOT a teardown artifact.
    KnownEip { eip: 0x00478E43, label: "mesh-geometry handler: destructible piece-geometry read off end (injected mesh in destructible donor; SEGM mismatch)", teardown: false },
    // 0x00858DB8: inside Mtrl_Parse (FUN_00858790 +0x628). The parser reads each material as a FIXED
    // record (104-byte preamble then a 128-byte record: u16 flags, u16 tex_count, tex_count u32 hashes,
    // props, trailing shader ref) and ends with a SHADER-POOL lookup `[*shader_pool[slot] + 8]`. AV
    // READ [null+8] = the pool entry is null. Root cause seen: a from-scratch MTRL record that is NOT
    // exactly 128 bytes → the parser over-reads into the next chunk → garbage shader hash → pool miss →
    // null. Fix: emit the full 128-byte material record (104-byte preamble + 128). Same site the DLC
    // mattias_v5 skin crashed on (MTRL layout). NOT teardown.
    KnownEip { eip: 0x00858DB8, label: "Mtrl_Parse (FUN_00858790+0x628) shader-pool lookup: MTRL record wrong size (must be 128B) -> parser over-reads -> garbage shader hash -> null pool entry", teardown: false },
    // 0x750BD9: texture BODY null-reader deref (the "null DXT1" crash). The streaming
    // upload loop at 0x750BA2 reads a texture's BODY chunk and dereferences the result at
    // 0x750BD9 (EDX="DXT1"=0x31545844) without a NULL check -> AV read target=0 when the
    // body is null/empty. Causes: a texture whose resident BODY is empty (pixels are in the
    // streaming tiers), or a base texture whose block was overridden by a patch WAD that
    // ships an empty-bodied replacement (e.g. booting `vz` with the DLC level-replacement
    // patch mounted). patch_anim_table.py guard #3 (@0x750B90) is the runtime NULL-guard.
    KnownEip { eip: 0x00750BD9, label: "texture BODY null-reader deref (null-DXT1): streaming upload derefs a null/empty texture body (patch override or streamed-empty body)", teardown: false },
];

pub fn eip_label(eip: u32) -> Option<&'static str> {
    KNOWN_EIPS.iter().find(|k| k.eip == eip).map(|k| k.label)
}

/// True if this EIP is a known process-teardown / hard-close artifact (not a real crash).
pub fn is_teardown_eip(eip: u32) -> bool {
    KNOWN_EIPS.iter().any(|k| k.eip == eip && k.teardown)
}

/// Faction prefixes used by job/contract module names (for the progression section).
pub static FACTION_PREFIXES: &[&str] = &["Chi", "Oil", "Gur", "All", "Pir", "Pmc"];

/// Is `module` a faction job/contract module name, e.g. `OilJob004`, `PmcCon033`?
pub fn is_job_module(module: &str) -> bool {
    let tail_digits = |s: &str| -> bool {
        let d: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
        !d.is_empty()
    };
    for f in FACTION_PREFIXES {
        if let Some(rest) = module.strip_prefix(f) {
            if (rest.starts_with("Job") || rest.starts_with("Con")) && tail_digits(rest) {
                return true;
            }
        }
    }
    module.starts_with("MrxTaskObjective")
}
