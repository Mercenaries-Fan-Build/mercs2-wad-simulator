//! Composition: does the merge model actually let real mods coexist, and does it catch the cases
//! that silently break the game?
//!
//! These are the tests that make the composition model more than a design note. All hermetic — no
//! game install — because merge semantics are a property of the base game we have already reversed,
//! not something we need the WADs to re-derive.

use mercs2_quartermaster::blast::{self, Access, Claim, MergeClass};
use mercs2_quartermaster::link::Level;
use mercs2_quartermaster::{from_str, Format, Manifest};

fn parse(yaml: &str) -> Manifest {
    from_str(yaml, Format::Yaml).unwrap_or_else(|e| panic!("fixture must parse: {e}\n{yaml}"))
}

/// A wardrobe mod: one outfit for one hero.
fn outfit(shipment: &str, asset: &str, wearer: &str, slug: &str) -> Manifest {
    parse(&format!(
        "format: 2
shipment: {{ name: {shipment}, version: 1.0.0, target: retail }}
contributions:
  - kind: add_outfit
    name: {asset}
    slug: {slug}
    display: Display Name
    wearer: {wearer}
    model: src/m.glb
"
    ))
}

fn replace_texture(shipment: &str, target: &str) -> Manifest {
    parse(&format!(
        "format: 2
shipment: {{ name: {shipment}, version: 1.0.0, target: retail }}
contributions:
  - kind: replace_texture
    target: {target}
    image: src/t.png
"
    ))
}

// ---------------------------------------------------------------------------
// The headline: wardrobe mods must compose.
// ---------------------------------------------------------------------------

/// THE case the whole composition model exists for. Two independent outfit mods, same hero,
/// different outfits. Under naive whole-block semantics one silently annihilates the other; under
/// the merge model they coexist.
#[test]
fn two_outfit_mods_for_the_same_hero_coexist() {
    let a = outfit("sean-devlin", "sean_devlin", "mattias", "SeanDevlin");
    let b = outfit("roze-skin", "roze", "mattias", "Roze");

    let found = blast::conflicts(&[("sean-devlin", &a), ("roze-skin", &b)]);
    assert!(found.is_empty(), "outfit mods must compose, got: {found:?}");
}

/// ...but the same slug on the same hero is a genuine duplicate key.
#[test]
fn the_same_outfit_slug_on_one_hero_collides() {
    let a = outfit("mod-a", "sean_a", "mattias", "Commando");
    let b = outfit("mod-b", "sean_b", "mattias", "Commando");

    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::KeyedSet);
    assert!(
        matches!(&found[0].claim, Claim::OutfitSlot { wearer, slug } if wearer == "mattias" && slug == "Commando")
    );
}

/// The key is (wearer, slug), NOT slug alone — retail itself reuses `Original` and `ChickenSuit`
/// across all three heroes, so keying on slug alone would reject legitimate manifests.
#[test]
fn the_same_slug_on_different_heroes_is_fine() {
    let a = outfit("mod-a", "asset_a", "mattias", "Original");
    let b = outfit("mod-b", "asset_b", "jennifer", "Original");

    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert!(found.is_empty(), "slug is scoped per hero, got: {found:?}");
}

/// Both outfit mods claim `wifpmcinterior`. That claim must NOT be exclusive, or the headline case
/// above could never pass — it is merge-able because we reversed how `_tOutfits` composes.
#[test]
fn the_wardrobe_script_is_mergeable_not_exclusive() {
    let m = outfit("s", "a", "mattias", "X");
    let script_claim = blast::claims(&m)
        .into_iter()
        .find(|r| matches!(&r.claim, Claim::Script { name, level: Level::Vz } if name == "wifpmcinterior"))
        .expect("add_outfit must claim the wardrobe script");
    assert_eq!(script_claim.class, MergeClass::OrderedList);
}

// ---------------------------------------------------------------------------
// Fail closed.
// ---------------------------------------------------------------------------

/// An append to ANY script composes: the linker concatenates every Shipment's appends onto
/// the base source and compiles once, so there is no curated list of scripts that may be patched.
#[test]
fn patch_lua_any_script_composes() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: patch_lua
    target: wifmissionflow
    append: src/a.lua
"
        ))
    };
    let a = mk("mod-a");
    let b = mk("mod-b");
    assert!(blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]).is_empty());
    assert_eq!(blast::claims(&a)[0].class, MergeClass::OrderedList);
}

