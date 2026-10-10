//! `data/images.tsv`: the executable images the function map spans, across platforms.
//!
//! Each row is one image file, identified by sha256, with the Ghidra language and load base it is
//! analysed with:
//!
//! * a **PC** row names its `builds.tsv` build (`build_id`); the build's own row carries its
//!   provenance, and the two rows must agree on sha256 and size;
//! * a **console** row carries its derivation: `from_sha256` and `step` are `;`-separated lists of
//!   equal length, where step *i* turns the file with `from_sha256[i]` into the next file of the
//!   chain and the last step yields the image itself. [`derive`] replays a chain and checks every
//!   sha256 on the way;
//! * `code_image`, when set, names the image whose code this one shares byte for byte (its
//!   executable sections and its function-start table): a regional build analysed once.
//!   [`shared_code`] checks that claim.
//!
//! The table is tab-separated with one header line that must match exactly, rows sorted by
//! `image_id`. No field is empty: a column with no value holds `-` ([`crate::NO_VALUE`]), and an
//! empty field is an error naming the line and column. The columns that may hold `-` are
//! `build_id` (every console row), `code_image`, and `from_sha256` with `step` (every PC row); in
//! every other column `-` is an error.
//!
//! Steps:
//!
//! | step | input → output |
//! |---|---|
//! | `xdvdfs:<path>` | an Xbox disc image → the file at `<path>` ([`crate::xdvdfs`]) |
//! | `xex:retail` / `xex:devkit` | an XEX2 → its PE image ([`crate::xbox::unpack_xex`]) |
//! | `ghidra-pe` | an Xbox PE image → its RVA-layout PE ([`crate::xbox::ghidra_pe`]) |
//! | `pkg:<path>` | a PSN `.pkg` → the entry at `<path>` (`mercs2_formats::ps3_pkg`) |
//! | `unself:app` / `unself:npdrm` | a PS3 SELF → its ELF (`mercs2_formats::ps3_self`) |

use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};

use mercs2_formats::ps3_self::SelfKind;
use sha2::{Digest, Sha256};

use crate::starts::function_starts;
use crate::xbox::XexKey;
use crate::{field_value, sha256_hex, table_row, Catalogue, Error};

pub const IMAGES_HEADER: &str =
    "image_id\tplatform\tghidra_language\tsha256\tsize\tbase\tbuild_id\tcode_image\tfrom_sha256\tstep";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Platform {
    Pc,
    X360,
    Ps3,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Pc => "pc",
            Platform::X360 => "x360",
            Platform::Ps3 => "ps3",
        }
    }
    fn parse(s: &str) -> Option<Platform> {
        match s {
            "pc" => Some(Platform::Pc),
            "x360" => Some(Platform::X360),
            "ps3" => Some(Platform::Ps3),
            _ => None,
        }
    }
}

/// One derivation step of a console image's chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Xdvdfs(String),
    Xex(XexKey),
    GhidraPe,
    Pkg(String),
    Unself(SelfKind),
}

impl Step {
    fn parse(s: &str) -> Option<Step> {
        let (head, arg) = match s.split_once(':') {
            Some((h, a)) => (h, Some(a)),
            None => (s, None),
        };
        match (head, arg) {
            ("xdvdfs", Some(p)) if !p.is_empty() => Some(Step::Xdvdfs(p.to_string())),
            ("xex", Some(k)) => k.parse().ok().map(Step::Xex),
            ("ghidra-pe", None) => Some(Step::GhidraPe),
            ("pkg", Some(p)) if !p.is_empty() => Some(Step::Pkg(p.to_string())),
            ("unself", Some("app")) => Some(Step::Unself(SelfKind::App)),
            ("unself", Some("npdrm")) => Some(Step::Unself(SelfKind::Npdrm)),
            _ => None,
        }
    }
}

/// One link of a chain: the input file's sha256 and the step applied to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub from_sha256: String,
    pub step: Step,
}

