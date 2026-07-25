//! Discovery + source-path checks. All hermetic — this is the set `qm lint` runs in template CI,
//! where the retail WADs will never exist.

use mercs2_quartermaster::discover::{self, DiscoverError, SourceIssue};
use mercs2_quartermaster::Format;
use std::path::{Path, PathBuf};

/// Workspace idiom (see `mercs2_script`): a temp dir keyed by pid, plus a per-test label so
/// concurrently-running tests cannot collide.
fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm_test_{}_{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

const MINIMAL_YAML: &str = "\
format: 1
shipment:
  name: boss-reskin
  version: 1.0.0
  target: retail
contributions:
  - kind: replace_texture
    target: al_hum_boss_ub
    image: src/boss_ub.png
";

const MINIMAL_JSON: &str = r#"{"format":1,"shipment":{"name":"boss-reskin","version":"1.0.0","target":"retail"},"contributions":[]}"#;

fn write(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).expect("write fixture");
}

#[test]
fn finds_a_single_manifest_and_reports_its_format() {
    for (name, expected) in [
        ("manifest.yaml", Format::Yaml),
        ("manifest.yml", Format::Yaml),
        ("manifest.json", Format::Json),
    ] {
        let dir = scratch(&format!("single_{}", name.replace('.', "_")));
        let body = if expected == Format::Json { MINIMAL_JSON } else { MINIMAL_YAML };
        write(&dir, name, body);

        let (path, format) = discover::find_manifest(&dir).expect("should find one manifest");
        assert_eq!(path.file_name().unwrap(), name);
        assert_eq!(format, expected);
    }
}

/// The rule the format cares most about: never silently pick one.
#[test]
fn multiple_manifests_are_an_ambiguity_error() {
    let dir = scratch("ambiguous");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    write(&dir, "manifest.json", MINIMAL_JSON);

    let err = discover::find_manifest(&dir).expect_err("two manifests must be an error");
    match &err {
        DiscoverError::Ambiguous { found, .. } => assert_eq!(found.len(), 2),
        other => panic!("expected Ambiguous, got {other:?}"),
    }
    // The message must name the offenders — an author has to know which files to reconcile.
    let msg = err.to_string();
    assert!(msg.contains("manifest.yaml"), "{msg}");
    assert!(msg.contains("manifest.json"), "{msg}");
    assert!(msg.contains("refusing to guess"), "{msg}");
}

/// `.yaml` and `.yml` are the same format but still two files — a real way to confuse yourself.
#[test]
fn yaml_and_yml_together_are_still_ambiguous() {
    let dir = scratch("yaml_and_yml");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    write(&dir, "manifest.yml", MINIMAL_YAML);
    assert!(matches!(
        discover::find_manifest(&dir),
        Err(DiscoverError::Ambiguous { .. })
    ));
}

#[test]
fn missing_manifest_lists_what_was_expected() {
    let dir = scratch("empty");
    let err = discover::find_manifest(&dir).expect_err("no manifest");
    let msg = err.to_string();
    assert!(msg.contains("manifest.yaml"), "should list accepted names: {msg}");
    assert!(msg.contains("manifest.toml"), "should list accepted names: {msg}");
}

#[test]
fn a_file_is_not_a_shipment_root() {
    let dir = scratch("not_a_dir");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    assert!(matches!(
        discover::find_manifest(&dir.join("manifest.yaml")),
        Err(DiscoverError::NotADirectory(_))
    ));
}

/// Unrelated files must not be mistaken for a manifest.
#[test]
fn only_the_manifest_stem_counts() {
    let dir = scratch("other_files");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    write(&dir, "README.md", "hi");
    write(&dir, "notes.yaml", "a: 1");
    write(&dir, "manifest.txt", "not a manifest");
    let (path, _) = discover::find_manifest(&dir).expect("one manifest");
    assert_eq!(path.file_name().unwrap(), "manifest.yaml");
}

#[test]
fn open_reads_parses_and_validates() {
    let dir = scratch("open_ok");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    let shipment = discover::open(&dir).expect("open");
    assert_eq!(shipment.manifest.shipment.name, "boss-reskin");
    assert_eq!(shipment.format, Format::Yaml);
    assert_eq!(shipment.manifest.contributions.len(), 1);
}

#[test]
fn open_surfaces_validation_failures() {
    let dir = scratch("open_bad");
    write(&dir, "manifest.yaml", &MINIMAL_YAML.replace("format: 1", "format: 99"));
    let err = discover::open(&dir).expect_err("future format");
    assert!(err.to_string().contains("refusing to guess"), "{err}");
}

// ---------------------------------------------------------------------------
// Source paths
// ---------------------------------------------------------------------------

