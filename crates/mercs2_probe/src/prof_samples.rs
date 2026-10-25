//! `prof-samples`: where the game's threads spent their time in a profiler session, from the session's
//! `<session>.samples` (`mercs2_formats::prof_samples`): one report per sampled thread, then, for the
//! ticks the main thread was outside game code, what each other thread was doing in the same tick.
//!
//! The samples name the exe they came from by sha256. Its build (`mercs2_exemap` catalogue) gives its
//! code layout; the code of that layout is the image of the layout's memory dump (a catalogued build
//! with no recipe, `images.tsv`), whose bytes are read from the canonical tree and whose functions are
//! that image's Ghidra export (`<exports>/<image>.functions.jsonl`, names from `<image>.annotate.jsonl`).
//!
//! A sample's EIP is its leaf. Its stack words are candidates; one counts as a return address only when
//! the bytes before it decode as a call ending exactly there ([`is_return_address`]): `E8 rel32` whose
//! target is in a code section, or `FF /2` with any ModRM/SIB/displacement form. A stale return address
//! left in a stack slot passes that test too, so a direct call is also chain-checked ([`resolve`]): its
//! target names the frame it called. When that is the leaf or a frame already on the chain, every frame
//! accepted after it was stale and is removed; when it is a thunk or code in another section than the
//! frame inside (SecuROM's split-code forwarders jump back into `.text`), the link cannot be checked and
//! the frame is kept; otherwise the candidate is stale. Every removed or refused frame is counted. Indirect
//! calls (`FF /2`) cannot be checked either, so the inclusive counts stay an upper bound; the self counts
//! (EIP) are exact. Samples whose EIP is outside the game's
//! code are also tallied by the innermost game function on their stack (the code that called out) and by
//! the module EIP was in (the samples header lists the modules loaded when the session started).

use mercs2_exemap::annotate::read_functions;
use mercs2_exemap::canonical::{build_path, builds_by_sha};
use mercs2_exemap::images::Images;
use mercs2_exemap::Catalogue;
use mercs2_formats::exe_image::Image;
use mercs2_formats::game_paths::local_config_canonical_dir;
use mercs2_formats::prof_samples::{self, Header, Record};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const USAGE: &str =
    "usage: mercs2_probe prof-samples <session.samples> --exports <ghidra exports dir> [--top N] [--folded <out.txt>] [--also <image_id>] [--pair <va>]...";
/// The leaf of a sample whose EIP is outside the game's code (a system DLL, d3d9, the driver, Wine).
pub const OUTSIDE: &str = "[outside game code]";

/// Whether the bytes of `img` before `ra` are a call instruction ending at `ra`.
pub fn is_return_address(img: &Image, ra: u32) -> bool {
    let code = img.code_ranges();
    let in_code = |va: u32| code.iter().any(|&(s, e)| va >= s && va < e);
    if let Some(b) = ra.checked_sub(5).and_then(|at| img.bytes_at(at)) {
        if b.len() >= 5 && b[0] == 0xe8 {
            let rel = i32::from_le_bytes([b[1], b[2], b[3], b[4]]);
            if in_code(ra.wrapping_add(rel as u32)) {
                return true;
            }
        }
    }
    for len in [2u32, 3, 4, 6, 7] {
        let Some(b) = ra.checked_sub(len).and_then(|at| img.bytes_at(at)) else { continue };
        if b.len() < len as usize || b[0] != 0xff || (b[1] >> 3) & 7 != 2 {
            continue;
        }
        if call_ff_len(&b[..len as usize]) == Some(len) {
            return true;
        }
    }
    false
}

/// The length of the `FF /2` (`call r/m32`) starting `b`, from its ModRM, SIB and displacement.
fn call_ff_len(b: &[u8]) -> Option<u32> {
    let modrm = *b.get(1)?;
    let (md, rm) = (modrm >> 6, modrm & 7);
    if md == 3 {
        return Some(2);
    }
    let sib = rm == 4;
    let disp = match md {
        0 if rm == 5 => 4,
        0 if sib && (*b.get(2)? & 7) == 5 => 4,
        0 => 0,
        1 => 1,
        _ => 4,
    };
    Some(2 + u32::from(sib) + disp)
}

