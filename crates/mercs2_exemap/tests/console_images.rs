//! `images.tsv` against the original console containers.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The containers are
//! found only through the repo-root `.mercs2-local.toml` keys `x360_devkit_iso` (the Jul 11 2008
//! Xbox 360 preview disc), `x360_retail_xex` (the Xbox 360 retail `default.xex`), `ps3_eboot_bin` (the PS3 disc `EBOOT.BIN`), `ps3_update_pkgs` (the
//! directory of PS3 v1.03 update packages), and `ps3_jp_disc_dir`, `ps3_de_disc_dir` and
//! `ps3_eu_disc_dir` (the PS3 Japanese, German and European disc dumps, each read at
//! `PS3_GAME/USRDIR/EBOOT.BIN`); a missing key or path fails the test naming it.
//!
//! Every console image is rebuilt from the container whose sha256 starts its chain, with every
//! intermediate sha256 checked; every `code_image` claim is checked byte for byte; and every
//! analysed image yields function starts and vtables (on PS3, every vtable slot an `.opd` function).
//! Every Xbox 360 image's XEX import records read and validate against its image, with the record
//! counts per library and kind that the three XEX import headers list. The PS3 v1.00 and v1.03
//! images' PRX function imports read and validate, with the per-library counts their import tables
//! list (17 libraries and 227 functions, 18 and 235), and no variable imports.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use mercs2_exemap::images::{derive, shared_code, stream_sha256, Images, Platform};
use mercs2_exemap::luareg::tables;
use mercs2_exemap::rtti::vtables;
use mercs2_exemap::starts::function_starts;
use mercs2_exemap::Catalogue;
use mercs2_formats::game_paths::{
    local_config_ps3_de_disc_dir, local_config_ps3_eboot_bin, local_config_ps3_eu_disc_dir,
    local_config_ps3_jp_disc_dir, local_config_ps3_update_pkgs, local_config_x360_devkit_iso,
    local_config_x360_retail_xex,
};

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Every container the keys name, by sha256: the disc image, the retail XEX, the EBOOT, the
/// `EBOOT.BIN` of each regional disc dump, and each `.pkg` in the update directory.
fn containers() -> BTreeMap<String, PathBuf> {
    let mut files = vec![
        local_config_x360_devkit_iso(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_x360_retail_xex(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_ps3_eboot_bin(start()).unwrap_or_else(|e| panic!("{e}")),
    ];
    for disc in [local_config_ps3_jp_disc_dir, local_config_ps3_de_disc_dir, local_config_ps3_eu_disc_dir] {
        let dir = disc(start()).unwrap_or_else(|e| panic!("{e}"));
        files.push(dir.join("PS3_GAME").join("USRDIR").join("EBOOT.BIN"));
    }
    let dir = local_config_ps3_update_pkgs(start()).unwrap_or_else(|e| panic!("{e}"));
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("list {}: {e}", dir.display())) {
        let p = entry.unwrap_or_else(|e| panic!("list {}: {e}", dir.display())).path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pkg")) {
            files.push(p);
        }
    }
    let mut out = BTreeMap::new();
    for p in files {
        let mut f = std::fs::File::open(&p).unwrap_or_else(|e| panic!("open {}: {e}", p.display()));
        let sha = stream_sha256(&mut f).unwrap_or_else(|e| panic!("hash {}: {e}", p.display()));
        out.insert(sha, p);
    }
    out
}

