//! Linter behaviour. Every rule gets a case that FIRES and a case that stays QUIET — a rule with
//! only the former will eventually fire on everything and get ignored, which is how linters die.

use mercs2_quartermaster::lint::{self, Severity};
use mercs2_quartermaster::names::NameTable;
use mercs2_quartermaster::{from_str, Format, Manifest};

fn parse(yaml: &str) -> Manifest {
    from_str(yaml, Format::Yaml).unwrap_or_else(|e| panic!("fixture must parse: {e}\n{yaml}"))
}

fn shipment_with(contributions: &str) -> Manifest {
    parse(&format!(
        "format: 1
shipment: {{ name: s, version: 1.0.0, target: retail }}
contributions:
{contributions}"
    ))
}

fn codes(diags: &[lint::Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.rule.code).collect()
}

fn outfit(wearer: &str) -> Manifest {
    shipment_with(&format!(
        "  - kind: add_outfit
    name: sean_devlin
    slug: SeanDevlin
    display: Sean Devlin
    wearer: {wearer}
    model: src/m.glb
"
    ))
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn a_clean_shipment_produces_nothing_and_does_not_block() {
    let m = outfit("mattias");
    let diags = lint::lint(&m, None, None);
    assert!(diags.is_empty(), "clean shipment should be silent, got {diags:?}");
    assert!(!lint::blocks_build(&diags));
}

#[test]
fn errors_block_the_build_and_warnings_do_not() {
    let blocking = lint::lint(&outfit("bulldog"), None, None);
    assert!(lint::blocks_build(&blocking), "an unknown wearer must block");

    let warning_only = shipment_with(
        "  - kind: patch_lua
    target: wifmissionflow
    append: src/a.lua
",
    );
    let diags = lint::lint(&warning_only, None, None);
    assert_eq!(codes(&diags), vec!["M0141"]);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert!(!lint::blocks_build(&diags), "a warning must not block");
}

// ---------------------------------------------------------------------------
// M0140 — the wardrobe hero keys
// ---------------------------------------------------------------------------

/// `_tOutfits` has lists for exactly three heroes. An outfit filed anywhere else is appended to a
/// table nothing reads — it never appears, and the game reports nothing.
#[test]
fn an_unknown_wearer_is_an_error_with_a_suggested_spelling() {
    let diags = lint::lint(&outfit("mattius"), None, None);
    assert_eq!(codes(&diags), vec!["M0140"]);
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(diags[0].fix.as_deref(), Some("mattias"), "a typo should be auto-fixable");
}

#[test]
fn every_real_hero_is_accepted() {
    for hero in lint::WARDROBE_HEROES {
        assert!(lint::lint(&outfit(hero), None, None).is_empty(), "{hero} must be valid");
    }
}

/// A confident wrong suggestion is worse than none, so a stranger gets flagged without one.
#[test]
fn an_unrelated_wearer_is_flagged_without_a_bogus_fix() {
    let diags = lint::lint(&outfit("bulldog"), None, None);
    assert_eq!(codes(&diags), vec!["M0140"]);
    assert_eq!(diags[0].fix, None);
}

// ---------------------------------------------------------------------------
// M0150 — raw must declare its radius
// ---------------------------------------------------------------------------

/// The declared blast radius is what makes an opaque payload safe. Claiming nothing means the
/// conflict system cannot see it, so it would overwrite other Shipments silently.
#[test]
fn a_raw_contribution_with_no_touches_is_an_error() {
    let m = shipment_with(
        "  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: []
",
    );
    let diags = lint::lint(&m, None, None);
    assert_eq!(codes(&diags), vec!["M0150"]);
    assert!(lint::blocks_build(&diags));
}

#[test]
fn a_raw_contribution_that_declares_its_radius_is_quiet() {
    let m = shipment_with(
        "  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"al_veh_boat_destroyer\"]
",
    );
    assert!(lint::lint(&m, None, None).is_empty());
}

// ---------------------------------------------------------------------------
// M0160 / M0161 — Code layer
// ---------------------------------------------------------------------------

/// An .asi is a RETAIL mechanism; pmc_bb.dll loads it into the retail exe. Attaching one to a
/// reimpl target ships a file that will never be loaded.
#[test]
fn an_asi_on_a_reimpl_target_is_an_error() {
    let m = parse(
        "format: 1
shipment: { name: s, version: 1.0.0, target: reimpl }
contributions:
  - kind: native_hook
    target: reimpl
    plugin: src/x.asi
    touches: [\"0x004CF340\"]
",
    );
    assert!(codes(&lint::lint(&m, None, None)).contains(&"M0160"));
}

#[test]
fn an_asi_on_a_retail_target_is_fine() {
    let m = shipment_with(
        "  - kind: native_hook
    target: retail
    plugin: src/x.asi
    touches: [\"0x004CF340\"]
",
    );
    assert!(lint::lint(&m, None, None).is_empty());
}

#[test]
fn a_hook_with_nothing_to_install_is_an_error() {
    let m = shipment_with(
        "  - kind: native_hook
    target: retail
    touches: [\"0x004CF340\"]
",
    );
    assert!(codes(&lint::lint(&m, None, None)).contains(&"M0161"));
}

// ---------------------------------------------------------------------------
// M0170 / M0171 — pinned external requirements
// ---------------------------------------------------------------------------

fn with_requirement(url: &str, sha: &str) -> Manifest {
    parse(&format!(
        "format: 1
shipment: {{ name: s, version: 1.0.0, target: retail }}
load:
  requires:
    - url: {url}
      sha256: {sha}
contributions: []
"
    ))
}

const GOOD_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// An unusable pin is worse than none: it reads as verified.
#[test]
fn a_malformed_digest_is_an_error() {
    let diags = lint::lint(&with_requirement("https://example.com/x.asi", "deadbeef"), None, None);
    assert!(codes(&diags).contains(&"M0170"));
    assert!(lint::blocks_build(&diags));
}

#[test]
fn a_well_formed_pin_over_https_is_quiet() {
    let m = with_requirement("https://github.com/o/r/releases/download/v1/x.asi", GOOD_SHA);
    assert!(lint::lint(&m, None, None).is_empty());
}

/// The digest still protects integrity over plain http, so this is a warning rather than fatal.
#[test]
fn an_http_requirement_warns_but_does_not_block() {
    let diags = lint::lint(&with_requirement("http://example.com/x.asi", GOOD_SHA), None, None);
    assert_eq!(codes(&diags), vec!["M0171"]);
    assert_eq!(diags[0].severity, Severity::Warning);
    assert!(!lint::blocks_build(&diags));
}

// ---------------------------------------------------------------------------
// Wiring of the earlier increments
// ---------------------------------------------------------------------------

#[test]
fn a_bare_hash_becomes_an_auto_fixable_warning() {
    let m = shipment_with(
        "  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"0xE54047D5\"]
",
    );
    let names = NameTable::from_pairs([(0xE540_47D5u32, "al_veh_boat_destroyer")]);
    let diags = lint::lint(&m, None, Some(&names));
    assert_eq!(codes(&diags), vec!["M0130"]);
    assert_eq!(diags[0].fix.as_deref(), Some("al_veh_boat_destroyer"));
    assert!(!lint::blocks_build(&diags), "a nameable hash is a warning, not a blocker");
}

/// Without a table we cannot suggest a name, so the rule must not fire at all rather than emit a
/// finding the author cannot act on.
#[test]
fn without_a_name_table_the_bare_hash_rule_is_silent() {
    let m = shipment_with(
        "  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"0xE54047D5\"]
",
    );
    assert!(lint::lint(&m, None, None).is_empty());
}

#[test]
fn a_self_conflict_surfaces_as_a_blocking_diagnostic() {
    let m = shipment_with(
        "  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/a.png
  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/b.png
",
    );
    let diags = lint::lint(&m, None, None);
    assert_eq!(codes(&diags), vec!["M0120"]);
    assert!(lint::blocks_build(&diags));
}

#[test]
fn source_checks_only_run_when_a_root_is_supplied() {
    let m = outfit("mattias");
    // No root: the model file is never looked for.
    assert!(lint::lint(&m, None, None).is_empty());

    let dir = std::env::temp_dir().join(format!("qm_lint_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let diags = lint::lint(&m, Some(&dir), None);
    assert_eq!(codes(&diags), vec!["M0110"], "with a root, the missing model is found");
    assert!(lint::blocks_build(&diags));
}

// ---------------------------------------------------------------------------
// The registry itself
// ---------------------------------------------------------------------------

/// A linter that silently omits its most important rules reads as a clean bill of health. The
/// HANG-class checks that need the WAD stack must stay VISIBLE until they are implemented.
#[test]
fn the_pending_hang_rules_are_registered_not_hidden() {
    assert!(!lint::PENDING.is_empty());
    for r in lint::PENDING {
        assert!(!r.title.is_empty() && !r.doc.is_empty(), "{} needs a title and doc", r.code);
    }
    // The three named in Plan 01 as silent-and-catastrophic.
    let codes: Vec<&str> = lint::PENDING.iter().map(|r| r.code).collect();
    for expected in ["M0001", "M0002", "M0003"] {
        assert!(codes.contains(&expected), "{expected} must stay registered");
    }
}

#[test]
fn every_implemented_rule_carries_a_doc_link() {
    for r in lint::RULES {
        assert!(!r.doc.is_empty(), "{} has no doc link", r.code);
        assert!(!r.title.is_empty(), "{} has no title", r.code);
    }
}

/// The single-block predicate M0007 will use. Both LOD halves must be sentinel — a row names up to
/// FOUR rungs, and checking only `packed_block_ref` misses `_P002`/`_P003`.
#[test]
fn single_block_requires_both_lod_halves_to_be_sentinel() {
    assert!(lint::aset_row_is_single_block(0x0000_FFFF, 0xFFFF_FFFF));
    // ch_veh_tank_ztz98 from docs/aset_format.md: _P001 in lo16, _P002 in secondary hi16.
    assert!(!lint::aset_row_is_single_block(0x0DED_14D7, 0x2093_FFFF));
    // A _P001 rung present but both other rungs absent is still multi-rung.
    assert!(!lint::aset_row_is_single_block(0x0DED_14D7, 0xFFFF_FFFF));
    // _P002 present, _P001 absent.
    assert!(!lint::aset_row_is_single_block(0x0000_FFFF, 0x2093_FFFF));
}