/// The target of the `E8 rel32` that ends at `ra`, when that is the instruction before it.
pub fn direct_target(img: &Image, ra: u32) -> Option<u32> {
    let b = img.bytes_at(ra.checked_sub(5)?)?;
    (b.len() >= 5 && b[0] == 0xe8).then(|| ra.wrapping_add(i32::from_le_bytes([b[1], b[2], b[3], b[4]]) as u32))
}

/// One function for [`Functions`].
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub name: String,
    pub thunk: bool,
}

/// Function lookup: `[start, end)` sorted by start, with each one's name and whether it is a thunk.
pub struct Functions {
    spans: Vec<Span>,
}

impl Functions {
    pub fn new(mut spans: Vec<Span>) -> Self {
        spans.sort_by_key(|s| s.start);
        Functions { spans }
    }

    /// The function whose body holds `va`.
    pub fn at(&self, va: u32) -> Option<usize> {
        let i = self.spans.partition_point(|s| s.start <= va).checked_sub(1)?;
        let s = &self.spans[i];
        (va >= s.start && va < s.end).then_some(i)
    }

    pub fn name(&self, id: usize) -> &str {
        &self.spans[id].name
    }

    pub fn start(&self, id: usize) -> u32 {
        self.spans[id].start
    }

    pub fn is_thunk(&self, id: usize) -> bool {
        self.spans[id].thunk
    }
}

/// One sample, resolved: its leaf (`None` outside game code), its callers, innermost first, and how many
/// return-address candidates the chain check dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    pub leaf: Option<usize>,
    pub callers: Vec<usize>,
    pub dropped: u32,
}

pub fn resolve(img: &Image, fns: &Functions, rec: &Record) -> Stack {
    let leaf = fns.at(rec.eip);
    let mut callers: Vec<usize> = Vec::new();
    let mut dropped = 0;
    for &w in &rec.words {
        if !is_return_address(img, w) {
            continue;
        }
        let Some(f) = fns.at(w) else { continue };
        let inner = callers.last().copied().or(leaf);
        if let (Some(t), Some(inner)) = (direct_target(img, w), inner) {
            let tf = fns.at(t);
            if tf.is_some() && tf == leaf {
                // It called the leaf: every caller kept so far was stale.
                dropped += callers.len() as u32;
                callers.clear();
            } else if let Some(k) = tf.and_then(|t| callers.iter().rposition(|&c| c == t)) {
                // It called a frame on the chain: the frames kept after that one were stale.
                dropped += (callers.len() - k - 1) as u32;
                callers.truncate(k + 1);
            } else if !(tf.is_some_and(|t| fns.is_thunk(t)) || img.section_of(t) != img.section_of(fns.start(inner))) {
                dropped += 1;
                continue;
            }
        }
        // A function calling itself shows once per run.
        if callers.last() != Some(&f) && !(callers.is_empty() && leaf == Some(f)) {
            callers.push(f);
        }
    }
    Stack { leaf, callers, dropped }
}

/// The tallies of one thread over a session.
#[derive(Debug, Default)]
pub struct Tally {
    pub samples: u64,
    pub outside: u64,
    pub with_caller: u64,
    pub dropped: u64,
    pub self_: HashMap<Option<usize>, u64>,
    pub inclusive: HashMap<usize, u64>,
    /// Samples outside game code, by the innermost game function on the stack (`None`: no caller found).
    pub called_out: HashMap<Option<usize>, u64>,
    /// Samples outside game code, by the module EIP was in.
    pub modules: HashMap<String, u64>,
    pub folded: BTreeMap<String, u64>,
}

/// The module name of an EIP outside the game's code, as a frame: `[ntdll.dll]`, or [`OUTSIDE`] when no
/// module listed in the samples header holds it.
pub fn outside_frame(header: &Header, eip: u32) -> String {
    header.module_of(eip).map_or_else(|| OUTSIDE.to_string(), |m| format!("[{}]", m.name))
}

/// One sample's state in a line: its leaf function, or the module outside game code and the game function
/// that called out to it.
pub fn state(fns: &Functions, header: &Header, rec: &Record, s: &Stack) -> String {
    match (s.leaf, s.callers.first()) {
        (Some(f), _) => fns.name(f).to_string(),
        (None, Some(&c)) => format!("{} from {}", outside_frame(header, rec.eip), fns.name(c)),
        (None, None) => format!("{} from no game caller", outside_frame(header, rec.eip)),
    }
}

