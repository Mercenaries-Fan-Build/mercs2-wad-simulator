//! The catalogue against the real executables.
//!
//! Game-gated: built by the `retail` feature, run with `cargo xtask retail-test`. The executables are
//! found only through the repo-root `.mercs2-local.toml` keys `exe_corpus` (directory of retail
//! builds), `exe_dumps` (directory of derived builds and process-image dumps) and `drmfree_exe` (the
//! DRM-free executable, v1.0 code); a missing file, key or path fails the test naming it. The
//! official-patcher test also reads `v11_patcher_exe` (the official v1.1 patcher) and `wine_binary`
//! (the `wine64` that runs it).
//!
//! In both directories every file that starts with `MZ` is an executable under test; the other files
//! (the corpus's checksum list, raw memory scratch) are not executables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use mercs2_exemap::membership::check;
use mercs2_exemap::image::Image;
use mercs2_exemap::{identify_in, sha256_hex, Catalogue, Space};
use mercs2_formats::game_paths::{
    local_config_drmfree_exe, local_config_exe_corpus, local_config_exe_dumps, local_config_v11_patcher_exe,
    local_config_wine_binary,
};

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
fn the_drmfree_executable_is_v10_drmfree_laa() {
    let c = catalogue();
    let p = drmfree_exe();
    let b = identify_in(&c, &read(&p)).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(b.id, "v10-drmfree-laa");
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

/// The two SecuROM-wrapped retail builds and their layout references (unwrapped files of the same
/// code version) differ only inside the documented overlays. Checked byte by byte over the mapped
/// image and the file slack, apart from the run machinery: each byte outside every overlay range
/// must be present in the build and equal to the reference's.
#[test]
fn the_same_lineage_pairs_are_identity_outside_the_documented_overlays() {
    let c = catalogue();
    let files = by_sha();
    for id in ["archived-2-disk-dump", "v11-official-securom"] {
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
    assert_eq!(reproduced, ["v10-drmfree-laa", "v10-securom-unwrap"]);
}

/// sha256 of the official v1.1 patcher. The four regional patcher executables are this one file.
const PATCHER_SHA256: &str = "b27fedc2834022dc78dece1fef4374f75691fc176807372806a4aab5dc93be92";
/// The registry key the patcher reads its install folder from, as a 64-bit Wine prefix stores the
/// 32-bit view, and the one value it reads there.
const INSTALL_KEY: &str = r"HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames";
const INSTALL_VALUE: &str = "Install Dir";
/// Names of the install folder and of the game executable inside it.
const INSTALL_FOLDER: &str = "Mercenaries 2 World in Flames";
const GAME_EXE: &str = "Mercenaries2.exe";
/// Longest the patcher may run before the test fails.
const PATCHER_TIMEOUT: Duration = Duration::from_secs(600);

/// The builds.tsv recipe step this test reproduces.
fn patcher_step() -> String {
    format!(
        "official v1.1 patcher {} (Wine, HKLM{}\\{INSTALL_VALUE} = install folder)",
        &PATCHER_SHA256[..8],
        INSTALL_KEY.strip_prefix("HKEY_LOCAL_MACHINE").expect("INSTALL_KEY is under HKLM")
    )
}

/// `path` as a Wine `Z:` path. Wine maps `Z:` to the host root.
fn z_path(path: &Path) -> String {
    let s = path.to_str().unwrap_or_else(|| panic!("{} is not UTF-8", path.display()));
    format!("Z:{}", s.replace('/', "\\"))
}

/// A throwaway install folder and Wine prefix under the system temp dir. Dropping it stops the
/// prefix's wineserver and removes both.
struct Scratch {
    root: PathBuf,
    prefix: PathBuf,
    wineserver: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let killed = Command::new(&self.wineserver).arg("-k").env("WINEPREFIX", &self.prefix).status();
        let waited = Command::new(&self.wineserver).arg("-w").env("WINEPREFIX", &self.prefix).status();
        let removed = std::fs::remove_dir_all(&self.root);
        if std::thread::panicking() {
            return;
        }
        killed.unwrap_or_else(|e| panic!("{} -k: {e}", self.wineserver.display()));
        waited.unwrap_or_else(|e| panic!("{} -w: {e}", self.wineserver.display()));
        removed.unwrap_or_else(|e| panic!("remove {}: {e}", self.root.display()));
    }
}

/// Run `cmd` to completion within `timeout`, with stdout and stderr captured to `log`. Fails on a
/// timeout or a non-zero exit, quoting the log.
fn run_logged(mut cmd: Command, log: &Path, timeout: Duration, what: &str) {
    let out = std::fs::File::create(log).unwrap_or_else(|e| panic!("create {}: {e}", log.display()));
    let err = out.try_clone().unwrap_or_else(|e| panic!("{}: {e}", log.display()));
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .unwrap_or_else(|e| panic!("{what}: spawn: {e}"));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(s) = child.try_wait().unwrap_or_else(|e| panic!("{what}: wait: {e}")) {
            break s;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap_or_else(|e| panic!("{what}: kill after timeout: {e}"));
            child.wait().unwrap_or_else(|e| panic!("{what}: wait after kill: {e}"));
            panic!(
                "{what}: still running after {timeout:?}; log:\n{}",
                std::fs::read_to_string(log).unwrap_or_default()
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    assert!(status.success(), "{what}: exited with {status}; log:\n{}", std::fs::read_to_string(log).unwrap_or_default());
}

/// The official v1.1 patcher, run under Wine on the recipe's input, writes the catalogued build and
/// backs the input up, byte for byte. The input is copied into a fresh install folder, the one
/// registry value the patcher reads is imported into a fresh prefix, and the patcher runs with the
/// install folder as its working directory.
#[test]
fn the_official_patcher_recipe_reproduces_its_build() {
    let c = catalogue();
    let step = patcher_step();
    let targets: Vec<_> = c
        .builds
        .iter()
        .flat_map(|b| b.recipes.iter().filter(|r| r.step == step).map(move |r| (b, r)))
        .collect();
    assert_eq!(targets.len(), 1, "builds.tsv must hold exactly one recipe with step {step:?}");
    let (target, recipe) = targets[0];

    let start = start();
    let patcher = local_config_v11_patcher_exe(start).unwrap_or_else(|e| panic!("{e}"));
    let wine = local_config_wine_binary(start).unwrap_or_else(|e| panic!("{e}"));
    let wineserver = wine.with_file_name("wineserver");
    assert!(wineserver.is_file(), "{} has no wineserver beside it", wine.display());
    let patcher_bytes = read(&patcher);
    assert_eq!(sha256_hex(&patcher_bytes), PATCHER_SHA256, "{} is not the official v1.1 patcher", patcher.display());

    let corpus = corpus_dir();
    let input_path = executables(&corpus)
        .into_iter()
        .find(|p| sha256_hex(&read(p)) == recipe.from_sha256)
        .unwrap_or_else(|| panic!("{} recipe input sha256 {} is not in {}", target.id, recipe.from_sha256, corpus.display()));
    let input = read(&input_path);

    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock after 1970").as_nanos();
    let root = std::env::temp_dir().join(format!("mercs2_exemap_patcher_{}_{nanos}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap_or_else(|e| panic!("create {}: {e}", root.display()));
    let root = root.canonicalize().unwrap_or_else(|e| panic!("canonicalize {}: {e}", root.display()));
    let scratch = Scratch { prefix: root.join("prefix"), root: root.clone(), wineserver };
    let install = root.join(INSTALL_FOLDER);
    std::fs::create_dir_all(&install).unwrap_or_else(|e| panic!("create {}: {e}", install.display()));
    std::fs::write(install.join(GAME_EXE), &input).unwrap_or_else(|e| panic!("write {GAME_EXE}: {e}"));
    let patcher_copy = root.join(patcher.file_name().expect("a file has a name"));
    std::fs::write(&patcher_copy, &patcher_bytes).unwrap_or_else(|e| panic!("write {}: {e}", patcher_copy.display()));

    let reg = root.join("install.reg");
    let install_dir = format!("{}\\", z_path(&install)).replace('\\', "\\\\");
    std::fs::write(
        &reg,
        format!("Windows Registry Editor Version 5.00\r\n\r\n[{INSTALL_KEY}]\r\n\"{INSTALL_VALUE}\"=\"{install_dir}\"\r\n"),
    )
    .unwrap_or_else(|e| panic!("write {}: {e}", reg.display()));

    let wine_cmd = || {
        let mut cmd = Command::new(&wine);
        cmd.env("WINEPREFIX", &scratch.prefix).env("WINEDEBUG", "-all");
        cmd
    };
    let mut import = wine_cmd();
    import.args(["reg", "import"]).arg(z_path(&reg));
    run_logged(import, &root.join("reg-import.log"), PATCHER_TIMEOUT, "wine64 reg import");

    let mut patch = wine_cmd();
    patch.arg(&patcher_copy).current_dir(&install);
    run_logged(patch, &root.join("patcher.log"), PATCHER_TIMEOUT, "wine64 patcher");

    let output = read(&install.join(GAME_EXE));
    assert_eq!(sha256_hex(&output), target.sha256, "patcher output is not {}", target.id);
    assert_eq!(identify_in(&c, &output).map(|b| b.id.as_str()), Ok(target.id.as_str()));

    let backup = install.join("BACKUP");
    let mut names: Vec<String> = std::fs::read_dir(&backup)
        .unwrap_or_else(|e| panic!("list {}: {e}", backup.display()))
        .map(|e| e.unwrap_or_else(|e| panic!("list {}: {e}", backup.display())).file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 2, "BACKUP holds {names:?}, expected one Mercenaries2.A###.exe and unpatch.bak");
    assert_eq!(names[1], "unpatch.bak", "BACKUP holds {names:?}");
    let copy = &names[0];
    let digits = copy.strip_prefix("Mercenaries2.A").and_then(|r| r.strip_suffix(".exe"));
    assert!(
        digits.is_some_and(|d| d.len() == 3 && d.bytes().all(|b| b.is_ascii_digit())),
        "BACKUP copy {copy:?} is not Mercenaries2.A###.exe"
    );
    assert_eq!(sha256_hex(&read(&backup.join(copy))), recipe.from_sha256, "BACKUP\\{copy} is not the input");

    drop(scratch);
    assert!(!root.exists(), "{} survived cleanup", root.display());
}
