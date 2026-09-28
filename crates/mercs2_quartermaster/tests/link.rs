//! The linker's hermetic tests: what it can be checked on without a game install.
//!
//! The tests that link against the retail scripts blocks — including the one that installs two
//! script mods and checks both survive — are game-gated and live in `link_retail.rs`.

mod common {
    pub mod corpus;
}

use common::corpus::corpus_root;
use mercs2_quartermaster::link;

#[test]
fn the_corpus_lookup_finds_a_vz_script_and_reports_what_it_tried() {
    let Some(corpus) = corpus_root() else { return };
    let found =
        link::base_source_path(&corpus, "wifpmcinterior").expect("wifpmcinterior is in vz/");
    assert!(
        found.ends_with("vz/wifpmcinterior.lua"),
        "{}",
        found.display()
    );

    let tried = link::base_source_path(&corpus, "definitely_not_a_script").unwrap_err();
    assert!(
        tried.len() >= 3,
        "should report every location it searched: {tried:?}"
    );
}
