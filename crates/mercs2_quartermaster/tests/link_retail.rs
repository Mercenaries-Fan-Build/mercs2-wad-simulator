//! Does the linker actually let two script mods coexist? Checked against the retail scripts blocks.
//!
//! That is the failure the whole `patch_lua`-as-a-mutation design exists to prevent: two Shipments
//! each shipping a finished `scripts_vz` do not merge and do not error — the later one wins and the
//! earlier one's Lua vanishes silently. Every other property here is secondary to the one test that
//! installs two mods and checks both survive.
//!
//! Game-gated: built by the `retail` feature (`cargo xtask retail-test`), reads the retail vz.wad
//! named by the repo-root `.mercs2-local.toml`, and fails if it is absent. The hermetic linker tests
//! are in `link.rs`.

mod common {
    pub mod corpus;
}

use std::path::{Path, PathBuf};

use common::corpus::corpus_root;
use mercs2_formats::ffcs::load_ffcs_archive;
use mercs2_formats::scripts_block::ScriptsBlock;
use mercs2_formats::sges::decompress_block;
use mercs2_quartermaster::link::{self, ScriptMutation, UiRegistration};

/// A resolved load order, as the Shipment names `link_into_blocks` sorts its contributors by.
fn order(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

/// The vendored Lua corpus. Panics when the corpus is missing.
fn corpus() -> PathBuf {
    corpus_root().expect("the vendored Lua corpus crates/mercs2_script/corpus/mercs2-luacd/src is missing")
}

/// A retail block's raw decompressed bytes, located by a PTHS substring, from the vz.wad the
/// repo-root `.mercs2-local.toml` names. Panics when the archive or the block cannot be read.
fn retail_block_bytes(needle: &str) -> Vec<u8> {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut file =
        std::fs::File::open(&wad).unwrap_or_else(|e| panic!("open {}: {e}", wad.display()));
    let size = file
        .metadata()
        .unwrap_or_else(|e| panic!("stat {}: {e}", wad.display()))
        .len();
    let archive = load_ffcs_archive(&mut file, size)
        .unwrap_or_else(|e| panic!("read the FFCS archive {}: {e:?}", wad.display()));
    let lowered = needle.to_lowercase();
    let idx = archive
        .paths
        .iter()
        .position(|p| p.to_lowercase().contains(&lowered))
        .unwrap_or_else(|| panic!("no block matching {needle:?} in {}", wad.display()));
    decompress_block(&mut file, &archive.indx, idx as u16)
        .unwrap_or_else(|e| panic!("decompress the {needle:?} block of {}: {e:?}", wad.display()))
}

/// The retail `scripts_vz` block.
fn retail_block() -> ScriptsBlock {
    ScriptsBlock::parse(&retail_block_bytes("scripts_vz"))
        .unwrap_or_else(|e| panic!("the retail scripts_vz block must parse: {e}"))
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

    let raw = retail_block_bytes("\\resident_P000_Q3.block");
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

/// Both retail scripts blocks, in `SCRIPT_BLOCKS` order.
fn retail_blocks() -> Vec<(String, ScriptsBlock)> {
    link::SCRIPT_BLOCKS
        .iter()
        .map(|(needle, path)| {
            let raw = retail_block_bytes(needle);
            let block = ScriptsBlock::parse(&raw)
                .unwrap_or_else(|e| panic!("the retail {path} block must parse: {e}"));
            ((*path).to_string(), block)
        })
        .collect()
}

/// ★ The capability the fix pack needs: a `patch_lua` whose target lives in the RESIDENT block.
///
/// Every framework module the bug register touches — `mrxplayer`, `mrxguipda`,
/// `mrxtaskjobcollecttype` — is resident, not `vz`. Before this, `link_into` searched only
/// `scripts_vz` and every one of them failed as `UnknownScript`.
#[test]
fn a_resident_script_links_into_the_resident_block() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
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
    let linked = link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &order(&["fixpack"]))
        .expect("link must succeed")
        .scripts;
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

/// ★ add_ui's mod-loader mechanism, end-to-end against retail: a UI registration with NO plain
/// mutation mints a brand-new `qm_modloader` script into `scripts_vz`, wires the one-line trampoline
/// into `wifpmcinterior`, and leaves the block valid — the "expandable load space + stable trampoline"
/// the whole design is for. This is the half the heli experiment got wrong (it edited the resident
/// block directly); the fix is a new scripts_vz script reached by `import`.
#[test]
fn add_ui_mints_the_mod_loader_and_trampolines_from_the_resident() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let counts: Vec<usize> = loaded.iter().map(|(_, b)| b.entries.len()).collect();
    // The scripts_vz block is index 0 in SCRIPT_BLOCKS order, and `import` is scripts_vz-only, so the
    // loader must land there — pin which block we expect to grow.
    let vz = 0usize;
    assert!(loaded[vz].0.to_lowercase().contains("scripts_vz"));
    assert!(loaded[vz].1.find_script_by_name("qm_modloader").is_none(), "must start novel");

    // NO ScriptMutation — the whole edit is driven by the UI registration, proving add_ui does not
    // depend on any other script contribution to reach the game.
    let regs = vec![UiRegistration {
        shipment: "hud-mod".into(),
        movie: "my_hud_overlay".into(),
    }];
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock { path: path.clone(), block })
        .collect();
    let linked = link::link_into_blocks(&mut targets, &corpus, &[], &regs, &[], &[], &[], &[], &order(&["hud-mod"]))
        .expect("link must succeed")
        .scripts;
    drop(targets);

    // Both the trampoline host and the minted loader come back as linked, both in scripts_vz.
    let host = linked.iter().find(|l| l.target == "wifpmcinterior").expect("trampoline host linked");
    let loader = linked.iter().find(|l| l.target == "qm_modloader").expect("loader minted");
    assert_eq!(host.block, vz, "the trampoline lands in scripts_vz");
    assert_eq!(loader.block, vz, "the loader lands in scripts_vz (import is scripts_vz-only)");
    assert_eq!(loader.base_source_bytes, 0, "the loader has no base — it is newly minted");
    assert_eq!(loader.contributors, vec!["hud-mod".to_string()]);

    // Exactly one block grew, by exactly one entry (the loader); nothing else was disturbed.
    for (i, ((path, block), before)) in loaded.iter().zip(counts).enumerate() {
        let expected = if i == vz { before + 1 } else { before };
        assert_eq!(block.entries.len(), expected, "{path} entry count wrong");
    }

    // The edited scripts_vz block re-parses, every container (loader included) CSUM-verifies, and the
    // loader resolves by name and carries real Lua bytecode.
    let (_, vz_block) = &loaded[vz];
    let reparsed = ScriptsBlock::parse(&vz_block.serialize()).expect("scripts_vz must re-parse");
    reparsed.verify_csums().expect("CSUMs must verify, new container included");
    let idx = reparsed.find_script_by_name("qm_modloader").expect("loader resolves by name");
    assert!(reparsed
        .extract_lua(idx)
        .expect("extract loader")
        .starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER));
    // wifpmcinterior is still present and still a script (the trampoline appended, not replaced).
    assert!(reparsed.find_script_by_name("wifpmcinterior").is_some());

    // The loader's ASET row is emitted by `script_patch_blocks`' new-entry branch: its name hash has
    // no row in the base block, which is precisely the condition that mints a PRIMARY type-35 row.
    // (Proven directly in build.rs; asserted here at the source — a novel entry the base never had.)
    let loader_hash = mercs2_formats::hash::pandemic_hash_m2("qm_modloader");
    assert_eq!(reparsed.entries[idx].name_hash, loader_hash, "entry keyed by the import name");
}

