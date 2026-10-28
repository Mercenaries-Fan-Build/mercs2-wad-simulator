//! Names are the identity; the hash is only the normalized comparison key.
//!
//! Two properties these pin down:
//!
//! * Both spellings of one asset are the SAME claim — otherwise writing the hash would evade
//!   conflict detection.
//! * A hash an author wrote by hand is resolved BACK to its name everywhere a human reads it, and
//!   the author is told to write the name instead.

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_quartermaster::blast::{self, Claim};
use mercs2_quartermaster::names::{bare_hash_suggestions, NameTable, PRODUCTION_NAMES};
use mercs2_quartermaster::{from_str, Format, Manifest};

const DESTROYER: &str = "al_veh_boat_destroyer";
const DESTROYER_HASH: u32 = 0xE540_47D5;

fn raw_touching(shipment: &str, touch: &str) -> Manifest {
    from_str(
        &format!(
            "format: 2
shipment: {{ name: {shipment}, version: 1.0.0, target: retail }}
contributions:
  - kind: raw
    payload: src/b.bin
    target_layer: data
    touches: [\"{touch}\"]
"
        ),
        Format::Yaml,
    )
    .expect("fixture must parse")
}

fn table() -> NameTable {
    NameTable::from_pairs([(DESTROYER_HASH, DESTROYER)])
}

/// The rule: a value starting with `0x` and made of hex digits is an ALREADY-HASHED value; anything
/// else is a name to be hashed. Both land on the same claim.
#[test]
fn a_name_and_its_hash_produce_one_claim() {
    let by_name = raw_touching("a", DESTROYER);
    let by_hash = raw_touching("b", "0xE54047D5");

    let claim_of = |m: &Manifest| blast::claims(m).into_iter().next().unwrap().claim;
    assert_eq!(claim_of(&by_name), claim_of(&by_hash));
    assert_eq!(
        claim_of(&by_name),
        Claim::Asset {
            hash: pandemic_hash_m2(DESTROYER)
        }
    );
}

#[test]
fn hash_spelling_is_case_insensitive() {
    let lower = raw_touching("a", "0xe54047d5");
    let upper = raw_touching("b", "0xE54047D5");
    let claim_of = |m: &Manifest| blast::claims(m).into_iter().next().unwrap().claim;
    assert_eq!(claim_of(&lower), claim_of(&upper));
}

/// A name that merely looks hexy is still a NAME — it lacks the `0x` prefix, so it gets hashed.
#[test]
fn a_hexish_name_without_the_prefix_is_hashed_as_a_name() {
    let m = raw_touching("a", "deadbeef");
    let claim = blast::claims(&m).into_iter().next().unwrap().claim;
    assert_eq!(
        claim,
        Claim::Asset {
            hash: pandemic_hash_m2("deadbeef")
        }
    );
    assert_ne!(claim, Claim::Asset { hash: 0xDEAD_BEEF });
}

/// Writing the hash must not let a Shipment slip past conflict detection.
#[test]
fn the_two_spellings_still_conflict_with_each_other() {
    let by_name = raw_touching("a", DESTROYER);
    let by_hash = raw_touching("b", "0xE54047D5");
    let found = blast::conflicts(&[("a", &by_name), ("b", &by_hash)]);
    assert_eq!(found.len(), 1, "got {found:?}");
}

// ---------------------------------------------------------------------------
// Resolving back to names for humans
// ---------------------------------------------------------------------------

/// A hand-written hash should not stay a hash in anything a person reads.
#[test]
fn a_bare_hash_claim_is_named_in_diagnostics() {
    let m = raw_touching("a", "0xE54047D5");
    let mut records = blast::claims(&m);
    assert_eq!(records[0].name, None, "nothing to go on before enrichment");

    table().enrich(&mut records);
    assert_eq!(records[0].name.as_deref(), Some(DESTROYER));
    assert!(
        records[0]
            .claim
            .describe(records[0].name.as_deref())
            .contains(DESTROYER),
        "the human-readable label must lead with the name"
    );
}

#[test]
fn enrichment_never_overwrites_a_name_the_author_wrote() {
    let m = raw_touching("a", DESTROYER);
    let mut records = blast::claims(&m);
    NameTable::from_pairs([(DESTROYER_HASH, "some_other_name")]).enrich(&mut records);
    assert_eq!(records[0].name.as_deref(), Some(DESTROYER));
}

#[test]
fn an_unknown_hash_stays_unnamed_rather_than_guessing() {
    let m = raw_touching("a", "0x00000001");
    let mut records = blast::claims(&m);
    table().enrich(&mut records);
    assert_eq!(records[0].name, None);
    assert!(records[0].claim.label().contains("0x00000001"));
}

// ---------------------------------------------------------------------------
// Telling the author to write the name
// ---------------------------------------------------------------------------

/// A bare hash is legal ONLY when no name is known. This is what makes that enforceable.
#[test]
fn writing_a_known_hash_is_flagged_with_the_name_to_use() {
    let m = raw_touching("a", "0xE54047D5");
    let suggestions = bare_hash_suggestions(&m, &table());
    assert_eq!(suggestions.len(), 1);
    assert_eq!(suggestions[0].written, "0xE54047D5");
    assert_eq!(suggestions[0].name, DESTROYER);
    // Auto-fixable: the message has to carry the replacement text.
    assert!(
        suggestions[0].to_string().contains(DESTROYER),
        "{}",
        suggestions[0]
    );
}

#[test]
fn writing_the_name_is_not_flagged() {
    let m = raw_touching("a", DESTROYER);
    assert!(bare_hash_suggestions(&m, &table()).is_empty());
}

/// The documented escape: a hash with no known name is legal and must stay quiet, or the escape
/// hatch becomes unusable.
#[test]
fn an_unnameable_hash_is_not_nagged_about() {
    let m = raw_touching("a", "0x00000001");
    assert!(bare_hash_suggestions(&m, &table()).is_empty());
}

/// A sound override written by hash: a known cue hash is offered its name, and a localized bank's
/// hash — its entry's, `<bank>.<language>` — is offered the bank name without the suffix, which is
/// what the manifest writes beside `language`. An entry name that does not end in the declared
/// language is not offered (the hash is not that language's copy).
#[test]
fn a_hashed_sound_override_is_offered_its_names() {
    let manifest = |bank: u32, language: &str| {
        from_str(
            &format!(
                "format: 2
shipment: {{ name: a, version: 1.0.0, target: retail }}
contributions:
  - kind: replace_sound_cue
    bank: \"0x{bank:08X}\"
    language: {language}
    category: vo
    cue:
      name: \"0x{:08X}\"
      wave: src/x.wav
      group_gain_db: 0
      cue_gain_db: 0
      pitch_semitones: 0
      positional: false
      min_distance: 1
      max_distance: 2
      distance_exponent: 1
      doppler_scale: 1
      start_limit: 0
      sound_id: 0
      priority: 1
      group_20: 1
      cue_16: 0
      clip_hash: 0
",
                pandemic_hash_m2("chris-Banter-Contract-All01-29")
            ),
            Format::Yaml,
        )
        .unwrap()
    };
    let entry = pandemic_hash_m2("vo_chris.english");
    let names = NameTable::from_pairs([
        (entry, "vo_chris.english"),
        (pandemic_hash_m2("chris-Banter-Contract-All01-29"), "chris-Banter-Contract-All01-29"),
    ]);
    let s = bare_hash_suggestions(&manifest(entry, "english"), &names);
    let offered: Vec<(&str, &str)> = s.iter().map(|s| (s.written.as_str(), s.name.as_str())).collect();
    assert!(offered.contains(&(format!("0x{entry:08X}").as_str(), "vo_chris")), "{offered:?}");
    assert!(offered.iter().any(|(_, n)| *n == "chris-Banter-Contract-All01-29"), "{offered:?}");
    let german = bare_hash_suggestions(&manifest(entry, "german"), &names);
    assert!(!german.iter().any(|s| s.name.starts_with("vo_chris")), "{german:?}");
}

/// A native hook's `touches` are CODE ADDRESSES, not asset hashes. Reversing one through the asset
/// name table would produce a confident, wrong suggestion.
#[test]
fn native_hook_addresses_are_never_suggested_as_asset_names() {
    let m = from_str(
        "format: 2
shipment: { name: a, version: 1.0.0, target: retail }
contributions:
  - kind: native_hook
    target: retail
    plugin: src/x.asi
    touches: [\"0xE54047D5\"]
",
        Format::Yaml,
    )
    .unwrap();
    assert!(
        bare_hash_suggestions(&m, &table()).is_empty(),
        "a code address must not be renamed to an asset"
    );
}

// ---------------------------------------------------------------------------
// The real table
// ---------------------------------------------------------------------------

/// Against the committed 23,110-entry lookup. Skips rather than fails when the crate is built
/// outside the workspace, since a missing table degrades diagnostics but is never fatal.
#[test]
fn the_committed_table_reverses_a_known_asset() {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The table is committed in this workspace, so not finding it is a failure, not a skip.
    let names = NameTable::find_from(here)
        .unwrap_or_else(|| panic!("no {} found walking up from {}", PRODUCTION_NAMES, here.display()));
    assert!(
        names.len() > 20_000,
        "expected the full table, got {}",
        names.len()
    );
    assert_eq!(names.reverse(DESTROYER_HASH), Some(DESTROYER));
    // And the pairing that motivated names-only `touches` in the first place.
    assert_eq!(
        names.reverse(pandemic_hash_m2("ch_veh_boat_destroyer")),
        Some("ch_veh_boat_destroyer")
    );
}

use std::path::Path;
