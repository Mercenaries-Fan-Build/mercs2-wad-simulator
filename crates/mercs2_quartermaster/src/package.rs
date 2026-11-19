//! `fetch_package`: content the player's machine downloads and converts at build, which no Shipment ships.
//!
//! A Shipment names a package by URL, size and sha256 ([`Contribution::FetchPackage`]) and its rows name
//! it with `from:`. [`materialize`] turns such a Shipment into an ordinary one before build, preflight and
//! link see it: it checks every `fetch_package` row ([`row_problems`]), fetches the package into qm's cache
//! ([`cache_dir`]) and checks its size and sha256, converts it against the player's `vz.wad`, and roots the
//! Shipment at a staging copy that holds its `src/` and the converted components, with every `from:` row
//! rewritten into the rows `dlc_port --shipment` would have written. So every later step runs the same code
//! as a Shipment that carries its components itself.
//!
//! The cache holds:
//! - `packages/<sha256>.pkg`: a downloaded package, renamed into place only once its size and sha256 match.
//! - `converted/<key>/`: one package's components (`src/level/...`) and `.complete.json`, keyed by this
//!   crate's version, the package's sha256 and the `vz.wad`'s sha256 ([`converted_key`]). The conversion
//!   writes into a temporary folder renamed to `<key>` when it is complete, under `converted/<key>.lock`.
//! - `staging/<shipment>/<run>/`: one build's copy of the Shipment: its `src/` (minus `src/level`) copied,
//!   and the converted files hard-linked in. A build makes a fresh one and deletes the Shipment's older
//!   ones that no other qm holds (`<run>.lock`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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

/// Parse every `--package <id>=<path>`; an id given twice is refused.
pub fn parse_overrides(args: &[String]) -> Result<Overrides, String> {
    let mut overrides = Overrides::new();
    for arg in args {
        let (id, path) = parse_override(arg)?;
        if overrides.insert(id.clone(), path).is_some() {
            return Err(format!("--package {id}: given more than once"));
        }
    }
    Ok(overrides)
}

/// Each `--package` id must name a `fetch_package` of `shipments`, the whole set being built or linked.
pub fn check_overrides(overrides: &Overrides, shipments: &[&LoadedShipment]) -> Result<(), String> {
    let ids: BTreeSet<&str> = shipments.iter().flat_map(|s| packages_of(s).into_iter().map(|p| p.id)).collect();
    for id in overrides.keys() {
        if !ids.contains(id.as_str()) {
            return Err(format!("--package {id}: no fetch_package of the Shipment(s) given has this id"));
        }
    }
    Ok(())
}

/// Whether `shipment` has a `fetch_package` row.
pub fn has_package(shipment: &LoadedShipment) -> bool {
    !packages_of(shipment).is_empty()
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

/// `url` as an `http://` or `https://` URL with a host, or why not.
fn web_url(url: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("is not a URL: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("is not http:// or https://".to_string());
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("names no host".to_string());
    }
    Ok(parsed)
}

/// What is wrong with one `fetch_package` row's fields: an `id` that is not a lowercase `[a-z0-9_]` token,
/// a `url` that is not an `http://` or `https://` URL with a host, a `sha256` that is not 64 lowercase hex
/// digits, a zero `size`. M0316 reports each; [`materialize`] refuses a row with any before it touches the
/// filesystem or the network.
pub fn row_problems(id: &str, url: &str, sha256: &str, size: u64) -> Vec<String> {
    let mut out = Vec::new();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') {
        out.push(format!("fetch_package id {id:?}: not a lowercase [a-z0-9_] token"));
    }
    if let Err(why) = web_url(url) {
        out.push(format!("fetch_package {id}: url {url:?} {why}"));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        out.push(format!("fetch_package {id}: sha256 {sha256:?} is not 64 lowercase hex digits"));
    }
    if size == 0 {
        out.push(format!("fetch_package {id}: size is 0"));
    }
    out
}

/// One `fetch_package` row.
struct Package<'a> {
    id: &'a str,
    format: PackageFormat,
    url: &'a str,
    sha256: &'a str,
    size: u64,
}

fn packages_of(shipment: &LoadedShipment) -> Vec<Package<'_>> {
    shipment
        .manifest
        .contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::FetchPackage { id, format, url, sha256, size } => {
                Some(Package { id, format: *format, url, sha256, size: *size })
            }
            _ => None,
        })
        .collect()
}

/// Check that `path` is the package a row names: a regular file, its size, then its sha256.
pub fn verify(path: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a regular file", path.display()));
    }
    let len = meta.len();
    if len != size {
        return Err(format!("{}: {len} bytes, the manifest says {size}", path.display()));
    }
    let actual = file_sha256(path)?;
    if actual != sha256 {
        return Err(format!("{}: sha256 {actual}, the manifest says {sha256}", path.display()));
    }
    Ok(())
}

/// A name no other call in this process gives: the process id, the time and a counter.
fn unique() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{}-{nanos}-{n}", std::process::id())
}

/// The slowest download [`download_deadline`] allows for: 64 KiB/s, about half a megabit.
const MIN_DOWNLOAD_RATE: u64 = 64 * 1024;
/// Time on top of the transfer for connecting, redirects and a slow start.
const DOWNLOAD_SLACK: Duration = Duration::from_secs(300);
/// Redirects a download follows.
const MAX_REDIRECTS: usize = 5;

/// How long a download of `size` bytes may take in all, redirects included: `size` at
/// [`MIN_DOWNLOAD_RATE`] plus [`DOWNLOAD_SLACK`]. BIUA's 407,578,320-byte package gets 6,219 s + 300 s,
/// about 1 h 49 min, which a connection of half a megabit or better finishes. ureq's overall timeout
/// replaces its per-read timeout (`AgentBuilder::timeout_read` and `timeout`), so a stalled transfer
/// fails at this deadline.
pub fn download_deadline(size: u64) -> Duration {
    DOWNLOAD_SLACK + Duration::from_secs(size / MIN_DOWNLOAD_RATE)
}

