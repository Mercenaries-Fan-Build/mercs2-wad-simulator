//! Dev bin: dump every localized string out of a WAD's `stringdb` containers.
//!
//! The fix-pack needs this: community bug reports name a string by the text the player SEES,
//! but the engine stores only `pandemic_hash_m2("[OilCon001.Objectives.001]") -> offset`. To
//! locate a reported typo you need the full text index first; the key name is recovered
//! separately (the Lua corpus quotes the bracket keys verbatim).
//!
//! Layout per `docs/format_reference.md` §4.1 — SYEK = key table, SRTS = string heap, both
//! documented as natively BIG-endian on all platforms. That claim is CONTESTED by
//! `tools/build_shell_string_patch.py`, which does UTF-16**LE** replacements on shell.wad, so
//! this tool DETECTS the endianness per chunk and reports it rather than trusting either.
//!
//!   cargo run -p mercs2_probe --bin stringdb_dump -- --wad <wad> [--filter english] [--out f.tsv]

use std::collections::BTreeMap;
use std::fs::File;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_STRINGDB;
use mercs2_formats::ucfx::{extract_chunk_body, walk_decompressed_block};

#[derive(Clone, Copy, PartialEq, Debug)]
enum End {
    Be,
    Le,
}

impl End {
    fn u32(self, d: &[u8], off: usize) -> u32 {
        let b = [d[off], d[off + 1], d[off + 2], d[off + 3]];
        match self {
            End::Be => u32::from_be_bytes(b),
            End::Le => u32::from_le_bytes(b),
        }
    }
    fn u16(self, d: &[u8], off: usize) -> u16 {
        let b = [d[off], d[off + 1]];
        match self {
            End::Be => u16::from_be_bytes(b),
            End::Le => u16::from_le_bytes(b),
        }
    }
    fn name(self) -> &'static str {
        match self {
            End::Be => "BE",
            End::Le => "LE",
        }
    }
}

/// Pick the endianness under which `count` is a plausible entry count for the buffer.
/// Returns None when neither reading fits — that means the chunk is not what we think it is.
fn detect_header_endian(body: &[u8], bytes_per_entry: usize) -> Option<(End, u32)> {
    if body.len() < 4 {
        return None;
    }
    let fits = |e: End| {
        let n = e.u32(body, 0) as usize;
        // The table must fit in the body. A zero count is legal but uninformative, so a
        // non-zero count that fits is always preferred over a zero one.
        n.checked_mul(bytes_per_entry)
            .map_or(false, |need| 4 + need <= body.len())
            .then_some(n)
    };
    match (fits(End::Be), fits(End::Le)) {
        (Some(b), Some(l)) => {
            // Both fit: the tighter fit is the real one (the loose one is a small number
            // byte-swapped into a smaller number, which trivially "fits").
            if b >= l {
                Some((End::Be, b as u32))
            } else {
                Some((End::Le, l as u32))
            }
        }
        (Some(b), None) => Some((End::Be, b as u32)),
        (None, Some(l)) => Some((End::Le, l as u32)),
        (None, None) => None,
    }
}

/// Read a NUL-terminated UTF-16 string at `off` within the heap.
fn read_utf16(heap: &[u8], off: usize, e: End) -> Option<String> {
    let mut units = Vec::new();
    let mut p = off;
    loop {
        if p + 2 > heap.len() {
            // Unterminated at end of heap — take what we have rather than dropping the row.
            break;
        }
        let u = e.u16(heap, p);
        p += 2;
        if u == 0 {
            break;
        }
        units.push(u);
    }
    Some(char::decode_utf16(units).map(|r| r.unwrap_or('\u{FFFD}')).collect())
}

