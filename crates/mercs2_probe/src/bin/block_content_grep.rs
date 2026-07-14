//! `block_content_grep` — grep the DECOMPRESSED CONTENTS of every WAD block.
//!
//! NOTE: `mercs2_probe block-grep` only greps block PATH NAMES (`diag::block_grep`
//! walks `wad::block_paths`), which is easy to mistake for a content search — it
//! silently reports "0 blocks match" for any string that only exists *inside* a
//! block. This is the content-side counterpart: it decompresses each block and
//! searches the bytes, printing the block index/path plus the matching strings.
//!
//! ```text
//! cargo run -p mercs2_probe --bin block_content_grep -- bone_ [--wad path]
//! ```

use mercs2_engine::wad;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let needle = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            eprintln!("usage: block_content_grep <needle> [--wad <path>]");
            std::process::exit(2);
        });
    let wadpath = args
        .iter()
        .position(|a| a == "--wad")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(wad::registry_vz_wad)
        .unwrap_or_else(|| "game-files/vz.wad".into());

    // Console (Xbox/PS3) WAD payloads are 32-bit byte-swapped, so ASCII is scrambled WITHIN each
    // 4-byte word ("UCFX" reads as "XFCU"). `--swap` un-swaps each block before searching. Without
    // it, a console WAD yields a silent, bogus "0 matches".
    let swap = args.iter().any(|a| a == "--swap");

    let mut w = wad::open(&wadpath).expect("open wad");
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    let n = paths.len();
    eprintln!("[content-grep] scanning {n} blocks of {wadpath} for {needle:?} (swap={swap})");

    let pat = needle.as_bytes();
    let mut blocks_hit = 0usize;
    // Decompression stats — a block that fails to decompress must NOT look like "no match".
    let (mut ok, mut fail) = (0usize, 0usize);
    for bi in 0..n {
        let data = match wad::decompress_block_index(&mut w, bi as u16) {
            Ok(d) => {
                ok += 1;
                d
            }
            Err(_) => {
                fail += 1;
                continue;
            }
        };
        let data = if swap {
            let mut d = data;
            for c in d.chunks_exact_mut(4) {
                c.reverse();
            }
            d
        } else {
            data
        };
        if bi % 1000 == 0 {
            eprintln!("  [{bi}/{n}]");
        }
        // collect the printable-ASCII runs that contain the needle
        let mut found: Vec<String> = Vec::new();
        let mut i = 0usize;
        while i + pat.len() <= data.len() {
            if data[i..].starts_with(pat) {
                // widen to the surrounding identifier run
                let mut s = i;
                while s > 0 && is_id(data[s - 1]) {
                    s -= 1;
                }
                let mut e = i + pat.len();
                while e < data.len() && is_id(data[e]) {
                    e += 1;
                }
                if let Ok(t) = std::str::from_utf8(&data[s..e]) {
                    if !found.iter().any(|f| f == t) {
                        found.push(t.to_string());
                    }
                }
                i = e.max(i + 1);
            } else {
                i += 1;
            }
        }
        if !found.is_empty() {
            blocks_hit += 1;
            println!("\nblock={bi:<6} {}  ({} matches)", paths[bi], found.len());
            for f in &found {
                println!("    {f}");
            }
        }
    }
    eprintln!(
        "[content-grep] {blocks_hit} blocks contain {needle:?}  (decompressed ok={ok} failed={fail})"
    );
}

fn is_id(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}
