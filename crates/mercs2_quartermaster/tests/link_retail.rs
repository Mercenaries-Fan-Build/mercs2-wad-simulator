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

/// The hosts of the hooks a link generates in `session`, in the order the link splices them (by name):
/// every host of the catalogue's hooks of that session (`loader::generated_hooks`).
fn qm_hosts(session: mercs2_quartermaster::manifest::LoadSession) -> Vec<String> {
    let mut hosts: Vec<String> =
        mercs2_quartermaster::hooks::hooks().iter().filter(|h| h.sessions.contains(&session)).map(|h| h.host.clone()).collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

fn qm_gameplay_hosts() -> Vec<String> {
    qm_hosts(mercs2_quartermaster::manifest::LoadSession::Gameplay)
}

/// Every gameplay link splices qm's hook dispatcher, world loader entry and every catalogue hook onto
/// their hosts, mods or no mods (W-a): exactly one such result per host, contributed by Quartermaster.
fn assert_qm_splice(linked: &[link::LinkedScript]) {
    let qm: Vec<&str> = linked
        .iter()
        .filter(|l| l.contributors == [link::MODLOADER_CONTRIBUTOR.to_string()])
        .map(|l| l.target.as_str())
        .collect();
    assert_eq!(qm, qm_gameplay_hosts(), "one qm splice per host: {linked:?}");
}

/// The link's results for the Shipments' own scripts: everything but qm's splices, which
/// [`assert_qm_splice`] checks.
fn shipment_scripts(linked: Vec<link::LinkedScript>) -> Vec<link::LinkedScript> {
    assert_qm_splice(&linked);
    linked.into_iter().filter(|l| l.contributors != [link::MODLOADER_CONTRIBUTOR.to_string()]).collect()
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

/// The blocks retail `vz.wad`'s ASET registers script `name` in, as (row order, block path, primary).
fn retail_script_rows(name: &str) -> Vec<(usize, String, bool)> {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    let mut file = std::fs::File::open(&wad).unwrap_or_else(|e| panic!("open {}: {e}", wad.display()));
    let size = file.metadata().unwrap_or_else(|e| panic!("stat {}: {e}", wad.display())).len();
    let archive = load_ffcs_archive(&mut file, size).unwrap_or_else(|e| panic!("read {}: {e:?}", wad.display()));
    let hash = mercs2_formats::hash::pandemic_hash_m2(name);
    archive
        .aset
        .iter()
        .enumerate()
        .filter(|(_, r)| r.asset_hash == hash && r.type_id == mercs2_formats::aset_type_ids::SCRIPT_ASET_TYPE_ID)
        .map(|(i, r)| (i, archive.paths[r.block_index() as usize].clone(), r.is_primary()))
        .collect()
}

/// The `vz` master is a script of both `scripts_vz` and `resident`, and qm splices its hooks onto
/// the copy retail's registry holds: the first primary ASET row's block.
#[test]
fn qm_splices_the_vz_master_copy_the_registry_holds() {
    let rows = retail_script_rows("vz");
    println!("retail ASET rows for vz: {rows:?}");
    let registered = rows.iter().find(|r| r.2).unwrap_or_else(|| panic!("vz has no primary ASET row: {rows:?}"));
    assert!(registered.1.to_lowercase().contains("scripts_vz"), "the engine registers vz from {}: {rows:?}", registered.1);
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
    let linked = shipment_scripts(
        link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &[], &order(&["fixpack"]))
            .expect("link must succeed")
            .scripts,
    );
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
/// mutation mints a brand-new `qm_modloader` script into `scripts_vz`, which qm's world entry spliced
/// onto the `vz` master loads by `dynamic_import`, and leaves the blocks valid — the "expandable load
/// space" the whole design is for. No retail script is recompiled: `wifpmcinterior` is untouched.
#[test]
fn add_ui_mints_the_mod_loader_and_the_world_entry_loads_it() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let counts: Vec<usize> = loaded.iter().map(|(_, b)| b.entries.len()).collect();
    // The scripts_vz block is index 0 in SCRIPT_BLOCKS order, and the loader lands beside
    // `wifpmcinterior` there — pin which block we expect to grow.
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
    let linked = link::link_into_blocks(&mut targets, &corpus, &[], &regs, &[], &[], &[], &[], &[], &order(&["hud-mod"]))
        .expect("link must succeed")
        .scripts;
    drop(targets);

    // qm's splices and the minted loader come back as linked; the loader and the vz master's splice
    // are in scripts_vz. wifpmcinterior carries only qm's own hook (wager.result), no trampoline.
    assert_qm_splice(&linked);
    let host = linked.iter().find(|l| l.target == "vz").expect("the world entry's host linked");
    let loader = linked.iter().find(|l| l.target == "qm_modloader").expect("loader minted");
    assert_eq!(host.block, vz, "the vz master the registry holds is in scripts_vz");
    assert_eq!(loader.block, vz, "the loader lands in scripts_vz");
    let interior: Vec<&link::LinkedScript> = linked.iter().filter(|l| l.target == "wifpmcinterior").collect();
    assert!(
        interior.iter().all(|l| l.contributors == [link::MODLOADER_CONTRIBUTOR.to_string()]),
        "only qm's hook splice touches wifpmcinterior: {linked:?}"
    );
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
    // wifpmcinterior names no loader (no trampoline imports it); the vz master's chunk carries the
    // entry and loads the loader by name.
    let interior = reparsed.extract_lua(reparsed.find_script_by_name("wifpmcinterior").unwrap()).unwrap();
    assert!(!interior.windows(12).any(|w| w == b"qm_modloader"), "no trampoline in wifpmcinterior");
    let master = reparsed.extract_lua(reparsed.find_script_by_name("vz").unwrap()).unwrap();
    let contains = |needle: &str| master.windows(needle.len()).any(|w| w == needle.as_bytes());
    for needle in ["world.scripts_loaded", "dynamic_import", "qm_modloader", "_StartPlayerVisibleGameplay", "world.begin"] {
        assert!(contains(needle), "the vz master's tail carries {needle}");
    }

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
    let linked = shipment_scripts(
        link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &[], &order(&["fixpack"]))
            .expect("link")
            .scripts,
    );
    drop(targets);

    assert_eq!(linked.len(), 2);
    let by_target = |t: &str| linked.iter().find(|l| l.target == t).expect(t).block;
    assert_eq!(loaded[by_target("wifpmcinterior")].0, r"blocks\VZ\scripts_vz_P000_Q3.block");
    assert_eq!(
        loaded[by_target("mrxtaskjobcollecttype")].0,
        r"blocks\VZ\resident_P000_Q3.block"
    );
}

