//! Dev bin: scan every block in a WAD for Lua 5.1 chunks (`\x1bLua` + ver 0x51) and emit a TSV
//! census, with per-block count and the embedded Lua chunk source names.
//!
//! The three major Lua homes in the retail PC/Xbox game are already extracted (`scripts_vz`,
//! `resident_P000_Q3` in both vz.wad and shell.wad). This probe answers the completeness
//! question — "are there Lua chunks in OTHER blocks we have not pulled?" — by decompressing
//! every block, scanning for the `\x1bLua\x51` 5-byte header, and parsing enough of the chunk
//! header to recover the source-name string.
//!
//! Handles **both** PC (FFCS, little-endian sges) and console (SCFF, big-endian segs): blocks are
//! inflated with `mercs2_formats::sges::decompress_block`, and a block that fails to inflate stops
//! the scan with its index and path.
//!
//!   cargo run --release -p mercs2_probe --bin lua_chunk_scan -- \
//!       --wad game-files/vz.wad [--out scratchpad/pc-vz.tsv]

use mercs2_engine::wad;
use mercs2_formats::ffcs::{Endian, IndxEntry};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let wadpath = args
        .iter()
        .position(|a| a == "--wad")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| {
            eprintln!("usage: lua_chunk_scan --wad <path> [--out <tsv>]");
            std::process::exit(2);
        });
    let outpath: Option<PathBuf> = args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| args.get(i + 1).cloned())
        .map(PathBuf::from);
    let extract_block: Option<usize> = args
        .iter()
        .position(|a| a == "--extract-block")
        .and_then(|i| args.get(i + 1).and_then(|s| s.parse().ok()));
    let extract_dir: Option<PathBuf> = args
        .iter()
        .position(|a| a == "--extract-dir")
        .and_then(|i| args.get(i + 1).cloned())
        .map(PathBuf::from);

    let mut w = wad::open(&wadpath).expect("open wad");
    let (indx, endian): (Vec<IndxEntry>, Endian) = {
        let (arch, _) = wad::archive_and_file(&mut w);
        (arch.indx.clone(), arch.endian)
    };
    let paths: Vec<String> = wad::block_paths(&w).to_vec();
    let n = paths.len();
    let is_console = endian == Endian::Big;
    eprintln!(
        "[lua-scan] wad={wadpath} blocks={n} endian={} console={is_console}",
        if is_console { "BE" } else { "LE" }
    );

    let mut file = File::open(&wadpath).expect("reopen wad");

    if let (Some(bi), Some(dir)) = (extract_block, extract_dir.as_ref()) {
        let data = decompress_any(&mut file, &indx, bi).expect("decompress block");
        let hits = scan_block(&data);
        std::fs::create_dir_all(dir).expect("mkdir");
        eprintln!(
            "[extract] block={} path={} decomp_size={} chunks={}",
            bi,
            paths[bi],
            data.len(),
            hits.len()
        );
        let dump_raw = args.iter().any(|a| a == "--dump-raw");
        if dump_raw {
            let fname = format!("block_{:05}_raw.bin", bi);
            let path = dir.join(&fname);
            std::fs::write(&path, &data).expect("write raw");
            eprintln!("  wrote {} ({} B)", fname, data.len());
        }
        for (idx, h) in hits.iter().enumerate() {
            let end = hits.get(idx + 1).map(|n| n.offset).unwrap_or(data.len());
            let bytes = &data[h.offset..end];
            let fname = format!("chunk_{:04}_@{:X}.luac", idx, h.offset);
            let path = dir.join(&fname);
            std::fs::write(&path, bytes).expect("write chunk");
            eprintln!("  wrote {} ({} B)", fname, bytes.len());
        }
        return;
    }

    let mut tsv = String::new();
    tsv.push_str("block_index\tblock_path\tdecomp_size\tluac_count\tchunk_names\n");

    let mut total_chunks = 0usize;
    let mut blocks_with_lua = 0usize;
    for bi in 0..n {
        if bi % 500 == 0 {
            eprintln!("  [{bi}/{n}]");
        }
        let data = decompress_any(&mut file, &indx, bi)
            .unwrap_or_else(|e| panic!("block {bi} ({}): {e}", paths[bi]));

        let hits = scan_block(&data);
        if hits.is_empty() {
            continue;
        }
        blocks_with_lua += 1;
        total_chunks += hits.len();
        let joined: Vec<String> = hits.iter().map(|h| h.summary()).collect();
        tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            bi,
            paths[bi],
            data.len(),
            hits.len(),
            joined.join(",")
        ));
    }

    eprintln!("[lua-scan] blocks_with_lua={blocks_with_lua} total_chunks={total_chunks}");

    print!("{}", tsv);
    if let Some(p) = outpath {
        let mut f = File::create(&p).expect("open --out");
        f.write_all(tsv.as_bytes()).expect("write --out");
        eprintln!("[lua-scan] wrote {}", p.display());
    }
}