/// Raw is the open lower bound: we cannot infer anything about the bytes, so the declared blast
/// radius is trusted and the class fails closed.
#[test]
fn two_raw_contributions_touching_one_target_collide() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: raw
    payload: src/blob.bin
    target_layer: data
    touches: [\"al_veh_boat_destroyer\"]
"
        ))
    };
    let (a, b) = (mk("mod-a"), mk("mod-b"));
    assert_eq!(blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]).len(), 1);
}

/// ASI plugins have NO arbitration — discovery is filesystem order across four directories, so
/// there is no load order that resolves two plugins hooking one address.
#[test]
fn two_native_hooks_on_one_address_collide() {
    let mk = |name: &str, asi: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: native_hook
    target: retail
    plugin: src/{asi}
    touches: [\"0x004CF340\"]
"
        ))
    };
    let (a, b) = (mk("mod-a", "a.asi"), mk("mod-b", "b.asi"));
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::Exclusive);
    assert!(matches!(&found[0].claim, Claim::NativeHook { .. }));
}

/// Two Shipments shipping an `.asi` of the same FILENAME collide on one path, even if they hook
/// different addresses.
#[test]
fn two_plugins_with_the_same_filename_collide() {
    let mk = |name: &str, at: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: native_hook
    target: retail
    plugin: src/bridge.asi
    touches: [\"{at}\"]
"
        ))
    };
    let (a, b) = (mk("mod-a", "0x1000"), mk("mod-b", "0x2000"));
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert!(
        found.iter().any(
            |c| matches!(&c.claim, Claim::FileArtifact { path } if path == "scripts/bridge.asi")
        ),
        "same .asi filename must collide, got {found:?}"
    );
}

/// A placed companion is a claim on a DESTINATION PATH, and the filesystem has no arbitration of
/// any kind — no stack to reorder, no first-writer registry, no load order. Whichever deploy step
/// runs last simply overwrites, and the loser does not degrade to the base game: the plugin goes on
/// reading the file and gets somebody else's config, with nothing logged.
#[test]
fn two_shipments_writing_one_companion_path_collide() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: place_file
    file: src/config.ini
    dest: scripts
"
        ))
    };
    let (a, b) = (mk("mod-a"), mk("mod-b"));
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::Exclusive);
    assert!(
        matches!(&found[0].claim, Claim::FileArtifact { path } if path == "scripts/config.ini")
    );
}

/// ...but the same FILENAME in two different destinations is two files, and they must not fight.
/// This is why the claim is keyed on the path rather than the name: keying on the name would have
/// called this a conflict and been right about the previous test by accident.
#[test]
fn one_filename_in_two_destinations_does_not_collide() {
    let mk = |name: &str, dest: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: place_file
    file: src/config.ini
    dest: {dest}
"
        ))
    };
    let (a, b) = (mk("mod-a", "scripts"), mk("mod-b", "plugins"));
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert!(found.is_empty(), "two paths, two files: {found:?}");
}

/// A companion and the plugin it belongs to are different paths, so a Shipment shipping both is not
/// conflicting with itself — the case every real Code-layer mod is.
#[test]
fn a_plugin_and_its_companion_are_not_a_self_conflict() {
    let m = parse(
        "format: 2
shipment: { name: bridge, version: 1.0.0, target: retail }
contributions:
  - kind: native_hook
    target: retail
    plugin: src/lua_bridge_DEV.asi
    touches: [\"0x004CF340\"]
  - kind: place_file
    file: src/lua_bridge_DEV.ini
    dest: scripts
",
    );
    let found = blast::self_conflicts(&m);
    assert!(found.is_empty(), "got {found:?}");
    let paths: Vec<String> = blast::claims(&m)
        .into_iter()
        .filter(|r| r.access == Access::Write)
        .filter_map(|r| match r.claim {
            Claim::FileArtifact { path } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(
        paths,
        vec!["scripts/lua_bridge_dev.asi", "scripts/lua_bridge_dev.ini"],
        "the plugin and its companion each claim their own path, in the same directory, keyed \
         lowercased"
    );
}

// ---------------------------------------------------------------------------
// Additive vs replacement.
// ---------------------------------------------------------------------------

/// Minting the same NEW asset name in two Shipments is a hard error: the chunk registry is
/// first-wins, so one of them silently vanishes. Load order does not save you.
#[test]
fn two_shipments_minting_the_same_new_name_collide() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: add_model
    name: my_custom_helipad
    model: src/m.glb
"
        ))
    };
    let (a, b) = (mk("mod-a"), mk("mod-b"));
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::KeyedSet);
}