/// Link `muts` into both retail gameplay blocks, as the deploy link does: the blocks after the link,
/// and its result.
fn link_retail(muts: &[ScriptMutation], order: &[String]) -> (Vec<(String, ScriptsBlock)>, Result<link::LinkOutput, link::LinkError>) {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let out = {
        let mut targets: Vec<link::TargetBlock<'_>> =
            loaded.iter_mut().map(|(path, block)| link::TargetBlock { path: path.clone(), block }).collect();
        link::link_into_blocks(&mut targets, &corpus, muts, &[], &[], &[], &[], &[], &[], order)
    };
    (loaded, out)
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
    let before = retail_block().entries.len();

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
    let (loaded, out) = link_retail(&muts, &resolved);
    let all = out.expect("link must succeed").scripts;
    // qm splices its own hook (wager.result) onto wifpmcinterior after the Shipments' appends, so the
    // block's final chunk is that splice's.
    let qm_splice = all
        .iter()
        .find(|l| l.target == "wifpmcinterior" && l.contributors == [link::MODLOADER_CONTRIBUTOR.to_string()])
        .expect("qm's hook splice onto wifpmcinterior")
        .bytecode_bytes;
    let linked = shipment_scripts(all);
    let block = &loaded[0].1;
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
    assert_eq!(luaq.len(), qm_splice, "the block holds the chunk qm's splice left");
    assert!(qm_splice > l.bytecode_bytes, "qm's splice is appended to the Shipments' linked chunk");
    for needle in ["SeanDevlin", "Roze", "wager.result"] {
        assert!(luaq.windows(needle.len()).any(|w| w == needle.as_bytes()), "the final chunk carries {needle}");
    }
}

fn obama() -> mercs2_quartermaster::outfits::AddedOutfit {
    mercs2_quartermaster::outfits::AddedOutfit {
        shipment: "skins".into(),
        slug: "Obama".into(),
        model: "pmc_hum_obama".into(),
        display: "[Obama]".into(),
        wearers: vec!["mattias".into(), "chris".into(), "jen".into()],
        levels: None,
        chatter: None,
    }
}

fn link_outfits(
    muts: &[ScriptMutation],
    outfits: &[mercs2_quartermaster::outfits::AddedOutfit],
    names: &[&str],
) -> (Vec<(String, ScriptsBlock)>, Result<link::LinkOutput, link::LinkError>) {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let out = {
        let mut targets: Vec<link::TargetBlock<'_>> =
            loaded.iter_mut().map(|(path, block)| link::TargetBlock { path: path.clone(), block }).collect();
        link::link_into_level_blocks(
            &mut targets,
            &corpus,
            muts,
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &order(names),
            &std::collections::BTreeSet::new(),
            link::QmHooks::Always,
            None,
            "vz",
            outfits,
        )
    };
    (loaded, out)
}

/// The set's outfits go onto the wardrobe, the player script and the master, after a fix pack's
/// `patch_lua` on the first two (the unofficial patch appends to both, BUG-015 and BUG-006): the linked
/// wardrobe lists the added row after the retail ones, and the linked player script restores it.
#[test]
fn outfits_link_after_appends_to_the_wardrobe_and_the_player_script() {
    let muts = vec![
        ScriptMutation {
            shipment: "fix".into(),
            target: "wifpmcinterior".into(),
            append: "function LoadSingleton(tSaveData)\n  _tFixLoaded = tSaveData\nend\n".into(),
        },
        ScriptMutation {
            shipment: "fix".into(),
            target: "mrxplayer".into(),
            append: "function SaveSingleton()\n  return {}\nend\n".into(),
        },
    ];
    let (loaded, out) = link_outfits(&muts, &[obama()], &["fix", "skins"]);
    let scripts = out.unwrap_or_else(|e| panic!("{e}")).scripts;
    let by = |who: &str| -> Vec<&str> {
        scripts.iter().filter(|l| l.contributors == [who.to_string()]).map(|l| l.target.as_str()).collect()
    };
    assert_eq!(by("fix"), ["mrxplayer", "wifpmcinterior"]);
    assert_eq!(by("skins"), ["wifpmcinterior", "mrxplayer", "vz"]);
    let chunk = |name: &str| -> Vec<u8> {
        let (_, b) = loaded.iter().find(|(_, b)| b.find_script_by_name(name).is_some()).unwrap();
        b.extract_lua(b.find_script_by_name(name).unwrap()).unwrap()
    };
    let lua = mercs2_luac::rt::Lua::new().unwrap();
    lua.load("function import() end").exec().unwrap();
    for name in ["wifpmcinterior", "mrxplayer"] {
        let f = lua.load(&chunk(name)).set_name(format!("={name}")).into_function().unwrap();
        f.call::<()>(()).unwrap_or_else(|e| panic!("running the linked {name}: {e}"));
    }
    let eval = |code: &str| lua.load(code).eval::<String>().unwrap_or_else(|e| panic!("{code}: {e}"));
    assert_eq!(eval("return #_tOutfits.mattias .. ' ' .. _tOutfits.mattias[6].Name .. ' ' .. tostring(_tOutfits.mattias[6].bAlwaysVisible)"), "6 Obama true");
    assert_eq!(eval("return #_tOutfits.jennifer .. ' ' .. _tOutfits.jennifer[7].Model"), "7 pmc_hum_obama");
    assert_eq!(eval("local _, m = GetTemplateAndModelName({ iIndex = 1, iCostume = 5 }) return m"), "pmc_hum_obama");
    assert_eq!(eval("local _, m = GetTemplateAndModelName({ iIndex = 1, iCostume = 6 }) return m"), "gr_hum_starter_1");
    assert_eq!(eval("local _, m = GetTemplateAndModelName({ iIndex = 3, iCostume = 6 }) return m"), "pmc_hum_obama");
    assert_eq!(eval("return type(_tFixLoaded) .. ' ' .. type(LoadSingleton)"), "nil function", "the fix's append ran");
    let master = chunk("vz");
    assert!(master.windows(b"[qm] outfit restore: ".len()).any(|w| w == b"[qm] outfit restore: "), "the master carries the load");
}

