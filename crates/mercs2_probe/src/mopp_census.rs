//! `mopp-census`: what the MOPP codes of a PC WAD, or of a console archive, are made of.
//!
//! Every code of every PHY2 body is walked with [`mercs2_formats::mopp::steps`], and each instruction
//! counts toward these facts, printed one `kind<TAB>key<TAB>count` line each:
//!
//! * `opcode`: the opcode and its class.
//! * `pair`: the class the walk came from and the class of the instruction (`root` for the first; a
//!   split's two children apart, as `near child` for the inline one and `far child` for the offset one).
//! * `reanchor to shift`, `split at shift`, `cut at shift`: the shift a re-anchor leaves, and the shift
//!   a split or cut is read at.
//! * `split bounds` (`0x10–0x12`): the inline child's upper bound against the offset child's lower.
//! * `cut width` (`0x26–0x28`): the upper bound less the lower.
//! * `total`: bodies, codes, the deepest split nesting, the longest code, the lowest shift.
//!
//! `--never-in <census.tsv>` prints instead the facts these codes hold that the saved census of other
//! codes never does, which is how a code of ours is held against Havok's: retail `vz.wad` is the
//! census, a built patch the WAD. `total` lines are left out of that comparison.
//!
//! Exit 1 on a code that does not walk, and, under `--never-in`, on any fact the census lacks.

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::mopp::{self, Step};
use mercs2_formats::phy2_container::phy2_span_in_container;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::walk_decompressed_block;
use std::collections::BTreeMap;
use std::path::Path;

const USAGE: &str = "usage: mercs2_probe mopp-census <pc wad> [--never-in <census.tsv>]\n\
                     \x20      mercs2_probe mopp-census --archive <big-endian archive> [--never-in <census.tsv>]";

#[derive(Default)]
struct Census {
    facts: BTreeMap<(String, String), u64>,
    bodies: u64,
    codes: u64,
    failed: Vec<String>,
    max_depth: usize,
    max_len: usize,
    min_shift: Option<i32>,
}

impl Census {
    fn add(&mut self, kind: &str, key: String) {
        *self.facts.entry((kind.to_string(), key)).or_default() += 1;
    }
}

/// The class of an opcode [`mopp::steps`] hands back.
fn class(op: u8) -> &'static str {
    match op {
        0x00 => "return",
        0x01..=0x04 => "reanchor",
        0x05 => "jump8",
        0x06 => "jump16",
        0x07 => "jump24",
        0x09 => "keybase+u8",
        0x0a => "keybase+be16",
        0x0b => "keybase=be32",
        0x0c => "chunk jump",
        0x10..=0x12 => "split",
        0x13..=0x1c => "split-dop",
        0x20..=0x22 => "split-1value",
        0x23..=0x25 => "split-16bit",
        0x26..=0x28 => "cut",
        0x29..=0x2b => "cut-be24",
        0x30..=0x4f => "terminal",
        0x50 => "terminal+u8",
        0x51 => "terminal+be16",
        0x52 => "terminal+be24",
        0x53 => "terminal+be32",
        0x60..=0x63 => "property-u8",
        0x64..=0x67 => "property-be16",
        0x68..=0x6b => "property-be32",
        _ => "unknown",
    }
}

fn is_split(op: u8) -> bool {
    matches!(op, 0x10..=0x25)
}

fn count(code: &[u8], steps: &[Step], c: &mut Census) {
    c.codes += 1;
    c.max_len = c.max_len.max(code.len());
    for s in steps {
        let name = class(s.op);
        c.max_depth = c.max_depth.max(s.depth);
        c.add("opcode", format!("0x{:02x} {name}", s.op));
        let from = match s.after {
            None => "root".to_string(),
            Some((op, far)) if is_split(op) => format!("{}({} child)", class(op), if far { "far" } else { "near" }),
            Some((op, _)) => class(op).to_string(),
        };
        c.add("pair", format!("{from} -> {name}"));
        match name {
            "reanchor" => {
                let to = s.shift - s.op as i32;
                c.min_shift = Some(c.min_shift.map_or(to, |m| m.min(to)));
                c.add("reanchor to shift", format!("{to:3}"));
            }
            "split" | "split-dop" | "split-1value" | "split-16bit" => {
                c.add("split at shift", format!("{:3}", s.shift));
                if name == "split" {
                    let over = code[s.pc + 1] as i64 - code[s.pc + 2] as i64;
                    let bucket = match over {
                        i64::MIN..=-2 => "near bound 2+ below far bound",
                        -1 => "near bound 1 below far bound",
                        0 => "bounds equal",
                        1 => "near bound 1 above far bound",
                        _ => "near bound 2+ above far bound",
                    };
                    c.add("split bounds", bucket.to_string());
                }
            }
            "cut" | "cut-be24" => {
                c.add("cut at shift", format!("{:3}", s.shift));
                if name == "cut" {
                    let width = code[s.pc + 2] as i64 - code[s.pc + 1] as i64;
                    let bucket = match width {
                        i64::MIN..=-1 => "upper below lower",
                        0 => "0",
                        1 => "1",
                        2..=15 => "2-15",
                        _ => "16+",
                    };
                    c.add("cut width", bucket.to_string());
                }
            }
            _ => {}
        }
    }
}

