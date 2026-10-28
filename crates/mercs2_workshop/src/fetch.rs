//! Reference data: fetch the pinned `mercs2-workshop-data.zip` release asset and install it.
//!
//! The bundle is built by CI and published as a release asset. The Workshop reads its reference
//! data (`names.bin`, the Lua corpora, the registry rows, …) only from a complete bundle. When
//! [`crate::index::data_home`] finds none, [`crate::index::require_data_home`] downloads and
//! installs it through [`install`] before anything that needs it loads. The Settings page's
//! Download button runs the same install on request, replacing the downloaded copy with the
//! pinned release's.
//!
//! The release is the one [`PINNED`] names, read at compile time from the `BUNDLE_URL` and
//! `BUNDLE_SHA256` of `data/workshop_data_pin.env` — the file `.github/workflows/ci.yml` sources
//! to download the bundle the corpus gates run over. The install downloads exactly that URL and
//! checks the archive's sha256 against `BUNDLE_SHA256` before unpacking anything; a mismatch is
//! an error and leaves the installed copy as it was.
//!
//! It installs into the user's cache directory, the last step of the
//! [`crate::index::data_home`] chain: a bundle found at any earlier step is used ahead of the
//! downloaded one. When the download fails, the error carries [`manual_install_help`]: where to
//! download the bundle by hand, its sha256, and where to unpack it.

use std::io::Read;
use std::path::{Path, PathBuf};

/// [`REPO`] as a literal, for the `concat!` in [`pin_from_env`]'s compile-time messages.
macro_rules! repo {
    () => {
        "Mercenaries-Fan-Build/mercs2-wad-simulator"
    };
}
/// [`ASSET`] as a literal, for the same messages.
macro_rules! asset {
    () => {
        "mercs2-workshop-data.zip"
    };
}
/// The pin file's path in the repo, for the same messages.
macro_rules! pin_file {
    () => {
        "crates/mercs2_workshop/data/workshop_data_pin.env"
    };
}
/// The repo publishing the release assets — the same one the modkit fetches `wad_simulator` from.
const REPO: &str = repo!();
/// The release asset built by the workflow's "Build workshop_data reference bundle" step.
pub const ASSET: &str = asset!();
/// The pinned bundle as `BUNDLE_URL=` and `BUNDLE_SHA256=` lines. The CI workflow's "Download
/// the Workshop reference bundle (pinned)" step sources this file to fetch the bundle the corpus
/// gates run over (`index::PINNED_CORPUS` holds that bundle's script counts); [`PINNED`] is read
/// from the same two lines. Inside the crate, so a packaged copy of the crate carries it.
const PIN_ENV: &str = include_str!("../data/workshop_data_pin.env");

/// One release of [`ASSET`]: its download URL, the release tag in that URL, and the archive's
/// sha256 (lowercase hex).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin<'a> {
    pub url: &'a str,
    pub tag: &'a str,
    pub sha256: &'a str,
}

/// The bundle [`install`] downloads, read from [`PIN_ENV`] at compile time. A pin file without
/// exactly one `BUNDLE_URL=` and one `BUNDLE_SHA256=` line, a URL that is not
/// `https://github.com/<REPO>/releases/download/<tag>/<ASSET>`, or a digest that is not 64
/// lowercase hex digits stops the build with the reason.
pub const PINNED: Pin<'static> = match pin_from_env(PIN_ENV) {
    Ok(p) => p,
    Err(e) => panic!("{}", e),
};

