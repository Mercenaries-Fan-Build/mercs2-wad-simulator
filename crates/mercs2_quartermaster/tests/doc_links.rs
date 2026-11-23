//! Every rule's doc link must actually resolve.
//!
//! Diagnostics print a URL now, and a 404 in the output of a linter whose whole purpose is to
//! explain a trap is worse than printing nothing — it tells a modder the explanation exists and
//! then wastes their time.
//!
//! The rule docs are this repository's `docs/`, so every check here needs nothing but the checkout
//! and runs on CI: each rule's doc file exists, each anchor resolves, and the printed URL points at
//! the commit the docs were checked at.

mod common {
    pub mod doc_links;
}

use common::doc_links::{every_rule, github_anchor};
use std::path::{Path, PathBuf};

/// This repository's root: the crate is `crates/mercs2_quartermaster` in it.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The rule on headings whose anchors the rules link to.
#[test]
fn github_anchor_keeps_underscores_and_drops_punctuation() {
    assert_eq!(github_anchor("`edit_stringdb`"), "edit_stringdb");
    assert_eq!(github_anchor("`add_language`"), "add_language");
    assert_eq!(github_anchor("The Code layer"), "the-code-layer");
    assert_eq!(
        github_anchor("Trap 7 — Your reskin makes the game HANG"),
        "trap-7--your-reskin-makes-the-game-hang"
    );
    assert_eq!(github_anchor("1. SDK version — settled"), "1-sdk-version--settled");
}

/// The printed form must be a URL, not a path, into this repository at one commit: a path is only
/// meaningful to someone holding a checkout, and a branch moves under a released qm.
#[test]
fn diagnostics_print_a_url_at_one_commit() {
    let base = mercs2_quartermaster::lint::DOC_BASE;
    let rev = base
        .strip_prefix("https://github.com/Mercenaries-Fan-Build/mercs2-wad-simulator/blob/")
        .and_then(|r| r.strip_suffix('/'))
        .unwrap_or_else(|| panic!("DOC_BASE {base} is not this repository's blob URL"));
    assert!(rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()), "DOC_BASE names {rev:?}, not a commit");
    for rule in every_rule() {
        let url = rule.url();
        assert!(url.starts_with(base), "{}: {url}", rule.code);
        assert!(!url.contains(' '), "{}: url contains a space: {url}", rule.code);
    }
}

#[test]
fn every_rule_doc_file_exists() {
    let root = repo_root();
    let mut missing = Vec::new();
    let mut checked = 0;
    for rule in every_rule() {
        let path = rule.doc.split('#').next().unwrap();
        checked += 1;
        if !root.join(path).is_file() {
            missing.push(format!("{} -> {path}", rule.code));
        }
    }
    assert!(checked > 0, "no rules were checked — the test is not exercising anything");
    assert!(missing.is_empty(), "doc files that do not exist:\n  {}", missing.join("\n  "));
}

/// An anchor that does not exist silently drops the reader at the top of a long document.
#[test]
fn every_rule_doc_anchor_exists() {
    let root = repo_root();
    let mut broken = Vec::new();
    let mut checked = 0;
    for rule in every_rule() {
        let Some((path, anchor)) = rule.doc.split_once('#') else { continue };
        let text = std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{}: read {path}: {e}", rule.code));
        let anchors: Vec<String> = text.lines().filter_map(|l| l.strip_prefix('#')).map(|l| github_anchor(l.trim_start_matches('#'))).collect();
        checked += 1;
        if !anchors.iter().any(|a| a == anchor) {
            broken.push(format!("{} -> {path}#{anchor}", rule.code));
        }
    }
    assert!(checked > 0, "no anchored links were checked — the test is not exercising anything");
    assert!(
        broken.is_empty(),
        "anchors that do not resolve (the link lands at the top of the page instead):\n  {}",
        broken.join("\n  ")
    );
}