/// One row of `images.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapImage {
    pub id: String,
    pub platform: Platform,
    pub ghidra_language: String,
    pub sha256: String,
    pub size: u64,
    pub base: u32,
    pub build_id: Option<String>,
    pub code_image: Option<String>,
    pub chain: Vec<Link>,
}

/// The image table, parsed and cross-checked against the build catalogue.
#[derive(Clone, Debug)]
pub struct Images {
    pub images: Vec<MapImage>,
}

fn field_err(n: usize, msg: String) -> Error {
    Error::new(format!("images.tsv line {n}: {msg}"))
}

fn lower_hex(n: usize, col: &str, s: &str, len: usize) -> Result<String, Error> {
    if s.len() != len || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err(field_err(n, format!("{col} {s:?} is not {len} lowercase hex digits")));
    }
    Ok(s.to_string())
}

impl Images {
    /// The table compiled into this crate, checked against `catalogue`.
    pub fn embedded(catalogue: &Catalogue) -> Result<Images, Error> {
        Images::parse(include_str!("../data/images.tsv"), catalogue)
    }

    pub fn parse(text: &str, catalogue: &Catalogue) -> Result<Images, Error> {
        let mut lines = text.lines();
        let first = lines.next().ok_or_else(|| Error::new("images.tsv is empty"))?;
        if first != IMAGES_HEADER {
            return Err(Error::new(format!("images.tsv: header line is {first:?}, expected {IMAGES_HEADER:?}")));
        }
        let mut images: Vec<MapImage> = Vec::new();
        for (i, line) in lines.enumerate() {
            let n = i + 2;
            let f = table_row("images.tsv", IMAGES_HEADER, n, line)?;
            let id = field_value(f[0]).ok_or_else(|| field_err(n, "image_id holds no value".into()))?;
            if let Some(prev) = images.last() {
                if prev.id.as_str() >= id {
                    return Err(field_err(n, format!("image_id {id} is not after {}", prev.id)));
                }
            }
            let platform = Platform::parse(f[1]).ok_or_else(|| field_err(n, format!("platform {:?} is not pc, x360 or ps3", f[1])))?;
            if field_value(f[2]).is_none() {
                return Err(field_err(n, "ghidra_language holds no value".into()));
            }
            let sha256 = lower_hex(n, "sha256", f[3], 64)?;
            let size = f[4].parse::<u64>().map_err(|e| field_err(n, format!("size {:?}: {e}", f[4])))?;
            let base = f[5]
                .strip_prefix("0x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .ok_or_else(|| field_err(n, format!("base {:?} is not 0x-prefixed 32-bit hex", f[5])))?;
            let build_id = field_value(f[6]).map(str::to_string);
            let code_image = field_value(f[7]).map(str::to_string);
            let froms: Vec<&str> = field_value(f[8]).map(|v| v.split(';').collect()).unwrap_or_default();
            let steps: Vec<&str> = field_value(f[9]).map(|v| v.split(';').collect()).unwrap_or_default();
            if froms.len() != steps.len() {
                return Err(field_err(n, format!("{} from_sha256 entries but {} steps", froms.len(), steps.len())));
            }
            let mut chain = Vec::new();
            for (from, step) in froms.into_iter().zip(steps) {
                chain.push(Link {
                    from_sha256: lower_hex(n, "from_sha256", from, 64)?,
                    step: Step::parse(step).ok_or_else(|| field_err(n, format!("step {step:?} is not a known step")))?,
                });
            }
            match (platform, &build_id, chain.is_empty()) {
                (Platform::Pc, Some(_), true) => {}
                (Platform::Pc, None, _) => return Err(field_err(n, "a pc image names no build_id".into())),
                (Platform::Pc, Some(_), false) => {
                    return Err(field_err(n, "a pc image's provenance is its builds.tsv row; it carries no chain".into()))
                }
                (_, Some(_), _) => return Err(field_err(n, "only pc images name a build_id".into())),
                (_, None, true) => return Err(field_err(n, "a console image carries no derivation chain".into())),
                (_, None, false) => {}
            }
            images.push(MapImage {
                id: id.to_string(),
                platform,
                ghidra_language: f[2].to_string(),
                sha256,
                size,
                base,
                build_id,
                code_image,
                chain,
            });
        }
        let t = Images { images };
        t.cross_check(catalogue)?;
        Ok(t)
    }

    fn cross_check(&self, catalogue: &Catalogue) -> Result<(), Error> {
        let shas: BTreeSet<&str> = self.images.iter().map(|m| m.sha256.as_str()).collect();
        if shas.len() != self.images.len() {
            return Err(Error::new("images.tsv lists one sha256 under two image ids"));
        }
        for m in &self.images {
            if let Some(b) = &m.build_id {
                let build = catalogue
                    .build(b)
                    .ok_or_else(|| Error::new(format!("image {} names build {b}, which builds.tsv does not list", m.id)))?;
                if build.sha256 != m.sha256 || build.size != m.size {
                    return Err(Error::new(format!(
                        "image {} says sha256 {} size {}; its build {} says {} and {}",
                        m.id, m.sha256, m.size, build.id, build.sha256, build.size
                    )));
                }
            }
            if let Some(c) = &m.code_image {
                let target = self
                    .image(c)
                    .ok_or_else(|| Error::new(format!("image {} shares code with {c}, which images.tsv does not list", m.id)))?;
                if target.code_image.is_some() {
                    return Err(Error::new(format!("image {} shares code with {c}, which itself shares code with another image", m.id)));
                }
                if target.platform != m.platform || target.base != m.base || target.ghidra_language != m.ghidra_language {
                    return Err(Error::new(format!("image {} shares code with {c} but differs in platform, base or language", m.id)));
                }
            }
        }
        Ok(())
    }

    pub fn image(&self, id: &str) -> Option<&MapImage> {
        self.images.iter().find(|m| m.id == id)
    }

    pub fn by_sha(&self, sha256: &str) -> Option<&MapImage> {
        self.images.iter().find(|m| m.sha256 == sha256)
    }

    /// The images analysed in their own right (no `code_image`).
    pub fn code_images(&self) -> impl Iterator<Item = &MapImage> {
        self.images.iter().filter(|m| m.code_image.is_none())
    }
}

/// Apply one step to the file `input`.
pub fn apply<R: Read + Seek>(step: &Step, input: &mut R) -> Result<Vec<u8>, Error> {
    let all = |r: &mut R| -> Result<Vec<u8>, Error> {
        r.seek(SeekFrom::Start(0)).map_err(|e| Error::new(format!("seek: {e}")))?;
        let mut v = Vec::new();
        r.read_to_end(&mut v).map_err(|e| Error::new(format!("read: {e}")))?;
        Ok(v)
    };
    match step {
        Step::Xdvdfs(path) => crate::xdvdfs::read_file(input, path),
        Step::Xex(key) => crate::xbox::unpack_xex(&all(input)?, *key).map(|(pe, _)| pe),
        Step::GhidraPe => crate::xbox::ghidra_pe(&all(input)?),
        Step::Pkg(path) => {
            let pkg = mercs2_formats::ps3_pkg::parse_pkg(&all(input)?).map_err(|e| Error::new(format!("pkg: {e}")))?;
            pkg.file_bytes(path)
                .map(|b| b.to_vec())
                .ok_or_else(|| Error::new(format!("pkg {} has no entry {path}", pkg.content_id)))
        }
        Step::Unself(kind) => {
            mercs2_formats::ps3_self::decrypt_self(&all(input)?, *kind).map_err(|e| Error::new(format!("unself: {e}")))
        }
    }
}

/// sha256 of a whole stream, read in 8 MiB chunks.
pub fn stream_sha256<R: Read + Seek>(r: &mut R) -> Result<String, Error> {
    r.seek(SeekFrom::Start(0)).map_err(|e| Error::new(format!("seek: {e}")))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 8 << 20];
    loop {
        let k = r.read(&mut buf).map_err(|e| Error::new(format!("read: {e}")))?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
    }
    Ok(format!("{:x}", h.finalize()))
}

