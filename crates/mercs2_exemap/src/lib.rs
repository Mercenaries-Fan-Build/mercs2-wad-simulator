//! `mercs2_exemap` — the cross-build map of Mercenaries 2 PC executables.
//!
//! Every known `Mercenaries2.exe` build (and every process-image dump taken from one) is a row of
//! `data/builds.tsv`, identified by sha256 alone. Builds fall into three code layouts, each named by
//! the PE timestamp it was compiled with and pinned to one reference build in `data/layouts.tsv`.
//! Each layout is one code version of the game, recorded per build in the `code_version` column:
//!
//! * `U` — timestamp `0x48a370c7`, reference `v10-cracked-pmcbb`, code version 1.0;
//! * `E` — timestamp `0x48cae190`, reference `v11-patched-securom`, code version 1.1;
//! * `D` — timestamp `0x48cb59e8`, reference `demo-official-securom` (the PC demo's
//!   `Merc2-Demo.exe` as its installer ships it), code version `demo`.
//!
//! The official v1.1 patcher (sha256 `b27fedc2…`, the same file in all four regions) turns
//! `v10-ea-signed` (E) into `v11-patched-securom` (E). Every
//! cracked, DRM-free and SecuROM-unwrapped build in the U layout runs v1.0 code.
//!
//! A build belongs to its layout when every byte that differs from the reference is accounted for by
//! one of the build's rows in `data/overlays.tsv`. [`membership`] checks exactly that and fails on
//! anything else; [`membership::runs`] documents how differences are found.
//!
//! `data/aliases.tsv` maps other names for a build (ids Modkit uses among them) to its canonical
//! id, with the reason the alias is not canonical. [`identify`] returns the canonical id;
//! [`Catalogue::build`] and [`Catalogue::canonical_id`] accept an alias wherever they accept an id.
//!
//! The tables are compiled in; [`Catalogue::embedded`] parses and cross-checks them. No field of
//! any `data/*.tsv` table is empty: a column with no value holds [`NO_VALUE`] (`-`), and an empty
//! field is an error naming the table, line and column.
//!
//! `data/images.tsv` lists the images the function map spans on every platform ([`images`]): the
//! PC images by build id (the two layout references' process dumps and the DRM-free v1.0
//! executable), and each console image with the chain that rebuilds it from
//! its original container. Console executables come in through [`xdvdfs`] (a file from an Xbox
//! disc image), [`xbox`] (XEX2 → PE image, the PE layout Ghidra's loader reads, and XEX imports),
//! [`prx`] (the PRX imports of a PS3 ELF) and [`starts`] (function starts from Xbox `.pdata` or
//! PS3 `.opd`). `data/xbox_exports.tsv` (Xenia's export tables) names each Xbox import by library
//! and ordinal ([`xbox::XboxExports`]); `data/ps3_fnid_names.tsv` (candidate names, each checked
//! against its FNID hash on load) and `data/ps3_fixed_nids.tsv` (module-level NIDs that are not a
//! hash) name each PS3 import by FNID ([`prx::FnidNames`]). [`rtti`] recovers vtables from C++ type
//! information and [`luareg`] the Lua C-function registration tables, on every platform;
//! [`matcher`] pairs the functions of two images from those exports.
//!
//! `data/corpora.tsv` and `data/wads.tsv` catalogue the Lua corpus by product, platform and build,
//! each WAD with the chain that rebuilds it from its original file or the install it is found in
//! ([`corpus`]); PS3 disc images are read through [`iso9660`].
//!
//! [`canonical`] finds every catalogued PC build by sha256 in the canonical tree of PC builds.

pub mod canonical;
pub mod catalogue;
pub mod corpus;
mod dtor;
pub mod image;
pub mod images;
pub mod iso9660;
pub mod luachunk;
pub mod luaindex;
pub mod lualink;
pub mod luapair;
pub mod luareg;
pub mod matcher;
pub mod membership;
mod mem;
pub mod prx;
pub mod rtti;
pub mod starts;
pub mod wadscripts;
pub mod xbox;
pub mod xdvdfs;
pub mod xmap;

use md5::Md5;
use sha2::{Digest, Sha256};

pub use catalogue::{Alias, Build, Catalogue, CodeVersion, Kind, Layout, Overlay, Space};