/// Block `block_index`, inflated (PC `sges` or console `segs`).
fn decompress_any(file: &mut File, indx: &[IndxEntry], block_index: usize) -> Result<Vec<u8>, String> {
    let bi = u16::try_from(block_index).map_err(|_| format!("block index {block_index} exceeds u16"))?;
    mercs2_formats::sges::decompress_block(file, indx, bi)
}


#[derive(Debug)]
struct ChunkHit {
    offset: usize,
    source: Option<String>,
}

impl ChunkHit {
    fn summary(&self) -> String {
        let n = self.source.as_deref().unwrap_or("<unnamed>");
        let n = n.strip_prefix('@').unwrap_or(n);
        format!("{}@{:X}", n, self.offset)
    }
}

/// Scan `data` for every `\x1bLua` + 0x51 occurrence; for each, parse the fixed 12-byte header
/// plus the top-level proto's `source` (`size_t` length + raw bytes + NUL).
fn scan_block(data: &[u8]) -> Vec<ChunkHit> {
    let mut hits = Vec::new();
    let sig = b"\x1bLua";
    let mut i = 0usize;
    while i + 12 <= data.len() {
        if &data[i..i + 4] == sig && data[i + 4] == 0x51 {
            if let Some(src) = parse_header(&data[i..]) {
                hits.push(ChunkHit { offset: i, source: src });
                i += 12;
                continue;
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    hits
}

fn parse_header(data: &[u8]) -> Option<Option<String>> {
    if data.len() < 12 {
        return None;
    }
    let format = data[5];
    let endian_byte = data[6];
    let int_size = data[7];
    let sz_size = data[8];
    let instr_size = data[9];
    let number_size = data[10];
    if format != 0 {
        return None;
    }
    if ![0u8, 1].contains(&endian_byte) {
        return None;
    }
    if ![4u8, 8].contains(&int_size) {
        return None;
    }
    if ![4u8, 8].contains(&sz_size) {
        return None;
    }
    if instr_size != 4 {
        return None;
    }
    if ![4u8, 8].contains(&number_size) {
        return None;
    }
    let endian_is_le = endian_byte == 1;

    let pos = 12;
    if pos + sz_size as usize > data.len() {
        return Some(None);
    }
    let sz = read_sized(&data[pos..pos + sz_size as usize], endian_is_le);
    let pos = pos + sz_size as usize;
    if sz == 0 || sz > 512 || pos + sz as usize > data.len() {
        return Some(None);
    }
    let bytes = &data[pos..pos + sz as usize];
    let end = if bytes.last() == Some(&0) { bytes.len() - 1 } else { bytes.len() };
    let s = String::from_utf8_lossy(&bytes[..end]).into_owned();
    Some(Some(s))
}

fn read_sized(bytes: &[u8], le: bool) -> u64 {
    let mut v = 0u64;
    if le {
        for (i, b) in bytes.iter().enumerate() {
            v |= (*b as u64) << (8 * i);
        }
    } else {
        for b in bytes {
            v = (v << 8) | (*b as u64);
        }
    }
    v
}