/// Download `url` to `to` within `deadline`: into a `.part` file of its own beside `to`, at most `size`
/// bytes, hashed as written, renamed to `to` only when its size and sha256 match. A redirect is followed
/// only to another `http://` or `https://` URL, at most [`MAX_REDIRECTS`] times, each logged. qm asks for
/// the body unencoded and refuses any other encoding but gzip, which ureq decodes itself, dropping the
/// `Content-Encoding` and `Content-Length` (ureq `response.rs`, "automatic decompression"); so the decoded
/// bytes are what is counted and hashed. A `Content-Length` must be `size`. On any failure the `.part` is
/// deleted.
fn download(url: &str, size: u64, sha256: &str, to: &Path, deadline: Duration, log: &mut Vec<String>) -> Result<(), String> {
    let dir = to.parent().ok_or("a cache file has a parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let part = dir.join(format!("{sha256}.{}.part", unique()));
    let result = download_to(url, size, sha256, &part, deadline, log)
        .and_then(|n| std::fs::rename(&part, to).map(|_| n).map_err(|e| format!("{} -> {}: {e}", part.display(), to.display())));
    match result {
        Ok(n) => {
            log.push(format!("downloaded {url} ({n} bytes)"));
            Ok(())
        }
        Err(e) => match std::fs::remove_file(&part) {
            Ok(()) => Err(e),
            Err(rm) if rm.kind() == std::io::ErrorKind::NotFound => Err(e),
            Err(rm) => Err(format!("{e}; and removing {}: {rm}", part.display())),
        },
    }
}

/// [`download`]'s transfer into `part`, which it creates; the bytes written.
fn download_to(url: &str, size: u64, sha256: &str, part: &Path, deadline: Duration, log: &mut Vec<String>) -> Result<u64, String> {
    let end = Instant::now() + deadline;
    let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(30)).redirects(0).build();
    let mut current = web_url(url).map_err(|why| format!("downloading {url}: the url {why}"))?;
    let mut hops = 0;
    let response = loop {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(format!("downloading {url}: no response within {}s", deadline.as_secs()));
        }
        let response = agent
            .request_url("GET", &current)
            .set("Accept-Encoding", "identity")
            .timeout(left)
            .call()
            .map_err(|e| format!("downloading {current}: {e}"))?;
        if !(300..400).contains(&response.status()) {
            break response;
        }
        hops += 1;
        if hops > MAX_REDIRECTS {
            return Err(format!("downloading {url}: more than {MAX_REDIRECTS} redirects"));
        }
        let location = response
            .header("Location")
            .ok_or_else(|| format!("downloading {current}: status {} with no Location", response.status()))?;
        let next = current.join(location).map_err(|e| format!("downloading {current}: redirect to {location:?}: {e}"))?;
        web_url(next.as_str()).map_err(|why| format!("downloading {current}: it redirects to {next}, which {why}"))?;
        log.push(format!("{current} redirects to {next}"));
        current = next;
    };
    if response.status() != 200 {
        return Err(format!("downloading {current}: status {} {}", response.status(), response.status_text()));
    }
    if let Some(encoding) = response.header("Content-Encoding").filter(|e| !e.eq_ignore_ascii_case("identity")) {
        return Err(format!("downloading {current}: the body is {encoding}-encoded, and qm asked for it unencoded"));
    }
    if let Some(length) = response.header("Content-Length") {
        match length.trim().parse::<u64>() {
            Ok(n) if n == size => {}
            Ok(n) => return Err(format!("downloading {current}: the server sends {n} bytes, the manifest says {size}")),
            Err(_) => return Err(format!("downloading {current}: Content-Length {length:?} is not a number")),
        }
    }
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(part).map_err(|e| format!("{}: {e}", part.display()))?;
    let mut body = response.into_reader().take(size + 1);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut written = 0u64;
    loop {
        let n = body.read(&mut buf).map_err(|e| format!("downloading {current}: {e}"))?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > size {
            return Err(format!("downloading {current}: the server sent more than the {size} bytes the manifest says"));
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", part.display()))?;
    }
    if written != size {
        return Err(format!("downloading {current}: {written} bytes, the manifest says {size}"));
    }
    let actual = hex(&hasher.finalize());
    if actual != sha256 {
        return Err(format!("downloading {current}: sha256 {actual}, the manifest says {sha256}"));
    }
    file.sync_all().map_err(|e| format!("{}: {e}", part.display()))?;
    Ok(written)
}