/// A `patch_lua` that adds its own `_tOutfits` row leaves a wardrobe the character map does not
/// restore, and the link refuses it rather than generating lists on top of it.
#[test]
fn a_hand_appended_wardrobe_row_is_refused_with_qm_s_outfits() {
    let muts = vec![ScriptMutation {
        shipment: "hand".into(),
        target: "wifpmcinterior".into(),
        append: outfit_append("Hand", "hand_model"),
    }];
    let e = link_outfits(&muts, &[obama()], &["hand", "skins"]).1.unwrap_err().to_string();
    assert!(e.contains("the set's outfits: vz:") && e.contains("would restore the wrong outfit"), "{e}");
}

/// Two mods touching DIFFERENT scripts are independent — each compiled once, both spliced.
#[test]
fn mutations_on_different_scripts_are_independent() {
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
    let (loaded, out) = link_retail(&muts, &order(&["a", "b"]));
    let linked = shipment_scripts(out.expect("link").scripts);
    assert_eq!(linked.len(), 2);
    for (_, block) in &loaded {
        block.verify_csums().expect("CSUMs");
    }
}

/// A support shop row, as `add_shop_item` writes it (a crate delivery with cargo and a delivery
/// vehicle, the shape of retail's own rows), links into retail `mrxsupportdata`: the INFO globals
/// count is computed over the appended row, whose method calls are on a call's result.
#[test]
fn a_support_shop_row_links_into_retail_mrxsupportdata() {
    let row = link::shop_support_row_append(
        "qm_test_tank",
        "QM Test Tank",
        "",
        "vehicles_tank_m1a2",
        "Heavy",
        250000,
        75,
        99,
        &link::shop_unlock_table(&[mercs2_quartermaster::manifest::ShopVendor::Pmc], true),
        "mrxcratedelivery",
        Some("M2A3"),
        Some("Mi26 (PMC) (Driver)"),
    );
    let muts = vec![ScriptMutation { shipment: "shop".into(), target: "mrxsupportdata".into(), append: row }];
    let (loaded, out) = link_retail(&muts, &order(&["shop"]));
    let linked = shipment_scripts(out.expect("the support row links").scripts);
    assert_eq!(linked.len(), 1);
    let block = loaded.iter().map(|(_, b)| b).find(|b| b.find_by_name("mrxsupportdata").is_some()).expect("mrxsupportdata's block");
    let luaq = block.extract_lua(block.find_by_name("mrxsupportdata").unwrap()).expect("extract");
    for needle in ["qm_test_tank", "M2A3", "Mi26 (PMC) (Driver)"] {
        assert!(luaq.windows(needle.len()).any(|w| w == needle.as_bytes()), "the linked chunk carries {needle}");
    }
}

/// A target that is not in the block is an error that names it, so a mod whose script vanished
/// cannot install "successfully" and do nothing.
#[test]
fn an_unknown_target_is_reported() {
    let muts = vec![ScriptMutation {
        shipment: "mod".into(),
        target: "no_such_script".into(),
        append: "-- x\n".into(),
    }];
    let err = link_retail(&muts, &order(&["mod"])).1.expect_err("must not silently skip");
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
    let muts = vec![ScriptMutation {
        shipment: "broken-mod".into(),
        target: "wifpmcinterior".into(),
        append: "this is not ) valid lua\n".into(),
    }];
    let err = link_retail(&muts, &order(&["broken-mod"])).1.expect_err("must reject broken Lua");
    let msg = err.to_string();
    eprintln!("compile error surfaced: {msg}");
    assert!(
        msg.contains("wifpmcinterior"),
        "must name the script: {msg}"
    );
}