/// Rebuild `image` from `container`, the first file of its chain: the container's sha256 must be
/// the chain's first `from_sha256`, every intermediate file the next link's, and the result the
/// image's.
pub fn derive<R: Read + Seek>(image: &MapImage, container: &mut R) -> Result<Vec<u8>, Error> {
    derive_keeping_xex(image, container, false).map(|(out, _)| out)
}

/// Rebuild `image` as [`derive`] does and also return the XEX2 file its chain unpacks (the input
/// of its one `xex:` step). Fails when the chain has no `xex:` step or more than one.
pub fn derive_with_xex<R: Read + Seek>(image: &MapImage, container: &mut R) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let xex_steps = image.chain.iter().filter(|l| matches!(l.step, Step::Xex(_))).count();
    if xex_steps != 1 {
        return Err(Error::new(format!(
            "image {}: its derivation chain has {xex_steps} xex steps, not one",
            image.id
        )));
    }
    let (out, xex) = derive_keeping_xex(image, container, true)?;
    let xex = xex.ok_or_else(|| Error::new(format!("image {}: the xex step's input was not kept", image.id)))?;
    Ok((out, xex))
}

fn derive_keeping_xex<R: Read + Seek>(
    image: &MapImage,
    container: &mut R,
    keep_xex: bool,
) -> Result<(Vec<u8>, Option<Vec<u8>>), Error> {
    let first = image
        .chain
        .first()
        .ok_or_else(|| Error::new(format!("image {} has no derivation chain", image.id)))?;
    let got = stream_sha256(container)?;
    if got != first.from_sha256 {
        return Err(Error::new(format!(
            "image {}: the container has sha256 {got}, the chain starts from {}",
            image.id, first.from_sha256
        )));
    }
    let mut xex = None;
    if keep_xex && matches!(first.step, Step::Xex(_)) {
        container.seek(SeekFrom::Start(0)).map_err(|e| Error::new(format!("seek: {e}")))?;
        let mut v = Vec::new();
        container.read_to_end(&mut v).map_err(|e| Error::new(format!("read: {e}")))?;
        xex = Some(v);
    }
    let mut out = apply(&first.step, container)?;
    for link in &image.chain[1..] {
        let sha = sha256_hex(&out);
        if sha != link.from_sha256 {
            return Err(Error::new(format!(
                "image {}: step output has sha256 {sha}, the next link expects {}",
                image.id, link.from_sha256
            )));
        }
        if keep_xex && matches!(link.step, Step::Xex(_)) {
            xex = Some(out.clone());
        }
        out = apply(&link.step, &mut std::io::Cursor::new(out))?;
    }
    let sha = sha256_hex(&out);
    if sha != image.sha256 || out.len() as u64 != image.size {
        return Err(Error::new(format!(
            "image {}: the chain yields sha256 {sha} ({} bytes), images.tsv says {} ({} bytes)",
            image.id,
            out.len(),
            image.sha256,
            image.size
        )));
    }
    Ok((out, xex))
}

