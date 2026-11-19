//! `fetch_package`: content the player's machine downloads and converts at build, which no Shipment ships.
//!
//! A Shipment names a package by URL, size and sha256 ([`Contribution::FetchPackage`]) and its rows name
//! it with `from:`. [`materialize`] turns such a Shipment into an ordinary one before build, preflight and
//! link see it: it fetches the package into qm's cache ([`cache_dir`]), checks its size and sha256,
//! converts it against the player's `vz.wad`, writes the components into a staging copy of the
//! Shipment, and rewrites every `from:` row into the rows `dlc_port --shipment` would have written. So
//! every later step runs the same code as a Shipment that carries its components itself.
//!
//! The staging copy is keyed by the manifest's text, each package's sha256, the `vz.wad`'s sha256 and
//! this crate's version, and reused while all four are unchanged.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::discover::LoadedShipment;
use crate::manifest::{Contribution, LevelRung};

/// The converter a `fetch_package` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageFormat {
    /// A PS3 PSN `.pkg` of a Mercenaries 2 DLC: decrypted and converted by `dlc_port`.
    Ps3DlcPkg,
}

/// Local copies given for package ids (`--package <id>=<path>`), used instead of downloading. They are
/// checked against the row's size and sha256 like a download.
pub type Overrides = BTreeMap<String, PathBuf>;

/// Parse one `--package <id>=<path>`.
pub fn parse_override(arg: &str) -> Result<(String, PathBuf), String> {
    let (id, path) = arg.split_once('=').ok_or_else(|| format!("--package {arg:?}: expected <id>=<path>"))?;
    if id.is_empty() || path.is_empty() {
        return Err(format!("--package {arg:?}: expected <id>=<path>"));
    }
    Ok((id.to_string(), PathBuf::from(path)))
}

/// qm's cache folder: `QM_CACHE_DIR` when set, else the platform's per-user cache folder
/// (`%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS, `$XDG_CACHE_HOME` or `~/.cache`
/// elsewhere), with `mercs2_quartermaster` appended.
pub fn cache_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("QM_CACHE_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        var("LOCALAPPDATA").ok_or("no cache folder: %LOCALAPPDATA% is not set (set QM_CACHE_DIR)")?
    } else if cfg!(target_os = "macos") {
        var("HOME").ok_or("no cache folder: $HOME is not set (set QM_CACHE_DIR)")?.join("Library").join("Caches")
    } else {
        match var("XDG_CACHE_HOME") {
            Some(d) => d,
            None => var("HOME").ok_or("no cache folder: neither $XDG_CACHE_HOME nor $HOME is set (set QM_CACHE_DIR)")?.join(".cache"),
        }
    };
    Ok(base.join("mercs2_quartermaster"))
}