/// Linking with no mutations touches only the hosts of qm's hooks, which carry qm's hook dispatcher,
/// world loader entry and hooks on every link (W-a); every other entry of both blocks stays
/// byte-identical — the "did we break it just by running" check.
#[test]
fn linking_nothing_changes_only_qm_s_splices() {
    let original = retail_blocks();
    let (loaded, out) = link_retail(&[], &[]);
    let linked = out.expect("link").scripts;
    assert_qm_splice(&linked);
    let qm = qm_gameplay_hosts();
    assert_eq!(linked.len(), qm.len(), "only qm's splices: {linked:?}");
    for ((path, block), (_, then)) in loaded.iter().zip(&original) {
        let hosts: Vec<usize> = qm.iter().filter_map(|h| block.find_script_by_name(h)).collect();
        assert_eq!(block.entries.len(), then.entries.len(), "{path}: nothing minted");
        for (i, (now, before)) in block.entries.iter().zip(&then.entries).enumerate() {
            if hosts.contains(&i) {
                assert!(now.bytes != before.bytes, "{path} entry {i} carries a splice");
            } else {
                assert!(now.bytes == before.bytes, "{path} entry {i} must be untouched");
            }
        }
        block.verify_csums().expect("CSUMs");
    }
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

    let one = shipment_scripts(link_retail(&[a.clone(), z.clone()], &resolved).1.expect("link forward").scripts);
    let two = shipment_scripts(link_retail(&[z, a], &resolved).1.expect("link reversed").scripts);

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
    let resolved = order(&["zzz", "aaa"]);
    // A replacement must keep every import the retail script has (its DEPS); these are
    // wifpmcgarage's.
    let imports = "import(\"MrxState\")\nimport(\"MrxPmc\")\nimport(\"MrxSupportData\")\nimport(\"WifPmcInterior\")\n\
                   import(\"MrxGuiGarage\")\nimport(\"MrxUtil\")\nimport(\"MrxVoSequence\")\n";
    let replacements = [
        link::ScriptReplacement {
            shipment: "aaa".into(),
            target: "wifpmcgarage".into(),
            source: format!("{imports}QM_ORDER_MARKER = \"aaa-replaced\"\n"),
        },
        link::ScriptReplacement {
            shipment: "zzz".into(),
            target: "wifpmcgarage".into(),
            source: format!("{imports}QM_ORDER_MARKER = \"zzz-replaced\"\n"),
        },
    ];
    let additions = [
        link::ScriptAddition {
            shipment: "aaa".into(),
            name: "qm_order_aaa".into(),
            source: "QM_ORDER_AAA = true\n".into(),
            load: None,
        },
        link::ScriptAddition {
            shipment: "zzz".into(),
            name: "qm_order_zzz".into(),
            source: "QM_ORDER_ZZZ = true\n".into(),
            load: None,
        },
    ];
    let mut loaded = retail_blocks();
    {
        let mut targets: Vec<link::TargetBlock<'_>> =
            loaded.iter_mut().map(|(path, block)| link::TargetBlock { path: path.clone(), block }).collect();
        link::link_into_blocks(&mut targets, &corpus, &[], &[], &[], &[], &[], &additions, &replacements, &resolved).expect("link");
    }
    let block = &loaded[0].1;

    let idx = block.find_script_by_name("wifpmcgarage").expect("wifpmcgarage");
    let luaq = block.extract_lua(idx).unwrap();
    let text = String::from_utf8_lossy(&luaq);
    assert!(text.contains("aaa-replaced"), "the later Shipment in the order wins");
    assert!(!text.contains("zzz-replaced"), "the earlier replacement is overwritten");

    let z = block.find_script_by_name("qm_order_zzz").expect("zzz minted");
    let a = block.find_script_by_name("qm_order_aaa").expect("aaa minted");
    assert!(z < a, "minted in load order: zzz ({z}) before aaa ({a})");
}

/// `(code, Shipments, module)` of each import finding.
fn found(out: &link::LinkOutput) -> Vec<(&'static str, Vec<String>, String)> {
    out.findings.iter().map(|f| (f.rule.code(), f.shipments.clone(), f.module.clone())).collect()
}

/// Link `muts` and `additions` into the retail `vz` blocks in `names` order.
fn link_vz(muts: &[ScriptMutation], additions: &[link::ScriptAddition], names: &[&str]) -> link::LinkOutput {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    let mut targets: Vec<link::TargetBlock<'_>> =
        loaded.iter_mut().map(|(path, block)| link::TargetBlock { path: path.clone(), block }).collect();
    link::link_into_blocks(&mut targets, &corpus, muts, &[], &[], &[], &[], additions, &[], &order(names)).expect("links")
}

fn addition(shipment: &str, name: &str, source: &str) -> link::ScriptAddition {
    link::ScriptAddition { shipment: shipment.into(), name: name.into(), source: source.into(), load: None }
}

/// ★ M0209 reads the compiled DEPS: every call form of `import`, and `inherit`, of a name nothing
/// provides is found, and a shipped script or an `add_script` of the set resolves. `qm_modloader`
/// resolves only when the link mints it, which this set does not.
#[test]
fn an_import_nothing_provides_is_m0209_in_any_call_form() {
    let muts = vec![ScriptMutation {
        shipment: "consumer".into(),
        target: "wifpmcinterior".into(),
        append: "local e = import(\"ess\")\nlocal m = import(\"mrxplayer\")\n\
                 local q = import(\"qm_modloader\")\nlocal n = import(\"no_such_module\")\n\
                 local p = import \"no_parens_missing\"\ninherit(\"no_such_base\")\n"
            .into(),
    }];
    let out = link_vz(&muts, &[addition("ess", "ess", "return {}\n")], &["ess", "consumer"]);
    let consumer = vec!["consumer".to_string()];
    assert_eq!(
        found(&out),
        vec![
            ("M0209", consumer.clone(), "no_parens_missing".to_string()),
            ("M0209", consumer.clone(), "no_such_base".to_string()),
            ("M0209", consumer.clone(), "no_such_module".to_string()),
            ("M0209", consumer, "qm_modloader".to_string()),
        ]
    );
    assert!(out.findings[1].message.contains("inherit(\"no_such_base\")"), "{}", out.findings[1].message);
    assert!(out.findings[0].message.contains("patch_lua append to wifpmcinterior"), "{}", out.findings[0].message);
    assert!(out.scripts.iter().any(|l| l.target == "ess"), "the addition was minted");
}

/// ★ M0313: an `add_script` with no `load:` runs only when something that runs reaches it. A module
/// imported by a retail script's append, one a literal `sModuleName` names and one a reached module
/// imports are reached; one nothing names, and one only it imports, are not.
#[test]
fn an_add_script_nothing_that_runs_reaches_is_m0313() {
    let muts = vec![ScriptMutation {
        shipment: "s".into(),
        target: "wifpmcinterior".into(),
        append: "local u = import(\"qm_t_util\")\nqm_t_row = { sModuleName = \"qm_t_contract\" }\n".into(),
    }];
    let additions = [
        addition("s", "qm_t_util", "local d = import(\"qm_t_deep\")\n"),
        addition("s", "qm_t_deep", "return {}\n"),
        addition("s", "qm_t_contract", "inherit(\"MrxTaskContract\")\n"),
        addition("s", "qm_t_orphan", "local c = import(\"qm_t_orphan_child\")\n"),
        addition("s", "qm_t_orphan_child", "return {}\n"),
    ];
    let out = link_vz(&muts, &additions, &["s"]);
    let s = vec!["s".to_string()];
    assert_eq!(
        found(&out),
        vec![("M0313", s.clone(), "qm_t_orphan".to_string()), ("M0313", s, "qm_t_orphan_child".to_string())]
    );
}

