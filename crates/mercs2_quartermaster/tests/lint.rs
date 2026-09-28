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
        "format: 2
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

    // Editing one copy of a string table shared between shell.wad and vz.wad is advisory (M0191):
    // the fix is a deploy question, not a defect in the manifest.
    let warning_only = shipment_with(
        "  - kind: edit_stringdb
    target: english
    strings: src/english.txt
",
    );
    let diags = lint::lint(&warning_only, None, None);
    assert_eq!(codes(&diags), vec!["M0191"]);
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
        "format: 2
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
// M0162 / M0163 — placed companion files
// ---------------------------------------------------------------------------

fn placed(file: &str, dest: &str) -> Manifest {
    shipment_with(&format!(
        "  - kind: place_file
    file: src/{file}
    dest: {dest}
"
    ))
}

/// The exe and the WADs, reached through the one destination that could touch them. The lowering
/// refuses these too; the rule exists so template CI — which runs `qm lint` and never has a game —
/// says so on the push instead of leaving it to somebody's machine.
#[test]
fn a_placed_file_that_would_clobber_the_game_is_an_error() {
    for name in ["Mercenaries2.exe", "vz.wad", "pmc_bb.dll", "pmc_bb.asi"] {
        let diags = lint::lint(&placed(name, "game_root"), None, None);
        assert!(codes(&diags).contains(&"M0162"), "{name}: {diags:?}");
        assert!(lint::blocks_build(&diags), "{name} must block");
    }
}

/// A plugin is not a companion: allowing one here would route around `native_hook`'s PE checks, its
/// reserved-name refusal and its hooked-address claims.
#[test]
fn a_placed_asi_is_an_error_that_names_the_kind_to_use() {
    let diags = lint::lint(&placed("evil.asi", "scripts"), None, None);
    let hit = diags
        .iter()
        .find(|d| d.rule.code == "M0162")
        .expect("M0162 must fire");
    assert!(hit.message.contains("native_hook"), "{hit}");
}

/// The companions real mods actually ship. A rule that fired on these would be a rule everyone
/// turns off.
#[test]
fn an_ordinary_companion_is_quiet() {
    for (file, dest) in [
        ("quiet_freeplay_vo.ini", "scripts"),
        ("lua_console.py", "scripts"),
        ("00_core.lua", "on_boot"),
        ("keys.lua", "on_key"),
        ("readme.txt", "game_root"),
    ] {
        let diags = lint::lint(&placed(file, dest), None, None);
        assert!(diags.is_empty(), "{file} -> {dest}: {diags:?}");
    }
}

