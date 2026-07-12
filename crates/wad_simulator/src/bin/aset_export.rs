//! Export every ASET row of one or more WADs with rainbow-table name candidates.
//!
//! Each ASET row is 16 bytes: `{ asset_hash, secondary_ref, packed_block_ref,
//! type_id }` (see `docs/aset_format.md`). The asset NAME is not stored in the
//! WAD — only `pandemic_hash_m2(name)`. This tool joins:
//!
//!   * ASET  — the rows themselves (asset_hash, type_id, owning block, sub-offset)
//!   * PTHS  — the owning block's path string (context for unresolved hashes)
//!   * rainbow table — hash -> candidate name(s), all candidates emitted
//!   * type_id -> type name (docs/type_hash_registry.md, 1:1 with UCFX type_hash)
//!
//! Usage:
//!   cargo run --release -p wad_simulator --bin aset_export -- \
//!       --wad game-files/vz.wad --wad game-files/shell.wad \
//!       --rainbow tools/rainbow_table.json \
//!       --out docs/data/aset_export.csv

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use clap::Parser;
use mercs2_formats::ffcs::load_ffcs_archive;

// The crate is bin-only (no lib target), so pull in the real rainbow resolver
// directly rather than duplicating it as registry_hash_dump.rs had to.
#[path = "../names.rs"]
mod names;
use names::RainbowTable;

/// ASET `type_id` -> type name. 1:1 with the UCFX `type_hash` in decompressed
/// block entry tables; all 35 resolved via pandemic_hash_m2 collision.
/// Source: docs/type_hash_registry.md ("ASET type_id <-> type_hash Complete Mapping").
const TYPE_NAMES: &[(u32, &str, u32)] = &[
    (0, "fxdict", 0xFA46D8A8),
    (1, "decaltable", 0x3B0AABF8),
    (3, "binary", 0x8F0A54E2),
    (4, "musiccue", 0xE8DF4D87),
    (5, "facefxanimationset", 0x665EF13E),
    (6, "wavebank", 0xF753F6D0),
    (7, "stringdb", 0x39E5E978),
    (8, "worldentity", 0x5647C35D),
    (9, "layer", 0xE6B81A54),
    (10, "guidmap", 0x140E8728),
    (11, "animationtable", 0x207359C7),
    (12, "scrub", 0x600B904E),
    (13, "sounddb", 0xE5273C14),
    (14, "materialparam", 0xDE982D61),
    (15, "font", 0x99E77ACE),
    (16, "animation", 0x18166555),
    (17, "animstatemachine", 0xECE70371),
    (18, "chatter", 0xFA0B8DBC),
    (19, "model", 0x5B724250),
    (20, "level", 0xEA4829D5),
    (21, "soundbank", 0x9F8BCA10),
    (22, "lowresterrain", 0x1602815C),
    (23, "scaleformgfx", 0xFE0E8320),
    (24, "musicstatemap", 0xC122545A),
    (25, "fxdict", 0xFA46D8A8),
    (26, "musicstatemap", 0xC122545A),
    (27, "texture", 0xF011157A),
    (28, "path", 0xBCFE6314),
    (29, "effect", 0x5608BD5A),
    (30, "lineregion", 0x6310807F),
    (31, "animstatemachine", 0xECE70371),
    (32, "terrainmesh", 0x7C569307),
    (33, "sequencetable", 0xACCE47F2),
    (34, "facefxactor", 0x1CF649BB),
    (35, "script", 0x42498680),
];

#[derive(Parser)]
#[command(about = "Export all ASET rows with hash -> name candidates")]
struct Cli {
    /// WAD to export (repeatable). Default: the retail PC set.
    #[arg(long)]
    wad: Vec<PathBuf>,

    /// rainbow_table.json (pandemic_hash_m2 -> [names]). Repeatable: later tables
    /// supply names for hashes the earlier ones missed (e.g. the fragment emitted
    /// by `asset_gap_probe --emit`). Defaults to the main table + the discovered-
    /// names fragment.
    #[arg(long)]
    rainbow: Vec<PathBuf>,

    /// Output CSV (one row per ASET entry)
    #[arg(long, default_value = "docs/data/aset_export.csv")]
    out: PathBuf,

    /// Output CSV of the DISTINCT unresolved hashes (hash, types, blocks seen in)
    #[arg(long, default_value = "docs/data/aset_unresolved.csv")]
    unresolved_out: PathBuf,

    /// Output CSV: one row per DISTINCT asset hash (the name dictionary)
    #[arg(long, default_value = "docs/data/aset_names.csv")]
    dict_out: PathBuf,
}

/// Accumulated facts about one distinct asset hash across all WADs.
#[derive(Default)]
struct HashFacts {
    names: Vec<String>,
    types: HashSet<String>,
    wads: HashSet<String>,
    blocks: HashSet<String>,
    rows: usize,
}