/// The package file for one `fetch_package`: the `--package` copy for its id, else the cached download
/// (`<cache>/packages/<sha256>.pkg`, fetched when absent). A `--package` copy or a cached file is checked
/// against `size` and `sha256`; a cached file that fails is an error naming it. `package` has passed
/// [`row_problems`].
fn package_file(package: &Package, cache: &Path, overrides: &Overrides, log: &mut Vec<String>) -> Result<PathBuf, String> {
    let id = package.id;
    if let Some(p) = overrides.get(id) {
        verify(p, package.size, package.sha256).map_err(|e| format!("fetch_package {id}: --package {e}"))?;
        return Ok(p.clone());
    }
    let p = cache.join("packages").join(format!("{}.pkg", package.sha256));
    match std::fs::symlink_metadata(&p) {
        Ok(_) => verify(&p, package.size, package.sha256)
            .map_err(|e| format!("fetch_package {id}: the cached package {e}; delete it so the next build downloads it again"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => download(
            package.url,
            package.size,
            package.sha256,
            &p,
            download_deadline(package.size),
            log,
        )
        .map_err(|e| format!("fetch_package {id}: {e}"))?,
        Err(e) => return Err(format!("fetch_package {id}: {}: {e}", p.display())),
    }
    Ok(p)
}

/// Check that `file` starts with the magic of `format`, before anything reads it whole.
pub fn check_format(file: &Path, format: PackageFormat) -> Result<(), String> {
    let mut magic = [0u8; 4];
    let mut f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    f.read_exact(&mut magic).map_err(|e| format!("{}: reading its magic: {e}", file.display()))?;
    match format {
        PackageFormat::Ps3DlcPkg if &magic == mercs2_formats::ps3_pkg::PKG_MAGIC => Ok(()),
        PackageFormat::Ps3DlcPkg => Err(format!(
            "{}: starts {magic:02X?}, not a PS3 package's {:02X?} (format: ps3_dlc_pkg)",
            file.display(),
            mercs2_formats::ps3_pkg::PKG_MAGIC
        )),
    }
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

/// Copy `from` into `to`, recursively, except `skip` (a path under `from`). Only folders and regular files
/// are copied: a symlink or any other kind of file is refused, naming it, so nothing outside the Shipment
/// reaches the staging copy.
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
        if ty.is_symlink() {
            return Err(format!("{}: a symlink; a Shipment with a fetch_package copies only folders and regular files", src.display()));
        } else if ty.is_dir() {
            copy_tree(&src, &dst, skip)?;
        } else if ty.is_file() {
            std::fs::copy(&src, &dst).map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))?;
        } else {
            return Err(format!("{}: neither a folder nor a regular file ({ty:?})", src.display()));
        }
    }
    Ok(())
}

/// Hard-link every file under `from` to the same path under `to`, making folders as needed. A file that
/// already exists at `to`, or a link the filesystem refuses, is an error.
fn link_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", from.display()))?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let ty = entry.file_type().map_err(|e| format!("{}: {e}", src.display()))?;
        if ty.is_dir() {
            link_tree(&src, &dst)?;
        } else if ty.is_file() {
            std::fs::hard_link(&src, &dst).map_err(|e| {
                format!(
                    "hard-linking {} to {}: {e} (the converted package and the staging copy are both in qm's cache, which must be on a filesystem with hard links)",
                    src.display(),
                    dst.display()
                )
            })?;
        } else {
            return Err(format!("{}: neither a folder nor a regular file in the converted package ({ty:?})", src.display()));
        }
    }
    Ok(())
}

/// The name of one package's converted folder: this crate's version, the package's sha256 and the
/// `vz.wad`'s sha256, hashed.
fn converted_key(package_sha256: &str, vz_sha256: &str) -> String {
    let mut key = Sha256::new();
    for part in [env!("CARGO_PKG_VERSION"), package_sha256, vz_sha256] {
        key.update(part.as_bytes());
        key.update([0]);
    }
    hex(&key.finalize())
}

/// Open (creating) `path` and take its exclusive lock, waiting for it.
fn lock(path: &Path) -> Result<std::fs::File, String> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    f.lock().map_err(|e| format!("locking {}: {e}", path.display()))?;
    Ok(f)
}

/// Read a converted folder's `.complete.json`.
fn read_complete(dir: &Path) -> Result<Converted, String> {
    let complete = dir.join(".complete.json");
    let text = std::fs::read_to_string(&complete)
        .map_err(|e| format!("{}: {e}; this converted package is incomplete, delete {} to convert it again", complete.display(), dir.display()))?;
    let stored: StoredConverted = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", complete.display()))?;
    Ok(stored.into())
}

/// Converts one package file into a folder: `(format, package file, output root, log)`. The output root
/// gets `src/level/...`.
pub type Converter<'a> = dyn FnMut(PackageFormat, &Path, &Path, &mut Vec<String>) -> Result<Converted, String> + 'a;

/// One package's converted folder, converting it on a miss ([`converted_key`]). The conversion runs under
/// `converted/<key>.lock` into a temporary folder renamed to `converted/<key>` once `.complete.json` is
/// in it, so a `converted/<key>` folder is always complete.
fn converted_package(
    package: &Package,
    vz_sha256: &str,
    cache: &Path,
    overrides: &Overrides,
    convert: &mut Converter,
    log: &mut Vec<String>,
) -> Result<(PathBuf, Converted), String> {
    let id = package.id;
    let root = cache.join("converted");
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let key = converted_key(package.sha256, vz_sha256);
    let dir = root.join(&key);
    let _lock = lock(&root.join(format!("{key}.lock")))?;
    if dir.exists() {
        // A `--package` copy is checked even when its conversion is cached: a path given is never ignored.
        if let Some(local) = overrides.get(id) {
            verify(local, package.size, package.sha256).map_err(|e| format!("fetch_package {id}: --package {e}"))?;
        }
        log.push(format!("fetch_package {id}: reusing the conversion in {}", dir.display()));
        return Ok((dir.clone(), read_complete(&dir)?));
    }
    let file = package_file(package, cache, overrides, log)?;
    let tmp = root.join(format!("{key}.{}.tmp", unique()));
    let result = (|| {
        std::fs::create_dir(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
        log.push(format!("converting {} (fetch_package {id})", file.display()));
        let converted = convert(package.format, &file, &tmp, log).map_err(|e| format!("fetch_package {id}: {e}"))?;
        log.push(format!("fetch_package {id}: {} assets, {} streams", converted.assets.len(), converted.streams.len()));
        let text = serde_json::to_string(&StoredConverted::from(&converted)).map_err(|e| e.to_string())?;
        let complete = tmp.join(".complete.json");
        std::fs::write(&complete, text).map_err(|e| format!("{}: {e}", complete.display()))?;
        std::fs::rename(&tmp, &dir).map_err(|e| format!("{} -> {}: {e}", tmp.display(), dir.display()))?;
        Ok(converted)
    })();
    match result {
        Ok(converted) => Ok((dir, converted)),
        Err(e) => match std::fs::remove_dir_all(&tmp) {
            Ok(()) => Err(e),
            Err(rm) if rm.kind() == std::io::ErrorKind::NotFound => Err(e),
            Err(rm) => Err(format!("{e}; and removing {}: {rm}", tmp.display())),
        },
    }
}

/// Delete every staging copy in `dir` but `keep` whose lock no other qm holds.
fn remove_old_staging(dir: &Path, keep: &str, log: &mut Vec<String>) -> Result<(), String> {
    let mut runs = BTreeSet::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let run = name.strip_suffix(".lock").unwrap_or(&name).to_string();
        if run != keep {
            runs.insert(run);
        }
    }
    for run in runs {
        let lock_path = dir.join(format!("{run}.lock"));
        let f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| format!("{}: {e}", lock_path.display()))?;
        match f.try_lock() {
            Ok(()) => {
                let old = dir.join(&run);
                match std::fs::remove_dir_all(&old) {
                    Ok(()) => log.push(format!("removed the older staging copy {}", old.display())),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("{}: {e}", old.display())),
                }
                std::fs::remove_file(&lock_path).map_err(|e| format!("{}: {e}", lock_path.display()))?;
            }
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(e)) => return Err(format!("locking {}: {e}", lock_path.display())),
        }
    }
    Ok(())
}

