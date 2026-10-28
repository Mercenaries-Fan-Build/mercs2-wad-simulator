//! The vendored Lua tree against upstream: the half of the vendor guards that needs a second tree.
//!
//! `tools/lua51-mercs2/README.md` declares the patches to be the single source of truth, and
//! `vendor/` to be upstream `tools/lua51-src/src` with those patches applied. Nothing enforced
//! that, so the two could drift silently — and a drifted VM is the kind of failure that shows up
//! as a mod that builds fine and crashes the game.
//!
//! Rather than shell out to `patch` (absent on a stock Windows box), this asserts the invariant the
//! patches exist to produce: every file the patches do NOT touch is byte-identical to upstream,
//! modulo line endings. That each touched file carries its documented change is checked from
//! `vendor/` alone, in `vendor_integrity.rs`.
//!
//! Built by the `retail` feature because it reads a tree outside this repository: the pristine Lua
//! 5.1 `src` folder named by `lua51_src_dir` in the repo-root `.mercs2-local.toml`. It does not read
//! the game. A missing config, a missing key, or a path that is not a directory fails the test
//! naming the key. Run with `cargo xtask retail-test`.

use std::path::{Path, PathBuf};

/// Upstream lives outside the crate: `lua51_src_dir` in `.mercs2-local.toml` names it.
fn upstream_dir() -> PathBuf {
    mercs2_formats::game_paths::local_config_lua51_src_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("the vendor-vs-upstream comparison needs the pristine Lua 5.1 source tree: {e}"))
}

fn vendor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor")
}

/// Compare ignoring line endings — the vendored copy is CRLF, upstream is LF, and that difference
/// is not a code change.
fn same_ignoring_line_endings(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

/// The only files any patch is allowed to touch.
const PATCHED: [&str; 3] = ["luaconf.h", "ldump.c", "lundump.c"];

#[test]
fn vendor_is_upstream_plus_exactly_the_three_patched_files() {
    let upstream = upstream_dir();
    let vendor = vendor_dir();

    let mut checked = 0usize;
    let mut unexpected = Vec::new();

    for entry in std::fs::read_dir(&vendor).expect("read vendor/") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !(name.ends_with(".c") || name.ends_with(".h")) || PATCHED.contains(&name.as_str()) {
            continue;
        }
        let up = upstream.join(&name);
        assert!(up.is_file(), "{name} is in vendor/ but not upstream — where did it come from?");

        let a = std::fs::read_to_string(&up).expect("read upstream");
        let b = std::fs::read_to_string(&path).expect("read vendor");
        if !same_ignoring_line_endings(&a, &b) {
            unexpected.push(name);
        }
        checked += 1;
    }

    assert!(checked > 20, "expected the full Lua tree, only compared {checked} files");
    assert!(
        unexpected.is_empty(),
        "these vendored files differ from upstream but no patch claims them: {unexpected:?}. \
         Either the change belongs in tools/lua51-mercs2/patches/ or it should not be there."
    );
}
