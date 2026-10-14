//! The Lua corpus catalogue: `data/corpora.tsv` and `data/wads.tsv`.
//!
//! A **corpus** is the script content of one product on one platform in one build: a base game, a
//! demo, a patch, or a DLC (`product`), its `platform` and `build`, and, for a patch or DLC, the
//! corpus it `applies_to`. Products stay apart: scripts are only ever compared within a product.
//!
//! A **WAD** row names one WAD file of a corpus by sha256 and size, with the chain that rebuilds it
//! from the original file it ships in. `from_sha256` and `step` are `;`-separated lists of equal
//! length; step *i* turns the file with `from_sha256[i]` into the next file of the chain, and the
//! last step yields the WAD. [`derive`] replays a chain on files in a work directory and checks
//! every sha256 on the way. A WAD with an empty chain is catalogued from an install, where
//! [`check_installed`] finds it by name; its `install` column ([`Install`]) is either the
//! local-config key of that install directory or `canonical:<dir>`, a directory under the local
//! config's `canonical_dir`. A WAD with a chain names no install.
//!
//! Both tables are tab-separated with one header line that must match exactly. No field is empty:
//! a column with no value holds `-` ([`crate::NO_VALUE`]), and an empty field is an error naming
//! the table, line and column. The columns that may hold `-` are `applies_to` in `corpora.tsv`,
//! and `from_sha256`, `step` (together: a WAD with no chain) and `install` in `wads.tsv`; in every
//! other column `-` is an error.
//!
//! | step | input → output |
//! |---|---|
//! | `zip:<entry>` | a zip archive → its entry |
//! | `xdvdfs:<path>` | an Xbox disc image → the file at `<path>` ([`crate::xdvdfs`]) |
//! | `stfs:<name>` | an Xbox 360 STFS package (LIVE/PIRS/CON) → the file `<name>` in it |
//! | `iso9660:<path>` | an ISO-9660 disc image → the file at `<path>` ([`crate::iso9660`]) |
//! | `pkg:<path>` | a PSN `.pkg` → its entry at `<path>` (`mercs2_formats::ps3_pkg`) |
//! | `edat:<klicensee>` | an NPDRM EDAT → its payload, decrypted with the 32-hex-digit title klicensee, which must validate against the EDAT header first (`mercs2_formats::ps3_edat`, `ps3_klic`) |

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::{field_value, table_row, Error};

pub const CORPORA_HEADER: &str = "corpus_id\tproduct\tplatform\tbuild\tapplies_to\tdescription";
pub const WADS_HEADER: &str = "corpus_id\twad\tsha256\tsize\tfrom_sha256\tstep\tinstall";

/// One row of `corpora.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpus {
    pub id: String,
    /// `base`, `demo`, `patch`, or `dlc:<name>`.
    pub product: String,
    /// `pc`, `x360` or `ps3`.
    pub platform: String,
    pub build: String,
    /// The corpus a patch or DLC applies to.
    pub applies_to: Option<String>,
    pub description: String,
}

/// One derivation step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Zip(String),
    Xdvdfs(String),
    Stfs(String),
    Iso9660(String),
    Pkg(String),
    Edat([u8; 16]),
}

impl Step {
    fn parse(s: &str) -> Option<Step> {
        let (head, arg) = s.split_once(':')?;
        if arg.is_empty() {
            return None;
        }
        match head {
            "zip" => Some(Step::Zip(arg.to_string())),
            "xdvdfs" => Some(Step::Xdvdfs(arg.to_string())),
            "stfs" => Some(Step::Stfs(arg.to_string())),
            "iso9660" => Some(Step::Iso9660(arg.to_string())),
            "pkg" => Some(Step::Pkg(arg.to_string())),
            "edat" => klicensee(arg).map(Step::Edat),
            _ => None,
        }
    }
}

/// A klicensee written as exactly 32 hex digits.
fn klicensee(s: &str) -> Option<[u8; 16]> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut k = [0u8; 16];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(k)
}

/// One link of a chain: the input file's sha256 and the step applied to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub from_sha256: String,
    pub step: Step,
}