impl Tally {
    /// `outside` is the leaf frame used when EIP is outside game code ([`outside_frame`]).
    pub fn add(&mut self, fns: &Functions, s: &Stack, outside: &str) {
        self.samples += 1;
        self.dropped += u64::from(s.dropped);
        if s.leaf.is_none() {
            self.outside += 1;
            *self.called_out.entry(s.callers.first().copied()).or_default() += 1;
            *self.modules.entry(outside.to_string()).or_default() += 1;
        }
        if !s.callers.is_empty() {
            self.with_caller += 1;
        }
        *self.self_.entry(s.leaf).or_default() += 1;
        let mut seen = HashSet::new();
        for f in s.leaf.iter().chain(&s.callers) {
            if seen.insert(*f) {
                *self.inclusive.entry(*f).or_default() += 1;
            }
        }
        let leaf = s.leaf.map_or(outside.replace(';', ":"), |f| fns.name(f).replace(';', ":"));
        let mut frames: Vec<String> = s.callers.iter().rev().map(|&f| fns.name(f).replace(';', ":")).collect();
        frames.push(leaf);
        *self.folded.entry(frames.join(";")).or_default() += 1;
    }
}

/// The flags that take a value.
const VALUE_FLAGS: [&str; 5] = ["--exports", "--top", "--folded", "--also", "--pair"];

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// Every value of a flag given more than once.
fn flags(args: &[String], name: &str) -> Vec<String> {
    args.windows(2).filter(|w| w[0] == name).map(|w| w[1].clone()).collect()
}

