//! `mopp_swap` — PHY2 MOPP-swap tool + OFFLINE gate.
//!
//! Given a real Mercs2 cell's `PHY2` collision chunk, replace its baked `hkpMoppCode` with a freshly
//! compiled MOPP and rebuild a valid Havok packfile — then PROVE it offline (re-parse + re-decode, no
//! game). Three modes:
//!   --identity    re-serialize the SAME decoded MOPP bytes (plumbing check; byte-identical output)
//!   --return-all  encode_return_all(N): a universal-collision MOPP over N=source key count (no verts)
//!   --spatial     encode(tris,verts) from the co-located WpMeshShape16 (frame-fidelity caveat)
//! With none of those flags it runs ALL THREE.
//!
//! Source selection:
//!   --block <N>          the vz.wad block to read (u16; default 767 when absent; 826 = terrain)
//!   --mopp <IDX>         a specific hkpMoppCode index within the chosen body (usize; when absent, the
//!                        first single-subpart MOPP with clean [0..N-1] keys — an error if the body has none)
//!   --body <FILE>        a raw PHY2 body dumped to disk (skips vz.wad; exclusive with --block)
//!   --out <DIR>          write the patched bodies here (required)
//! A flag given without a value, or with a value that does not parse, is an error naming the flag and
//! the value (exit 2); defaults apply only when the flag is absent.
//!
//! Usage:
//!   cargo run -p mercs2_probe --bin mopp_swap -- --out out/mopp
//!   cargo run -p mercs2_probe --bin mopp_swap -- --block 767 --return-all --out out/mopp
//!   cargo run -p mercs2_probe --bin mopp_swap -- --body cell.phy2 --out out/mopp

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::game_paths;
use mercs2_formats::phy2_moppswap::{
    count_phy2_mopps, decode_phy2_mopp_keys, swap_phy2_mopp, validate_swapped_body, MoppSwapReport,
    OfflineGate, SwapMode,
};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::ucfx::{extract_chunk_body, parse_block_entry_table};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn sha256_hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

fn arg_val(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}
fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// Parse the value of `name`: `Ok(None)` when the flag is absent, an error naming the flag when it has
/// no value or its value does not parse as `T`.
fn parse_flag<T: std::str::FromStr>(args: &[String], name: &str, what: &str) -> Result<Option<T>, String>
where
    T::Err: std::fmt::Display,
{
    if !has_flag(args, name) {
        return Ok(None);
    }
    let v = arg_val(args, name).ok_or_else(|| format!("{name} requires a value ({what})"))?;
    v.parse::<T>().map(Some).map_err(|e| format!("{name} {v:?} is not a valid {what}: {e}"))
}

/// Report a bad command-line flag and exit with the usage status.
fn flag_error(msg: String) -> ! {
    eprintln!("mopp_swap: {msg}");
    std::process::exit(2);
}

/// Extract a PHY2 body (that carries at least one hkpMoppCode) from a vz.wad block. Returns
/// `(body, container_hash)`.
fn load_phy2_from_wad(block: u16) -> Result<(Vec<u8>, u32), String> {
    let path = game_paths::vz_wad(Path::new("."))
        .ok_or("vz.wad not found — set MERCS2_GAME_DIR or .mercs2-local.toml, or pass --body")?;
    let mut f = std::fs::File::open(&path).map_err(|e| format!("open {path:?}: {e}"))?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let arch = load_ffcs_archive(&mut f, size).map_err(|e| format!("ffcs: {e}"))?;
    let dec = decompress_block(&mut f, &arch.indx, block).map_err(|e| format!("block {block}: {e}"))?;
    let (count, entries) = parse_block_entry_table(&dec);
    let mut pos = 4 + count as usize * 16;
    for e in &entries {
        let end = pos + e.chunk_size as usize;
        if end > dec.len() {
            break;
        }
        if let Some(body) = extract_chunk_body(&dec[pos..end], b"PHY2") {
            if count_phy2_mopps(&body) > 0 {
                return Ok((body, e.name_hash));
            }
        }
        pos = end;
    }
    Err(format!("no PHY2 chunk carrying an hkpMoppCode found in block {block}"))
}

/// The first MOPP index whose source keys are a perfect contiguous [0..N-1] run.
fn first_contiguous_mopp(body: &[u8]) -> Option<usize> {
    for i in 0..count_phy2_mopps(body) {
        if let Ok(d) = decode_phy2_mopp_keys(body, i) {
            let (ks, range, missing) = d.key_summary();
            if !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1)) {
                return Some(i);
            }
        }
    }
    None
}

