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
//! is routed to the converted blocks that actually own its `asset_hash`, one per
//! rung, and then has to agree with them. Rows owned by no shipped block are
//! dropped so the retail WAD resolves them instead.
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
//! `--java <path>` and `--unluac <path>` (both required) name the java executable and the
//! unluac jar that convert the DLC's Lua bytecode. `--x360-rar` also needs the RAR extractor
//! named explicitly: exactly one of `--unrar <path>` or `--bsdtar <path>`.
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

use clap::{ArgGroup, Parser};

use mercs2_formats::aset_type_ids::{
    type_id_for_type_hash, SCRIPT_ASET_TYPE_ID, SCRIPT_TYPE_HASH, STRINGDB_ASET_TYPE_ID,
    STRINGDB_TYPE_HASH,
};
use mercs2_formats::dlc_input::{
    parse_be_aset, parse_be_ffcs, parse_be_indx, parse_be_pths, read_be_block,
};
use mercs2_formats::dlc_stfs::{extract_stfs_from_rar, load_stfs_or_doh_with_streams, RarExtractor};
use mercs2_formats::patch_wad::{build_patch_wad_multi, AsetEntry, PatchBlock, FFCS_CERT_BLOB};
use mercs2_formats::sges::{compress_sges, decompress_sges};
use mercs2_formats::ucfx::parse_block_entry_table;
use mercs2_formats::be_to_le::audio::StreamTable;
use mercs2_formats::be_to_le::texture::{archive_textures, rung_of_path, BlockTables, TextureTable};
use ucfx_byteswap::convert::{convert_block, QUIET};
use ucfx_byteswap::ecs::ComponentSchemas;
use ucfx_byteswap::lua::LuaTools;

use dlc_port::xbox_doh_oracle::{is_ps3_compact_decl_rejection, primary_name_hash_be, XboxDohOracle};

#[derive(Parser)]
#[command(name = "dlc_port", about = "Port Xbox 360 / PS3 DLC to a PC vz-patch.wad (all-Rust)")]
#[command(group(ArgGroup::new("rar_extractor").args(["unrar", "bsdtar"])))]
struct Cli {
    /// Xbox 360 DLC RAR archive (needs exactly one of --unrar / --bsdtar)
    #[arg(long, requires = "rar_extractor")]
    x360_rar: Option<PathBuf>,
    /// UnRAR.exe that extracts --x360-rar
    #[arg(long, value_name = "UNRAR_PATH")]
    unrar: Option<PathBuf>,
    /// bsdtar that extracts --x360-rar
    #[arg(long, value_name = "BSDTAR_PATH")]
    bsdtar: Option<PathBuf>,
    /// java executable that runs unluac to convert the DLC's Lua bytecode
    #[arg(long, value_name = "JAVA_PATH")]
    java: PathBuf,
    /// unluac jar that converts the DLC's Lua bytecode
    #[arg(long, value_name = "UNLUAC_JAR")]
    unluac: PathBuf,
    /// The player's PC vz.wad, whose `schm` chunks lay out the DLC's compact ECS containers
    #[arg(long, value_name = "VZ_WAD")]
    vz_wad: PathBuf,
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
    let lua_tools = LuaTools { java: cli.java.clone(), unluac: cli.unluac.clone() };
    println!("Reading class layouts from {}...", cli.vz_wad.display());
    let schemas = ComponentSchemas::from_vz_wad(&cli.vz_wad)?;
    println!("  {} classes", schemas.len());
    let base_textures = archive_textures(&cli.vz_wad)?;

