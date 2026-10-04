//! All-Rust DLC porter: Xbox 360 DLC RAR/STFS / PS3 BE SCFF → PC `vz-patch.wad`.
//!
//! Reimplements the core of `tools/dlc_port.py::port_x360_dlc` using the Rust
//! pipeline: STFS extract → BE FFCS parse → per-block (BE-sges decompress or
//! XFCU passthrough → convert BE→LE → sges recompress + round-trip verify) →
//! content-routed ASET resolution + synthetic ASET → FFCS assembly.
//!
//! ASET routing is by *content*, not by index: the Xbox `block_index` cannot be
//! rebased arithmetically (the DLC's blocks are not a contiguous run in the
//! source index space), so every row — per-block and `0xFFFF` "global" alike —
//! is routed to whichever converted block actually owns its `asset_hash`, and
//! its `type_id` is refined from that entry's `type_hash`. Rows owned by no
//! shipped block are dropped so the retail WAD resolves them instead.
//!
//! The bootstrap / import-chain injection is deliberately *not* performed: those
//! passes existed to graft the DLC contracts into the *vz* master script, but the
//! DLC ships its own master script and boots as its own level via
//! `LevelBootstrap.LoadLevel("dlc01", "dlc01")`. A note is printed on every run.
//!
//! Formats are borrowed, not re-derived: `mercs2_formats` supplies the STFS
//! reader, the BE FFCS/INDX/ASET/PTHS parsers, the sges codec, the UCFX block
//! entry table and the patch-WAD assembler; `ucfx_byteswap` supplies `convert_block`.
//!
//! Run `--list-blocks` to dump the block table without converting anything;
//! `--start-block` / `--max-blocks` narrow the converted range while iterating.
//!
//! `--xbox-doh-oracle <path>` opts into the Xbox-DOH side-oracle resolution for
//! PS3 blocks the UCFX descriptor walker refuses with the `PS3 compact decl
//! detected` class (see `docs/_dlc_port_xbox_doh_side_oracle.md`). When the
//! oracle is supplied, the main input must be a PS3 BE SCFF (via `--x360-stfs`;
//! the same loader accepts a raw BE SCFF). A rejected PS3 block's primary
//! `name_hash` is resolved against the Xbox DOH's indexed primary name_hashes;
//! an unresolved hash fails the whole run with a message naming the hash and
//! both container paths.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::Ordering;

use clap::Parser;

use mercs2_formats::aset_type_ids::{
    type_id_for_type_hash, SCRIPT_ASET_TYPE_ID, SCRIPT_TYPE_HASH, STRINGDB_ASET_TYPE_ID,
    STRINGDB_TYPE_HASH,
};
use mercs2_formats::dlc_input::{
    decompress_be_sges, parse_be_aset, parse_be_ffcs, parse_be_indx, parse_be_pths, PAGE_SIZE,
};
use mercs2_formats::dlc_stfs::{extract_stfs_from_rar, load_stfs_or_doh};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_sges};
use mercs2_formats::ucfx::parse_block_entry_table;
use ucfx_byteswap::convert::{convert_block, QUIET};

use dlc_port::xbox_doh_oracle::{is_ps3_compact_decl_rejection, primary_name_hash_be, XboxDohOracle};

