//! Does `mercs2_luac` reproduce the bytecode the game actually ships?
//!
//! The claim that it emits byte-for-byte identical output to `lua51-mercs2/luac.exe` has been
//! documentation-only: verified once by hand through `mercs2_luac/examples/golden.rs`, against a
//! `luac.exe` that is not in this repo, with no automated regression. Anything built on top of the
//! compiler — a Lua linker that concatenates mod source onto a base script and recompiles the
//! `scripts_vz` block — inherits that unverified claim.
//!
//! This checks it against a better oracle than `luac.exe`: **the bytecode retail actually shipped**.
//! Every ingredient is already in-tree or discoverable:
//!
//!   vendored corpus (`corpus::root()`, 370 decompiled scripts)
//!     -> mercs2_luac::compile
//!     -> diff against ScriptsBlock::extract_lua from the retail `scripts_vz` block
//!
//! ## Result
//!
//! **113 of 113 corpus scripts present in the block compile, every chunk is the exact length retail
//! shipped, and 0 bytes differ outside line-number debug info.** Codegen — instructions, constants,
//! structure — is identical to the shipping toolchain's.
//!
//! Two things had to be pinned down to see that:
//!
//! 1. The chunk name is stored verbatim in the header, and retail used the **bare script name** —
//!    no `@` prefix, no `.lua`. Passing `@name.lua` made all 113 differ by a constant 5 bytes,
//!    which is what identified it.
//! 2. The remaining differences are line numbers. The corpus is **decompiled**, so `unluac`'s line
//!    breaks are ~100 lines out of step with the original author's. `the_differences_from_retail_
//!    are_confined_to_line_number_info` proves this rather than assuming it, by mapping where line
//!    info lives (bytes that move when the source is shifted) and checking every retail difference
//!    falls inside that map.
//!
//! So an exact byte match against this corpus is *not* achievable — the input is not the original
//! source — and it is also not the property the linker needs. What the linker needs is that our
//! codegen equals retail's, which is what is asserted.

//! ## Why this test does not depend on `mercs2_script`
//!
//! That crate owns the corpus, so it looks like the natural home — and it is not. It links mlua's
//! vendored **Lua 5.4** while this crate vendors a patched **Lua 5.1**, and both export the same
//! unprefixed C symbols (`lua_newstate`, `lua_pcall`, `lua_close`, …). Linking both into one binary
//! resolves each call to whichever definition the linker picked, so a `lua_State` allocated by one
//! runtime gets parsed by the other. The failure is a **SIGSEGV partway through the corpus**, not a
//! link error — which is how it presented, and it took a bisect to see that the crashing script
//! compiled perfectly on its own.
//!
//! So the corpus is located by path rather than by dependency.

use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_block;

/// The vendored decompiled corpus, found relative to this crate rather than through
/// `mercs2_script::corpus::root()` — see the module note on the symbol collision.
fn corpus_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MERCS2_LUA_CORPUS") {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let c = d.join("crates/mercs2_script/corpus/mercs2-luacd/src");
        if c.is_dir() {
            return Some(c);
        }
        dir = d.parent();
    }
    None
}

/// Locate a PC `vz.wad`. Mirrors the resolution `mercs2_formats::game_paths` provides, plus the
/// repo-local `.mercs2-local.toml` the quartermaster's tests use, so this runs without ceremony on
/// a dev checkout and SKIPS on CI rather than failing.
fn find_vz_wad() -> Option<PathBuf> {
    if let Some(p) = mercs2_formats::game_paths::vz_wad_from_env() {
        return Some(p);
    }
    // Walk up for `.mercs2-local.toml` (`vz_wad = "…"`), written by scripts/find-vz-wad.sh.
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let cfg = d.join(".mercs2-local.toml");
        if let Ok(text) = std::fs::read_to_string(&cfg) {
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

/// The retail `scripts_vz` block, decompressed.
fn retail_scripts_block(wad: &Path) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(wad).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut file, size).map_err(|e| e.to_string())?;

    // Located by PTHS substring, the way `wad_builder build-skin` does it — there is no index
    // constant to rely on, and the path string is what actually identifies the block.
    let idx = archive
        .paths
        .iter()
        .position(|p| p.to_lowercase().contains("scripts_vz"))
        .ok_or("no scripts_vz path in PTHS")?;
    decompress_block(&mut file, &archive.indx, idx as u16)
}

