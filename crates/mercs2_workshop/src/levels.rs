//! The levels the game can boot by name, found beside the base WAD.
//!
//! A level is a WAD holding the master script `<name>` and the layer `<name>_base`
//! ([`mercs2_quartermaster::level::holds_level`]), named `<name>-patch.wad` the way a Shipment places
//! it, or `<name>.wad`. Nothing else in a WAD says it is a level, and those two tell one from a patch
//! or language WAD without a list of names.
//!
//! The game mounts a level's patch above a copy of `vz.wad`. The workshop mounts it above `vz.wad`
//! itself, which is the same view.

use std::path::{Path, PathBuf};

use mercs2_engine::wad;
use mercs2_quartermaster::level::holds_level;

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    /// The name the game boots it by.
    pub name: String,
    pub wad: PathBuf,
}

/// The level name a WAD file would be: its stem, less a `-patch`.
fn stem(p: &Path) -> Option<String> {
    let s = p.file_stem().and_then(|s| s.to_str())?;
    Some(s.strip_suffix("-patch").unwrap_or(s).to_string())
}

/// The base WAD's level, then every other level among the WADs beside it and `extra` (each
/// `--overlay`), by name. A WAD that will not open is an error naming it: it may be the level.
pub fn found(base: &Path, extra: &[String]) -> Result<Vec<Level>, String> {
    let name = stem(base).ok_or_else(|| format!("{} has no file name", base.display()))?;
    let dir = base.parent().ok_or_else(|| format!("{} has no folder", base.display()))?;
    let mut paths: Vec<PathBuf> = extra.iter().map(PathBuf::from).collect();
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("wad")) {
            paths.push(p);
        }
    }
    paths.retain(|p| p != base);
    paths.sort();
    paths.dedup();
    let mut levels = Vec::new();
    for p in paths {
        let w = wad::open(&p.to_string_lossy()).map_err(|e| format!("{}: {e}", p.display()))?;
        let Some(n) = stem(&p).filter(|n| *n != name) else { continue };
        if holds_level(wad::all_asets(&w).into_iter().map(|(h, t, _)| (h, t)), &n) {
            levels.push(Level { name: n, wad: p });
        }
    }
    levels.sort_by(|a, b| a.name.cmp(&b.name));
    levels.insert(0, Level { name, wad: base.to_path_buf() });
    Ok(levels)
}

/// The level called `name` among `levels`; an error listing them when there is none.
pub fn pick<'a>(levels: &'a [Level], name: &str) -> Result<&'a Level, String> {
    levels.iter().find(|l| l.name.eq_ignore_ascii_case(name)).ok_or_else(|| {
        let names: Vec<&str> = levels.iter().map(|l| l.name.as_str()).collect();
        format!("no level {name:?} beside the game archive; found {}", names.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Shipment's `dlc01-patch.wad` is level `dlc01`, as a bare `dlc01.wad` is.
    #[test]
    fn a_patch_wad_names_its_level() {
        assert_eq!(stem(Path::new("data/dlc01-patch.wad")).as_deref(), Some("dlc01"));
        assert_eq!(stem(Path::new("data/dlc01.wad")).as_deref(), Some("dlc01"));
    }

    #[test]
    fn a_wad_that_will_not_open_is_an_error_naming_it() {
        let dir = std::env::temp_dir().join(format!("m2ws_levels_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("vz.wad"), b"x").unwrap();
        std::fs::write(dir.join("broken.WAD"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let e = found(&dir.join("vz.wad"), &[]).unwrap_err();
        assert!(e.contains("broken.WAD"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pick_folds_case_and_lists_what_it_found() {
        let l = [Level { name: "vz".into(), wad: "a/vz.wad".into() }, Level { name: "dlc01".into(), wad: "a/dlc01.wad".into() }];
        assert_eq!(pick(&l, "DLC01").unwrap().name, "dlc01");
        let e = pick(&l, "dlc02").unwrap_err();
        assert!(e.contains("dlc02") && e.contains("vz, dlc01"), "{e}");
    }
}