#[derive(Parser)]
#[command(name = "dlc_port", about = "Port Xbox 360 / PS3 DLC to a PC vz-patch.wad (all-Rust)")]
struct Cli {
    /// Xbox 360 DLC RAR archive
    #[arg(long)]
    x360_rar: Option<PathBuf>,
    /// STFS container, raw DLC01.doh, or PS3 BE SCFF file
    #[arg(long)]
    x360_stfs: Option<PathBuf>,
    /// Output vz-patch.wad path
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// List blocks and exit
    #[arg(long)]
    list_blocks: bool,
    /// Only process the first N blocks (testing)
    #[arg(long)]
    max_blocks: Option<usize>,
    /// Start at block N
    #[arg(long, default_value_t = 0)]
    start_block: usize,
    /// Xbox 360 DOH side oracle: when the main input is a PS3 SCFF, the
    /// descriptor walker refuses blocks that ship PS3's non-interleaved
    /// multi-stream vertex layout (`PS3 compact decl detected` class, see
    /// `docs/_descriptor_walker_oracle.md`). With this flag set, each such
    /// block is resolved against the Xbox DOH at matching primary
    /// `name_hash` and the Xbox-DOH LE body is used in its place. The output
    /// WAD then carries 100 % of the DLC01 content with no silent drops.
    ///
    /// Loud-fail contract: if the Xbox DOH does not hold a PS3-rejected
    /// block's primary `name_hash`, the whole run fails with a message
    /// naming the hash and both container paths. No silent fallback.
    #[arg(long, value_name = "XBOX_DOH_PATH")]
    xbox_doh_oracle: Option<PathBuf>,
    #[arg(short, long)]
    verbose: bool,
}

fn strip_xbox_sub_entry(u2: u32) -> u32 {
    (u2 & 0xFFFF_0000) | 0xFFFF
}

/// Per-block decode result the main loop then recompresses + verifies +
/// stores in a `PatchBlock`. Separating the dispatch from the output
/// assembly keeps the loop readable and makes the side-oracle a pure
/// routing decision.
enum BlockDecode {
    /// PS3 BE→LE walker converted the block directly.
    Ps3Direct(Vec<u8>),
    /// PS3 walker refused the block with the `PS3 compact decl detected`
    /// class; the Xbox-DOH oracle resolved it. The `Vec<u8>` is the
    /// Xbox-DOH LE body already produced by `convert_block` at oracle-
    /// load time. `xbox_path` is the Xbox DOH's PTHS entry for the block;
    /// `name_hash` is the primary name_hash used as the lookup key.
    XboxDohSideOracle {
        le_bytes: Vec<u8>,
        xbox_path: String,
        name_hash: u32,
    },
}

