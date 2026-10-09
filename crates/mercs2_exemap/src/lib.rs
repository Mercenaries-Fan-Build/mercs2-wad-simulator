//! `mercs2_exemap` — the cross-build map of Mercenaries 2 PC executables.
//!
//! Every known `Mercenaries2.exe` build (and every process-image dump taken from one) is a row of
//! `data/builds.tsv`, identified by sha256 alone. Builds fall into two code layouts, each named by
//! the PE timestamp it was compiled with and pinned to one reference build in `data/layouts.tsv`.
//! Each layout is one code version of the game, recorded per build in the `code_version` column:
//!
//! * `U` — timestamp `0x48a370c7`, reference `v10-cracked-pmcbb`, code version 1.0;
//! * `E` — timestamp `0x48cae190`, reference `v11-official-unwrapped`, code version 1.1.
//!
//! The official v1.1 patcher (sha256 `b27fedc2…`, the same file in all four regions) turns
//! `v10-unsigned` (U) into `v11-official-securom` (E), which ties U to 1.0 and E to 1.1. Every
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
//! The tables are compiled in; [`Catalogue::embedded`] parses and cross-checks them.
//!
//! `data/images.tsv` lists the images the function map spans on every platform ([`images`]): the
//! PC layout references by build id, and each console image with the chain that rebuilds it from
//! its original container. Console executables come in through [`xdvdfs`] (a file from an Xbox
//! disc image), [`xbox`] (XEX2 → PE image, and the PE layout Ghidra's loader reads) and [`starts`]
//! (function starts from Xbox `.pdata` or PS3 `.opd`). [`rtti`] recovers vtables from C++ type
//! information and [`luareg`] the Lua C-function registration tables, on every platform;
//! [`matcher`] pairs the functions of two images from those exports.

pub mod catalogue;
mod dtor;
pub mod image;
pub mod images;
pub mod luareg;
pub mod matcher;
pub mod membership;
mod mem;
pub mod rtti;
pub mod starts;
pub mod xbox;
pub mod xdvdfs;

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
}
