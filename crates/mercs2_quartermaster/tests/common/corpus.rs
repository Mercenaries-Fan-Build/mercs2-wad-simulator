//! The vendored Lua corpus, shared by `link.rs` and `link_retail.rs`.

use std::path::{Path, PathBuf};

/// The in-tree decompiled Lua corpus (`crates/mercs2_script/corpus/mercs2-luacd/src`), found by
/// walking up from this crate, or `None` when the checkout has none.
pub fn corpus_root() -> Option<PathBuf> {
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let c = d.join("crates/mercs2_script/corpus/mercs2-luacd/src");
        if c.is_dir() {
            return Some(c);
        }
        dir = d.parent();
    }
    None
}
