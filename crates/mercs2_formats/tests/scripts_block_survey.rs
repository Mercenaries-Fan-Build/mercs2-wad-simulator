//! Can every `scripts_vz` entry actually be relinked?
//!
//! A Lua linker rewrites script bytecode inside the block: read the base script, append each
//! Shipment's declared source, recompile, splice the new LuaQ back in. `ScriptsBlock::replace_lua`
//! is the splice, and it **hard-errors** when a container's `BINN` body is larger than its LuaQ
//! payload — "metadata-bearing BINN not yet supported" (`scripts_block.rs`). Anything trailing the
//! bytecode inside `BINN` would be silently dropped by a naive splice, so it refuses instead.
//!
//! Nobody has checked whether retail actually ships such a container. If even one of the 114 does,
//! the linker cannot relink that script and the plan needs a branch for it; if none does, the
//! restriction is theoretical and the approach is clear. That is a cheap question with an expensive
//! wrong answer, so it is measured here rather than assumed.

use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::{parse_container, ScriptsBlock};
use mercs2_formats::sges::decompress_block;
use mercs2_formats::types::TYPE_HASH_SCRIPT;

fn vz_wad() -> Option<PathBuf> {
    mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
}

/// Every block whose PTHS path names a script block, decompressed.
fn script_blocks(wad: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut file = std::fs::File::open(wad).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = load_ffcs_archive(&mut file, size).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for (idx, path) in archive.paths.iter().enumerate() {
        if !path.to_lowercase().contains("scripts_vz") {
            continue;
        }
        let dec = decompress_block(&mut file, &archive.indx, idx as u16)?;
        out.push((path.clone(), dec));
    }
    Ok(out)
}

#[test]
fn no_retail_script_container_carries_metadata_after_its_bytecode() {
    let Some(wad) = vz_wad() else {
        eprintln!(
            "SKIPPING: no vz.wad (set MERCS2_GAME_DIR or run scripts/find-vz-wad.sh --write)"
        );
        return;
    };
    let blocks = script_blocks(&wad).expect("read the script blocks");
    assert!(!blocks.is_empty(), "no scripts_vz block found in the WAD");

    let mut total = 0usize;
    let mut metadata_bearing: Vec<(String, usize, usize)> = Vec::new();
    let mut unparseable: Vec<(u32, String)> = Vec::new();
    let mut wrong_type = 0usize;

    for (path, dec) in &blocks {
        let block = ScriptsBlock::parse(dec).unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("{path}: {} entries", block.entries.len());
        for entry in &block.entries {
            total += 1;
            if entry.type_hash != TYPE_HASH_SCRIPT {
                wrong_type += 1;
            }
            match parse_container(&entry.bytes) {
                Ok(layout) => {
                    // The exact predicate `replace_lua` refuses on. `binn_body_size` is the
                    // descriptor's declared size (u32); `luaq_len` is the measured payload.
                    let binn = layout.binn_body_size as usize;
                    if binn != layout.luaq_len {
                        metadata_bearing.push((
                            format!("0x{:08X}", entry.name_hash),
                            binn,
                            layout.luaq_len,
                        ));
                    }
                }
                Err(e) => unparseable.push((entry.name_hash, e)),
            }
        }
    }

    eprintln!(
        "surveyed {total} script containers | metadata-bearing: {} | unparseable: {} | \
         non-script type_hash: {wrong_type}",
        metadata_bearing.len(),
        unparseable.len()
    );
    for (name, binn, luaq) in metadata_bearing.iter().take(20) {
        eprintln!(
            "   METADATA {name}: BINN body {binn} B vs LuaQ {luaq} B (+{})",
            binn - luaq
        );
    }
    for (hash, why) in unparseable.iter().take(10) {
        eprintln!("   UNPARSEABLE 0x{hash:08X}: {why}");
    }

    assert!(total > 0, "no script containers surveyed");
    assert!(
        unparseable.is_empty(),
        "{} container(s) do not parse — the linker cannot round-trip what it cannot read",
        unparseable.len()
    );
    assert!(
        metadata_bearing.is_empty(),
        "{} of {total} containers carry metadata after their bytecode, so `replace_lua` refuses \
         them. The linker needs a branch for these; see the listing above.",
        metadata_bearing.len()
    );
}

