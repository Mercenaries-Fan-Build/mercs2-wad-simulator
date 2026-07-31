//! Can this VM **load the bytecode the game shipped**?
//!
//! `tests/parity.rs` proves the compiler *emits* retail's dialect. This proves the other direction:
//! the runtime *reads* it. Together they close the loop — one Lua, and it is the game's.
//!
//! That matters beyond tidiness. The reimpl runs the **decompiled** corpus, so everything it
//! executes inherits the decompiler's fidelity. Loading the shipped chunks directly removes that
//! dependency for anything that does not need readable source, and it is free here: `lundump` was
//! already patched to read 4-byte string lengths and a `sizeof(size_t)=4` header, because the
//! compiler needs the same dialect on the dump side.
//!
//! It also could not have been written before. Executing retail bytecode needs a **runtime**, and
//! this crate used to be compiler-only precisely because a second Lua in the process was a SIGSEGV
//! (see the module note in `parity.rs`). There is one Lua now.

use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_block;
use mercs2_luac::rt::{Function, Lua};

/// Locate a PC `vz.wad`, or `None` to skip. Same resolution as `parity.rs`: the env var the rest of
/// the workspace uses, then the repo-local `.mercs2-local.toml` a dev checkout writes.
fn find_vz_wad() -> Option<PathBuf> {
    if let Some(p) = mercs2_formats::game_paths::vz_wad_from_env() {
        return Some(p);
    }
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join(".mercs2-local.toml")) {
            if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("vz_wad")) {
                if let Some(raw) = line.split('=').nth(1) {
                    let p = PathBuf::from(raw.trim().trim_matches('"'));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
        dir = d.parent();
    }
    None
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

/// The round trip, without needing a game install: our own compiler's output must load and run in
/// our own runtime. If the dump and undump halves ever disagree, this fails before the WAD test
/// gets a chance to be skipped on a machine with no install.
#[test]
fn our_own_bytecode_loads_and_runs() {
    let bytes = mercs2_luac::compile("return 6 * 7", "roundtrip").expect("compile");
    assert_eq!(&bytes[..4], b"\x1bLua", "compiled to a binary chunk");

    let lua = Lua::new().expect("vm");
    // Loading BYTES, not source — `luaL_loadbuffer` dispatches on the signature.
    let n: f32 = lua.load(&bytes).eval().expect("load + run the compiled chunk");
    assert_eq!(n, 42.0);
}

/// A precompiled chunk and its source must produce the same result, so "we loaded bytecode" is not
/// quietly "we reparsed text".
#[test]
fn bytecode_and_source_agree() {
    let src = "local t = {} for i = 1, 5 do t[i] = i * i end return t[1] + t[2] + t[3] + t[4] + t[5]";
    let lua = Lua::new().expect("vm");

    let from_source: f32 = lua.load(src).eval().expect("source");
    let compiled = mercs2_luac::compile(src, "agree").expect("compile");
    let from_bytecode: f32 = lua.load(&compiled).eval().expect("bytecode");

    assert_eq!(from_source, from_bytecode);
    assert_eq!(from_source, 55.0);
}

/// **The claim**: every chunk in the retail `scripts_vz` block loads into this VM.
///
/// Loading is the meaningful assertion, not running — running a mission script would need the whole
/// 1086-cfunc engine binding surface. What load proves is that `lundump` accepts retail's header
/// (`sizeof(size_t)=4`, `lua_Number` 4-byte float) and its instruction stream, i.e. that the VM the
/// reimpl embeds is the VM the game shipped.
#[test]
fn every_retail_chunk_loads_into_this_vm() {
    let Some(wad) = find_vz_wad() else {
        eprintln!("SKIPPING: no vz.wad discovered (set MERCS2_GAME_DIR or run scripts/find-vz-wad.sh --write)");
        return;
    };
    let decompressed = match retail_scripts_block(&wad) {
        Ok(d) => d,
        Err(e) => panic!("reading the retail scripts_vz block: {e}"),
    };
    let block = ScriptsBlock::parse(&decompressed).expect("parse scripts_vz");

    let lua = Lua::new().expect("vm");
    let mut loaded = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (idx, entry) in block.entries.iter().enumerate() {
        let Ok(chunk) = block.extract_lua(idx) else { continue };
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
    let Some(wad) = find_vz_wad() else {
        eprintln!("SKIPPING: no vz.wad discovered");
        return;
    };
    let decompressed = retail_scripts_block(&wad).expect("scripts_vz");
    let block = ScriptsBlock::parse(&decompressed).expect("parse");

    let lua = Lua::new().expect("vm");
    let mut checked = 0usize;
    for (idx, entry) in block.entries.iter().enumerate() {
        let Ok(chunk) = block.extract_lua(idx) else { continue };
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
