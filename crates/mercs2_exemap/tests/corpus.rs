//! The catalogue against the real executables.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The executables are
//! found only through the repo-root `.mercs2-local.toml` keys `exe_corpus` (directory of retail
//! builds), `exe_dumps` (directory of derived builds and process-image dumps) and `drmfree_exe` (the
//! DRM-free v1.1 executable); a missing file, key or path fails the test naming it.
//!
//! In both directories every file that starts with `MZ` is an executable under test; the other files
//! (the corpus's checksum list, raw memory scratch) are not executables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use mercs2_exemap::membership::check;
use mercs2_exemap::image::Image;
use mercs2_exemap::{identify_in, sha256_hex, Catalogue, Space};
use mercs2_formats::game_paths::{local_config_drmfree_exe, local_config_exe_corpus, local_config_exe_dumps};

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

fn start() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn catalogue() -> Catalogue {
    Catalogue::embedded().unwrap_or_else(|e| panic!("embedded catalogue: {e}"))
}

fn read(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every file in `dir` whose first two bytes are `MZ`, sorted by path.
fn executables(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("list {}: {e}", dir.display())) {
        let path = entry.unwrap_or_else(|e| panic!("list {}: {e}", dir.display())).path();
        if !path.is_file() {
            continue;
        }
        let mut head = [0u8; 2];
        let mut f = std::fs::File::open(&path).unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
        if std::io::Read::read(&mut f, &mut head).unwrap_or_else(|e| panic!("read {}: {e}", path.display())) == 2
            && &head == b"MZ"
        {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn corpus_dir() -> PathBuf {
    local_config_exe_corpus(start()).unwrap_or_else(|e| panic!("{e}"))
}

fn dumps_dir() -> PathBuf {
    local_config_exe_dumps(start()).unwrap_or_else(|e| panic!("{e}"))
}

fn drmfree_exe() -> PathBuf {
    local_config_drmfree_exe(start()).unwrap_or_else(|e| panic!("{e}"))
}

/// sha256 → path for every executable the config names.
fn by_sha() -> BTreeMap<String, PathBuf> {
    let mut paths = executables(&corpus_dir());
    paths.extend(executables(&dumps_dir()));
    paths.push(drmfree_exe());
    paths.into_iter().map(|p| (sha256_hex(&read(&p)), p)).collect()
}

fn bytes_of(files: &BTreeMap<String, PathBuf>, sha: &str, what: &str) -> Vec<u8> {
    let p = files
        .get(sha)
        .unwrap_or_else(|| panic!("{what} (sha256 {sha}) is in none of exe_corpus, exe_dumps, drmfree_exe"));
    read(p)
}

fn identifies_everything_in(paths: Vec<PathBuf>, dir: &Path) {
    let c = catalogue();
    assert!(!paths.is_empty(), "{} holds no executable", dir.display());
    for p in paths {
        let bytes = read(&p);
        let b = identify_in(&c, &bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert_eq!(b.sha256, sha256_hex(&bytes), "{}", p.display());
    }
}

#[test]
fn every_corpus_executable_identifies_to_its_row() {
    let dir = corpus_dir();
    identifies_everything_in(executables(&dir), &dir);
}

#[test]
fn every_dumps_executable_identifies_to_its_row() {
    let dir = dumps_dir();
    identifies_everything_in(executables(&dir), &dir);
}

#[test]
fn the_drmfree_executable_is_v11_patched_drmfree() {
    let c = catalogue();
    let p = drmfree_exe();
    let b = identify_in(&c, &read(&p)).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(b.id, "v11-patched-drmfree");
}

/// Every catalogued build is present, and its membership overlays are exactly its overlays.tsv rows.
#[test]
fn every_build_membership_equals_its_overlay_rows() {
    let c = catalogue();
    let files = by_sha();
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

/// The v1.0 builds and their layout references differ only inside the documented overlays. Checked
/// byte by byte over the mapped image and the file slack, apart from the run machinery: each byte
/// outside every overlay range must be present in the build and equal to the reference's.
#[test]
fn the_same_lineage_pairs_are_identity_outside_the_documented_overlays() {
    let c = catalogue();
    let files = by_sha();
    for id in ["v10-unsigned", "v10-ea-signed"] {
        let b = c.build(id).unwrap_or_else(|| panic!("{id} is not catalogued"));
        let layout = c.layout(&b.layout).expect("cross-checked at load");
        let bytes = bytes_of(&files, &b.sha256, id);
        let refb = bytes_of(&files, &layout.reference_sha256, "reference");
        let (bp, rp) = (Image::parse(&bytes).unwrap(), Image::parse(&refb).unwrap());
        let rows = c.overlay_set(b.overlay_set.as_deref().expect("a v1.0 build has overlays"));
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
/// running that call on the recipe's input. This covers `v11-securom-unwrap` (default options) and
/// `v11-patched-drmfree` (`large_address_aware`).
#[test]
fn securom_unwrap_recipes_reproduce_their_builds() {
    let c = catalogue();
    let files = by_sha();
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
    assert_eq!(reproduced, ["v11-patched-drmfree", "v11-securom-unwrap"]);
}