    // ── Step 1: load DOH bytes and the package's streams ──
    let (doh, package_files): (Vec<u8>, Vec<(String, Vec<u8>)>) = if let Some(rar) = &cli.x360_rar {
        println!("Step 1: Extracting STFS from RAR...");
        let work = std::env::temp_dir().join("dlc_port_rs");
        // clap's `requires = "rar_extractor"` on --x360-rar guarantees exactly one of these.
        let extractor = match (&cli.unrar, &cli.bsdtar) {
            (Some(p), None) => RarExtractor::UnRar(p.clone()),
            (None, Some(p)) => RarExtractor::Bsdtar(p.clone()),
            _ => return Err("--x360-rar needs exactly one of --unrar or --bsdtar".into()),
        };
        let reader = extract_stfs_from_rar(rar, &work, &extractor)?;
        let entry = reader.doh_entry().ok_or("No DOH in STFS")?;
        println!("  Reading DOH ({} bytes)...", entry.file_size);
        (reader.read_doh()?, reader.read_pws()?)
    } else if let Some(p) = &cli.x360_stfs {
        println!("Step 1: Loading {}...", p.display());
        let (doh, src, files) = load_stfs_or_doh_with_streams(p)?;
        println!("  Source: {src}, size: {} bytes", doh.len());
        (doh, files)
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
            Some(XboxDohOracle::load(p, &lua_tools, &schemas, &base_textures)?)
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
    // The Xbox block indices cannot be rebased arithmetically: the DLC's blocks are
    // not a contiguous run in the source index space, so `idx - min(idx)` misroutes
    // (measured: 1045 texture rows landing on the wrong block). Instead every row —
    // both per-block and the 0xFFFF "global" ones — is routed to the converted blocks
    // that actually contain its asset_hash, one per rung.
    println!("  ASET rows to route by content: {}", aset.len());

    println!("Reading texture rungs...");
    let textures = TextureTable::from_doh(&doh, &indx, &pths, &base_textures)?;
    println!("  {} textures", textures.textures.len());

    println!("Re-encoding the {} streams...", package_files.len());
    let streams = StreamTable::read(num_blocks, |i| read_be_block(&doh, &indx[i]), &package_files)?;
    let fitted: u64 = streams.streams().iter().map(|(_, s)| s.fitted_frames).sum();
    println!("  {} streams, ffmpeg's decode off the records' frame counts by {fitted} frames in all", streams.streams().len());

    // ── Step 3: per-block convert + recompress ──
    QUIET.store(true, Ordering::Relaxed); // silence convert_block's per-block diagnostics
    let end = num_blocks.min(cli.start_block + cli.max_blocks.unwrap_or(num_blocks));
    let mut converted: Vec<PatchBlock> = Vec::new();
    let mut skips: Vec<String> = Vec::new();
    let mut ps3_direct_count = 0usize;
    let mut xbox_doh_sourced_count = 0usize;
    let total = end - cli.start_block;

    for blk_idx in cli.start_block..end {
        let e = &indx[blk_idx];
        let path = pths.get(blk_idx).cloned().unwrap_or_else(|| format!("block_{blk_idx:05}"));
        // Decompress (segs) or XFCU passthrough.
        let decompressed: Vec<u8> = match read_be_block(&doh, e) {
            Ok(d) => d,
            Err(err) => {
                skips.push(format!("[{blk_idx}] {path}: {err}"));
                continue;
            }
        };

        // Convert BE→LE, routing a PS3-compact-decl rejection through the
        // optional Xbox-DOH side oracle. Anything the oracle cannot resolve
        // fails the whole run — no silent fallback — so the output WAD is
        // either 100 % resolved or does not exist.
        let block_textures = BlockTables { textures: &textures, rung: rung_of_path(&path), streams: Some(&streams) };
        let decode = match convert_block(&decompressed, false, None, Some(&lua_tools), Some(&schemas), Some(block_textures)) {
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
                            skips.push(format!("[{blk_idx}] {path}: {err}"));
                            continue;
                        }
                    }
                } else {
                    // A non-PS3-compact-decl rejection (wavebank schema, unluac,
                    // truncated UCFX…) is shared between PS3 and Xbox DOH inputs
                    // on the DLC01 corpus and is out of scope for the oracle.
                    // Keep the historical skip path for parity with pre-change.
                    skips.push(format!("[{blk_idx}] {path}: {err}"));
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
            source_block_index: Some(blk_idx as u32),
        });

        if (converted.len() + skips.len()) % 200 == 0 {
            println!("  [{}/{total}] converting...", converted.len() + skips.len());
        }
    }
    println!(
        "  Converted: {} (ps3-direct: {}, xbox-doh-side-oracle: {}), Skipped: {}",
        converted.len(),
        ps3_direct_count,
        xbox_doh_sourced_count,
        skips.len()
    );
    if !skips.is_empty() {
        return Err(format!("{} blocks did not convert, so no WAD was written:\n  {}", skips.len(), skips.join("\n  ")));
    }
    if converted.is_empty() {
        return Err("No blocks converted".into());
    }

    // ── ASET resolution: route every row to the blocks that actually own its hash ──
    // Owner map, built once from the converted blocks: each asset's block per rung.
    // Xbox-DOH-sourced blocks contribute their entries' name_hashes here the same
    // way PS3-direct blocks do: the LE entry table is already in `compressed_data`
    // on both paths, so the PS3-side ASET rows for assets that lived in a rejected
    // PS3 block still route correctly against the Xbox-DOH-sourced body.
    let mut owners: HashMap<u32, Vec<(u32, [Option<usize>; 4])>> = HashMap::new();
    for (i, blk) in converted.iter().enumerate() {
        let raw = decompress_sges(&blk.compressed_data)?;
        let rung = rung_of_path(&blk.path_string).ok_or_else(|| format!("{} names no rung", blk.path_string))?;
        let (_, entries) = parse_block_entry_table(&raw);
        for ent in entries {
            let types = owners.entry(ent.name_hash).or_default();
            if !types.iter().any(|(t, _)| *t == ent.type_hash) {
                types.push((ent.type_hash, [None; 4]));
            }
            let slots = &mut types.iter_mut().find(|(t, _)| *t == ent.type_hash).expect("pushed above").1;
            if slots[rung].replace(i).is_some() {
                return Err(format!("0x{:08X} is twice in rung {rung}, the second in {}", ent.name_hash, blk.path_string));
            }
        }
    }
    let (mut resolved, mut unresolved, mut chained) = (0usize, 0usize, 0usize);
    let mut source_blocks: HashMap<u16, usize> = HashMap::new();
    for ae in &aset {
        let Some(types) = owners.get(&ae.asset_hash) else {
            // Hash owned by no shipped block: an Xbox row for a base-game asset that
            // lives in the retail WAD. Dropping it lets the base WAD resolve it instead
            // of shadowing it with a dangling patch row.
            unresolved += 1;
            continue;
        };
        let (_, slots) = types
            .iter()
            .find(|(t, _)| type_id_for_type_hash(*t) == Some(ae.u3))
            .ok_or_else(|| format!("0x{:08X}: no block holds it as type {}", ae.asset_hash, ae.u3))?;
        let resident = slots[0].ok_or_else(|| format!("0x{:08X} has no block of rung 0", ae.asset_hash))?;
        // The row's own indices are in another space, but each must name one block throughout.
        for (r, source) in xbox_row_rungs(ae.u1, ae.u2).into_iter().enumerate() {
            let agrees = match (source, slots[r]) {
                (0xFFFF, block) => block.is_none(),
                (_, None) => false,
                (s, Some(i)) => *source_blocks.entry(s).or_insert(i) == i,
            };
            if !agrees {
                return Err(format!("0x{:08X} rung {r}: the Xbox row says {source:#06X}, the blocks say {:?}", ae.asset_hash, slots[r]));
            }
        }
        let (secondary, finer) = rung_chain(slots.map(|s| s.and_then(|i| converted[i].source_block_index)));
        chained += usize::from(finer != 0xFFFF);
        converted[resident].aset_entries.push(AsetEntry::new(ae.asset_hash, secondary, finer, ae.u3));
        resolved += 1;
    }
    println!("  ASET routed by content: {resolved}, {chained} with finer rungs, dropped (not owned by any shipped block): {unresolved}");

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
    // The game keeps its streams in `Audios` beside the WAD.
    let audios = output.with_file_name("Audios");
    for (name, stream) in streams.streams() {
        std::fs::create_dir_all(&audios).map_err(|e| format!("mkdir {}: {e}", audios.display()))?;
        let to = audios.join(name);
        std::fs::write(&to, &stream.pws).map_err(|e| format!("write {}: {e}", to.display()))?;
        println!("  Stream: {} ({} clips, {} bytes)", to.display(), stream.records.len(), stream.pws.len());
    }
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