/// The linker round-trips a block it did not build. Splicing identical bytecode back in must
/// reproduce the block byte for byte, or the rebuild is losing something before any mod is involved.
#[test]
fn replacing_bytecode_with_itself_round_trips_the_block() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let blocks = script_blocks(&wad).expect("read the script blocks");
    for (path, dec) in &blocks {
        let mut block = ScriptsBlock::parse(dec).expect("parse");
        assert!(
            block.verify_csums().is_ok(),
            "{path}: retail CSUMs must verify as shipped"
        );

        for i in 0..block.entries.len() {
            let original = block.extract_lua(i).expect("extract");
            block
                .replace_lua(i, &original)
                .expect("replace with identical bytes");
        }
        let rebuilt = block.serialize();
        assert_eq!(
            rebuilt.len(),
            dec.len(),
            "{path}: rebuilt block changed length after a no-op replace"
        );
        assert!(
            rebuilt == *dec,
            "{path}: rebuilt block differs from the original bytes"
        );
        // And it must still verify after the round trip.
        ScriptsBlock::parse(&rebuilt)
            .expect("re-parse")
            .verify_csums()
            .expect("CSUMs after rebuild");
    }
}

/// ★ Adding a NEW script to `scripts_vz` produces a block that still round-trips and CSUM-verifies,
/// and the new script resolves by name while every existing script is byte-for-byte untouched.
///
/// This proves the block-side half of the DLC's own recipe for a new importable script (the ASET
/// row is the caller's half — see `add_script`'s doc). Uses an existing script's real LuaQ as the
/// new payload so the test needs no compiler.
#[test]
fn adding_a_new_script_round_trips_and_leaves_the_others_intact() {
    let Some(wad) = vz_wad() else {
        eprintln!("SKIPPING: no vz.wad");
        return;
    };
    let blocks = script_blocks(&wad).expect("read the script blocks");
    let (_, dec) = blocks.into_iter().next().expect("a scripts_vz block");

    let before = ScriptsBlock::parse(&dec).expect("parse base");
    let n_before = before.entries.len();
    // Snapshot the existing containers so we can prove none moved.
    let originals: Vec<Vec<u8>> = before.entries.iter().map(|e| e.bytes.clone()).collect();
    // Borrow a real script's LuaQ as the new script's bytecode.
    let payload = before.extract_lua(0).expect("extract a template LuaQ");

    let mut block = ScriptsBlock::parse(&dec).expect("parse for edit");
    let name = "qm_test_new_script";
    assert!(block.find_by_name(name).is_none(), "the test name must be novel");
    let idx = block.add_script(name, &payload).expect("add_script");
    assert_eq!(idx, n_before, "the new entry appends after the existing ones");

    // Round-trip: serialize → re-parse.
    let out = block.serialize();
    let reparsed = ScriptsBlock::parse(&out).expect("re-parse the extended block");
    assert_eq!(reparsed.entries.len(), n_before + 1, "one more entry");
    reparsed.verify_csums().expect("every container, new one included, must CSUM-verify");

    // The new script resolves by name, is a script type, and carries our payload.
    let found = reparsed
        .find_script_by_name(name)
        .expect("the new script must resolve by name");
    assert_eq!(
        reparsed.extract_lua(found).expect("extract new"),
        payload,
        "the new script must carry exactly the bytecode we added"
    );

    // Every ORIGINAL container is byte-for-byte unchanged — adding one must disturb nothing.
    for (i, orig) in originals.iter().enumerate() {
        assert_eq!(&reparsed.entries[i].bytes, orig, "existing script {i} was modified");
    }

    // Refuses a duplicate.
    let mut dup = ScriptsBlock::parse(&out).expect("parse extended");
    assert!(
        dup.add_script(name, &payload).is_err(),
        "adding a second entry for the same name must be refused"
    );
}
