//! Cross-format conformance: the SAME logical manifest, written three ways, must deserialize to
//! one identical model.
//!
//! This is the direct test of the format's central claim ("ONE serde model parses all three") and
//! it exists to settle the open schema risk early: `#[serde(tag = "kind")]` internally-tagged enums
//! are clean in serde_json/yaml, but the `toml` crate has historically had limited support for
//! them. If TOML cannot carry the tag, that is a FORMAT change, not an implementation detail.
//!
//! The fixtures below are the spec's own worked examples (Plan 04 "Conformance fixtures"), so a
//! change to the spec that does not update these is an incomplete change.

use mercs2_quartermaster::{from_str, manifest::*, Format};

/// Fixture A+B+C combined — every v1 kind in one document, so the tagged enum is exercised for all
/// of them in every format.
const YAML: &str = r#"
format: 1

shipment:
  name: sean-devlin-outfit
  title: Sean Devlin Outfit
  version: 1.0.0
  authors: ["you <you@example.com>"]
  description: Adds Sean Devlin as a wearable outfit for Mattias.
  target: retail
  quartermaster: ">=0.1"

load:
  after: []
  before: []
  requires:
    - some-other-shipment
    - url: https://github.com/loganw234/mercs2-lua-mods/releases/download/v1/bridge.asi
      sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
  conflicts: []

contributions:
  - kind: add_outfit
    name: sean_devlin
    slug: SeanDevlin
    display: Sean Devlin
    wearer: mattias
    model: src/sean/sean.glb
    donor: pmc_hum_mattias
    textures:
      diffuse: src/sean/sean_d.png
      normal: src/sean/sean_n.png

  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/boss/new_boss_ub.png

  - kind: add_model
    name: my_custom_helipad
    model: src/helipad/pad.glb
    group: 3
    textures:
      diffuse: src/helipad/pad_d.png

  - kind: add_texture
    name: my_custom_decal
    image: src/decals/decal.png
    normal_map: false

  - kind: add_sound
    name: amb_myjungle
    bank: src/audio/myjungle.bnk
    sound: soundbank

  - kind: edit_state_machine
    target: al_veh_boat_destroyer
    states: src/destroyer/states.yaml

  - kind: add_movie
    name: my_hud_widgets
    movie: src/ui/widgets.gfx

  - kind: patch_lua
    target: wifpmcinterior
    append: src/scripts/my_append.lua

  - kind: native_hook
    target: retail
    plugin: src/native/mybridge.asi
    touches: ["0x004CF340"]

  - kind: place_file
    file: src/native/mybridge.ini
    dest: scripts

  - kind: raw
    description: hand-tuned destruction states for the destroyer
    payload: src/destroyer_states.block
    target_layer: data
    touches: ["al_veh_boat_destroyer"]
"#;