/// M0163. These plugins resolve config with `m2_module_path(g_hModule, "x.ini", …)` — that is
/// `GetModuleFileNameA` truncated at the last separator, so the file is looked up beside the loaded
/// module and nowhere else. A companion sent anywhere else is never opened, and the plugin falls
/// back to its defaults with the file sitting there looking installed.
#[test]
fn a_companion_away_from_its_plugin_is_a_warning() {
    for dest in ["game_root", "plugins", "update", "on_boot"] {
        let m = shipment_with(&format!(
            "  - kind: native_hook
    target: retail
    plugin: src/quiet_freeplay_vo.asi
    touches: [\"0x004CF340\"]
  - kind: place_file
    file: src/quiet_freeplay_vo.ini
    dest: {dest}
"
        ));
        let diags = lint::lint(&m, None, None);
        assert!(codes(&diags).contains(&"M0163"), "{dest}: {diags:?}");
        // A heuristic that blocked the build over a filename coincidence would be worse than the
        // trap it catches.
        assert!(!lint::blocks_build(&diags), "{dest} must not block");
    }
}

/// Quiet when the companion is beside its plugin — and quiet when the name is nobody's companion,
/// because the rule is a claim about a specific plugin's config lookup, not about `.ini` files.
#[test]
fn a_companion_beside_its_plugin_or_belonging_to_no_plugin_is_quiet() {
    let beside = shipment_with(
        "  - kind: native_hook
    target: retail
    plugin: src/quiet_freeplay_vo.asi
    touches: [\"0x004CF340\"]
  - kind: place_file
    file: src/quiet_freeplay_vo.ini
    dest: scripts
",
    );
    assert!(lint::lint(&beside, None, None).is_empty());

    let unrelated = shipment_with(
        "  - kind: native_hook
    target: retail
    plugin: src/quiet_freeplay_vo.asi
    touches: [\"0x004CF340\"]
  - kind: place_file
    file: src/something_else.ini
    dest: game_root
",
    );
    assert!(
        !codes(&lint::lint(&unrelated, None, None)).contains(&"M0163"),
        "a file that is nobody's companion is not this rule's business"
    );
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

// ---------------------------------------------------------------------------
// add_language (M0200)
// ---------------------------------------------------------------------------

/// A name the game already ships (`.\Data\english.wad`) is an ERROR: add_language may only ADD, and
/// placing over a shipped WAD would shadow base-game data — the `data/` safety pivot.
#[test]
fn a_language_name_that_collides_with_a_shipped_wad_is_an_error() {
    for name in ["english", "vz", "shell", "russian"] {
        let m = shipment_with(&format!(
            "  - kind: add_language
    name: {name}
    display: X
    strings: src/text/x.txt
"
        ));
        let diags = lint::lint(&m, None, None);
        assert!(
            codes(&diags).contains(&"M0200"),
            "{name} must be refused: {diags:?}"
        );
        assert!(lint::blocks_build(&diags), "{name} must block the build");
    }
}

/// A name that is not a lowercase `[a-z0-9_]` token cannot be a filename / stringdb key — M0200.
#[test]
fn a_language_name_that_is_not_a_token_is_an_error() {
    for name in ["Polski", "pl-PL", "pl.pl", "es/es"] {
        let m = shipment_with(&format!(
            "  - kind: add_language
    name: \"{name}\"
    display: X
    strings: src/text/x.txt
"
        ));
        assert!(
            codes(&lint::lint(&m, None, None)).contains(&"M0200"),
            "{name} is not a language token"
        );
    }
}

/// A novel language on its own is clean. Switching the game into it is Modkit's job, not something a
/// Shipment has to carry, so nothing asks for a companion plugin.
#[test]
fn a_language_alone_lints_clean() {
    let m = shipment_with(
        "  - kind: add_language
    name: polski
    display: Polski
    strings: src/text/polski.txt
",
    );
    let diags = lint::lint(&m, None, None);
    assert!(diags.is_empty(), "{diags:?}");
}

/// M0200 is registered, so `qm rules` can list it; M0201 (a language without a selector plugin) is
/// gone, because selection is not the Shipment's concern.
#[test]
fn the_language_rules_are_registered() {
    assert!(lint::RULES.iter().any(|r| r.code == "M0200"));
    assert!(!lint::RULES.iter().any(|r| r.code == "M0201"));
}

// ---------------------------------------------------------------------------
// M0191 / M0202 — message text and registration
// ---------------------------------------------------------------------------

/// The M0191 message is one sentence run, not a string with the source's indentation embedded in it.
#[test]
fn the_m0191_message_has_no_runs_of_spaces() {
    let m = shipment_with("  - kind: edit_stringdb\n    target: english\n    strings: src/e.txt\n");
    let diags = lint::lint(&m, None, None);
    let d = diags.iter().find(|d| d.rule.code == "M0191").expect("M0191 fires");
    assert!(!d.message.contains("  "), "double space in: {:?}", d.message);
}

/// Every rule the hermetic `lint` can emit is registered, so a modder can look its code up.
#[test]
fn m0202_and_m0213_are_registered() {
    for code in ["M0202", "M0213"] {
        assert!(lint::RULES.iter().any(|r| r.code == code), "{code}");
    }
}

// ---------------------------------------------------------------------------
// M0300 / M0301 / M0302 — Lua source rules
// ---------------------------------------------------------------------------

/// Write a Shipment scratch directory with the given file entries, using a per-test unique root
/// under `%TEMP%` so parallel `cargo test` runs do not step on each other. Removes and recreates
/// the root each call.
fn scratch(label: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir()
        .join(format!("qm_lint_lua_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("scratch");
    for (rel, body) in files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("scratch parent");
        }
        std::fs::write(&path, body).expect("write scratch");
    }
    root
}

/// A Shipment whose add_script registers itself as a mission (its name shows up as an
/// `sModuleName = "…"` in a `patch_lua mrxmissionflow` append) but whose source does not
/// `inherit("MrxTaskContract")` is M0300. This is [[custom-mission-inherit-mrxtask-required]] —
/// the FioDef001 support-drop bug caught before it ships.
#[test]
fn m0300_fires_on_a_mission_add_script_missing_inherit() {
    let module_body = "\
-- Missing inherit here; without it, oMission:IsActive resolves to nil.
function LoadAssets(self, tSaveData)
end
function Activated(self)
end
function Cleanup(self)
end
";
    let register_body = "\
tMissionData['FioDef001'] = {
  sModuleName = 'FioDef001',
  sFactionId = 'Pmc',
  bContract = true,
}
";
    let root = scratch(
        "m0300_fires",
        &[
            ("src/fiodef001.lua", module_body),
            ("src/register.lua", register_body),
        ],
    );
    let m = shipment_with(
        "  - kind: add_script
    name: FioDef001
    source: src/fiodef001.lua
  - kind: patch_lua
    target: mrxmissionflow
    append: src/register.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    let found: Vec<_> = diags.iter().filter(|d| d.rule.code == "M0300").collect();
    assert_eq!(found.len(), 1, "{diags:?}");
    assert_eq!(found[0].severity, Severity::Error);
    assert!(lint::blocks_build(&diags), "M0300 must block the build");
    assert!(
        found[0].message.contains("FioDef001"),
        "message must name the module: {}",
        found[0].message
    );
    assert_eq!(
        found[0].fix.as_deref(),
        Some("inherit(\"MrxTaskContract\")"),
        "the fix is a concrete inherit call the modder can paste in"
    );
}

/// The same shape with a proper `inherit("MrxTaskContract")` at the top is quiet. Every retail
/// contract / job script writes this line, so a rule that fired here would fire on the shipping
/// game.
#[test]
fn m0300_is_quiet_when_the_add_script_inherits_mrxtask() {
    let module_body = "\
inherit(\"MrxTaskContract\")
function LoadAssets(self, tSaveData)
end
function Activated(self)
end
function Cleanup(self)
end
";
    let register_body = "\
tMissionData['FioDef001'] = {
  sModuleName = 'FioDef001',
  bContract = true,
}
";
    let root = scratch(
        "m0300_quiet",
        &[
            ("src/fiodef001.lua", module_body),
            ("src/register.lua", register_body),
        ],
    );
    let m = shipment_with(
        "  - kind: add_script
    name: FioDef001
    source: src/fiodef001.lua
  - kind: patch_lua
    target: mrxmissionflow
    append: src/register.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0300"),
        "{diags:?}"
    );
}

/// An `add_script` that ships a helper library — no `sModuleName` reference in this Shipment's
/// Lua — is not a mission, so M0300 must not fire on it. A rule that flagged every AddScript
/// would refuse a bespoke utility module.
#[test]
fn m0300_stays_quiet_on_a_non_mission_add_script() {
    let root = scratch(
        "m0300_non_mission",
        &[(
            "src/util.lua",
            "function ClampToRange(x, lo, hi) return math.max(lo, math.min(hi, x)) end\n",
        )],
    );
    let m = shipment_with(
        "  - kind: add_script
    name: my_util
    source: src/util.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0300"),
        "helper libs are not missions: {diags:?}"
    );
}

/// A Shipment whose Lua calls `Event.Create(` directly leaks the handle: `MrxTask.DestroyEvents`
/// on Cleanup cannot delete an event that was never inserted into `self._tEvents`. M0301 flags
/// each call with a (line, col) anchor so the modder can jump to the site.
#[test]
fn m0301_fires_on_bare_event_create() {
    let body = "\
inherit(\"MrxTaskContract\")
function Activated(self)
  local uHandle = Event.Create(Event.TimerRelative, {5}, DoStuff, {self})
  local uPersist = Event.CreatePersistent(Event.ScriptEvent, {'x'}, F, {self})
end
";
    let root = scratch("m0301_fires", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    let found: Vec<_> = diags.iter().filter(|d| d.rule.code == "M0301").collect();
    assert_eq!(found.len(), 2, "one hit per call: {diags:?}");
    assert!(found.iter().all(|d| d.severity == Severity::Error));
    assert!(
        found.iter().any(|d| d.message.contains("Event.CreatePersistent(")),
        "persistent variant is named explicitly (survives level transitions): {found:?}"
    );
    assert!(lint::blocks_build(&diags));
}

/// Colon-syntax methods (`function X:Y(...)`) have `self` implicitly, so the M0301 fix
/// (`self:_CreateEvent`) is applicable and the rule must fire.
#[test]
fn m0301_fires_on_colon_method_form() {
    let body = "\
inherit(\"MrxTaskContract\")
function MyMission:Activated()
  Event.Create(Event.TimerRelative, {5}, DoStuff, {self})
end
";
    let root = scratch("m0301_colon", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert_eq!(
        diags.iter().filter(|d| d.rule.code == "M0301").count(),
        1,
        "colon method has implicit self: {diags:?}"
    );
}

/// A `patch_lua` of engine code — `MrxPlayer.LoadSingleton` and friends — has no MrxTask context;
/// there is no `self`. The engine itself uses `Event.Create` in these functions (this is why
/// `bug_006_heroswap_save_restore.lua` in unofficial-patch preserves them verbatim: "Body is
/// retail's + the clip restore"). The rule's suggested fix cannot apply — refusing this build
/// would be a false positive.
#[test]
fn m0301_is_quiet_on_patch_lua_of_engine_code_with_no_self() {
    let body = "\
function LoadSingleton(tSaveData)
  for i, uCharGuid in ipairs(GetPlayers()) do
    function _RestoreEquipment(uGuid, tSavedEquipment)
      Event.Create(Event.ObjectHibernation, {uGuid, 'a'}, _QmRestoreAmmo, {uGuid})
    end
    Event.Create(Event.ObjectHibernation, {uCharGuid, 'a'}, _RestoreEquipment, {uCharGuid})
  end
end
";
    let root = scratch("m0301_patch_lua_no_self", &[("src/patch.lua", body)]);
    let m = shipment_with(
        "  - kind: patch_lua
    target: mrxplayer
    append: src/patch.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0301"),
        "engine-code patch has no MrxTask context, self:_CreateEvent doesn't apply: {diags:?}"
    );
}

/// An Event.Create at file scope, with no enclosing function at all, has no `self` — the fix
/// cannot apply, so the rule must not fire.
#[test]
fn m0301_is_quiet_at_top_level() {
    let body = "\
inherit(\"MrxTaskContract\")
Event.Create(Event.TimerRelative, {5}, DoStuff, {})
";
    let root = scratch("m0301_top_level", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0301"),
        "top-level call has no enclosing self: {diags:?}"
    );
}

/// The safe form — `self:_CreateEvent(…)` / `self:_CreatePersistentEvent(…)` — is not caught,
/// because `_CreateEvent` is a method on `MrxTask` and inserts the handle into `self._tEvents`
/// before returning it. Every retail contract / job uses this form; a rule that flagged it would
/// fire on all of them.
#[test]
fn m0301_is_quiet_on_the_self_createevent_form() {
    let body = "\
inherit(\"MrxTaskContract\")
function Activated(self)
  self:_CreateEvent(Event.TimerRelative, {5}, DoStuff, {self})
  self:_CreatePersistentEvent(Event.ScriptEvent, {'x'}, F, {self})
end
";
    let root = scratch("m0301_quiet", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0301"),
        "self:_CreateEvent must be silent: {diags:?}"
    );
}

/// A Shipment's Lua writes to `_G.<name>` or `_MODULES[<name>]` at file scope — either pollutes
/// the global environment or reaches into another module's namespace. Both are the M0302
/// class — the `__newindex` on `_G` crash at `0x0059C82A` is the concrete failure this exists to
/// keep out of the shipped ecosystem.
#[test]
fn m0302_fires_on_g_and_modules_writes() {
    let body = "\
inherit(\"MrxTaskContract\")
_G.MyGlobal = 42
_MODULES['MrxTaskContract'] = { hacked = true }
_MODULES.MrxPmc = nil
";
    let root = scratch("m0302_fires", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    let found: Vec<_> = diags.iter().filter(|d| d.rule.code == "M0302").collect();
    assert_eq!(found.len(), 3, "one hit per write: {diags:?}");
    assert!(found.iter().all(|d| d.severity == Severity::Error));
    assert!(
        found.iter().any(|d| d.message.contains("_G.MyGlobal")),
        "the _G. hit names the exact identifier: {found:?}"
    );
    assert!(lint::blocks_build(&diags));
}

/// A comparison like `if _G.foo == nil then` is a READ, not a write, and must not be flagged —
/// the rule scans for `= ` not `==`. Similarly a legitimate module-local write (`tEvents =
/// tEvents or {}` in module scope) is silent: it targets the module's own env, not `_G`.
#[test]
fn m0302_is_quiet_on_reads_and_module_locals() {
    let body = "\
inherit(\"MrxTaskContract\")
if _G.OptionalHook == nil then return end
if _MODULES.MrxPmc then Debug.Printf('ok') end
tEvents = tEvents or {}
local uHandle = self:_CreateEvent(Event.TimerRelative, {1}, F, {self})
";
    let root = scratch("m0302_quiet", &[("src/mission.lua", body)]);
    let m = shipment_with(
        "  - kind: add_script
    name: MyMission
    source: src/mission.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0302"),
        "reads and module-scoped writes must stay silent: {diags:?}"
    );
}

/// A `tMissionData` row whose key is off the engine's `<Faction3><Con|Job><NN+>` shape wedges
/// the briefing-dialog teardown at `mrxbriefing.lua:2849` the instant the player picks it (F12).
/// The scanner catches literal-string and dot-access assignments; the shape check lets anything
/// 7+ bytes long that is 3 chars + `Con|Job` + pure digits pass.
#[test]
fn m0303_fires_on_unparseable_mission_id() {
    let body = "\
WifMissionData.tMissionData[\"AbTest_C_Visible\"] = { sFactionId = \"Pmc\" }
WifMissionData.tMissionData.Pmc01Repeat = { sFactionId = \"Pmc\" }
WifMissionData.tMissionData[\"PmcCon001\"] = { sFactionId = \"Pmc\" }
WifMissionData.tMissionData.AbtJob017 = { sFactionId = \"All\" }
";
    let root = scratch("m0303_fires", &[("src/append.lua", body)]);
    let m = shipment_with(
        "  - kind: patch_lua
    target: wifpmcinterior
    append: src/append.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    let found: Vec<_> = diags.iter().filter(|d| d.rule.code == "M0303").collect();
    assert_eq!(
        found.len(),
        2,
        "two bad keys; the shipped-shape ones must stay silent: {diags:?}"
    );
    assert!(
        found.iter().any(|d| d.message.contains("AbTest_C_Visible")),
        "literal-string key is surfaced: {found:?}"
    );
    assert!(
        found.iter().any(|d| d.message.contains("Pmc01Repeat")),
        "dot-ident key is surfaced: {found:?}"
    );
    assert!(lint::blocks_build(&diags));
}

/// Dynamic keys (`tMissionData[e.name] = ...` inside a loop) are invisible to the static scan
/// by design — M0303 is best-effort for the common literal-key pattern. Equality comparisons
/// (`if tMissionData.X == nil then`) and reads do not fire either.
#[test]
fn m0303_is_quiet_on_dynamic_keys_and_reads() {
    let body = "\
for _, e in ipairs(entries) do
  WifMissionData.tMissionData[e.name] = e.row
end
if WifMissionData.tMissionData[\"BadNameX\"] == nil then return end
local row = WifMissionData.tMissionData.BadNameX
";
    let root = scratch("m0303_quiet", &[("src/append.lua", body)]);
    let m = shipment_with(
        "  - kind: patch_lua
    target: wifpmcinterior
    append: src/append.lua
",
    );
    let diags = lint::lint(&m, Some(&root), None);
    assert!(
        !diags.iter().any(|d| d.rule.code == "M0303"),
        "dynamic keys and reads must stay silent: {diags:?}"
    );
}

/// Every new rule is registered, so `qm rules` can list them.
#[test]
fn the_lua_source_rules_are_registered() {
    for code in ["M0300", "M0301", "M0302"] {
        assert!(lint::RULES.iter().any(|r| r.code == code), "{code}");
    }
}
// ---------------------------------------------------------------------------
// M0213 — an animation's clip / trnm / events pairing
// ---------------------------------------------------------------------------

/// A real Havok 5.5 clip, shared with `mercs2_formats`' own tests.
const CLIP: &[u8] = include_bytes!("../../mercs2_formats/tests/fixtures/anim_ks750_le.bin");

fn trnm_of(count: u32) -> Vec<u8> {
    let mut t = count.to_le_bytes().to_vec();
    t.extend_from_slice(&0u32.to_le_bytes());
    for k in 0..count {
        t.extend_from_slice(&(0x1000 + k).to_le_bytes());
    }
    t
}

/// A scratch Shipment with `src/clip.hkx`, `src/clip.trnm` and optionally `src/clip.evnt`, linted
/// with an `add_animation` naming them.
fn lint_animation(label: &str, trnm: &[u8], events: Option<&[u8]>) -> Vec<lint::Diagnostic> {
    let root = std::env::temp_dir().join(format!("qm_lint_anim_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/clip.hkx"), CLIP).unwrap();
    std::fs::write(root.join("src/clip.trnm"), trnm).unwrap();
    let mut yaml = "  - kind: add_animation\n    name: my_clip\n    clip: src/clip.hkx\n    trnm: src/clip.trnm\n".to_string();
    if let Some(e) = events {
        std::fs::write(root.join("src/clip.evnt"), e).unwrap();
        yaml.push_str("    events: src/clip.evnt\n");
    }
    lint::lint(&shipment_with(&yaml), Some(&root), None)
}

fn clip_tracks() -> u32 {
    mercs2_formats::animgroup::read_clip_header(CLIP)
        .expect("fixture is a clip")
        .num_transform_tracks
}

#[test]
fn m0213_is_quiet_on_a_consistent_clip_trnm_and_events() {
    use mercs2_formats::anim_container::{build_evnt, AnimEvent};
    let evnt = build_evnt(&[
        AnimEvent { time: 0.0, name: "fol_rustle_human".into(), category: "sound".into() },
        AnimEvent { time: 0.2, name: "opendoor".into(), category: String::new() },
    ])
    .unwrap();
    let diags = lint_animation("ok", &trnm_of(clip_tracks()), Some(&evnt));
    assert!(diags.is_empty(), "{diags:?}");
}

#[test]
fn m0213_fires_when_the_trnm_count_is_not_the_clips_track_count() {
    let diags = lint_animation("count", &trnm_of(clip_tracks() + 2), None);
    assert_eq!(codes(&diags), vec!["M0213"], "{diags:?}");
    assert!(diags[0].message.contains("numTransformTracks"), "{}", diags[0].message);
    assert!(lint::blocks_build(&diags));
}

#[test]
fn m0213_fires_on_events_that_do_not_parse() {
    // Declares two events, carries one.
    let mut evnt = 2u32.to_le_bytes().to_vec();
    evnt.extend_from_slice(&0f32.to_le_bytes());
    evnt.extend_from_slice(b"a\0sound\0");
    let diags = lint_animation("evnt", &trnm_of(clip_tracks()), Some(&evnt));
    assert_eq!(codes(&diags), vec!["M0213"], "{diags:?}");
    assert!(diags[0].message.contains("evnt"), "{}", diags[0].message);
}

/// Without a root the files cannot be read, so the rule does not run rather than guess.
#[test]
fn m0213_needs_the_files() {
    let m = shipment_with(
        "  - kind: add_animation\n    name: c\n    clip: src/c.hkx\n    trnm: src/c.trnm\n",
    );
    assert!(lint::lint(&m, None, None).is_empty());
}

// ---------------------------------------------------------------------------
// M0162 / M0178 — add_runtime_dll
// ---------------------------------------------------------------------------

/// A header-only PE image with the given COFF machine and characteristics (the four fields
/// `pe::pe_dll_load_blocker` reads).
fn pe_image(machine: u16, characteristics: u16) -> Vec<u8> {
    let pe_at = 0x80usize;
    let mut out = vec![0u8; pe_at + 24];
    out[0..2].copy_from_slice(b"MZ");
    out[0x3C..0x40].copy_from_slice(&(pe_at as u32).to_le_bytes());
    out[pe_at..pe_at + 4].copy_from_slice(b"PE\0\0");
    out[pe_at + 4..pe_at + 6].copy_from_slice(&machine.to_le_bytes());
    out[pe_at + 22..pe_at + 24].copy_from_slice(&characteristics.to_le_bytes());
    out
}

/// M0162 needs only the manifest, so it fires with no root; M0178 needs the
/// bytes, so it fires only with one — and is quiet on a loadable i386 DLL.
#[test]
fn m0162_and_m0178_from_lint() {
    // Shipment `s`: its runtime DLL must be `s.dll`.
    let wrong_name = shipment_with("  - kind: add_runtime_dll\n    dll: src/m2-sdk.dll\n");
    let diags = lint::lint(&wrong_name, None, None);
    assert_eq!(codes(&diags), vec!["M0162"], "{diags:?}");
    assert!(diags[0].message.contains("`s.dll`"), "{}", diags[0].message);
    assert_eq!(diags[0].at, Some(0));

    let deny = shipment_with("  - kind: add_runtime_dll\n    dll: src/Cruise.dll\n");
    assert_eq!(codes(&lint::lint(&deny, None, None)), vec!["M0162"]);

    let right = shipment_with("  - kind: add_runtime_dll\n    dll: src/s.dll\n");
    assert!(lint::lint(&right, None, None).is_empty(), "no root: M0178 cannot run");

    let root = std::env::temp_dir().join(format!("qm_lint_rtdll_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/s.dll"), pe_image(0x8664, 0x2102)).unwrap();
    let diags = lint::lint(&right, Some(&root), None);
    assert_eq!(codes(&diags), vec!["M0178"], "{diags:?}");
    assert!(diags[0].message.contains("not i386"), "{}", diags[0].message);
    assert!(lint::blocks_build(&diags));

    std::fs::write(root.join("src/s.dll"), pe_image(0x014C, 0x2102)).unwrap();
    assert!(lint::lint(&right, Some(&root), None).is_empty(), "a loadable i386 DLL is clean");

    // A missing file is M0110's to report; M0178 does not also fire on it.
    std::fs::remove_file(root.join("src/s.dll")).unwrap();
    assert_eq!(codes(&lint::lint(&right, Some(&root), None)), vec!["M0110"]);
}

/// The finding element a `lint-report.json` carries: lint's four severities
/// map one to one, `at` becomes a `contributions` ref, and `fix` is carried through. No hermetic rule
/// emits HANG today (the HANG-class rules run against a built WAD), so the mapping is pinned here.
#[test]
fn a_diagnostic_becomes_the_shared_finding_element() {
    use mercs2_quartermaster::plan::{FindingSeverity, Section};
    let d = lint::Diagnostic {
        rule: lint::M0001_DANGLING_RUNG,
        severity: Severity::Hang,
        message: "m".into(),
        at: Some(3),
        fix: Some("f".into()),
    };
    let f = d.to_finding();
    assert_eq!(f.code, "M0001");
    assert_eq!(f.severity, FindingSeverity::Hang);
    assert_eq!(f.items, Vec::<String>::new());
    assert_eq!(f.refs.len(), 1);
    assert_eq!(f.refs[0].section, Section::Contributions);
    assert_eq!(f.refs[0].index, 3);
    assert_eq!(f.fix.as_deref(), Some("f"));
    let json = serde_json::to_value(&f).unwrap();
    assert_eq!(json["severity"], "hang");
    assert_eq!(json["refs"], serde_json::json!([{ "section": "contributions", "index": 3 }]));

    for (severity, wire) in [
        (Severity::Info, "info"),
        (Severity::Warning, "warning"),
        (Severity::Error, "error"),
    ] {
        let f = lint::Diagnostic { severity, at: None, fix: None, ..d.clone() }.to_finding();
        assert_eq!(serde_json::to_value(&f).unwrap()["severity"], wire);
        assert!(f.refs.is_empty(), "no `at`, no ref");
        assert_eq!(f.fix, None);
    }
}

// ---------------------------------------------------------------------------
// M0199 — native_hook signature guards (hermetic half; the exe-byte check is in game_checks)
// ---------------------------------------------------------------------------

/// A native_hook with `symbol` (so M0161 stays quiet) touching two addresses, plus whatever guard
/// block the test supplies.
fn hook(guard: &str) -> Manifest {
    shipment_with(&format!(
        "  - kind: native_hook
    target: retail
    symbol: SomeDetour
    touches: ['0x004CF340', '0x004CF400']
{guard}"
    ))
}

#[test]
fn m0199_guard_for_an_untouched_address_is_an_error() {
    let m = hook("    signature_guard:\n      '0x00DEAD00': '55 8B EC'\n");
    let diags = lint::lint(&m, None, None);
    let d = diags
        .iter()
        .find(|d| d.rule.code == "M0199")
        .unwrap_or_else(|| panic!("M0199 should fire, got {diags:?}"));
    assert_eq!(d.severity, Severity::Error);
    assert!(lint::blocks_build(&diags), "a guard for an un-touched address must block");
}

#[test]
fn m0199_malformed_prologue_bytes_are_an_error() {
    let m = hook("    signature_guard:\n      '0x004CF340': 'not hex'\n");
    let diags = lint::lint(&m, None, None);
    let d = diags
        .iter()
        .find(|d| d.rule.code == "M0199")
        .unwrap_or_else(|| panic!("M0199 should fire, got {diags:?}"));
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn m0199_partial_coverage_warns_on_the_unguarded_touch() {
    // One of two touched addresses guarded — the other is likely an oversight.
    let m = hook("    signature_guard:\n      '0x004CF340': '55 8B EC'\n");
    let diags = lint::lint(&m, None, None);
    let d = diags
        .iter()
        .find(|d| d.rule.code == "M0199")
        .unwrap_or_else(|| panic!("M0199 should warn, got {diags:?}"));
    assert_eq!(d.severity, Severity::Warning);
    assert!(!lint::blocks_build(&diags), "a coverage gap is advisory, not blocking");
    assert!(d.message.contains("0x004CF400"), "names the unguarded address: {}", d.message);
}

#[test]
fn m0199_is_quiet_with_no_guards_or_full_valid_coverage() {
    // Opt-out: declaring no guards at all is legitimate.
    assert!(
        !codes(&lint::lint(&hook(""), None, None)).contains(&"M0199"),
        "no guards must be silent"
    );
    // Full, well-formed coverage of every touched address.
    let full = hook(
        "    signature_guard:\n      '0x004CF340': '55 8B EC'\n      '0x004CF400': '53 56 57'\n",
    );
    assert!(
        !codes(&lint::lint(&full, None, None)).contains(&"M0199"),
        "full valid coverage must be silent"
    );
}

// ---------------------------------------------------------------------------
// Sound kinds (M0214–M0217)
// ---------------------------------------------------------------------------

/// Every field of one SoundCue, YAML at `indent`.
fn cue_fields(indent: &str, name: &str, wave: &str) -> String {
    [
        format!("name: {name}"),
        format!("wave: {wave}"),
        "group_gain_db: -4.0".into(),
        "cue_gain_db: -6.0".into(),
        "pitch_semitones: 0.0".into(),
        "positional: false".into(),
        "min_distance: 10.0".into(),
        "max_distance: 1000.0".into(),
        "distance_exponent: 1.0".into(),
        "doppler_scale: 1.0".into(),
        "start_limit: 0".into(),
        "sound_id: 0".into(),
        "priority: 0.95".into(),
        "group_20: 1.0".into(),
        "cue_16: 0".into(),
        "clip_hash: 0".into(),
    ]
    .iter()
    .map(|l| format!("{indent}{l}\n"))
    .collect()
}

/// An `add_sound` bank with one cue per name, all of category `category`.
fn add_sound(bank: &str, category: &str, cues: &[&str]) -> Manifest {
    let mut list = String::new();
    for c in cues {
        let f = cue_fields("        ", c, "src/a.wav");
        list.push_str(&format!("      - {}", &f[8..]));
    }
    let cues = if cues.is_empty() { "    cues: []\n".to_string() } else { format!("    cues:\n{list}") };
    shipment_with(&format!("  - kind: add_sound\n    bank: {bank}\n    category: {category}\n{cues}"))
}

/// A `replace_sound_cue` of `bank`, with `language` when given.
fn replace_cue(bank: &str, language: Option<&str>) -> Manifest {
    let language = language.map(|l| format!("    language: {l}\n")).unwrap_or_default();
    shipment_with(&format!(
        "  - kind: replace_sound_cue\n    bank: {bank}\n{language}    category: ui\n    cue:\n{}",
        cue_fields("      ", "ui_PDA_Open_01_st", "src/a.wav")
    ))
}

#[test]
fn a_well_formed_sound_bank_lints_clean() {
    let diags = lint::lint(&add_sound("mod_sounds", "ui", &["mod_click", "mod_whoosh"]), None, None);
    assert!(diags.is_empty(), "{diags:?}");
    assert!(lint::lint(&replace_cue("ui_hud", None), None, None).is_empty());
    assert!(lint::lint(&replace_cue("vo_mattias", Some("english")), None, None).is_empty());
}

/// M0214: a WAV the strict reader refuses blocks; a PCM16 one does not.
#[test]
fn m0214_fires_on_a_wav_the_reader_refuses() {
    let root = std::env::temp_dir().join(format!("qm_lint_m0214_{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    let wav = |bits: u16| {
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&40u32.to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&22050u32.to_le_bytes());
        w.extend_from_slice(&(22050u32 * u32::from(bits / 8)).to_le_bytes());
        w.extend_from_slice(&(bits / 8).to_le_bytes());
        w.extend_from_slice(&bits.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&4u32.to_le_bytes());
        w.extend_from_slice(&[1, 2, 3, 4]);
        w
    };
    let m = add_sound("mod_sounds", "ui", &["mod_click"]);
    std::fs::write(root.join("src/a.wav"), wav(8)).unwrap();
    let diags = lint::lint(&m, Some(&root), None);
    assert!(codes(&diags).contains(&"M0214"), "{diags:?}");
    assert!(lint::blocks_build(&diags));
    std::fs::write(root.join("src/a.wav"), wav(16)).unwrap();
    assert!(!codes(&lint::lint(&m, Some(&root), None)).contains(&"M0214"));
}

/// M0215: names the engine cannot reach — and none for usable ones.
#[test]
fn m0215_fires_on_unusable_sound_names() {
    // Two names that differ only in case hash alike.
    assert!(codes(&lint::lint(&add_sound("mod_sounds", "ui", &["Mod_Click", "mod_click"]), None, None)).contains(&"M0215"));
    // A bare hash, a padded name.
    assert!(codes(&lint::lint(&add_sound("mod_sounds", "ui", &["\"0x1234ABCD\""]), None, None)).contains(&"M0215"));
    assert!(codes(&lint::lint(&add_sound("mod_sounds", "ui", &["\" mod_click\""]), None, None)).contains(&"M0215"));
    assert!(codes(&lint::lint(&add_sound("\"0xDEADBEEF\"", "ui", &["mod_click"]), None, None)).contains(&"M0215"));
    // No cues, and an add_sound bank the loader would localize.
    assert!(codes(&lint::lint(&add_sound("mod_sounds", "ui", &[]), None, None)).contains(&"M0215"));
    assert!(codes(&lint::lint(&add_sound("vo_mine", "ui", &["mod_click"]), None, None)).contains(&"M0215"));
    // A usable bank is quiet.
    assert!(!codes(&lint::lint(&add_sound("mod_sounds", "ui", &["mod_click"]), None, None)).contains(&"M0215"));
}

/// M0216: a category outside the tree, with the nearest named one offered.
#[test]
fn m0216_fires_on_an_unknown_category_and_suggests_one() {
    let diags = lint::lint(&add_sound("mod_sounds", "weapn", &["mod_click"]), None, None);
    let d = diags.iter().find(|d| d.rule.code == "M0216").expect("M0216 fires");
    assert_eq!(d.fix.as_deref(), Some("weapon"));
    for name in mercs2_audio::encode::RETAIL_CATEGORY_NAMES {
        assert!(!codes(&lint::lint(&add_sound("mod_sounds", name, &["mod_click"]), None, None)).contains(&"M0216"), "{name}");
    }
}

/// M0217: a language on a bank that is not vo_*, or none on one that is.
#[test]
fn m0217_fires_when_the_language_does_not_match_the_bank() {
    assert!(codes(&lint::lint(&replace_cue("ui_hud", Some("english")), None, None)).contains(&"M0217"));
    assert!(codes(&lint::lint(&replace_cue("vo_mattias", None), None, None)).contains(&"M0217"));
    assert!(!codes(&lint::lint(&replace_cue("vo_mattias", Some("french")), None, None)).contains(&"M0217"));
}

/// The sound rules are registered with their doc anchors; the game-gated ones in GAME_RULES.
#[test]
fn the_sound_rules_are_registered() {
    for code in ["M0214", "M0215", "M0216", "M0217"] {
        let r = lint::RULES.iter().find(|r| r.code == code).unwrap_or_else(|| panic!("{code}"));
        assert_eq!(r.doc, format!("docs/modding/manifest_format.md#{}", code.to_lowercase()));
    }
    for code in ["M0218", "M0219", "M0220"] {
        assert!(lint::GAME_RULES.iter().any(|r| r.code == code), "{code}");
    }
}
