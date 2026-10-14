//! `corpora.tsv` / `wads.tsv` against the original files.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The originals are
//! found only through the repo-root `.mercs2-local.toml` keys `canonical_dir` (the canonical tree
//! of PC builds, which holds the PC installs and the PC images), `x360_devkit_iso` (the Xbox 360
//! preview disc), `x360_retail_dir` (the Xbox 360 retail disc dump), `x360_dlc_biua_stfs` (the
//! retail Blow It Up Again Pack), `x360_dlc_prototypes_dir` (the zipped DLC prototypes),
//! `ps3_disc_iso` (the PS3 NA disc image), `ps3_jp_disc_dir` (the PS3 Japanese disc dump),
//! `ps3_de_disc_dir` (the PS3 German disc dump), `ps3_eu_disc_dir` (the PS3 European disc dump),
//! `ps3_update_pkgs` (the folder of PS3 update packages) and `ps3_dlc_biua_pkg` (the PS3 NA Blow It
//! Up Again Pack); a missing key or path fails the test naming it. Nothing is run.
//!
//! Every WAD with an `install` column (the PC demo, retail and repack corpora under
//! `canonical_dir`, the Xbox 360 retail dump, the PS3 Japanese, German and European dumps) is
//! checked in place in the install that column names; every other WAD is rebuilt from its
//! container, with every intermediate sha256 checked, in a fresh work directory under the system
//! temp directory.
//!
//! Every catalogued corpus is also indexed (`luaindex::index`, which decodes every Lua chunk and
//! requires it to re-encode byte for byte) once per test run, from its WADs checked in place or
//! rebuilt from their containers in a fresh work directory; the index tests check each corpus's
//! function and script counts, the pairings of the corpora of one lineage (`luapair::pair`), and the
//! links of the PC and Xbox 360 retail corpora to their images' Lua registration tables
//! (`lualink::link` over `luareg::tables`). The images are the PC v1.1 image `pc-v11`, found by
//! sha256 in the canonical tree, and the Xbox 360 retail image `x360-retail`, rebuilt from the
//! `x360_retail_xex` key's `default.xex`.

use std::path::Path;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use mercs2_exemap::canonical::builds_by_sha;
use mercs2_exemap::corpus::{check_installed, derive, file_sha256, Corpora, Corpus, Env, Install};
use mercs2_exemap::images::Images;
use mercs2_exemap::luaindex::{index, FnRow};
use mercs2_exemap::lualink::{link, Link};
use mercs2_exemap::luapair::{pair, Pairing};
use mercs2_exemap::luareg::tables;
use mercs2_exemap::Catalogue;
use mercs2_formats::game_paths::{
    local_config_canonical_dir, local_config_ps3_de_disc_dir, local_config_ps3_disc_iso, local_config_ps3_dlc_biua_pkg,
    local_config_ps3_eu_disc_dir, local_config_ps3_jp_disc_dir, local_config_ps3_update_pkgs, local_config_x360_devkit_iso,
    local_config_x360_dlc_biua_stfs, local_config_x360_dlc_prototypes_dir, local_config_x360_retail_dir,
    local_config_x360_retail_xex,
};

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn canonical_dir() -> PathBuf {
    local_config_canonical_dir(start()).unwrap_or_else(|e| panic!("{e}"))
}

fn install_dir(install: &Install) -> PathBuf {
    let r = match install {
        Install::Canonical(dir) => return canonical_dir().join(dir),
        Install::Key(key) => match key.as_str() {
            "x360_retail_dir" => local_config_x360_retail_dir(start()),
            "ps3_jp_disc_dir" => local_config_ps3_jp_disc_dir(start()),
            "ps3_de_disc_dir" => local_config_ps3_de_disc_dir(start()),
            "ps3_eu_disc_dir" => local_config_ps3_eu_disc_dir(start()),
            other => panic!("wads.tsv names install key {other}, which this test does not resolve"),
        },
    };
    r.unwrap_or_else(|e| panic!("{e}"))
}