fn body(label: &str, phy2: &[u8], chunked: bool, c: &mut Census) {
    c.bodies += 1;
    for (i, code) in mopp::extract_mopp_buffers(phy2).iter().enumerate() {
        let (steps, error) = if chunked { mopp::steps_chunked(code) } else { mopp::steps(code) };
        if let Some(e) = error {
            c.failed.push(format!("{label} code {i}: {e}"));
        }
        count(code, &steps, c);
    }
}

fn pc_wad(wad: &Path, c: &mut Census) -> Result<(), String> {
    let mut f = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let ar = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    for blk in 0..ar.indx.len() {
        let path = ar.paths.get(blk).cloned().unwrap_or_default();
        let dec = decompress_block(&mut f, &ar.indx, blk as u16).map_err(|e| format!("block {blk} ({path}): {e}"))?;
        let (parsed, _) = walk_decompressed_block(&dec, "mopp-census");
        for (ci, cont) in parsed.containers.iter().enumerate() {
            let Some((at, len)) = phy2_span_in_container(cont) else { continue };
            let name = parsed.entries.get(ci).map_or(0, |e| e.name_hash);
            body(&format!("{path} 0x{name:08X}"), &cont[at..at + len], false, c);
        }
    }
    Ok(())
}

fn console_archive(archive: &Path, c: &mut Census) -> Result<(), String> {
    use mercs2_formats::be_to_le::havok::convert_phy2_be_to_le_as_baked;
    use mercs2_formats::be_to_le::texture::container_rows;
    for chunk in crate::dlc_census::archive_chunks(archive, |_, _| true)? {
        if !(chunk.body.len() > 20 && chunk.body.starts_with(b"XFCU")) {
            continue;
        }
        let label = format!("{} 0x{:08X}", chunk.block, chunk.name);
        let rows = container_rows(&chunk.body).map_err(|e| format!("{label}: {e}"))?;
        let Some((_, off, size)) = rows.into_iter().find(|(tag, off, size)| tag == b"PHY2" && *size > 0 && off + size <= chunk.body.len()) else { continue };
        let le = convert_phy2_be_to_le_as_baked(&chunk.body[off..off + size]).map_err(|e| format!("{label}: {e}"))?;
        // A PS3 code is chunked and an Xbox one is not; the chunked walk reads both.
        body(&label, &le, true, c);
    }
    Ok(())
}

/// The `(kind, key)` facts of a saved census, its `total` lines aside.
fn saved(path: &Path) -> Result<std::collections::BTreeSet<(String, String)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    text.lines()
        .enumerate()
        .map(|(n, line)| {
            let mut cols = line.split('\t');
            match (cols.next(), cols.next(), cols.next(), cols.next()) {
                (Some(kind), Some(key), Some(_), None) => Ok((kind.to_string(), key.to_string())),
                _ => Err(format!("{}:{}: not a kind, key, count line", path.display(), n + 1)),
            }
        })
        .filter(|row| !matches!(row, Ok((kind, _)) if kind == "total"))
        .collect()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

pub fn run(args: &[String]) -> i32 {
    let mut c = Census::default();
    let never_in = flag(args, "--never-in");
    let done = (|| {
        match (flag(args, "--archive"), args.first().filter(|a| !a.starts_with("--"))) {
            (Some(archive), None) => console_archive(Path::new(&archive), &mut c)?,
            (None, Some(wad)) => pc_wad(Path::new(wad), &mut c)?,
            _ => return Err(USAGE.to_string()),
        }
        never_in.as_ref().map(|p| saved(Path::new(p))).transpose()
    })();
    let other = match done {
        Ok(other) => other,
        Err(e) => {
            eprintln!("mopp-census: {e}");
            return 2;
        }
    };
    let mut lacking = 0usize;
    for ((kind, key), n) in &c.facts {
        if let Some(other) = &other {
            if other.contains(&(kind.clone(), key.clone())) {
                continue;
            }
            lacking += 1;
        }
        println!("{kind}\t{key}\t{n}");
    }
    if other.is_none() {
        println!("total\tPHY2 bodies\t{}", c.bodies);
        println!("total\tcodes\t{}", c.codes);
        println!("total\tdeepest split nesting\t{}", c.max_depth);
        println!("total\tlongest code, bytes\t{}", c.max_len);
        if let Some(m) = c.min_shift {
            println!("total\tlowest shift\t{m}");
        }
    }
    for f in &c.failed {
        eprintln!("mopp-census: {f}");
    }
    if !c.failed.is_empty() {
        eprintln!("mopp-census: {} of {} codes did not walk", c.failed.len(), c.codes);
        return 1;
    }
    if other.is_some() {
        eprintln!("mopp-census: {} codes in {} PHY2 bodies; {lacking} facts the census lacks", c.codes, c.bodies);
        return i32::from(lacking != 0);
    }
    0
}