const JSON: &str = r#"
{
  "format": 1,
  "shipment": {
    "name": "sean-devlin-outfit",
    "title": "Sean Devlin Outfit",
    "version": "1.0.0",
    "authors": ["you <you@example.com>"],
    "description": "Adds Sean Devlin as a wearable outfit for Mattias.",
    "target": "retail",
    "quartermaster": ">=0.1"
  },
  "load": {
    "after": [],
    "before": [],
    "requires": [
      "some-other-shipment",
      {
        "url": "https://github.com/loganw234/mercs2-lua-mods/releases/download/v1/bridge.asi",
        "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
      }
    ],
    "conflicts": []
  },
  "contributions": [
    {
      "kind": "add_outfit",
      "name": "sean_devlin",
      "slug": "SeanDevlin",
      "display": "Sean Devlin",
      "wearer": "mattias",
      "model": "src/sean/sean.glb",
      "donor": "pmc_hum_mattias",
      "textures": {
        "diffuse": "src/sean/sean_d.png",
        "normal": "src/sean/sean_n.png"
      }
    },
    {
      "kind": "replace_texture",
      "target": "al_hum_boss_ub",
      "image": "src/boss/new_boss_ub.png"
    },
    {
      "kind": "add_model",
      "name": "my_custom_helipad",
      "model": "src/helipad/pad.glb",
      "group": 3,
      "textures": { "diffuse": "src/helipad/pad_d.png" }
    },
    {
      "kind": "add_texture",
      "name": "my_custom_decal",
      "image": "src/decals/decal.png",
      "normal_map": false
    },
    {
      "kind": "add_sound",
      "name": "amb_myjungle",
      "bank": "src/audio/myjungle.bnk",
      "sound": "soundbank"
    },
    {
      "kind": "edit_state_machine",
      "target": "al_veh_boat_destroyer",
      "states": "src/destroyer/states.yaml"
    },
    {
      "kind": "add_movie",
      "name": "my_hud_widgets",
      "movie": "src/ui/widgets.gfx"
    },
    {
      "kind": "patch_lua",
      "target": "wifpmcinterior",
      "append": "src/scripts/my_append.lua"
    },
    {
      "kind": "native_hook",
      "target": "retail",
      "plugin": "src/native/mybridge.asi",
      "touches": ["0x004CF340"]
    },
    {
      "kind": "place_file",
      "file": "src/native/mybridge.ini",
      "dest": "scripts"
    },
    {
      "kind": "raw",
      "description": "hand-tuned destruction states for the destroyer",
      "payload": "src/destroyer_states.block",
      "target_layer": "data",
      "touches": ["al_veh_boat_destroyer"]
    }
  ]
}
"#;

// NOTE the TOML shape: every scalar key of a `[[contributions]]` element must precede any of its
// sub-tables (`[contributions.textures]`), or TOML reports a value-after-table error. That is a
// property of the FORMAT, not of our schema.
const TOML: &str = r#"
format = 1

[shipment]
name = "sean-devlin-outfit"
title = "Sean Devlin Outfit"
version = "1.0.0"
authors = ["you <you@example.com>"]
description = "Adds Sean Devlin as a wearable outfit for Mattias."
target = "retail"
quartermaster = ">=0.1"

[load]
after = []
before = []
conflicts = []
requires = [
  "some-other-shipment",
  { url = "https://github.com/loganw234/mercs2-lua-mods/releases/download/v1/bridge.asi", sha256 = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" },
]

[[contributions]]
kind = "add_outfit"
name = "sean_devlin"
slug = "SeanDevlin"
display = "Sean Devlin"
wearer = "mattias"
model = "src/sean/sean.glb"
donor = "pmc_hum_mattias"

[contributions.textures]
diffuse = "src/sean/sean_d.png"
normal = "src/sean/sean_n.png"

[[contributions]]
kind = "replace_texture"
target = "al_hum_boss_ub"
image = "src/boss/new_boss_ub.png"

[[contributions]]
kind = "add_model"
name = "my_custom_helipad"
model = "src/helipad/pad.glb"
group = 3
textures = { diffuse = "src/helipad/pad_d.png" }

[[contributions]]
kind = "add_texture"
name = "my_custom_decal"
image = "src/decals/decal.png"
normal_map = false

[[contributions]]
kind = "add_sound"
name = "amb_myjungle"
bank = "src/audio/myjungle.bnk"
sound = "soundbank"

[[contributions]]
kind = "edit_state_machine"
target = "al_veh_boat_destroyer"
states = "src/destroyer/states.yaml"

[[contributions]]
kind = "add_movie"
name = "my_hud_widgets"
movie = "src/ui/widgets.gfx"

[[contributions]]
kind = "patch_lua"
target = "wifpmcinterior"
append = "src/scripts/my_append.lua"

[[contributions]]
kind = "native_hook"
target = "retail"
plugin = "src/native/mybridge.asi"
touches = ["0x004CF340"]

[[contributions]]
kind = "place_file"
file = "src/native/mybridge.ini"
dest = "scripts"

[[contributions]]
kind = "raw"
description = "hand-tuned destruction states for the destroyer"
payload = "src/destroyer_states.block"
target_layer = "data"
touches = ["al_veh_boat_destroyer"]
"#;

#[test]
fn yaml_json_and_toml_agree() {
    let y = from_str(YAML, Format::Yaml).expect("YAML must parse");
    let j = from_str(JSON, Format::Json).expect("JSON must parse");
    let t = from_str(TOML, Format::Toml).expect("TOML must parse");

    assert_eq!(y, j, "YAML and JSON disagree");
    assert_eq!(y, t, "YAML and TOML disagree");
}

/// The specific risk: the internally-tagged `kind` must survive TOML's array-of-tables.
#[test]
fn toml_carries_the_kind_tag_for_every_v1_kind() {
    let m = from_str(TOML, Format::Toml).expect("TOML must parse");
    let kinds: Vec<&str> = m.contributions.iter().map(|c| c.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            "add_outfit",
            "replace_texture",
            "add_model",
            "add_texture",
            "add_sound",
            "edit_state_machine",
            "add_movie",
            "patch_lua",
            "native_hook",
            "place_file",
            "raw"
        ]
    );
}

