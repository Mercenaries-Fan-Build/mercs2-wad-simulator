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

/// Fixture A+B+C combined — every kind in one document, so the tagged enum is exercised for all of
/// them in every format, together with every requirement and conflict form and `supersedes`.
const YAML: &str = r#"
format: 2

shipment:
  name: sean-devlin-outfit
  title: Sean Devlin Outfit
  version: 1.0.0
  authors: ["you <you@example.com>"]
  description: Adds Sean Devlin as a wearable outfit for Mattias.
  target: retail
  quartermaster: ">=0.1"

supersedes:
  - { dest: on_load, file: 1_Sean.lua }

load:
  requires:
    - some-other-shipment
    - { shipment: lua-bridge, version: "^1.0.0" }
    - { capability: widescreen }
  conflicts:
    - old-sean-outfit
    - { shipment: sean-legacy, version: "<2" }
  provides: [sean-outfit]

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

  - kind: edit_world
    layer: vz_state_pmccon004
    edits: src/world.yaml

  - kind: activate_layer
    layer: vz_state_pmccon004_destroyed
    replaces:
      - vz_state_pmccon004_pristine

  - kind: edit_stringdb
    target: english
    strings: src/text/english.txt

  - kind: add_language
    name: polski
    display: Polski
    strings: src/text/polski.txt

  - kind: add_movie
    name: my_hud_widgets
    movie: src/ui/widgets.gfx

  - kind: add_ui
    name: my_hud_overlay
    movie: src/ui/overlay.gfx

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

  - kind: add_shop_item
    id: dlcm1a1
    name: "[vehicle.m1a1]"
    icon: vehicles_tank_m1a2
    type: heavy
    shops: [pmc, gur]
    cash_cost: 250000
    fuel_cost: 75
    max_stock: 4
    unlocked: true
    behaviour: { module: mrxcratedelivery, cargo: "LAVIII (Minigun)", delivery_vehicle: "Mi26 (PMC) (Driver)" }

  - kind: raw
    description: hand-tuned destruction states for the destroyer
    payload: src/destroyer_states.block
    target_layer: data
    touches: ["al_veh_boat_destroyer"]
"#;

