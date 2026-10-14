//! The catalogue against the real executables.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The executables are
//! found only in the canonical tree of PC builds that the repo-root `.mercs2-local.toml` key
//! `canonical_dir` names, each by sha256 ([`mercs2_exemap::canonical`]); a missing key, path or
//! build fails the test naming it. Nothing is run: every check reads bytes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_exemap::canonical::{builds_by_sha, files_under};
use mercs2_exemap::image::Image;
use mercs2_exemap::membership::check;
use mercs2_exemap::{identify_in, sha256_hex, Catalogue, Space};
use mercs2_formats::game_paths::local_config_canonical_dir;

/// The `securom_unwrap::unwrap` options a builds.tsv recipe step names, or `None` when the step is
/// not a `securom_unwrap` transform. A `securom_unwrap` step this does not know fails the test.
fn unwrap_options(step: &str) -> Option<securom_unwrap::Options> {
    use securom_unwrap::Options;
    match step {
        "securom_unwrap::unwrap(Options::default())" => Some(Options::default()),
        "securom_unwrap::unwrap(Options { large_address_aware: true, .. })" => {
            Some(Options { large_address_aware: true, ..Options::default() })
        }
        s if s.starts_with("securom_unwrap") => panic!("recipe step {s:?} names securom_unwrap options this test does not know"),
        _ => None,
    }
}

fn catalogue() -> Catalogue {
    Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"))
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

fn canonical_dir() -> PathBuf {
    local_config_canonical_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
}

/// sha256 → path for every catalogued build in the canonical tree.
fn by_sha(c: &Catalogue) -> BTreeMap<String, PathBuf> {
    builds_by_sha(c, &canonical_dir()).unwrap_or_else(|e| panic!("{e}"))
}

fn bytes_of(files: &BTreeMap<String, PathBuf>, sha: &str, what: &str) -> Vec<u8> {
    let p = files.get(sha).unwrap_or_else(|| panic!("{what} (sha256 {sha}) is not in the canonical tree"));
    read(p)
}

/// Every catalogued build is in the canonical tree.
#[test]
fn every_catalogued_build_is_in_the_canonical_tree() {
    let c = catalogue();
    let files = by_sha(&c);
    let missing: Vec<String> =
        c.builds.iter().filter(|b| !files.contains_key(&b.sha256)).map(|b| format!("{} ({})", b.id, b.sha256)).collect();
    assert!(missing.is_empty(), "not in the canonical tree: {missing:?}");
}

/// Every file in the canonical tree that has the size of a catalogued build is that build: an
/// unknown variant of a known size fails here instead of passing unnoticed.
#[test]
fn every_canonical_file_of_a_build_size_identifies_to_its_row() {
    let c = catalogue();
    let sizes: std::collections::BTreeSet<u64> = c.builds.iter().map(|b| b.size).collect();
    let mut n = 0;
    for p in files_under(&canonical_dir()).unwrap_or_else(|e| panic!("{e}")) {
        let size = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display())).len();
        if !sizes.contains(&size) {
            continue;
        }
        let bytes = read(&p);
        let b = identify_in(&c, &bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(b.sha256, sha256_hex(&bytes), "{}", p.display());
        n += 1;
    }
    assert!(n >= c.builds.len(), "{n} files identified, the catalogue lists {} builds", c.builds.len());
}

/// Every catalogued build's membership overlays are exactly its overlays.tsv rows.
#[test]
fn every_build_membership_equals_its_overlay_rows() {
    let c = catalogue();
    let files = by_sha(&c);
    for b in &c.builds {
        let layout = c.layout(&b.layout).expect("cross-checked at load");
        let bytes = bytes_of(&files, &b.sha256, &b.id);
        let reference = bytes_of(&files, &layout.reference_sha256, &format!("layout {} reference", layout.id));
        let got = check(&c, &bytes, &reference).unwrap_or_else(|e| panic!("{}: {e}", b.id));
        let want = match &b.overlay_set {
            None => Vec::new(),
            Some(set) => c.overlay_set(set),
        };
        assert_eq!(got, want, "{}", b.id);
    }
}

/// The two SecuROM-wrapped retail builds and their layout references (unwrapped files of the same
/// code version) differ only inside the documented overlays. Checked byte by byte over the mapped
/// image and the file slack, apart from the run machinery: each byte outside every overlay range
/// must be present in the build and equal to the reference's.
#[test]
fn the_same_lineage_pairs_are_identity_outside_the_documented_overlays() {
    let c = catalogue();
    let files = by_sha(&c);
    for id in ["archived-2-disk-dump", "v10-ea-signed"] {
        let b = c.build(id).unwrap_or_else(|| panic!("{id} is not catalogued"));
        let layout = c.layout(&b.layout).expect("cross-checked at load");
        let bytes = bytes_of(&files, &b.sha256, id);
        let refb = bytes_of(&files, &layout.reference_sha256, "reference");
        let (bp, rp) = (Image::parse(&bytes).unwrap(), Image::parse(&refb).unwrap());
        let rows = c.overlay_set(b.overlay_set.as_deref().expect("a wrapped retail build has overlays"));
        let covered = |space: Space, at: u64| rows.iter().any(|o| o.space == space && o.start <= at && at < o.end);
        let base = rp.pe.image_base as u64;

        let extents: Vec<_> =
            std::iter::once(0..rp.pe.size_of_headers).chain((0..rp.pe.sections.len()).map(|i| rp.section_extent(i))).collect();
        let mut checked = 0u64;
        for ext in extents {
            let want = rp.mapped(ext.clone()).unwrap();
            for piece in bp.view(ext.clone()) {
                for rva in piece.rva.clone() {
                    let va = base + rva as u64;
                    if covered(Space::Va, va) {
                        continue;
                    }
                    let have = piece.bytes.as_ref().map(|v| v[(rva - piece.rva.start) as usize]);
                    assert_eq!(have, Some(want[(rva - ext.start) as usize]), "{id}: VA 0x{va:x}");
                    checked += 1;
                }
            }
        }
        for range in rp.slack().iter().chain(bp.slack()) {
            for o in range.clone() {
                if covered(Space::File, o) {
                    continue;
                }
                assert_eq!(bytes.get(o as usize), refb.get(o as usize), "{id}: file offset 0x{o:x}");
            }
        }
        assert!(checked > 0, "{id}: nothing outside the overlays was compared");
    }
}

/// Every build whose recipe is a `securom_unwrap::unwrap` call is reproduced, byte for byte, by
/// running that call on the recipe's input. This covers `v10-securom-unwrap` (default options) and
/// `v10-drmfree-laa` (`large_address_aware`).
#[test]
fn securom_unwrap_recipes_reproduce_their_builds() {
    let c = catalogue();
    let files = by_sha(&c);
    let mut reproduced = Vec::new();
    for b in &c.builds {
        for r in &b.recipes {
            let Some(opts) = unwrap_options(&r.step) else { continue };
            let input = bytes_of(&files, &r.from_sha256, &format!("{} recipe input", b.id));
            let (out, _) = securom_unwrap::unwrap(&input, &opts)
                .unwrap_or_else(|e| panic!("{}: securom_unwrap::unwrap: {e}", b.id));
            assert_eq!(sha256_hex(&out), b.sha256, "{}: {}", b.id, r.step);
            reproduced.push(b.id.as_str());
        }
    }
    reproduced.sort();
    assert_eq!(reproduced, ["v10-drmfree-laa", "v10-securom-unwrap"]);
}