/// Read the [`Pin`] from `BUNDLE_URL=` and `BUNDLE_SHA256=` lines (each key the first thing on its
/// line after indentation, exactly once in the file; the value is the rest of the line, as is).
const fn pin_from_env(src: &str) -> Result<Pin<'_>, &'static str> {
    let url = match env_value(src, b"BUNDLE_URL") {
        Lookup::Found(v) => v,
        Lookup::Missing => return Err(concat!(pin_file!(), " has no `BUNDLE_URL=` line")),
        Lookup::Repeated => return Err(concat!(pin_file!(), " has more than one `BUNDLE_URL=` line")),
        Lookup::NotUtf8 => return Err(concat!("BUNDLE_URL in ", pin_file!(), " is not UTF-8")),
    };
    let sha256 = match env_value(src, b"BUNDLE_SHA256") {
        Lookup::Found(v) => v,
        Lookup::Missing => return Err(concat!(pin_file!(), " has no `BUNDLE_SHA256=` line")),
        Lookup::Repeated => {
            return Err(concat!(pin_file!(), " has more than one `BUNDLE_SHA256=` line"))
        }
        Lookup::NotUtf8 => return Err(concat!("BUNDLE_SHA256 in ", pin_file!(), " is not UTF-8")),
    };
    let tag = match release_tag(url) {
        Some(t) => t,
        None => {
            return Err(concat!(
                "BUNDLE_URL in ",
                pin_file!(),
                " is not https://github.com/",
                repo!(),
                "/releases/download/<tag>/",
                asset!()
            ))
        }
    };
    if !is_sha256_hex(sha256.as_bytes()) {
        return Err(concat!("BUNDLE_SHA256 in ", pin_file!(), " is not 64 lowercase hex digits"));
    }
    Ok(Pin { url, tag, sha256 })
}

/// What [`env_value`] found for a key.
enum Lookup<'a> {
    Found(&'a str),
    Missing,
    Repeated,
    NotUtf8,
}

/// The value of the `<key>=<value>` line in `src`: everything after the `=` up to the line end,
/// untrimmed, so a stray space or CR fails the URL / digest checks rather than being dropped.
const fn env_value<'a>(src: &'a str, key: &[u8]) -> Lookup<'a> {
    let b = src.as_bytes();
    let mut found: Option<(usize, usize)> = None;
    let mut line = 0;
    while line < b.len() {
        let mut end = line;
        while end < b.len() && b[end] != b'\n' {
            end += 1;
        }
        let mut k = line;
        while k < end && (b[k] == b' ' || b[k] == b'\t') {
            k += 1;
        }
        let eq = k + key.len();
        if eq < end && bytes_at(b, k, key) && b[eq] == b'=' {
            if found.is_some() {
                return Lookup::Repeated;
            }
            found = Some((eq + 1, end));
        }
        line = end + 1;
    }
    match found {
        Some((v, e)) => match std::str::from_utf8(sub(b, v, e)) {
            Ok(s) => Lookup::Found(s),
            Err(_) => Lookup::NotUtf8,
        },
        None => Lookup::Missing,
    }
}

/// The `<tag>` of `https://github.com/<REPO>/releases/download/<tag>/<ASSET>`; `None` for any
/// other URL (including GitHub's `/releases/latest/download/` form).
const fn release_tag(url: &str) -> Option<&str> {
    let b = url.as_bytes();
    let mut at = 0;
    let parts: [&[u8]; 3] = [b"https://github.com/", REPO.as_bytes(), b"/releases/download/"];
    let mut i = 0;
    while i < parts.len() {
        if !bytes_at(b, at, parts[i]) {
            return None;
        }
        at += parts[i].len();
        i += 1;
    }
    let start = at;
    while at < b.len() && b[at] != b'/' {
        at += 1;
    }
    if at == start || at >= b.len() {
        return None;
    }
    let asset = ASSET.as_bytes();
    if !bytes_at(b, at + 1, asset) || at + 1 + asset.len() != b.len() {
        return None;
    }
    match std::str::from_utf8(sub(b, start, at)) {
        Ok(t) => Some(t),
        Err(_) => None,
    }
}

/// Whether `s` is a sha256 digest written as 64 lowercase hex digits.
const fn is_sha256_hex(s: &[u8]) -> bool {
    if s.len() != 64 {
        return false;
    }
    let mut i = 0;
    while i < s.len() {
        if !matches!(s[i], b'0'..=b'9' | b'a'..=b'f') {
            return false;
        }
        i += 1;
    }
    true
}

