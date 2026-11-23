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

/// The DLC corpus root (`<crate>/corpus/mercs2-dlc-luacd/src`): the decompiled Lua of the console
/// DLC levels, one folder per level. Not a module root of the retail script host.
///
/// `None` only if the vendored tree is absent.
pub fn dlc_root() -> Option<PathBuf> {
    let vendored = crate_root().join("corpus").join("mercs2-dlc-luacd").join("src");
    vendored.is_dir().then(|| vendored.canonicalize().unwrap_or(vendored))
}

/// The module roots a script host should be built with: the corpus.
///
/// Empty when the corpus is absent, which callers treat as "skip, do not fail".
pub fn roots() -> Vec<PathBuf> {
    root().into_iter().collect()
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

    /// The DLC corpus is vendored beside it, one folder per DLC level.
    #[test]
    fn the_vendored_dlc_corpus_resolves() {
        let root = dlc_root().expect("the DLC corpus is vendored in-tree and must resolve");
        assert!(root.join("dlc01").is_dir(), "dlc01/ present");
        assert!(root.join("dlctest01").is_dir(), "dlctest01/ present");
    }

    /// The script host's module roots are the corpus alone.
    #[test]
    fn the_module_roots_are_the_corpus() {
        assert_eq!(roots(), vec![root().expect("the corpus is vendored in-tree")]);
    }

    /// The subtitle modules `MrxGuiCinematic.ShowMovie` imports ship in the corpus, each with its
    /// `SubtitleData` table (`resident/mrxguicinematic.lua` re-enters `ShowMovie` until it has one).
    #[test]
    fn the_subtitle_modules_are_in_the_corpus() {
        let dir = root().expect("the corpus is vendored in-tree").join("vz_subtitles");
        let mut n = 0;
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with("subtitles_") && name.ends_with(".lua") {
                let text = std::fs::read_to_string(&p).unwrap();
                assert!(text.contains("SubtitleData = {"), "{name} has a SubtitleData table");
                n += 1;
            }
        }
        assert_eq!(n, 35, "the 35 subtitle modules in {}", dir.display());
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