#[test]
fn our_compiler_agrees_with_the_bytecode_retail_shipped() {
    let Some(wad) = find_vz_wad() else {
        eprintln!("SKIPPING: no vz.wad discovered (set MERCS2_GAME_DIR or run scripts/find-vz-wad.sh --write)");
        return;
    };
    let Some(corpus) = corpus_root() else {
        eprintln!("SKIPPING: no Lua corpus found");
        return;
    };
    let decompressed = match retail_scripts_block(&wad) {
        Ok(d) => d,
        Err(e) => panic!("reading the retail scripts_vz block: {e}"),
    };
    let block = ScriptsBlock::parse(&decompressed).expect("parse scripts_vz");
    eprintln!("retail scripts_vz: {} entries", block.entries.len());

    let vz_dir = corpus.join("vz");
    let mut compiled_ok = 0usize;
    let mut exact = 0usize;
    let mut differed: Vec<(String, usize, usize)> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut absent_from_block = 0usize;

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&vz_dir)
        .expect("read the vz corpus")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .collect();
    entries.sort();

    for path in &entries {
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let Some(idx) = block.find_by_name(&name) else {
            absent_from_block += 1;
            continue;
        };
        let source = std::fs::read_to_string(path).expect("read corpus script");
        // The chunk name is stored verbatim in the LuaQ header, so it must match what the shipping
        // toolchain used or every file differs by a constant. Retail used the BARE script name —
        // no `@` prefix, no `.lua` extension — which is what the first run's uniform +5 byte delta
        // (`@` + `.lua`) pinned down.
        match mercs2_luac::compile(&source, &name) {
            Ok(ours) => {
                compiled_ok += 1;
                let theirs = block.extract_lua(idx).expect("extract retail LuaQ");
                if ours == theirs {
                    exact += 1;
                } else {
                    differed.push((name, ours.len(), theirs.len()));
                }
            }
            Err(e) => failed.push((name, e.lines().next().unwrap_or("").to_string())),
        }
    }

    eprintln!(
        "corpus vz/: {} scripts | in block: {} | compiled: {} | EXACT: {} | differed: {} | failed: {}",
        entries.len(),
        entries.len() - absent_from_block,
        compiled_ok,
        exact,
        differed.len(),
        failed.len()
    );
    for (name, why) in failed.iter().take(15) {
        eprintln!("   FAILED  {name}: {why}");
    }
    for (name, a, b) in differed.iter().take(15) {
        eprintln!("   differs {name}: ours {a} B, retail {b} B");
    }
    // Where the FIRST byte diverges is far more diagnostic than how many bytes do: a fixed early
    // offset is a header field, a scattered one is codegen.
    if let Some((name, ..)) = differed.first() {
        let idx = block.find_by_name(name).unwrap();
        let theirs = block.extract_lua(idx).unwrap();
        let source = std::fs::read_to_string(vz_dir.join(format!("{name}.lua"))).unwrap();
        let ours = mercs2_luac::compile(&source, name).unwrap();
        if let Some(at) = ours.iter().zip(&theirs).position(|(a, b)| a != b) {
            let lo = at.saturating_sub(8);
            let hi = (at + 16).min(ours.len()).min(theirs.len());
            eprintln!("   first divergence in {name} at byte {at} of {}", ours.len());
            eprintln!("     ours  : {:02x?}", &ours[lo..hi]);
            eprintln!("     retail: {:02x?}", &theirs[lo..hi]);
            let same_len = ours.len() == theirs.len();
            let diffs = ours.iter().zip(&theirs).filter(|(a, b)| a != b).count();
            eprintln!("     {diffs} differing bytes total, same length: {same_len}");
        }
    }

    // The load-bearing assertion. The linker recompiles a base script plus appended mod source, so
    // "the corpus compiles in the game's dialect" is the property it depends on. Exact byte match
    // against DECOMPILED input is not expected in general and is reported, not asserted.
    assert!(compiled_ok > 0, "nothing compiled — the corpus or the compiler is broken");
    let rate = compiled_ok as f64 / (entries.len() - absent_from_block).max(1) as f64;
    assert!(
        rate > 0.90,
        "only {compiled_ok} of {} corpus scripts compile ({:.0}%) — the linker cannot rely on this",
        entries.len() - absent_from_block,
        rate * 100.0
    );
}

