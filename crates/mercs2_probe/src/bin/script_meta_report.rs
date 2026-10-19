//! Probe: compare every script container's stored INFO and DEPS with what
//! `mercs2_formats::script_meta::derive` computes from its own bytecode.
//!
//! For a retail WAD every row matches (the `script_meta_retail` test holds that). For a mod build it
//! shows which scripts were written with the wrong INFO or DEPS, and which scripts' INFO globals count
//! cannot be computed at all, with the reason.
//!
//!   cargo run -p mercs2_probe --bin script_meta_report -- <wad> [<wad> ...]
//!
//! One tab-separated row per script:
//!   wad  block  entry  name_hash  info_name  status  stored(globals/inherits/deps)  derived(...)  detail
//! status is one of:
//!   match        stored INFO and DEPS equal the derived ones
//!   differs      both are known and differ
//!   name         the INFO name does not hash to the entry's name hash (derived from the chunk's source name)
//!   underivable  derive failed; detail says why
//!
//! Exit 0 when every row is `match`, 1 when any is not, 2 when a WAD cannot be read.

use std::fs::File;
use std::path::Path;
use std::process::ExitCode;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::luachunk::decode;
use mercs2_formats::script_meta::derive;
use mercs2_formats::scripts_block::{container_chunk, container_deps, container_info};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_SCRIPT;

fn report(wad: &Path) -> Result<(usize, usize), String> {
    let mut f = File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
    let size = f.metadata().map_err(|e| format!("{}: {e}", wad.display()))?.len();
    let arch = load_ffcs_archive(&mut f, size).map_err(|e| format!("{}: {e}", wad.display()))?;
    let (mut rows, mut bad) = (0usize, 0usize);
    for bi in 0..arch.indx.len() {
        let block = decompress_block(&mut f, &arch.indx, bi as u16).map_err(|e| format!("{} block {bi}: {e}", wad.display()))?;
        let rd = |off: usize| u32::from_le_bytes(block[off..off + 4].try_into().unwrap());
        let count = rd(0) as usize;
        let mut pos = 4 + count * 16;
        for i in 0..count {
            let row = 4 + i * 16;
            let (name_hash, type_hash, size) = (rd(row), rd(row + 4), rd(row + 12) as usize);
            let c = &block[pos..pos + size];
            pos += size;
            if type_hash != TYPE_HASH_SCRIPT {
                continue;
            }
            rows += 1;
            let (status, stored, derived, detail, info_name) = classify(c, name_hash);
            if status != "match" {
                bad += 1;
            }
            println!(
                "{}\t{bi}\t{i}\t0x{name_hash:08x}\t{info_name}\t{status}\t{stored}\t{derived}\t{detail}",
                wad.display()
            );
        }
    }
    Ok((rows, bad))
}

/// `(status, stored, derived, detail, info_name)` for one container.
fn classify(c: &[u8], name_hash: u32) -> (&'static str, String, String, String, String) {
    let info = match container_info(c) {
        Ok(i) => i,
        Err(e) => return ("underivable", "-".into(), "-".into(), format!("INFO: {e}"), "-".into()),
    };
    let deps = container_deps(c).unwrap_or_default();
    let stored = format!("{}/{}/{}", info.tail_u16, info.tail_u8, deps.len());
    let luaq = match container_chunk(c, b"BINN") {
        Ok(Some(l)) => l,
        Ok(None) => return ("underivable", stored, "-".into(), "no BINN".into(), info.name),
        Err(e) => return ("underivable", stored, "-".into(), e, info.name),
    };
    // A container whose INFO names another script is derived under its chunk's own source name.
    let name_ok = pandemic_hash_m2(&info.name) == name_hash;
    let derive_name = if name_ok {
        info.name.clone()
    } else {
        match decode(luaq).ok().and_then(|ch| ch.main.source) {
            Some(s) => String::from_utf8_lossy(&s).into_owned(),
            None => return ("name", stored, "-".into(), "INFO names another script and the chunk has no source name".into(), info.name),
        }
    };
    match derive(&derive_name, luaq) {
        Err(e) => {
            let status = if name_ok { "underivable" } else { "name" };
            (status, stored, "-".into(), e, info.name)
        }
        Ok(m) => {
            let derived = format!("{}/{}/{}", m.globals, m.inherits, m.deps.len());
            let hashes: Vec<u32> = m.deps.iter().map(|d| d.hash).collect();
            if !name_ok {
                let detail = format!("INFO name {:?} is not this entry's; chunk source is {derive_name:?}", info.name);
                ("name", stored, derived, detail, info.name)
            } else if m.globals == info.tail_u16 && m.inherits == info.tail_u8 && hashes == deps {
                ("match", stored, derived, String::new(), info.name)
            } else {
                ("differs", stored, derived, String::new(), info.name)
            }
        }
    }
}

fn main() -> ExitCode {
    let wads: Vec<String> = std::env::args().skip(1).collect();
    if wads.is_empty() {
        eprintln!("usage: script_meta_report <wad> [<wad> ...]");
        return ExitCode::from(2);
    }
    println!("wad\tblock\tentry\tname_hash\tinfo_name\tstatus\tstored\tderived\tdetail");
    let (mut rows, mut bad) = (0usize, 0usize);
    for w in &wads {
        match report(Path::new(w)) {
            Ok((r, b)) => {
                rows += r;
                bad += b;
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    }
    eprintln!("{rows} scripts, {bad} not matching");
    if bad == 0 { ExitCode::SUCCESS } else { ExitCode::from(1) }
}
