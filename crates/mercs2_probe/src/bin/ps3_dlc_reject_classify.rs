//! Classify every PS3 (BE SCFF) block that `ucfx_byteswap::convert_block`
//! rejects, so the oracle work has a per-case list to target.
//!
//! For each block the probe walks the same decode path `dlc_port` runs
//! (segs -> convert_block), and when a step returns `Err`, it records the
//! error string. Successes are counted too. The run prints an aggregated
//! table (error-class -> count, examples), and optionally writes one raw
//! decompressed block body per unique error into
//! `<out>/<class>__block<N>__<path>.bin` for use as oracle fixtures.
//!
//!   cargo run --release -p mercs2_probe --bin ps3_dlc_reject_classify -- \
//!       --scff <path> [--out <scratch_dir>] [--limit-fixtures 2]
//!
//! Writes `<out>/classification.tsv` (one row per block) and
//! `<out>/summary.txt` (counts per class).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use mercs2_formats::be_to_le::convert::{convert_block, QUIET};
use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};
use mercs2_formats::dlc_stfs::load_stfs_or_doh;

fn arg_str(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1).cloned())
}

fn arg_usize(args: &[String], key: &str) -> Option<usize> {
    arg_str(args, key).and_then(|s| s.parse().ok())
}

/// Normalize a convert_block error string into a short classification key.
fn classify(err: &str) -> String {
    let e = err.trim();
    let after_entry = if let Some(idx) = e.find("Entry ") {
        let rest = &e[idx + "Entry ".len()..];
        if let Some(colon) = rest.find(": ") {
            rest[colon + 2..].to_string()
        } else {
            e.to_string()
        }
    } else {
        e.to_string()
    };
    let no_parens = {
        let mut s = after_entry.clone();
        while let Some(open) = s.rfind('(') {
            if let Some(close) = s[open..].find(')') {
                s.replace_range(open..open + close + 1, "");
                s = s.trim().to_string();
                continue;
            }
            break;
        }
        s
    };
    let bracket_stripped = {
        let mut s = no_parens;
        while let Some(open) = s.find('[') {
            if let Some(close) = s[open..].find(']') {
                s.replace_range(open..open + close + 1, "[..]");
            } else {
                break;
            }
        }
        s
    };
    bracket_stripped.trim().to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let scff = arg_str(&args, "--scff").unwrap_or_else(|| {
        eprintln!("usage: ps3_dlc_reject_classify --scff <path> [--out <dir>] [--limit-fixtures N]");
        std::process::exit(2);
    });
    let out_dir: Option<PathBuf> = arg_str(&args, "--out").map(PathBuf::from);
    let fixture_limit: usize = arg_usize(&args, "--limit-fixtures").unwrap_or(2);

    if let Some(d) = &out_dir {
        fs::create_dir_all(d).expect("mkdir --out");
    }

    QUIET.store(true, std::sync::atomic::Ordering::Relaxed);

    let scff_path = PathBuf::from(&scff);
    let (doh, src) = load_stfs_or_doh(&scff_path).expect("load scff");
    eprintln!("[classify] source={src} size={}", doh.len());
    let (version, rows) = parse_be_ffcs(&doh).expect("parse FFCS");
    eprintln!("[classify] FFCS v{version} chunks={}", rows.len());
    let chunk = |t: &str| rows.iter().find(|r| r.tag == t);
    let indx_row = chunk("INDX").expect("INDX").clone();
    let num_blocks = indx_row.meta as usize;
    let indx = parse_be_indx(&doh, indx_row.offset as usize, num_blocks);
    let pths_row = chunk("PTHS");
    let pths = pths_row
        .map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize))
        .unwrap_or_default();

    let mut classes: BTreeMap<String, Vec<(usize, String, Vec<u8>)>> = BTreeMap::new();
    let mut converted = 0usize;
    let mut per_block_tsv = String::new();
    per_block_tsv.push_str("block_index\tpath\tdecomp_size\tclass\tfull_err\n");

    for (blk_idx, e) in indx.iter().enumerate() {
        let path = pths.get(blk_idx).cloned().unwrap_or_else(|| format!("block_{blk_idx:05}"));
        let block_offset = e.file_offset();
        let block_size = e.page_count as usize * PAGE_SIZE;
        if block_offset + 4 > doh.len() {
            classes
                .entry("block_offset_out_of_range".into())
                .or_default()
                .push((blk_idx, path.clone(), Vec::new()));
            per_block_tsv.push_str(&format!("{blk_idx}\t{path}\t0\tblock_offset_out_of_range\t\n"));
            continue;
        }
        let slice = &doh[block_offset..(block_offset + block_size).min(doh.len())];

        let (decompressed, decomp_class): (Vec<u8>, Option<String>) = if slice.len() >= 4 && &slice[..4] == b"segs" {
            match decompress_be_sges(slice, 0, slice.len()) {
                Ok(d) => (d, None),
                Err(err) => (Vec::new(), Some(format!("sges_decompress_fail: {err}"))),
            }
        } else if slice.len() >= 8 {
            let rec = u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]) as usize;
            let header_end = 4 + rec * 16;
            let first_tag = slice.get(header_end..header_end + 4);
            if rec > 0 && rec < 5000 && first_tag == Some(b"XFCU") {
                let mut d = slice.to_vec();
                let mut z = d.len();
                while z > 4 && d[z - 1] == 0 {
                    z -= 1;
                }
                z = (z + 3) & !3;
                d.truncate(z);
                (d, None)
            } else {
                (Vec::new(), Some("not_segs_and_not_xfcu".into()))
            }
        } else {
            (Vec::new(), Some("block_shorter_than_8".into()))
        };

        if let Some(c) = decomp_class {
            per_block_tsv.push_str(&format!("{blk_idx}\t{path}\t0\t{c}\t\n"));
            classes.entry(c).or_default().push((blk_idx, path, Vec::new()));
            continue;
        }

        match convert_block(&decompressed, false, None) {
            Ok(_) => {
                converted += 1;
                per_block_tsv.push_str(&format!(
                    "{blk_idx}\t{path}\t{}\tOK\t\n",
                    decompressed.len()
                ));
            }
            Err(err) => {
                let class = classify(&err);
                let class_key = format!("convert_block: {class}");
                per_block_tsv.push_str(&format!(
                    "{blk_idx}\t{path}\t{}\t{class_key}\t{err}\n",
                    decompressed.len()
                ));
                classes
                    .entry(class_key)
                    .or_default()
                    .push((blk_idx, path, decompressed));
            }
        }
    }

    let mut summary = String::new();
    let total_blocks = indx.len();
    let total_rejected: usize = classes.values().map(|v| v.len()).sum();
    summary.push_str(&format!(
        "scff: {scff}\nblocks: {total_blocks}\nconverted: {converted}\nrejected: {total_rejected}\n\n"
    ));
    summary.push_str("Rejections grouped by class (sorted by count):\n");
    let mut by_count: Vec<(&String, &Vec<(usize, String, Vec<u8>)>)> = classes.iter().collect();
    by_count.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
    for (cls, items) in &by_count {
        summary.push_str(&format!("  {:5}  {}\n", items.len(), cls));
        for (bi, path, _) in items.iter().take(4) {
            summary.push_str(&format!("           block[{bi}] {path}\n"));
        }
    }

    eprintln!("{summary}");

    if let Some(dir) = &out_dir {
        fs::write(dir.join("summary.txt"), &summary).expect("write summary");
        fs::write(dir.join("classification.tsv"), &per_block_tsv).expect("write tsv");
        for (cls, items) in &by_count {
            let slug: String = cls
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let slug = slug.trim_matches('_').to_string();
            let slug = if slug.len() > 80 { slug[..80].to_string() } else { slug };
            for (idx, (bi, path, body)) in items.iter().take(fixture_limit).enumerate() {
                if body.is_empty() {
                    continue;
                }
                let safe_path: String = path
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                    .collect();
                let fname = format!("{slug}__{bi:04}_{idx}_{safe_path}.bin");
                let fp = dir.join(&fname);
                fs::write(&fp, body).expect("write fixture");
                eprintln!("  fixture {} ({} B)", fname, body.len());
            }
        }
    }
}
