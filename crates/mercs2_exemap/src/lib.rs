//! `mercs2_exemap` — the cross-build map of Mercenaries 2 PC executables.
//!
//! Every known `Mercenaries2.exe` build (and every process-image dump taken from one) is a row of
//! `data/builds.tsv`, identified by sha256 alone. Builds fall into two code layouts, each named by
//! the PE timestamp it was compiled with and pinned to one reference build in `data/layouts.tsv`:
//!
//! * `U` — timestamp `0x48a370c7`, reference `v11-cracked-pmcbb`;
//! * `E` — timestamp `0x48cae190`, reference `v11-patched-securom`.
//!
//! A build belongs to its layout when every byte that differs from the reference is accounted for by
//! one of the build's rows in `data/overlays.tsv`. [`membership`] checks exactly that and fails on
//! anything else; [`membership::runs`] documents how differences are found.
//!
//! The tables are compiled in; [`Catalogue::embedded`] parses and cross-checks them.

pub mod catalogue;
pub mod membership;
pub mod pe;

use md5::Md5;
use sha2::{Digest, Sha256};

pub use catalogue::{Build, Catalogue, Kind, Layout, Overlay, Space};

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

/// The build id of `bytes`, against the embedded catalogue.
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
