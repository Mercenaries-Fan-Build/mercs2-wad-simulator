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

/// The retail `scripts_vz` block, or `None` so the test skips loudly rather than failing on CI.
fn retail_block() -> Option<ScriptsBlock> {
    let wad = mercs2_formats::game_paths::vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let mut file = std::fs::File::open(&wad).ok()?;
    let size = file.metadata().ok()?.len();
    let archive = load_ffcs_archive(&mut file, size).ok()?;
    let idx = archive
        .paths
        .iter()
        .position(|p| p.to_lowercase().contains("scripts_vz"))?;
    let dec = decompress_block(&mut file, &archive.indx, idx as u16).ok()?;
    ScriptsBlock::parse(&dec).ok()
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