/// `dest:` is the first field in the format whose value set is CLOSED, so it is the first place the
/// three serializations could disagree about how an author spells one. All three must map the same
/// snake_case name onto the same destination — a format that read `on_boot` as anything else would
/// place a file in a different directory depending on which extension the manifest happened to use.
#[test]
fn every_destination_spells_the_same_in_all_three_formats() {
    for (yaml_name, expected) in [
        ("game_root", PlaceIn::GameRoot),
        ("scripts", PlaceIn::Scripts),
        ("plugins", PlaceIn::Plugins),
        ("update", PlaceIn::Update),
        ("on_boot", PlaceIn::OnBoot),
        ("on_load", PlaceIn::OnLoad),
        ("on_key", PlaceIn::OnKey),
    ] {
        let head = "\"format\":1,\"shipment\":{\"name\":\"s\",\"version\":\"1.0.0\",\"target\":\"retail\"}";
        let cases = [
            (
                format!(
                    "format: 1\nshipment: {{ name: s, version: 1.0.0, target: retail }}\n\
                     contributions:\n  - kind: place_file\n    file: src/x.ini\n    dest: {yaml_name}\n"
                ),
                Format::Yaml,
            ),
            (
                format!(
                    "{{{head},\"contributions\":[{{\"kind\":\"place_file\",\
                     \"file\":\"src/x.ini\",\"dest\":\"{yaml_name}\"}}]}}"
                ),
                Format::Json,
            ),
            (
                format!(
                    "format = 1\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
                     [[contributions]]\nkind = \"place_file\"\nfile = \"src/x.ini\"\n\
                     dest = \"{yaml_name}\"\n"
                ),
                Format::Toml,
            ),
        ];
        for (text, fmt) in cases {
            let m = from_str(&text, fmt).unwrap_or_else(|e| panic!("{fmt:?} {yaml_name}: {e}"));
            match &m.contributions[0] {
                Contribution::PlaceFile { dest, .. } => {
                    assert_eq!(*dest, expected, "{fmt:?} {yaml_name}")
                }
                other => panic!("{fmt:?}: expected place_file, got {other:?}"),
            }
        }
    }
}

/// A destination is a NAME out of a closed set, so anything path-shaped is not rejected — it does
/// not parse. Checked in all three formats, because "unreachable by construction" is a property of
/// the SCHEMA and would be worth nothing if one serializer were laxer than the others.
#[test]
fn a_path_shaped_destination_parses_in_no_format() {
    for attempt in ["..", "../..", "/etc", "C:\\\\Windows", "data", "scripts/.."] {
        let head =
            "\"format\":1,\"shipment\":{\"name\":\"s\",\"version\":\"1.0.0\",\"target\":\"retail\"}";
        let cases = [
            (
                format!(
                    "format: 1\nshipment: {{ name: s, version: 1.0.0, target: retail }}\n\
                     contributions:\n  - kind: place_file\n    file: src/x.ini\n    dest: '{attempt}'\n"
                ),
                Format::Yaml,
            ),
            (
                format!(
                    "{{{head},\"contributions\":[{{\"kind\":\"place_file\",\
                     \"file\":\"src/x.ini\",\"dest\":\"{attempt}\"}}]}}"
                ),
                Format::Json,
            ),
            (
                format!(
                    "format = 1\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
                     [[contributions]]\nkind = \"place_file\"\nfile = \"src/x.ini\"\n\
                     dest = \"{attempt}\"\n"
                ),
                Format::Toml,
            ),
        ];
        for (text, fmt) in cases {
            assert!(
                from_str(&text, fmt).is_err(),
                "{fmt:?}: dest {attempt:?} must not parse"
            );
        }
    }
}