fn check_corpus(corpus: &str, expect: usize) {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let mut n = 0;
    for wad in corpora.wads_of(corpus) {
        let install = wad.install.as_ref().unwrap_or_else(|| panic!("{corpus} {} has a chain, not an install", wad.name));
        check_installed(wad, &install_dir(install)).unwrap_or_else(|e| panic!("{e}"));
        n += 1;
    }
    assert_eq!(n, expect, "{corpus} lists {expect} WADs");
}

#[test]
fn every_pc_demo_wad_is_in_the_demo_install() {
    check_corpus("base-pc-demo", 4);
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

#[test]
fn every_ps3_de_wad_is_in_the_de_disc_dump() {
    check_corpus("base-ps3-retail-de", 5);
}

#[test]
fn every_ps3_eu_wad_is_in_the_eu_disc_dump() {
    check_corpus("base-ps3-retail-eu", 9);
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
    let env = Env { work: work.clone() };
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
    derive_all("ps3-patch", |c| c.platform == "ps3" && c.product == "patch", containers, 6);
}

#[test]
fn every_ps3_dlc_wad_rebuilds_from_its_package() {
    let pkg = local_config_ps3_dlc_biua_pkg(start()).unwrap_or_else(|e| panic!("{e}"));
    derive_all("ps3-dlc", |c| c.platform == "ps3" && c.product.starts_with("dlc:"), vec![pkg], 1);
}

/// The files in `dir` with extension `ext` (any case).
fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("list {}: {e}", dir.display())) {
        let p = entry.unwrap_or_else(|e| panic!("list {}: {e}", dir.display())).path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
    out
}

/// Every corpus's index, by corpus id, built once per test run.
static INDEXES: OnceLock<BTreeMap<String, Vec<FnRow>>> = OnceLock::new();

fn indexes() -> &'static BTreeMap<String, Vec<FnRow>> {
    INDEXES.get_or_init(build_indexes)
}

