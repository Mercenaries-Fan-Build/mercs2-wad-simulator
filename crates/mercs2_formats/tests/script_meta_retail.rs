//! Every retail script container, rebuilt from nothing but its name and its LuaQ chunk, is the
//! container retail shipped.
//!
//! `script_meta::derive` computes INFO (name, globals count, inherit count) and DEPS (one hash per
//! literal `import`/`inherit` call) from the compiled chunk, and `scripts_block::build_container`
//! lays the container out. If both reproduce every retail script byte for byte, a script the
//! toolchain mints or recompiles carries the INFO and DEPS the engine expects from the same source.
//!
//! Covers every script entry of every block of `vz.wad` (the level, resident and per-contract
//! blocks) and of `shell.wad` (the front end), found by type hash. The counts are asserted exactly,
//! so a run that reads fewer scripts fails instead of passing.
//!
//! Game-gated: built by the `retail` feature, reads the `vz.wad` named by the repo-root
//! `.mercs2-local.toml` and the `shell.wad` beside it, and fails if either is absent.

use std::fs::File;
use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::script_meta::derive;
use mercs2_formats::scripts_block::{build_container, container_chunk, container_info, SCRIPT_FIELD_C};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_SCRIPT;

/// Script containers in retail `vz.wad` and `shell.wad`.
const VZ_SCRIPTS: usize = 645;
const SHELL_SCRIPTS: usize = 28;

fn vz_wad() -> PathBuf {
    mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
}

struct Script {
    what: String,
    field_c: u32,
    bytes: Vec<u8>,
    /// The block is stored uncompressed and this is its last entry: retail zeroes that entry's
    /// 8-byte trailer instead of writing a CSUM (`mercs2_exemap::wadscripts`).
    zeroed_trailer: bool,
}

fn scripts(path: &Path) -> Vec<Script> {
    let mut f = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let size = f.metadata().unwrap().len();
    let arch = load_ffcs_archive(&mut f, size).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut out = Vec::new();
    for bi in 0..arch.indx.len() {
        let block = decompress_block(&mut f, &arch.indx, bi as u16)
            .unwrap_or_else(|e| panic!("{} block {bi}: {e}", path.display()));
        let rd = |off: usize| u32::from_le_bytes(block[off..off + 4].try_into().unwrap());
        let count = rd(0) as usize;
        let mut pos = 4 + count * 16;
        for i in 0..count {
            let row = 4 + i * 16;
            let (name_hash, type_hash, field_c, size) = (rd(row), rd(row + 4), rd(row + 8), rd(row + 12) as usize);
            if type_hash == TYPE_HASH_SCRIPT {
                out.push(Script {
                    what: format!("{} block {bi} ({}) entry {i} 0x{name_hash:08x}", path.display(), arch.paths[bi]),
                    field_c,
                    bytes: block[pos..pos + size].to_vec(),
                    zeroed_trailer: !arch.indx[bi].is_compressed() && i + 1 == count,
                });
            }
            pos += size;
        }
    }
    out
}

/// Rebuild `s` from its INFO name and LuaQ; `Err` describes the first difference.
fn rebuild(s: &Script) -> Result<(), String> {
    if s.field_c != SCRIPT_FIELD_C {
        return Err(format!("field_c 0x{:08x}", s.field_c));
    }
    let name = container_info(&s.bytes)?.name;
    let luaq = container_chunk(&s.bytes, b"BINN")?.ok_or("no BINN")?;
    let meta = derive(&name, luaq)?;
    let rebuilt = build_container(&meta, luaq);
    if rebuilt.len() != s.bytes.len() {
        return Err(format!("{name}: rebuilt {} bytes, retail {} bytes", rebuilt.len(), s.bytes.len()));
    }
    let body = s.bytes.len() - 8;
    if let Some(at) = (0..body).find(|&i| rebuilt[i] != s.bytes[i]) {
        return Err(format!(
            "{name}: first difference at byte {at} (rebuilt {:02x}, retail {:02x}); derived globals {} inherits {} deps {}",
            rebuilt[at],
            s.bytes[at],
            meta.globals,
            meta.inherits,
            meta.deps.len()
        ));
    }
    let trailer = &s.bytes[body..];
    if s.zeroed_trailer {
        if trailer != [0u8; 8] {
            return Err(format!("{name}: expected a zeroed trailer, found {trailer:02x?}"));
        }
    } else if trailer != &rebuilt[body..] {
        return Err(format!("{name}: CSUM trailer differs"));
    }
    Ok(())
}

#[test]
fn every_retail_script_container_rebuilds_byte_for_byte() {
    let vz = vz_wad();
    let shell = vz.with_file_name("shell.wad");
    assert!(shell.is_file(), "{} is not a file; shell.wad sits beside vz.wad", shell.display());

    let mut failures = Vec::new();
    let mut zeroed = 0usize;
    for (wad, expected) in [(&vz, VZ_SCRIPTS), (&shell, SHELL_SCRIPTS)] {
        let all = scripts(wad);
        assert_eq!(all.len(), expected, "{}: script count", wad.display());
        for s in &all {
            zeroed += s.zeroed_trailer as usize;
            if let Err(e) = rebuild(s) {
                failures.push(format!("{}: {e}", s.what));
            }
        }
    }
    for f in &failures {
        eprintln!("MISMATCH {f}");
    }
    eprintln!(
        "rebuilt {} retail script containers ({VZ_SCRIPTS} vz + {SHELL_SCRIPTS} shell); {} differ; {zeroed} with a zeroed trailer",
        VZ_SCRIPTS + SHELL_SCRIPTS,
        failures.len()
    );
    assert!(failures.is_empty(), "{} of {} retail script containers do not rebuild", failures.len(), VZ_SCRIPTS + SHELL_SCRIPTS);
}
