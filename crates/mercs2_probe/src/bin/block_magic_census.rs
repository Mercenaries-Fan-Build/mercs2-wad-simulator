//! Which leading bytes do the blocks of a WAD start with, and are the non-deflate ones stored
//! at their decompressed size? Tallies, per WAD given, the first four bytes of every INDX block
//! (printable as text when they are ASCII) with how many of those blocks have
//! `compressed_page_count == decompressed_page_count`, plus one example block index per magic.
//!
//!   cargo run -p mercs2_probe --bin block_magic_census -- <wad> [<wad> ...]

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

use mercs2_formats::ffcs::{load_ffcs_archive, PAGE_SIZE};

fn label(m: &[u8; 4]) -> String {
    if m.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(m).into_owned()
    } else {
        format!("{:02x}{:02x}{:02x}{:02x}", m[0], m[1], m[2], m[3])
    }
}

fn main() {
    let wads: Vec<String> = std::env::args().skip(1).collect();
    if wads.is_empty() {
        eprintln!("usage: block_magic_census <wad> [<wad> ...]");
        std::process::exit(2);
    }
    for path in wads {
        let mut f = File::open(&path).unwrap_or_else(|e| panic!("open {path}: {e}"));
        let size = f.metadata().unwrap_or_else(|e| panic!("stat {path}: {e}")).len();
        let a = load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("{path}: {e}"));
        // magic -> (blocks, blocks with equal page counts, first block index)
        let mut tally: BTreeMap<String, (usize, usize, usize)> = BTreeMap::new();
        for (i, e) in a.indx.iter().enumerate() {
            let mut m = [0u8; 4];
            f.seek(SeekFrom::Start(e.page_index as u64 * PAGE_SIZE)).unwrap_or_else(|e| panic!("{path}: seek: {e}"));
            f.read_exact(&mut m).unwrap_or_else(|e| panic!("{path}: block {i}: read: {e}"));
            let t = tally.entry(label(&m)).or_insert((0, 0, i));
            t.0 += 1;
            if e.compressed_page_count() == e.decompressed_page_count() {
                t.1 += 1;
            }
        }
        println!("== {path} ({:?}, {} blocks)", a.endian, a.indx.len());
        for (m, (n, same, first)) in tally {
            println!("  {m:10} blocks={n:6} equal_page_counts={same:6} first_block={first}");
        }
    }
}
