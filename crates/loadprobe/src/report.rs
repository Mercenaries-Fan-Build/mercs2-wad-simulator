//! Analysis of parsed log lines → a forensic `Report`, with a detailed text dump and
//! an optional JSON form.

use crate::parse::LogLine;
use crate::phases::{self, LADDER, REACHED_WORLD_IDX};
use colored::*;
use serde::Serialize;
use std::collections::BTreeMap;

// ----- small string helpers -------------------------------------------------

fn after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    s.find(key).map(|i| &s[i + key.len()..])
}
fn dec_after(s: &str, key: &str) -> Option<i64> {
    let t = after(s, key)?;
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
    digits.parse().ok()
}
fn hex_after(s: &str, key: &str) -> Option<u32> {
    let t = after(s, key)?.trim_start();
    let h: String = t.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if h.is_empty() { None } else { u32::from_str_radix(&h, 16).ok() }
}
fn token_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let t = after(s, key)?.trim_start();
    let end = t.find(|c: char| c.is_whitespace()).unwrap_or(t.len());
    Some(&t[..end])
}

fn fmt_dur(ms: i64) -> String {
    if ms < 0 { return "0ms".into(); }
    let s = ms / 1000;
    let m = s / 60;
    if m > 0 { format!("{}m{:02}s", m, s % 60) }
    else if s > 0 { format!("{}.{:01}s", s, (ms % 1000) / 100) }
    else { format!("{}ms", ms) }
}

fn fmt_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB * KB { format!("{:.1} GB", b / (KB * KB * KB)) }
    else if b >= KB * KB { format!("{:.1} MB", b / (KB * KB)) }
    else if b >= KB { format!("{:.1} KB", b / KB) }
    else { format!("{} B", bytes) }
}

// ----- report data model ----------------------------------------------------

#[derive(Serialize)]
pub struct PhaseHit { pub idx: usize, pub name: String, pub raw_ts: String, pub ts_ms: u64 }

#[derive(Serialize)]
pub struct StreamSummary {
    pub cycles_enter: usize,
    pub cycles_exit: usize,
    pub max_refcount: i64,
    pub first_ts: String,
    pub last_ts: String,
    pub duration_ms: i64,
}

#[derive(Serialize)]
pub struct PoolHealth {
    pub distinct: Option<i64>,
    pub cap: Option<i64>,
    pub fits: Option<bool>,
    pub total_inserts: Option<i64>,
    pub top_callers: Vec<(String, i64)>,    // (caller addr, count)
    pub garbage_distinct: usize,
    pub garbage_samples: Vec<String>,       // "key=.. +4=.. caller=.."
    pub free_min: Option<i64>,
    pub free_final: Option<i64>,
    pub bursts: Vec<String>,
    pub refills: Vec<String>,
}

#[derive(Serialize)]
pub struct CrashInfo {
    pub raw_ts: String,
    pub ts_ms: u64,
    pub code: String,
    pub eip: u32,
    pub eip_label: Option<String>,
    pub av: Option<String>,        // "READ target=F011157A"
    pub block: Vec<String>,        // the full [crash] block lines (message bodies)
    pub terminal: bool,            // no progress after it
    pub since_world_load_ms: Option<i64>,
}

#[derive(Serialize)]
pub struct SourceGroup {
    pub source: String,
    pub count: usize,
    pub first_ts: String,
    pub last_ts: String,
    pub samples: Vec<String>,
}

#[derive(Serialize)]
pub struct SignalMarker { pub text: String, pub count: usize, pub first_ts: String, pub last_ts: String }

#[derive(Serialize)]
pub struct Gap { pub ms: i64, pub at_ts: String, pub before: String, pub after: String }

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum Verdict {
    ReachedWorld { furthest: usize, name: String, post_load_crash: Option<u32> },
    Crash { furthest: usize, name: String, eip: u32, label: Option<String> },
    Hang { furthest: usize, name: String, stuck_ms: i64, steady_free: Option<i64> },
    Truncated { furthest: usize, name: String },
}

/// One game artifact fingerprinted by pmc_bb's `[blackbox] BUILD` lines — the
/// objective binding from this run to the exact bytes that produced it.
#[derive(Serialize, Clone)]
pub struct BuildArtifact {
    pub kind: String,        // exe | dll | asi | wad
    pub name: String,
    pub hash_type: String,   // sha256 | qsha256 (head+tail+size, for >1GiB files)
    pub sha256: String,      // 64-hex, or "UNREADABLE"
    pub size: Option<u64>,
}

/// Parse a `BUILD <kind>=<name> <sha256|qsha256>=<hex> size=<n>` message body.
fn parse_build_line(msg: &str) -> Option<BuildArtifact> {
    let rest = msg.trim_start().strip_prefix("BUILD ")?;
    let (kind, after_kind) = rest.split_once('=')?;
    let (name, hash_type, val_rest) = if let Some(i) = after_kind.find(" qsha256=") {
        (&after_kind[..i], "qsha256", &after_kind[i + " qsha256=".len()..])
    } else if let Some(i) = after_kind.find(" sha256=") {
        (&after_kind[..i], "sha256", &after_kind[i + " sha256=".len()..])
    } else {
        return None;
    };
    let sha256 = val_rest.split_whitespace().next().unwrap_or("").to_string();
    let size = after(val_rest, "size=").and_then(|t| {
        let d: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        d.parse::<u64>().ok()
    });
    Some(BuildArtifact {
        kind: kind.trim().to_string(),
        name: name.trim().to_string(),
        hash_type: hash_type.to_string(),
        sha256,
        size,
    })
}

