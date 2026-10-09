//! `images.tsv` against the original console containers.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The containers are
//! found only through the repo-root `.mercs2-local.toml` keys `x360_devkit_iso` (the Jul 11 2008
//! Xbox 360 preview disc), `ps3_eboot_bin` (the PS3 disc `EBOOT.BIN`) and `ps3_update_pkgs` (the
//! directory of PS3 v1.03 update packages); a missing key or path fails the test naming it.
//!
//! Every console image is rebuilt from the container whose sha256 starts its chain, with every
//! intermediate sha256 checked; every `code_image` claim is checked byte for byte; and every
//! analysed image yields function starts and vtables (on PS3, every vtable slot an `.opd` function).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_exemap::images::{derive, shared_code, stream_sha256, Images, Platform};
use mercs2_exemap::rtti::vtables;
use mercs2_exemap::starts::function_starts;
use mercs2_exemap::Catalogue;
use mercs2_formats::game_paths::{
    local_config_ps3_eboot_bin, local_config_ps3_update_pkgs, local_config_x360_devkit_iso,
};

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Every container the keys name, by sha256: the disc image, the EBOOT, and each `.pkg` in the
/// update directory.
fn containers() -> BTreeMap<String, PathBuf> {
    let mut files = vec![
        local_config_x360_devkit_iso(start()).unwrap_or_else(|e| panic!("{e}")),
        local_config_ps3_eboot_bin(start()).unwrap_or_else(|e| panic!("{e}")),
    ];
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
                "image {}: no container with sha256 {head} among the files the x360_devkit_iso, ps3_eboot_bin and ps3_update_pkgs keys name",
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