/// Every failure in this crate: a message saying what was not handled and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Error {
        Error(msg.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// The field a `data/*.tsv` table writes where a column holds no value. No field of those tables is
/// empty, so no row ends in whitespace.
pub const NO_VALUE: &str = "-";

/// The fields of one row of the `data/*.tsv` table `table` (line `n`), split on tabs: exactly as
/// many as `header` names. An empty field is an error naming the table, line and column; a column
/// with no value holds [`NO_VALUE`].
pub(crate) fn table_row<'t>(table: &str, header: &str, n: usize, line: &'t str) -> Result<Vec<&'t str>, Error> {
    let columns: Vec<&str> = header.split('\t').collect();
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != columns.len() {
        return Err(Error::new(format!("{table} line {n}: {} fields, expected {}", fields.len(), columns.len())));
    }
    if let Some(i) = fields.iter().position(|f| f.is_empty()) {
        return Err(Error::new(format!(
            "{table} line {n}: column {} ({}) is empty; a column with no value holds {NO_VALUE:?}",
            i + 1,
            columns[i]
        )));
    }
    Ok(fields)
}

/// The rows of the `data/*.tsv` table `table` whose text opens with `#` comment lines (its
/// provenance) followed by the header line `header`, which must match exactly: (line number,
/// fields) per row, each split by [`table_row`]. Only the lines before the header are comments.
pub(crate) fn commented_table_rows<'t>(table: &str, header: &str, text: &'t str) -> Result<Vec<(usize, Vec<&'t str>)>, Error> {
    let mut lines = text.lines().enumerate().skip_while(|(_, l)| l.starts_with('#'));
    match lines.next() {
        Some((_, l)) if l == header => {}
        other => {
            return Err(Error::new(format!(
                "{table}: the first line after the # comment lines is {:?}, not the header {header:?}",
                other.map(|(_, l)| l)
            )))
        }
    }
    lines.map(|(i, l)| table_row(table, header, i + 1, l).map(|f| (i + 1, f))).collect()
}

/// `None` for a field holding [`NO_VALUE`], the field otherwise.
pub(crate) fn field_value(field: &str) -> Option<&str> {
    (field != NO_VALUE).then_some(field)
}

/// Lowercase hex sha256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The catalogued build whose sha256 is that of `bytes`.
///
/// An unknown sha256 is an error naming it. A known sha256 whose catalogued md5 or size disagrees
/// with `bytes` is an error too: the row is wrong.
pub fn identify_in<'c>(catalogue: &'c Catalogue, bytes: &[u8]) -> Result<&'c Build, Error> {
    let sha = sha256_hex(bytes);
    let b = catalogue.build_by_sha(&sha).ok_or_else(|| {
        Error::new(format!("unknown build: sha256 {sha} ({} bytes) is not in builds.tsv", bytes.len()))
    })?;
    let md5 = format!("{:x}", Md5::digest(bytes));
    if b.md5 != md5 || b.size != bytes.len() as u64 {
        return Err(Error::new(format!(
            "build {} matches by sha256 but the bytes have md5 {md5} and size {}; builds.tsv says {} and {}",
            b.id,
            bytes.len(),
            b.md5,
            b.size
        )));
    }
    Ok(b)
}

/// The canonical build id of `bytes`, against the embedded catalogue.
pub fn identify(bytes: &[u8]) -> Result<String, Error> {
    let c = Catalogue::embedded()?;
    identify_in(&c, bytes).map(|b| b.id.clone())
}

/// The overlays of `build` relative to its layout reference `reference`, against the embedded
/// catalogue. Fails on any difference the build's overlay set does not list, and on any listed
/// overlay the bytes do not reproduce.
pub fn membership(build: &[u8], reference: &[u8]) -> Result<Vec<Overlay>, Error> {
    let c = Catalogue::embedded()?;
    membership::check(&c, build, reference).map(|rows| rows.into_iter().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_sha_is_an_error_naming_it() {
        let e = identify(b"not a catalogued exe").unwrap_err().to_string();
        assert!(e.contains(&sha256_hex(b"not a catalogued exe")), "{e}");
        assert!(e.contains("unknown build"), "{e}");
    }

    #[test]
    fn a_table_row_keeps_the_no_value_marker_and_refuses_an_empty_field() {
        let f = table_row("t.tsv", "a\tb\tc", 2, "x\t-\tz").unwrap();
        assert_eq!(f, ["x", "-", "z"]);
        assert_eq!(field_value(f[0]), Some("x"));
        assert_eq!(field_value(f[1]), None);
        let e = table_row("t.tsv", "a\tb\tc", 7, "x\t\tz").unwrap_err().to_string();
        assert!(e.contains("t.tsv line 7: column 2 (b) is empty"), "{e}");
        let e = table_row("t.tsv", "a\tb\tc", 3, "x\ty\t").unwrap_err().to_string();
        assert!(e.contains("t.tsv line 3: column 3 (c) is empty"), "{e}");
        let e = table_row("t.tsv", "a\tb\tc", 4, "x\ty").unwrap_err().to_string();
        assert!(e.contains("t.tsv line 4: 2 fields, expected 3"), "{e}");
    }
}