#[derive(Serialize)]
pub struct Report {
    pub file: String,
    /// SHA-256 of the analyzed log file (identity of the log itself).
    pub log_sha256: String,
    /// Game artifacts fingerprinted by pmc_bb at boot (empty ⇒ not self-attributing).
    pub build: Vec<BuildArtifact>,
    pub records: usize,
    pub first_ts: String,
    pub last_ts: String,
    pub wall_ms: i64,
    pub furthest_idx: usize,
    pub furthest_name: String,
    pub pct: u32,
    pub verdict: Verdict,
    pub phases: Vec<PhaseHit>,
    pub streaming: Option<StreamSummary>,
    pub acts: Vec<(String, String)>,            // (ts, text)
    pub jobs: Vec<(String, usize)>,             // (module, count) faction jobs only
    pub all_modules: usize,
    pub portals: Vec<(String, String)>,         // (ts, "Enabling X portal")
    pub players: Vec<(String, String)>,
    pub pool: PoolHealth,
    pub crash: Option<CrashInfo>,
    pub mtrl_overcounts: Vec<String>,
    pub stall_dumps: Vec<String>,
    pub flagged: Vec<SourceGroup>,
    pub signals: Vec<SignalMarker>,
    pub gaps: Vec<Gap>,
    // --- robustness / "undetected" surfacing ---
    pub unknown_sources: Vec<(String, usize)>,  // source tags loadprobe doesn't recognize
    pub unparsed_lines: usize,                   // lines that didn't match the log format
    pub tail: Vec<String>,                       // the last meaningful lines (what it was doing at the end)
    pub last_progress_ts: String,                // last [lua]/[world] line ts
    pub last_progress_msg: String,
}

// ----- analysis -------------------------------------------------------------