#[test]
fn every_console_image_rebuilds_from_its_container_and_shares_the_code_it_claims() {
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let images = Images::embedded(&catalogue).unwrap_or_else(|e| panic!("embedded images: {e}"));
    let containers = containers();

    let mut built: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for m in images.images.iter().filter(|m| m.platform != Platform::Pc) {
        let head = &m.chain[0].from_sha256;
        let path = containers.get(head).unwrap_or_else(|| {
            panic!(
                "image {}: no container with sha256 {head} among the files the x360_devkit_iso, x360_retail_xex, ps3_eboot_bin, ps3_update_pkgs, ps3_jp_disc_dir, ps3_de_disc_dir and ps3_eu_disc_dir keys name",
                m.id
            )
        });
        let mut f = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let bytes = derive(m, &mut f).unwrap_or_else(|e| panic!("{e}"));
        built.insert(m.id.clone(), bytes);
    }

    for m in images.images.iter().filter(|m| m.platform != Platform::Pc) {
        let bytes = &built[&m.id];
        match &m.code_image {
            Some(c) => shared_code(bytes, &built[c]).unwrap_or_else(|e| panic!("image {} against {c}: {e}", m.id)),
            None => {
                let (starts, _) = function_starts(bytes).unwrap_or_else(|e| panic!("image {}: {e}", m.id));
                assert!(!starts.is_empty(), "image {} has no function starts", m.id);
                let vts = vtables(bytes).unwrap_or_else(|e| panic!("image {}: {e}", m.id));
                assert!(!vts.is_empty(), "image {} has no vtables", m.id);
                // A PS3 vtable slot is an .opd descriptor's code; Xbox .pdata omits leaf functions,
                // so only the PS3 slots are all function starts.
                if m.platform == Platform::Ps3 {
                    for v in &vts {
                        for f in &v.slots {
                            assert!(starts.binary_search(f).is_ok(), "image {}: {} slot 0x{f:08x} is not an .opd function", m.id, v.raw_name);
                        }
                    }
                }
            }
        }
    }
}

/// Every Xbox 360 image's import records, per library: (functions, variables).
#[test]
fn every_xbox_image_lists_its_xex_import_records() {
    use mercs2_exemap::images::derive_with_xex;
    use mercs2_exemap::xbox::{import_records, ImportKind};
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let images = Images::embedded(&catalogue).unwrap_or_else(|e| panic!("embedded images: {e}"));
    let containers = containers();
    let expected: BTreeMap<&str, Vec<(&str, usize, usize)>> = BTreeMap::from([
        ("x360-preview-final", vec![("xam.xex", 109, 109), ("xboxkrnl.exe", 151, 162)]),
        ("x360-preview-profile", vec![("xam.xex", 110, 110), ("xbdm.xex", 1, 1), ("xboxkrnl.exe", 151, 162)]),
        ("x360-retail", vec![("xam.xex", 109, 109), ("xboxkrnl.exe", 151, 162)]),
    ]);
    let xbox: Vec<&str> = images.images.iter().filter(|m| m.platform == Platform::X360).map(|m| m.id.as_str()).collect();
    assert_eq!(xbox, expected.keys().copied().collect::<Vec<_>>(), "the Xbox 360 images of images.tsv");
    for (id, want) in &expected {
        let m = images.image(id).unwrap_or_else(|| panic!("no image {id} in images.tsv"));
        let head = &m.chain[0].from_sha256;
        let path = containers.get(head).unwrap_or_else(|| panic!("image {id}: no container with sha256 {head}"));
        let mut f = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let (bytes, xex) = derive_with_xex(m, &mut f).unwrap_or_else(|e| panic!("{e}"));
        let records = import_records(&xex, &bytes).unwrap_or_else(|e| panic!("image {id}: {e}"));
        let mut got: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for r in &records {
            let e = got.entry(r.library.as_str()).or_default();
            match r.kind {
                ImportKind::Function => e.0 += 1,
                ImportKind::Variable => e.1 += 1,
            }
        }
        let got: Vec<(&str, usize, usize)> = got.into_iter().map(|(l, (f, v))| (l, f, v)).collect();
        assert_eq!(&got, want, "image {id}: (library, functions, variables)");
    }
}