/// One row of `wads.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wad {
    pub corpus: String,
    /// The WAD's path relative to the game's install or disc root (`data/vz.wad` on PC, `vz.wad` on
    /// Xbox 360, `PS3_GAME/USRDIR/VZ.WAD` on PS3), or its path inside its package for a patch or DLC.
    pub name: String,
    pub sha256: String,
    pub size: u64,
    pub chain: Vec<Link>,
    /// For a WAD with no chain, the install directory it is found in.
    pub install: Option<Install>,
}

/// The install directory a WAD with no chain is found in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// The directory a local-config key names (`x360_retail_dir`, `ps3_jp_disc_dir`, …).
    Key(String),
    /// A directory relative to the local config's `canonical_dir`, written `canonical:<dir>`.
    Canonical(String),
}

impl Install {
    fn parse(s: &str) -> Option<Install> {
        match s.split_once(':') {
            None => Some(Install::Key(s.to_string())),
            Some(("canonical", dir)) if !dir.is_empty() => Some(Install::Canonical(dir.to_string())),
            Some(_) => None,
        }
    }
}

/// Both tables, parsed and cross-checked.
#[derive(Clone, Debug)]
pub struct Corpora {
    pub corpora: Vec<Corpus>,
    pub wads: Vec<Wad>,
}

fn err(table: &str, n: usize, msg: impl std::fmt::Display) -> Error {
    Error::new(format!("{table} line {n}: {msg}"))
}

fn lines<'t>(table: &str, text: &'t str, header: &str) -> Result<Vec<(usize, Vec<&'t str>)>, Error> {
    let mut it = text.lines();
    let first = it.next().ok_or_else(|| Error::new(format!("{table} is empty")))?;
    if first != header {
        return Err(Error::new(format!("{table}: header line is {first:?}, expected {header:?}")));
    }
    it.enumerate().map(|(i, l)| Ok((i + 2, table_row(table, header, i + 2, l)?))).collect()
}

/// The `;`-separated entries of a list column; [`crate::NO_VALUE`] is the empty list.
fn list(s: &str) -> Vec<&str> {
    field_value(s).map(|v| v.split(';').collect()).unwrap_or_default()
}

fn sha(table: &str, n: usize, col: &str, s: &str) -> Result<String, Error> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(err(table, n, format!("{col} {s:?} is not 64 lowercase hex digits")));
    }
    Ok(s.to_string())
}

impl Corpora {
    /// The tables compiled into this crate.
    pub fn embedded() -> Result<Corpora, Error> {
        Corpora::parse(include_str!("../data/corpora.tsv"), include_str!("../data/wads.tsv"))
    }

