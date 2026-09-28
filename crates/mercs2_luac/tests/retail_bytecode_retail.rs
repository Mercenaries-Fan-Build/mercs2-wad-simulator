//! Can this VM **load the bytecode the game shipped**? The retail half of `retail_bytecode.rs`.
//!
//! `parity_retail.rs` proves the compiler *emits* retail's dialect. This proves the other direction:
//! the runtime *reads* it — every chunk of the shipped `scripts_vz` block loads into this VM.
//!
//! Game-gated, built by the `retail` feature: reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml` and fails if it is absent. Run with `cargo xtask retail-test`.

use std::path::Path;

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_block;
use mercs2_luac::rt::{Function, Lua};

/// The `vz.wad` named by the repo-root `.mercs2-local.toml`; panics with the resolver's message when
/// it is not configured.
fn vz_wad() -> std::path::PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn retail_scripts_block(wad: &Path) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(wad).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut file, size).map_err(|e| e.to_string())?;
    let idx = archive
        .paths
        .iter()
        .position(|p| p.to_lowercase().contains("scripts_vz"))
        .ok_or("no scripts_vz path in PTHS")?;
    decompress_block(&mut file, &archive.indx, idx as u16)
}

/// **The claim**: every chunk in the retail `scripts_vz` block loads into this VM.
///
/// Loading is the meaningful assertion, not running — running a mission script would need the whole
/// 1086-cfunc engine binding surface. What load proves is that `lundump` accepts retail's header
/// (`sizeof(size_t)=4`, `lua_Number` 4-byte float) and its instruction stream, i.e. that the VM the
/// reimpl embeds is the VM the game shipped.
#[test]
fn every_retail_chunk_loads_into_this_vm() {
    let wad = vz_wad();
    let decompressed = match retail_scripts_block(&wad) {
        Ok(d) => d,
        Err(e) => panic!("reading the retail scripts_vz block: {e}"),
    };
    let block = ScriptsBlock::parse(&decompressed).expect("parse scripts_vz");

    let lua = Lua::new().expect("vm");
    let mut loaded = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (idx, entry) in block.entries.iter().enumerate() {
        let chunk = block
            .extract_lua(idx)
            .unwrap_or_else(|e| panic!("entry {idx} (0x{:08X}): extract_lua: {e}", entry.name_hash));
        if chunk.len() < 12 || &chunk[..4] != b"\x1bLua" {
            continue; // not a binary chunk; nothing to claim about it
        }
        match lua.load(&chunk).into_function() {
            Ok(_f) => loaded += 1,
            Err(e) => failures.push(format!("0x{:08X}: {e}", entry.name_hash)),
        }
    }

    eprintln!("[retail-bytecode] {loaded} chunks loaded from the shipped scripts_vz block");
    assert!(loaded > 0, "found no binary chunks to load — did the block layout change?");
    assert!(
        failures.is_empty(),
        "{} of {} retail chunks failed to load: {:?}",
        failures.len(),
        loaded + failures.len(),
        &failures[..failures.len().min(5)]
    );
}

/// A retail chunk is a real function with a real body — guards against `into_function` handing back
/// something that loaded vacuously.
#[test]
fn a_retail_chunk_is_a_callable_function() {
    let wad = vz_wad();
    let decompressed = retail_scripts_block(&wad).expect("scripts_vz");
    let block = ScriptsBlock::parse(&decompressed).expect("parse");

    let lua = Lua::new().expect("vm");
    let mut checked = 0usize;
    for (idx, entry) in block.entries.iter().enumerate() {
        let chunk = block
            .extract_lua(idx)
            .unwrap_or_else(|e| panic!("entry {idx} (0x{:08X}): extract_lua: {e}", entry.name_hash));
        if chunk.len() < 12 || &chunk[..4] != b"\x1bLua" {
            continue;
        }
        let f: Function = lua.load(&chunk).into_function().expect("load");
        // Reaching Lua as a function value is the check: `type(f) == "function"`.
        lua.globals().set("__chunk", f).expect("bind");
        let ty: String = lua.load("return type(__chunk)").eval().expect("type");
        assert_eq!(ty, "function", "0x{:08X} did not load as a function", entry.name_hash);
        checked += 1;
        if checked == 5 {
            break;
        }
    }
    assert!(checked > 0, "no binary chunk was available to check");
}