/// A `vz` target and a `resident` target in one Shipment must each land in their own block.
#[test]
fn vz_and_resident_targets_split_across_two_blocks() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
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
    let linked = link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &order(&["fixpack"]))
        .expect("link")
        .scripts;
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
    let mut block = retail_block();
    let corpus = corpus();
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

    // Neither requires the other, so the resolved load order is the request order (the lowest
    // request index goes first on a tie): sean-devlin, then roze-skin — deliberately NOT name order,
    // so an assertion that still expected sorting by name could not pass here by accident.
    let resolved = order(&["sean-devlin", "roze-skin"]);
    let linked = link::link_into(&mut block, &corpus, &muts, &resolved)
        .expect("link must succeed")
        .scripts;
    assert_eq!(
        linked.len(),
        1,
        "one target, one compile — not one per Shipment"
    );
    let l = &linked[0];
    assert_eq!(l.target, "wifpmcinterior");
    assert_eq!(
        l.contributors, resolved,
        "appends concatenate in the resolved load order, not by Shipment name"
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
    let mut block = retail_block();
    let corpus = corpus();
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
    let linked = link::link_into(&mut block, &corpus, &muts, &order(&["a", "b"])).expect("link").scripts;
    assert_eq!(linked.len(), 2);
    block.verify_csums().expect("CSUMs");
}