    pub fn parse(corpora: &str, wads: &str) -> Result<Corpora, Error> {
        const C: &str = "corpora.tsv";
        const W: &str = "wads.tsv";
        let mut cs: Vec<Corpus> = Vec::new();
        for (n, f) in lines(C, corpora, CORPORA_HEADER)? {
            if [f[0], f[3], f[5]].iter().any(|v| field_value(v).is_none()) {
                return Err(err(C, n, "corpus_id, build and description must hold a value"));
            }
            if let Some(prev) = cs.last() {
                if prev.id.as_str() >= f[0] {
                    return Err(err(C, n, format!("corpus_id {} is not after {}", f[0], prev.id)));
                }
            }
            let product_ok = matches!(f[1], "base" | "demo" | "patch")
                || f[1].strip_prefix("dlc:").is_some_and(|d| !d.is_empty());
            if !product_ok {
                return Err(err(C, n, format!("product {:?} is not base, demo, patch or dlc:<name>", f[1])));
            }
            if !matches!(f[2], "pc" | "x360" | "ps3") {
                return Err(err(C, n, format!("platform {:?} is not pc, x360 or ps3", f[2])));
            }
            let applies_to = field_value(f[4]).map(str::to_string);
            let needs_base = f[1] == "patch" || f[1].starts_with("dlc:");
            if needs_base != applies_to.is_some() {
                return Err(err(C, n, "a patch or DLC names the corpus it applies to, and nothing else does"));
            }
            cs.push(Corpus {
                id: f[0].to_string(),
                product: f[1].to_string(),
                platform: f[2].to_string(),
                build: f[3].to_string(),
                applies_to,
                description: f[5].to_string(),
            });
        }
        let mut ws: Vec<Wad> = Vec::new();
        for (n, f) in lines(W, wads, WADS_HEADER)? {
            if !cs.iter().any(|c| c.id == f[0]) {
                return Err(err(W, n, format!("corpus {} is not in corpora.tsv", f[0])));
            }
            if field_value(f[1]).is_none() {
                return Err(err(W, n, "wad holds no value"));
            }
            if let Some(prev) = ws.last() {
                if (prev.corpus.as_str(), prev.name.as_str()) >= (f[0], f[1]) {
                    return Err(err(W, n, format!("{} {} is not after {} {}", f[0], f[1], prev.corpus, prev.name)));
                }
            }
            let froms = list(f[4]);
            let steps = list(f[5]);
            if froms.len() != steps.len() {
                return Err(err(W, n, format!("{} from_sha256 entries but {} steps", froms.len(), steps.len())));
            }
            let mut chain = Vec::new();
            for (from, step) in froms.into_iter().zip(steps) {
                chain.push(Link {
                    from_sha256: sha(W, n, "from_sha256", from)?,
                    step: Step::parse(step).ok_or_else(|| err(W, n, format!("step {step:?} is not a known step")))?,
                });
            }
            let install = field_value(f[6])
                .map(|v| Install::parse(v).ok_or_else(|| err(W, n, format!("install {v:?} is neither a key nor canonical:<dir>"))))
                .transpose()?;
            if chain.is_empty() != install.is_some() {
                return Err(err(W, n, "a WAD has either a chain or an install key, not both and not neither"));
            }
            ws.push(Wad {
                install,
                corpus: f[0].to_string(),
                name: f[1].to_string(),
                sha256: sha(W, n, "sha256", f[2])?,
                size: f[3].parse().map_err(|e| err(W, n, format!("size {:?}: {e}", f[3])))?,
                chain,
            });
        }
        for c in &cs {
            if let Some(a) = &c.applies_to {
                let base = cs.iter().find(|x| &x.id == a).ok_or_else(|| {
                    Error::new(format!("corpus {} applies to {a}, which corpora.tsv does not list", c.id))
                })?;
                if base.applies_to.is_some() || base.platform != c.platform {
                    return Err(Error::new(format!("corpus {} applies to {a}, which is not a base corpus of the same platform", c.id)));
                }
            }
            if !ws.iter().any(|w| w.corpus == c.id) {
                return Err(Error::new(format!("corpus {} has no WADs in wads.tsv", c.id)));
            }
        }
        // The same file may ship in several corpora (the official install and a repack share WADs),
        // but never twice within one.
        let pairs: BTreeSet<(&str, &str)> = ws.iter().map(|w| (w.corpus.as_str(), w.sha256.as_str())).collect();
        if pairs.len() != ws.len() {
            return Err(Error::new("wads.tsv lists one sha256 twice within a corpus"));
        }
        Ok(Corpora { corpora: cs, wads: ws })
    }

    pub fn corpus(&self, id: &str) -> Option<&Corpus> {
        self.corpora.iter().find(|c| c.id == id)
    }

    pub fn wads_of<'a>(&'a self, corpus: &'a str) -> impl Iterator<Item = &'a Wad> + 'a {
        self.wads.iter().filter(move |w| w.corpus == corpus)
    }
}

/// Where derivations run: a work directory for the files each step writes, reused across
/// derivations.
pub struct Env {
    pub work: PathBuf,
}