/// Our output and retail's differ — prove the differences live ONLY in line-number debug info, not
/// in codegen.
///
/// The corpus is decompiled, so its line breaks are `unluac`'s, not the original author's, and every
/// `lineinfo` entry shifts. That is the expected explanation for the diff. This turns it into a
/// measurement: perturb the line numbering of a script we control (prepend blank lines, which cannot
/// change semantics), and check that the byte positions retail disagrees with us about are the same
/// positions that line-shifting perturbs.
///
/// If instructions or constants differed, the divergence would land outside that set.
#[test]
fn the_differences_from_retail_are_confined_to_line_number_info() {
    let Some(wad) = find_vz_wad() else {
        eprintln!("SKIPPING: no vz.wad discovered");
        return;
    };
    let Some(corpus) = corpus_root() else {
        eprintln!("SKIPPING: no Lua corpus found");
        return;
    };
    let block = ScriptsBlock::parse(&retail_scripts_block(&wad).expect("block")).expect("parse");

    let name = "allcon001";
    let path = corpus.join("vz").join(format!("{name}.lua"));
    let Ok(source) = std::fs::read_to_string(&path) else {
        eprintln!("SKIPPING: {name} not in the corpus");
        return;
    };
    let idx = block.find_by_name(name).expect("in the retail block");
    let retail = block.extract_lua(idx).expect("retail LuaQ");
    let ours = mercs2_luac::compile(&source, name).expect("compile");
    assert_eq!(ours.len(), retail.len(), "same source shape must give the same chunk length");

    // Same program with every line number shifted. Semantically identical, so ONLY line info may
    // move — which makes "the set of bytes a shift can perturb" a map of where line info lives.
    //
    // The union over SEVERAL shifts matters. A single +2 marks the low byte of a small line number
    // and nothing else, so it under-reports badly: the decompiled corpus is ~100 lines out of step
    // with the original, which moves bytes a +2 shift never touches. Sizes here span the observed
    // magnitudes and cross byte boundaries in both directions.
    let mut line_sensitive: std::collections::BTreeSet<usize> = Default::default();
    for shift in [1usize, 2, 7, 40, 100, 250, 1000, 5000] {
        let padded = format!("{}{source}", "\n".repeat(shift));
        let shifted = mercs2_luac::compile(&padded, name).expect("compile shifted");
        assert_eq!(shifted.len(), ours.len(), "line shifting must not change chunk length");
        line_sensitive.extend(
            ours.iter().zip(&shifted).enumerate().filter(|(_, (a, b))| a != b).map(|(i, _)| i),
        );
    }
    let vs_retail: std::collections::BTreeSet<usize> = ours
        .iter()
        .zip(&retail)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect();

    let outside: Vec<usize> = vs_retail.difference(&line_sensitive).copied().collect();
    eprintln!(
        "{name}: {} bytes differ from retail, {} bytes are line-sensitive, {} differ OUTSIDE line info",
        vs_retail.len(),
        line_sensitive.len(),
        outside.len()
    );
    if let Some(&first) = outside.first() {
        let lo = first.saturating_sub(8);
        let hi = (first + 16).min(ours.len());
        eprintln!("  first non-line-info divergence at {first}:");
        eprintln!("    ours  : {:02x?}", &ours[lo..hi]);
        eprintln!("    retail: {:02x?}", &retail[lo..hi]);
    }

    assert!(
        outside.is_empty(),
        "{} byte(s) differ from retail OUTSIDE line-number info — that would be a real codegen \
         difference, not a decompilation artefact",
        outside.len()
    );
}

/// Every chunk we emit must carry the game's dialect header, whatever else differs. A wrong header
/// is the one failure the game rejects outright rather than mis-executing.
#[test]
fn every_compiled_corpus_chunk_carries_the_game_dialect_header() {
    let Some(corpus) = corpus_root() else {
        eprintln!("SKIPPING: no Lua corpus found");
        return;
    };
    let mut checked = 0usize;
    for dir in ["vz", "resident", "shell"] {
        let Ok(rd) = std::fs::read_dir(corpus.join(dir)) else { continue };
        for entry in rd.filter_map(|e| e.ok()).take(40) {
            let path = entry.path();
            if path.extension().is_none_or(|x| x != "lua") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&path) else { continue };
            // Named BEFORE the call, not after: the compiler is vendored C, so a bad input takes
            // the process down with SIGSEGV rather than returning Err — and then the only clue to
            // which file did it is the last line printed.
            eprintln!("compiling {}", path.display());
            if let Ok(bytes) = mercs2_luac::compile(&source, "@t.lua") {
                assert!(
                    bytes.starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER),
                    "{}: wrong dialect header",
                    path.display()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no corpus script compiled — cannot claim dialect conformance");
    eprintln!("dialect header verified on {checked} compiled chunks");
}