/// Replacing the SAME shipped texture is not an error — the WAD stack is last-mounted-wins and
/// picking the winner is exactly what load order is for.
#[test]
fn two_texture_replacements_are_load_order_not_conflict() {
    let a = replace_texture("reskin-a", "al_hum_boss_ub");
    let b = replace_texture("reskin-b", "al_hum_boss_ub");
    let found = blast::conflicts(&[("reskin-a", &a), ("reskin-b", &b)]);
    assert!(
        found.is_empty(),
        "texture replacement is LastWins, got {found:?}"
    );

    let m = blast::claims(&a);
    assert_eq!(m[0].class, MergeClass::LastWins);
}

// ---------------------------------------------------------------------------
// Reads vs writes.
// ---------------------------------------------------------------------------

/// `donor:` is BORROWED — read, never written. If it were recorded as a write, every mod using the
/// same donor would falsely conflict.
#[test]
fn a_donor_is_a_read_not_a_write() {
    let m = parse(
        "format: 2
shipment: { name: s, version: 1.0.0, target: retail }
contributions:
  - kind: add_model
    name: my_thing
    model: src/m.glb
    donor: deliverycrate
",
    );
    let records = blast::claims(&m);
    let donor = records
        .iter()
        .find(|r| r.name.as_deref() == Some("deliverycrate"))
        .expect("donor must appear in the blast radius");
    assert_eq!(donor.access, Access::Read);
    assert!(matches!(donor.claim, Claim::Asset { .. }));

    // ...and the thing we MINT is a write, so the two are not confusable.
    let minted = records
        .iter()
        .find(|r| r.name.as_deref() == Some("my_thing"))
        .expect("the new asset must appear too");
    assert_eq!(minted.access, Access::Write);
}

#[test]
fn many_shipments_may_share_one_donor() {
    let mk = |name: &str, asset: &str| {
        parse(&format!(
            "format: 2
shipment: {{ name: {name}, version: 1.0.0, target: retail }}
contributions:
  - kind: add_model
    name: {asset}
    model: src/m.glb
    donor: deliverycrate
"
        ))
    };
    let (a, b) = (mk("mod-a", "thing_a"), mk("mod-b", "thing_b"));
    assert!(blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]).is_empty());
}

/// A read nothing in the set provides. This is deliberately NOT called "missing" — most donors are
/// base-game assets, and confirming that needs the WAD stack.
#[test]
fn an_unprovided_read_is_reported_without_claiming_it_is_missing() {
    let m = parse(
        "format: 2
shipment: { name: s, version: 1.0.0, target: retail }
contributions:
  - kind: add_model
    name: my_thing
    model: src/m.glb
    donor: some_other_mods_asset
",
    );
    let unsat = blast::unsatisfied_reads(&[("s", &m)]);
    assert_eq!(unsat.len(), 1);
    assert_eq!(unsat[0].by.shipment, "s");
}

/// The cross-Shipment dependency the read-set exists to model: B provides what A borrows.
#[test]
fn a_read_satisfied_by_another_shipment_is_not_reported() {
    let a = parse(
        "format: 2
shipment: { name: consumer, version: 1.0.0, target: retail }
contributions:
  - kind: add_model
    name: derived_thing
    model: src/m.glb
    donor: provided_thing
",
    );
    let b = parse(
        "format: 2
shipment: { name: provider, version: 1.0.0, target: retail }
contributions:
  - kind: add_model
    name: provided_thing
    model: src/p.glb
",
    );
    let unsat = blast::unsatisfied_reads(&[("consumer", &a), ("provider", &b)]);
    assert!(
        unsat.is_empty(),
        "provider satisfies the read, got {unsat:?}"
    );
}

// ---------------------------------------------------------------------------
// Within one Shipment.
// ---------------------------------------------------------------------------

/// Inside one Shipment there is no load order to appeal to, so any duplicate write is an error
/// regardless of merge class — and far more likely a copy-paste mistake than an intention.
#[test]
fn a_shipment_may_not_claim_one_target_twice() {
    let m = parse(
        "format: 2
shipment: { name: s, version: 1.0.0, target: retail }
contributions:
  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/a.png
  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/b.png
",
    );
    let selfs = blast::self_conflicts(&m);
    assert_eq!(selfs.len(), 1, "got {selfs:?}");
    assert_eq!(selfs[0].indices, vec![0, 1]);
    assert!(
        selfs[0].to_string().contains("al_hum_boss_ub"),
        "{}",
        selfs[0]
    );
}