const JSON: &str = r#"
{
  "format": 2,
  "shipment": {
    "name": "sean-devlin-outfit",
    "title": "Sean Devlin Outfit",
    "version": "1.0.0",
    "authors": ["you <you@example.com>"],
    "description": "Adds Sean Devlin as a wearable outfit for Mattias.",
    "target": "retail",
    "quartermaster": ">=0.1"
  },
  "supersedes": [
    { "dest": "on_load", "file": "1_Sean.lua" }
  ],
  "load": {
    "requires": [
      "some-other-shipment",
      { "shipment": "lua-bridge", "version": "^1.0.0" },
      { "capability": "widescreen" }
    ],
    "conflicts": [
      "old-sean-outfit",
      { "shipment": "sean-legacy", "version": "<2" }
    ],
    "provides": ["sean-outfit"]
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
      "kind": "edit_world",
      "layer": "vz_state_pmccon004",
      "edits": "src/world.yaml"
    },
    {
      "kind": "activate_layer",
      "layer": "vz_state_pmccon004_destroyed",
      "replaces": ["vz_state_pmccon004_pristine"]
    },
    {
      "kind": "edit_stringdb",
      "target": "english",
      "strings": "src/text/english.txt"
    },
    {
      "kind": "add_language",
      "name": "polski",
      "display": "Polski",
      "strings": "src/text/polski.txt"
    },
    {
      "kind": "add_movie",
      "name": "my_hud_widgets",
      "movie": "src/ui/widgets.gfx"
    },
    {
      "kind": "add_ui",
      "name": "my_hud_overlay",
      "movie": "src/ui/overlay.gfx"
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
      "kind": "add_shop_item",
      "id": "dlcm1a1",
      "name": "[vehicle.m1a1]",
      "icon": "vehicles_tank_m1a2",
      "type": "heavy",
      "shops": ["pmc", "gur"],
      "cash_cost": 250000,
      "fuel_cost": 75,
      "max_stock": 4,
      "unlocked": true,
      "behaviour": { "module": "mrxcratedelivery", "cargo": "LAVIII (Minigun)", "delivery_vehicle": "Mi26 (PMC) (Driver)" }
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
format = 2

[shipment]
name = "sean-devlin-outfit"
title = "Sean Devlin Outfit"
version = "1.0.0"
authors = ["you <you@example.com>"]
description = "Adds Sean Devlin as a wearable outfit for Mattias."
target = "retail"
quartermaster = ">=0.1"

[[supersedes]]
dest = "on_load"
file = "1_Sean.lua"

[load]
requires = [
  "some-other-shipment",
  { shipment = "lua-bridge", version = "^1.0.0" },
  { capability = "widescreen" },
]
conflicts = [
  "old-sean-outfit",
  { shipment = "sean-legacy", version = "<2" },
]
provides = ["sean-outfit"]

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
kind = "edit_world"
layer = "vz_state_pmccon004"
edits = "src/world.yaml"

[[contributions]]
kind = "activate_layer"
layer = "vz_state_pmccon004_destroyed"
replaces = ["vz_state_pmccon004_pristine"]

[[contributions]]
kind = "edit_stringdb"
target = "english"
strings = "src/text/english.txt"

[[contributions]]
kind = "add_language"
name = "polski"
display = "Polski"
strings = "src/text/polski.txt"

[[contributions]]
kind = "add_movie"
name = "my_hud_widgets"
movie = "src/ui/widgets.gfx"

[[contributions]]
kind = "add_ui"
name = "my_hud_overlay"
movie = "src/ui/overlay.gfx"

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
kind = "add_shop_item"
id = "dlcm1a1"
name = "[vehicle.m1a1]"
icon = "vehicles_tank_m1a2"
type = "heavy"
shops = ["pmc", "gur"]
cash_cost = 250000
fuel_cost = 75
max_stock = 4
unlocked = true
behaviour = { module = "mrxcratedelivery", cargo = "LAVIII (Minigun)", delivery_vehicle = "Mi26 (PMC) (Driver)" }

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
            "edit_world",
            "activate_layer",
            "edit_stringdb",
            "add_language",
            "add_movie",
            "add_ui",
            "patch_lua",
            "native_hook",
            "place_file",
            "add_shop_item",
            "raw"
        ]
    );
}

