//! `corpora.tsv` / `wads.tsv` against the original files.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The originals are
//! found only through the repo-root `.mercs2-local.toml` keys `pc_demo_zip` (the public PC demo
//! archive), `wine_binary` (the `wine64` that runs its installer), `pc_official_install_dir` (the
//! official PC install), `pc_repack_install_dir` (the ElAmigos repack install), `x360_devkit_iso`
//! (the Xbox 360 preview disc), `x360_retail_dir` (the Xbox 360 retail disc dump),
//! `x360_dlc_biua_stfs` (the retail Blow It Up Again Pack), `x360_dlc_prototypes_dir` (the zipped
//! DLC prototypes), `ps3_disc_iso` (the PS3 NA disc image), `ps3_jp_disc_dir` (the PS3 Japanese disc dump), `ps3_update_pkgs` (the folder of PS3
//! update packages) and `ps3_dlc_biua_pkg` (the PS3 NA Blow It Up Again Pack); a missing key or path
//! fails the test naming it.
//!
//! Every WAD of the PC demo corpus is rebuilt from the demo archive, with every intermediate sha256
//! checked, in a fresh work directory under the system temp directory; every WAD with an `install`
//! column (the PC retail and repack corpora, the Xbox 360 retail dump, the PS3 Japanese dump) is
//! checked in place in the install that column names.

use std::path::Path;

use std::collections::BTreeMap;
use std::path::PathBuf;

use mercs2_exemap::corpus::{check_installed, derive, file_sha256, Corpora, Corpus, Env};
use mercs2_formats::game_paths::{
    local_config_pc_demo_zip, local_config_pc_official_install_dir, local_config_pc_repack_install_dir,
    local_config_ps3_disc_iso, local_config_ps3_dlc_biua_pkg, local_config_ps3_jp_disc_dir, local_config_ps3_update_pkgs, local_config_wine_binary, local_config_x360_devkit_iso, local_config_x360_dlc_biua_stfs,
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
        "ps3_jp_disc_dir" => local_config_ps3_jp_disc_dir(start()),
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

#[test]
fn every_ps3_jp_wad_is_in_the_jp_disc_dump() {
    check_corpus("base-ps3-retail-jp", 5);
}

/// Rebuild every WAD of the corpora `select` picks (named `what` in messages) from the containers
/// given, matched to each chain by sha256.
fn derive_all(what: &str, select: impl Fn(&Corpus) -> bool, containers: Vec<PathBuf>, expect: usize) {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let mut by_sha = BTreeMap::new();
    for p in containers {
        by_sha.insert(file_sha256(&p).unwrap_or_else(|e| panic!("{e}")), p);
    }
    let work = std::env::temp_dir().join(format!("mercs2_exemap_lua_corpus_{what}"));
    std::fs::remove_dir_all(&work).ok();
    let env = Env { wine: None, work: work.clone() };
    let mut n = 0;
    for wad in corpora.wads.iter().filter(|w| select(corpora.corpus(&w.corpus).expect("every WAD names a listed corpus"))) {
        let head = &wad.chain[0].from_sha256;
        let path = by_sha
            .get(head)
            .unwrap_or_else(|| panic!("{} {}: no container with sha256 {head} among the files the keys name", wad.corpus, wad.name));
        derive(wad, path, &env).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert_eq!(n, expect, "{what} lists {expect} WADs");
    std::fs::remove_dir_all(&work).ok();
}

#[test]
fn every_x360_preview_wad_rebuilds_from_the_preview_disc() {
    let iso = local_config_x360_devkit_iso(start()).unwrap_or_else(|e| panic!("{e}"));
    derive_all("base-x360-preview", |c| c.id == "base-x360-preview", vec![iso], 5);
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
    derive_all("x360-dlc", |c| c.platform == "x360" && c.product.starts_with("dlc:"), containers, 5);
}

#[test]
fn every_ps3_disc_wad_rebuilds_from_the_disc_image() {
    let iso = local_config_ps3_disc_iso(start()).unwrap_or_else(|e| panic!("{e}"));
    derive_all("base-ps3-retail-na", |c| c.id == "base-ps3-retail-na", vec![iso], 9);
}

#[test]
fn every_ps3_update_wad_rebuilds_from_its_package() {
    let dir = local_config_ps3_update_pkgs(start()).unwrap_or_else(|e| panic!("{e}"));
    let mut containers = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("list {}: {e}", dir.display())) {
        let p = entry.unwrap_or_else(|e| panic!("list {}: {e}", dir.display())).path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkg")) {
            containers.push(p);
        }
    }
    derive_all("ps3-patch", |c| c.platform == "ps3" && c.product == "patch", containers, 2);
}

#[test]
fn every_ps3_dlc_wad_rebuilds_from_its_package() {
    let pkg = local_config_ps3_dlc_biua_pkg(start()).unwrap_or_else(|e| panic!("{e}"));
    derive_all("ps3-dlc", |c| c.platform == "ps3" && c.product.starts_with("dlc:"), vec![pkg], 1);
}