/// The executable sections of a PS3 ELF or Xbox PE, as (address, bytes), in header order.
fn code_sections(image: &[u8]) -> Result<Vec<(u64, Vec<u8>)>, Error> {
    if image.starts_with(b"MZ") {
        let pe = securom_unwrap::Pe::parse(image).map_err(|e| Error::new(format!("PE parse: {e}")))?;
        let mut out = Vec::new();
        for s in pe.sections.iter().filter(|s| s.characteristics & 0x2000_0000 != 0) {
            let off = pe.rva_to_off(s.vaddr).ok_or_else(|| Error::new(format!("section {} does not map", s.name)))?;
            let bytes = image
                .get(off..off + s.raw_size as usize)
                .ok_or_else(|| Error::new(format!("section {} runs past the file", s.name)))?;
            out.push((pe.image_base as u64 + s.vaddr as u64, bytes.to_vec()));
        }
        Ok(out)
    } else if image.starts_with(b"\x7fELF") {
        let be16 = |o: usize| u16::from_be_bytes(image[o..o + 2].try_into().unwrap());
        let be64 = |o: usize| u64::from_be_bytes(image[o..o + 8].try_into().unwrap());
        let shoff = be64(0x28) as usize;
        let shentsize = be16(0x3a) as usize;
        let shnum = be16(0x3c) as usize;
        let mut out = Vec::new();
        for i in 0..shnum {
            let o = shoff + i * shentsize;
            let flags = be64(o + 8);
            let (addr, off, size) = (be64(o + 16), be64(o + 24) as usize, be64(o + 32) as usize);
            if flags & 0x4 != 0 && size > 0 {
                let bytes = image.get(off..off + size).ok_or_else(|| Error::new(format!("ELF section {i} runs past the file")))?;
                out.push((addr, bytes.to_vec()));
            }
        }
        Ok(out)
    } else {
        Err(Error::new("not a PE (MZ) or ELF image"))
    }
}