/// A Shipment ready for build, preflight and link, and the lock on its staging copy: while this is
/// alive no other qm deletes the copy. A Shipment without `fetch_package` has no staging copy and no lock.
pub struct Staged {
    pub shipment: LoadedShipment,
    _lock: Option<std::fs::File>,
}

impl From<LoadedShipment> for Staged {
    fn from(shipment: LoadedShipment) -> Self {
        Staged { shipment, _lock: None }
    }
}

/// `shipment` with every `fetch_package` resolved ([`expand`]), rooted at a fresh staging copy that holds
/// its `src/` (minus any `src/level`) and the converted components. A Shipment without `fetch_package` is
/// returned as it is. `vz_wad` is the player's, and `vz_sha256` its sha256: the conversion reads its class
/// layouts and leaves to it what it already holds.
pub fn materialize(
    shipment: &LoadedShipment,
    vz_wad: &Path,
    vz_sha256: &str,
    cache: &Path,
    overrides: &Overrides,
    log: &mut Vec<String>,
) -> Result<Staged, String> {
    materialize_with(shipment, vz_sha256, cache, overrides, log, &mut |format, file, out, log| match format {
        PackageFormat::Ps3DlcPkg => {
            check_format(file, format)?;
            let (doh, _, package_files) =
                mercs2_formats::dlc_stfs::load_stfs_or_doh_with_streams(file).map_err(|e| format!("{}: {e}", file.display()))?;
            let ported = dlc_port::components::port(&doh, &package_files, vz_wad, None)?;
            let written = dlc_port::components::write_components(out, &ported.chunks, &ported.streams)?;
            log.push(format!("wrote {} asset rungs and {} streams", ported.chunks.len(), written.streams.len()));
            Ok(Converted { assets: written.assets, streams: written.streams })
        }
    })
}

/// [`materialize`] with the conversion given.
fn materialize_with(
    shipment: &LoadedShipment,
    vz_sha256: &str,
    cache: &Path,
    overrides: &Overrides,
    log: &mut Vec<String>,
    convert: &mut Converter,
) -> Result<Staged, String> {
    let packages = packages_of(shipment);
    if packages.is_empty() {
        return Ok(shipment.clone().into());
    }
    let mut ids = BTreeSet::new();
    for p in &packages {
        let problems = row_problems(p.id, p.url, p.sha256, p.size);
        if !problems.is_empty() {
            return Err(problems.join("; "));
        }
        if !ids.insert(p.id) {
            return Err(format!("fetch_package id {:?}: more than one fetch_package has it", p.id));
        }
    }

    let mut converted = BTreeMap::new();
    let mut folders = Vec::new();
    for p in &packages {
        let (dir, c) = converted_package(p, vz_sha256, cache, overrides, convert, log)?;
        folders.push(dir);
        converted.insert(p.id.to_string(), c);
    }
    let mut assets: BTreeMap<&(String, u32), &str> = BTreeMap::new();
    let mut streams: BTreeMap<&str, &str> = BTreeMap::new();
    for (id, c) in &converted {
        for k in c.assets.keys() {
            if let Some(prev) = assets.insert(k, id) {
                return Err(format!("fetch_package {prev} and {id} both convert {} 0x{:08X}", k.0, k.1));
            }
        }
        for s in &c.streams {
            if let Some(prev) = streams.insert(s, id) {
                return Err(format!("fetch_package {prev} and {id} both convert the stream {s}"));
            }
        }
    }

    let canonical = std::fs::canonicalize(&shipment.root).map_err(|e| format!("{}: {e}", shipment.root.display()))?;
    let mut key = Sha256::new();
    key.update(canonical.to_string_lossy().as_bytes());
    let shipment_key = hex(&key.finalize());
    let runs = cache.join("staging").join(&shipment_key);
    std::fs::create_dir_all(&runs).map_err(|e| format!("{}: {e}", runs.display()))?;
    let run = unique();
    // Taking a run's lock and deleting older runs both happen under the Shipment's lock, so a run's lock
    // file exists only once its owner holds it.
    let guard = lock(&cache.join("staging").join(format!("{shipment_key}.lock")))?;
    let held = lock(&runs.join(format!("{run}.lock")))?;
    remove_old_staging(&runs, &run, log)?;
    drop(guard);
    let staging = runs.join(&run);
    std::fs::create_dir(&staging).map_err(|e| format!("{}: {e}", staging.display()))?;
    let src = shipment.root.join("src");
    if src.is_dir() {
        copy_tree(&src, &staging.join("src"), &src.join("level"))?;
    }
    for dir in &folders {
        link_tree(&dir.join("src"), &staging.join("src"))?;
    }
    log.push(format!("staging copy {}", staging.display()));

    let mut manifest = shipment.manifest.clone();
    manifest.contributions = expand(&shipment.manifest.contributions, &converted)?;
    Ok(Staged {
        shipment: LoadedShipment { root: staging, manifest_path: shipment.manifest_path.clone(), format: shipment.format, manifest },
        _lock: Some(held),
    })
}