/// The `requires` dual form (bare name | pinned external artifact) is an UNTAGGED enum — the other
/// serde feature that formats disagree about. Exercise it everywhere too.
#[test]
fn requires_dual_form_agrees_across_formats() {
    for (text, fmt) in [
        (YAML, Format::Yaml),
        (JSON, Format::Json),
        (TOML, Format::Toml),
    ] {
        let m = from_str(text, fmt).unwrap_or_else(|e| panic!("{fmt:?}: {e}"));
        assert_eq!(m.load.requires.len(), 2, "{fmt:?}");
        assert!(
            matches!(&m.load.requires[0], Requirement::Shipment(s) if s == "some-other-shipment"),
            "{fmt:?}: first requirement should be a bare shipment name"
        );
        match &m.load.requires[1] {
            Requirement::External { url, sha256 } => {
                assert!(url.starts_with("https://"), "{fmt:?}");
                assert_eq!(sha256.len(), 64, "{fmt:?}: expected a hex sha256");
            }
            other => panic!("{fmt:?}: expected an external requirement, got {other:?}"),
        }
    }
}

/// Ordering is load-bearing: the list preserves cross-kind apply order within a Shipment.
#[test]
fn contribution_order_is_preserved() {
    let m = from_str(YAML, Format::Yaml).unwrap();
    assert_eq!(
        m.contributions.first().map(|c| c.kind()),
        Some("add_outfit")
    );
    assert_eq!(m.contributions.last().map(|c| c.kind()), Some("raw"));
}

/// A manifest the Quartermaster WROTE must read back identically.
#[test]
fn yaml_round_trips() {
    let original = from_str(YAML, Format::Yaml).unwrap();
    let emitted = mercs2_quartermaster::to_yaml(&original).expect("emit YAML");
    let reparsed = from_str(&emitted, Format::Yaml)
        .unwrap_or_else(|e| panic!("re-reading emitted YAML failed: {e}\n---\n{emitted}"));
    assert_eq!(original, reparsed);
}

#[test]
fn extension_detection() {
    assert_eq!(Format::from_extension("yaml"), Some(Format::Yaml));
    assert_eq!(Format::from_extension("yml"), Some(Format::Yaml));
    assert_eq!(Format::from_extension("YAML"), Some(Format::Yaml));
    assert_eq!(Format::from_extension("json"), Some(Format::Json));
    assert_eq!(Format::from_extension("toml"), Some(Format::Toml));
    assert_eq!(Format::from_extension("txt"), None);
}

// ---------------------------------------------------------------------------
// Validation — every one of these must FAIL, and fail by name.
// ---------------------------------------------------------------------------

fn minimal(target: &str, name: &str, format: u32) -> String {
    format!(
        "format: {format}\nshipment:\n  name: {name}\n  version: 1.0.0\n  target: {target}\ncontributions: []\n"
    )
}

/// Direction matters: NEWER than known is the reject; older is accepted.
#[test]
fn a_future_format_version_is_loudly_rejected() {
    let err = from_str(
        &minimal("retail", "ok-name", FORMAT_VERSION + 1),
        Format::Yaml,
    )
    .expect_err("a future format must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to guess"),
        "unhelpful message: {msg}"
    );
}