/// How ASCII-like is this decode? Used to settle BE vs LE for the text heap independently
/// of the header, because the two are documented inconsistently.
fn ascii_score(heap: &[u8], e: End) -> usize {
    let mut score = 0usize;
    let n = heap.len().min(4096) / 2;
    for i in 0..n {
        let u = e.u16(heap, i * 2);
        if (0x20..0x7F).contains(&u) || u == 0 || u == 0x0A {
            score += 1;
        }
    }
    score
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n").replace('\r', "\\r")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut wad = String::new();
    let mut filter = String::new();
    let mut out: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--wad" => {
                wad = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--filter" => {
                filter = args.get(i + 1).cloned().unwrap_or_default().to_lowercase();
                i += 2;
            }
            "--out" => {
                out = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }
    if wad.is_empty() {
        eprintln!(
            "usage: stringdb_dump --wad <wad> [--filter <path-substr>] [--out <tsv>]\n\
             \n\
             Dumps key_hash -> localized text for every stringdb container in the WAD."
        );
        std::process::exit(2);
    }

    let mut file = File::open(&wad).unwrap_or_else(|e| {
        eprintln!("open {wad}: {e}");
        std::process::exit(1);
    });
    let size = file.metadata().expect("stat").len();
    let arch = load_ffcs_archive(&mut file, size).unwrap_or_else(|e| {
        eprintln!("parse {wad}: {e}");
        std::process::exit(1);
    });

    eprintln!("{wad}: {} blocks, {} ASET rows", arch.indx.len(), arch.aset.len());

    // key_hash -> (text, source label). BTreeMap keeps the dump stable across runs so a
    // diff between two builds of the fix pack is readable.
    let mut rows: BTreeMap<u32, (String, String)> = BTreeMap::new();
    let mut dbs = 0usize;
    let mut collisions = 0usize;

    for bi in 0..arch.indx.len() {
        let path = arch.paths.get(bi).cloned().unwrap_or_default();
        if !filter.is_empty() && !path.to_lowercase().contains(&filter) {
            continue;
        }
        let decomp = match decompress_block(&mut file, &arch.indx, bi as u16) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let (parsed, _issues) = walk_decompressed_block(&decomp, &path);
        for (ei, entry) in parsed.entries.iter().enumerate() {
            if entry.type_hash != TYPE_HASH_STRINGDB {
                continue;
            }
            let Some(container) = parsed.containers.get(ei) else { continue };
            let label = format!("{path}#{ei}");

            let keys = extract_chunk_body(container, b"SYEK").or_else(|| extract_chunk_body(container, b"KEYS"));
            let strs = extract_chunk_body(container, b"SRTS").or_else(|| extract_chunk_body(container, b"STRS"));
            let (Some(keys), Some(strs)) = (keys, strs) else {
                eprintln!("  {label}: stringdb container missing SYEK and/or SRTS — skipped");
                continue;
            };

            let Some((kend, count)) = detect_header_endian(&keys, 8) else {
                eprintln!("  {label}: SYEK count implausible under BE and LE — skipped");
                continue;
            };
            // SRTS: u32 total-bytes header, then the heap.
            if strs.len() < 4 {
                eprintln!("  {label}: SRTS too short — skipped");
                continue;
            }
            let heap = &strs[4..];
            let tend = if ascii_score(heap, End::Be) >= ascii_score(heap, End::Le) { End::Be } else { End::Le };

            dbs += 1;
            eprintln!(
                "  {label}: {count} keys  SYEK={} text={}  heap={}B (SRTS hdr claims {}B)",
                kend.name(),
                tend.name(),
                heap.len(),
                End::Be.u32(&strs, 0).min(End::Le.u32(&strs, 0)),
            );

            for k in 0..count as usize {
                let base = 4 + k * 8;
                if base + 8 > keys.len() {
                    break;
                }
                let key_hash = kend.u32(&keys, base);
                let off = kend.u32(&keys, base + 4) as usize;
                if off >= heap.len() {
                    continue;
                }
                let Some(text) = read_utf16(heap, off, tend) else { continue };
                if let Some((prev, _)) = rows.get(&key_hash) {
                    if *prev != text {
                        collisions += 1;
                    }
                    continue;
                }
                rows.insert(key_hash, (text, label.clone()));
            }
        }
    }

    eprintln!("\n{dbs} stringdb container(s), {} unique keys, {collisions} conflicting duplicate(s)", rows.len());

    let mut sink: Box<dyn std::io::Write> = match &out {
        Some(p) => Box::new(std::io::BufWriter::new(File::create(p).expect("create out"))),
        None => Box::new(std::io::BufWriter::new(std::io::stdout())),
    };
    writeln!(sink, "key_hash\tsource\ttext").expect("write");
    for (h, (text, label)) in &rows {
        writeln!(sink, "0x{h:08X}\t{label}\t{}", escape(text)).expect("write");
    }
    drop(sink);
    if let Some(p) = out {
        eprintln!("wrote {p}");
    }
}
