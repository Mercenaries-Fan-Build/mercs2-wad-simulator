//! Composition: does the merge model actually let real mods coexist, and does it catch the cases
//! that silently break the game?
//!
//! These are the tests that make the composition model more than a design note. All hermetic — no
//! game install — because merge semantics are a property of the base game we have already reversed,
//! not something we need the WADs to re-derive.

use mercs2_quartermaster::blast::{self, Access, Claim, MergeClass};
use mercs2_quartermaster::{from_str, Format, Manifest};

fn parse(yaml: &str) -> Manifest {
    from_str(yaml, Format::Yaml).unwrap_or_else(|e| panic!("fixture must parse: {e}\n{yaml}"))
}

/// A wardrobe mod: one outfit for one hero.
fn outfit(shipment: &str, asset: &str, wearer: &str, slug: &str) -> Manifest {
    parse(&format!(
        "format: 1
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
        "format: 1
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
        .find(|r| matches!(&r.claim, Claim::Script { name } if name == "wifpmcinterior"))
        .expect("add_outfit must claim the wardrobe script");
    assert_eq!(script_claim.class, MergeClass::OrderedList);
}

// ---------------------------------------------------------------------------
// Fail closed.
// ---------------------------------------------------------------------------

/// A script whose composition we have NOT reversed falls to Exclusive. Being wrong here costs a
/// false conflict (visible, annoying) instead of silent mutual annihilation (invisible, fatal).
#[test]
fn an_unreversed_script_is_exclusive() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 1
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
    let found = blast::conflicts(&[("mod-a", &a), ("mod-b", &b)]);
    assert_eq!(found.len(), 1, "got {found:?}");
    assert_eq!(found[0].class, MergeClass::Exclusive);
    assert!(
        found[0].to_string().contains("no load order resolves"),
        "{}",
        found[0]
    );
}

/// Raw is the open lower bound: we cannot infer anything about the bytes, so the declared blast
/// radius is trusted and the class fails closed.
#[test]
fn two_raw_contributions_touching_one_target_collide() {
    let mk = |name: &str| {
        parse(&format!(
            "format: 1
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
            "format: 1
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
            "format: 1
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
        found
            .iter()
            .any(|c| matches!(&c.claim, Claim::FileArtifact { name } if name == "bridge.asi")),
        "same .asi filename must collide, got {found:?}"
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
            "format: 1
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
        "format: 1
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
            "format: 1
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
        "format: 1
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
        "format: 1
shipment: { name: consumer, version: 1.0.0, target: retail }
contributions:
  - kind: add_model
    name: derived_thing
    model: src/m.glb
    donor: provided_thing
",
    );
    let b = parse(
        "format: 1
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
        "format: 1
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
        "format: 1
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
        "format: 1
shipment: { name: a, version: 1.0.0, target: retail }
contributions:
  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"al_veh_boat_destroyer\"]
",
    );
    let by_hash = parse(
        "format: 1
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