/// A target that is not in the block is an error that names it, so a mod whose script vanished
/// cannot install "successfully" and do nothing.
#[test]
fn an_unknown_target_is_reported() {
    let mut block = retail_block();
    let corpus = corpus();
    let muts = vec![ScriptMutation {
        shipment: "mod".into(),
        target: "no_such_script".into(),
        append: "-- x\n".into(),
    }];
    let err = link::link_into(&mut block, &corpus, &muts, &order(&["mod"]))
        .expect_err("must not silently skip");
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
    let mut block = retail_block();
    let corpus = corpus();
    let muts = vec![ScriptMutation {
        shipment: "broken-mod".into(),
        target: "wifpmcinterior".into(),
        append: "this is not ) valid lua\n".into(),
    }];
    let err = link::link_into(&mut block, &corpus, &muts, &order(&["broken-mod"]))
        .expect_err("must reject broken Lua");
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
    let mut block = retail_block();
    let corpus = corpus();
    let original = block.serialize();
    let linked = link::link_into(&mut block, &corpus, &[], &[]).expect("link").scripts;
    assert!(linked.is_empty());
    assert!(
        block.serialize() == original,
        "a no-op link must not touch the block"
    );
}

/// The linked block is a function of the resolved load ORDER, not of the order the mutations are
/// handed over in.
///
/// The property is worth pinning because breaking it is silent and it corrupts player state:
/// `_tOutfits[hero]` is an ordered list and the save file persists a POSITION, not a name, so a set
/// that appended in a different order on reinstall would resolve a saved game to the wrong costume
/// — no error, no crash, just the wrong clothes on a character the player already owns. The order
/// comes from the load plan (requires edges, then the request order), never from how the caller
/// happened to collect the mutations.
#[test]
fn link_output_follows_the_order_not_the_input_order() {
    let corpus = corpus();
    let base = retail_block_bytes("scripts_vz");

    // Two mutations on one target, handed over in opposite orders under one resolved order that
    // is NOT the alphabetical one.
    let mk = |shipment: &str, marker: &str| ScriptMutation {
        shipment: shipment.to_string(),
        target: "wifpmcinterior".to_string(),
        append: format!("-- {marker}\n"),
    };
    let a = mk("alpha-outfit", "ALPHA");
    let z = mk("zulu-outfit", "ZULU");
    let resolved = order(&["zulu-outfit", "alpha-outfit"]);

    let mut fwd = ScriptsBlock::parse(&base).expect("parse the retail scripts block");
    let mut rev = ScriptsBlock::parse(&base).expect("parse the retail scripts block");
    let path = "blocks\\VZ\\scripts_vz_P000_Q3.block".to_string();
    let one = link::link_into_blocks(
        &mut [link::TargetBlock { path: path.clone(), block: &mut fwd }],
        &corpus,
        &[a.clone(), z.clone()],
        &[],
        &[],
        &[],
        &[],
        &[],
        &resolved,
    )
    .expect("link forward")
    .scripts;
    let two = link::link_into_blocks(
        &mut [link::TargetBlock { path, block: &mut rev }],
        &corpus,
        &[z, a],
        &[],
        &[],
        &[],
        &[],
        &[],
        &resolved,
    )
    .expect("link reversed")
    .scripts;

    assert_eq!(
        one.len(),
        two.len(),
        "the same set must produce the same number of linked targets"
    );
    for (f, r) in one.iter().zip(two.iter()) {
        assert_eq!(
            f.contributors, r.contributors,
            "contributor order must follow the resolved order, not the input order"
        );
        assert_eq!(f.contributors, resolved, "the appends concatenate in the resolved order");
        assert_eq!(
            f.bytecode_bytes, r.bytecode_bytes,
            "one order must compile to identical bytecode, or saved costume positions move when a \
             mod is reinstalled"
        );
    }
}

