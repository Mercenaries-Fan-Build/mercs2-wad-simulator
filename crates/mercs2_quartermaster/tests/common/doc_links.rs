//! Shared by `doc_links.rs` (hermetic) and `doc_links_retail.rs` (needs the notes checkout).

use mercs2_quartermaster::lint;

/// GitHub's heading→anchor rule, as `github-slugger` (the slugger GitHub's renderer uses) applies
/// it: lowercase; remove every character that is not a letter, a number, connector punctuation
/// (`_`), a space or a hyphen; then each space becomes a hyphen.
///
/// `_` is KEPT — `### \`edit_stringdb\`` is `#edit_stringdb` — and the backticks around it are
/// dropped like any other punctuation. The em dashes in these headings are what make the anchors
/// long and ugly: `## Trap 7 — Your…` becomes `trap-7--your…`, with the doubled hyphen coming from
/// the dropped dash plus its spaces. That is exactly why this test exists — nobody derives those by
/// hand correctly.
pub fn github_anchor(heading: &str) -> String {
    heading
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '_' | ' ' | '-'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

pub fn every_rule() -> Vec<lint::Rule> {
    lint::RULES
        .iter()
        .chain(lint::PENDING)
        .chain(lint::ARTIFACT_RULES)
        .chain(lint::GAME_RULES)
        .copied()
        .collect()
}
