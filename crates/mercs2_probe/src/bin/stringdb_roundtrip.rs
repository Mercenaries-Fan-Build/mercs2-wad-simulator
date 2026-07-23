//! Dev bin: prove the stringdb writer against RETAIL data before trusting it with real fixes.
//!
//! Unit tests in `mercs2_formats::stringdb` only prove the codec is self-consistent. This parses a
//! real shipped stringdb container, rebuilds it with no edits, and requires the output bytes to be
//! **identical** to what shipped. If that holds, any difference after a real edit is attributable
//! to the edit alone.
//!
//! Then it applies a throwaway edit and re-parses, to confirm offset re-pointing survives a length
//! change (the thing the old equal-length Python approach could not do).
//!
//!   cargo run -p mercs2_probe --bin stringdb_roundtrip -- --wad <wad> [--filter english]

use std::fs::File;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::sges::decompress_block;
use mercs2_formats::stringdb;
use mercs2_formats::types::TYPE_HASH_STRINGDB;
use mercs2_formats::ucfx::{extract_chunk_body, walk_decompressed_block};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut wad = String::new();
    let mut filter = String::new();
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
            o => {
                eprintln!("unknown arg: {o}");
                std::process::exit(2);
            }
        }
    }
    if wad.is_empty() {
        eprintln!("usage: stringdb_roundtrip --wad <wad> [--filter <path-substr>]");
        std::process::exit(2);
    }

    let mut file = File::open(&wad).expect("open wad");
    let size = file.metadata().expect("stat").len();
    let arch = load_ffcs_archive(&mut file, size).expect("parse ffcs");

    let mut checked = 0usize;
    let mut failed = 0usize;

    for bi in 0..arch.indx.len() {
        let path = arch.paths.get(bi).cloned().unwrap_or_default();
        if !filter.is_empty() && !path.to_lowercase().contains(&filter) {
            continue;
        }
        let Ok(decomp) = decompress_block(&mut file, &arch.indx, bi as u16) else { continue };
        let (parsed, _) = walk_decompressed_block(&decomp, &path);
        for (ei, entry) in parsed.entries.iter().enumerate() {
            if entry.type_hash != TYPE_HASH_STRINGDB {
                continue;
            }
            let Some(container) = parsed.containers.get(ei) else { continue };
            // `format_reference.md` §4.1 names these SYEK/SRTS; the PC build actually stores them
            // as KEYS/STRS. Try both and report which hit, so the doc can be corrected.
            let keyed = [(b"SYEK", b"SRTS"), (b"KEYS", b"STRS")]
                .into_iter()
                .find_map(|(kt, st)| {
                    let k = extract_chunk_body(container, kt)?;
                    let s = extract_chunk_body(container, st)?;
                    Some((std::str::from_utf8(kt).unwrap(), std::str::from_utf8(st).unwrap(), k, s))
                });
            let Some((ktag, stag, syek, srts)) = keyed else { continue };

            let label = format!("{path}#{ei} [{ktag}/{stag}]");
            let db = match stringdb::parse(&syek, &srts) {
                Ok(d) => d,
                Err(e) => {
                    println!("FAIL {label}: parse: {e}");
                    failed += 1;
                    continue;
                }
            };
            checked += 1;

            let (s2, r2) = stringdb::build(&db);
            let syek_ok = s2 == syek;
            let srts_ok = r2 == srts;

            if syek_ok && srts_ok {
                println!("PASS {label}: {} keys, {} B heap — rebuild byte-identical", db.entries.len(), db.heap_bytes);
            } else {
                failed += 1;
                println!(
                    "FAIL {label}: SYEK {} ({}/{} B), SRTS {} ({}/{} B)",
                    if syek_ok { "ok" } else { "DIFFERS" },
                    s2.len(),
                    syek.len(),
                    if srts_ok { "ok" } else { "DIFFERS" },
                    r2.len(),
                    srts.len(),
                );
                if !srts_ok {
                    if let Some(at) = (0..r2.len().min(srts.len())).find(|&i| r2[i] != srts[i]) {
                        println!("      first SRTS byte difference at 0x{at:X}");
                    }
                }
            }

            // Length-changing edit must survive a reparse.
            let Some(victim) = db.entries.iter().find(|e| !e.text.is_empty()).cloned() else { continue };
            let mut edited = db.clone();
            let longer = format!("{} (edited and made substantially longer)", victim.text);
            assert!(edited.set_by_hash(victim.key_hash, &longer), "set_by_hash missed a known key");
            let (es, er) = stringdb::build(&edited);
            match stringdb::parse(&es, &er) {
                Ok(back) => {
                    let got = back.entries.iter().find(|e| e.key_hash == victim.key_hash).map(|e| e.text.clone());
                    let intact = back.entries.len() == db.entries.len()
                        && back
                            .entries
                            .iter()
                            .zip(db.entries.iter())
                            .filter(|(a, _)| a.key_hash != victim.key_hash)
                            .all(|(a, b)| a.text == b.text);
                    if got.as_deref() == Some(longer.as_str()) && intact {
                        println!(
                            "      edit-reparse OK: heap {} B -> {} B, all {} other strings intact",
                            db.heap_bytes,
                            back.heap_bytes,
                            db.entries.len() - 1
                        );
                    } else {
                        failed += 1;
                        println!("      edit-reparse FAILED (edited={got:?}, others_intact={intact})");
                    }
                }
                Err(e) => {
                    failed += 1;
                    println!("      edit-reparse FAILED to parse: {e}");
                }
            }
        }
    }

    println!("\n{checked} container(s) checked, {failed} failure(s)");
    if failed > 0 || checked == 0 {
        std::process::exit(1);
    }
}