pub fn analyze(file: &str, log_sha256: String, lines: &[LogLine], routine: &[String], signals: &[String], hang_secs: u64, top_gaps: usize) -> Report {
    let real: Vec<&LogLine> = lines.iter().filter(|l| !l.raw_ts.is_empty()).collect();

    // --- self-attribution: pmc_bb's [blackbox] BUILD fingerprint lines ---
    let build: Vec<BuildArtifact> = lines.iter()
        .filter(|l| l.source == "blackbox")
        .filter_map(|l| parse_build_line(&l.msg))
        .collect();
    let first_ts = real.first().map(|l| l.raw_ts.clone()).unwrap_or_default();
    let last_ts = real.last().map(|l| l.raw_ts.clone()).unwrap_or_default();
    let wall_ms = match (real.first(), real.last()) { (Some(a), Some(b)) => (b.ts_ms - a.ts_ms) as i64, _ => 0 };

    // --- phase hits ---
    let mut phases: Vec<PhaseHit> = Vec::new();
    for ph in LADDER {
        if let Some(l) = lines.iter().find(|l| phase_matches(ph, &l.msg)) {
            phases.push(PhaseHit { idx: ph.idx, name: ph.name.into(), raw_ts: l.raw_ts.clone(), ts_ms: l.ts_ms });
        }
    }
    let furthest_idx = phases.iter().map(|p| p.idx).max().unwrap_or(0);
    let furthest_name = LADDER.iter().find(|p| p.idx == furthest_idx).map(|p| p.name).unwrap_or("?").to_string();
    let pct = ((furthest_idx as f64 / (LADDER.len() - 1) as f64) * 100.0).round() as u32;

    // --- streaming aggregation ---
    let streaming = aggregate_streaming(&lines);

    // --- acts / jobs / portals / players ---
    let mut acts = Vec::new();
    let mut module_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut portals = Vec::new();
    let mut players = Vec::new();
    for l in lines.iter() {
        if l.msg.contains("Staging Act") {
            acts.push((l.raw_ts.clone(), l.msg.trim_start_matches('*').trim().to_string()));
        }
        if let Some(m) = after(&l.msg, "Dynamically imported module ") {
            *module_counts.entry(m.trim().to_string()).or_default() += 1;
        }
        if l.msg.contains("Enabling ") && l.msg.contains("portal") {
            let name = strip_at(&l.msg);
            if !portals.iter().any(|(_, p)| *p == name) {
                portals.push((l.raw_ts.clone(), name));
            }
        }
        if l.msg.contains("CreatePlayerCharacter") {
            players.push((l.raw_ts.clone(), strip_at(&l.msg)));
        }
    }
    let all_modules = module_counts.values().sum();
    let mut jobs: Vec<(String, usize)> = module_counts.iter()
        .filter(|(m, _)| phases::is_job_module(m))
        .map(|(m, c)| (m.clone(), *c)).collect();
    jobs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    // --- pool / cc health ---
    let pool = analyze_pool(&lines);

    // --- crash detection ---
    let crash = analyze_crash(&lines, &phases);

    // --- mtrl overcounts + stall dumps (full text) ---
    let mtrl_overcounts: Vec<String> = lines.iter()
        .filter(|l| l.source == "mtrl" && l.msg.contains("OVERCOUNT"))
        .map(|l| format!("[{}] {}", l.raw_ts, l.msg)).collect();
    let stall_dumps: Vec<String> = lines.iter()
        .filter(|l| l.source == "stall" && !l.msg.contains("watchdog started"))
        .map(|l| format!("[{}] {}", l.raw_ts, l.msg)).collect();

    // --- flagged extras grouped by non-routine source ---
    let flagged = group_flagged(&lines, routine);

    // --- high-signal lua markers ---
    let sigrefs: Vec<&str> = signals.iter().map(|s| s.as_str()).collect();
    let signals_out = collect_signals(&lines, &sigrefs);

    // --- gaps ---
    let gaps = top_gaps_fn(&real, top_gaps);

    // --- robustness: unknown sources, unparsed lines, and the end-of-log tail ---
    let mut src_counts: BTreeMap<String, usize> = BTreeMap::new();
    for l in lines { *src_counts.entry(l.source.clone()).or_default() += 1; }
    let unparsed_lines = *src_counts.get("raw").unwrap_or(&0);
    let unknown_sources: Vec<(String, usize)> = src_counts.iter()
        .filter(|(s, _)| !phases::is_known_source(s))
        .map(|(s, c)| (s.clone(), *c)).collect();
    let last_prog = lines.iter().rev().find(|l| l.source == "lua" || l.source == "world");
    let last_progress_ts = last_prog.map(|l| l.raw_ts.clone()).unwrap_or_default();
    let last_progress_msg = last_prog.map(|l| l.msg.clone()).unwrap_or_default();
    // tail = the last ~10 lines, but collapse a run of identical [pool] free polls into one.
    let tail = build_tail(&real, 10);

    // --- verdict ---
    // The world is "fully loaded" once GlobalExit-Complete (phase 20) fires. A crash
    // AFTER that is a POST-LOAD event (gameplay/exit/teardown) — the run still booted
    // into the game, so the headline stays REACHED-WORLD with the crash noted. A crash
    // BEFORE the world finishes loading is a blocking CRASH.
    // A teardown-EIP crash (e.g. 0x874E7D) is a HARD-CLOSE artifact, NOT a bug — so it
    // never becomes a "CRASH" headline. Only a non-teardown terminal crash that hit
    // before the world finished loading is a blocking crash.
    let globalexit_ms = phases.iter().find(|p| p.idx == REACHED_WORLD_IDX).map(|p| p.ts_ms);
    let reached_world = globalexit_ms.is_some();
    let term_crash = crash.as_ref().filter(|c| c.terminal);
    let is_teardown = term_crash.map(|c| phases::is_teardown_eip(c.eip)).unwrap_or(false);
    let post_load_crash = match (term_crash, globalexit_ms) {
        (Some(c), Some(gx)) if c.ts_ms >= gx => Some(c.eip),
        _ => None,
    };
    // blocking crash: a REAL (non-teardown) terminal crash that occurred before the world loaded
    let blocking_crash = term_crash.filter(|c| !phases::is_teardown_eip(c.eip) && post_load_crash.is_none());
    let hang = detect_hang(&lines, &phases, hang_secs, &pool);
    let verdict = if let Some(c) = blocking_crash {
        Verdict::Crash { furthest: furthest_idx, name: furthest_name.clone(), eip: c.eip, label: c.eip_label.clone() }
    } else if reached_world {
        // booted into the game; a post-load crash (incl. a hard-close teardown) is noted, not fatal
        Verdict::ReachedWorld { furthest: furthest_idx, name: furthest_name.clone(), post_load_crash }
    } else if let Some((stuck, free)) = hang {
        // load wedged; if a teardown crash followed it was the user killing the stuck process
        Verdict::Hang { furthest: furthest_idx, name: furthest_name.clone(), stuck_ms: stuck, steady_free: free }
    } else if is_teardown {
        // user hard-closed mid-load before it qualified as a hang
        Verdict::Truncated { furthest: furthest_idx, name: furthest_name.clone() }
    } else {
        Verdict::Truncated { furthest: furthest_idx, name: furthest_name.clone() }
    };

    Report {
        file: file.to_string(), log_sha256, build,
        records: real.len(), first_ts, last_ts, wall_ms,
        furthest_idx, furthest_name, pct, verdict, phases, streaming,
        acts, jobs, all_modules, portals, players, pool, crash,
        mtrl_overcounts, stall_dumps, flagged, signals: signals_out, gaps,
        unknown_sources, unparsed_lines, tail, last_progress_ts, last_progress_msg,
    }
}

/// Build a compact end-of-log tail: the last `n` records, collapsing a trailing run of
/// identical `[pool] free=N` polls into a single "... ×K" line so the real last activity
/// (the line before the wedge/crash) stays visible even for unrecognized end-states.
fn build_tail(real: &[&LogLine], n: usize) -> Vec<String> {
    let take = real.len().min(n.max(1) * 3);
    let slice = &real[real.len() - take..];
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < slice.len() {
        let l = slice[i];
        // collapse consecutive identical pool polls
        if l.source == "pool" && l.msg.starts_with("free=") {
            let mut j = i + 1;
            while j < slice.len() && slice[j].source == "pool" && slice[j].msg == l.msg { j += 1; }
            let k = j - i;
            if k > 1 {
                out.push(format!("[{}] [pool] {}  (×{} steady)", l.raw_ts, l.msg, k));
                i = j; continue;
            }
        }
        out.push(format!("[{}] [{}] {}", l.raw_ts, l.source, truncate(&l.msg, 90)));
        i += 1;
    }
    // keep only the last n collapsed entries
    if out.len() > n { out.drain(0..out.len() - n); }
    out
}

