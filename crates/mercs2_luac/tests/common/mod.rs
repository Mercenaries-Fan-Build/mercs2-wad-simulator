//! The vendored Lua corpus, shared by `parity.rs` and `parity_retail.rs`.
//!
//! ## Why the corpus is located by path — and why that is no longer forced
//!
//! **Historically:** `mercs2_script` owns the corpus, so it looked like the natural dependency, and
//! it was not. That crate linked mlua's vendored **Lua 5.4** while this one vendors a patched
//! **Lua 5.1**, and both export the same unprefixed C symbols (`lua_newstate`, `lua_pcall`,
//! `lua_close`, …). Linking both into one binary resolved each call to whichever definition the
//! linker picked, so a `lua_State` allocated by one runtime got parsed by the other. The failure was
//! a **SIGSEGV partway through the corpus**, not a link error — which is how it presented, and it
//! took a bisect to see that the crashing script compiled perfectly on its own.
//!
//! **That constraint is gone.** `mercs2_script` now runs *this* crate's VM: there is one Lua in the
//! workspace, and `mercs2_engine/tests/one_lua.rs` links the runtime and the compiler side by side
//! to keep it that way. The path lookup is kept anyway — it costs a dozen lines, it lets this test
//! run without pulling the whole engine binding surface in, and a compiler test that can be built
//! from a bare checkout is worth more than the tidiness of a dependency.

use std::path::{Path, PathBuf};

/// The vendored decompiled corpus, found relative to this crate rather than through
/// `mercs2_script::corpus::root()` — see the module note above on the symbol collision.
pub fn corpus_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MERCS2_LUA_CORPUS") {
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
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