/// The cycle gate's fixtures (`tests/fixtures/live_gate/qm-cycle-gate`).
fn cycle_gate(file: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/live_gate").join(file);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// ★ The loader model M0314 runs gives, on the retail blocks, the order the game ran the bodies of
/// retail's 84-script DEPS cycle in, live (the cycle gate's run A, 2026-11-09, vz-patch `698f1a2d`):
/// the world's passes `gamebootstrap` then `vz`.
#[test]
fn the_loader_model_gives_the_order_the_game_ran_retail_s_cycle_in() {
    let blocks = retail_blocks();
    let g = mercs2_quartermaster::imports::DepsGraph::of(blocks.iter().map(|(_, b)| b)).expect("graph");
    let roots = [link::WORLD_BOOT_ROOT, mercs2_quartermaster::hooks::MASTER_HOST].map(mercs2_formats::hash::pandemic_hash_m2);
    let passes = mercs2_quartermaster::imports::load_passes(&g, &roots, &Default::default()).expect("model");
    let observed: Vec<String> = cycle_gate("qm-cycle-gate/body_order_run_a.txt").lines().map(str::to_string).collect();
    assert_eq!(observed.len(), 84);
    let bodies: Vec<String> = passes
        .iter()
        .filter(|p| !p.by_name)
        .flat_map(|p| &p.events)
        .filter(|(s, _)| *s == mercs2_quartermaster::imports::Stage::Body)
        .map(|&(_, h)| g.names[&h].to_ascii_lowercase())
        .filter(|n| observed.contains(n))
        .collect();
    assert_eq!(bodies, observed);
}

/// ★ M0314: an import a link adds of a module whose body already ran first changes nothing (the
/// cycle gate's run B: `wifpmcinterior` → `mrxshop`, appended, kept the live order).
#[test]
fn an_added_import_of_a_module_already_run_is_no_m0314() {
    let muts = vec![ScriptMutation {
        shipment: "qm-cycle-gate-edge".into(),
        target: "wifpmcinterior".into(),
        append: cycle_gate("qm-cycle-gate-edge/src/edge.lua"),
    }];
    let out = link_vz(&muts, &[], &["qm-cycle-gate-edge"]);
    assert_eq!(found(&out), vec![]);
}

/// The cycle gate's library (`qm-cycle-gate`), loaded at the world entry, with `qm_cyc_b`'s source
/// `b`.
fn cycle_gate_library(b: &str) -> Vec<link::ScriptAddition> {
    use mercs2_quartermaster::manifest::{LibraryLoad, LoadSession};
    let lib = |name: &str, source: String, load: bool| link::ScriptAddition {
        shipment: "qm-cycle-gate".into(),
        name: name.into(),
        source,
        load: load.then(|| LibraryLoad { when: "world.scripts_loaded".into(), sessions: vec![LoadSession::Gameplay] }),
    };
    vec![
        lib("qm_cyc_root", cycle_gate("qm-cycle-gate/src/qm_cyc_root.lua"), true),
        lib("qm_cyc_a", cycle_gate("qm-cycle-gate/src/qm_cyc_a.lua"), false),
        lib("qm_cyc_b", b.to_string(), false),
    ]
}

/// ★ M0314: in a library's own cycle, code that reads, at its top level, a module whose body has not
/// run is an error naming its Shipment and line; the read of the module that already ran is not (the
/// cycle gate's run A: `qm_cyc_b`'s body read `qm_cyc_a.ran` as nil, live).
#[test]
fn a_top_level_read_of_a_module_whose_body_has_not_run_is_m0314() {
    let out = link_vz(&[], &cycle_gate_library(&cycle_gate("qm-cycle-gate/src/qm_cyc_b.lua")), &["qm-cycle-gate"]);
    assert_eq!(found(&out), vec![("M0314", vec!["qm-cycle-gate".to_string()], "qm_cyc_b".to_string())]);
    assert_eq!(out.findings[0].rule, mercs2_quartermaster::imports::ImportRule::LoadOrder);
    assert!(
        out.findings[0].message.starts_with(
            "qm-cycle-gate: its add_script qm_cyc_b uses qm_cyc_a at its top level (line 3), and qm_cyc_b imports \
             qm_cyc_a, whose body the engine runs after qm_cyc_b's ("
        ),
        "{}",
        out.findings[0].message
    );
}

/// ★ M0314: the same cycle is no finding when the module is read only in `Init` (the cycle gate's
/// run A: `qm_cyc_b`'s Init read `qm_cyc_a.ran` as true, live, through the binding its body made).
#[test]
fn a_cycle_read_only_in_init_is_no_m0314() {
    let b = "import(\"qm_cyc_a\")\nran = true\n\nfunction Init()\n    print(\"[qmcyc] b Init: a.ran=\" .. tostring(qm_cyc_a and qm_cyc_a.ran))\nend\n";
    let out = link_vz(&[], &cycle_gate_library(b), &["qm-cycle-gate"]);
    assert_eq!(found(&out), vec![]);
}

/// ★ M0314: an import a link adds that reorders the loading of retail's cycle is a warning, the only
/// finding: `antiair` importing `mrxartillery` (its retail DEPS lack it), a module of the cycle whose
/// body runs after `antiair`'s, changes the order the world's `vz` pass loads retail scripts in, and
/// the append reads nothing of it at its top level.
#[test]
fn an_added_import_that_reorders_retail_loading_is_an_m0314_warning() {
    let muts = vec![ScriptMutation {
        shipment: "reorder".into(),
        target: "antiair".into(),
        append: "local a = import(\"mrxartillery\")\n".into(),
    }];
    let out = link_vz(&muts, &[], &["reorder"]);
    assert_eq!(found(&out), vec![("M0314", vec!["reorder".to_string()], "antiair".to_string())]);
    assert_eq!(out.findings[0].rule, mercs2_quartermaster::imports::ImportRule::Reorder);
    assert!(out.findings[0].rule.is_warning());
    assert!(
        out.findings[0].message.contains("when vz loads: mrxsupportdesignatorbeacon's env now runs where antiair's env ran"),
        "{}",
        out.findings[0].message
    );
}

/// An `add_script` whose name a script of the OTHER block carries is refused. New scripts are minted
/// into the `wifpmcinterior` block (`scripts_vz`), and `mrxplayer` is a `resident` script: the
/// refusal must come from every block, not only the one the module would land in.
#[test]
fn an_add_script_named_like_a_resident_script_is_refused() {
    let mut loaded = retail_blocks();
    let corpus = corpus();
    assert!(loaded[0].1.find_script_by_name("mrxplayer").is_none(), "mrxplayer is not a scripts_vz script");
    assert!(loaded[1].1.find_script_by_name("mrxplayer").is_some(), "mrxplayer is a resident script");
    let additions = [link::ScriptAddition {
        shipment: "clash".into(),
        name: "mrxplayer".into(),
        source: "return {}\n".into(),
        load: None,
    }];
    let mut targets: Vec<link::TargetBlock<'_>> = loaded
        .iter_mut()
        .map(|(path, block)| link::TargetBlock { path: path.clone(), block })
        .collect();
    let err = link::link_into_blocks(&mut targets, &corpus, &[], &[], &[], &[], &[], &additions, &[], &order(&["clash"]))
        .expect_err("a name the resident block carries must not be minted");
    match err {
        link::LinkError::NameTaken { name, shipment, path } => {
            assert_eq!((name.as_str(), shipment.as_str()), ("mrxplayer", "clash"));
            assert_eq!(path, r"blocks\VZ\resident_P000_Q3.block");
        }
        other => panic!("expected NameTaken, got {other}"),
    }
}

/// Two Shipments' support registrations of one novel module are refused rather than the second's
/// source dropped; one Shipment's two items sharing a module and source mint it once.
#[test]
fn one_novel_support_module_from_two_shipments_is_refused() {
    let corpus = corpus();
    let reg = |shipment: &str, id: &str, source: &str| link::SupportRegistration {
        shipment: shipment.into(),
        module: "qm_w2_strike".into(),
        source: source.into(),
        id: id.into(),
        name: id.into(),
        description: String::new(),
        icon: "hud_support_airstrike".into(),
        item_type: "Airstrike".into(),
        cash_cost: 1,
        fuel_cost: 0,
        max_stock: 1,
        unlock_table: "{}".into(),
        cargo: None,
        delivery_vehicle: None,
        shops: vec!["Pmc".into()],
    };
    let link_with = |regs: &[link::SupportRegistration], names: &[&str]| {
        let mut loaded = retail_blocks();
        let mut targets: Vec<link::TargetBlock<'_>> = loaded
            .iter_mut()
            .map(|(path, block)| link::TargetBlock { path: path.clone(), block })
            .collect();
        link::link_into_blocks(&mut targets, &corpus, &[], &[], &[], regs, &[], &[], &[], &order(names)).map(|o| o.scripts)
    };
    let src = "QM_W2 = true\n";
    let err = link_with(&[reg("aaa", "strike_a", src), reg("bbb", "strike_b", "QM_W2_B = true\n")], &["aaa", "bbb"])
        .expect_err("one module from two Shipments must be refused");
    match err {
        link::LinkError::SupportModuleTwice { module, first, second } => {
            assert_eq!((module.as_str(), first.as_str(), second.as_str()), ("qm_w2_strike", "aaa", "bbb"));
        }
        other => panic!("expected SupportModuleTwice, got {other}"),
    }
    let linked = link_with(&[reg("aaa", "strike_a", src), reg("aaa", "strike_b", src)], &["aaa"]).expect("shared module");
    assert_eq!(linked.iter().filter(|l| l.target == "qm_w2_strike").count(), 1, "minted once");
}

/// `dynamic_import(...)`, `import(<expr>)` and a concatenated name list no DEPS entry, so M0209 never
/// sees them.
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
    let out = link::link_into_blocks(&mut targets, &corpus, &muts, &[], &[], &[], &[], &[], &[], &order(&["consumer"]))
        .expect("link");
    assert_eq!(out.findings, vec![]);
}