/// Whether `hay` holds `needle` starting at `at`.
const fn bytes_at(hay: &[u8], at: usize, needle: &[u8]) -> bool {
    if at > hay.len() || hay.len() - at < needle.len() {
        return false;
    }
    let mut i = 0;
    while i < needle.len() {
        if hay[at + i] != needle[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// `b[start..end]`, in a const context.
const fn sub(b: &[u8], start: usize, end: usize) -> &[u8] {
    let (_, rest) = b.split_at(start);
    let (mid, _) = rest.split_at(end - start);
    mid
}
/// GitHub rejects API requests without one.
const UA: &str = "mercs2-workshop";
/// The bundle is a few MB of text; anything past this is a hung connection, not a slow one.
const READ_LIMIT: u64 = 256 * 1024 * 1024;

/// What a download reports back to the UI thread.
pub enum Event {
    Progress(String),
    Done(PathBuf),
    Failed(String),
}

/// `<cache>/mercs2-workshop`, per-OS. Same env-var derivation as [`crate::settings::config_path`] —
/// cache rather than config, because this is re-downloadable data, not the user's own choices.
pub fn cache_dir() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Caches")
    } else {
        match std::env::var_os("XDG_CACHE_HOME") {
            Some(x) if !x.is_empty() => PathBuf::from(x),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".cache"),
        }
    };
    Some(dir.join("mercs2-workshop"))
}

/// Where a fetched bundle lands. `Some` even when nothing is installed yet — callers test `is_dir`.
pub fn installed_dir() -> Option<PathBuf> {
    Some(cache_dir()?.join("workshop_data"))
}

/// The page of the [`PINNED`] release, which carries [`ASSET`] for a download by hand.
pub fn release_page() -> String {
    format!("https://github.com/{REPO}/releases/tag/{}", PINNED.tag)
}

/// How to install the bundle by hand: the download link and the sha256 to check it against, the
/// folder to unpack it into, and the Settings option that points the Workshop at a folder of the
/// user's choosing.
pub fn manual_install_help() -> String {
    let settings = match crate::settings::config_path() {
        Some(p) => format!(
            "Settings > Reference data > Choose... (saved as `data_dir` in {})",
            p.display()
        ),
        None => "Settings > Reference data > Choose...".to_string(),
    };
    let pin = PINNED;
    match installed_dir() {
        Some(dir) => format!(
            "Download {} (sha256 {}) and unpack its workshop_data folder to {}, or point {settings} at the unpacked workshop_data folder.",
            pin.url,
            pin.sha256,
            dir.display()
        ),
        None => format!(
            "Download {} (sha256 {}), unpack it, and point {settings} at the unpacked workshop_data folder.",
            pin.url, pin.sha256
        ),
    }
}

/// Download the pinned bundle and unpack it into [`installed_dir`], reporting progress as it goes.
/// Blocking — run it on a thread and pipe `report` into a channel.
pub fn download(mut report: impl FnMut(Event)) {
    match install(&mut report) {
        Ok(p) => report(Event::Done(p)),
        Err(e) => report(Event::Failed(e)),
    }
}

/// Download the [`PINNED`] bundle and install it into [`installed_dir`], returning that folder.
/// Blocking. `report` receives [`Event::Progress`] lines; the outcome is the return value.
pub fn install(report: &mut impl FnMut(Event)) -> Result<PathBuf, String> {
    let dest = installed_dir().ok_or("no cache directory on this platform")?;
    install_pinned(&PINNED, &dest, get, report)
}