/// Every ordered decision follows the resolved order. With the order `zzz` then `aaa`
/// (not the names' sort order), `aaa`'s `replace_lua` is applied last and so is what remains, and
/// `zzz`'s `add_script` module is minted before `aaa`'s.
#[test]
fn replace_lua_and_add_script_follow_the_order() {
    let corpus = corpus();
    let base = retail_block_bytes("scripts_vz");
    let resolved = order(&["zzz", "aaa"]);
    let replacements = [
        link::ScriptReplacement {
            shipment: "aaa".into(),
            target: "wifpmcgarage".into(),
            source: "QM_ORDER_MARKER = \"aaa-replaced\"\n".into(),
        },
        link::ScriptReplacement {
            shipment: "zzz".into(),
            target: "wifpmcgarage".into(),
            source: "QM_ORDER_MARKER = \"zzz-replaced\"\n".into(),
        },
    ];
    let additions = [
        link::ScriptAddition {
            shipment: "aaa".into(),
            name: "qm_order_aaa".into(),
            source: "QM_ORDER_AAA = true\n".into(),
        },
        link::ScriptAddition {
            shipment: "zzz".into(),
            name: "qm_order_zzz".into(),
            source: "QM_ORDER_ZZZ = true\n".into(),
        },
    ];
    let mut block = ScriptsBlock::parse(&base).expect("parse the retail scripts block");
    link::link_into_blocks(
        &mut [link::TargetBlock { path: "blocks\\VZ\\scripts_vz_P000_Q3.block".into(), block: &mut block }],
        &corpus,
        &[],
        &[],
        &[],
        &[],
        &additions,
        &replacements,
        &resolved,
    )
    .expect("link");

    let idx = block.find_script_by_name("wifpmcgarage").expect("wifpmcgarage");
    let luaq = block.extract_lua(idx).unwrap();
    let text = String::from_utf8_lossy(&luaq);
    assert!(text.contains("aaa-replaced"), "the later Shipment in the order wins");
    assert!(!text.contains("zzz-replaced"), "the earlier replacement is overwritten");

    let z = block.find_script_by_name("qm_order_zzz").expect("zzz minted");
    let a = block.find_script_by_name("qm_order_aaa").expect("aaa minted");
    assert!(z < a, "minted in load order: zzz ({z}) before aaa ({a})");
}

/// A literal `import("x")` nothing provides is a WARNING — the link still succeeds — and
/// a literal naming a shipped script, an `add_script` in the set or `qm_modloader` is resolved.
#[test]
fn literal_import_unknown_warns_but_links() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let muts = vec![ScriptMutation {
        shipment: "consumer".into(),
        target: "wifpmcinterior".into(),
        append: "local e = import(\"ess\")\nlocal m = import(\"mrxplayer\")\n\
                 local q = import(\"qm_modloader\")\nlocal n = import(\"no_such_module\")\n"
            .into(),
    }];
    let additions = [link::ScriptAddition {
        shipment: "ess".into(),
        name: "ess".into(),
        source: "return {}\n".into(),
    }];
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock { path: path.clone(), block })
        .collect();
    let out = link::link_into_blocks(
        &mut targets,
        &corpus,
        &muts,
        &[],
        &[],
        &[],
        &additions,
        &[],
        &order(&["ess", "consumer"]),
    )
    .expect("an unresolved import never fails the link");
    assert_eq!(
        out.unresolved_imports,
        vec![link::UnresolvedImport {
            shipment: "consumer".into(),
            module: "no_such_module".into(),
            source: "patch_lua append to wifpmcinterior".into(),
        }]
    );
    assert!(out.scripts.iter().any(|l| l.target == "ess"), "the addition was minted");
}

/// `dynamic_import(...)` and `import(<expr>)` are unchecked by design: never flagged.
#[test]
fn dynamic_import_not_flagged() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let muts = vec![ScriptMutation {
        shipment: "consumer".into(),
        target: "wifpmcinterior".into(),
        append: "local a = dynamic_import(\"not_here\")\nlocal b = import(sName)\n\
                 local c = import(\"not\" .. \"_here\")\n"
            .into(),
    }];
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock { path: path.clone(), block })
        .collect();
    let out = link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &order(&["consumer"]))
        .expect("link");
    assert_eq!(out.unresolved_imports, vec![]);
}