/// sha256 of a file, streamed.
pub fn file_sha256(path: &Path) -> Result<String, Error> {
    let mut f = File::open(path).map_err(|e| Error::new(format!("open {}: {e}", path.display())))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 8 << 20];
    loop {
        let k = f.read(&mut buf).map_err(|e| Error::new(format!("read {}: {e}", path.display())))?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Apply one step to the file at `input` (whose sha256 is `input_sha`), returning the output path.
fn apply(step: &Step, input: &Path, input_sha: &str, env: &Env) -> Result<PathBuf, Error> {
    let dir = env.work.join(input_sha);
    match step {
        Step::Zip(entry) => {
            let out = dir.join("zip").join(entry);
            if !out.is_file() {
                let f = File::open(input).map_err(|e| Error::new(format!("open {}: {e}", input.display())))?;
                let mut z = zip::ZipArchive::new(f).map_err(|e| Error::new(format!("zip {}: {e}", input.display())))?;
                let mut e = z.by_name(entry).map_err(|e| Error::new(format!("zip {}: {entry}: {e}", input.display())))?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                let mut o = File::create(&out).map_err(|e| Error::new(format!("create {}: {e}", out.display())))?;
                std::io::copy(&mut e, &mut o).map_err(|e| Error::new(format!("unzip {entry}: {e}")))?;
            }
            Ok(out)
        }
        Step::Xdvdfs(member) => {
            let out = dir.join("xdvdfs").join(member);
            if !out.is_file() {
                use std::io::{Seek, SeekFrom};
                let mut f = File::open(input).map_err(|e| Error::new(format!("open {}: {e}", input.display())))?;
                let (off, size) = crate::xdvdfs::locate(&mut f, member)?;
                f.seek(SeekFrom::Start(off)).map_err(|e| Error::new(format!("seek {}: {e}", input.display())))?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                let mut o = File::create(&out).map_err(|e| Error::new(format!("create {}: {e}", out.display())))?;
                let copied = std::io::copy(&mut f.take(size), &mut o).map_err(|e| Error::new(format!("copy {member}: {e}")))?;
                if copied != size {
                    return Err(Error::new(format!("xdvdfs: {member}: {copied} of {size} bytes before the image ended")));
                }
            }
            Ok(out)
        }
        Step::Stfs(name) => {
            let out = dir.join("stfs").join(name);
            if !out.is_file() {
                let data = std::fs::read(input).map_err(|e| Error::new(format!("read {}: {e}", input.display())))?;
                let magic = data.get(0..4).unwrap_or_default();
                if magic != b"LIVE" && magic != b"PIRS" && magic != b"CON " {
                    return Err(Error::new(format!("stfs: {} is not an STFS package (magic {magic:?})", input.display())));
                }
                let r = mercs2_formats::dlc_stfs::StfsReader::new(data);
                let e = r.entry(name).ok_or_else(|| Error::new(format!("stfs: {} holds no {name}", input.display())))?;
                let bytes = r.read_entry(e).map_err(|e| Error::new(format!("stfs: {name}: {e}")))?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                std::fs::write(&out, bytes).map_err(|e| Error::new(format!("write {}: {e}", out.display())))?;
            }
            Ok(out)
        }
        Step::Iso9660(member) => {
            let out = dir.join("iso9660").join(member);
            if !out.is_file() {
                use std::io::{Seek, SeekFrom};
                let mut f = File::open(input).map_err(|e| Error::new(format!("open {}: {e}", input.display())))?;
                let extents = crate::iso9660::locate(&mut f, member)?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                let mut o = File::create(&out).map_err(|e| Error::new(format!("create {}: {e}", out.display())))?;
                for (off, size) in extents {
                    f.seek(SeekFrom::Start(off)).map_err(|e| Error::new(format!("seek {}: {e}", input.display())))?;
                    let copied = std::io::copy(&mut (&mut f).take(size), &mut o).map_err(|e| Error::new(format!("copy {member}: {e}")))?;
                    if copied != size {
                        return Err(Error::new(format!("iso9660: {member}: {copied} of {size} bytes before the image ended")));
                    }
                }
            }
            Ok(out)
        }
        Step::Pkg(entry) => {
            let out = dir.join("pkg").join(entry);
            if !out.is_file() {
                let data = std::fs::read(input).map_err(|e| Error::new(format!("read {}: {e}", input.display())))?;
                let pkg = mercs2_formats::ps3_pkg::parse_pkg(&data).map_err(|e| Error::new(format!("pkg {}: {e}", input.display())))?;
                let bytes = pkg
                    .file_bytes(entry)
                    .ok_or_else(|| Error::new(format!("pkg {} ({}) has no entry {entry}", input.display(), pkg.content_id)))?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                std::fs::write(&out, bytes).map_err(|e| Error::new(format!("write {}: {e}", out.display())))?;
            }
            Ok(out)
        }
        Step::Edat(klic) => {
            let out = dir.join("edat").join("payload");
            if !out.is_file() {
                let data = std::fs::read(input).map_err(|e| Error::new(format!("read {}: {e}", input.display())))?;
                if !mercs2_formats::ps3_klic::validate_klicensee(&data, klic) {
                    return Err(Error::new(format!("edat: the klicensee does not validate against the header of {}", input.display())));
                }
                let bytes = mercs2_formats::ps3_edat::decrypt_edat(&data, klic)
                    .map_err(|e| Error::new(format!("edat {}: {e}", input.display())))?;
                std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| Error::new(format!("create dir: {e}")))?;
                std::fs::write(&out, bytes).map_err(|e| Error::new(format!("write {}: {e}", out.display())))?;
            }
            Ok(out)
        }
    }
}