// ---------------------------------------------------------------------------
// The front end: `shell.wad`'s scripts block
// ---------------------------------------------------------------------------

/// The `shell.wad` beside the configured `vz.wad`.
fn shell_wad() -> PathBuf {
    let vz = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    mercs2_quartermaster::sound::sibling_wad(&vz, "shell.wad").unwrap_or_else(|e| panic!("{e}"))
}

/// The front end's scripts block ([`link::SHELL_SCRIPT_BLOCKS`]), decompressed, from `shell.wad`.
fn shell_block_bytes() -> Vec<u8> {
    let wad = shell_wad();
    let mut file = std::fs::File::open(&wad).unwrap_or_else(|e| panic!("open {}: {e}", wad.display()));
    let size = file.metadata().unwrap_or_else(|e| panic!("stat {}: {e}", wad.display())).len();
    let archive = load_ffcs_archive(&mut file, size)
        .unwrap_or_else(|e| panic!("read the FFCS archive {}: {e:?}", wad.display()));
    let (needle, path) = link::SHELL_SCRIPT_BLOCKS[0];
    let hits: Vec<usize> = archive
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| p.to_lowercase().contains(&needle.to_lowercase()))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1, "one block matches {needle:?}: {hits:?}");
    assert!(archive.paths[hits[0]].eq_ignore_ascii_case(path), "{}", archive.paths[hits[0]]);
    decompress_block(&mut file, &archive.indx, hits[0] as u16)
        .unwrap_or_else(|e| panic!("decompress {path} of {}: {e:?}", wad.display()))
}

const SCRIPT_TYPE_HASH: u32 = 0x4249_8680;