fn phase_matches(ph: &phases::Phase, msg: &str) -> bool {
    // "World entities online" needs both "Enabling " and "portal" (keyed on content,
    // not a hardcoded index, so reordering the ladder can't break it).
    if ph.matches == ["Enabling "] { return msg.contains("Enabling ") && msg.contains("portal"); }
    ph.matches.iter().any(|m| msg.contains(m))
}

fn strip_at(msg: &str) -> String {
    match msg.find("  @") { Some(i) => msg[..i].to_string(), None => msg.to_string() }
}

fn aggregate_streaming(lines: &[LogLine]) -> Option<StreamSummary> {
    let mut enter = 0; let mut exit = 0; let mut maxr = -1i64;
    let mut first = String::new(); let mut last = String::new();
    let mut first_ms = 0u64; let mut last_ms = 0u64;
    for l in lines {
        if !l.msg.contains("STATE_WAITFORSTREAMING") { continue; }
        let is_enter = l.msg.contains("MrxState.Enter");
        let is_exit = l.msg.contains("MrxState.Exit");
        if !is_enter && !is_exit { continue; }
        if first.is_empty() { first = l.raw_ts.clone(); first_ms = l.ts_ms; }
        last = l.raw_ts.clone(); last_ms = l.ts_ms;
        if is_enter { enter += 1; } else { exit += 1; }
        if let Some(r) = dec_after(&l.msg, "refcount=") { if r > maxr { maxr = r; } }
    }
    if enter == 0 && exit == 0 { return None; }
    Some(StreamSummary { cycles_enter: enter, cycles_exit: exit, max_refcount: maxr,
        first_ts: first, last_ts: last, duration_ms: (last_ms - first_ms) as i64 })
}

fn analyze_pool(lines: &[LogLine]) -> PoolHealth {
    let mut distinct = None; let mut cap = None; let mut fits = None;
    let mut total_inserts = None; let mut top_callers = Vec::new();
    let mut garbage_keys: BTreeMap<String, ()> = BTreeMap::new();
    let mut garbage_samples = Vec::new();
    let mut free_min = None; let mut free_final = None;
    let mut bursts = Vec::new(); let mut refills = Vec::new();

    for l in lines {
        if l.source == "cc" {
            if let Some(d) = dec_after(&l.msg, "DISTINCT texture hashes inserted: ") {
                distinct = Some(d);
                cap = dec_after(&l.msg, "pool cap ");
                fits = Some(l.msg.contains("fits"));
            }
            if let Some(t) = dec_after(&l.msg, "total_inserts=") { total_inserts = Some(t); }
            if l.msg.starts_with("  caller=") {
                if let (Some(c), Some(n)) = (token_after(&l.msg, "caller="), dec_after(&l.msg, "count=")) {
                    if top_callers.len() < 6 { top_callers.push((c.to_string(), n)); }
                }
            }
            if l.msg.starts_with("GARBAGE ") {
                if let Some(k) = token_after(&l.msg, "key=") {
                    if garbage_keys.insert(k.to_string(), ()).is_none() && garbage_samples.len() < 8 {
                        let plus4 = token_after(&l.msg, "+4=").unwrap_or("?");
                        let caller = token_after(&l.msg, "caller=").unwrap_or("?");
                        garbage_samples.push(format!("key={} +4={} caller={}", k, plus4, caller));
                    }
                }
            }
        }
        if l.source == "pool" {
            // "free 5120 -> 5119 (-1)" / "free=5054 (min=5054)" / "BURST drop ... " / "free REFILLED A -> B"
            if l.msg.contains("BURST") { bursts.push(format!("[{}] {}", l.raw_ts, l.msg)); }
            if l.msg.contains("REFILLED") { refills.push(format!("[{}] {}", l.raw_ts, l.msg)); }
            if let Some(m) = dec_after(&l.msg, "min=") { free_min = Some(m); }
            // current free: prefer "-> N" then "free=N"
            let cur = dec_after(&l.msg, "-> ").or_else(|| dec_after(&l.msg, "free="));
            if let Some(f) = cur { free_final = Some(f); }
        }
    }
    PoolHealth { distinct, cap, fits, total_inserts, top_callers,
        garbage_distinct: garbage_keys.len(), garbage_samples, free_min, free_final, bursts, refills }
}

fn analyze_crash(lines: &[LogLine], phases: &[PhaseHit]) -> Option<CrashInfo> {
    let idx = lines.iter().rposition(|l| l.source == "crash" && l.msg.contains("VEH EXCEPTION"))?;
    let head = &lines[idx];
    let code = token_after(&head.msg, "EXCEPTION ").unwrap_or("?").to_string();
    let eip = hex_after(&head.msg, "EIP=").unwrap_or(0);
    // gather the contiguous [crash] block from idx forward
    let mut block = Vec::new();
    let mut av = None;
    for l in lines[idx..].iter() {
        if l.source != "crash" { break; }
        if l.msg.starts_with("AV ") { av = Some(l.msg.trim_start_matches("AV ").to_string()); }
        block.push(l.msg.clone());
        if block.len() >= 40 { break; }
    }
    // terminal if no lua/world progress after the crash ts
    let crash_ms = head.ts_ms;
    let terminal = !lines.iter().any(|l| (l.source == "lua" || l.source == "world") && l.ts_ms > crash_ms);
    let world_ms = phases.iter().find(|p| p.idx == 10).map(|p| p.ts_ms);
    let since = world_ms.map(|w| (crash_ms - w) as i64);
    Some(CrashInfo {
        raw_ts: head.raw_ts.clone(), ts_ms: head.ts_ms, code, eip, eip_label: phases::eip_label(eip).map(String::from),
        av, block, terminal, since_world_load_ms: since,
    })
}