/// Two outfits in ONE Shipment is normal and must not trip the self-conflict check — they share the
/// wardrobe script claim, which is OrderedList.
#[test]
fn one_shipment_may_add_several_outfits() {
    let m = parse(
        "format: 2
shipment: { name: pack, version: 1.0.0, target: retail }
contributions:
  - kind: add_outfit
    name: outfit_one
    slug: One
    display: One
    wearer: mattias
    model: src/1.glb
  - kind: add_outfit
    name: outfit_two
    slug: Two
    display: Two
    wearer: mattias
    model: src/2.glb
",
    );
    let selfs = blast::self_conflicts(&m);
    assert!(
        selfs
            .iter()
            .all(|c| !matches!(&c.claim, Claim::Script { .. })),
        "a shared mergeable script claim is not a self-conflict: {selfs:?}"
    );
    assert!(selfs.is_empty(), "got {selfs:?}");
}

/// `touches` accepts a bare hash only as the documented escape; it must resolve to the SAME claim
/// as the name, or a Shipment could evade conflict detection by writing the hash instead.
#[test]
fn a_bare_hash_touch_and_its_name_are_the_same_claim() {
    let by_name = parse(
        "format: 2
shipment: { name: a, version: 1.0.0, target: retail }
contributions:
  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"al_veh_boat_destroyer\"]
",
    );
    let by_hash = parse(
        "format: 2
shipment: { name: b, version: 1.0.0, target: retail }
contributions:
  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"0xE54047D5\"]
",
    );
    let found = blast::conflicts(&[("a", &by_name), ("b", &by_hash)]);
    assert_eq!(
        found.len(),
        1,
        "writing the hash must not evade detection: {found:?}"
    );
}

// ---------------------------------------------------------------------------
// Shop items: two shop mods must compose, and the emit hits the right scripts.
// ---------------------------------------------------------------------------