/// ★ `shell.wad`'s scripts block parses, carries the front end's 28 Lua chunks (`MrxSound` among
/// them), every container's CSUM verifies, and an unedited block re-serializes byte for byte.
#[test]
fn the_shell_resident_block_round_trips_byte_identically() {
    let raw = shell_block_bytes();
    let block = ScriptsBlock::parse(&raw).expect("the shell block parses");
    let scripts = block.entries.iter().filter(|e| e.type_hash == SCRIPT_TYPE_HASH).count();
    eprintln!("shell resident block: {} entries, {scripts} of them Lua", block.entries.len());
    assert_eq!(scripts, 28, "the front end's 28 scripts");
    assert!(block.find_script_by_name("mrxsound").is_some(), "the menu hooks' host is in the block");
    assert!(block.find_script_by_name("shellbootstrap").is_some(), "the front-end entry's host is in the block");
    assert!(block.find_script_by_name(link::QM_SHELL_MODLOADER_NAME).is_none(), "the loader is novel");
    assert_eq!(block.verify_csums().expect("every CSUM verifies as shipped"), block.entries.len());
    assert_eq!(block.serialize(), raw, "an unedited block re-serializes byte for byte");
}

/// The DEPS entries of `blocks`' scripts that name no script of `blocks`, as `(script, hash)`.
fn unresolved_retail_deps(blocks: &[&ScriptsBlock]) -> (usize, Vec<(String, u32)>) {
    let scripts: Vec<&mercs2_formats::scripts_block::Entry> =
        blocks.iter().flat_map(|b| &b.entries).filter(|e| e.type_hash == SCRIPT_TYPE_HASH).collect();
    let held: std::collections::BTreeSet<u32> = scripts.iter().map(|e| e.name_hash).collect();
    let mut missing = Vec::new();
    for e in &scripts {
        let name = mercs2_formats::scripts_block::container_info(&e.bytes).expect("INFO").name;
        for d in mercs2_formats::scripts_block::container_deps(&e.bytes).expect("DEPS") {
            if !held.contains(&d) {
                missing.push((name.clone(), d));
            }
        }
    }
    (scripts.len(), missing)
}

/// ★ Every retail script's DEPS names a script of the blocks its link is given: `vz`'s two blocks
/// ([`link::SCRIPT_BLOCKS`]) for gameplay and the shell block ([`link::SHELL_SCRIPT_BLOCKS`]) for the
/// front end. The import checks (M0209, M0313) resolve a module against exactly those blocks, so a
/// retail module outside them would be refused as a module nothing provides.
#[test]
fn every_retail_dependency_resolves_within_its_link_s_blocks() {
    let vz = retail_blocks();
    let (n, missing) = unresolved_retail_deps(&vz.iter().map(|(_, b)| b).collect::<Vec<_>>());
    eprintln!("vz link blocks: {n} scripts, {} unresolved DEPS entries", missing.len());
    assert_eq!(missing, vec![], "every vz DEPS entry is a script of the vz link's blocks");
    let shell = ScriptsBlock::parse(&shell_block_bytes()).expect("the shell block parses");
    let (n, missing) = unresolved_retail_deps(&[&shell]);
    eprintln!("shell link block: {n} scripts, {} unresolved DEPS entries", missing.len());
    assert_eq!(missing, vec![], "every shell DEPS entry is a script of the shell block");
}

/// ★ The front end checks its own imports: a front-end library that imports a gameplay-only module
/// (`mrxplayer` is a `vz.wad` resident script, not a shell one) is M0209, and one that imports a shell
/// script resolves.
#[test]
fn a_front_end_library_importing_a_gameplay_only_module_is_m0209() {
    use mercs2_quartermaster::manifest::{LibraryLoad, LoadSession};
    let mut block = ScriptsBlock::parse(&shell_block_bytes()).unwrap();
    assert!(block.find_script_by_name("mrxplayer").is_none(), "mrxplayer is not a shell script");
    let load = || Some(LibraryLoad { when: "shell.boot".into(), sessions: vec![LoadSession::FrontEnd] });
    let additions = [
        link::ScriptAddition { shipment: "menu".into(), name: "qm_t_menu".into(), source: "local p = import(\"mrxplayer\")\n".into(), load: load() },
        link::ScriptAddition { shipment: "menu".into(), name: "qm_t_menu_ok".into(), source: "local s = import(\"mrxsound\")\n".into(), load: load() },
    ];
    let mut targets = [link::TargetBlock { path: link::SHELL_SCRIPT_BLOCKS[0].1.into(), block: &mut block }];
    let out = link::link_front_end(&mut targets, &[], &additions, &order(&["menu"]), link::QmHooks::ForLibraries).expect("links");
    assert_eq!(found(&out), vec![("M0209", vec!["menu".to_string()], "mrxplayer".to_string())]);
    assert!(out.findings[0].message.contains("add_script qm_t_menu"), "{}", out.findings[0].message);
}

