//! Every rule's doc link must actually resolve.
//!
//! Diagnostics print a URL now, and a 404 in the output of a linter whose whole purpose is to
//! explain a trap is worse than printing nothing — it tells a modder the explanation exists and
//! then wastes their time.
//!
//! The checks here need nothing but the crate. The two that read the notes repo — every doc file
//! exists, every anchor resolves — live in `doc_links_retail.rs`, built by the `retail` feature
//! (`cargo xtask retail-test`), because the notes repo is not present on CI. They fail rather than
//! skip when it cannot be found.

mod common {
    pub mod doc_links;
}

use common::doc_links::{every_rule, github_anchor};

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