/// Rebuild `wad` from `container`, the first file of its chain, checking every sha256; returns the
/// path of the rebuilt WAD inside `env.work`.
pub fn derive(wad: &Wad, container: &Path, env: &Env) -> Result<PathBuf, Error> {
    let first = wad.chain.first().ok_or_else(|| {
        Error::new(format!("{} {}: no chain; the WAD is catalogued from its install (check_installed)", wad.corpus, wad.name))
    })?;
    let got = file_sha256(container)?;
    if got != first.from_sha256 {
        return Err(Error::new(format!(
            "{} {}: the container {} has sha256 {got}, the chain starts from {}",
            wad.corpus,
            wad.name,
            container.display(),
            first.from_sha256
        )));
    }
    let mut path = apply(&first.step, container, &got, env)?;
    for link in &wad.chain[1..] {
        let s = file_sha256(&path)?;
        if s != link.from_sha256 {
            return Err(Error::new(format!(
                "{} {}: {} has sha256 {s}, the next link expects {}",
                wad.corpus,
                wad.name,
                path.display(),
                link.from_sha256
            )));
        }
        path = apply(&link.step, &path, &s, env)?;
    }
    let s = file_sha256(&path)?;
    let size = std::fs::metadata(&path).map_err(|e| Error::new(format!("stat {}: {e}", path.display())))?.len();
    if s != wad.sha256 || size != wad.size {
        return Err(Error::new(format!(
            "{} {}: the chain yields sha256 {s} ({size} bytes), wads.tsv says {} ({} bytes)",
            wad.corpus, wad.name, wad.sha256, wad.size
        )));
    }
    Ok(path)
}