fn detect_hang(lines: &[LogLine], phases: &[PhaseHit], hang_secs: u64, _pool: &PoolHealth) -> Option<(i64, Option<i64>)> {
    // last lua/world progress ts
    let last_prog = lines.iter().rev().find(|l| l.source == "lua" || l.source == "world")?;
    let last = lines.iter().rev().find(|l| !l.raw_ts.is_empty())?;
    let stuck = (last.ts_ms - last_prog.ts_ms) as i64;
    if stuck < (hang_secs as i64) * 1000 { return None; }
    // tail after last_prog must be dominated by pool, with a steady free value
    let tail: Vec<&LogLine> = lines.iter().filter(|l| l.ts_ms > last_prog.ts_ms && !l.raw_ts.is_empty()).collect();
    if tail.is_empty() { return None; }
    let pool_n = tail.iter().filter(|l| l.source == "pool").count();
    if pool_n * 2 < tail.len() { return None; } // not pool-dominated
    let frees: Vec<i64> = tail.iter().filter(|l| l.source == "pool")
        .filter_map(|l| dec_after(&l.msg, "free=")).collect();
    let steady = if !frees.is_empty() && frees.iter().all(|f| *f == frees[0]) { Some(frees[0]) } else { None };
    let _ = phases;
    Some((stuck, steady))
}

fn group_flagged(lines: &[LogLine], routine: &[String]) -> Vec<SourceGroup> {
    let mut groups: BTreeMap<String, Vec<&LogLine>> = BTreeMap::new();
    for l in lines {
        if l.source == "raw" { continue; }
        if routine.iter().any(|r| r == &l.source) { continue; }
        groups.entry(l.source.clone()).or_default().push(l);
    }
    let mut out = Vec::new();
    for (src, ls) in groups {
        let sample_n = if phases::INIT_NOISE.contains(&src.as_str()) { 1 } else { 6 };
        let samples = ls.iter().take(sample_n).map(|l| l.msg.clone()).collect();
        out.push(SourceGroup {
            source: src, count: ls.len(),
            first_ts: ls.first().map(|l| l.raw_ts.clone()).unwrap_or_default(),
            last_ts: ls.last().map(|l| l.raw_ts.clone()).unwrap_or_default(),
            samples,
        });
    }
    // sort: instrumentation first, then by count desc
    out.sort_by(|a, b| {
        let ia = phases::INSTRUMENTATION.contains(&a.source.as_str());
        let ib = phases::INSTRUMENTATION.contains(&b.source.as_str());
        ib.cmp(&ia).then(b.count.cmp(&a.count))
    });
    out
}

fn collect_signals(lines: &[LogLine], prefixes: &[&str]) -> Vec<SignalMarker> {
    let mut map: BTreeMap<String, (usize, String, String)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for l in lines {
        if l.source != "lua" && l.source != "world" { continue; }
        if l.signal_prefix(prefixes).is_none() { continue; }
        // Portal enable/disable @@@ lines are already summarized in PROGRESSION — skip the flood.
        if l.msg.contains("portal") && (l.msg.contains("Enabling") || l.msg.contains("Disabling")) { continue; }
        let key = l.msg.clone();
        let e = map.entry(key.clone()).or_insert_with(|| { order.push(key.clone()); (0, l.raw_ts.clone(), l.raw_ts.clone()) });
        e.0 += 1; e.2 = l.raw_ts.clone();
    }
    order.into_iter().map(|k| { let v = &map[&k]; SignalMarker { text: k.clone(), count: v.0, first_ts: v.1.clone(), last_ts: v.2.clone() } }).collect()
}

fn top_gaps_fn(real: &[&LogLine], n: usize) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = Vec::new();
    for w in real.windows(2) {
        let d = (w[1].ts_ms - w[0].ts_ms) as i64;
        if d <= 0 { continue; }
        gaps.push(Gap { ms: d, at_ts: w[0].raw_ts.clone(),
            before: format!("[{}] {}", w[0].source, truncate(&w[0].msg, 60)),
            after: format!("[{}] {}", w[1].source, truncate(&w[1].msg, 60)) });
    }
    gaps.sort_by(|a, b| b.ms.cmp(&a.ms));
    gaps.truncate(n);
    gaps
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n).collect::<String>()) }
}

// ----- text dump ------------------------------------------------------------

