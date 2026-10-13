//! The canonical tree of PC builds: one directory holding the PC installs, the archived and derived
//! executables and the process-image dumps.
//!
//! A catalogued build is found in it by sha256 alone, whatever its file name or folder. Only files
//! whose size is the size of a catalogued build are hashed, so the installs' WADs and DLLs are
//! never read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::corpus::file_sha256;
use crate::{Catalogue, Error};

/// Every file under `dir`, recursively, sorted by path.
pub fn files_under(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut out = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).map_err(|e| Error::new(format!("list {}: {e}", d.display())))? {
            let entry = entry.map_err(|e| Error::new(format!("list {}: {e}", d.display())))?;
            let kind = entry.file_type().map_err(|e| Error::new(format!("stat {}: {e}", entry.path().display())))?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                out.push(entry.path());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// sha256 → path for every file under `dir` whose size is the size of a build in `catalogue`.
/// When several files hold the same bytes, the first by path is kept.
pub fn builds_by_sha(catalogue: &Catalogue, dir: &Path) -> Result<BTreeMap<String, PathBuf>, Error> {
    let sizes: BTreeSet<u64> = catalogue.builds.iter().map(|b| b.size).collect();
    let mut out = BTreeMap::new();
    for path in files_under(dir)? {
        let size = std::fs::metadata(&path).map_err(|e| Error::new(format!("stat {}: {e}", path.display())))?.len();
        if sizes.contains(&size) {
            out.entry(file_sha256(&path)?).or_insert(path);
        }
    }
    Ok(out)
}

/// The path of build `id` in `files` (from [`builds_by_sha`] over `dir`).
pub fn build_path<'f>(catalogue: &Catalogue, files: &'f BTreeMap<String, PathBuf>, dir: &Path, id: &str) -> Result<&'f Path, Error> {
    let b = catalogue.build(id).ok_or_else(|| Error::new(format!("build {id} is not catalogued")))?;
    files
        .get(&b.sha256)
        .map(PathBuf::as_path)
        .ok_or_else(|| Error::new(format!("build {} (sha256 {}) is not under {}", b.id, b.sha256, dir.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_are_found_by_sha256_and_only_catalogued_sizes_are_hashed() {
        let c = Catalogue::embedded().unwrap();
        let root = std::env::temp_dir().join("mercs2_exemap_canonical_test");
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/other.bin"), b"not a build").unwrap();
        std::fs::write(root.join("a/x.txt"), b"x").unwrap();

        let files = builds_by_sha(&c, &root).unwrap();
        assert!(files.is_empty(), "{files:?}");
        assert_eq!(files_under(&root).unwrap(), vec![root.join("a/b/other.bin"), root.join("a/x.txt")]);

        let b = &c.builds[0];
        let e = build_path(&c, &files, &root, &b.id).unwrap_err().to_string();
        assert!(e.contains(&format!("build {} (sha256 {}) is not under", b.id, b.sha256)), "{e}");
        let e = build_path(&c, &files, &root, "no-such-build").unwrap_err().to_string();
        assert!(e.contains("build no-such-build is not catalogued"), "{e}");
        std::fs::remove_dir_all(&root).ok();
    }
}