/// Quote a CSV field (RFC4180): wrap in quotes and double any embedded quote.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let wads: Vec<PathBuf> = if cli.wad.is_empty() {
        ["vz.wad", "shell.wad", "English.wad", "Loading.wad"]
            .iter()
            .map(|n| PathBuf::from("game-files").join(n))
            .collect()
    } else {
        cli.wad.clone()
    };

    let type_name: HashMap<u32, (&str, u32)> = TYPE_NAMES
        .iter()
        .map(|&(id, name, hash)| (id, (name, hash)))
        .collect();

    let requested: Vec<PathBuf> = if cli.rainbow.is_empty() {
        vec![
            PathBuf::from("tools/rainbow_table.json"),
            // Fragments cracked from the WAD itself, each a set of verified preimages:
            //   asset_gap_probe      — texture-base -> model convention
            //   block_string_harvest — identifiers mined from decompressed block payloads
            //   name_expand          — slot-grammar expansion of the above
            PathBuf::from("docs/data/aset_discovered_names.json"),
            PathBuf::from("docs/data/aset_block_strings.json"),
            PathBuf::from("docs/data/aset_expanded_names.json"),
        ]
    } else {
        cli.rainbow.clone()
    };
    let tables: Vec<PathBuf> = requested
        .iter()
        .filter(|p| {
            let ok = p.exists();
            if !ok {
                eprintln!("  (skipping absent rainbow table {})", p.display());
            }
            ok
        })
        .cloned()
        .collect();
    eprintln!("Loading {} rainbow table(s) ...", tables.len());
    let rainbow = RainbowTable::load_many(&tables)?;
    eprintln!("  {} m2 hashes loaded", rainbow.len());

    if let Some(dir) = cli.out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut out = BufWriter::new(File::create(&cli.out)?);
    writeln!(
        out,
        "wad,row_index,asset_hash,name,name_candidates,candidate_count,\
         type_id,type_name,type_hash,block_index,block_path,\
         sub_offset,is_primary,secondary_ref,secondary_name"
    )?;

    // Distinct-unresolved accumulator across all WADs.
    let mut unresolved: BTreeMap<u32, (HashSet<String>, HashSet<String>)> = BTreeMap::new();
    // Distinct-hash dictionary across all WADs.
    let mut dict: BTreeMap<u32, HashFacts> = BTreeMap::new();
    let mut grand_rows = 0usize;
    let mut grand_resolved = 0usize;
    // per-type: (rows, resolved rows, distinct hashes, distinct resolved)
    let mut per_type: BTreeMap<String, (usize, usize, HashSet<u32>, HashSet<u32>)> =
        BTreeMap::new();

    for wad_path in &wads {
        if !wad_path.exists() {
            eprintln!("SKIP (missing): {}", wad_path.display());
            continue;
        }
        let wad_name = wad_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let mut file = File::open(wad_path)?;
        let file_size = file.metadata()?.len();
        let arch = load_ffcs_archive(&mut file, file_size)?;

        let mut rows = 0usize;
        let mut resolved = 0usize;
        let mut distinct: HashSet<u32> = HashSet::new();
        let mut distinct_resolved: HashSet<u32> = HashSet::new();

        for (i, e) in arch.aset.iter().enumerate() {
            let blk = e.block_index() as usize;
            let block_path = arch.paths.get(blk).map(|s| s.as_str()).unwrap_or("");

            let (tname, thash) = type_name
                .get(&e.type_id)
                .copied()
                .unwrap_or(("unknown", 0));

            let cands: &[String] = rainbow.candidates(e.asset_hash);
            let best = cands.first().map(|s| s.as_str()).unwrap_or("");
            let all = cands.join(";");

            let sec_name = if e.secondary_ref == 0xFFFFFFFF {
                ""
            } else {
                rainbow.resolve(e.secondary_ref).unwrap_or("")
            };

            rows += 1;
            distinct.insert(e.asset_hash);
            if !cands.is_empty() {
                resolved += 1;
                distinct_resolved.insert(e.asset_hash);
            } else {
                let ent = unresolved.entry(e.asset_hash).or_default();
                ent.0.insert(tname.to_string());
                if !block_path.is_empty() {
                    ent.1.insert(block_path.to_string());
                }
            }

            let d = dict.entry(e.asset_hash).or_default();
            if d.names.is_empty() {
                d.names = cands.to_vec();
            }
            d.types.insert(tname.to_string());
            d.wads.insert(wad_name.clone());
            if !block_path.is_empty() {
                d.blocks.insert(block_path.to_string());
            }
            d.rows += 1;

            let pt = per_type.entry(tname.to_string()).or_default();
            pt.0 += 1;
            pt.2.insert(e.asset_hash);
            if !cands.is_empty() {
                pt.1 += 1;
                pt.3.insert(e.asset_hash);
            }

            writeln!(
                out,
                "{},{},0x{:08X},{},{},{},{},{},{},{},{},{},{},0x{:08X},{}",
                csv_field(&wad_name),
                i,
                e.asset_hash,
                csv_field(best),
                csv_field(&all),
                cands.len(),
                e.type_id,
                tname,
                if thash != 0 {
                    format!("0x{thash:08X}")
                } else {
                    String::new()
                },
                e.block_index(),
                csv_field(block_path),
                e.sub_entry(),
                if e.is_primary() { 1 } else { 0 },
                e.secondary_ref,
                csv_field(sec_name),
            )?;
        }

        println!(
            "{:<14} {:>7} rows  {:>7} distinct hashes  resolved: {:>6} rows ({:.1}%)  {:>6} distinct ({:.1}%)",
            wad_name,
            rows,
            distinct.len(),
            resolved,
            100.0 * resolved as f64 / rows.max(1) as f64,
            distinct_resolved.len(),
            100.0 * distinct_resolved.len() as f64 / distinct.len().max(1) as f64,
        );
        grand_rows += rows;
        grand_resolved += resolved;
    }
    out.flush()?;

    // Distinct-unresolved report.
    if let Some(dir) = cli.unresolved_out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut uf = BufWriter::new(File::create(&cli.unresolved_out)?);
    writeln!(uf, "asset_hash,types,block_paths")?;
    for (h, (types, blocks)) in &unresolved {
        let mut t: Vec<&str> = types.iter().map(|s| s.as_str()).collect();
        t.sort_unstable();
        let mut b: Vec<&str> = blocks.iter().map(|s| s.as_str()).collect();
        b.sort_unstable();
        b.truncate(4); // enough context to identify; full join lives in the main CSV
        writeln!(
            uf,
            "0x{:08X},{},{}",
            h,
            csv_field(&t.join(";")),
            csv_field(&b.join(";"))
        )?;
    }
    uf.flush()?;

    // Distinct-hash name dictionary — the headline artifact.
    if let Some(dir) = cli.dict_out.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut df = BufWriter::new(File::create(&cli.dict_out)?);
    writeln!(
        df,
        "asset_hash,name,name_candidates,candidate_count,resolved,types,wads,aset_rows,block_count,example_block"
    )?;
    for (h, f) in &dict {
        let mut types: Vec<&str> = f.types.iter().map(|s| s.as_str()).collect();
        types.sort_unstable();
        let mut wads: Vec<&str> = f.wads.iter().map(|s| s.as_str()).collect();
        wads.sort_unstable();
        let mut blocks: Vec<&str> = f.blocks.iter().map(|s| s.as_str()).collect();
        blocks.sort_unstable();
        writeln!(
            df,
            "0x{:08X},{},{},{},{},{},{},{},{},{}",
            h,
            csv_field(f.names.first().map(|s| s.as_str()).unwrap_or("")),
            csv_field(&f.names.join(";")),
            f.names.len(),
            if f.names.is_empty() { 0 } else { 1 },
            csv_field(&types.join(";")),
            csv_field(&wads.join(";")),
            f.rows,
            blocks.len(),
            csv_field(blocks.first().copied().unwrap_or("")),
        )?;
    }
    df.flush()?;

    let dict_named = dict.values().filter(|f| !f.names.is_empty()).count();
    println!(
        "\nDISTINCT hashes: {} ({} named, {:.1}%)",
        dict.len(),
        dict_named,
        100.0 * dict_named as f64 / dict.len().max(1) as f64
    );

    println!(
        "TOTAL  {} rows, resolved {} ({:.1}%); {} distinct unresolved hashes",
        grand_rows,
        grand_resolved,
        100.0 * grand_resolved as f64 / grand_rows.max(1) as f64,
        unresolved.len()
    );

    println!("\nPer type (rows resolved / rows, distinct resolved / distinct):");
    let mut types: Vec<_> = per_type.iter().collect();
    types.sort_by_key(|(_, v)| std::cmp::Reverse(v.0));
    for (name, (rows, res, dist, dres)) in types {
        println!(
            "  {:<20} {:>6}/{:<6} ({:>5.1}%)   {:>6}/{:<6} ({:>5.1}%)",
            name,
            res,
            rows,
            100.0 * *res as f64 / (*rows).max(1) as f64,
            dres.len(),
            dist.len(),
            100.0 * dres.len() as f64 / dist.len().max(1) as f64,
        );
    }

    println!("\nWrote {}", cli.dict_out.display());
    println!("Wrote {}", cli.out.display());
    println!("Wrote {}", cli.unresolved_out.display());
    Ok(())
}