/// Index every catalogued corpus: a WAD with an `install` column is checked in place, every other
/// WAD is rebuilt from the container (named by a key) whose sha256 starts its chain.
fn build_indexes() -> BTreeMap<String, Vec<FnRow>> {
    let corpora = Corpora::embedded().unwrap_or_else(|e| panic!("embedded corpus tables: {e}"));
    let mut files = vec![
        local_config_x360_devkit_iso(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_x360_dlc_biua_stfs(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_ps3_disc_iso(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_ps3_dlc_biua_pkg(start()).unwrap_or_else(|e| panic!("{e}")),
    ];
    files.extend(files_with_extension(&local_config_x360_dlc_prototypes_dir(start()).unwrap_or_else(|e| panic!("{e}")), "zip"));
    files.extend(files_with_extension(&local_config_ps3_update_pkgs(start()).unwrap_or_else(|e| panic!("{e}")), "pkg"));
    let mut containers = BTreeMap::new();
    for p in files {
        containers.insert(file_sha256(&p).unwrap_or_else(|e| panic!("{e}")), p);
    }
    let work = std::env::temp_dir().join("mercs2_exemap_lua_corpus_index");
    std::fs::remove_dir_all(&work).ok();
    let env = Env { work: work.clone() };
    let mut out = BTreeMap::new();
    for c in &corpora.corpora {
        let mut paths = Vec::new();
        for wad in corpora.wads_of(&c.id) {
            let path = match &wad.install {
                Some(install) => check_installed(wad, &install_dir(install)),
                None => {
                    let head = &wad.chain[0].from_sha256;
                    let container = containers.get(head).unwrap_or_else(|| {
                        panic!("{} {}: no container with sha256 {head} among the files the keys name", wad.corpus, wad.name)
                    });
                    derive(wad, container, &env)
                }
            };
            paths.push(path.unwrap_or_else(|e| panic!("{e}")));
        }
        let rows = index(&corpora, &c.id, &paths).unwrap_or_else(|e| panic!("lua-index {}: {e}", c.id));
        out.insert(c.id.clone(), rows);
    }
    std::fs::remove_dir_all(&work).unwrap_or_else(|e| panic!("remove {}: {e}", work.display()));
    out
}

fn corpus_index(id: &str) -> &'static [FnRow] {
    indexes().get(id).unwrap_or_else(|| panic!("corpora.tsv lists no corpus {id}"))
}

/// Every corpus indexes, and its (functions, scripts) are the counts listed here; a script is one
/// (WAD, name hash).
#[test]
fn every_catalogued_corpus_indexes_with_its_function_and_script_counts() {
    let want: BTreeMap<&str, (usize, usize)> = [
        ("base-pc-demo", (6522, 432)),
        ("base-pc-elamigos", (6922, 679)),
        ("base-pc-retail", (6922, 679)),
        ("base-ps3-retail-de", (6778, 672)),
        ("base-ps3-retail-eu", (6794, 676)),
        ("base-ps3-retail-jp", (6778, 672)),
        ("base-ps3-retail-na", (6794, 676)),
        ("base-x360-preview", (6813, 675)),
        ("base-x360-retail", (6778, 672)),
        ("dlc-amazon-x360-proto", (22, 6)),
        ("dlc-biua-ps3-na", (934, 36)),
        ("dlc-biua-x360-proto-20081106", (909, 35)),
        ("dlc-biua-x360-proto-20081125", (934, 36)),
        ("dlc-biua-x360-proto-packb", (22, 6)),
        ("dlc-biua-x360-retail", (934, 36)),
        ("patch-ps3-v103-de", (1229, 28)),
        ("patch-ps3-v103-eu", (1229, 28)),
        ("patch-ps3-v103-na", (1229, 28)),
    ]
    .into_iter()
    .collect();
    let got: BTreeMap<&str, (usize, usize)> = indexes()
        .iter()
        .map(|(id, rows)| {
            let scripts: BTreeSet<(&str, u32)> = rows.iter().map(|r| (r.wad.as_str(), r.name_hash)).collect();
            (id.as_str(), (rows.len(), scripts.len()))
        })
        .collect();
    assert_eq!(got, want);
}

/// `luapair::pair(a, b)` counted by kind, under the names `exemap lua-pair` prints.
fn pair_counts(a: &str, b: &str) -> BTreeMap<&'static str, usize> {
    let pairs = pair(corpus_index(a), corpus_index(b)).unwrap_or_else(|e| panic!("lua-pair {a} {b}: {e}"));
    let mut counts = BTreeMap::new();
    for p in &pairs {
        let kind = match p {
            Pairing::Pair { class, .. } => class.name(),
            Pairing::OnlyA { .. } => "only-a",
            Pairing::OnlyB { .. } => "only-b",
            Pairing::ScriptOnlyA { .. } => "script-only-a",
            Pairing::ScriptOnlyB { .. } => "script-only-b",
        };
        *counts.entry(kind).or_default() += 1;
    }
    counts
}

fn assert_pairs(a: &str, b: &str, want: &[(&'static str, usize)]) {
    assert_eq!(pair_counts(a, b), want.iter().copied().collect::<BTreeMap<_, _>>(), "lua-pair {a} {b}");
}

#[test]
fn the_ps3_na_and_eu_discs_carry_identical_scripts() {
    assert_pairs("base-ps3-retail-na", "base-ps3-retail-eu", &[("identical", 6314)]);
}

#[test]
fn every_ps3_v103_update_carries_identical_scripts() {
    assert_pairs("patch-ps3-v103-na", "patch-ps3-v103-eu", &[("identical", 1229)]);
    assert_pairs("patch-ps3-v103-na", "patch-ps3-v103-de", &[("identical", 1229)]);
}

#[test]
fn the_ps3_blow_it_up_again_pack_is_the_xbox_retail_pack() {
    assert_pairs("dlc-biua-x360-retail", "dlc-biua-ps3-na", &[("identical", 934)]);
}

#[test]
fn the_nov_25_blow_it_up_again_prototype_is_the_retail_pack() {
    assert_pairs("dlc-biua-x360-proto-20081125", "dlc-biua-x360-retail", &[("identical", 934)]);
}

#[test]
fn the_elamigos_repack_carries_the_official_pc_scripts() {
    assert_pairs("base-pc-retail", "base-pc-elamigos", &[("identical", 6393)]);
}

#[test]
fn the_xbox_retail_and_ps3_eu_scripts_are_one_revision() {
    assert_pairs(
        "base-x360-retail",
        "base-ps3-retail-eu",
        &[("identical", 6297), ("code-differs", 1), ("script-only-b", 4)],
    );
}

#[test]
fn the_pc_retail_and_ps3_jp_scripts_are_one_revision() {
    assert_pairs(
        "base-pc-retail",
        "base-ps3-retail-jp",
        &[
            ("identical", 6221),
            ("constants-differ", 13),
            ("code-differs", 57),
            ("only-a", 69),
            ("only-b", 3),
            ("script-only-a", 6),
            ("script-only-b", 1),
        ],
    );
}

/// The image's registration entries by (namespace, name), as `lualink::parse_luareg` reads them
/// from an `exemap luareg` export (an unnamed table's namespace is empty).
fn registrations(image: &[u8], id: &str) -> BTreeMap<(String, String), BTreeSet<u64>> {
    let mut reg: BTreeMap<(String, String), BTreeSet<u64>> = BTreeMap::new();
    for t in tables(image).unwrap_or_else(|e| panic!("luareg {id}: {e}")) {
        for (name, f) in &t.entries {
            reg.entry((t.namespace.clone().unwrap_or_default(), name.clone())).or_default().insert(*f as u64);
        }
    }
    reg
}

/// Distinct lookups of `corpus` linked through `image`: (distinct, resolved, ambiguous,
/// unregistered in a native namespace, in a Lua namespace).
fn link_counts(corpus: &str, image: &[u8], id: &str) -> (usize, usize, usize, usize, usize) {
    let links = link(corpus_index(corpus), &registrations(image, id)).unwrap_or_else(|e| panic!("lua-link {corpus} {id}: {e}"));
    let mut distinct: BTreeMap<&str, &Link> = BTreeMap::new();
    for l in &links {
        if let Some(prev) = distinct.insert(l.lookup.as_str(), &l.link) {
            assert_eq!(prev, &l.link, "lua-link {corpus} {id}: lookup {} links two ways", l.lookup);
        }
    }
    let count = |f: fn(&Link) -> bool| distinct.values().filter(|l| f(l)).count();
    (
        distinct.len(),
        count(|l| matches!(l, Link::Resolved(_))),
        count(|l| matches!(l, Link::Ambiguous(_))),
        count(|l| matches!(l, Link::UnregisteredInNativeNamespace)),
        count(|l| matches!(l, Link::LuaNamespace)),
    )
}

fn images() -> Images {
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    Images::embedded(&catalogue).unwrap_or_else(|e| panic!("embedded images: {e}"))
}

#[test]
fn the_pc_retail_lookups_link_to_the_pc_v11_registration_tables() {
    let images = images();
    let m = images.image("pc-v11").expect("images.tsv lists pc-v11");
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let dir = canonical_dir();
    let files = builds_by_sha(&catalogue, &dir).unwrap_or_else(|e| panic!("{e}"));
    let path = files.get(&m.sha256).unwrap_or_else(|| panic!("pc-v11 (sha256 {}) is not under {}", m.sha256, dir.display()));
    let image = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(link_counts("base-pc-retail", &image, "pc-v11"), (1902, 719, 0, 46, 1137));
}

#[test]
fn the_x360_retail_lookups_link_to_the_x360_retail_registration_tables() {
    let images = images();
    let m = images.image("x360-retail").expect("images.tsv lists x360-retail");
    let xex = local_config_x360_retail_xex(start()).unwrap_or_else(|e| panic!("{e}"));
    let mut f = std::fs::File::open(&xex).unwrap_or_else(|e| panic!("open {}: {e}", xex.display()));
    let image = mercs2_exemap::images::derive(m, &mut f).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(link_counts("base-x360-retail", &image, "x360-retail"), (1835, 664, 0, 46, 1125));
}