/// The lowercase hex sha256 of a file.
pub fn file_sha256(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Check that `path` is the package a row names: its size, then its sha256.
pub fn verify(path: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let len = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len != size {
        return Err(format!("{}: {len} bytes, the manifest says {size}", path.display()));
    }
    let actual = file_sha256(path)?;
    if actual != sha256 {
        return Err(format!("{}: sha256 {actual}, the manifest says {sha256}", path.display()));
    }
    Ok(())
}

/// Download `url` to `to`, through a `.part` file renamed once the whole body is written.
fn download(url: &str, to: &Path, log: &mut Vec<String>) -> Result<(), String> {
    let dir = to.parent().ok_or("a cache file has a parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = to.with_extension("part");
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .timeout_read(std::time::Duration::from_secs(120))
        .build();
    let response = agent.get(url).call().map_err(|e| format!("downloading {url}: {e}"))?;
    let mut body = response.into_reader();
    let mut file = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let copied = std::io::copy(&mut body, &mut file).map_err(|e| format!("downloading {url}: {e}"))?;
    file.flush().map_err(|e| format!("{}: {e}", part.display()))?;
    drop(file);
    std::fs::rename(&part, to).map_err(|e| format!("{} -> {}: {e}", part.display(), to.display()))?;
    log.push(format!("downloaded {url} ({copied} bytes)"));
    Ok(())
}

/// The package file for one `fetch_package`: the `--package` copy for its id, else the cached download
/// (`<cache>/packages/<sha256>.pkg`, fetched when absent). Checked against `size` and `sha256` either
/// way; a cached file that fails the check is an error naming it, not something to fetch over.
pub fn package_file(
    id: &str,
    url: &str,
    sha256: &str,
    size: u64,
    cache: &Path,
    overrides: &Overrides,
    log: &mut Vec<String>,
) -> Result<PathBuf, String> {
    let path = match overrides.get(id) {
        Some(p) => p.clone(),
        None => {
            let p = cache.join("packages").join(format!("{sha256}.pkg"));
            if !p.exists() {
                download(url, &p, log)?;
            }
            p
        }
    };
    verify(&path, size, sha256).map_err(|e| format!("fetch_package {id}: {e}"))?;
    Ok(path)
}

/// `0xHHHHHHHH` as its hash, any other name as its `pandemic_hash_m2`.
fn name_hash(name: &str) -> Result<u32, String> {
    match name.strip_prefix("0x").or_else(|| name.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).map_err(|_| format!("{name:?} is not a 0xHHHHHHHH hash")),
        None => Ok(mercs2_formats::hash::pandemic_hash_m2(name)),
    }
}

/// What one package converted to, as `src/`-relative paths in the staging copy.
pub struct Converted {
    /// Each asset's rungs by `(type name, name hash)`: `(block stem, file)`, coarsest first.
    pub assets: BTreeMap<(String, u32), Vec<(String, String)>>,
    /// Each stream's file.
    pub streams: Vec<String>,
}

/// Rewrite every `from:` row of `contributions` with what its package converted to (`converted`, by
/// package id). An `add_level` with `from:` gets the package's streams and is followed by one
/// `level_asset` row per converted asset; a `level_asset` with `from:` gets that asset's rungs.
/// `fetch_package` rows are dropped: nothing after this step reads them.
pub fn expand(contributions: &[Contribution], converted: &BTreeMap<String, Converted>) -> Result<Vec<Contribution>, String> {
    let get = |id: &str| converted.get(id).ok_or_else(|| format!("`from: {id}` names no fetch_package of this Shipment"));
    let mut out = Vec::with_capacity(contributions.len());
    for c in contributions {
        match c {
            Contribution::FetchPackage { .. } => {}
            Contribution::AddLevel { name, display, streams, from: Some(id) } => {
                if !streams.is_empty() {
                    return Err(format!("add_level {name}: `from: {id}` and `streams` both name its streams"));
                }
                let pkg = get(id)?;
                out.push(Contribution::AddLevel {
                    name: name.clone(),
                    display: display.clone(),
                    streams: pkg.streams.iter().map(PathBuf::from).collect(),
                    from: None,
                });
                for ((ty, hash), rungs) in &pkg.assets {
                    out.push(Contribution::LevelAsset {
                        level: name.clone(),
                        asset_type: ty.clone(),
                        name: format!("0x{hash:08X}"),
                        rungs: rungs.iter().map(|(block, file)| LevelRung { block: block.clone(), file: PathBuf::from(file) }).collect(),
                        from: None,
                        templates: Vec::new(),
                        edits: BTreeMap::new(),
                    });
                }
            }
            Contribution::LevelAsset { level, asset_type, name, rungs, from: Some(id), templates, edits } => {
                if !rungs.is_empty() {
                    return Err(format!("level_asset {name}: `from: {id}` and `rungs` both name its rungs"));
                }
                let pkg = get(id)?;
                let key = (asset_type.clone(), name_hash(name)?);
                let found = pkg
                    .assets
                    .get(&key)
                    .ok_or_else(|| format!("level_asset {name} ({asset_type}): package {id} converted no such asset"))?;
                out.push(Contribution::LevelAsset {
                    level: level.clone(),
                    asset_type: asset_type.clone(),
                    name: name.clone(),
                    rungs: found.iter().map(|(block, file)| LevelRung { block: block.clone(), file: PathBuf::from(file) }).collect(),
                    from: None,
                    templates: templates.clone(),
                    edits: edits.clone(),
                });
            }
            other => out.push(other.clone()),
        }
    }
    Ok(out)
}

/// Copy `from` into `to`, recursively, except `skip` (a path under `from`).
fn copy_tree(from: &Path, to: &Path, skip: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
        let src = entry.path();
        if src == skip {
            continue;
        }
        let dst = to.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| format!("{}: {e}", src.display()))?;
        if ty.is_dir() {
            copy_tree(&src, &dst, skip)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))?;
        }
    }
    Ok(())
}