/// Download `pin.url` through `fetch`, check the archive's sha256 against `pin.sha256`, and
/// install the bundle it holds as `dest`. A digest mismatch returns an error before anything is
/// unpacked or removed, so `dest` keeps whatever it held.
fn install_pinned(
    pin: &Pin<'_>,
    dest: &Path,
    fetch: impl Fn(&str) -> Result<Vec<u8>, String>,
    report: &mut impl FnMut(Event),
) -> Result<PathBuf, String> {
    report(Event::Progress(format!("downloading {ASSET} ({})…", pin.tag)));
    let zip_bytes = fetch(pin.url)?;

    report(Event::Progress(format!("checking sha256 of {} KB…", zip_bytes.len() / 1024)));
    let got = crate::publish::sha256_hex(&zip_bytes);
    if got != pin.sha256 {
        return Err(format!(
            "{} has sha256 {got}, but the {} bundle is recorded as {} (BUNDLE_SHA256 in {}); \
             nothing was installed",
            pin.url,
            pin.tag,
            pin.sha256,
            pin_file!()
        ));
    }

    // Unpack to a SIBLING directory first, then swap. Extracting over the live directory would
    // leave a half-written bundle behind if the process died mid-way, and a truncated names.bin
    // would stop every later launch with an unreadable-pack error.
    report(Event::Progress(format!("unpacking {} KB…", zip_bytes.len() / 1024)));
    let staging = dest.with_extension("incoming");
    remove_tree(&staging, "clear an earlier staging copy")?;
    unzip(&zip_bytes, &staging)?;

    // The archive holds a top-level `workshop_data/`; lift it so `dest` IS the bundle root. The
    // installed folder must pass the same completeness test [`crate::index::data_home`] applies
    // (the `manifest.json` that `--pack-data` writes), and must carry the `names.bin` it promises.
    let root = {
        let inner = staging.join("workshop_data");
        if inner.join(crate::index::BUNDLE_MANIFEST).is_file() { inner } else { staging.clone() }
    };
    for required in [crate::index::BUNDLE_MANIFEST, "names.bin"] {
        if !root.join(required).is_file() {
            let missing = format!("the downloaded archive has no {required}");
            return Err(match remove_tree(&staging, "clear the staging copy of that archive") {
                Ok(()) => missing,
                Err(e) => format!("{missing}; {e}"),
            });
        }
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    remove_tree(dest, "replace the installed bundle")?;
    std::fs::rename(&root, dest).map_err(|e| format!("install to {}: {e}", dest.display()))?;
    // When the archive's root was the staging folder itself, the rename moved it and this finds
    // nothing; otherwise it removes the emptied staging folder.
    remove_tree(&staging, "clear the staging folder after the install")?;
    Ok(dest.to_path_buf())
}

/// Remove the directory tree at `path`. A path that does not exist is already gone; any other
/// failure is an error naming the path and what the removal was for.
fn remove_tree(path: &Path, purpose: &str) -> Result<(), String> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove {} (to {purpose}): {e}", path.display())),
    }
}

/// One GET, following redirects (the asset URL redirects to object storage).
fn get(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::builder()
        .user_agent(UA)
        .build()
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(READ_LIMIT)
        .read_to_end(&mut buf)
        .map_err(|e| format!("read {url}: {e}"))?;
    Ok(buf)
}

