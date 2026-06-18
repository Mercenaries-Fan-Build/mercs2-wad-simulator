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