/// `dest:` is the first field in the format whose value set is CLOSED, so it is the first place the
/// `contribution_yaml` (the Workshop's "show me what this does") must emit the SAME kind-tagged
/// block that lands in a manifest — a legible, self-describing preview, not a debug dump. Proven by
/// round-tripping every fixture contribution through it back into a Contribution.
#[test]
fn contribution_yaml_previews_the_exact_manifest_block() {
    use mercs2_quartermaster::manifest::Contribution;
    let m = from_str(YAML, Format::Yaml).expect("YAML parses");
    // Every kind the format knows must produce a legible, kind-tagged preview — no debug noise, no
    // silently-empty block. The fixture exercises all of them (guarded by
    // `the_fixtures_exercise_every_kind_the_format_knows`), so this covers each kind.
    for c in &m.contributions {
        let yaml = mercs2_quartermaster::contribution_yaml(c);
        assert!(
            yaml.contains(&format!("kind: {}", c.kind())),
            "the preview must carry the kind tag: {yaml}"
        );
        assert!(!yaml.contains("cannot serialize"), "the preview must not be an error: {yaml}");
        // It reads as manifest text, not a Rust debug dump: mapping keys, no struct syntax.
        assert!(!yaml.contains('{') && !yaml.contains("Contribution"), "not manifest YAML: {yaml}");
    }
}

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
        let head = "\"format\":2,\"shipment\":{\"name\":\"s\",\"version\":\"1.0.0\",\"target\":\"retail\"}";
        let cases = [
            (
                format!(
                    "format: 2\nshipment: {{ name: s, version: 1.0.0, target: retail }}\n\
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
                    "format = 2\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
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
            "\"format\":2,\"shipment\":{\"name\":\"s\",\"version\":\"1.0.0\",\"target\":\"retail\"}";
        let cases = [
            (
                format!(
                    "format: 2\nshipment: {{ name: s, version: 1.0.0, target: retail }}\n\
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
                    "format = 2\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
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

/// Every requirement form, both conflict forms and `supersedes`: the requirement and conflict
/// enums are UNTAGGED — the serde feature formats disagree about — so each form is checked in all
/// three formats.
#[test]
fn format2_forms_agree_yaml_json_toml() {
    for (text, fmt) in [
        (YAML, Format::Yaml),
        (JSON, Format::Json),
        (TOML, Format::Toml),
    ] {
        let m = from_str(text, fmt).unwrap_or_else(|e| panic!("{fmt:?}: {e}"));
        assert_eq!(
            m.load.requires,
            vec![
                Requirement::Shipment("some-other-shipment".into()),
                Requirement::ShipmentRange(ShipmentReq {
                    shipment: "lua-bridge".into(),
                    version: "^1.0.0".into()
                }),
                Requirement::Capability(CapabilityReq {
                    capability: "widescreen".into()
                }),
            ],
            "{fmt:?}"
        );
        assert_eq!(
            m.load.conflicts,
            vec![
                ConflictDecl::Name("old-sean-outfit".into()),
                ConflictDecl::Range(ShipmentReq {
                    shipment: "sean-legacy".into(),
                    version: "<2".into()
                }),
            ],
            "{fmt:?}"
        );
        assert_eq!(
            m.supersedes,
            vec![Superseded {
                dest: PlaceIn::OnLoad,
                file: "1_Sean.lua".into()
            }],
            "{fmt:?}"
        );
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

/// A document with the given shipment name and version and `extra` top-level YAML.
fn doc(name: &str, version: &str, extra: &str) -> String {
    format!(
        "format: 2\nshipment:\n  name: {name}\n  version: {version}\n  target: retail\n{extra}contributions: []\n"
    )
}

/// The validation failure `text` produces, with the code it is reported under.
fn validation_failure(text: &str, fmt: Format) -> ValidateError {
    match from_str(text, fmt) {
        Err(mercs2_quartermaster::ReadError::Validate(e)) => e,
        Err(other) => panic!("expected a validation failure, got a parse failure: {other}\n{text}"),
        Ok(_) => panic!("expected a validation failure, got a manifest:\n{text}"),
    }
}

/// What lint reports for a manifest that parses but fails validation. Lint receives manifests
/// that did not come through `from_str` (the Workshop edits one in memory), so this is the code a
/// modder sees.
fn lint_code(text: &str) -> &'static str {
    let m: Manifest = serde_norway::from_str(text).expect("parses");
    let found = mercs2_quartermaster::lint(&m, None, None);
    assert_eq!(found.len(), 1, "{found:?}");
    found[0].rule.code
}

/// A future format is refused, and says why.
#[test]
fn format3_future_rejected() {
    let text = minimal("retail", "ok-name", FORMAT_VERSION + 1);
    let e = validation_failure(&text, Format::Yaml);
    assert!(e.to_string().contains("refusing to guess"), "unhelpful message: {e}");
    assert_eq!(e.code(), None);
    assert_eq!(lint_code(&text), "M0100");
}

/// Format 1 no longer exists: it fails exactly like any other unknown format.
#[test]
fn format1_rejected_as_unsupported_m0100() {
    let text = minimal("retail", "ok-name", 1);
    let e = validation_failure(&text, Format::Yaml);
    assert!(e.to_string().contains("the only manifest format is 2"), "{e}");
    assert_eq!(lint_code(&text), "M0100");
}

#[test]
fn format0_rejected_as_unsupported_m0100() {
    let text = minimal("retail", "ok-name", 0);
    validation_failure(&text, Format::Yaml);
    assert_eq!(lint_code(&text), "M0100");
}

#[test]
fn the_current_format_version_is_accepted() {
    assert_eq!(FORMAT_VERSION, 2);
    from_str(&minimal("retail", "ok-name", FORMAT_VERSION), Format::Yaml).expect("current format");
}

#[test]
fn target_both_is_rejected_by_name() {
    let err = from_str(&minimal("both", "ok-name", FORMAT_VERSION), Format::Yaml)
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
            from_str(&minimal("retail", &format!("{bad:?}"), FORMAT_VERSION), Format::Yaml).is_err(),
            "{bad:?} should not be a valid shipment name"
        );
    }
    for good in ["sean-devlin-outfit", "boss-reskin", "a", "mod123"] {
        from_str(&minimal("retail", good, FORMAT_VERSION), Format::Yaml)
            .unwrap_or_else(|e| panic!("{good:?} should be valid: {e}"));
    }
}

/// `after` and `before` are gone; `Load` denies unknown fields, so either is refused.
#[test]
fn after_before_rejected() {
    for field in ["after", "before"] {
        let text = doc("s", "1.0.0", &format!("load:\n  {field}: [other]\n"));
        assert!(from_str(&text, Format::Yaml).is_err(), "{field} must not parse");
    }
}

#[test]
fn non_semver_version_rejected_m0100() {
    for bad in ["1.0", "v1.0.0", "latest", "1"] {
        let text = doc("s", &format!("\"{bad}\""), "");
        let e = validation_failure(&text, Format::Yaml);
        assert!(matches!(e, ValidateError::VersionNotSemver { .. }), "{bad}: {e:?}");
        assert_eq!(lint_code(&text), "M0100", "{bad}");
    }
}

/// A range that does not parse is M0172 wherever it appears.
#[test]
fn bad_range_rejected_m0172() {
    for extra in [
        "load:\n  requires:\n    - { shipment: other, version: \"not a range\" }\n",
        "load:\n  conflicts:\n    - { shipment: other, version: \"~>1\" }\n",
    ] {
        let text = doc("s", "1.0.0", extra);
        assert_eq!(validation_failure(&text, Format::Yaml).code(), Some("M0172"), "{extra}");
        assert_eq!(lint_code(&text), "M0172", "{extra}");
    }
    let text = "format: 2\nshipment:\n  name: s\n  version: 1.0.0\n  target: retail\n  \
                quartermaster: \"newest\"\ncontributions: []\n";
    assert_eq!(validation_failure(text, Format::Yaml).code(), Some("M0172"));
}

/// Requiring or conflicting with yourself is a recursive dependency: M0173, in every form.
#[test]
fn self_reference_rejected_m0173() {
    for extra in [
        "load:\n  requires: [self-ref]\n",
        "load:\n  requires:\n    - { shipment: self-ref, version: \"^1\" }\n",
        "load:\n  conflicts: [self-ref]\n",
        "load:\n  conflicts:\n    - { shipment: self-ref, version: \"^1\" }\n",
    ] {
        let text = doc("self-ref", "1.0.0", extra);
        assert_eq!(validation_failure(&text, Format::Yaml).code(), Some("M0173"), "{extra}");
        assert_eq!(lint_code(&text), "M0173", "{extra}");
    }
}

/// A Shipment named after a deny-listed DLL stem is refused, whatever its case.
#[test]
fn reserved_shipment_name_rejected_m0211() {
    for name in DENY_LISTED_DLL_STEMS.iter().copied().chain(["Cruise", "PMC_BB", "DxWrapper"]) {
        let text = doc(name, "1.0.0", "");
        let e = validation_failure(&text, Format::Yaml);
        assert_eq!(e.code(), Some("M0211"), "{name}: {e}");
        assert_eq!(lint_code(&text), "M0211", "{name}");
    }
    assert_eq!(DENY_LISTED_DLL_STEMS, &["pmc_bb", "cruise", "dxwrapper", "binkw32"]);
}

/// A Shipment name in `requires` / `conflicts` must be a slug, or it can name nothing.
#[test]
fn a_referenced_name_that_is_not_a_slug_is_rejected() {
    for extra in [
        "load:\n  requires: [Some_Mod]\n",
        "load:\n  conflicts: [\"has space\"]\n",
    ] {
        let text = doc("s", "1.0.0", extra);
        let e = validation_failure(&text, Format::Yaml);
        assert!(matches!(e, ValidateError::ReferenceNotSlug { .. }), "{extra}: {e:?}");
    }
}

/// `{ name, version }` is not a form this format has; the message names the one it does.
#[test]
fn compatible_form_rejected() {
    let text = doc(
        "s",
        "1.0.0",
        "load:\n  requires:\n    - { name: lua-bridge, version: \"^1.0.0\" }\n",
    );
    let e = validation_failure(&text, Format::Yaml);
    assert!(
        e.to_string().contains("{ shipment: lua-bridge, version: \"^1.0.0\" }"),
        "{e}"
    );
    assert_eq!(lint_code(&text), "M0100");
}

/// `{ url, sha256 }` has no variant: it fails to PARSE, like any shape the model does not have.
#[test]
fn external_form_fails_to_parse() {
    let yaml = doc(
        "s",
        "1.0.0",
        "load:\n  requires:\n    - { url: \"https://example.com/x.asi\", sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 }\n",
    );
    let json = r#"{"format":2,"shipment":{"name":"s","version":"1.0.0","target":"retail"},
        "load":{"requires":[{"url":"https://example.com/x.asi","sha256":"e3b0"}]}}"#;
    let toml = "format = 2\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
                [load]\nrequires = [{ url = \"https://example.com/x.asi\", sha256 = \"e3b0\" }]\n";
    for (text, fmt) in [(yaml.as_str(), Format::Yaml), (json, Format::Json), (toml, Format::Toml)] {
        match from_str(text, fmt) {
            Err(mercs2_quartermaster::ReadError::Parse { .. }) => {}
            other => panic!("{fmt:?}: expected a parse failure, got {other:?}"),
        }
    }
}

/// An object carrying keys from two forms matches neither (each wraps a `deny_unknown_fields`
/// struct), in every format.
#[test]
fn mixed_key_objects_rejected_all_formats() {
    let yaml = doc(
        "s",
        "1.0.0",
        "load:\n  requires:\n    - { shipment: a, name: a, version: \"^1\" }\n",
    );
    let json = r#"{"format":2,"shipment":{"name":"s","version":"1.0.0","target":"retail"},
        "load":{"requires":[{"shipment":"a","capability":"c"}]}}"#;
    let toml = "format = 2\n[shipment]\nname = \"s\"\nversion = \"1.0.0\"\ntarget = \"retail\"\n\
                [load]\nconflicts = [{ shipment = \"a\", version = \"^1\", name = \"a\" }]\n";
    for (text, fmt) in [(yaml.as_str(), Format::Yaml), (json, Format::Json), (toml, Format::Toml)] {
        match from_str(text, fmt) {
            Err(mercs2_quartermaster::ReadError::Parse { .. }) => {}
            other => panic!("{fmt:?}: expected a parse failure, got {other:?}"),
        }
    }
}

/// `supersedes` is a top-level field.
#[test]
fn supersedes_top_level_parses() {
    let text = doc(
        "ess",
        "0.7.0",
        "supersedes:\n  - { dest: on_load, file: 1_Ess.lua }\n  - { dest: on_load, file: 2_EssNames.lua }\n",
    );
    let m = from_str(&text, Format::Yaml).expect("parses");
    assert_eq!(m.supersedes.len(), 2);
    assert_eq!(m.supersedes[1].file, "2_EssNames.lua");
}

#[test]
fn supersedes_under_load_rejected() {
    let text = doc(
        "ess",
        "0.7.0",
        "load:\n  supersedes:\n    - { dest: on_load, file: 1_Ess.lua }\n",
    );
    assert!(from_str(&text, Format::Yaml).is_err());
}

/// A superseded `file` is one filename; the destination half is the closed `dest` set.
#[test]
fn a_superseded_path_is_rejected() {
    for file in ["../1_Ess.lua", "OnLoad/1_Ess.lua", "C:x.lua", "..", ""] {
        let text = doc(
            "ess",
            "0.7.0",
            &format!("supersedes:\n  - {{ dest: on_load, file: \"{file}\" }}\n"),
        );
        let e = validation_failure(&text, Format::Yaml);
        assert!(matches!(e, ValidateError::SupersededNotAFilename { .. }), "{file:?}: {e:?}");
    }
    // Not a write, so the placement extension ban does not apply to what is only detected.
    from_str(
        &doc("ess", "0.7.0", "supersedes:\n  - { dest: game_root, file: legacy.dll }\n"),
        Format::Yaml,
    )
    .expect("a .dll may be superseded");
}

#[test]
fn an_unknown_contribution_kind_is_rejected() {
    let text = "format: 2\nshipment:\n  name: x\n  version: 1.0.0\n  target: retail\ncontributions:\n  - kind: reticulate_splines\n    foo: bar\n";
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