fn run() -> Result<(), String> {
    let cli = Cli::parse();

    // ── Step 1: load DOH bytes ──
    let doh: Vec<u8> = if let Some(rar) = &cli.x360_rar {
        println!("Step 1: Extracting STFS from RAR...");
        let work = std::env::temp_dir().join("dlc_port_rs");
        let reader = extract_stfs_from_rar(rar, &work)?;
        let entry = reader.doh_entry().ok_or("No DOH in STFS")?;
        println!("  Reading DOH ({} bytes)...", entry.file_size);
        reader.read_doh()?
    } else if let Some(p) = &cli.x360_stfs {
        println!("Step 1: Loading {}...", p.display());
        let (doh, src) = load_stfs_or_doh(p)?;
        println!("  Source: {src}, size: {} bytes", doh.len());
        doh
    } else {
        return Err("Provide --x360-rar or --x360-stfs".into());
    };

    // ── Step 1b: optionally load the Xbox-DOH side oracle ──
    //
    // Loaded eagerly so a bad path fails before block conversion starts
    // rather than halfway through. The oracle also sets QUIET on
    // ucfx_byteswap so its own BE→LE pass is silent; the main loop
    // re-asserts QUIET below anyway for the same reason.
    let xbox_doh_oracle: Option<XboxDohOracle> = match &cli.xbox_doh_oracle {
        Some(p) => {
            println!("Step 1b: Loading Xbox-DOH side oracle from {}...", p.display());
            Some(XboxDohOracle::load(p)?)
        }
        None => None,
    };

    // ── Step 2: parse BE FFCS ──
    let (version, rows) = parse_be_ffcs(&doh)?;
    println!("  FFCS version: {version}, chunks: {}", rows.len());
    let chunk = |t: &str| rows.iter().find(|r| r.tag == t);
    let indx_row = chunk("INDX").ok_or("Missing INDX")?.clone();
    let num_blocks = indx_row.meta as usize;
    let indx = parse_be_indx(&doh, indx_row.offset as usize, num_blocks);
    let aset = chunk("ASET")
        .map(|r| parse_be_aset(&doh, r.offset as usize, r.meta as usize))
        .unwrap_or_default();
    let pths = chunk("PTHS")
        .map(|r| parse_be_pths(&doh, r.offset as usize, r.meta as usize))
        .unwrap_or_default();
    let csum_value = chunk("CSUM").map(|r| r.offset).unwrap_or(0);
    let csum_meta = chunk("CSUM").map(|r| r.meta);
    println!("  INDX: {num_blocks} blocks, ASET: {} entries, PTHS: {}", aset.len(), pths.len());

    if cli.list_blocks {
        for (i, e) in indx.iter().enumerate() {
            let p = pths.get(i).map(String::as_str).unwrap_or("?");
            println!("  [{i}] pages={} packed=0x{:08X} {p}", e.page_count, e.packed_field);
        }
        return Ok(());
    }

    let output = cli.output.ok_or("--output is required")?;

    // ── Normalize every ASET row; routing is resolved from block CONTENT below.
    //
    // The Xbox block_index cannot be rebased arithmetically: the DLC's blocks are
    // not a contiguous run in the source index space, so `idx - min(idx)` misroutes
    // (measured: 1045 texture rows landing on the wrong block). Instead every row —
    // both per-block and the 0xFFFF "global" ones — is routed to whichever converted
    // block actually contains its asset_hash.
    let pending_aset: Vec<AsetEntry> = aset
        .iter()
        .map(|ae| AsetEntry::new(ae.asset_hash, ae.u1, strip_xbox_sub_entry(ae.u2), ae.u3))
        .collect();
    println!("  ASET rows to route by content: {}", pending_aset.len());

    // ── Step 3: per-block convert + recompress ──
    QUIET.store(true, Ordering::Relaxed); // silence convert_block's per-block diagnostics
    let end = num_blocks.min(cli.start_block + cli.max_blocks.unwrap_or(num_blocks));
    let mut converted: Vec<PatchBlock> = Vec::new();
    let mut skipped = 0usize;
    let mut ps3_direct_count = 0usize;
    let mut xbox_doh_sourced_count = 0usize;
    let total = end - cli.start_block;

    for blk_idx in cli.start_block..end {
        let e = &indx[blk_idx];
        let path = pths.get(blk_idx).cloned().unwrap_or_else(|| format!("block_{blk_idx:05}"));
        let block_offset = e.file_offset();
        let block_size = e.page_count as usize * PAGE_SIZE;
        if block_offset + 4 > doh.len() {
            skipped += 1;
            continue;
        }
        let slice = &doh[block_offset..(block_offset + block_size).min(doh.len())];

        // Decompress (segs) or XFCU passthrough.
        let decompressed: Vec<u8> = if slice.len() >= 4 && &slice[..4] == b"segs" {
            match decompress_be_sges(slice, 0, slice.len()) {
                Ok(d) => d,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
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
                d
            } else {
                skipped += 1;
                continue;
            }
        } else {
            skipped += 1;
            continue;
        };

        // Convert BE→LE, routing a PS3-compact-decl rejection through the
        // optional Xbox-DOH side oracle. Anything the oracle cannot resolve
        // fails the whole run — no silent fallback — so the output WAD is
        // either 100 % resolved or does not exist.
        let decode = match convert_block(&decompressed, false, None) {
            Ok(s) => BlockDecode::Ps3Direct(s),
            Err(err) => {
                if is_ps3_compact_decl_rejection(&err) {
                    match &xbox_doh_oracle {
                        Some(oracle) => {
                            let Some(name_hash) = primary_name_hash_be(&decompressed) else {
                                return Err(format!(
                                    "block {blk_idx} ({path}): descriptor-walker refused as \
                                     PS3-compact-decl but the BE entry table is unreadable, so \
                                     no primary name_hash is available to look up in the Xbox \
                                     DOH oracle at {}. convert_block error: {err}",
                                    oracle.source_path.display()
                                ));
                            };
                            match oracle.lookup(name_hash) {
                                Some(ob) => BlockDecode::XboxDohSideOracle {
                                    le_bytes: ob.le_decompressed.clone(),
                                    xbox_path: ob.xbox_path.clone(),
                                    name_hash,
                                },
                                None => {
                                    return Err(format!(
                                        "Xbox-DOH side-oracle miss: PS3 block [{blk_idx}] {path} \
                                         primary name_hash=0x{name_hash:08X} is not in the Xbox \
                                         DOH container at {}. The side oracle must resolve EVERY \
                                         PS3-compact-decl rejection or the output WAD is \
                                         incomplete; silent fallback is forbidden. convert_block \
                                         error: {err}",
                                        oracle.source_path.display()
                                    ));
                                }
                            }
                        }
                        None => {
                            // No oracle supplied: keep the historical skip
                            // behaviour so running without `--xbox-doh-oracle`
                            // is identical to the pre-change build.
                            skipped += 1;
                            continue;
                        }
                    }
                } else {
                    // A non-PS3-compact-decl rejection (wavebank schema, unluac,
                    // truncated UCFX…) is shared between PS3 and Xbox DOH inputs
                    // on the DLC01 corpus and is out of scope for the oracle.
                    // Keep the historical skip path for parity with pre-change.
                    skipped += 1;
                    continue;
                }
            }
        };

        let (swapped, oracle_hit): (Vec<u8>, Option<(String, u32)>) = match decode {
            BlockDecode::Ps3Direct(s) => {
                ps3_direct_count += 1;
                (s, None)
            }
            BlockDecode::XboxDohSideOracle { le_bytes, xbox_path, name_hash } => {
                xbox_doh_sourced_count += 1;
                (le_bytes, Some((xbox_path, name_hash)))
            }
        };

        if let Some((xbox_path, name_hash)) = oracle_hit {
            // Light trace: one line per side-oracle hit, cheap at the 470-block
            // DLC01 scale. The caller can grep logs for `oracle-hit:` to audit.
            eprintln!(
                "  oracle-hit: ps3=[{blk_idx}] {path} ← xbox={xbox_path} \
                 (name_hash=0x{name_hash:08X})"
            );
        }

        // Recompress + round-trip verify.
        let pc_sges = compress_sges(&swapped)?;
        let rt = decompress_sges(&pc_sges)?;
        if rt != swapped {
            return Err(format!("block {blk_idx} ({path}): sges round-trip mismatch"));
        }

        // Recompute packed_field: (xbox_tier << 24) | ceil(size / PAGE_SIZE).
        //
        // The tier byte is inherited from the PS3 INDX entry even on an Xbox-DOH
        // side-oracle hit: the output WAD lives in the PS3 output's address
        // space, so the INDX layout (and the tier byte that lives in it) tracks
        // the PS3 side. The DECOMPRESSED SIZE is of course the Xbox-DOH LE
        // body's size, since that is what the engine will actually inflate.
        let xbox_tier = (e.packed_field >> 24) & 0xFF;
        let pages = ((swapped.len() + 0x7FFF) / 0x8000) as u32;
        let recomputed_packed = (xbox_tier << 24) | pages;

        converted.push(PatchBlock {
            compressed_data: pc_sges,
            path_string: path,
            aset_entries: Vec::new(),
            packed_field: recomputed_packed,
            flags: e.flags,
            source_block_index: None, // carries no ASET rows, so no rungs to remap
        });

        if (converted.len() + skipped) % 200 == 0 {
            println!("  [{}/{total}] converting...", converted.len() + skipped);
        }
    }
    println!(
        "  Converted: {} (ps3-direct: {}, xbox-doh-side-oracle: {}), Skipped: {skipped}",
        converted.len(),
        ps3_direct_count,
        xbox_doh_sourced_count
    );
    if converted.is_empty() {
        return Err("No blocks converted".into());
    }

    // ── ASET resolution: route every row to the block that actually owns its hash ──
    // Owner map + the owning entry's type_hash, built once from the converted blocks.
    // Xbox-DOH-sourced blocks contribute their entries' name_hashes here the same
    // way PS3-direct blocks do: the LE entry table is already in `compressed_data`
    // on both paths, so the PS3-side ASET rows for assets that lived in a rejected
    // PS3 block still route correctly against the Xbox-DOH-sourced body.
    let mut hash_owner: HashMap<u32, (usize, u32)> = HashMap::new();
    for (i, blk) in converted.iter().enumerate() {
        if let Ok(raw) = decompress_sges(&blk.compressed_data) {
            let (_, entries) = parse_block_entry_table(&raw);
            for ent in entries {
                hash_owner.entry(ent.name_hash).or_insert((i, ent.type_hash));
            }
        }
    }
    let (mut resolved, mut unresolved) = (0usize, 0usize);
    for mut ae in pending_aset {
        match hash_owner.get(&ae.asset_hash) {
            Some(&(i, type_hash)) => {
                // Refine type_id from the owning entry rather than trusting the Xbox row.
                if let Some(tid) = type_id_for_type_hash(type_hash) {
                    ae.u32_3 = tid;
                }
                converted[i].aset_entries.push(ae);
                resolved += 1;
            }
            // Hash owned by no shipped block: an Xbox row for a base-game asset that
            // lives in the retail WAD. Dropping it lets the base WAD resolve it instead
            // of shadowing it with a dangling patch row.
            None => unresolved += 1,
        }
    }
    println!("  ASET routed by content: {resolved}, dropped (not owned by any shipped block): {unresolved}");

    // ── Synthetic ASET for script / stringdb entries lacking a row ──
    let (mut script_added, mut stringdb_added) = (0usize, 0usize);
    for blk in converted.iter_mut() {
        let mut existing: std::collections::HashSet<u32> =
            blk.aset_entries.iter().map(|e| e.asset_hash).collect();
        let raw = match decompress_sges(&blk.compressed_data) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let (_, entries) = parse_block_entry_table(&raw);
        for ent in entries {
            let h = ent.name_hash;
            if h == 0 || existing.contains(&h) {
                continue;
            }
            if ent.type_hash == SCRIPT_TYPE_HASH {
                blk.aset_entries.push(AsetEntry::new(h, 0xFFFFFFFF, 0xFFFF, SCRIPT_ASET_TYPE_ID));
                existing.insert(h);
                script_added += 1;
            } else if ent.type_hash == STRINGDB_TYPE_HASH {
                blk.aset_entries.push(AsetEntry::new(h, 0xFFFFFFFF, 0xFFFF, STRINGDB_ASET_TYPE_ID));
                existing.insert(h);
                stringdb_added += 1;
            }
        }
    }
    if script_added + stringdb_added > 0 {
        println!("  ASET fix: +{script_added} script, +{stringdb_added} stringdb synthetic rows");
    }

    // ── Not-yet-ported passes ──
    // The import-chain/bootstrap passes deliberately stay out: they existed to graft the
    // DLC contracts into the *vz* master script. The DLC ships its own master script
    // (dlc01), so it is loaded as a level via LevelBootstrap.LoadLevel("dlc01","dlc01")
    // instead of being injected into Venezuela.
    eprintln!("  NOTE: bootstrap/import-chain injection intentionally omitted (dlc01 boots as its own level)");

    // ── Assemble FFCS patch WAD ──
    let wad = build_patch_wad_multi(&converted, csum_value, csum_meta, &FFCS_CERT_BLOB)?;
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    std::fs::write(&output, &wad).map_err(|e| format!("write: {e}"))?;
    println!(
        "  Output: {} ({} bytes / {:.1} MB), {} blocks (ps3-direct {}, xbox-doh-side-oracle {})",
        output.display(),
        wad.len(),
        wad.len() as f64 / 1024.0 / 1024.0,
        converted.len(),
        ps3_direct_count,
        xbox_doh_sourced_count
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dlc_port: error: {e}");
            ExitCode::FAILURE
        }
    }
}
