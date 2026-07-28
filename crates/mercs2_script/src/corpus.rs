//! Locating the vendored game-script corpus (`corpus/`, inside this crate).
//!
//! The corpus is the decompiled Lua the shipped game runs, and it is the **behavioural spec** for this
//! crate's binding surface: real call arities, real argument shapes, the clamps and control flow the
//! bodies have to satisfy. It is **vendored in-tree** (see `corpus/README.md`) so the pipeline resolves
//! it deterministically — and it sits **inside this package**, so it ships to downstream users with
//! the crate rather than only existing in our checkout.
//!
//! **Never hardcode a path to it.** Everything here derives from `CARGO_MANIFEST_DIR`, which Cargo
//! resolves at build time, so the tree works from any checkout location and under any CI layout. An
//! earlier revision searched a list of guessed sibling-checkout paths; on a machine laid out
//! differently every corpus-driven test silently degraded to "0 [lua] lines" rather than failing
//! loudly, which is how a boot test came to report a regression it did not have.
//!
//! [`MERCS2_LUA_CORPUS`](CORPUS_ENV) still overrides, for pointing a run at a different decompile.

use std::path::PathBuf;

/// Environment variable that overrides corpus discovery. Intended for pointing a run at a different
/// decompile, not for normal use — the vendored copy is the default and needs no configuration.
pub const CORPUS_ENV: &str = "MERCS2_LUA_CORPUS";

/// This crate's own directory, which is where the corpus lives.
///
/// Deliberately **inside the package**: `CARGO_MANIFEST_DIR` points at the extracted `.crate` when this
/// crate is consumed from the registry, so a downstream user gets the corpus with the dependency and
/// needs no checkout of ours. A workspace-root location would resolve only for us.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The corpus root (`<crate>/corpus/mercs2-luacd/src`), or the [`CORPUS_ENV`] override.
///
/// Returns `None` only if the vendored tree is genuinely absent — e.g. a consumer who stripped it, or
/// a vendoring tool configured to exclude it. Callers should print [`skip_notice`] and return rather
/// than fail in that case.
pub fn root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(CORPUS_ENV) {
        // Honour an explicit override verbatim: if it is wrong, report a missing corpus at the path the
        // user actually asked for rather than silently falling back to the vendored copy.
        let p = PathBuf::from(p);
        return p.is_dir().then_some(p);
    }
    let vendored = crate_root().join("corpus").join("mercs2-luacd").join("src");
    vendored.is_dir().then(|| vendored.canonicalize().unwrap_or(vendored))
}

/// The stand-in root (`<crate>/corpus/stubs`) for shipped modules the 370/382 decompile lacks.
///
/// Search it **after** [`root`], so a module that later decompiles automatically shadows its stand-in.
pub fn stubs() -> Option<PathBuf> {
    let dir = crate_root().join("corpus").join("stubs");
    dir.is_dir().then(|| dir.canonicalize().unwrap_or(dir))
}

/// The module roots a script host should be built with: the corpus first, then the stand-ins.
///
/// Empty when the corpus is absent, which callers treat as "skip, do not fail".
pub fn roots() -> Vec<PathBuf> {
    root().into_iter().chain(stubs()).collect()
}

/// The one-line message a test or example should print when [`root`] returns `None`, naming the
/// override so the reader knows how to opt in.
pub fn skip_notice(who: &str) -> String {
    format!(
        "[{who}] SKIP: vendored script corpus not found at <mercs2_script>/corpus/mercs2-luacd/src. \
         It ships with this crate; set {CORPUS_ENV} to point at a decompile if yours lives elsewhere."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corpus is vendored, so it resolves with no configuration — the property the whole pipeline
    /// depends on. A failure here means the in-tree copy is missing or moved.
    #[test]
    fn the_vendored_corpus_resolves_without_configuration() {
        // Guard against a stray override in the environment skewing the check.
        assert!(
            std::env::var_os(CORPUS_ENV).is_none(),
            "unset {CORPUS_ENV} to exercise the vendored default"
        );
        let root = root().expect("the corpus is vendored in-tree and must resolve");
        assert!(root.join("resident").is_dir(), "resident/ present");
        assert!(root.join("vz").is_dir(), "vz/ present");
        assert!(root.join("shell").is_dir(), "shell/ present");
    }

    /// The stand-in root resolves too, and [`roots`] orders the corpus ahead of it so a real module
    /// always wins a name collision against our stand-in.
    #[test]
    fn stubs_resolve_and_are_searched_after_the_corpus() {
        assert!(stubs().expect("stub root is vendored").is_dir());
        let roots = roots();
        assert_eq!(roots.len(), 2);
        assert!(roots[0].ends_with("src"), "the corpus is searched first");
        assert!(roots[1].ends_with("stubs"), "the stand-ins are searched second");
    }

    /// The skip notice names the override, because that string is the only guidance a consumer without
    /// the corpus ever sees.
    #[test]
    fn skip_notice_names_the_override() {
        let m = skip_notice("corpus-replay");
        assert!(m.contains(CORPUS_ENV));
        assert!(m.contains("corpus-replay"));
    }
}