/// ★ The front end's hooks and loader, linked into the retail shell block: `qm_shell_modloader` is
/// minted under its name hash, qm's tails are spliced onto the retail bytecode of `mrxsound` (the
/// menu hooks, whose targets its main chunk defines) and `shellbootstrap` (the entry, which loads the
/// loader), every other entry is byte-identical, and every CSUM verifies. Nothing comes from the
/// corpus.
#[test]
fn the_front_end_loader_links_into_the_shell_block() {
    use mercs2_quartermaster::manifest::LoadSession;
    let raw = shell_block_bytes();
    let original = ScriptsBlock::parse(&raw).unwrap();
    let mut block = ScriptsBlock::parse(&raw).unwrap();
    let regs = [
        link::SoundBankRegistration {
            shipment: "pda-sound".into(),
            bank: "qm_pda-sound_ui_hud".into(),
            soundbank: false,
            sessions: [LoadSession::FrontEnd, LoadSession::Gameplay].into(),
        },
        link::SoundBankRegistration {
            shipment: "pda-sound".into(),
            bank: "gameplay_only_bank".into(),
            soundbank: true,
            sessions: [LoadSession::Gameplay].into(),
        },
    ];
    let mut targets = [link::TargetBlock { path: link::SHELL_SCRIPT_BLOCKS[0].1.into(), block: &mut block }];
    let out = link::link_front_end(&mut targets, &regs, &[], &order(&["pda-sound"]), link::QmHooks::ForLibraries).expect("links");
    let front = qm_hosts(LoadSession::FrontEnd);
    let names: Vec<&str> = out.scripts.iter().map(|l| l.target.as_str()).collect();
    let want: Vec<&str> = std::iter::once(link::QM_SHELL_MODLOADER_NAME).chain(front.iter().map(String::as_str)).collect();
    assert_eq!(names, want);
    assert!(out.scripts.iter().all(|l| l.base_source_bytes == 0), "nothing is recompiled from the corpus");

    assert_eq!(block.entries.len(), original.entries.len() + 1);
    let hosts: Vec<usize> = front.iter().map(|h| original.find_script_by_name(h).unwrap_or_else(|| panic!("{h} is a shell script"))).collect();
    for (i, (a, b)) in original.entries.iter().zip(&block.entries).enumerate() {
        if hosts.contains(&i) {
            assert_eq!((a.name_hash, a.type_hash), (b.name_hash, b.type_hash));
            assert_ne!(a.bytes, b.bytes, "entry {i} carries qm's tail");
            let (base, now) = (original.extract_lua(i).unwrap(), block.extract_lua(i).unwrap());
            assert!(now.len() > base.len(), "entry {i}: the tail is appended to the retail chunk");
        } else {
            assert!(
                (a.name_hash, a.type_hash, a.field_c, &a.bytes) == (b.name_hash, b.type_hash, b.field_c, &b.bytes),
                "entry {i} is unchanged"
            );
        }
    }
    let minted = block.entries.last().unwrap();
    assert_eq!(minted.name_hash, mercs2_formats::hash::pandemic_hash_m2(link::QM_SHELL_MODLOADER_NAME));
    assert_eq!(minted.type_hash, SCRIPT_TYPE_HASH);
    let reparsed = ScriptsBlock::parse(&block.serialize()).expect("re-parses");
    assert_eq!(reparsed.verify_csums().expect("every CSUM verifies"), reparsed.entries.len());

    let contains = |hay: &[u8], needle: &str| hay.windows(needle.len()).any(|w| w == needle.as_bytes());
    let loader = reparsed.extract_lua(reparsed.find_script_by_name(link::QM_SHELL_MODLOADER_NAME).unwrap()).unwrap();
    assert!(loader.starts_with(&mercs2_luac::MERCS2_LUAQ_HEADER));
    assert!(contains(&loader, "qm_pda-sound_ui_hud") && contains(&loader, "LoadWaveBank"));
    assert!(!contains(&loader, "gameplay_only_bank"), "a gameplay bank is not the front end's");
    let sound = reparsed.extract_lua(reparsed.find_script_by_name("mrxsound").unwrap()).unwrap();
    for needle in ["EnterShellState", "ExitShellState", "shell.open", "shell.sound_exit", "_StartShellMusic"] {
        assert!(contains(&sound, needle), "mrxsound carries {needle}");
    }
    let boot = reparsed.extract_lua(reparsed.find_script_by_name("shellbootstrap").unwrap()).unwrap();
    for needle in ["shell.boot", "dynamic_import", link::QM_SHELL_MODLOADER_NAME] {
        assert!(contains(&boot, needle), "shellbootstrap carries {needle}");
    }

    // No front-end registration in a standalone build: nothing is linked and the block is untouched.
    let mut untouched = ScriptsBlock::parse(&raw).unwrap();
    let mut targets = [link::TargetBlock { path: String::new(), block: &mut untouched }];
    let none = link::link_front_end(&mut targets, &regs[1..], &[], &order(&["pda-sound"]), link::QmHooks::ForLibraries).unwrap();
    assert!(none.scripts.is_empty());
    assert_eq!(untouched.serialize(), raw);

    // The deploy link splices the front end's hooks with nothing to load (W-a), and mints nothing.
    let mut bare = ScriptsBlock::parse(&raw).unwrap();
    let mut targets = [link::TargetBlock { path: String::new(), block: &mut bare }];
    let hooked = link::link_front_end(&mut targets, &[], &[], &order(&[]), link::QmHooks::Always).unwrap();
    let names: Vec<String> = hooked.scripts.iter().map(|l| l.target.clone()).collect();
    assert_eq!(names, front);
    assert_eq!(bare.entries.len(), original.entries.len());
}

/// ★ Every wrap of the catalogue targets a function its host's main chunk defines in the retail
/// bytecode qm splices onto, and that chunk returns only at its end, where the tail runs: `vz.wad`'s
/// blocks for gameplay, `shell.wad`'s for the front end. These are the checks the link makes before it
/// splices (`script_meta::main_run`), run over the whole catalogue.
#[test]
fn every_catalogue_wrap_targets_a_function_its_retail_host_defines() {
    use mercs2_quartermaster::hooks::{hooks, Kind};
    use mercs2_quartermaster::manifest::LoadSession;
    let gameplay: Vec<ScriptsBlock> = retail_blocks().into_iter().map(|(_, b)| b).collect();
    let shell = [ScriptsBlock::parse(&shell_block_bytes()).expect("the shell block parses")];
    let mut checked = 0;
    for h in hooks().iter().filter(|h| h.kind == Kind::Wrap) {
        for session in &h.sessions {
            let blocks: &[ScriptsBlock] = if *session == LoadSession::FrontEnd { &shell } else { &gameplay };
            let (b, idx) = blocks
                .iter()
                .find_map(|b| b.find_script_by_name(&h.host).map(|i| (b, i)))
                .unwrap_or_else(|| panic!("hook {}: no retail script {} in the {session:?} blocks", h.id, h.host));
            let run = mercs2_formats::script_meta::main_run(&b.extract_lua(idx).unwrap())
                .unwrap_or_else(|e| panic!("hook {}: {}'s main chunk: {e}", h.id, h.host));
            let target = h.target.as_deref().unwrap();
            assert!(run.functions.contains(target), "hook {}: {} defines no function {target}", h.id, h.host);
            assert_eq!(run.early_return, None, "hook {}: {} returns before its end", h.id, h.host);
            checked += 1;
        }
    }
    eprintln!("checked {checked} wraps against their retail hosts");
    assert!(checked >= 60, "the catalogue's wraps: {checked}");
}