/// Check that `variant` shares its code with `code_image`: identical executable sections (address
/// and bytes) and identical function starts.
pub fn shared_code(variant: &[u8], code_image: &[u8]) -> Result<(), Error> {
    let a = code_sections(variant)?;
    let b = code_sections(code_image)?;
    if a.len() != b.len() {
        return Err(Error::new(format!("{} executable sections against {}", a.len(), b.len())));
    }
    for ((aa, ab), (ba, bb)) in a.iter().zip(&b) {
        if aa != ba || ab != bb {
            let first = ab.iter().zip(bb.iter()).position(|(x, y)| x != y);
            return Err(Error::new(format!(
                "executable section at 0x{aa:x} differs from the one at 0x{ba:x} (first differing offset {first:?}, lengths {} and {})",
                ab.len(),
                bb.len()
            )));
        }
    }
    let (sa, _) = function_starts(variant)?;
    let (sb, _) = function_starts(code_image)?;
    if sa != sb {
        return Err(Error::new(format!("function starts differ ({} against {})", sa.len(), sb.len())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalogue {
        Catalogue::embedded().unwrap()
    }

    #[test]
    fn the_embedded_table_parses_and_cross_checks() {
        let t = Images::embedded(&cat()).unwrap();
        assert!(t.image("x360-preview-profile").is_some());
        assert_eq!(t.image("ps3-v103-de").unwrap().code_image.as_deref(), Some("ps3-v103-na"));
        assert!(t.code_images().all(|m| m.code_image.is_none()));
    }

    #[test]
    fn steps_parse_by_exact_name() {
        assert_eq!(Step::parse("xex:devkit"), Some(Step::Xex(XexKey::Devkit)));
        assert_eq!(Step::parse("ghidra-pe"), Some(Step::GhidraPe));
        assert_eq!(Step::parse("unself:npdrm"), Some(Step::Unself(SelfKind::Npdrm)));
        assert_eq!(Step::parse("xdvdfs:default.xex"), Some(Step::Xdvdfs("default.xex".into())));
        assert_eq!(Step::parse("xdvdfs:"), None);
        assert_eq!(Step::parse("unself:other"), None);
        assert_eq!(Step::parse("ghidra-pe:x"), None);
    }

    fn row(fields: [&str; 10]) -> String {
        format!("{IMAGES_HEADER}\n{}\n", fields.join("\t"))
    }

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn rows_that_break_the_rules_are_rejected() {
        let c = cat();
        // console row without a chain
        let e = Images::parse(&row(["x", "x360", "L", SHA_A, "1", "0x0", "-", "-", "-", "-"]), &c).unwrap_err().to_string();
        assert!(e.contains("carries no derivation chain"), "{e}");
        // console row naming a build
        let e = Images::parse(&row(["x", "ps3", "L", SHA_A, "1", "0x0", "b", "-", SHA_B, "unself:app"]), &c).unwrap_err().to_string();
        assert!(e.contains("only pc images"), "{e}");
        // pc row whose build is unknown
        let e = Images::parse(&row(["x", "pc", "L", SHA_A, "1", "0x0", "nope", "-", "-", "-"]), &c).unwrap_err().to_string();
        assert!(e.contains("does not list"), "{e}");
        // unknown step
        let e = Images::parse(&row(["x", "ps3", "L", SHA_A, "1", "0x0", "-", "-", SHA_B, "decrypt"]), &c).unwrap_err().to_string();
        assert!(e.contains("not a known step"), "{e}");
        // code_image that does not exist
        let e = Images::parse(&row(["x", "ps3", "L", SHA_A, "1", "0x0", "-", "y", SHA_B, "unself:app"]), &c).unwrap_err().to_string();
        assert!(e.contains("shares code with y"), "{e}");
    }

    #[test]
    fn a_no_value_field_reads_as_none_and_an_empty_field_is_rejected() {
        let c = cat();
        let t = Images::parse(&row(["x", "ps3", "L", SHA_A, "1", "0x0", "-", "-", SHA_B, "unself:app"]), &c).unwrap();
        assert_eq!(t.images[0].build_id, None);
        assert_eq!(t.images[0].code_image, None);
        assert_eq!(t.images[0].chain.len(), 1);
        let embedded = Images::embedded(&c).unwrap();
        let drmfree = embedded.image("pc-v10-drmfree").unwrap();
        assert_eq!(drmfree.build_id.as_deref(), Some("v10-drmfree-laa"));
        assert!(drmfree.code_image.is_none() && drmfree.chain.is_empty());
        let e = Images::parse(&row(["x", "ps3", "L", SHA_A, "1", "0x0", "", "-", SHA_B, "unself:app"]), &c).unwrap_err().to_string();
        assert!(e.contains("images.tsv line 2: column 7 (build_id) is empty"), "{e}");
        let e = Images::parse(&row(["x", "pc", "L", SHA_A, "1", "0x0", "b", "-", "-", ""]), &c).unwrap_err().to_string();
        assert!(e.contains("images.tsv line 2: column 10 (step) is empty"), "{e}");
        let e = Images::parse(&row(["-", "ps3", "L", SHA_A, "1", "0x0", "-", "-", SHA_B, "unself:app"]), &c).unwrap_err().to_string();
        assert!(e.contains("image_id holds no value"), "{e}");
    }

    #[test]
    fn derive_checks_every_sha_on_the_chain() {
        // A container whose sha256 is not the chain's first is refused before any step runs.
        let m = MapImage {
            id: "t".into(),
            platform: Platform::X360,
            ghidra_language: "L".into(),
            sha256: SHA_A.into(),
            size: 1,
            base: 0,
            build_id: None,
            code_image: None,
            chain: vec![Link { from_sha256: SHA_B.into(), step: Step::GhidraPe }],
        };
        let e = derive(&m, &mut std::io::Cursor::new(b"not it".to_vec())).unwrap_err().to_string();
        assert!(e.contains("the chain starts from"), "{e}");
    }

    #[test]
    fn derive_with_xex_refuses_a_chain_without_one_xex_step() {
        let mut m = MapImage {
            id: "t".into(),
            platform: Platform::Ps3,
            ghidra_language: "L".into(),
            sha256: SHA_A.into(),
            size: 1,
            base: 0,
            build_id: None,
            code_image: None,
            chain: vec![Link { from_sha256: SHA_B.into(), step: Step::Unself(SelfKind::App) }],
        };
        let e = derive_with_xex(&m, &mut std::io::Cursor::new(Vec::new())).unwrap_err().to_string();
        assert!(e.contains("0 xex steps"), "{e}");
        let x = Link { from_sha256: SHA_B.into(), step: Step::Xex(XexKey::Retail) };
        m.chain = vec![x.clone(), x];
        let e = derive_with_xex(&m, &mut std::io::Cursor::new(Vec::new())).unwrap_err().to_string();
        assert!(e.contains("2 xex steps"), "{e}");
        let pc = Images::embedded(&cat()).unwrap().image("pc-v10").unwrap().clone();
        let e = derive_with_xex(&pc, &mut std::io::Cursor::new(Vec::new())).unwrap_err().to_string();
        assert!(e.contains("0 xex steps"), "{e}");
    }
}