#[test]
fn the_current_format_version_is_accepted() {
    from_str(&minimal("retail", "ok-name", FORMAT_VERSION), Format::Yaml).expect("current format");
}

#[test]
fn target_both_is_rejected_by_name() {
    let err = from_str(&minimal("both", "ok-name", 1), Format::Yaml)
        .expect_err("target: both is reserved in v1");
    let msg = err.to_string();
    assert!(
        msg.contains("reserved"),
        "should explain, not just fail: {msg}"
    );
}

#[test]
fn shipment_name_must_be_a_slug() {
    for bad in [
        "Sean_Devlin",
        "sean devlin",
        "-leading",
        "trailing-",
        "double--hyphen",
        "",
    ] {
        assert!(
            from_str(&minimal("retail", &format!("{bad:?}"), 1), Format::Yaml).is_err(),
            "{bad:?} should not be a valid shipment name"
        );
    }
    for good in ["sean-devlin-outfit", "boss-reskin", "a", "mod123"] {
        from_str(&minimal("retail", good, 1), Format::Yaml)
            .unwrap_or_else(|e| panic!("{good:?} should be valid: {e}"));
    }
}

#[test]
fn an_unknown_contribution_kind_is_rejected() {
    let text = "format: 1\nshipment:\n  name: x\n  version: 1.0.0\n  target: retail\ncontributions:\n  - kind: reticulate_splines\n    foo: bar\n";
    assert!(from_str(text, Format::Yaml).is_err());
}

// ---------------------------------------------------------------------------
// Identity — names, never hashes.
// ---------------------------------------------------------------------------

/// Regression for the drift that was live in the spec draft: it paired `ch_veh_boat_destroyer`
/// with `0xE54047D5`, but that hash belongs to `al_veh_boat_destroyer`. This is exactly why
/// `touches` takes names.
#[test]
fn destroyer_name_hash_vectors() {
    use mercs2_formats::hash::pandemic_hash_m2;
    assert_eq!(pandemic_hash_m2("al_veh_boat_destroyer"), 0xE540_47D5);
    assert_eq!(pandemic_hash_m2("ch_veh_boat_destroyer"), 0x25FE_00A7);
    assert_ne!(
        pandemic_hash_m2("ch_veh_boat_destroyer"),
        0xE540_47D5,
        "the spec draft's example paired these; they are different assets"
    );
}

#[test]
fn bare_hash_touches_are_detectable() {
    assert!(Touch("0xE54047D5".into()).is_bare_hash());
    assert!(Touch("0xe54047d5".into()).is_bare_hash());
    assert!(!Touch("al_veh_boat_destroyer".into()).is_bare_hash());
    // A name that merely looks hexy is still a name.
    assert!(!Touch("deadbeef".into()).is_bare_hash());
    assert!(!Touch("0x".into()).is_bare_hash());
}

/// Every kind the FORMAT knows must appear in the fixtures above.
///
/// The kind list in `toml_carries_the_kind_tag_for_every_v1_kind` is hand-written, so it can only
/// say "the fixture contains what I expected" — it cannot notice a kind that was added to the format
/// and never exercised anywhere. `Contribution::ALL_KINDS` is the authoritative list, and this
/// closes the loop against it.
///
/// `edit_state_machine` is the reason this exists: it parsed, claimed a blast radius and had linter
/// rules, while being absent from the Workshop's add-menu and from every fixture. Nothing failed.
#[test]
fn the_fixtures_exercise_every_kind_the_format_knows() {
    use mercs2_quartermaster::manifest::Contribution;
    let m = from_str(YAML, Format::Yaml).expect("YAML must parse");
    let present: std::collections::BTreeSet<&str> =
        m.contributions.iter().map(|c| c.kind()).collect();
    let missing: Vec<&&str> = Contribution::ALL_KINDS
        .iter()
        .filter(|k| !present.contains(**k))
        .collect();
    assert!(
        missing.is_empty(),
        "kinds in the format with no conformance fixture: {missing:?} — add one to YAML, JSON and \
         TOML, or remove the kind"
    );
}
