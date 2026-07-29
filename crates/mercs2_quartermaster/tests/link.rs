//! Does the linker actually let two script mods coexist?
//!
//! That is the failure the whole `patch_lua`-as-a-mutation design exists to prevent: two Shipments
//! each shipping a finished `scripts_vz` do not merge and do not error — the later one wins and the
//! earlier one's Lua vanishes silently. Every other property here is secondary to the one test that
//! installs two mods and checks both survive.

use std::path::{Path, PathBuf};

use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_block;
use mercs2_quartermaster::link::{self, ScriptMutation};

fn corpus_root() -> Option<PathBuf> {
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

/// A retail block's raw decompressed bytes, located by a PTHS substring.
fn retail_block_bytes(needle: &str) -> Option<Vec<u8>> {
    let wad = mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let mut file = std::fs::File::open(&wad).ok()?;
    let size = file.metadata().ok()?.len();
    let archive = load_ffcs_archive(&mut file, size).ok()?;
    let needle = needle.to_lowercase();
    let idx = archive
        .paths
        .iter()
        .position(|p| p.to_lowercase().contains(&needle))?;
    decompress_block(&mut file, &archive.indx, idx as u16).ok()
}

/// The retail `scripts_vz` block, or `None` so the test skips loudly rather than failing on CI.
fn retail_block() -> Option<ScriptsBlock> {
    ScriptsBlock::parse(&retail_block_bytes("scripts_vz")?).ok()
}

/// ★ The gate for resident-block linking: the resident block is NOT scripts-only.
///
/// `scripts_vz` is 114 containers that are all Lua. `resident_P000_Q3` is a MIXED block — Lua
/// chunks alongside animation tables and other assets — so before any `patch_lua` may target it we
/// have to know that `ScriptsBlock` carries the non-script entries through untouched. The
/// byte-identical re-serialize is what proves that: anything the parser failed to model would show
/// up as a diff here rather than as a corrupt block in someone's game.
///
/// ⚠ The needle is ANCHORED (`\resident_P000_Q3.block`). Unanchored, `resident_P000_Q3` also
/// matches `sound_resident_P000_Q3` — a different block entirely.
#[test]
fn the_resident_block_parses_and_round_trips_byte_identically() {
    const SCRIPT_TYPE_HASH: u32 = 0x4249_8680;

    let Some(raw) = retail_block_bytes("\\resident_P000_Q3.block") else {
        eprintln!("SKIPPING: need a vz.wad");
        return;
    };
    let block = ScriptsBlock::parse(&raw).expect("the resident block must parse as a UCFX table");

    let total = block.entries.len();
    let scripts = block
        .entries
        .iter()
        .filter(|e| e.type_hash == SCRIPT_TYPE_HASH)
        .count();
    eprintln!("resident block: {total} entries, {scripts} of them Lua ({} other)", total - scripts);
    assert!(scripts > 0, "the resident block must carry Lua chunks");
    assert!(
        scripts < total,
        "expected a MIXED block — if this fires, the block is scripts-only and this test's \
         premise is wrong"
    );

    block.verify_csums().expect("every container's CSUM must verify as shipped");
    assert_eq!(
        block.serialize(),
        raw,
        "re-serializing an unedited resident block must be byte-identical"
    );
}

/// Both retail scripts blocks, in `SCRIPT_BLOCKS` order, or `None` to skip.
fn retail_blocks() -> Option<Vec<(String, ScriptsBlock)>> {
    let mut out = Vec::new();
    for (needle, path) in link::SCRIPT_BLOCKS {
        let raw = retail_block_bytes(needle)?;
        out.push(((*path).to_string(), ScriptsBlock::parse(&raw).ok()?));
    }
    Some(out)
}

/// ★ The capability the fix pack needs: a `patch_lua` whose target lives in the RESIDENT block.
///
/// Every framework module the bug register touches — `mrxplayer`, `mrxguipda`,
/// `mrxtaskjobcollecttype` — is resident, not `vz`. Before this, `link_into` searched only
/// `scripts_vz` and every one of them failed as `UnknownScript`.
#[test]
fn a_resident_script_links_into_the_resident_block() {
    let (Some(mut loaded), Some(corpus)) = (retail_blocks(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let counts: Vec<usize> = loaded.iter().map(|(_, b)| b.entries.len()).collect();

    let muts = vec![ScriptMutation {
        shipment: "fixpack".into(),
        target: "mrxplayer".into(),
        append: "-- [fixpack] resident reach\n".into(),
    }];

    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock {
            path: path.clone(),
            block,
        })
        .collect();
    let linked = link::link_into_blocks(&mut targets, &corpus, &muts).expect("link must succeed");
    drop(targets);

    assert_eq!(linked.len(), 1);
    let l = &linked[0];
    assert_eq!(l.target, "mrxplayer");
    assert_eq!(
        loaded[l.block].0, r"blocks\VZ\resident_P000_Q3.block",
        "a resident module must resolve to the RESIDENT block, not scripts_vz"
    );
    assert!(l.linked_source_bytes > l.base_source_bytes);

    // The untouched block must be reported as untouched, so the overlay does not republish it.
    assert_ne!(l.block, 0, "mrxplayer is not a scripts_vz script");

    for ((path, block), before) in loaded.iter().zip(counts) {
        assert_eq!(block.entries.len(), before, "{path} gained or lost entries");
    }
    let (_, resident) = &loaded[l.block];
    let reparsed = ScriptsBlock::parse(&resident.serialize()).expect("resident block must re-parse");
    reparsed.verify_csums().expect("CSUMs must verify");
    let idx = reparsed.find_script_by_name("mrxplayer").expect("still present");
    assert!(reparsed
        .extract_lua(idx)
        .expect("extract")
        .starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER));
}

/// A `vz` target and a `resident` target in one Shipment must each land in their own block.
#[test]
fn vz_and_resident_targets_split_across_two_blocks() {
    let (Some(mut loaded), Some(corpus)) = (retail_blocks(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let muts = vec![
        ScriptMutation {
            shipment: "fixpack".into(),
            target: "wifpmcinterior".into(),
            append: "-- vz\n".into(),
        },
        ScriptMutation {
            shipment: "fixpack".into(),
            target: "mrxtaskjobcollecttype".into(),
            append: "-- resident\n".into(),
        },
    ];
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock {
            path: path.clone(),
            block,
        })
        .collect();
    let linked = link::link_into_blocks(&mut targets, &corpus, &muts).expect("link");
    drop(targets);

    assert_eq!(linked.len(), 2);
    let by_target = |t: &str| linked.iter().find(|l| l.target == t).expect(t).block;
    assert_eq!(loaded[by_target("wifpmcinterior")].0, r"blocks\VZ\scripts_vz_P000_Q3.block");
    assert_eq!(
        loaded[by_target("mrxtaskjobcollecttype")].0,
        r"blocks\VZ\resident_P000_Q3.block"
    );
}

fn outfit_append(slug: &str, model: &str) -> String {
    format!(
        "table.insert(_tOutfits.mattias, {{ Name = \"{slug}\", Model = \"{model}\", \
         PlayerVisibleName = \"{slug}\" }})\n"
    )
}

/// ★ The one that matters. Two independent wardrobe mods, both patching `wifpmcinterior`. Under
/// whole-block semantics one silently annihilates the other; linked, both survive into one block.
#[test]
fn two_script_mods_both_survive_the_link() {
    let (Some(mut block), Some(corpus)) = (retail_block(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let before = block.entries.len();

    let muts = vec![
        ScriptMutation {
            shipment: "sean-devlin".into(),
            target: "wifpmcinterior".into(),
            append: outfit_append("SeanDevlin", "sean_devlin"),
        },
        ScriptMutation {
            shipment: "roze-skin".into(),
            target: "wifpmcinterior".into(),
            append: outfit_append("Roze", "roze"),
        },
    ];

    let linked = link::link_into(&mut block, &corpus, &muts).expect("link must succeed");
    assert_eq!(
        linked.len(),
        1,
        "one target, one compile — not one per Shipment"
    );
    let l = &linked[0];
    assert_eq!(l.target, "wifpmcinterior");
    assert_eq!(
        l.contributors,
        vec!["roze-skin", "sean-devlin"],
        "sorted by Shipment name"
    );
    assert!(
        l.linked_source_bytes > l.base_source_bytes,
        "the linked source must be longer than the base"
    );
    eprintln!(
        "linked {}: base {} B -> {} B source -> {} B bytecode, contributors {:?}",
        l.target, l.base_source_bytes, l.linked_source_bytes, l.bytecode_bytes, l.contributors
    );

    // The block must still be a block: same entry count, CSUMs intact, and it must re-parse.
    assert_eq!(
        block.entries.len(),
        before,
        "linking must not add or drop entries"
    );
    let rebuilt = block.serialize();
    let reparsed = ScriptsBlock::parse(&rebuilt).expect("the linked block must re-parse");
    reparsed
        .verify_csums()
        .expect("CSUMs must verify after linking");

    // And the payload really is our compiled chunk, in the game's dialect.
    let idx = reparsed
        .find_by_name("wifpmcinterior")
        .expect("still present");
    let luaq = reparsed.extract_lua(idx).expect("extract");
    assert!(
        luaq.starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER),
        "the linked script must carry the game's LuaQ header"
    );
    assert_eq!(luaq.len(), l.bytecode_bytes);
}

/// Two mods touching DIFFERENT scripts are independent — each compiled once, both spliced.
#[test]
fn mutations_on_different_scripts_are_independent() {
    let (Some(mut block), Some(corpus)) = (retail_block(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let muts = vec![
        ScriptMutation {
            shipment: "a".into(),
            target: "wifpmcinterior".into(),
            append: "-- a\n".into(),
        },
        ScriptMutation {
            shipment: "b".into(),
            target: "wifpmcgarage".into(),
            append: "-- b\n".into(),
        },
    ];
    let linked = link::link_into(&mut block, &corpus, &muts).expect("link");
    assert_eq!(linked.len(), 2);
    block.verify_csums().expect("CSUMs");
}

/// A target that is not in the block is named, not silently skipped — a mod whose script vanished
/// would otherwise install "successfully" and do nothing.
#[test]
fn an_unknown_target_is_reported() {
    let (Some(mut block), Some(corpus)) = (retail_block(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let muts = vec![ScriptMutation {
        shipment: "mod".into(),
        target: "no_such_script".into(),
        append: "-- x\n".into(),
    }];
    let err = link::link_into(&mut block, &corpus, &muts).expect_err("must not silently skip");
    let msg = err.to_string();
    assert!(
        msg.contains("no_such_script") && msg.contains("mod"),
        "{msg}"
    );
}

/// Broken Lua in a mod must fail the link with the compiler's own message — line number included —
/// rather than emitting a block whose script silently does not run.
#[test]
fn a_syntax_error_in_an_append_fails_the_link_with_a_line_number() {
    let (Some(mut block), Some(corpus)) = (retail_block(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let muts = vec![ScriptMutation {
        shipment: "broken-mod".into(),
        target: "wifpmcinterior".into(),
        append: "this is not ) valid lua\n".into(),
    }];
    let err = link::link_into(&mut block, &corpus, &muts).expect_err("must reject broken Lua");
    let msg = err.to_string();
    eprintln!("compile error surfaced: {msg}");
    assert!(
        msg.contains("wifpmcinterior"),
        "must name the script: {msg}"
    );
}

/// Linking with no mutations must leave the block byte-identical — the "did we break it just by
/// running" check.
#[test]
fn linking_nothing_changes_nothing() {
    let (Some(mut block), Some(corpus)) = (retail_block(), corpus_root()) else {
        eprintln!("SKIPPING: need a vz.wad and the Lua corpus");
        return;
    };
    let original = block.serialize();
    let linked = link::link_into(&mut block, &corpus, &[]).expect("link");
    assert!(linked.is_empty());
    assert!(
        block.serialize() == original,
        "a no-op link must not touch the block"
    );
}

#[test]
fn the_corpus_lookup_finds_a_vz_script_and_reports_what_it_tried() {
    let Some(corpus) = corpus_root() else { return };
    let found =
        link::base_source_path(&corpus, "wifpmcinterior").expect("wifpmcinterior is in vz/");
    assert!(
        found.ends_with("vz/wifpmcinterior.lua"),
        "{}",
        found.display()
    );

    let tried = link::base_source_path(&corpus, "definitely_not_a_script").unwrap_err();
    assert!(
        tried.len() >= 3,
        "should report every location it searched: {tried:?}"
    );
}