/// The first argument that is neither a flag nor a flag's value.
fn positional(args: &[String]) -> Option<&String> {
    let mut skip = false;
    args.iter().find(|a| {
        if std::mem::take(&mut skip) {
            return false;
        }
        if VALUE_FLAGS.contains(&a.as_str()) {
            skip = true;
            return false;
        }
        !a.starts_with("--")
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn parse_va(s: &str, what: &str, line: usize) -> Result<u32, String> {
    u32::from_str_radix(s.trim_start_matches("0x"), 16).map_err(|e| format!("{what} line {line}: va {s:?}: {e}"))
}

/// The best name of each function (the first name not from Ghidra's default naming, else Ghidra's), and
/// its agreed translation into image `also`, when asked for and listed.
fn names_of(annotate: &str, what: &str, also: Option<&str>) -> Result<HashMap<u32, (String, Option<u32>)>, String> {
    #[derive(serde::Deserialize)]
    struct N {
        name: String,
        source: String,
    }
    #[derive(serde::Deserialize)]
    struct T {
        image: String,
        va: String,
    }
    #[derive(serde::Deserialize)]
    struct R {
        va: Option<String>,
        names: Option<Vec<N>>,
        translations: Option<Vec<T>>,
    }
    let mut out = HashMap::new();
    for (i, line) in annotate.lines().enumerate() {
        let r: R = serde_json::from_str(line).map_err(|e| format!("{what} line {}: {e}", i + 1))?;
        let (Some(va), Some(names)) = (r.va, r.names) else { continue };
        let va = parse_va(&va, what, i + 1)?;
        let Some(best) = names.iter().find(|n| n.source != "ghidra:DEFAULT").or(names.first()) else { continue };
        let other = match (also, &r.translations) {
            (Some(img), Some(ts)) => ts.iter().find(|t| t.image == img).map(|t| parse_va(&t.va, what, i + 1)).transpose()?,
            _ => None,
        };
        out.insert(va, (best.name.clone(), other));
    }
    Ok(out)
}

pub fn main(args: &[String]) -> i32 {
    match run(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("prof-samples: {e}");
            1
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let samples = positional(args).ok_or(USAGE)?;
    let exports = PathBuf::from(flag(args, "--exports").ok_or(USAGE)?);
    let top: usize = match flag(args, "--top") {
        Some(t) => t.parse().map_err(|e| format!("--top {t:?}: {e}"))?,
        None => 30,
    };
    let pairs: Vec<u32> = flags(args, "--pair").iter().map(|v| parse_va(v, "--pair", 0)).collect::<Result<_, _>>()?;

    let bytes = std::fs::read(samples).map_err(|e| format!("reading {samples}: {e}"))?;
    let (header, records) = prof_samples::decode(&bytes).map_err(|e| format!("{samples}: {e}"))?;
    let (img_bytes, image_id, describe) = code_image(&header)?;
    let img = Image::from_file(&img_bytes).map_err(|e| format!("{image_id}: {e}"))?;

    let fpath = exports.join(format!("{image_id}.functions.jsonl"));
    let ftext = std::fs::read_to_string(&fpath).map_err(|e| format!("reading {}: {e}", fpath.display()))?;
    let file = read_functions(&ftext, &fpath.display().to_string()).map_err(|e| e.to_string())?;
    if file.image_id != image_id {
        return Err(format!("{} is the export of {}, not {image_id}", fpath.display(), file.image_id));
    }
    let apath = exports.join(format!("{image_id}.annotate.jsonl"));
    let atext = std::fs::read_to_string(&apath).map_err(|e| format!("reading {}: {e}", apath.display()))?;
    let also = flag(args, "--also");
    let names = names_of(&atext, &apath.display().to_string(), also.as_deref())?;
    let fns = Functions::new(
        file.functions
            .iter()
            .map(|f| {
                let (n, other) = names.get(&f.va).cloned().unwrap_or_else(|| (f.name.clone(), None));
                let mut label = if n.starts_with("FUN_") { n } else { format!("{n} ({:#x})", f.va) };
                if let Some(img) = &also {
                    label += &match other {
                        Some(v) => format!(" [{img} {v:#010x}]"),
                        None => format!(" [{img} -]"),
                    };
                }
                Span { start: f.va, end: f.va + f.size.max(1), name: label, thunk: f.thunk }
            })
            .collect(),
    );

    // Each thread's tally, and each sample's state by tick for the same-tick table.
    let mut tallies: HashMap<u32, Tally> = header.threads.iter().map(|t| (t.tid, Tally::default())).collect();
    let mut at_tick: HashMap<u32, Vec<(u32, Stack, String)>> = HashMap::new();
    let mut exited: Vec<(u32, u32)> = Vec::new();
    for r in &records {
        if r.exited() {
            exited.push((r.tid, r.tick));
            continue;
        }
        let s = resolve(&img, &fns, r);
        let tally = tallies.get_mut(&r.tid).expect("prof_samples::decode refuses a record of an unlisted thread");
        tally.add(&fns, &s, &outside_frame(&header, r.eip));
        let line = state(&fns, &header, r, &s);
        at_tick.entry(r.tick).or_default().push((r.tid, s, line));
    }
    let main = header.main_thread;
    let main_samples = tallies[&main].samples;
    if main_samples == 0 {
        return Err(format!("{samples} holds no sample of the main thread {main}"));
    }
    let of_main: Vec<&Record> = records.iter().filter(|r| r.tid == main && !r.exited()).collect();
    let (first, last) = (of_main[0], of_main[of_main.len() - 1]);
    let secs = last.qpc.saturating_sub(first.qpc) as f64 / header.qpc_frequency as f64;
    let frames = last.frame.saturating_sub(first.frame).max(1);
    let ticks = last.tick.wrapping_sub(first.tick).wrapping_add(1);
    let start_of = |va: u32| match fns.at(va) {
        Some(f) => fns.name(f).to_string(),
        None => header.module_of(va).map_or_else(|| format!("{va:#x}"), |m| format!("{}+{:#x}", m.name, va - m.base)),
    };

    println!("samples: {samples}");
    println!("exe sha256 {} -> {describe}", hex(&header.exe_sha256));
    println!(
        "{ticks} ticks over {secs:.1} s ({:.0} Hz achieved, {} µs asked), {frames} frames ({:.1} ms/frame); {} threads sampled",
        f64::from(ticks) / secs.max(f64::EPSILON),
        header.period_us,
        secs * 1000.0 / f64::from(frames),
        header.threads.len()
    );
    let mut order: Vec<u32> = header.threads.iter().map(|t| t.tid).collect();
    order.sort_by_key(|&t| (t != main, std::cmp::Reverse(tallies[&t].samples)));
    for &tid in &order {
        let t = header.threads.iter().find(|t| t.tid == tid).expect("listed");
        let gone = exited.iter().find(|e| e.0 == tid).map_or(String::new(), |e| format!("; exited by tick {}", e.1));
        let main_mark = if tid == main { " (main thread)" } else { "" };
        println!("  thread {tid:>6}{main_mark}: started at {}; {} samples{gone}", start_of(t.start), tallies[&tid].samples);
    }

    for &tid in &order {
        let tally = &tallies[&tid];
        if tally.samples == 0 {
            continue;
        }
        let t = header.threads.iter().find(|t| t.tid == tid).expect("listed");
        // Each sample of a thread stands for the time between its ticks.
        let ms_per_sample = secs * 1000.0 / f64::from(ticks);
        let ms_per_frame = |n: u64| n as f64 * ms_per_sample / f64::from(frames);
        let pct = |n: u64| 100.0 * n as f64 / tally.samples as f64;
        let main_mark = if tid == main { ", the main thread" } else { "" };
        println!("\n==== thread {tid} ({}{main_mark}) ====", start_of(t.start));
        println!(
            "EIP outside game code: {:.1}%   samples with a validated caller: {:.1}%   direct calls dropped by the chain check: {} ({:.2} per sample)",
            pct(tally.outside),
            pct(tally.with_caller),
            tally.dropped,
            tally.dropped as f64 / tally.samples as f64
        );
        let table = |title: &str, rows: Vec<(String, u64)>| {
            println!("\n{title:<72} samples      %   ms/frame");
            let mut rows = rows;
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            for (name, n) in rows.into_iter().take(top) {
                println!("  {name:<68} {n:>9} {:>6.2} {:>9.2}", pct(n), ms_per_frame(n));
            }
        };
        table(
            "self (the function EIP was in)",
            tally.self_.iter().map(|(f, n)| (f.map_or_else(|| "[outside game code]".to_string(), |f| fns.name(f).to_string()), *n)).collect(),
        );
        table("inclusive (the function anywhere in the validated stack)", tally.inclusive.iter().map(|(f, n)| (fns.name(*f).to_string(), *n)).collect());
        table(
            "outside game code, by the game function that called out",
            tally.called_out.iter().map(|(f, n)| (f.map_or_else(|| "[no game caller found]".to_string(), |f| fns.name(f).to_string()), *n)).collect(),
        );
        table("outside game code, by the module EIP was in", tally.modules.iter().map(|(m, n)| (m.clone(), *n)).collect());
    }

    // What the other threads were doing in the ticks the main thread was outside game code, by the game
    // function the main thread called out from.
    let mut by_caller: HashMap<Option<usize>, (u64, HashMap<u32, HashMap<String, u64>>)> = HashMap::new();
    for samples_of_tick in at_tick.values() {
        let Some((_, ms, _)) = samples_of_tick.iter().find(|(t, _, _)| *t == main) else { continue };
        if ms.leaf.is_some() {
            continue;
        }
        let entry = by_caller.entry(ms.callers.first().copied()).or_default();
        entry.0 += 1;
        for (tid, _, line) in samples_of_tick.iter().filter(|(t, _, _)| *t != main) {
            *entry.1.entry(*tid).or_default().entry(line.clone()).or_default() += 1;
        }
    }
    let mut callers: Vec<_> = by_caller.into_iter().collect();
    callers.sort_by(|a, b| b.1.0.cmp(&a.1.0));
    if order.len() > 1 {
        println!("\n==== at the same tick: what the other threads did while the main thread was outside game code ====");
        for (caller, (n, threads)) in callers.into_iter().take(SAME_TICK_CALLERS) {
            let name = caller.map_or("[no game caller found]", |f| fns.name(f));
            println!("\nmain thread outside game code, called out from {name}: {n} ticks ({:.1}% of the main thread's samples)", 100.0 * n as f64 / main_samples as f64);
            for &tid in order.iter().filter(|&&t| t != main) {
                let Some(lines) = threads.get(&tid) else {
                    println!("  thread {tid:>6}: no sample in those ticks");
                    continue;
                };
                let seen: u64 = lines.values().sum();
                println!("  thread {tid:>6}: sampled in {seen} of those ticks");
                let mut rows: Vec<_> = lines.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
                for (line, k) in rows.into_iter().take(SAME_TICK_STATES) {
                    println!("    {line:<100} {k:>8} {:>6.1}%", 100.0 * *k as f64 / seen as f64);
                }
            }
        }
    }

    // What the other threads were doing in the ticks the main thread had each `--pair` function on its
    // validated stack.
    for va in pairs {
        let f = fns.at(va).ok_or_else(|| format!("--pair {va:#x} is not inside a function of {image_id}"))?;
        let mut n = 0u64;
        let mut threads: HashMap<u32, HashMap<String, u64>> = HashMap::new();
        for samples_of_tick in at_tick.values() {
            let Some((_, ms, _)) = samples_of_tick.iter().find(|(t, _, _)| *t == main) else { continue };
            if !(ms.leaf == Some(f) || ms.callers.contains(&f)) {
                continue;
            }
            n += 1;
            for (tid, _, line) in samples_of_tick.iter().filter(|(t, _, _)| *t != main) {
                *threads.entry(*tid).or_default().entry(line.clone()).or_default() += 1;
            }
        }
        println!(
            "\n==== at the same tick: what the other threads did while the main thread was in {} ====\n{n} ticks ({:.1}% of the main thread's samples)",
            fns.name(f),
            100.0 * n as f64 / main_samples as f64
        );
        for &tid in order.iter().filter(|&&t| t != main) {
            let Some(lines) = threads.get(&tid) else {
                println!("  thread {tid:>6}: no sample in those ticks");
                continue;
            };
            let seen: u64 = lines.values().sum();
            println!("  thread {tid:>6}: sampled in {seen} of those ticks");
            let mut rows: Vec<_> = lines.iter().collect();
            rows.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (line, k) in rows.into_iter().take(SAME_TICK_STATES) {
                println!("    {line:<100} {k:>8} {:>6.1}%", 100.0 * *k as f64 / seen as f64);
            }
        }
    }

    if let Some(out) = flag(args, "--folded") {
        let mut text = String::new();
        for &tid in &order {
            for (k, n) in &tallies[&tid].folded {
                text += &format!("thread {tid};{k} {n}\n");
            }
        }
        std::fs::write(&out, text).map_err(|e| format!("writing {out}: {e}"))?;
        println!("\nfolded stacks, one root per thread -> {out}");
    }
    Ok(())
}

/// How many of the main thread's call-out sites the same-tick table shows, and how many states per thread.
const SAME_TICK_CALLERS: usize = 6;
const SAME_TICK_STATES: usize = 6;

/// The bytes, id and description of the image holding the code of the exe the samples came from.
fn code_image(h: &Header) -> Result<(Vec<u8>, String, String), String> {
    let sha = hex(&h.exe_sha256);
    let cat = Catalogue::embedded().map_err(|e| e.to_string())?;
    let run = cat.build_by_sha(&sha).ok_or_else(|| format!("the samples came from exe {sha}, which is not a catalogued build"))?;
    let images = Images::embedded(&cat).map_err(|e| e.to_string())?;
    let dumps: Vec<_> = images
        .code_images()
        .filter_map(|i| {
            let b = cat.build(i.build_id.as_deref()?)?;
            (b.layout == run.layout && b.recipes.is_empty()).then_some((i, b))
        })
        .collect();
    let [(image, build)] = dumps[..] else {
        return Err(format!(
            "build {} (layout {}) needs exactly one image of a memory dump of that layout, found {}: {:?}",
            run.id,
            run.layout,
            dumps.len(),
            dumps.iter().map(|(i, _)| i.id.as_str()).collect::<Vec<_>>()
        ));
    };
    let dir = local_config_canonical_dir(&std::env::current_dir().map_err(|e| e.to_string())?)?;
    let files = builds_by_sha(&cat, &dir).map_err(|e| e.to_string())?;
    let path = build_path(&cat, &files, &dir, &build.id).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let describe = format!(
        "build {} (code {}, layout {}); code image {} ({})",
        run.id,
        run.code_version.as_str(),
        run.layout,
        image.id,
        Path::new(path).display()
    );
    Ok((bytes, image.id.clone(), describe))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-section code image at 0x401000 holding `code`, marked as code.
    fn image(code: Vec<u8>) -> Vec<u8> {
        let mut f = vec![0u8; 0x400];
        f[0..2].copy_from_slice(b"MZ");
        f[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        f[0x80..0x84].copy_from_slice(b"PE\0\0");
        f[0x86..0x88].copy_from_slice(&1u16.to_le_bytes());
        f[0x94..0x96].copy_from_slice(&0xe0u16.to_le_bytes());
        f[0x98..0x9a].copy_from_slice(&0x10bu16.to_le_bytes());
        f[0xb4..0xb8].copy_from_slice(&0x400000u32.to_le_bytes());
        let h = 0x98 + 0xe0;
        f[h..h + 5].copy_from_slice(b".text");
        f[h + 8..h + 12].copy_from_slice(&(code.len() as u32).to_le_bytes());
        f[h + 12..h + 16].copy_from_slice(&0x1000u32.to_le_bytes());
        f[h + 16..h + 20].copy_from_slice(&(code.len() as u32).to_le_bytes());
        f[h + 20..h + 24].copy_from_slice(&0x400u32.to_le_bytes());
        f[h + 36..h + 40].copy_from_slice(&0x6000_0020u32.to_le_bytes());
        f.extend(code);
        f
    }

    #[test]
    fn calls_of_every_form_end_at_their_return_address() {
        // 0x401000: call rel32 to 0x401100 (in code); 0x401005: call eax; 0x401007: call [esi+0x14];
        // 0x40100a: call [esp+8] (SIB, disp8); 0x40100e: call [0x401200]; 0x401014: call [eax*4+0x401300];
        // 0x40101b: call rel32 out of the image; 0x401020: data.
        let mut c = vec![0xe8];
        c.extend(0xfbu32.to_le_bytes()); // 0x401005 + 0xfb = 0x401100
        c.extend([0xff, 0xd0]);
        c.extend([0xff, 0x56, 0x14]);
        c.extend([0xff, 0x54, 0x24, 0x08]);
        c.extend([0xff, 0x15]);
        c.extend(0x401200u32.to_le_bytes());
        c.extend([0xff, 0x14, 0x85]);
        c.extend(0x401300u32.to_le_bytes());
        c.extend([0xe8]);
        c.extend(0x7000_0000u32.to_le_bytes());
        c.resize(0x400, 0x90);
        let file = image(c);
        let img = Image::from_file(&file).unwrap();
        for ra in [0x401005, 0x401007, 0x40100a, 0x40100e, 0x401014, 0x40101b] {
            assert!(is_return_address(&img, ra), "{ra:#x}");
        }
        assert!(!is_return_address(&img, 0x401020), "the call before it leaves the image");
        assert!(!is_return_address(&img, 0x401030), "nops");
        assert!(!is_return_address(&img, 0x401001), "inside an instruction");
    }

    fn span(start: u32, end: u32, name: &str) -> Span {
        Span { start, end, name: name.into(), thunk: false }
    }

    /// `rel32` of an `E8` at `at` calling `target`.
    fn call(c: &mut [u8], at: u32, target: u32) {
        let i = (at - 0x401000) as usize;
        c[i] = 0xe8;
        c[i + 1..i + 5].copy_from_slice(&target.wrapping_sub(at + 5).to_le_bytes());
    }

    #[test]
    fn stacks_resolve_and_tally() {
        let mut c = vec![0x90; 0x400];
        // f0 at 0x401000..0x401040 calls f1 (call at 0x401010, returns to 0x401015).
        call(&mut c, 0x401010, 0x401040);
        let file = image(c);
        let img = Image::from_file(&file).unwrap();
        let fns = Functions::new(vec![span(0x401000, 0x401040, "f0"), span(0x401040, 0x401080, "f1")]);
        let in_f1 = rec(0x401050, vec![0x401020, 0x401015]);
        let s = resolve(&img, &fns, &in_f1);
        assert_eq!(s, Stack { leaf: fns.at(0x401050), callers: vec![fns.at(0x401000).unwrap()], dropped: 0 }, "0x401020 is not after a call");
        let h = header();
        let outside = rec(0x7bc0_0010, vec![0x401015]);
        let unlisted = rec(0x6600_0000, vec![]);
        let mut t = Tally::default();
        t.add(&fns, &s, &outside_frame(&h, in_f1.eip));
        let so = resolve(&img, &fns, &outside);
        t.add(&fns, &so, &outside_frame(&h, outside.eip));
        let su = resolve(&img, &fns, &unlisted);
        t.add(&fns, &su, &outside_frame(&h, unlisted.eip));
        assert_eq!((t.samples, t.outside, t.with_caller), (3, 2, 2));
        assert_eq!(t.inclusive[&fns.at(0x401000).unwrap()], 2);
        assert_eq!(t.called_out[&fns.at(0x401000)], 1);
        assert_eq!(t.modules["[ntdll.dll]"], 1);
        assert_eq!(t.modules[OUTSIDE], 1);
        assert_eq!(t.folded.get("f0;f1"), Some(&1));
        assert_eq!(t.folded.get("f0;[ntdll.dll]"), Some(&1));
        assert_eq!(state(&fns, &h, &in_f1, &s), "f1");
        assert_eq!(state(&fns, &h, &outside, &so), "[ntdll.dll] from f0");
        assert_eq!(state(&fns, &h, &unlisted, &su), format!("{OUTSIDE} from no game caller"));
    }

    fn rec(eip: u32, words: Vec<u32>) -> Record {
        Record { qpc: 0, tick: 0, frame: 0, tid: 1, eip, flags: 0, words }
    }

    fn header() -> Header {
        Header {
            exe_sha256: [0; 32],
            image_base: 0x40_0000,
            qpc_frequency: 10_000_000,
            period_us: 4000,
            main_thread: 1,
            threads: vec![prof_samples::Thread { tid: 1, start: 0x401000 }],
            modules: vec![prof_samples::Module { base: 0x7bc0_0000, size: 0x1000, name: "ntdll.dll".into() }],
            code: vec![(0x401000, 0x401400)],
        }
    }

    #[test]
    fn a_direct_call_into_another_function_is_a_stale_frame() {
        let mut c = vec![0x90; 0x400];
        // f0 calls f1 (returns 0x401015); f2 calls f3 (returns 0x401095), a stale slot above f1's frame.
        call(&mut c, 0x401010, 0x401040);
        call(&mut c, 0x401090, 0x4010c0);
        let file = image(c);
        let img = Image::from_file(&file).unwrap();
        let fns = Functions::new(vec![
            span(0x401000, 0x401040, "f0"),
            span(0x401040, 0x401080, "f1"),
            span(0x401080, 0x4010c0, "f2"),
            span(0x4010c0, 0x401100, "f3"),
        ]);
        let in_f1 = rec(0x401050, vec![0x401095, 0x401015]);
        let s = resolve(&img, &fns, &in_f1);
        assert_eq!(s.callers, vec![fns.at(0x401000).unwrap()], "f2 called f3, not f1");
        assert_eq!(s.dropped, 1);
        // A call into a thunk cannot be checked, so f2 is kept at first; f0's verified call into the leaf
        // f1 then shows f2 was stale.
        let mut spans = vec![span(0x401000, 0x401040, "f0"), span(0x401040, 0x401080, "f1"), span(0x401080, 0x4010c0, "f2")];
        spans.push(Span { start: 0x4010c0, end: 0x401100, name: "f3".into(), thunk: true });
        let fns = Functions::new(spans);
        let s = resolve(&img, &fns, &in_f1);
        assert_eq!((s.callers, s.dropped), (vec![fns.at(0x401000).unwrap()], 1));
        // Without a verified link after it, a thunk-reached frame stays.
        let only_stale = rec(0x401050, vec![0x401095]);
        assert_eq!(resolve(&img, &fns, &only_stale).callers, vec![fns.at(0x401080).unwrap()]);
    }

    #[test]
    fn the_samples_path_is_the_argument_no_flag_takes() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(positional(&a(&["--exports", "dir", "x.samples"])).map(String::as_str), Some("x.samples"));
        assert_eq!(positional(&a(&["x.samples", "--top", "5"])).map(String::as_str), Some("x.samples"));
        assert_eq!(positional(&a(&["--folded", "out.txt", "--exports", "d"])), None);
        assert_eq!(positional(&a(&["--pair", "0x8765cf", "x.samples"])).map(String::as_str), Some("x.samples"));
        assert_eq!(flags(&a(&["--pair", "0x1", "x", "--pair", "0x2"]), "--pair"), ["0x1", "0x2"]);
    }

    #[test]
    fn a_function_is_found_only_inside_its_body() {
        let fns = Functions::new(vec![span(0x401040, 0x401050, "b"), span(0x401000, 0x401010, "a")]);
        assert_eq!(fns.at(0x401000).map(|f| fns.name(f)), Some("a"));
        assert_eq!(fns.at(0x40100f).map(|f| fns.name(f)), Some("a"));
        assert_eq!(fns.at(0x401010), None, "a gap between functions");
        assert_eq!(fns.at(0x401045).map(|f| fns.name(f)), Some("b"));
        assert_eq!(fns.at(0x400fff), None);
    }
}
