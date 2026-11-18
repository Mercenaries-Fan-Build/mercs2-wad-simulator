//! Game folders for the load plan, shared by `compat.rs`, `cli.rs` and `shader.rs`.
//!
//! The plan reads the link's blocks from the game's WADs (`build::link_block_paths`), so a hermetic
//! plan needs a `data/vz.wad` and a `data/shell.wad` that open. These are WADs the writer builds with
//! no blocks: they carry no scripts block, so no LOD rung either.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Write a block-less `vz.wad` and `shell.wad` into `root`'s `data` folder, creating it.
pub fn write_wads(root: &Path) {
    let data = root.join("data");
    std::fs::create_dir_all(&data).expect("the data folder");
    let wad = mercs2_formats::patch_wad::build_patch_wad_multi(&[], 0, &mercs2_formats::patch_wad::FFCS_CERT_BLOB)
        .expect("a WAD with no blocks");
    for name in ["vz.wad", "shell.wad"] {
        std::fs::write(data.join(name), &wad).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

/// A fresh game folder named by `tag`: a copy of every file under `fixture` (none when `None`), with
/// [`write_wads`] over it. Each call gets its own folder, so tests sharing a fixture never race.
pub fn game_folder(tag: &str, fixture: Option<&Path>) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "qm_game_{}_{tag}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("the game folder");
    if let Some(from) = fixture {
        copy_dir(from, &root);
    }
    write_wads(&root);
    root
}

/// One game folder with nothing but [`write_wads`], made once per test binary.
pub fn empty_game() -> &'static Path {
    static GAME: OnceLock<PathBuf> = OnceLock::new();
    GAME.get_or_init(|| game_folder("empty", None))
}

fn copy_dir(from: &Path, to: &Path) {
    for e in std::fs::read_dir(from).unwrap_or_else(|e| panic!("{}: {e}", from.display())) {
        let e = e.expect("a fixture entry");
        let dest = to.join(e.file_name());
        if e.file_type().expect("a fixture entry's type").is_dir() {
            std::fs::create_dir_all(&dest).expect("a fixture folder");
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), &dest).unwrap_or_else(|err| panic!("{}: {err}", e.path().display()));
        }
    }
}