/// Check that the WAD at `<install>/<name>` has the catalogued sha256 and size.
pub fn check_installed(wad: &Wad, install: &Path) -> Result<PathBuf, Error> {
    let path = install.join(&wad.name);
    let size = std::fs::metadata(&path).map_err(|e| Error::new(format!("{} {}: {}: {e}", wad.corpus, wad.name, path.display())))?.len();
    let s = file_sha256(&path)?;
    if s != wad.sha256 || size != wad.size {
        return Err(Error::new(format!(
            "{} {}: {} has sha256 {s} ({size} bytes), wads.tsv says {} ({} bytes)",
            wad.corpus,
            wad.name,
            path.display(),
            wad.sha256,
            wad.size
        )));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn corpora(rows: &[&str]) -> String {
        format!("{CORPORA_HEADER}\n{}\n", rows.join("\n"))
    }
    fn wads(rows: &[&str]) -> String {
        format!("{WADS_HEADER}\n{}\n", rows.join("\n"))
    }

    #[test]
    fn the_embedded_tables_parse_and_cross_check() {
        let c = Corpora::embedded().unwrap();
        assert!(c.corpus("base-pc-demo").is_some());
        assert_eq!(c.wads_of("base-pc-demo").count(), 4);
        assert_eq!(c.wads_of("base-pc-retail").count(), 8);
        assert_eq!(c.wads_of("base-pc-elamigos").count(), 8);
        assert!(c.wads_of("base-pc-retail").chain(c.wads_of("base-pc-elamigos")).all(|w| w.chain.is_empty()));
    }

    #[test]
    fn steps_parse_by_exact_name() {
        assert_eq!(Step::parse("zip:a.exe"), Some(Step::Zip("a.exe".into())));
        assert_eq!(Step::parse("wine-sfx:data/vz.wad"), None);
        assert_eq!(Step::parse("xdvdfs:vz.wad"), Some(Step::Xdvdfs("vz.wad".into())));
        assert_eq!(Step::parse("stfs:DLC01.doh"), Some(Step::Stfs("DLC01.doh".into())));
        assert_eq!(Step::parse("iso9660:A/VZ.WAD"), Some(Step::Iso9660("A/VZ.WAD".into())));
        assert_eq!(Step::parse("pkg:USRDIR/VZ-PATCH.WAD"), Some(Step::Pkg("USRDIR/VZ-PATCH.WAD".into())));
        assert_eq!(
            Step::parse("edat:000102030405060708090a0b0c0d0e0f"),
            Some(Step::Edat([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]))
        );
        assert_eq!(Step::parse("edat:0001"), None);
        assert_eq!(Step::parse("edat:zz0102030405060708090a0b0c0d0e0f"), None);
        assert_eq!(Step::parse("zip:"), None);
        assert_eq!(Step::parse("unrar:x"), None);
    }

    #[test]
    fn rows_that_break_the_rules_are_rejected() {
        let base = "b\tbase\tpc\tretail\t-\tthe game";
        let wad = |c: &str| format!("{c}\tvz.wad\t{SHA_A}\t1\t{SHA_B}\tzip:x\t-");
        // A DLC must name the base it applies to.
        let e = Corpora::parse(&corpora(&[base, "d\tdlc:x\tpc\tretail\t-\tdlc"]),&wads(&[&wad("b"), &wad("d").replace(SHA_A, SHA_B)]))
            .unwrap_err()
            .to_string();
        assert!(e.contains("applies to"), "{e}");
        // A WAD of an unknown corpus.
        let e = Corpora::parse(&corpora(&[base]), &wads(&[&wad("z")])).unwrap_err().to_string();
        assert!(e.contains("not in corpora.tsv"), "{e}");
        // A corpus with no WADs.
        let e = Corpora::parse(&corpora(&[base, "c\tdemo\tpc\tdemo\t-\tdemo"]), &wads(&[&wad("b")])).unwrap_err().to_string();
        assert!(e.contains("has no WADs"), "{e}");
        // A chained WAD naming an install, and an installed WAD naming none.
        let e = Corpora::parse(&corpora(&[base]), &wads(&[&wad("b").replace("\tzip:x\t-", "\tzip:x\tk")])).unwrap_err().to_string();
        assert!(e.contains("either a chain or an install key"), "{e}");
        let e = Corpora::parse(&corpora(&[base]), &wads(&[&format!("b\tvz.wad\t{SHA_A}\t1\t-\t-\t-")])).unwrap_err().to_string();
        assert!(e.contains("either a chain or an install key"), "{e}");
        // One file twice within a corpus.
        let e = Corpora::parse(&corpora(&[base]), &wads(&[&wad("b"), &wad("b").replace("vz.wad", "zz.wad")])).unwrap_err().to_string();
        assert!(e.contains("twice within a corpus"), "{e}");
        // An unknown product.
        let e = Corpora::parse(&corpora(&["b\tmod\tpc\tretail\t-\tx"]), &wads(&[&wad("b")])).unwrap_err().to_string();
        assert!(e.contains("product"), "{e}");
    }

    #[test]
    fn a_no_value_field_reads_as_none_and_an_empty_field_is_rejected() {
        let base = "b\tbase\tpc\tretail\t-\tthe game";
        let chained = format!("b\tvz.wad\t{SHA_A}\t1\t{SHA_B}\tzip:x\t-");
        let installed = format!("b\tzz.wad\t{SHA_B}\t1\t-\t-\tk");
        let canonical = format!("b\tzy.wad\t{}\t1\t-\t-\tcanonical:a/b", "c".repeat(64));
        let c = Corpora::parse(&corpora(&[base]), &wads(&[&chained, &canonical, &installed])).unwrap();
        assert_eq!(c.corpora[0].applies_to, None);
        assert_eq!(c.wads[0].install, None);
        assert_eq!(c.wads[1].install, Some(Install::Canonical("a/b".into())));
        assert_eq!(c.wads[2].install, Some(Install::Key("k".into())));
        assert!(c.wads[2].chain.is_empty());
        for bad in ["canonical:", "other:a"] {
            let e = Corpora::parse(&corpora(&[base]), &wads(&[&installed.replace("\tk", &format!("\t{bad}"))])).unwrap_err().to_string();
            assert!(e.contains("is neither a key nor canonical:<dir>"), "{bad}: {e}");
        }
        let e = Corpora::parse(&corpora(&["b\tbase\tpc\tretail\t\tthe game"]), &wads(&[&chained])).unwrap_err().to_string();
        assert!(e.contains("corpora.tsv line 2: column 5 (applies_to) is empty"), "{e}");
        let e = Corpora::parse(&corpora(&[base]), &wads(&[&chained.replace("\tzip:x\t-", "\tzip:x\t")])).unwrap_err().to_string();
        assert!(e.contains("wads.tsv line 2: column 7 (install) is empty"), "{e}");
        let e = Corpora::parse(&corpora(&["b\tbase\tpc\t-\t-\tthe game"]), &wads(&[&chained])).unwrap_err().to_string();
        assert!(e.contains("must hold a value"), "{e}");
    }

    #[test]
    fn a_zip_chain_derives_and_checks_its_hashes() {
        use std::io::Write;
        let work = std::env::temp_dir().join("mercs2_exemap_corpus_zip_test");
        std::fs::remove_dir_all(&work).ok();
        std::fs::create_dir_all(&work).unwrap();
        let zpath = work.join("in.zip");
        {
            let mut z = zip::ZipWriter::new(File::create(&zpath).unwrap());
            z.start_file("data/x.wad", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(b"FFCS wad bytes").unwrap();
            z.finish().unwrap();
        }
        let zsha = file_sha256(&zpath).unwrap();
        let wsha = format!("{:x}", Sha256::digest(b"FFCS wad bytes"));
        let wad = Wad {
            install: None,
            corpus: "c".into(),
            name: "x.wad".into(),
            sha256: wsha.clone(),
            size: 14,
            chain: vec![Link { from_sha256: zsha, step: Step::Zip("data/x.wad".into()) }],
        };
        let env = Env { work: work.join("work") };
        let out = derive(&wad, &zpath, &env).unwrap();
        assert_eq!(file_sha256(&out).unwrap(), wsha);
        // An installed WAD is checked in place; deriving it is an error.
        let inst = work.join("install");
        std::fs::create_dir_all(inst.join("data")).unwrap();
        std::fs::write(inst.join("data/x.wad"), b"FFCS wad bytes").unwrap();
        let installed = Wad { chain: vec![], install: Some(Install::Key("k".into())), name: "data/x.wad".into(), ..wad.clone() };
        assert_eq!(check_installed(&installed, &inst).unwrap(), inst.join("data/x.wad"));
        assert!(derive(&installed, &zpath, &env).unwrap_err().to_string().contains("no chain"));
        std::fs::write(inst.join("data/x.wad"), b"other").unwrap();
        assert!(check_installed(&installed, &inst).unwrap_err().to_string().contains("wads.tsv says"));
        std::fs::remove_dir_all(&work).ok();
    }
}
