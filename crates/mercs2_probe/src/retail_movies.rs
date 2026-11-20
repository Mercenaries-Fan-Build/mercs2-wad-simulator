//! `retail-movies`: list every Scaleform movie (`cfx_pack`, ASET type 23) the given WADs ship, as the
//! table `mercs2_quartermaster` routes `add_movie` with (`crates/mercs2_quartermaster/data/retail_movies.tsv`).
//!
//! One row per (movie, WAD): the name hash, the name when the rainbow table has one whose
//! `pandemic_hash_m2` is that hash (`-` otherwise), and the WAD's file name. Read from each WAD's
//! ASET table; no block is decompressed. Pass the vanilla game's WADs only — a `-patch.wad` or a
//! Shipment's level WAD is mod output, not the game's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use mercs2_formats::types::TYPE_ID_CFX_PACK;

/// `mercs2_probe retail-movies <out.tsv> <wad>...`
pub fn run(args: &[String]) -> Result<(), String> {
    let [out, wads @ ..] = args else {
        return Err("usage: mercs2_probe retail-movies <out.tsv> <wad>...".into());
    };
    if wads.is_empty() {
        return Err("name at least one WAD".into());
    }
    let mut rows: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for wad in wads {
        let path = Path::new(wad);
        let file_name = path.file_name().ok_or_else(|| format!("{wad} has no file name"))?.to_string_lossy().into_owned();
        if file_name.to_ascii_lowercase().ends_with("-patch.wad") {
            return Err(format!("{wad} is a patch WAD, not one the game ships"));
        }
        let mut f = std::fs::File::open(path).map_err(|e| format!("open {wad}: {e}"))?;
        let size = f.metadata().map_err(|e| format!("stat {wad}: {e}"))?.len();
        let archive = mercs2_formats::ffcs::load_ffcs_archive(&mut f, size).map_err(|e| format!("read {wad}: {e}"))?;
        let mut n = 0;
        for row in archive.aset.iter().filter(|r| r.type_id == TYPE_ID_CFX_PACK) {
            rows.entry(row.asset_hash).or_default().insert(file_name.clone());
            n += 1;
        }
        eprintln!("[retail-movies] {wad}: {n} cfx_pack rows");
    }
    let table = mercs2_formats::game_paths::local_config_rainbow_table(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let names = mercs2_engine::worldutil::rainbow_names(&table, &rows.keys().copied().collect())?;
    let read: Vec<String> = wads
        .iter()
        .map(|w| Path::new(w).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())
        .collect();
    let mut text = format!(
        "# Written by `mercs2_probe retail-movies` from the ASET tables of: {}.\n# name is `-` where the rainbow table names no string that hashes to the row's hash.\nhash\tname\twad\n",
        read.join(", ")
    );
    for (hash, in_wads) in &rows {
        let name = names
            .get(hash)
            .filter(|n| mercs2_formats::hash::pandemic_hash_m2(n) == *hash)
            .map(String::as_str)
            .unwrap_or("-");
        for wad in in_wads {
            text.push_str(&format!("0x{hash:08X}\t{name}\t{wad}\n"));
        }
    }
    std::fs::write(out, &text).map_err(|e| format!("write {out}: {e}"))?;
    println!("wrote {out}: {} movies, {} rows", rows.len(), text.lines().filter(|l| l.starts_with("0x")).count());
    Ok(())
}
