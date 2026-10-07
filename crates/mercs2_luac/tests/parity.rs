//! Every chunk `mercs2_luac` compiles from the vendored corpus carries the game's dialect header.
//!
//! Hermetic: reads only the in-tree corpus. The comparison against the bytecode retail shipped is
//! game-gated and lives in `parity_retail.rs`.

mod common;

/// Every chunk we emit must carry the game's dialect header, whatever else differs. A wrong header
/// is the one failure the game rejects outright rather than mis-executing.
#[test]
fn every_compiled_corpus_chunk_carries_the_game_dialect_header() {
    let Some(corpus) = common::corpus_root() else {
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