/// Extract every entry under `out`. Paths come from `enclosed_name`, which rejects absolute paths
/// and `..` traversal — an archive must never be able to write outside the directory we chose.
fn unzip(bytes: &[u8], out: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not a zip archive: {e}"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let Some(rel) = f.enclosed_name() else {
            return Err(format!("archive contains an unsafe path: {}", f.name()));
        };
        let path = out.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut w = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        std::io::copy(&mut f, &mut w).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in entries {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(body).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn unzip_round_trips_a_bundle() {
        let dir = std::env::temp_dir().join("m2ws_unzip_ok");
        let _ = std::fs::remove_dir_all(&dir);
        let z = zip_of(&[
            ("workshop_data/names.bin", b"M2NAMES1____"),
            ("workshop_data/lua/a.lua", b"-- hi"),
        ]);
        unzip(&z, &dir).unwrap();
        assert_eq!(std::fs::read(dir.join("workshop_data/names.bin")).unwrap(), b"M2NAMES1____");
        assert!(dir.join("workshop_data/lua/a.lua").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Zip slip: an entry escaping the destination must be REFUSED, not written. A bundle we fetch
    /// over the network gets to write only inside the directory we chose for it.
    #[test]
    fn unzip_refuses_path_traversal() {
        let dir = std::env::temp_dir().join("m2ws_unzip_evil");
        let _ = std::fs::remove_dir_all(&dir);
        let z = zip_of(&[("../../escaped.txt", b"pwned")]);
        let err = unzip(&z, &dir).unwrap_err();
        assert!(err.contains("unsafe path"), "expected refusal, got: {err}");
        assert!(!dir.parent().unwrap().join("escaped.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty scratch directory unique to this test and process; `install_pinned` writes
    /// `<dir>/workshop_data` and its `.incoming` sibling inside it.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("m2ws_fetch_{name}_{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("clear the scratch dir");
        }
        std::fs::create_dir_all(&dir).expect("make the scratch dir");
        dir
    }

    fn bundle_zip() -> Vec<u8> {
        zip_of(&[
            ("workshop_data/manifest.json", b"{}"),
            ("workshop_data/names.bin", b"M2NAMES1____"),
            ("workshop_data/lua/a.lua", b"-- hi"),
        ])
    }

    /// The compiled-in pin is a release-download URL of this repo's asset (never the `latest`
    /// alias), its tag, and a sha256 digest.
    #[test]
    fn pinned_bundle_is_a_tagged_release_download() {
        assert_eq!(
            PINNED.url,
            format!("https://github.com/{REPO}/releases/download/{}/{ASSET}", PINNED.tag)
        );
        assert!(!PINNED.tag.is_empty() && PINNED.tag != "latest", "tag {:?}", PINNED.tag);
        assert!(is_sha256_hex(PINNED.sha256.as_bytes()), "sha256 {:?}", PINNED.sha256);
        assert_eq!(release_page(), format!("https://github.com/{REPO}/releases/tag/{}", PINNED.tag));
        let help = manual_install_help();
        assert!(help.contains(PINNED.url) && help.contains(PINNED.sha256), "{help}");
        assert!(!format!("{help}{}", release_page()).contains("latest"), "{help}");
    }

    /// CI downloads the same bundle: its pinned-bundle step sources this crate's pin file and
    /// spells neither value out itself.
    #[test]
    fn ci_workflow_sources_the_pin_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(root.join(pin_file!()).is_file(), "{} is not in the repo", pin_file!());
        let ci_path = root.join(".github/workflows/ci.yml");
        let ci = std::fs::read_to_string(&ci_path).expect("read .github/workflows/ci.yml");
        assert!(ci.contains(concat!(". ", pin_file!())), "ci.yml does not source {}", pin_file!());
        for key in ["BUNDLE_URL", "BUNDLE_SHA256"] {
            assert!(
                !ci.lines().any(|l| {
                    let l = l.trim_start();
                    l.starts_with(&format!("{key}:")) || l.starts_with(&format!("{key}="))
                }),
                "ci.yml sets {key} itself; it must come from {}",
                pin_file!()
            );
        }
    }

    #[test]
    fn env_pin_parses_the_key_lines() {
        let sha = "f13605e27652cd4c45fe7a7df91808ff9ab7992974d0605edb7c2f7267ab161d";
        let url = format!("https://github.com/{REPO}/releases/download/v9.9.9/{ASSET}");
        let src = format!(
            "# BUNDLE_URL=https://example.invalid/commented-out\n\nBUNDLE_URL={url}\n  BUNDLE_SHA256={sha}"
        );
        assert_eq!(pin_from_env(&src), Ok(Pin { url: &url, tag: "v9.9.9", sha256: sha }));
    }

    /// Each malformed pin file is refused with its own reason — none yields a pin.
    #[test]
    fn env_pin_refuses_malformed_lines() {
        let sha = "f13605e27652cd4c45fe7a7df91808ff9ab7992974d0605edb7c2f7267ab161d";
        let good = format!("https://github.com/{REPO}/releases/download/v9.9.9/{ASSET}");
        let case = |url: &str, sha: &str| format!("BUNDLE_URL={url}\nBUNDLE_SHA256={sha}\n");
        let refused = |src: String, why: &str| {
            let err = pin_from_env(&src).expect_err(&src);
            assert!(err.contains(why), "{src:?}: expected {why:?}, got {err:?}");
        };
        refused(format!("BUNDLE_SHA256={sha}\n"), "no `BUNDLE_URL=` line");
        refused(format!("BUNDLE_URL={good}\n"), "no `BUNDLE_SHA256=` line");
        refused(format!("BUNDLE_URL: {good}\nBUNDLE_SHA256={sha}\n"), "no `BUNDLE_URL=` line");
        refused(format!("{}BUNDLE_URL={good}\n", case(&good, sha)), "more than one `BUNDLE_URL=`");
        refused(format!("{}BUNDLE_SHA256={sha}\n", case(&good, sha)), "more than one `BUNDLE_SHA256=`");
        for url in [
            String::new(),
            format!(" {good}"),
            format!("{good} "),
            format!("{good}\r"),
            format!("https://github.com/{REPO}/releases/latest/download/{ASSET}"),
            format!("https://github.com/{REPO}/releases/latest/download/{ASSET}"),
            format!("https://github.com/{REPO}/releases/download//{ASSET}"),
            format!("https://github.com/{REPO}/releases/download/v9.9.9/mercs2-workshop-data-debug.zip"),
            format!("https://github.com/someone/else/releases/download/v9.9.9/{ASSET}"),
            format!("http://github.com/{REPO}/releases/download/v9.9.9/{ASSET}"),
            format!("\"https://github.com/{REPO}/releases/download/v9.9.9/{ASSET}\""),
        ] {
            refused(case(&url, sha), "is not https://github.com/");
        }
        for bad in [
            sha[..63].to_string(),
            format!("{sha}\r"),
            sha.to_uppercase(),
            format!("{sha}0"),
            format!("sha256:{}", &sha[..57]),
        ] {
            refused(case(&good, &bad), "not 64 lowercase hex digits");
        }
    }

    /// The install asks for exactly the pinned URL — once, and nothing else — and installs the
    /// bundle when the archive's digest matches the pin.
    #[test]
    fn install_fetches_only_the_pinned_url_and_installs_a_matching_bundle() {
        let dir = scratch("match");
        let dest = dir.join("workshop_data");
        let zip = bundle_zip();
        let sha = crate::publish::sha256_hex(&zip);
        let pin = Pin { url: PINNED.url, tag: PINNED.tag, sha256: &sha };
        let asked = std::cell::RefCell::new(Vec::new());
        let got = install_pinned(
            &pin,
            &dest,
            |url| {
                asked.borrow_mut().push(url.to_string());
                Ok(zip.clone())
            },
            &mut |_| {},
        )
        .expect("install the fixture bundle");
        assert_eq!(asked.into_inner(), vec![PINNED.url.to_string()]);
        assert_eq!(got, dest);
        assert_eq!(std::fs::read(dest.join("names.bin")).unwrap(), b"M2NAMES1____");
        assert!(dest.join("manifest.json").is_file() && dest.join("lua/a.lua").is_file());
        assert!(!dest.with_extension("incoming").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A digest mismatch is an error naming both digests and the URL; nothing is unpacked and the
    /// bundle already installed stays as it was.
    #[test]
    fn install_refuses_a_digest_mismatch_and_keeps_the_installed_bundle() {
        let dir = scratch("mismatch");
        let dest = dir.join("workshop_data");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("names.bin"), b"previous").unwrap();
        let zip = bundle_zip();
        let got = crate::publish::sha256_hex(&zip);
        let want = crate::publish::sha256_hex(b"the archive the pin was recorded from");
        let pin = Pin { url: PINNED.url, tag: PINNED.tag, sha256: &want };
        let err = install_pinned(&pin, &dest, |_| Ok(zip.clone()), &mut |_| {})
            .expect_err("a mismatched archive must not install");
        assert!(err.contains(&got) && err.contains(&want) && err.contains(PINNED.url), "{err}");
        assert!(err.contains("nothing was installed"), "{err}");
        assert!(!dest.with_extension("incoming").exists());
        assert_eq!(std::fs::read(dest.join("names.bin")).unwrap(), b"previous");
        assert!(!dest.join("manifest.json").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// `<dir>/locked/held` with `locked` made read-only, so removing `dir`'s tree fails on `held`.
    /// Returns `locked`, for [`unlock`].
    #[cfg(unix)]
    fn lock_inside(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let locked = dir.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("held"), b"held").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
        locked
    }

    #[cfg(unix)]
    fn unlock(locked: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A staging folder left by an earlier run that cannot be removed stops the install with an
    /// error naming it and the removal; the installed bundle is not touched. (Unix: the fixture is
    /// a directory without write permission, which Windows does not model.)
    #[cfg(unix)]
    #[test]
    fn install_reports_a_staging_folder_it_cannot_remove() {
        let dir = scratch("stuckstaging");
        let dest = dir.join("workshop_data");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("names.bin"), b"previous").unwrap();
        let staging = dest.with_extension("incoming");
        let locked = lock_inside(&staging);
        let zip = bundle_zip();
        let sha = crate::publish::sha256_hex(&zip);
        let pin = Pin { url: PINNED.url, tag: PINNED.tag, sha256: &sha };
        let res = install_pinned(&pin, &dest, |_| Ok(zip.clone()), &mut |_| {});
        unlock(&locked);
        let err = res.expect_err("an unremovable staging folder must stop the install");
        assert!(
            err.starts_with(&format!("remove {} (to clear an earlier staging copy): ", staging.display())),
            "{err}"
        );
        assert_eq!(std::fs::read(dest.join("names.bin")).unwrap(), b"previous");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An installed bundle that cannot be removed stops the install with an error naming it,
    /// rather than the new bundle being dropped in silence. (Unix, as above.)
    #[cfg(unix)]
    #[test]
    fn install_reports_an_installed_bundle_it_cannot_replace() {
        let dir = scratch("stuckdest");
        let dest = dir.join("workshop_data");
        let locked = lock_inside(&dest);
        let zip = bundle_zip();
        let sha = crate::publish::sha256_hex(&zip);
        let pin = Pin { url: PINNED.url, tag: PINNED.tag, sha256: &sha };
        let res = install_pinned(&pin, &dest, |_| Ok(zip.clone()), &mut |_| {});
        unlock(&locked);
        let err = res.expect_err("an unremovable installed bundle must stop the install");
        assert!(
            err.starts_with(&format!("remove {} (to replace the installed bundle): ", dest.display())),
            "{err}"
        );
        assert!(locked.join("held").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An archive without the bundle's required files is refused and its staging copy removed.
    #[test]
    fn install_refuses_an_incomplete_archive_and_clears_its_staging_copy() {
        let dir = scratch("incomplete");
        let dest = dir.join("workshop_data");
        let zip = zip_of(&[("workshop_data/lua/a.lua", b"-- hi")]);
        let sha = crate::publish::sha256_hex(&zip);
        let pin = Pin { url: PINNED.url, tag: PINNED.tag, sha256: &sha };
        let err = install_pinned(&pin, &dest, |_| Ok(zip.clone()), &mut |_| {})
            .expect_err("no manifest, no install");
        assert_eq!(err, format!("the downloaded archive has no {}", crate::index::BUNDLE_MANIFEST));
        assert!(!dest.exists() && !dest.with_extension("incoming").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A failed download is the error; nothing is touched.
    #[test]
    fn install_reports_a_failed_download() {
        let dir = scratch("nodownload");
        let dest = dir.join("workshop_data");
        let err = install_pinned(&PINNED, &dest, |u| Err(format!("{u}: offline")), &mut |_| {})
            .expect_err("no bytes, no install");
        assert_eq!(err, format!("{}: offline", PINNED.url));
        assert!(!dest.exists() && !dest.with_extension("incoming").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
