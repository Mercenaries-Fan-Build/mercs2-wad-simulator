//! Every rule's doc file exists and every anchor resolves, in the notes repo.
//!
//! The notes repo is not a dependency and is not on CI, so these are built by the `retail` feature
//! and run with `cargo xtask retail-test` on a machine that has the checkout. They FAIL when the
//! notes repo cannot be found: a check that passes when it cannot run is the same failure mode as a
//! rule that silently does not fire.

mod common {
    pub mod doc_links;
}

use common::doc_links::{every_rule, github_anchor};
use std::path::{Path, PathBuf};

/// The file every rule doc is checked against; its presence identifies a notes-repo root.
const MARKER: &str = "docs/modding/manifest_format.md";

/// The notes repo: `MERCS2_NOTES` when set, else the nearest ancestor of this crate that holds
/// [`MARKER`] (this workspace is checked out inside it at `tools/wad_simulator`). Panics, naming
/// both, when neither is a notes checkout.
fn notes_repo() -> PathBuf {
    if let Ok(p) = std::env::var("MERCS2_NOTES") {
        let p = PathBuf::from(p);
        assert!(
            p.join(MARKER).is_file(),
            "MERCS2_NOTES={} has no {MARKER}",
            p.display()
        );
        return p;
    }
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .find(|a| a.join(MARKER).is_file())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            panic!(
                "no notes repo: no ancestor of {} holds {MARKER}, and MERCS2_NOTES is unset",
                crate_dir.display()
            )
        })
}

#[test]
fn every_rule_doc_file_exists() {
    let notes = notes_repo();
    let mut missing = Vec::new();
    let mut checked = 0;
    for rule in every_rule() {
        let path = rule.doc.split('#').next().unwrap();
        checked += 1;
        if !notes.join(path).is_file() {
            missing.push(format!("{} -> {path}", rule.code));
        }
    }
    assert!(checked > 0, "no rules were checked — the test is not exercising anything");
    assert!(
        missing.is_empty(),
        "doc files that do not exist:\n  {}",
        missing.join("\n  ")
    );
    println!("doc_links: {checked} rule doc files exist under {}", notes.display());
}

/// An anchor that does not exist silently drops the reader at the top of a long document.
#[test]
fn every_rule_doc_anchor_exists() {
    let notes = notes_repo();
    let mut broken = Vec::new();
    let mut checked = 0;
    for rule in every_rule() {
        let (path, anchor) = match rule.doc.split_once('#') {
            Some(pair) => pair,
            None => continue, // a whole-document link has no anchor to resolve
        };
        let text = std::fs::read_to_string(notes.join(path))
            .unwrap_or_else(|e| panic!("{}: read {path}: {e}", rule.code));
        let anchors: Vec<String> = text
            .lines()
            .filter_map(|l| l.strip_prefix('#'))
            .map(|l| github_anchor(l.trim_start_matches('#')))
            .collect();
        checked += 1;
        if !anchors.iter().any(|a| a == anchor) {
            broken.push(format!("{} -> {path}#{anchor}", rule.code));
        }
    }
    assert!(
        checked > 0,
        "no anchored links were checked — the test is not exercising anything"
    );
    assert!(
        broken.is_empty(),
        "anchors that do not resolve (the link lands at the top of the page instead):\n  {}",
        broken.join("\n  ")
    );
    println!("doc_links: {checked} anchored rule links resolve");
}