/// `shipment` with every `fetch_package` resolved ([`expand`]), rooted at a staging copy that holds its
/// `src/` (minus any `src/level`) and the converted components. A Shipment without `fetch_package` is
/// returned as it is. `vz_wad` is the player's: the conversion reads its class layouts and leaves to it
/// what it already holds.
pub fn materialize(
    shipment: &LoadedShipment,
    vz_wad: &Path,
    cache: &Path,
    overrides: &Overrides,
    log: &mut Vec<String>,
) -> Result<LoadedShipment, String> {
    let packages: Vec<(&str, PackageFormat, &str, &str, u64)> = shipment
        .manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::FetchPackage { id, format, url, sha256, size } => Some((id.as_str(), *format, url.as_str(), sha256.as_str(), *size)),
            _ => None,
        })
        .collect();
    if packages.is_empty() {
        return Ok(shipment.clone());
    }
    let manifest_text = std::fs::read(&shipment.manifest_path).map_err(|e| format!("{}: {e}", shipment.manifest_path.display()))?;
    let mut files = Vec::new();
    for (id, _, url, sha256, size) in &packages {
        files.push((*id, package_file(id, url, sha256, *size, cache, overrides, log)?));
    }
    let mut key = Sha256::new();
    key.update(env!("CARGO_PKG_VERSION").as_bytes());
    key.update(&manifest_text);
    for (_, _, _, sha256, _) in &packages {
        key.update(sha256.as_bytes());
    }
    key.update(file_sha256(vz_wad)?.as_bytes());
    let staging = cache.join("staging").join(hex(&key.finalize()));
    let complete = staging.join(".complete.json");

    let converted: BTreeMap<String, Converted> = if complete.exists() {
        let text = std::fs::read_to_string(&complete).map_err(|e| format!("{}: {e}", complete.display()))?;
        let stored: BTreeMap<String, StoredConverted> = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", complete.display()))?;
        log.push(format!("reusing converted packages in {}", staging.display()));
        stored.into_iter().map(|(id, s)| (id, s.into())).collect()
    } else {
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
        }
        let src = shipment.root.join("src");
        if src.is_dir() {
            copy_tree(&src, &staging.join("src"), &src.join("level"))?;
        }
        let mut converted = BTreeMap::new();
        for ((id, format, ..), (_, file)) in packages.iter().zip(&files) {
            match format {
                PackageFormat::Ps3DlcPkg => {
                    log.push(format!("converting {} (fetch_package {id})", file.display()));
                    let (doh, _, package_files) = mercs2_formats::dlc_stfs::load_stfs_or_doh_with_streams(file)
                        .map_err(|e| format!("fetch_package {id}: {}: {e}", file.display()))?;
                    let ported = dlc_port::components::port(&doh, &package_files, vz_wad, None).map_err(|e| format!("fetch_package {id}: {e}"))?;
                    let written = dlc_port::components::write_components(&staging, &ported.chunks, &ported.streams)
                        .map_err(|e| format!("fetch_package {id}: {e}"))?;
                    log.push(format!("fetch_package {id}: {} assets, {} streams", written.assets.len(), written.streams.len()));
                    converted.insert(id.to_string(), Converted { assets: written.assets, streams: written.streams });
                }
            }
        }
        if converted.len() > 1 {
            let mut seen: BTreeMap<&(String, u32), &str> = BTreeMap::new();
            for (id, c) in &converted {
                for k in c.assets.keys() {
                    if let Some(prev) = seen.insert(k, id) {
                        return Err(format!("fetch_package {prev} and {id} both convert {} 0x{:08X}", k.0, k.1));
                    }
                }
            }
        }
        let stored: BTreeMap<&String, StoredConverted> = converted.iter().map(|(id, c)| (id, StoredConverted::from(c))).collect();
        let text = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
        std::fs::write(&complete, text).map_err(|e| format!("{}: {e}", complete.display()))?;
        converted
    };

    let mut manifest = shipment.manifest.clone();
    manifest.contributions = expand(&shipment.manifest.contributions, &converted)?;
    Ok(LoadedShipment { root: staging, manifest_path: shipment.manifest_path.clone(), format: shipment.format, manifest })
}

/// [`Converted`] as the staging copy's `.complete.json` stores it.
#[derive(Serialize, Deserialize)]
struct StoredConverted {
    assets: Vec<(String, u32, Vec<(String, String)>)>,
    streams: Vec<String>,
}

impl From<&Converted> for StoredConverted {
    fn from(c: &Converted) -> Self {
        StoredConverted {
            assets: c.assets.iter().map(|((ty, hash), rungs)| (ty.clone(), *hash, rungs.clone())).collect(),
            streams: c.streams.clone(),
        }
    }
}

