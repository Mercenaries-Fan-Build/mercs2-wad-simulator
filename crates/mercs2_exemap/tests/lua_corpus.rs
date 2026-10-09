//! `corpora.tsv` / `wads.tsv` against the original files.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The originals are
//! found only through the repo-root `.mercs2-local.toml` keys `pc_demo_zip` (the public PC demo
//! archive), `wine_binary` (the `wine64` that runs its installer), `pc_official_install_dir` (the
//! official PC install), `pc_repack_install_dir` (the ElAmigos repack install), `x360_devkit_iso`
//! (the Xbox 360 preview disc), `x360_retail_dir` (the Xbox 360 retail disc dump),
//! `x360_dlc_biua_stfs` (the retail Blow It Up Again Pack) and `x360_dlc_prototypes_dir` (the zipped
//! DLC prototypes); a missing key or path fails the test naming it.
//!
//! Every WAD of the PC demo corpus is rebuilt from the demo archive, with every intermediate sha256
//! checked, in a fresh work directory under the system temp directory; every WAD of the PC retail and
//! repack corpora is checked in place in the install its `install` column names.

use std::path::Path;

use std::collections::BTreeMap;
use std::path::PathBuf;

use mercs2_exemap::corpus::{check_installed, derive, file_sha256, Corpora, Env};
use mercs2_formats::game_paths::{
    local_config_pc_demo_zip, local_config_pc_official_install_dir, local_config_pc_repack_install_dir,
    local_config_wine_binary, local_config_x360_devkit_iso, local_config_x360_dlc_biua_stfs,
    local_config_x360_dlc_prototypes_dir, local_config_x360_retail_dir,
};

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

fn install_dir(key: &str) -> std::path::PathBuf {
    let r = match key {
        "pc_official_install_dir" => local_config_pc_official_install_dir(start()),
        "pc_repack_install_dir" => local_config_pc_repack_install_dir(start()),
        "x360_retail_dir" => local_config_x360_retail_dir(start()),
        other => panic!("wads.tsv names install key {other}, which this test does not resolve"),
    };
    r.unwrap_or_else(|e| panic!("{e}"))
}

fn check_corpus(corpus: &str, expect: usize) {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let mut n = 0;
    for wad in corpora.wads_of(corpus) {
        let key = wad.install.as_deref().unwrap_or_else(|| panic!("{corpus} {} has a chain, not an install", wad.name));
        check_installed(wad, &install_dir(key)).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert_eq!(n, expect, "{corpus} lists {expect} WADs");
}

#[test]
fn every_pc_retail_wad_is_in_its_install() {
    check_corpus("base-pc-retail", 8);
}

#[test]
fn every_pc_repack_wad_is_in_the_repack_install() {
    check_corpus("base-pc-elamigos", 8);
}

#[test]
fn every_x360_retail_wad_is_in_the_retail_dump() {
    check_corpus("base-x360-retail", 5);
}

/// Rebuild every WAD of the corpora whose ids start with `prefix` from the containers given,
/// matched to each chain by sha256.
fn derive_all(prefix: &str, containers: Vec<PathBuf>, expect: usize) {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let mut by_sha = BTreeMap::new();
    for p in containers {
        by_sha.insert(file_sha256(&p).unwrap_or_else(|e| panic!("{e}")), p);
    }
    let work = std::env::temp_dir().join(format!("mercs2_exemap_lua_corpus_{prefix}"));
    std::fs::remove_dir_all(&work).ok();
    let env = Env { wine: None, work: work.clone() };
    let mut n = 0;
    for wad in corpora.wads.iter().filter(|w| w.corpus.starts_with(prefix)) {
        let head = &wad.chain[0].from_sha256;
        let path = by_sha
            .get(head)
            .unwrap_or_else(|| panic!("{} {}: no container with sha256 {head} among the files the keys name", wad.corpus, wad.name));
        derive(wad, path, &env).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert_eq!(n, expect, "{prefix}* lists {expect} WADs");
    std::fs::remove_dir_all(&work).ok();
}

#[test]
fn every_x360_preview_wad_rebuilds_from_the_preview_disc() {
    let iso = local_config_x360_devkit_iso(start()).unwrap_or_else(|e| panic!("{e}"));
    derive_all("base-x360-preview", vec![iso], 5);
}

#[test]
fn every_x360_dlc_package_rebuilds_from_its_original() {
    let mut containers = vec![local_config_x360_dlc_biua_stfs(start()).unwrap_or_else(|e| panic!("{e}"))];
    let dir = local_config_x360_dlc_prototypes_dir(start()).unwrap_or_else(|e| panic!("{e}"));
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("list {}: {e}", dir.display())) {
        let p = entry.unwrap_or_else(|e| panic!("list {}: {e}", dir.display())).path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) {
            containers.push(p);
        }
    }
    derive_all("dlc-", containers, 5);
}
