//! `corpora.tsv` / `wads.tsv` against the original files.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The originals are
//! found only through the repo-root `.mercs2-local.toml` keys `pc_demo_zip` (the public PC demo
//! archive) and `wine_binary` (the `wine64` that runs its installer); a missing key or path fails the
//! test naming it.
//!
//! Every WAD of the PC demo corpus is rebuilt from the demo archive, with every intermediate sha256
//! checked, in a fresh work directory under the system temp directory.

use std::path::Path;

use mercs2_exemap::corpus::{derive, Corpora, Env};
use mercs2_formats::game_paths::{local_config_pc_demo_zip, local_config_wine_binary};

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn every_pc_demo_wad_rebuilds_from_the_demo_archive() {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let zip = local_config_pc_demo_zip(start()).unwrap_or_else(|e| panic!("{e}"));
    let wine = local_config_wine_binary(start()).unwrap_or_else(|e| panic!("{e}"));
    let work = std::env::temp_dir().join("mercs2_exemap_lua_corpus_pc_demo");
    std::fs::remove_dir_all(&work).ok();
    let env = Env { wine: Some(wine), work: work.clone() };
    let mut n = 0;
    for wad in corpora.wads_of("base-pc-demo") {
        derive(wad, &zip, &env).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert_eq!(n, 4, "base-pc-demo lists four WADs");
    std::fs::remove_dir_all(&work).ok();
}
