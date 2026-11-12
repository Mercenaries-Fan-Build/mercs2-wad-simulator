//! The outfit-list generator against the retail vz wardrobe and player scripts.
//!
//! Game-gated: built by the `retail` feature (`cargo xtask retail-test`), reads the retail vz.wad
//! and the Blow It Up Again Shipment (`biua_shipment_dir`, the dlc01 level's PC-form scripts) named by
//! the repo-root `.mercs2-local.toml`, and fails if either is absent.

use std::path::Path;

use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::types::TYPE_HASH_SCRIPT;
use mercs2_quartermaster::outfits::{
    generate, luaq_of, read_vz, AddedOutfit, Identity, Listing, Restore, RetailLevel, PLAYER_SCRIPT, WARDROBE_SCRIPT,
};

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn vz() -> RetailLevel {
    let wad = mercs2_formats::game_paths::local_config_vz_wad(start()).unwrap_or_else(|e| panic!("{e}"));
    read_vz(&wad).unwrap_or_else(|e| panic!("{e}"))
}

/// The dlc01 level's lists, from the wardrobe and player scripts the BIUA Shipment's dlc01 rows ship
/// (the DLC's `dlc01_pmcinterior` and `dlc01_player`, under the base scripts' asset names).
fn dlc01() -> RetailLevel {
    let dir = mercs2_formats::game_paths::local_config_biua_shipment_dir(start()).unwrap_or_else(|e| panic!("{e}"));
    let s = mercs2_quartermaster::discover::open(&dir).unwrap_or_else(|e| panic!("{}: {e:?}", dir.display()));
    let assets = mercs2_quartermaster::level::assets_of(&s.manifest, "dlc01");
    let chunk = |name: &str| {
        let a = assets
            .iter()
            .find(|a| a.key() == Ok((m2(name), TYPE_HASH_SCRIPT)))
            .unwrap_or_else(|| panic!("the BIUA Shipment's dlc01 rows ship no {name} script"));
        let [rung] = a.rungs else { panic!("{name}: {} rungs, a script has one", a.rungs.len()) };
        let path = s.root.join(&rung.file);
        luaq_of(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    };
    RetailLevel::read(&chunk(WARDROBE_SCRIPT), &chunk(PLAYER_SCRIPT)).unwrap_or_else(|e| panic!("{e}"))
}

fn names(r: &RetailLevel, hero: &str) -> Vec<String> {
    r.wardrobe[hero].iter().map(|o| o.name.clone()).collect()
}

/// The DLC's own lists: its wardrobe adds Ewan, Misha, PirateBoss, Blanco, Fiona, Eva, Obama, Palin
/// and GrandpaMattias, and its map lines them up with the rows, leftovers last.
#[test]
fn the_dlc01_lists_read_from_the_shipped_bytecode() {
    let r = dlc01();
    assert_eq!(
        names(&r, "mattias"),
        ["Original", "MetalHead", "Suit", "Jacket", "ChickenSuit", "Ewan", "Misha", "Obama", "Palin", "GrandpaMattias"]
    );
    assert_eq!(
        names(&r, "chris"),
        ["Original", "Vacation", "Commando", "OffDuty", "ChickenSuit", "PirateBoss", "Blanco", "Obama", "Palin"]
    );
    assert_eq!(
        names(&r, "jennifer"),
        ["Original", "Rebel", "Tactical", "NoJacket", "CatSuit", "ChickenSuit", "Fiona", "Eva", "Obama", "Palin"]
    );
    assert_eq!(
        models(r.restore_only("mattias")),
        ["gr_hum_starter_1", "pmc_hum_stealth", "pmc_hum_hoang", "gr_hum_boss_fake", "oc_hum_pilot"]
    );
    assert_eq!(models(r.restore_only("chris")), Vec::<&str>::new());
    assert_eq!(models(r.restore_only("jennifer")), ["pmc_hum_diablo"]);
}

/// BIUA's skins, scoped to vz or naming no level, leave dlc01's lists as the DLC ships them; named
/// into dlc01, the DLC's own Obama row refuses the duplicate.
#[test]
fn vz_scoped_outfits_leave_the_dlc01_lists_alone() {
    let r = dlc01();
    let mut obama = AddedOutfit {
        shipment: "blow-it-up-again".into(),
        slug: "Obama".into(),
        model: "pmc_hum_obama".into(),
        display: "[SHELL.Costumes.obama]".into(),
        wearers: vec!["chris".into(), "jen".into(), "mattias".into()],
        levels: Some(vec!["vz".into()]),
        chatter: Some("Obama_Obama01".into()),
    };
    for levels in [Some(vec!["vz".to_string()]), None] {
        obama.levels = levels;
        let l = generate("dlc01", &r, std::slice::from_ref(&obama)).unwrap_or_else(|e| panic!("{e}"));
        for h in &l.heroes {
            assert_eq!(h.rows.len(), r.wardrobe[&h.hero].len(), "{}", h.hero);
            let (_, retail) = r.map.iter().find(|(k, _)| k == &h.hero).unwrap();
            assert_eq!(h.map, *retail, "{}: the generated map is the DLC's own", h.hero);
        }
    }
    obama.levels = Some(vec!["dlc01".into()]);
    let e = generate("dlc01", &r, &[obama]).unwrap_err();
    assert!(e.contains("already has a retail outfit named \"Obama\" in dlc01"), "{e}");
}

/// The end-game fixture saves costume byte 3 (header `0x4F`), and the join restores it as the
/// Jacket: the outfit the memory capture of that save loaded shows (`mercs2_probe` test
/// `dump::tests::costume_is_singleton_0x63_and_header_0x4f`).
#[test]
fn the_fixture_costume_restores_as_the_jacket() {
    let path = mercs2_formats::game_paths::save_fixtures().join("Mattias Nilsson_6A0E523C.profile");
    let p = mercs2_formats::save::parse(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!((p.character_index, p.costume_index), (1, 3));
    let r = vz();
    assert_eq!(r.restore(p.character_index, p.costume_index), Some(Restore::Model("pmc_hum_mattias_v4")));
    assert_eq!(r.wardrobe["mattias"][3].name, "Jacket");
}

fn models(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// The shipped lists, as `wifpmcinterior.lua:155-242` and `mrxplayer.lua:20-80` hold them.
#[test]
fn the_retail_vz_lists_read_from_the_shipped_bytecode() {
    let r = vz();
    let rows = |h: &str| r.wardrobe[h].iter().map(|o| o.name.as_str()).collect::<Vec<_>>();
    assert_eq!(rows("mattias"), ["Original", "MetalHead", "Suit", "Jacket", "ChickenSuit"]);
    assert_eq!(rows("chris"), ["Original", "Vacation", "Commando", "OffDuty", "ChickenSuit"]);
    assert_eq!(rows("jennifer"), ["Original", "Rebel", "Tactical", "NoJacket", "CatSuit", "ChickenSuit"]);
    let heroes: Vec<&str> = r.map.iter().map(|(h, _)| h.as_str()).collect();
    assert_eq!(heroes, ["mattias", "chris", "jennifer"]);
    let bases: Vec<&str> = r.map.iter().map(|(_, e)| e.base.as_str()).collect();
    assert_eq!(bases, ["mattias", "chris", "jen"]);
    assert_eq!(models(&r.map[0].1.templates), ["mattiasupgrade1", "mattiasupgrade2", "mattiasupgrade3"]);
    assert_eq!(
        models(r.restore_only("mattias")),
        [
            "gr_hum_starter_1",
            "pmc_hum_helipilot_unlockable",
            "pmc_hum_stealth",
            "pmc_hum_hoang",
            "pmc_hum_proppilot_unlockable",
            "gr_hum_boss_fake",
            "oc_hum_pilot"
        ]
    );
    assert_eq!(models(r.restore_only("chris")), ["pr_hum_starter05", "pmc_hum_blanco"]);
    assert_eq!(models(r.restore_only("jennifer")), ["pmc_hum_diablo", "pmc_hum_mechanic"]);
}

/// Two universal outfits and one hero-specific one: retail rows unchanged, added rows after them in
/// the order given, map entry `k` = row `k + 1`, the retail restore-only models after the added ones.
#[test]
fn added_outfits_follow_the_retail_vz_rows() {
    let r = vz();
    let add = |slug: &str, model: &str, wearers: &[&str]| AddedOutfit {
        shipment: "blow-it-up-again".into(),
        slug: slug.into(),
        model: model.into(),
        display: format!("[{slug}]"),
        wearers: wearers.iter().map(|w| w.to_string()).collect(),
        levels: Some(vec!["vz".into()]),
        chatter: None,
    };
    let all = ["chris", "jen", "mattias"];
    let set = [
        add("Obama", "pmc_hum_obama", &all),
        add("Palin", "pmc_hum_sarah", &all),
        add("GrandpaMattias", "pmc_hum_mattias_v5", &["mattias"]),
    ];
    let l = generate("vz", &r, &set).unwrap_or_else(|e| panic!("{e}"));
    for h in &l.heroes {
        let retail = &r.wardrobe[&h.hero];
        for (row, o) in h.rows.iter().zip(retail) {
            assert_eq!(row.identity, Identity::Retail { name: o.name.clone() });
            assert_eq!((row.model.as_str(), row.listing), (o.model.as_str(), Listing::Retail));
        }
        for row in &h.rows[retail.len()..] {
            assert_eq!(row.listing, Listing::Always);
        }
        for k in 1..h.rows.len() {
            assert_eq!(h.map.models[k - 1], h.rows[k].model, "{}: map entry {k} is row {}", h.hero, k + 1);
        }
        assert_eq!(&h.map.models[h.rows.len() - 1..], r.restore_only(&h.hero), "{}", h.hero);
    }
    let m = &l.heroes[0];
    assert_eq!(m.hero, "mattias");
    assert_eq!(
        m.rows[5..].iter().map(|r| r.model.as_str()).collect::<Vec<_>>(),
        ["pmc_hum_obama", "pmc_hum_sarah", "pmc_hum_mattias_v5"]
    );
    assert_eq!(l.heroes[2].rows.len(), 8, "jennifer: 6 retail + Obama + Palin");

    let again = generate("vz", &vz(), &set).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(format!("{l:?}"), format!("{again:?}"), "the same set gives the same lists");
}