/// The PS3 v1.00 and v1.03 images' PRX function imports, per library, as their import tables list
/// them; `prx_imports` fails on a library that imports a variable.
#[test]
fn the_ps3_v100_and_v103_images_list_their_prx_imports() {
    use mercs2_exemap::images::derive_self_elf;
    use mercs2_exemap::prx::prx_imports;
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let images = Images::embedded(&catalogue).unwrap_or_else(|e| panic!("embedded images: {e}"));
    let containers = containers();
    let common: &[(&str, usize)] = &[
        ("cellAdec", 8),
        ("cellAudio", 7),
        ("cellCelpEnc", 8),
        ("cellGcmSys", 26),
        ("cellL10n", 2),
        ("cellMic", 9),
        ("cellNetCtl", 9),
        ("cellRtc", 5),
        ("cellSpurs", 21),
        ("cellSync", 3),
        ("cellSysmodule", 2),
        ("sceNp", 35),
        ("sys_io", 11),
        ("sys_net", 26),
    ];
    let with = |extra: &[(&'static str, usize)]| -> Vec<(&'static str, usize)> {
        let mut v: Vec<(&str, usize)> = common.iter().copied().chain(extra.iter().copied()).collect();
        v.sort();
        v
    };
    // per image: (library, functions) rows, libraries, functions
    type Want = (Vec<(&'static str, usize)>, usize, usize);
    let expected: BTreeMap<&str, Want> = BTreeMap::from([
        ("ps3-v100", (with(&[("cellSysutil", 25), ("sysPrxForUser", 18), ("sys_fs", 12)]), 17, 227)),
        (
            "ps3-v103-na",
            (with(&[("cellSysutil", 26), ("sceNpTrophy", 5), ("sysPrxForUser", 19), ("sys_fs", 13)]), 18, 235),
        ),
    ]);
    for (id, (want, libraries, functions)) in &expected {
        let m = images.image(id).unwrap_or_else(|| panic!("no image {id} in images.tsv"));
        let head = &m.chain[0].from_sha256;
        let path = containers.get(head).unwrap_or_else(|| panic!("image {id}: no container with sha256 {head}"));
        let mut f = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let elf = derive_self_elf(m, &mut f).unwrap_or_else(|e| panic!("{e}"));
        let imports = prx_imports(&elf).unwrap_or_else(|e| panic!("image {id}: {e}"));
        let mut got: BTreeMap<&str, usize> = BTreeMap::new();
        for i in &imports {
            *got.entry(i.library.as_str()).or_default() += 1;
        }
        assert_eq!(got.len(), *libraries, "image {id}: libraries");
        assert_eq!(imports.len(), *functions, "image {id}: function imports");
        let got: Vec<(&str, usize)> = got.into_iter().collect();
        assert_eq!(&got, want, "image {id}: (library, functions)");
    }
}

/// The Xbox 360 retail image carries its Lua registry names as data; every other analysed console
/// image (the PS3 images and the Xbox 360 Profile image fill their registry names from code) names
/// at least those namespaces.
#[test]
fn every_analysed_console_image_names_the_retail_lua_namespaces() {
    let catalogue = Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"));
    let images = Images::embedded(&catalogue).unwrap_or_else(|e| panic!("embedded images: {e}"));
    let containers = containers();
    let namespaces = |id: &str| -> BTreeSet<String> {
        let m = images.images.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("no image {id} in images.tsv"));
        let head = &m.chain[0].from_sha256;
        let path = containers.get(head).unwrap_or_else(|| panic!("image {id}: no container with sha256 {head}"));
        let mut f = std::fs::File::open(path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        let bytes = derive(m, &mut f).unwrap_or_else(|e| panic!("{e}"));
        let t = tables(&bytes).unwrap_or_else(|e| panic!("image {id}: {e}"));
        t.into_iter().filter_map(|t| t.namespace).collect()
    };
    let retail = namespaces("x360-retail");
    assert!(!retail.is_empty(), "x360-retail names no Lua namespace");
    for m in images.images.iter().filter(|m| m.platform != Platform::Pc && m.code_image.is_none() && m.id != "x360-retail") {
        let got = namespaces(&m.id);
        let missing: Vec<&String> = retail.difference(&got).collect();
        assert!(missing.is_empty(), "image {} does not name the retail Lua namespaces {missing:?}", m.id);
    }
}