/// An Xbox ASET row's block indices, `[_P000, _P001, _P002, _P003]`. Each word's halves are the
/// reverse of PC's (`docs/aset_format.md`).
fn xbox_row_rungs(u1: u32, u2: u32) -> [u16; 4] {
    [u2 as u16, (u2 >> 16) as u16, u1 as u16, (u1 >> 16) as u16]
}

/// An asset's source block per rung as ASET words: `([_P002 | _P003], _P001)`, `0xFFFF` for an absent
/// rung. The engine stops at the first absent rung, so a rung past a gap is absent too.
fn rung_chain(blocks: [Option<u32>; 4]) -> (u32, u32) {
    let rung = |r: usize| if blocks[..=r].iter().all(Option::is_some) { blocks[r].map_or(0xFFFF, |b| b & 0xFFFF) } else { 0xFFFF };
    ((rung(2) << 16) | rung(3), rung(1))
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

#[cfg(test)]
mod tests {
    use super::rung_chain;

    #[test]
    fn a_chain_packs_its_rungs_and_stops_at_a_gap() {
        assert_eq!(rung_chain([Some(1), Some(2), Some(3), Some(4)]), (0x0003_0004, 2));
        assert_eq!(rung_chain([Some(1), Some(2), None, None]), (0xFFFF_FFFF, 2));
        assert_eq!(rung_chain([Some(1), None, Some(3), Some(4)]), (0xFFFF_FFFF, 0xFFFF));
    }
}