fn mode_name(m: SwapMode) -> &'static str {
    match m {
        SwapMode::Identity => "identity",
        SwapMode::ReturnAll => "return-all",
        SwapMode::Empty => "empty",
        SwapMode::Spatial => "spatial",
    }
}

fn print_gate(g: &OfflineGate) -> bool {
    let ok = g.reparse_ok
        && g.mopp_present
        && g.decode_clean
        && g.decode_coverage_full
        && g.keys_as_expected
        && g.mesh_still_decodes;
    println!(
        "    re-parse:{}  mopp-present:{}  decode-clean:{}  full-coverage:{}  keys-expected:{}  mesh-survives:{}  -> {}",
        yn(g.reparse_ok), yn(g.mopp_present), yn(g.decode_clean), yn(g.decode_coverage_full),
        yn(g.keys_as_expected), yn(g.mesh_still_decodes),
        if ok { "PASS" } else { "FAIL" }
    );
    if let Some(e) = &g.reparse_err {
        println!("    re-parse error: {e}");
    }
    ok
}
fn yn(b: bool) -> &'static str {
    if b { "OK" } else { "XX" }
}

#[allow(clippy::too_many_arguments)]
fn run_mode(
    body: &[u8],
    src_sha: &str,
    idx: usize,
    mode: SwapMode,
    out_dir: &Path,
    tag: &str,
    src_contiguous: bool,
) -> Result<bool, String> {
    println!("\n── mode: {} ──", mode_name(mode));
    if matches!(mode, SwapMode::ReturnAll | SwapMode::Spatial) && !src_contiguous {
        println!(
            "    CAVEAT: source keys are NOT a contiguous [0..N-1] run (multi-subpart cell). {} emits\n\
             \x20   plain [0..N-1] keys, which do NOT match this mesh's sparse subpart-encoded key\n\
             \x20   namespace — offline-valid, but NOT an in-game-faithful swap. Target a single-subpart cell.",
            mode_name(mode)
        );
    }
    let src_keys = {
        let d = decode_phy2_mopp_keys(body, idx)?;
        d.keys
    };
    let (new_body, rep): (Vec<u8>, MoppSwapReport) = swap_phy2_mopp(body, idx, mode)?;
    let new_sha = sha256_hex(&new_body);

    let expected = if mode == SwapMode::Identity { Some(src_keys.as_slice()) } else { None };
    let g = validate_swapped_body(&new_body, idx, mode, expected);
    let ok = print_gate(&g);

    println!(
        "    m_data buffer: {} -> {} B   packfile: {} -> {} B  (delta {:+})   {}",
        rep.old_buf_len, rep.new_buf_len, rep.old_packfile_size, rep.new_packfile_size, rep.size_delta(),
        if rep.grew { "GREW: appended + offsets/fixups/prefix recomputed" } else { "in-place overwrite (no shift)" }
    );
    if rep.info_rewritten {
        println!("    m_info: rewritten (spatial quantization frame)");
    }
    println!("    body bytes: {} -> {}  (delta {:+})", body.len(), new_body.len(), new_body.len() as i64 - body.len() as i64);
    println!("    sha256 src : {src_sha}");
    println!("    sha256 out : {new_sha}");
    if mode == SwapMode::Identity {
        println!("    identity byte-identical to source: {}", yn(new_body == *body));
    }

    let out = out_dir.join(format!("{tag}_{}.phy2", mode_name(mode)));
    std::fs::write(&out, &new_body).map_err(|e| format!("write {out:?}: {e}"))?;
    println!("    wrote {} ({} B) sha256 {}", out.display(), new_body.len(), new_sha);
    Ok(ok)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let Some(out_dir) = arg_val(&args, "--out").map(PathBuf::from) else {
        eprintln!("mopp_swap: usage: mopp_swap --out <DIR> [--block <N> | --body <FILE>] [--mopp <IDX>] [--identity] [--return-all] [--empty] [--spatial]");
        std::process::exit(2);
    };
    // Every flag value is validated before any file is touched.
    let block_arg: Option<u16> =
        parse_flag(&args, "--block", "vz.wad block index (u16)").unwrap_or_else(|e| flag_error(e));
    let mopp_arg: Option<usize> =
        parse_flag(&args, "--mopp", "hkpMoppCode index (usize)").unwrap_or_else(|e| flag_error(e));
    if has_flag(&args, "--body") && arg_val(&args, "--body").is_none() {
        flag_error("--body requires a value (PHY2 body file)".into());
    }
    let body_arg = arg_val(&args, "--body");
    if body_arg.is_some() && block_arg.is_some() {
        flag_error("--body and --block are exclusive (the source is either a dumped body or a vz.wad block)".into());
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("create {}: {e}", out_dir.display());
        std::process::exit(1);
    }

    // Source body.
    let (body, tag): (Vec<u8>, String) = if let Some(bf) = body_arg {
        let Some(stem) = Path::new(&bf).file_stem().and_then(|s| s.to_str()) else {
            flag_error(format!("--body {bf:?} has no UTF-8 file stem to name the outputs after"));
        };
        let stem = stem.to_string();
        match std::fs::read(&bf) {
            Ok(b) => (b, stem),
            Err(e) => {
                eprintln!("read {bf}: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let block: u16 = block_arg.unwrap_or(767);
        match load_phy2_from_wad(block) {
            Ok((b, hash)) => (b, format!("blk{block}_0x{hash:08X}")),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    };

    let n_mopps = count_phy2_mopps(&body);
    if n_mopps == 0 {
        eprintln!("PHY2 body carries no hkpMoppCode");
        std::process::exit(1);
    }
    let idx: usize = match mopp_arg {
        Some(i) => {
            if i >= n_mopps {
                flag_error(format!("--mopp {i} is out of range (the PHY2 body carries {n_mopps} MOPP(s))"));
            }
            i
        }
        None => first_contiguous_mopp(&body).unwrap_or_else(|| {
            eprintln!(
                "mopp_swap: none of the body's {n_mopps} MOPP(s) has clean single-subpart [0..N-1] keys; \
                 pass --mopp <IDX> to choose one"
            );
            std::process::exit(1);
        }),
    };

    let src_sha = sha256_hex(&body);
    let src_dec = decode_phy2_mopp_keys(&body, idx).unwrap_or_else(|e| {
        eprintln!("source MOPP decode failed: {e}");
        std::process::exit(1);
    });
    let (ks, range, missing) = src_dec.key_summary();
    let contiguous = !ks.is_empty() && missing.is_empty() && range == Some((0, ks.len() as u32 - 1));

    println!("=== mopp_swap — PHY2 MOPP swap + offline gate ===");
    println!("source: {tag}  ({} B PHY2 body, sha256 {src_sha})", body.len());
    println!("MOPPs in body: {n_mopps}  |  target index: {idx}");
    println!(
        "source MOPP: {} keys, range {:?}, {} missing -> {} keys{}",
        ks.len(),
        range,
        missing.len(),
        src_dec.keys.len(),
        if contiguous { "  [single-subpart: clean [0..N-1]]" } else { "  [multi-subpart / gaps]" }
    );

    // Which modes?
    let want: Vec<SwapMode> = {
        let mut v = Vec::new();
        if has_flag(&args, "--identity") {
            v.push(SwapMode::Identity);
        }
        if has_flag(&args, "--return-all") {
            v.push(SwapMode::ReturnAll);
        }
        if has_flag(&args, "--empty") {
            v.push(SwapMode::Empty);
        }
        if has_flag(&args, "--spatial") {
            v.push(SwapMode::Spatial);
        }
        if v.is_empty() {
            v = vec![SwapMode::Identity, SwapMode::ReturnAll, SwapMode::Spatial];
        }
        v
    };

    let mut all_ok = true;
    let mut any_fail_mode = Vec::new();
    for m in want {
        match run_mode(&body, &src_sha, idx, m, &out_dir, &tag, contiguous) {
            Ok(true) => {}
            Ok(false) => {
                all_ok = false;
                any_fail_mode.push(mode_name(m));
            }
            Err(e) => {
                println!("\n── mode: {} ── ERROR: {e}", mode_name(m));
                all_ok = false;
                any_fail_mode.push(mode_name(m));
            }
        }
    }

    println!("\n=== OFFLINE GATE: {} ===", if all_ok { "ALL PASS" } else { "FAIL" });
    if !all_ok {
        println!("failing modes: {any_fail_mode:?}");
        std::process::exit(1);
    }
}