pub fn print_text(r: &Report) {
    let bar = "─".repeat(78);
    println!("{}", bar.dimmed());
    println!("{} {}", "loadprobe".bold().cyan(), r.file.dimmed());
    println!("{}", bar.dimmed());

    // big "LOADED X%" banner with a progress bar (derived from the phase ladder)
    let filled = (r.pct as usize * 30 / 100).min(30);
    let pbar = format!("{}{}", "█".repeat(filled), "░".repeat(30 - filled));
    let pct_str = format!("LOADED {:>3}%", r.pct);
    let colored_pct = if r.pct >= 100 { pct_str.bold().green() } else if r.pct >= 50 { pct_str.bold().yellow() } else { pct_str.bold().red() };
    println!("{}  {}  (phase {}/{}: {})", colored_pct, pbar, r.furthest_idx, LADDER.len() - 1, r.furthest_name);

    let hard_closed = r.crash.as_ref().map(|c| phases::is_teardown_eip(c.eip)).unwrap_or(false);

    // verdict line
    let (tag, detail) = match &r.verdict {
        Verdict::ReachedWorld { post_load_crash, .. } => {
            let d = match post_load_crash {
                Some(eip) if phases::is_teardown_eip(*eip) => {
                    let gx = r.phases.iter().find(|p| p.idx == phases::REACHED_WORLD_IDX).map(|p| p.ts_ms);
                    let delay = match (gx, r.crash.as_ref()) { (Some(g), Some(c)) => format!(" {} after load", fmt_dur((c.ts_ms - g) as i64)), _ => String::new() };
                    format!("booted into game (GlobalExit complete); you then hard-closed it{} (teardown @0x{:08X}, benign)", delay, eip)
                }
                Some(eip) => {
                    let gx = r.phases.iter().find(|p| p.idx == phases::REACHED_WORLD_IDX).map(|p| p.ts_ms);
                    let delay = match (gx, r.crash.as_ref()) { (Some(g), Some(c)) => format!(" ({} after load)", fmt_dur((c.ts_ms - g) as i64)), _ => String::new() };
                    format!("booted into game (GlobalExit complete) — note: POST-LOAD crash @0x{:08X}{} (gameplay/exit, not a load blocker)", eip, delay)
                }
                None => "booted into game — GlobalExit complete, load finished".to_string(),
            };
            ("REACHED-WORLD".bold().green(), d)
        }
        Verdict::Crash { eip, label, furthest, .. } => {
            let lbl = match label {
                Some(l) => format!(" — {}", l),
                None => " — UNRECOGNIZED EIP (candidate NEW crash site — add to KNOWN_EIPS)".to_string(),
            };
            let when = if *furthest >= phases::REACHED_WORLD_IDX { " (after full world load)".to_string() }
                       else if *furthest >= phases::ENTERED_WORLD_IDX { " (after entering world)".to_string() }
                       else { String::new() };
            ("CRASH".bold().red(), format!("@ EIP=0x{:08X}{}{}", eip, lbl, when))
        }
        Verdict::Hang { stuck_ms, steady_free, .. } => {
            let closed = if hard_closed { " then hard-closed (teardown 0x874E7D)" } else { "" };
            ("HANG".bold().yellow(),
             format!("load wedged {}{}{}", fmt_dur(*stuck_ms),
                steady_free.map(|f| format!(", steady free={}", f)).unwrap_or_default(), closed))
        }
        Verdict::Truncated { furthest, .. } => {
            let note = if hard_closed { "manually hard-closed mid-load (teardown 0x874E7D), load did not complete".to_string() }
                       else if *furthest >= phases::ENTERED_WORLD_IDX { "entered world but load did not complete".to_string() }
                       else { "log ends mid-load (no crash/hang signature)".to_string() };
            ("TRUNCATED".bold().yellow(), note)
        }
    };
    println!("VERDICT: {} {}", tag, detail);
    println!("Span: {} → {}  ({})  •  {} records", r.first_ts, r.last_ts, fmt_dur(r.wall_ms), r.records);
    println!("Last progress: [{}] {}", r.last_progress_ts, truncate(&r.last_progress_msg, 80));

    // --- BUILD / RUN IDENTITY: bind this run's metrics to the exact bytes ---
    println!("\n{}", "── BUILD / RUN IDENTITY ───────────────────────".cyan().bold());
    println!("  log   {:<22} sha256 {}", "(this log)".dimmed(), r.log_sha256);
    if r.build.is_empty() {
        println!("  {} no [blackbox] BUILD lines — this run is NOT self-attributing",
                 "⚠".yellow());
        println!("    (old pmc_bb.dll without the fingerprint emitter; rebuild+deploy it,");
        println!("     or hash the deployed WAD by hand to attribute this run)");
    } else {
        // WAD / exe / asi / the self dll get a full line; redistributable dlls collapse.
        let is_primary = |a: &BuildArtifact| {
            a.kind == "wad" || a.kind == "exe" || a.kind == "asi"
                || (a.kind == "dll" && a.name.eq_ignore_ascii_case("pmc_bb.dll"))
        };
        for a in r.build.iter().filter(|a| is_primary(a)) {
            let sz = a.size.map(fmt_size).unwrap_or_default();
            let line = format!("  {:<4}  {:<20} {} {}  {}",
                               a.kind, a.name, a.hash_type, a.sha256, sz);
            if a.kind == "wad" { println!("{}", line.bold()); } else { println!("{}", line); }
        }
        let others = r.build.iter().filter(|a| !is_primary(a)).count();
        if others > 0 {
            println!("  {}  ({} other dll(s) — full hashes in --json)", "dll".dimmed(), others);
        }
    }

    // coverage / undetected surfacing — never let unknown content pass silently
    if !r.unknown_sources.is_empty() || r.unparsed_lines > 0 {
        println!("\n{}", "── COVERAGE / UNDETECTED ──────────────────────".yellow().bold());
        for (s, c) in &r.unknown_sources {
            println!("  {} UNKNOWN SOURCE [{}] ×{} — new instrumentation? teach loadprobe (phases::KNOWN_SOURCES)", "⚠".yellow(), s, c);
        }
        if r.unparsed_lines > 0 {
            println!("  {} {} unparsed line(s) (didn't match `[ts] [source] msg`) — kept as raw continuation", "⚠".yellow(), r.unparsed_lines);
        }
    }

    // crash detail
    if let Some(c) = &r.crash {
        let teardown = phases::is_teardown_eip(c.eip);
        let post_load = matches!(&r.verdict, Verdict::ReachedWorld { post_load_crash: Some(_), .. });
        let hdr = if teardown { "── HARD-CLOSE / TEARDOWN (not a real crash) ───".yellow().bold() }
                  else if post_load { "── CRASH (POST-LOAD — after world fully loaded) ".yellow().bold() }
                  else { "── CRASH ──────────────────────────────────────".red().bold() };
        println!("\n{}", hdr);
        let term = if !c.terminal { "VEH-recovered (load continued)".yellow().to_string() }
                   else if teardown { "process teardown — you force-closed the game".yellow().to_string() }
                   else if post_load { "terminal, but AFTER load completed".yellow().to_string() }
                   else { "TERMINAL (blocked the load)".red().to_string() };
        println!("  [{}] {} @ EIP=0x{:08X}  ({})", c.raw_ts, c.code, c.eip, term);
        match &c.eip_label {
            Some(l) => println!("  subsystem: {}", l.bold()),
            None => println!("  subsystem: {}", "UNRECOGNIZED — new crash site; add EIP to phases::KNOWN_EIPS".yellow().bold()),
        }
        if let Some(av) = &c.av { println!("  AV {}", av); }
        if let Some(s) = c.since_world_load_ms { println!("  {} after world-load start", fmt_dur(s)); }
        for line in c.block.iter().take(24) { println!("    {}", line.dimmed()); }
    }

    // phase timeline
    println!("\n{}", "── PHASE TIMELINE ─────────────────────────────".cyan().bold());
    let mut prev_ms: Option<u64> = None;
    for ph in LADDER {
        let hit = r.phases.iter().find(|p| p.idx == ph.idx);
        match hit {
            Some(h) => {
                let delta = match prev_ms {
                    Some(p) if h.ts_ms >= p => format!("+{}", fmt_dur((h.ts_ms - p) as i64)),
                    Some(p) => format!("−{} (out of ladder order)", fmt_dur((p - h.ts_ms) as i64)),
                    None => String::new(),
                };
                println!("  {} {:>2}  {:<28} [{}] {}", "✓".green(), ph.idx, ph.name, h.raw_ts, delta.dimmed());
                prev_ms = Some(h.ts_ms);
            }
            None => println!("  {} {:>2}  {}", "·".dimmed(), ph.idx, ph.name.dimmed()),
        }
    }
    if let Some(s) = &r.streaming {
        println!("  {} WAITFORSTREAMING: {} enter / {} exit, max refcount {}, {}→{} ({})",
            "↻".cyan(), s.cycles_enter, s.cycles_exit, s.max_refcount, s.first_ts, s.last_ts, fmt_dur(s.duration_ms));
    }

    // progression / acts
    if !r.acts.is_empty() || !r.jobs.is_empty() || !r.players.is_empty() || !r.portals.is_empty() {
        println!("\n{}", "── PROGRESSION / ACTS ─────────────────────────".magenta().bold());
        for (ts, a) in &r.acts { println!("  {} [{}] {}", "ACT".bold().magenta(), ts, a); }
        for (ts, p) in &r.players { println!("  {} [{}] {}", "PLAYER".bold().green(), ts, p); }
        if !r.jobs.is_empty() {
            let list: Vec<String> = r.jobs.iter().map(|(m, c)| if *c > 1 { format!("{}×{}", m, c) } else { m.clone() }).collect();
            println!("  jobs/contracts ({} of {} module imports): {}", r.jobs.len(), r.all_modules, list.join(", "));
        }
        if !r.portals.is_empty() {
            println!("  world entities: {} portal enables (e.g. {})", r.portals.len(),
                r.portals.iter().take(3).map(|(_, p)| p.clone()).collect::<Vec<_>>().join("; "));
        }
    }

    // pool / cc health
    println!("\n{}", "── TEXTURE-COMPONENT POOL HEALTH (cc/pool) ────".blue().bold());
    match (r.pool.distinct, r.pool.cap) {
        (Some(d), Some(cap)) => {
            let gauge = if d <= cap { format!("{} / {} cap  FITS", d, cap).green() } else { format!("{} / {} cap  EXCEEDS", d, cap).red() };
            println!("  distinct texture hashes: {}", gauge);
        }
        (Some(d), None) => println!("  distinct texture hashes: {}", d),
        _ => println!("  {}", "no cc DISTINCT line (no insert-histogram dump in this log)".dimmed()),
    }
    if let Some(t) = r.pool.total_inserts { println!("  total inserts: {}", t); }
    if !r.pool.top_callers.is_empty() {
        let dom = if r.pool.top_callers.len() == 1 { " (single dominant caller ⇒ one object inflating)".dimmed().to_string() } else { String::new() };
        let cs: Vec<String> = r.pool.top_callers.iter().map(|(c, n)| format!("{}×{}", c, n)).collect();
        println!("  insert callers: {}{}", cs.join(", "), dom);
    }
    if r.pool.garbage_distinct > 0 {
        println!("  {} {} GARBAGE keys sampled (cc dumps the first few; param-float-shaped, read as tex hashes):",
            "⚠".yellow(), r.pool.garbage_distinct);
        for s in r.pool.garbage_samples.iter().take(6) { println!("      {}", s.dimmed()); }
        println!("      {}", "note: +4=F011157A is the texture sentinel — same value as the 0x874E7D crash AV target".dimmed());
    }
    if let (Some(min), Some(fin)) = (r.pool.free_min, r.pool.free_final) {
        println!("  pool free: min {} / final {}", min, fin);
    }
    for b in r.pool.bursts.iter().take(3) { println!("  {}", b.yellow()); }
    for rf in r.pool.refills.iter().take(3) { println!("  {}", rf.green()); }

    // mtrl / stall
    if !r.mtrl_overcounts.is_empty() {
        println!("\n{}", "── MTRL OVERCOUNT (count>10) ──────────────────".red().bold());
        for l in r.mtrl_overcounts.iter().take(16) { println!("  {}", l); }
    }
    if !r.stall_dumps.is_empty() {
        println!("\n{}", "── STREAMING STALL DUMP ───────────────────────".red().bold());
        for l in r.stall_dumps.iter().take(20) { println!("  {}", l); }
    }

    // high-signal markers (cap to keep the dump readable; portals already in PROGRESSION)
    if !r.signals.is_empty() {
        println!("\n{}", "── HIGH-SIGNAL LUA MARKERS (### !!! @@@ *** ##@ =-=) ─".yellow().bold());
        const CAP: usize = 45;
        for s in r.signals.iter().take(CAP) {
            let c = if s.count > 1 { format!(" (×{})", s.count) } else { String::new() };
            println!("  [{}] {}{}", s.first_ts, truncate(&s.text, 92), c.dimmed());
        }
        if r.signals.len() > CAP {
            println!("  {}", format!("… +{} more distinct signal markers (use --json for all)", r.signals.len() - CAP).dimmed());
        }
    }

    // flagged extras
    println!("\n{}", "── FLAGGED SOURCES (non-routine) ──────────────".white().bold());
    for g in &r.flagged {
        let tag = if phases::INSTRUMENTATION.contains(&g.source.as_str()) { format!("[{}]", g.source).bold() } else { format!("[{}]", g.source).normal() };
        println!("  {} ×{}  ({}→{})", tag, g.count, g.first_ts, g.last_ts);
        if !phases::INIT_NOISE.contains(&g.source.as_str()) {
            for s in g.samples.iter().take(4) { println!("      {}", truncate(s, 96).dimmed()); }
        }
    }

    // gaps
    if !r.gaps.is_empty() {
        println!("\n{}", "── LARGEST TIME GAPS ──────────────────────────".cyan().bold());
        for g in &r.gaps {
            println!("  {:>7} at [{}]  {} → {}", fmt_dur(g.ms), g.at_ts, g.before.dimmed(), g.after.dimmed());
        }
    }

    // last activity — ALWAYS shown so an unclassified end-state still reveals what it was
    // doing when the log stopped (the line before a wedge/crash is the lead we want).
    println!("\n{}", "── LAST ACTIVITY (end of log) ─────────────────".cyan().bold());
    for l in &r.tail { println!("  {}", l.dimmed()); }

    println!("{}", bar.dimmed());
}

