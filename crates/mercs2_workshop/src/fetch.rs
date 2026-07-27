//! Self-service reference data: fetch `mercs2-workshop-data.zip` from the release and install it.
//!
//! The bundle is built by CI and published as a release asset, but a user who downloads a bare
//! `mercs2_workshop` binary has no way to know that — and with no bundle the tool falls back to the
//! embedded ASET dictionary alone, which is the ASSET namespace: models and textures resolve, and
//! essentially every BONE shows as a bare `0x…`. Asking someone to find a zip on a releases page,
//! unpack it, and drop it in the right directory is the kind of step that simply does not happen.
//!
//! So the tool fetches its own. Nothing here runs on its own initiative: the download happens when
//! the user asks for it, because a tool that reaches out to the network unprompted is a different
//! thing from a tool that can. It installs into the user's cache directory, which is the LAST step
//! of the [`crate::index::data_home`] chain — a bundle the user placed deliberately, or pointed at
//! in Settings, always wins over one we downloaded.

use std::io::Read;
use std::path::{Path, PathBuf};

/// The repo publishing the release assets — the same one the modkit fetches `wad_simulator` from.
const REPO: &str = "Mercenaries-Fan-Build/mercs2-wad-simulator";
/// The release asset built by the workflow's "Build workshop_data reference bundle" step.
const ASSET: &str = "mercs2-workshop-data.zip";
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

/// Download the release bundle and unpack it into [`installed_dir`], reporting progress as it goes.
/// Blocking — run it on a thread and pipe `report` into a channel.
pub fn download(mut report: impl FnMut(Event)) {
    match run(&mut report) {
        Ok(p) => report(Event::Done(p)),
        Err(e) => report(Event::Failed(e)),
    }
}

fn run(report: &mut impl FnMut(Event)) -> Result<PathBuf, String> {
    let dest = installed_dir().ok_or("no cache directory on this platform")?;

    report(Event::Progress("finding the latest release…".into()));
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = get(&api)?;
    let release: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("release JSON: {e}"))?;
    let tag = release["tag_name"].as_str().unwrap_or("latest").to_string();
    // Match the asset by EXACT name. A substring match would happily pick up a future
    // `mercs2-workshop-data-debug.zip` and install the wrong thing without saying so.
    let url = release["assets"]
        .as_array()
        .ok_or("release has no assets")?
        .iter()
        .find(|a| a["name"].as_str() == Some(ASSET))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or_else(|| format!("release {tag} has no asset named '{ASSET}'"))?
        .to_string();

    report(Event::Progress(format!("downloading {ASSET} ({tag})…")));
    let zip_bytes = get(&url)?;

    // Unpack to a SIBLING directory first, then swap. Extracting over the live directory would
    // leave a half-written bundle behind if the process died mid-way, and a truncated names.bin
    // fails the magic check and silently degrades to "no names" — the failure we are removing.
    report(Event::Progress(format!("unpacking {} KB…", zip_bytes.len() / 1024)));
    let staging = dest.with_extension("incoming");
    let _ = std::fs::remove_dir_all(&staging);
    unzip(&zip_bytes, &staging)?;

    // The archive holds a top-level `workshop_data/`; lift it so `dest` IS the bundle root.
    let root = {
        let inner = staging.join("workshop_data");
        if inner.join("names.bin").is_file() { inner } else { staging.clone() }
    };
    if !root.join("names.bin").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the downloaded archive has no names.bin".into());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&root, &dest).map_err(|e| format!("install to {}: {e}", dest.display()))?;
    let _ = std::fs::remove_dir_all(&staging);
    Ok(dest)
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
}