impl From<StoredConverted> for Converted {
    fn from(s: StoredConverted) -> Self {
        Converted { assets: s.assets.into_iter().map(|(ty, hash, rungs)| ((ty, hash), rungs)).collect(), streams: s.streams }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn converted() -> BTreeMap<String, Converted> {
        let mut assets = BTreeMap::new();
        assets.insert(
            ("model".to_string(), mercs2_formats::hash::pandemic_hash_m2("pmc_hum_obama")),
            vec![("pmc_hum_obama_P000_Q3".to_string(), "src/level/model/0x786ADB07.0.ucfx".to_string())],
        );
        assets.insert(
            ("script".to_string(), 0x1234_5678),
            vec![("scripts_dlc01_P000_Q3".to_string(), "src/level/script/0x12345678.0.ucfx".to_string())],
        );
        BTreeMap::from([("biua".to_string(), Converted { assets, streams: vec!["src/level/Audios/a.pws".to_string()] })])
    }

    fn fetch() -> Contribution {
        Contribution::FetchPackage {
            id: "biua".into(),
            format: PackageFormat::Ps3DlcPkg,
            url: "http://example.invalid/x.pkg".into(),
            sha256: "0".repeat(64),
            size: 1,
        }
    }

    #[test]
    fn an_add_level_from_a_package_becomes_its_streams_and_one_row_per_asset() {
        let rows = vec![fetch(), Contribution::AddLevel { name: "dlc01".into(), display: "D".into(), streams: vec![], from: Some("biua".into()) }];
        let out = expand(&rows, &converted()).unwrap();
        assert_eq!(out.len(), 3, "{out:?}");
        assert!(matches!(&out[0], Contribution::AddLevel { streams, from: None, .. } if streams == &[PathBuf::from("src/level/Audios/a.pws")]));
        let names: Vec<&str> = out[1..]
            .iter()
            .map(|c| match c {
                Contribution::LevelAsset { level, name, rungs, from: None, .. } if level == "dlc01" && rungs.len() == 1 => name.as_str(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(names, ["0x786ADB07", "0x12345678"]);
    }

    #[test]
    fn a_level_asset_from_a_package_takes_that_assets_rungs_by_name() {
        let row = Contribution::LevelAsset {
            level: "vz".into(),
            asset_type: "model".into(),
            name: "pmc_hum_obama".into(),
            rungs: vec![],
            from: Some("biua".into()),
            templates: vec![],
            edits: BTreeMap::new(),
        };
        let out = expand(&[fetch(), row], &converted()).unwrap();
        match &out[..] {
            [Contribution::LevelAsset { name, rungs, from: None, .. }] => {
                assert_eq!(name, "pmc_hum_obama");
                assert_eq!(rungs, &[LevelRung { block: "pmc_hum_obama_P000_Q3".into(), file: "src/level/model/0x786ADB07.0.ucfx".into() }]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_asset_the_package_lacks_or_an_unknown_package_fails() {
        let row = |name: &str, from: &str| Contribution::LevelAsset {
            level: "vz".into(),
            asset_type: "model".into(),
            name: name.into(),
            rungs: vec![],
            from: Some(from.into()),
            templates: vec![],
            edits: BTreeMap::new(),
        };
        assert!(expand(&[row("nothing_here", "biua")], &converted()).unwrap_err().contains("converted no such asset"));
        assert!(expand(&[row("pmc_hum_obama", "other")], &converted()).unwrap_err().contains("names no fetch_package"));
    }

    #[test]
    fn a_package_file_of_the_wrong_size_or_hash_is_refused() {
        let dir = std::env::temp_dir().join(format!("qm-package-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("p.pkg");
        std::fs::write(&file, b"abc").unwrap();
        let sha = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        verify(&file, 3, sha).unwrap();
        assert!(verify(&file, 4, sha).unwrap_err().contains("3 bytes"));
        assert!(verify(&file, 3, &"0".repeat(64)).unwrap_err().contains("sha256"));
        let overrides = Overrides::from([("biua".to_string(), file.clone())]);
        assert!(package_file("biua", "http://example.invalid/", sha, 3, &dir, &overrides, &mut Vec::new()).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_override_needs_an_id_and_a_path() {
        assert_eq!(parse_override("biua=/x/y.pkg").unwrap(), ("biua".to_string(), PathBuf::from("/x/y.pkg")));
        assert!(parse_override("biua").is_err());
        assert!(parse_override("=x").is_err());
    }
}