#[cfg(test)]
mod tests {
    use super::parse_build_line;

    #[test]
    fn build_full_sha() {
        let a = parse_build_line(
            "  BUILD wad=vz-patch.wad sha256=66359a9842bb787187eebdaae482e02d561a8e85487fa13989a5add8531dc050 size=263913472"
        ).expect("parses");
        assert_eq!(a.kind, "wad");
        assert_eq!(a.name, "vz-patch.wad");
        assert_eq!(a.hash_type, "sha256");
        assert_eq!(a.sha256, "66359a9842bb787187eebdaae482e02d561a8e85487fa13989a5add8531dc050");
        assert_eq!(a.size, Some(263913472));
    }

    #[test]
    fn build_quick_hash_big_wad() {
        let a = parse_build_line(
            "  BUILD wad=vz.wad qsha256=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef size=2565537792"
        ).expect("parses");
        assert_eq!(a.kind, "wad");
        assert_eq!(a.name, "vz.wad");
        assert_eq!(a.hash_type, "qsha256");
        assert_eq!(a.size, Some(2565537792));
    }

    #[test]
    fn build_unreadable_and_non_build() {
        let a = parse_build_line("BUILD dll=locked.dll sha256=UNREADABLE").expect("parses");
        assert_eq!(a.kind, "dll");
        assert_eq!(a.sha256, "UNREADABLE");
        assert_eq!(a.size, None);
        assert!(parse_build_line("Loading vz level with vz masterscript").is_none());
    }
}