/// A support-catalog shop mod adding one crate-delivery item to two vendors.
fn shop_item(shipment: &str, id: &str, cargo: &str) -> Manifest {
    parse(&format!(
        "format: 2
shipment: {{ name: {shipment}, version: 1.0.0, target: retail }}
contributions:
  - kind: add_shop_item
    id: {id}
    name: \"{id} display\"
    icon: vehicles_tank_m1a2
    type: heavy
    shops: [pmc, gur]
    cash_cost: 250000
    fuel_cost: 75
    max_stock: 4
    unlocked: true
    behaviour: {{ module: mrxcratedelivery, cargo: \"{cargo}\", delivery_vehicle: \"Mi26 (PMC) (Driver)\" }}
"
    ))
}

/// THE reason a shop item is delivered as linked appends, not a full-block replace of the resident
/// catalog block: two independent shop mods adding different items to the same vendor must coexist.
#[test]
fn two_shop_item_mods_coexist() {
    let a = shop_item("tank-pack", "dlcm1a1", "LAVIII (Minigun)");
    let b = shop_item("racer-pack", "nukeracer", "Nuke Racer");
    let found = blast::conflicts(&[("tank-pack", &a), ("racer-pack", &b)]);
    assert!(found.is_empty(), "shop mods must compose, got: {found:?}");
}

/// Every claim a shop item makes is `OrderedList` (mergeable), never `Exclusive` — the property
/// that makes the coexistence above hold rather than being luck.
#[test]
fn shop_item_claims_are_ordered_list() {
    let m = shop_item("tank-pack", "dlcm1a1", "LAVIII (Minigun)");
    let claims = blast::claims(&m);
    assert!(!claims.is_empty());
    for c in &claims {
        assert_eq!(
            c.class,
            MergeClass::OrderedList,
            "claim {:?} must be mergeable, not {:?}",
            c.claim,
            c.class
        );
    }
}

/// The item lowers to appends on the two resident catalog scripts, and the emitted Lua carries the
/// catalog row plus one reward row per listed vendor (faction keys Capitalized).
#[test]
fn shop_item_lowers_to_catalog_and_reward_appends() {
    let m = shop_item("tank-pack", "dlcm1a1", "LAVIII (Minigun)");
    let muts =
        mercs2_quartermaster::build::script_mutations(&m, std::path::Path::new(".")).unwrap();
    let targets: Vec<&str> = muts.iter().map(|x| x.target.as_str()).collect();
    assert!(targets.contains(&"mrxsupportdata"), "targets: {targets:?}");
    assert!(targets.contains(&"mrxrewarddata"), "targets: {targets:?}");

    let support = muts.iter().find(|x| x.target == "mrxsupportdata").unwrap();
    assert!(support.append.contains("tSupportData[\"dlcm1a1\"]"));
    assert!(support.append.contains("mrxcratedelivery:Create()"));
    assert!(support.append.contains("SetCargo(\"LAVIII (Minigun)\")"));
    assert!(support.append.contains("sType = \"Heavy\""));
    assert!(support.append.contains("tUnlockStatus = { Pmc = 1, Gur = 1 }"));

    let reward = muts.iter().find(|x| x.target == "mrxrewarddata").unwrap();
    assert!(reward.append.contains("sFactionId = \"Pmc\""), "{}", reward.append);
    assert!(reward.append.contains("sFactionId = \"Gur\""), "{}", reward.append);
}

/// A NOVEL-behaviour shop item (`behaviour.script` set) emits NO load-time catalog append — its row
/// defers into `qm_modloader`, so `script_mutations` must skip it. An eager `module:Create()` on a
/// nil global would abort a resident script (the whole reason the deferred path exists).
#[test]
fn novel_behaviour_shop_item_skips_the_eager_append() {
    let m = parse(
        "format: 2
shipment: { name: bomb-mod, version: 1.0.0, target: retail }
contributions:
  - kind: add_shop_item
    id: ggbomb
    name: \"[support.airstrike.greengoblinbomb.name]\"
    icon: support_cluster_bomb
    type: airstrike
    shops: [pmc]
    cash_cost: 100000
    behaviour: { module: DLC_MrxGreenGoblinBomb, script: src/ggbomb.lua }
",
    );
    let muts =
        mercs2_quartermaster::build::script_mutations(&m, std::path::Path::new(".")).unwrap();
    assert!(
        muts.is_empty(),
        "a novel behaviour must not emit a load-time append (it defers to the loader): {muts:?}"
    );
}

// ---------------------------------------------------------------------------
// Scripts, same-target replacements, and game-folder case.
// ---------------------------------------------------------------------------

/// One contribution per Shipment, from a YAML block.
fn one(shipment: &str, contribution: &str) -> Manifest {
    parse(&format!(
        "format: 2
shipment: {{ name: {shipment}, version: 1.0.0, target: retail }}
contributions:
{contribution}"
    ))
}

/// The single cross-Shipment conflict `a` and `b` produce, which must be Exclusive.
fn exclusive_conflict(a: &Manifest, b: &Manifest) -> blast::Conflict {
    let found = blast::conflicts(&[("mod-a", a), ("mod-b", b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::Exclusive, "{}", found[0]);
    found.into_iter().next().unwrap()
}

#[test]
fn two_replace_lua_conflict() {
    let c = "  - kind: replace_lua\n    target: wifpmcgarage\n    source: src/g.lua\n";
    let found = exclusive_conflict(&one("mod-a", c), &one("mod-b", c));
    assert_eq!(found.claim, Claim::Script { name: "wifpmcgarage".into(), level: Level::Vz });
}

/// Decided: an append beside a wholesale replacement of the same script is a hard conflict — the
/// append would be applied to source that is no longer there.
#[test]
fn patch_and_replace_lua_conflict() {
    let a = one("mod-a", "  - kind: patch_lua\n    target: wifpmcgarage\n    append: src/a.lua\n");
    let b = one("mod-b", "  - kind: replace_lua\n    target: wifpmcgarage\n    source: src/g.lua\n");
    exclusive_conflict(&a, &b);
}

/// Every kind that is a hard conflict on the same target, one pair each.
#[test]
fn each_newly_exclusive_kind_conflicts_on_the_same_target() {
    for block in [
        "  - kind: replace_shader\n    target: s_hero\n    blob: src/b.bin\n",
        "  - kind: replace_fx\n    target: fx_boom\n    payload: src/p.bin\n",
        "  - kind: replace_animation\n    target: a_run\n    clip: src/c.bin\n    trnm: src/t.bin\n",
        "  - kind: replace_phy2\n    target: m_crate\n    phy2: src/p.bin\n",
        "  - kind: replace_terrain_cell\n    target: cell_0_0\n    cell: src/c.bin\n",
        "  - kind: edit_state_machine\n    target: al_veh_boat_destroyer\n    states: src/s.yaml\n",
        "  - kind: edit_world\n    layer: vz_state_pmccon004\n    edits: src/w.yaml\n",
    ] {
        exclusive_conflict(&one("mod-a", block), &one("mod-b", block));
    }
}

/// `edit_world` edits the whole layer, so an `add_placement` on that layer in another Shipment
/// cannot compose with it: the stricter class wins.
#[test]
fn edit_world_and_add_placement_on_one_layer_conflict() {
    let a = one("mod-a", "  - kind: edit_world\n    layer: layers_static\n    edits: src/w.yaml\n");
    let b = one("mod-b", "  - kind: add_placement\n    layer: layers_static\n    entity: src/e.yaml\n");
    exclusive_conflict(&a, &b);
}

/// Windows file names are case-insensitive, so two Shipments placing names that differ only in
/// case write one file.
#[test]
fn place_file_names_differing_only_in_case_collide() {
    let a = one("mod-a", "  - kind: place_file\n    file: src/Config.ini\n    dest: scripts\n");
    let b = one("mod-b", "  - kind: place_file\n    file: src/config.INI\n    dest: scripts\n");
    let found = exclusive_conflict(&a, &b);
    assert_eq!(found.claim, Claim::FileArtifact { path: "scripts/config.ini".into() });
}

#[test]
fn native_hook_names_differing_only_in_case_collide() {
    let a = one("mod-a", "  - kind: native_hook\n    target: retail\n    plugin: src/Bridge.asi\n");
    let b = one("mod-b", "  - kind: native_hook\n    target: retail\n    plugin: src/bridge.ASI\n");
    let found = exclusive_conflict(&a, &b);
    assert_eq!(found.claim, Claim::FileArtifact { path: "scripts/bridge.asi".into() });
}

/// An `add_runtime_dll` claims its game-root path, lowercased; two Shipments shipping one DLL name
/// conflict.
#[test]
fn runtime_dll_name_case_collides() {
    let a = one("m2-sdk", "  - kind: add_runtime_dll\n    dll: src/m2-sdk.dll\n");
    let b = one("dup-runtime", "  - kind: add_runtime_dll\n    dll: src/M2-SDK.DLL\n");
    let found = blast::conflicts(&[("m2-sdk", &a), ("dup-runtime", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].claim, Claim::FileArtifact { path: "m2-sdk.dll".into() });
    assert_eq!(found[0].class, MergeClass::Exclusive);
}

/// Within ONE Shipment the same rule is a self-conflict (M0120): two placements differing only in
/// case.
#[test]
fn a_case_only_difference_within_one_shipment_is_a_self_conflict() {
    let m = one(
        "mod-a",
        "  - kind: place_file\n    file: src/a/Readme.txt\n    dest: scripts\n  - kind: place_file\n    file: src/b/README.TXT\n    dest: scripts\n",
    );
    let found = blast::self_conflicts(&m);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].indices, vec![0, 1]);
}

// ---------------------------------------------------------------------------
// String tables: every writer to a table composes; an opaque raw block still conflicts.
// ---------------------------------------------------------------------------

const EDIT_ENGLISH: &str = "  - kind: edit_stringdb\n    target: english\n    strings: src/e.txt\n";

/// Any two Shipments editing one table compose, whatever keys they touch: `qm link` merges them.
#[test]
fn edit_stringdb_on_one_table_composes() {
    let a = one("mod-a", EDIT_ENGLISH);
    let b = one("mod-b", EDIT_ENGLISH);
    assert!(blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]).is_empty());
    assert_eq!(blast::claims(&a)[0].class, MergeClass::OrderedList);
    // Two edits of one table in ONE Shipment are fine too: both are merged.
    let both = one("mod-a", &format!("{EDIT_ENGLISH}{EDIT_ENGLISH}"));
    assert!(blast::self_conflicts(&both).is_empty());
}

/// `replace_stringdb_text` composes too: the link applies it, in load
/// order, against the table as merged so far. So it never conflicts with another writer to the
/// table, across Shipments or inside one.
#[test]
fn replace_stringdb_text_composes_with_other_table_writers() {
    const REPLACE: &str = "  - kind: replace_stringdb_text\n    target: english\n    pairs: src/p.txt\n";
    let replace = one("mod-b", REPLACE);
    assert!(blast::conflicts(&[("mod-a", &one("mod-a", EDIT_ENGLISH)), ("mod-b", &replace)]).is_empty());
    assert!(blast::conflicts(&[("mod-b", &replace), ("mod-c", &one("mod-c", REPLACE))]).is_empty());
    assert_eq!(blast::claims(&replace)[0].class, MergeClass::OrderedList);
    // One Shipment may fix a table by key and by text.
    let both = one("mod-a", &format!("{EDIT_ENGLISH}{REPLACE}"));
    assert!(blast::self_conflicts(&both).is_empty());
}

/// The table claim is on the table's asset hash, so an opaque `raw` declaring that table still
/// fails closed against an editor.
#[test]
fn a_raw_block_on_an_edited_table_conflicts() {
    let raw = one(
        "mod-b",
        "  - kind: raw\n    payload: src/x.bin\n    target_layer: data\n    touches: [english]\n",
    );
    exclusive_conflict(&one("mod-a", EDIT_ENGLISH), &raw);
}

// ---------------------------------------------------------------------------
// Sound cues
// ---------------------------------------------------------------------------

fn sound(shipment: &str, contribution: &str) -> Manifest {
    parse(&format!(
        "format: 2\nshipment: {{ name: {shipment}, version: 1.0.0, target: retail }}\ncontributions:\n{contribution}"
    ))
}

fn cue_yaml(indent: &str, name: &str) -> String {
    format!(
        "{indent}name: {name}\n{indent}wave: src/a.wav\n{indent}group_gain_db: 0\n{indent}cue_gain_db: 0\n\
         {indent}pitch_semitones: 0\n{indent}positional: false\n{indent}min_distance: 1\n{indent}max_distance: 2\n\
         {indent}distance_exponent: 1\n{indent}doppler_scale: 1\n{indent}start_limit: 0\n{indent}sound_id: 0\n\
         {indent}priority: 1\n{indent}group_20: 1\n{indent}cue_16: 0\n{indent}clip_hash: 0\n"
    )
}

fn replace_cue(shipment: &str, bank: &str, language: Option<&str>, cue: &str) -> Manifest {
    let language = language.map(|l| format!("    language: {l}\n")).unwrap_or_default();
    sound(
        shipment,
        &format!("  - kind: replace_sound_cue\n    bank: {bank}\n{language}    category: ui\n    cue:\n{}", cue_yaml("      ", cue)),
    )
}

fn add_cue(shipment: &str, bank: &str, cue: &str) -> Manifest {
    add_cue_in(shipment, bank, cue, "[gameplay]")
}

fn add_cue_in(shipment: &str, bank: &str, cue: &str, load_in: &str) -> Manifest {
    let f = cue_yaml("        ", cue);
    sound(
        shipment,
        &format!("  - kind: add_sound\n    bank: {bank}\n    category: ui\n    load_in: {load_in}\n    cues:\n      - {}", &f[8..]),
    )
}

/// The script claims of `m`, as `(name, level, class)`.
fn script_claims(m: &Manifest) -> Vec<(String, Level, MergeClass)> {
    blast::claims(m)
        .into_iter()
        .filter_map(|r| match r.claim {
            Claim::Script { name, level } => Some((name, level, r.class)),
            _ => None,
        })
        .collect()
}

/// A sound kind claims the scripts of each session's loader, `Additive`: gameplay's trampoline
/// hosts in `vz.wad`, the front end's loader and host in `shell.wad`. An override claims the
/// sessions retail loads its bank in: `ui_hud` both, `ui_shell` the front end, a `vo_*` bank
/// gameplay.
#[test]
fn sound_kinds_claim_their_loaders_scripts() {
    let gameplay = vec![
        ("wifpmcinterior".to_string(), Level::Vz, MergeClass::OrderedList),
        ("mrxsoundbootstrap".to_string(), Level::Vz, MergeClass::OrderedList),
    ];
    let front = vec![
        ("qm_shell_modloader".to_string(), Level::Shell, MergeClass::OrderedList),
        ("mrxsound".to_string(), Level::Shell, MergeClass::OrderedList),
    ];
    let both: Vec<_> = gameplay.iter().chain(&front).cloned().collect();
    assert_eq!(script_claims(&add_cue_in("a", "b", "c", "[gameplay]")), gameplay);
    assert_eq!(script_claims(&add_cue_in("a", "b", "c", "[front_end]")), front);
    assert_eq!(script_claims(&add_cue_in("a", "b", "c", "[gameplay, front_end]")), both);
    assert_eq!(script_claims(&replace_cue("a", "ui_hud", None, "x")), both);
    assert_eq!(script_claims(&replace_cue("a", "ui_shell", None, "x")), front);
    assert_eq!(script_claims(&replace_cue("a", "vo_mattias", Some("english"), "x")), gameplay);
    // Two sound Shipments share the loaders.
    let a = replace_cue("a", "ui_hud", None, "one");
    let b = add_cue_in("b", "bank_b", "two", "[gameplay, front_end]");
    assert!(blast::conflicts(&[("a", &a), ("b", &b)]).is_empty());
    // The front end's `mrxsound` is not gameplay's.
    assert_ne!(
        Claim::Script { name: "mrxsound".into(), level: Level::Shell },
        Claim::Script { name: "mrxsound".into(), level: Level::Vz }
    );
}

/// `replace_lua wifpmcinterior` beside a sound override that gameplay loads is a conflict: the
/// replacement removes the trampoline host the loader is reached through. Beside a front-end-only
/// override it is not.
#[test]
fn replace_lua_of_the_loader_host_conflicts_with_a_gameplay_sound_override() {
    let replace = one("mod-a", "  - kind: replace_lua\n    target: wifpmcinterior\n    source: src/w.lua\n");
    let found = exclusive_conflict(&replace, &replace_cue("mod-b", "ui_hud", None, "ui_PDA_Accept"));
    assert_eq!(found.claim, Claim::Script { name: "wifpmcinterior".into(), level: Level::Vz });
    assert!(blast::conflicts(&[("mod-a", &replace), ("mod-b", &replace_cue("mod-b", "ui_shell", None, "x"))]).is_empty());
    // One Shipment doing both is a self-conflict.
    let both = one(
        "mod-a",
        &format!(
            "  - kind: replace_lua\n    target: wifpmcinterior\n    source: src/w.lua\n  - kind: replace_sound_cue\n    bank: ui_hud\n    category: ui\n    cue:\n{}",
            cue_yaml("      ", "ui_PDA_Accept")
        ),
    );
    assert!(
        blast::self_conflicts(&both).iter().any(|c| c.claim == Claim::Script { name: "wifpmcinterior".into(), level: Level::Vz }),
        "{:?}",
        blast::self_conflicts(&both)
    );
}

/// Two Shipments overriding different cues of one bank compose — the link merges them — while one
/// cue replaced twice is a conflict no load order resolves.
#[test]
fn cue_overrides_of_one_bank_compose_and_one_cue_twice_conflicts() {
    let a = replace_cue("a", "ui_hud", None, "ui_PDA_Open_01_st");
    let b = replace_cue("b", "ui_hud", None, "ui_PDA_Close_01_st");
    assert!(blast::conflicts(&[("a", &a), ("b", &b)]).is_empty());

    let c = replace_cue("c", "ui_hud", None, "ui_PDA_Open_01_st");
    let found = blast::conflicts(&[("a", &a), ("c", &c)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].class, MergeClass::Exclusive);
    assert!(matches!(found[0].claim, Claim::SoundCue { .. }));
}

/// One voice-over cue replaced in two languages is two claims: each language's bank is its own.
#[test]
fn different_languages_do_not_conflict() {
    let en = replace_cue("en", "vo_mattias", Some("english"), "mattias_line");
    let fr = replace_cue("fr", "vo_mattias", Some("french"), "mattias_line");
    assert!(blast::conflicts(&[("en", &en), ("fr", &fr)]).is_empty());
    let en2 = replace_cue("en2", "vo_mattias", Some("english"), "mattias_line");
    assert_eq!(blast::conflicts(&[("en", &en), ("en2", &en2)]).len(), 1);
}

/// An added cue name is a key across the set (KeyedSet): two banks adding one cue name collide, and
/// an added cue beside an override of the same cue takes the stricter class.
#[test]
fn added_cue_names_are_keys_and_meet_overrides_exclusively() {
    let a = add_cue("a", "bank_a", "mod_click");
    let b = add_cue("b", "bank_b", "mod_click");
    let found = blast::conflicts(&[("a", &a), ("b", &b)]);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].class, MergeClass::KeyedSet);
    assert!(blast::conflicts(&[("a", &a), ("c", &add_cue("c", "bank_c", "mod_other"))]).is_empty());

    let over = replace_cue("o", "ui_hud", None, "mod_click");
    let found = blast::conflicts(&[("a", &a), ("o", &over)]);
    assert_eq!(found[0].class, MergeClass::Exclusive);
}

/// A replaced bank is claimed whole by its entry: two replacements of one bank conflict, and the
/// claim is on `<bank>.<language>` for a voice-over bank.
#[test]
fn a_bank_replaced_twice_conflicts() {
    let bank = |s: &str, language: &str, cue: &str| {
        let f = cue_yaml("        ", cue);
        sound(
            s,
            &format!(
                "  - kind: replace_sound_bank\n    bank: vo_mattias\n    language: {language}\n    category: vo\n    cues:\n      - {}",
                &f[8..]
            ),
        )
    };
    let a = bank("a", "english", "line_a");
    let b = bank("b", "english", "line_b");
    let found = blast::conflicts(&[("a", &a), ("b", &b)]);
    let entry = mercs2_formats::hash::pandemic_hash_m2("vo_mattias.english");
    assert!(found.iter().any(|c| c.claim == Claim::Asset { hash: entry } && c.class == MergeClass::Exclusive), "{found:?}");
    assert!(blast::conflicts(&[("a", &a), ("f", &bank("f", "french", "line_b"))]).is_empty());
}
