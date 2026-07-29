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
    assert!(
        diags.is_empty(),
        "clean shipment should be silent, got {diags:?}"
    );
    assert!(!lint::blocks_build(&diags));
}

#[test]
fn errors_block_the_build_and_warnings_do_not() {
    let blocking = lint::lint(&outfit("bulldog"), None, None);
    assert!(
        lint::blocks_build(&blocking),
        "an unknown wearer must block"
    );

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
    assert_eq!(
        diags[0].fix.as_deref(),
        Some("mattias"),
        "a typo should be auto-fixable"
    );
}

#[test]
fn every_real_hero_is_accepted() {
    for hero in lint::WARDROBE_HEROES {
        assert!(
            lint::lint(&outfit(hero), None, None).is_empty(),
            "{hero} must be valid"
        );
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
    let diags = lint::lint(
        &with_requirement("https://example.com/x.asi", "deadbeef"),
        None,
        None,
    );
    assert!(codes(&diags).contains(&"M0170"));
    assert!(lint::blocks_build(&diags));
}

#[test]
fn a_well_formed_pin_over_https_is_quiet() {
    let m = with_requirement(
        "https://github.com/o/r/releases/download/v1/x.asi",
        GOOD_SHA,
    );
    assert!(lint::lint(&m, None, None).is_empty());
}

/// The digest still protects integrity over plain http, so this is a warning rather than fatal.
#[test]
fn an_http_requirement_warns_but_does_not_block() {
    let diags = lint::lint(
        &with_requirement("http://example.com/x.asi", GOOD_SHA),
        None,
        None,
    );
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
    assert!(
        !lint::blocks_build(&diags),
        "a nameable hash is a warning, not a blocker"
    );
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
    assert_eq!(
        codes(&diags),
        vec!["M0110"],
        "with a root, the missing model is found"
    );
    assert!(lint::blocks_build(&diags));
}

// ---------------------------------------------------------------------------
// The registry itself
// ---------------------------------------------------------------------------

/// A linter that silently omits its most important rules reads as a clean bill of health. The
/// HANG-class checks must stay VISIBLE — either implemented, or registered as a known gap.
///
/// Asserts against BOTH lists on purpose. Which list a rule sits in is allowed to change as one
/// gets implemented (M0001 and M0002 have already moved); a rule vanishing from both is the actual
/// regression, and pinning the location would have made this test block that implementation work
/// instead of guarding against the disappearance it exists to catch.
#[test]
fn the_hang_rules_are_registered_not_hidden() {
    assert!(!lint::PENDING.is_empty());
    for r in lint::PENDING.iter().chain(lint::ARTIFACT_RULES) {
        assert!(
            !r.title.is_empty() && !r.doc.is_empty(),
            "{} needs a title and doc",
            r.code
        );
    }
    // The three named in Plan 01 as silent-and-catastrophic.
    let codes: Vec<&str> = lint::PENDING
        .iter()
        .chain(lint::ARTIFACT_RULES)
        .map(|r| r.code)
        .collect();
    for expected in ["M0001", "M0002", "M0003"] {
        assert!(
            codes.contains(&expected),
            "{expected} must stay registered somewhere"
        );
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

// ---------------------------------------------------------------------------
// Bare hashes are legal
// ---------------------------------------------------------------------------

/// A bare `0x…` reference IS the hash, not a string to be hashed.
///
/// The base game ships hashes, so a modder working on an asset our name table does not cover has
/// nothing else to write. Hashing the string `"0x6F84F6A3"` yields `0xC6B71C1F` — a different asset
/// — so this is the difference between a target that resolves and one that reports "not in the
/// configured game stack" while looking like a spelling mistake.
#[test]
fn a_bare_hash_resolves_to_itself_not_to_a_hash_of_its_text() {
    use mercs2_quartermaster::manifest::asset_hash;
    assert_eq!(asset_hash("0x6F84F6A3"), 0x6F84_F6A3);
    assert_eq!(
        asset_hash("0X6f84f6a3"),
        0x6F84_F6A3,
        "case must not matter"
    );
    assert_eq!(
        asset_hash("  0x6F84F6A3  "),
        0x6F84_F6A3,
        "surrounding space must not matter"
    );

    // A name is hashed, and must NOT collide with the parse path.
    let by_name = asset_hash("global_4x4_nm");
    assert_eq!(by_name, 0x6F84_F6A3, "this name hashes to that asset");
    assert_ne!(
        asset_hash("0x6F84F6A3"),
        mercs2_formats::hash::pandemic_hash_m2("0x6F84F6A3"),
        "hashing the TEXT of a hash is the bug this exists to prevent"
    );
}

/// Hex too long to be a u32 is not an asset hash, whatever else it is.
#[test]
fn overlong_hex_is_treated_as_a_name() {
    use mercs2_quartermaster::manifest::bare_hash;
    assert_eq!(bare_hash("0xDEADBEEFCAFE"), None);
    assert_eq!(bare_hash("0x"), None);
    assert_eq!(bare_hash("0xZZZZ"), None);
    assert_eq!(bare_hash("global_4x4_nm"), None);
}

/// The preference is expressed where it applies — `target:`, not only `touches:` — and only when a
/// name can actually be offered.
#[test]
fn a_named_hash_is_suggested_and_an_unnamed_one_is_left_alone() {
    let names = NameTable::from_pairs([(0x6F84_F6A3u32, "global_4x4_nm")]);
    let manifest_for = |target: &str| {
        shipment_with(&format!(
            "  - kind: replace_texture
    target: \"{target}\"
    image: src/t.png
"
        ))
    };

    let known = lint::lint(&manifest_for("0x6F84F6A3"), None, Some(&names));
    let m0130: Vec<_> = known.iter().filter(|d| d.rule.code == "M0130").collect();
    assert_eq!(
        m0130.len(),
        1,
        "a hash we can name must be surfaced: {known:?}"
    );
    assert_eq!(m0130[0].severity, Severity::Warning, "never blocking");
    assert_eq!(m0130[0].fix.as_deref(), Some("global_4x4_nm"));

    // No name known: the hash is the only thing the author COULD write, so saying nothing is right.
    let unknown = lint::lint(&manifest_for("0xDEADBEEF"), None, Some(&names));
    assert!(
        !unknown.iter().any(|d| d.rule.code == "M0130"),
        "nagging about a hash with no known name asks for something impossible: {unknown:?}"
    );
}

// ---------------------------------------------------------------------------
// M0190 — a movie the runtime cannot script
// ---------------------------------------------------------------------------

/// Build a minimal uncompressed `GFX` movie carrying `tags`, each `(code, body)`.
///
/// Hand-rolled rather than fixture-checked-in so the AS3 case and the clean case differ by exactly
/// one tag. A binary fixture would leave "is this movie actually clean?" resting on a file nobody
/// can read in a diff.
fn movie_with(tags: &[(u16, Vec<u8>)]) -> Vec<u8> {
    let mut body = Vec::new();
    body.push(0); // stage RECT: nbits = 0 in the top 5 bits, so the bounds are empty
    body.extend_from_slice(&(30u16 << 8).to_le_bytes()); // 30 fps
    body.extend_from_slice(&1u16.to_le_bytes()); // one frame
    for (code, tag_body) in tags {
        // Long form for anything that will not fit the 6-bit inline length.
        if tag_body.len() < 0x3F {
            body.extend_from_slice(&((code << 6) | tag_body.len() as u16).to_le_bytes());
        } else {
            body.extend_from_slice(&((code << 6) | 0x3F).to_le_bytes());
            body.extend_from_slice(&(tag_body.len() as u32).to_le_bytes());
        }
        body.extend_from_slice(tag_body);
    }
    body.extend_from_slice(&0u16.to_le_bytes()); // End
    let mut file = Vec::new();
    file.extend_from_slice(b"GFX");
    file.push(8);
    file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

/// Write a movie into a scratch Shipment root and lint it.
fn lint_movie(label: &str, movie: Vec<u8>) -> Vec<lint::Diagnostic> {
    let root = std::env::temp_dir().join(format!("qm_lint_movie_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("scratch");
    std::fs::write(root.join("src/ui.gfx"), movie).expect("write movie");
    let m = shipment_with("  - kind: add_movie\n    name: qm_test_ui\n    movie: src/ui.gfx\n");
    lint::lint(&m, Some(&root), None)
}

/// M0190 fires on a movie carrying AS3. It BLOCKS: GFx 2.0.48 has no DoABC loader, so the tag is
/// skipped as unknown and the movie loads perfectly with none of its logic running — the failure is
/// invisible from inside the game.
#[test]
fn a_movie_carrying_as3_is_an_error() {
    // DoABC (82): u32 flags, NUL-terminated name, then ABC. The body is never parsed — the tag's
    // presence is the finding — but it is shaped correctly so the fixture is not nonsense.
    let mut abc = 1u32.to_le_bytes().to_vec();
    abc.extend_from_slice(b"widget\0");
    abc.extend_from_slice(&[0x10, 0x00, 0x2E, 0x00]);
    let diags = lint_movie("as3", movie_with(&[(82, abc)]));

    let found: Vec<_> = diags.iter().filter(|d| d.rule.code == "M0190").collect();
    assert_eq!(found.len(), 1, "{diags:?}");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(lint::blocks_build(&diags), "a silent no-op must not ship");
    assert!(
        found[0].message.contains("DoABC"),
        "the message must name the tag: {}",
        found[0].message
    );
}

/// M0190 stays quiet on an AS2 movie — the shape a GFx 2.x authoring tool emits and the shape all
/// 64 retail movies have. A rule that fired here would fire on every movie anyone could ship.
#[test]
fn an_as2_movie_is_left_alone() {
    // DoAction (12) is AVM1 — the bytecode this runtime DOES execute. Its presence must not be
    // mistaken for scripting the runtime cannot run.
    let diags = lint_movie("as2", movie_with(&[(12, vec![0x00])]));
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0190"),
        "an AVM1 movie is the supported case: {diags:?}"
    );
    assert!(!lint::blocks_build(&diags), "{diags:?}");
}

/// A payload that is not a movie is NOT M0190's problem. Reporting "no AS3 found" about a PNG would
/// be answering a question nobody asked; the lowering refuses it with a message that names what a
/// `.gfx` is.
#[test]
fn a_payload_that_is_not_a_movie_is_left_to_the_lowering() {
    let diags = lint_movie(
        "notamovie",
        b"\x89PNG\r\n\x1a\n not a movie at all".to_vec(),
    );
    assert!(!diags.iter().any(|d| d.rule.code == "M0190"), "{diags:?}");
}

/// The rule is registered, so `qm` can list it among what it checks. An unregistered rule is one a
/// modder cannot look up after seeing its code in CI output.
#[test]
fn m0190_is_registered() {
    assert!(lint::RULES.iter().any(|r| r.code == "M0190"));
}
