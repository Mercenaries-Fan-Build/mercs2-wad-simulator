//! Every rule's doc link must actually resolve.
//!
//! Diagnostics print a URL now, and a 404 in the output of a linter whose whole purpose is to
//! explain a trap is worse than printing nothing — it tells a modder the explanation exists and
//! then wastes their time.
//!
//! The links point at the notes repo, which is not a dependency and is not present on CI, so these
//! skip loudly when it is absent. That is deliberate: a check that silently passes when it cannot
//! run is the same failure mode as a rule that silently does not fire.
//!
//! Set `MERCS2_NOTES` to point at a checkout, or have it at `~/src/mercenaries-game`.

use mercs2_quartermaster::lint;
use std::path::PathBuf;

/// A checkout of the notes repo, or None.
fn notes_repo() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MERCS2_NOTES") {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join("src/mercenaries-game");
    p.is_dir().then_some(p)
}

/// GitHub's heading→anchor rule: lowercase, drop everything that is not alphanumeric/space/hyphen,
/// then spaces to hyphens.
///
/// The em dashes in these headings are what make the anchors long and ugly: `## Trap 7 — Your…`
/// becomes `trap-7--your…`, with the doubled hyphen coming from the dropped dash plus its spaces.
/// That is exactly why this test exists — nobody derives those by hand correctly.
fn github_anchor(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn every_rule() -> Vec<lint::Rule> {
    lint::RULES
        .iter()
        .chain(lint::PENDING)
        .chain(lint::ARTIFACT_RULES)
        .copied()
        .chain([lint::M0007_MULTI_RUNG_REPLACE, lint::M0009_NO_PRIMARY_ROW])
        .collect()
}

#[test]
fn every_rule_doc_file_exists() {
    let Some(notes) = notes_repo() else {
        eprintln!("SKIP: no notes repo — set MERCS2_NOTES or clone to ~/src/mercenaries-game");
        return;
    };
    let mut missing = Vec::new();
    for rule in every_rule() {
        let path = rule.doc.split('#').next().unwrap();
        if !notes.join(path).is_file() {
            missing.push(format!("{} -> {path}", rule.code));
        }
    }
    assert!(
        missing.is_empty(),
        "doc files that do not exist:\n  {}",
        missing.join("\n  ")
    );
}

/// An anchor that does not exist silently drops the reader at the top of a long document.
#[test]
fn every_rule_doc_anchor_exists() {
    let Some(notes) = notes_repo() else {
        eprintln!("SKIP: no notes repo — set MERCS2_NOTES or clone to ~/src/mercenaries-game");
        return;
    };
    let mut broken = Vec::new();
    let mut checked = 0;
    for rule in every_rule() {
        let (path, anchor) = match rule.doc.split_once('#') {
            Some(pair) => pair,
            None => continue, // whole-document link
        };
        let Ok(text) = std::fs::read_to_string(notes.join(path)) else {
            continue; // reported by the file test
        };
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
}

/// The printed form must be a URL, not a path. A path is only meaningful to someone holding a
/// checkout of a repo they have no reason to have.
#[test]
fn diagnostics_print_a_url() {
    for rule in every_rule() {
        let url = rule.url();
        assert!(url.starts_with("https://"), "{}: {url}", rule.code);
        assert!(
            !url.contains(" "),
            "{}: url contains a space: {url}",
            rule.code
        );
    }
}