/// [`Converted`] as a converted folder's `.complete.json` stores it.
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
    use std::net::{TcpListener, TcpStream};

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

    /// sha256 of `abc` and of `abd`.
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    const ABD: &str = "a52d159f262b2c6ddb724a61840befc36eb30c88877a4030b65cbe86298449c9";

    /// A new empty folder under the system temp folder.
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("qm-package-{name}-{}", unique()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The names in `dir`, sorted; none when it does not exist.
    fn names_in(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut v: Vec<String> = entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        v.sort();
        v
    }

    #[test]
    fn a_package_file_of_the_wrong_size_or_hash_or_kind_is_refused() {
        let dir = temp("verify");
        let file = dir.join("p.pkg");
        std::fs::write(&file, b"abc").unwrap();
        verify(&file, 3, ABC).unwrap();
        assert!(verify(&file, 4, ABC).unwrap_err().contains("3 bytes"));
        assert!(verify(&file, 3, &"0".repeat(64)).unwrap_err().contains("sha256"));
        assert!(verify(&dir, 3, ABC).unwrap_err().contains("not a regular file"));
        let package = Package { id: "biua", format: PackageFormat::Ps3DlcPkg, url: "http://127.0.0.1:9/", sha256: ABC, size: 3 };
        let overrides = Overrides::from([("biua".to_string(), file.clone())]);
        assert_eq!(package_file(&package, &dir, &overrides, &mut Vec::new()).unwrap(), file);
        let folder = Overrides::from([("biua".to_string(), dir.clone())]);
        assert!(package_file(&package, &dir, &folder, &mut Vec::new()).unwrap_err().contains("not a regular file"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_override_needs_an_id_and_a_path_and_is_given_once() {
        assert_eq!(parse_override("biua=/x/y.pkg").unwrap(), ("biua".to_string(), PathBuf::from("/x/y.pkg")));
        assert!(parse_override("biua").is_err());
        assert!(parse_override("=x").is_err());
        assert_eq!(parse_overrides(&["a=/x".to_string(), "b=/y".to_string()]).unwrap().len(), 2);
        assert!(parse_overrides(&["a=/x".to_string(), "a=/y".to_string()]).unwrap_err().contains("more than once"));
    }

    #[test]
    fn a_rows_fields_are_checked_by_one_rule() {
        assert!(row_problems("biua_ps3", "http://zeus.dl.playstation.net/x.pkg", ABC, 3).is_empty());
        assert!(row_problems("biua_ps3", "https://h/x", ABC, 3).is_empty());
        for (id, url, sha, size, says) in [
            ("Biua", "http://h/x", ABC, 3, "token"),
            ("", "http://h/x", ABC, 3, "token"),
            ("biua", "ftp://h/x", ABC, 3, "is not http:// or https://"),
            ("biua", "file:///etc/passwd", ABC, 3, "is not http:// or https://"),
            ("biua", "http://", ABC, 3, "url"),
            ("biua", "not a url", ABC, 3, "is not a URL"),
            ("biua", "http://h/x", "../../escape", 3, "64 lowercase hex"),
            ("biua", "http://h/x", "/abs/path", 3, "64 lowercase hex"),
            ("biua", "http://h/x", &ABC.to_uppercase(), 3, "64 lowercase hex"),
            ("biua", "http://h/x", ABC, 0, "size is 0"),
        ] {
            let problems = row_problems(id, url, sha, size);
            assert!(problems.iter().any(|p| p.contains(says)), "{id} {url} {sha} {size}: {problems:?}");
        }
    }

    #[test]
    fn a_package_whose_magic_is_not_its_formats_is_refused_before_it_is_read() {
        let dir = temp("magic");
        let check = |name: &str, bytes: &[u8]| {
            let f = dir.join(name);
            std::fs::write(&f, bytes).unwrap();
            check_format(&f, PackageFormat::Ps3DlcPkg)
        };
        check("ps3.pkg", b"\x7fPKG\0\0\0\0").unwrap();
        for (name, bytes) in [("live.pkg", &b"LIVE\0\0\0\0"[..]), ("con.pkg", b"CON \0\0\0\0"), ("scff.pkg", b"SCFF\0\0\0\0"), ("short.pkg", b"\x7fP")] {
            assert!(check(name, bytes).is_err(), "{name}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// An HTTP server on 127.0.0.1 answering each request by its path; the base URL.
    fn serve() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                std::thread::spawn(move || {
                    let mut request = Vec::new();
                    let mut b = [0u8; 1];
                    while !request.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut b).unwrap_or(0) == 0 {
                            return;
                        }
                        request.push(b[0]);
                    }
                    let text = String::from_utf8_lossy(&request).into_owned();
                    respond(text.split_whitespace().nth(1).unwrap_or(""), &mut stream);
                });
            }
        });
        base
    }

    fn respond(path: &str, s: &mut TcpStream) {
        let head = |s: &mut TcpStream, status: &str, headers: &str| {
            s.write_all(format!("HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n").as_bytes()).is_ok()
        };
        match path {
            "/abc" => _ = head(s, "200 OK", "Content-Length: 3\r\n") && s.write_all(b"abc").is_ok(),
            "/abd" => _ = head(s, "200 OK", "Content-Length: 3\r\n") && s.write_all(b"abd").is_ok(),
            "/abcd" => _ = head(s, "200 OK", "Content-Length: 4\r\n") && s.write_all(b"abcd").is_ok(),
            "/gzip" => _ = head(s, "200 OK", "Content-Length: 3\r\nContent-Encoding: gzip\r\n") && s.write_all(b"abc").is_ok(),
            "/deflate" => _ = head(s, "200 OK", "Content-Length: 3\r\nContent-Encoding: deflate\r\n") && s.write_all(b"abc").is_ok(),
            "/gzip-big" => {
                let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                gz.write_all(&vec![0u8; 1 << 20]).unwrap();
                let body = gz.finish().unwrap();
                _ = head(s, "200 OK", &format!("Content-Length: {}\r\nContent-Encoding: gzip\r\n", body.len())) && s.write_all(&body).is_ok();
            }
            "/endless" => {
                if head(s, "200 OK", "") {
                    let zeros = [0u8; 1 << 16];
                    for _ in 0..1024 {
                        if s.write_all(&zeros).is_err() {
                            return;
                        }
                    }
                }
            }
            "/drip" => {
                if head(s, "200 OK", "Content-Length: 3\r\n") {
                    for b in b"abc" {
                        std::thread::sleep(Duration::from_millis(1500));
                        if s.write_all(&[*b]).is_err() {
                            return;
                        }
                    }
                }
            }
            "/to-file" => _ = head(s, "302 Found", "Location: file:///etc/passwd\r\nContent-Length: 0\r\n"),
            "/to-abc" => _ = head(s, "302 Found", "Location: /abc\r\nContent-Length: 0\r\n"),
            "/loop" => _ = head(s, "302 Found", "Location: /loop\r\nContent-Length: 0\r\n"),
            _ => _ = head(s, "404 Not Found", "Content-Length: 0\r\n"),
        }
    }

    #[test]
    fn a_bad_download_leaves_nothing_in_the_cache_and_a_good_one_lands() {
        let base = serve();
        let dir = temp("download");
        let to = dir.join("packages").join(format!("{ABC}.pkg"));
        let get = |path: &str, deadline: Duration| download(&format!("{base}{path}"), 3, ABC, &to, deadline, &mut Vec::new());
        let minute = Duration::from_secs(60);
        for (path, deadline, says) in [
            ("/abd", minute, "sha256"),
            ("/abcd", minute, "sends 4 bytes"),
            ("/gzip", minute, "unexpected end of file"),
            ("/deflate", minute, "deflate-encoded"),
            ("/gzip-big", minute, "more than the 3 bytes"),
            ("/endless", minute, "more than the 3 bytes"),
            ("/drip", Duration::from_secs(1), "downloading"),
            ("/to-file", minute, "file:///etc/passwd"),
            ("/loop", minute, "more than 5 redirects"),
            ("/missing", minute, "404"),
        ] {
            let e = get(path, deadline).unwrap_err();
            assert!(e.contains(says), "{path}: {e}");
            assert_eq!(names_in(&dir.join("packages")), Vec::<String>::new(), "{path} left a file");
        }
        let mut log = Vec::new();
        download(&format!("{base}/to-abc"), 3, ABC, &to, minute, &mut log).unwrap();
        assert!(log.iter().any(|l| l.contains("redirects to") && l.ends_with("/abc")), "{log:?}");
        assert_eq!(std::fs::read(&to).unwrap(), b"abc");
        assert_eq!(names_in(&dir.join("packages")), [format!("{ABC}.pkg")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_downloads_of_one_package_at_once_each_use_their_own_part_file() {
        let base = serve();
        let dir = temp("download-race");
        let to = dir.join("packages").join(format!("{ABC}.pkg"));
        std::thread::scope(|scope| {
            let runs: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| download(&format!("{base}/abc"), 3, ABC, &to, Duration::from_secs(60), &mut Vec::new())))
                .collect();
            for r in runs {
                r.join().unwrap().unwrap();
            }
        });
        assert_eq!(std::fs::read(&to).unwrap(), b"abc");
        assert_eq!(names_in(&dir.join("packages")), [format!("{ABC}.pkg")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_shipment_that_declares_another_packages_sha256_cannot_poison_the_cache() {
        let base = serve();
        let cache = temp("poison");
        let hostile = Package { id: "evil", format: PackageFormat::Ps3DlcPkg, url: &format!("{base}/abd"), sha256: ABC, size: 3 };
        assert!(package_file(&hostile, &cache, &Overrides::new(), &mut Vec::new()).unwrap_err().contains("sha256"));
        let honest = Package { id: "biua", format: PackageFormat::Ps3DlcPkg, url: &format!("{base}/abc"), sha256: ABC, size: 3 };
        let file = package_file(&honest, &cache, &Overrides::new(), &mut Vec::new()).unwrap();
        assert_eq!(std::fs::read(file).unwrap(), b"abc");
        std::fs::remove_dir_all(&cache).unwrap();
    }

    /// A Shipment whose `fetch_package` rows are `packages` (`(id, sha256)`, 3 bytes each), each named by a
    /// `level_asset` row, and whose `src/lua/a.lua` is `lua`.
    fn shipment(dir: &Path, packages: &[(&str, &str)], lua: &str) -> LoadedShipment {
        let mut rows = String::new();
        for (id, sha) in packages {
            rows += &format!(
                "  - kind: fetch_package\n    id: {id}\n    format: ps3_dlc_pkg\n    url: http://127.0.0.1:9/{id}.pkg\n    sha256: \"{sha}\"\n    size: 3\n  - kind: level_asset\n    level: vz\n    type: model\n    name: pmc_hum_obama\n    from: {id}\n"
            );
        }
        std::fs::write(
            dir.join("manifest.yaml"),
            format!(
                "format: 2\nshipment:\n  name: pkgtest\n  title: T\n  version: 0.1.0\n  target: retail\n  authors: [x]\n  description: test\n  quartermaster: \">=7.1.0, <8\"\ncontributions:\n{rows}"
            ),
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("src").join("lua")).unwrap();
        std::fs::write(dir.join("src").join("lua").join("a.lua"), lua).unwrap();
        crate::discover::open(dir).unwrap_or_else(|e| panic!("{e:?}"))
    }

    const MODEL: &str = "src/level/model/0x786ADB07.0.ucfx";

    /// Write one model rung under `out`, as a conversion does.
    fn model_rung(out: &Path) -> Converted {
        let file = out.join(MODEL);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, b"model").unwrap();
        let key = ("model".to_string(), mercs2_formats::hash::pandemic_hash_m2("pmc_hum_obama"));
        Converted { assets: BTreeMap::from([(key, vec![("pmc_hum_obama_P000_Q3".to_string(), MODEL.to_string())])]), streams: Vec::new() }
    }

    /// A converter that writes one model rung and counts its calls.
    fn fake(calls: &std::cell::Cell<usize>) -> impl FnMut(PackageFormat, &Path, &Path, &mut Vec<String>) -> Result<Converted, String> + '_ {
        move |_, _, out, _| {
            calls.set(calls.get() + 1);
            Ok(model_rung(out))
        }
    }

    #[test]
    fn builds_at_once_convert_once_and_each_keeps_its_own_staging_copy() {
        let dir = temp("parallel");
        let cache = temp("parallel-cache");
        let pkg = dir.join("p.pkg");
        std::fs::write(&pkg, b"abc").unwrap();
        let overrides = Overrides::from([("pkg".to_string(), pkg)]);
        let s = shipment(&dir, &[("pkg", ABC)], "one");
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let staged: Vec<Staged> = std::thread::scope(|scope| {
            let runs: Vec<_> = (0..4)
                .map(|_| {
                    scope.spawn(|| {
                        let mut convert = |_: PackageFormat, _: &Path, out: &Path, _: &mut Vec<String>| -> Result<Converted, String> {
                            calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            std::thread::sleep(Duration::from_millis(300));
                            Ok(model_rung(out))
                        };
                        materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut convert).unwrap()
                    })
                })
                .collect();
            runs.into_iter().map(|r| r.join().unwrap()).collect()
        });
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let roots: BTreeSet<&Path> = staged.iter().map(|s| s.shipment.root.as_path()).collect();
        assert_eq!(roots.len(), 4);
        for s in &staged {
            assert_eq!(std::fs::read(s.shipment.root.join(MODEL)).unwrap(), b"model");
            assert_eq!(std::fs::read_to_string(s.shipment.root.join("src/lua/a.lua")).unwrap(), "one");
        }
        let key = converted_key(ABC, "v");
        assert_eq!(names_in(&cache.join("converted")), [key.clone(), format!("{key}.lock")]);
        drop(staged);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn a_conversion_is_cached_by_package_and_vz_and_each_build_gets_a_fresh_src() {
        let dir = temp("stage");
        let cache = temp("stage-cache");
        let pkg = dir.join("p.pkg");
        std::fs::write(&pkg, b"abc").unwrap();
        let overrides = Overrides::from([("pkg".to_string(), pkg)]);
        let calls = std::cell::Cell::new(0);
        let mut convert = fake(&calls);
        let mut stage = |lua: &str| {
            let s = shipment(&dir, &[("pkg", ABC)], lua);
            materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut convert).unwrap()
        };
        let read = |s: &Staged, rel: &str| std::fs::read_to_string(s.shipment.root.join(rel)).unwrap();

        let first = stage("one");
        assert_eq!(read(&first, "src/lua/a.lua"), "one");
        assert_eq!(read(&first, MODEL), "model");
        assert!(
            matches!(&first.shipment.manifest.contributions[..], [Contribution::LevelAsset { rungs, from: None, .. }] if rungs[0].file == Path::new(MODEL))
        );
        // An edited script and an edited manifest: the next build reads the new script, and converts nothing.
        let second = stage("two");
        assert_eq!(read(&second, "src/lua/a.lua"), "two");
        assert_eq!(read(&second, MODEL), "model");
        assert_eq!(calls.get(), 1);
        assert_ne!(first.shipment.root, second.shipment.root);
        // A staging copy another build holds is kept; one nobody holds is deleted by the next build.
        assert!(first.shipment.root.exists());
        let (first_root, second_root) = (first.shipment.root.clone(), second.shipment.root.clone());
        drop(first);
        let third = stage("three");
        assert!(!first_root.exists());
        assert!(second_root.exists());
        drop(second);
        drop(third);
        let fourth = stage("four");
        assert_eq!(names_in(fourth.shipment.root.parent().unwrap()).iter().filter(|n| !n.ends_with(".lock")).count(), 1);
        assert_eq!(read(&fourth, "src/lua/a.lua"), "four");
        assert_eq!(calls.get(), 1);
        let converted = cache.join("converted");
        let key = converted_key(ABC, "v");
        assert_eq!(names_in(&converted), [key.clone(), format!("{key}.lock")]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let ino = |p: &Path| std::fs::metadata(p).unwrap().ino();
            assert_eq!(ino(&fourth.shipment.root.join(MODEL)), ino(&converted.join(&key).join(MODEL)), "the staging copy hard-links the converted file");
        }
        // Another vz.wad is another conversion.
        drop(fourth);
        let s = shipment(&dir, &[("pkg", ABC)], "five");
        materialize_with(&s, "w", &cache, &overrides, &mut Vec::new(), &mut fake(&calls)).unwrap();
        assert_eq!(calls.get(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn a_failed_conversion_leaves_no_folder_and_an_incomplete_one_is_an_error() {
        let dir = temp("fail");
        let cache = temp("fail-cache");
        let pkg = dir.join("p.pkg");
        std::fs::write(&pkg, b"abc").unwrap();
        let overrides = Overrides::from([("pkg".to_string(), pkg)]);
        let s = shipment(&dir, &[("pkg", ABC)], "one");
        let mut failing = |_: PackageFormat, _: &Path, out: &Path, _: &mut Vec<String>| -> Result<Converted, String> {
            std::fs::write(out.join("half"), b"x").unwrap();
            Err("the converter failed".to_string())
        };
        let e = materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut failing).err().unwrap();
        assert!(e.contains("the converter failed"), "{e}");
        let key = converted_key(ABC, "v");
        assert_eq!(names_in(&cache.join("converted")), [format!("{key}.lock")]);
        std::fs::create_dir_all(cache.join("converted").join(&key)).unwrap();
        let calls = std::cell::Cell::new(0);
        let e = materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut fake(&calls)).err().unwrap();
        assert!(e.contains("incomplete") && e.contains(&key), "{e}");
        assert_eq!(calls.get(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&cache).unwrap();
    }

    #[test]
    fn a_malformed_row_is_refused_before_the_cache_is_touched() {
        let dir = temp("rows");
        let cache = std::env::temp_dir().join(format!("qm-package-rows-cache-{}", unique()));
        let calls = std::cell::Cell::new(0);
        for sha in ["../../escape_target/relative_escape", "/tmp/absolute_escape", "D62081DE"] {
            let s = shipment(&dir, &[("pkg", sha)], "one");
            let e = materialize_with(&s, "v", &cache, &Overrides::new(), &mut Vec::new(), &mut fake(&calls)).err().unwrap();
            assert!(e.contains("64 lowercase hex"), "{e}");
        }
        let s = shipment(&dir, &[("pkg", ABC), ("pkg", ABC)], "one");
        assert!(materialize_with(&s, "v", &cache, &Overrides::new(), &mut Vec::new(), &mut fake(&calls)).err().unwrap().contains("more than one"));
        assert!(!cache.exists());
        assert_eq!(calls.get(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_packages_converting_one_stream_are_refused() {
        let dir = temp("streams");
        let cache = temp("streams-cache");
        let (a, b) = (dir.join("a.pkg"), dir.join("b.pkg"));
        std::fs::write(&a, b"abc").unwrap();
        std::fs::write(&b, b"abd").unwrap();
        let overrides = Overrides::from([("a".to_string(), a), ("b".to_string(), b)]);
        let s = shipment(&dir, &[("a", ABC), ("b", ABD)], "one");
        let n = std::cell::Cell::new(0u32);
        let mut convert = |_: PackageFormat, _: &Path, out: &Path, _: &mut Vec<String>| -> Result<Converted, String> {
            n.set(n.get() + 1);
            let stream = "src/level/Audios/x.pws";
            std::fs::create_dir_all(out.join("src/level/Audios")).unwrap();
            std::fs::write(out.join(stream), b"pws").unwrap();
            Ok(Converted { assets: BTreeMap::from([(("model".to_string(), n.get()), Vec::new())]), streams: vec![stream.to_string()] })
        };
        let e = materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut convert).err().unwrap();
        assert!(e.contains("both convert the stream src/level/Audios/x.pws"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&cache).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_src_is_refused_naming_it() {
        let dir = temp("symlink");
        let cache = temp("symlink-cache");
        let outside = temp("symlink-outside");
        std::fs::write(outside.join("secret"), b"secret").unwrap();
        let pkg = outside.join("p.pkg");
        std::fs::write(&pkg, b"abc").unwrap();
        let overrides = Overrides::from([("pkg".to_string(), pkg)]);
        let s = shipment(&dir, &[("pkg", ABC)], "one");
        let calls = std::cell::Cell::new(0);
        for (name, target) in [("leak.txt", outside.join("secret")), ("dirlink", outside.clone())] {
            let link = dir.join("src").join(name);
            std::os::unix::fs::symlink(&target, &link).unwrap();
            let e = materialize_with(&s, "v", &cache, &overrides, &mut Vec::new(), &mut fake(&calls)).err().unwrap();
            assert!(e.contains("a symlink") && e.contains(&link.display().to_string()), "{e}");
            std::fs::remove_file(&link).unwrap();
        }
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&cache).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn a_package_id_must_name_a_fetch_package_of_the_set() {
        let (with, without) = (temp("set-with"), temp("set-without"));
        let packaged = shipment(&with, &[("pkg", ABC)], "one");
        std::fs::write(
            without.join("manifest.yaml"),
            "format: 2\nshipment:\n  name: plain\n  title: T\n  version: 0.1.0\n  target: retail\n  authors: [x]\n  description: test\n  quartermaster: \">=7.1.0, <8\"\ncontributions: []\n",
        )
        .unwrap();
        let plain = crate::discover::open(&without).unwrap_or_else(|e| panic!("{e:?}"));
        let given = |id: &str| Overrides::from([(id.to_string(), PathBuf::from("/x.pkg"))]);
        check_overrides(&given("pkg"), &[&packaged, &plain]).unwrap();
        assert!(check_overrides(&given("other"), &[&packaged, &plain]).unwrap_err().contains("--package other"));
        assert!(check_overrides(&given("pkg"), &[&plain]).is_err());
        assert!(has_package(&packaged) && !has_package(&plain));
        std::fs::remove_dir_all(&with).unwrap();
        std::fs::remove_dir_all(&without).unwrap();
    }
}