#[test]
fn every_referenced_path_is_collected_with_its_field() {
    let text = "\
format: 1
shipment: { name: s, version: 1.0.0, target: retail }
contributions:
  - kind: add_outfit
    name: sean_devlin
    slug: SeanDevlin
    display: Sean Devlin
    wearer: mattias
    model: src/sean/sean.glb
    textures:
      diffuse: src/sean/sean_d.png
      normal: src/sean/sean_n.png
  - kind: raw
    payload: src/raw/blob.bin
    target_layer: data
    touches: [\"al_veh_boat_destroyer\"]
";
    let m = mercs2_quartermaster::from_str(text, Format::Yaml).unwrap();
    let refs = discover::source_refs(&m);
    let fields: Vec<_> = refs.iter().map(|r| (r.index, r.field)).collect();
    assert_eq!(
        fields,
        vec![
            (0, "model"),
            (0, "textures.diffuse"),
            (0, "textures.normal"),
            (1, "payload"),
        ]
    );
    // An omitted optional texture contributes nothing.
    assert!(!refs.iter().any(|r| r.field == "textures.specular"));
}

#[test]
fn a_present_source_under_src_is_clean() {
    let dir = scratch("sources_ok");
    write(&dir, "manifest.yaml", MINIMAL_YAML);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    write(&dir.join("src"), "boss_ub.png", "fake png");

    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert!(issues.is_empty(), "expected no issues, got {issues:?}");
}

#[test]
fn a_missing_source_is_reported_with_its_position() {
    let dir = scratch("sources_missing");
    write(&dir, "manifest.yaml", MINIMAL_YAML);

    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    match issues.as_slice() {
        [SourceIssue::Missing { index, kind, field, .. }] => {
            assert_eq!((*index, *kind, *field), (0, "replace_texture", "image"));
        }
        other => panic!("expected one Missing, got {other:?}"),
    }
    assert!(issues[0].to_string().contains("contributions[0]"), "{}", issues[0]);
}

/// The Quartermaster reads and copies these files; climbing out of the Shipment must be rejected
/// even when the target exists.
#[test]
fn a_path_escaping_the_root_is_rejected() {
    let dir = scratch("sources_escape");
    let outside = dir.join("outside.png");
    std::fs::write(&outside, "secret").unwrap();
    let root = dir.join("shipment");
    std::fs::create_dir_all(&root).unwrap();
    write(
        &root,
        "manifest.yaml",
        &MINIMAL_YAML.replace("src/boss_ub.png", "../outside.png"),
    );

    let shipment = discover::open(&root).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert!(
        matches!(issues.as_slice(), [SourceIssue::EscapesRoot { .. }]),
        "got {issues:?}"
    );
}

#[test]
fn an_absolute_path_is_rejected() {
    let dir = scratch("sources_absolute");
    write(
        &dir,
        "manifest.yaml",
        &MINIMAL_YAML.replace("src/boss_ub.png", "/etc/hosts"),
    );
    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert!(
        matches!(issues.as_slice(), [SourceIssue::Absolute { .. }]),
        "got {issues:?}"
    );
}

/// `src/x/../boss_ub.png` stays inside — normalization must not over-reject.
#[test]
fn interior_parent_segments_are_fine() {
    let dir = scratch("sources_interior");
    std::fs::create_dir_all(dir.join("src/x")).unwrap();
    write(&dir.join("src"), "boss_ub.png", "fake png");
    write(
        &dir,
        "manifest.yaml",
        &MINIMAL_YAML.replace("src/boss_ub.png", "src/x/../boss_ub.png"),
    );
    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert!(issues.is_empty(), "got {issues:?}");
}

/// Contained and present, but not under `src/` — a convention violation, reported separately from
/// the safety ones so the linter can grade it differently.
#[test]
fn a_source_outside_src_is_reported_separately() {
    let dir = scratch("sources_outside_src");
    write(&dir, "boss_ub.png", "fake png");
    write(
        &dir,
        "manifest.yaml",
        &MINIMAL_YAML.replace("src/boss_ub.png", "boss_ub.png"),
    );
    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert!(
        matches!(issues.as_slice(), [SourceIssue::OutsideSrc { .. }]),
        "got {issues:?}"
    );
}

/// All issues are reported, not just the first — an author fixing a manifest wants the whole list.
#[test]
fn checking_does_not_short_circuit() {
    let text = "\
format: 1
shipment: { name: s, version: 1.0.0, target: retail }
contributions:
  - kind: replace_texture
    target: a
    image: src/missing_one.png
  - kind: replace_texture
    target: b
    image: /absolute.png
  - kind: replace_texture
    target: c
    image: ../escaping.png
";
    let dir = scratch("sources_many");
    write(&dir, "manifest.yaml", text);
    let shipment = discover::open(&dir).unwrap();
    let issues = discover::check_sources(&shipment.manifest, &shipment.root);
    assert_eq!(issues.len(), 3, "got {issues:?}");
    assert!(matches!(issues[0], SourceIssue::Missing { index: 0, .. }));
    assert!(matches!(issues[1], SourceIssue::Absolute { index: 1, .. }));
    assert!(matches!(issues[2], SourceIssue::EscapesRoot { index: 2, .. }));
}
