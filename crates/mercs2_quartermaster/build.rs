//! Embeds, at build:
//!
//! - the menu lettering: every `data/lettering/U+XXXX.svg` becomes an entry of `LETTERS` in
//!   `$OUT_DIR/lettering.rs`, keyed by the character its name gives, in character order. A file in that
//!   directory not named `U+<hex>.svg` for one character fails the build, naming it;
//! - `QM_DOC_REV`, the commit this crate is built from, which the rule docs are linked at
//!   (`lint::DOC_BASE`): the `git.sha1` of `.cargo_vcs_info.json` in a packaged crate, else
//!   `git rev-parse HEAD` of the checkout. Neither fails the build: a qm whose rule links point at no
//!   particular docs would send modders to whatever the docs say today.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    lettering(&manifest);
    doc_rev(&manifest);
}

fn lettering(manifest: &Path) {
    let dir = manifest.join("data").join("lettering");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut letters: Vec<(char, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap_or_else(|e| panic!("{}: {e}", dir.display())).path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_else(|| panic!("{}: a file name that is not UTF-8", path.display()));
        let ch = name
            .strip_prefix("U+")
            .and_then(|n| n.strip_suffix(".svg"))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .and_then(char::from_u32)
            .unwrap_or_else(|| panic!("{}: not a letter file (U+<hex>.svg)", path.display()));
        letters.push((ch, path));
    }
    letters.sort();
    let mut out = String::from("/// The lettering's letters, in character order: each character's SVG.\npub const LETTERS: &[(char, &[u8])] = &[\n");
    for (ch, path) in &letters {
        out.push_str(&format!("    ({ch:?}, include_bytes!({:?})),\n", path.display().to_string()));
    }
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("lettering.rs");
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("{}: {e}", target.display()));
}

/// A 40-digit lowercase hex commit id, or `None`.
fn commit(s: &str) -> Option<String> {
    let s = s.trim();
    (s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_lowercase())
}

fn doc_rev(manifest: &Path) {
    let vcs = manifest.join(".cargo_vcs_info.json");
    println!("cargo:rerun-if-changed={}", vcs.display());
    let rev = if vcs.is_file() {
        // `{"git": {"sha1": "<40 hex>"}, ...}`, as `cargo package` writes it.
        let text = std::fs::read_to_string(&vcs).unwrap_or_else(|e| panic!("{}: {e}", vcs.display()));
        let after = text.split_once("\"sha1\"").map(|(_, rest)| rest).unwrap_or_else(|| panic!("{}: no git.sha1", vcs.display()));
        let value = after.split('"').nth(1).unwrap_or_else(|| panic!("{}: git.sha1 is not a string", vcs.display()));
        commit(value).unwrap_or_else(|| panic!("{}: git.sha1 {value:?} is not a commit id", vcs.display()))
    } else {
        let git = |args: &[&str]| -> String {
            let out = Command::new("git")
                .args(args)
                .current_dir(manifest)
                .output()
                .unwrap_or_else(|e| panic!("git {}: {e} (qm links its rule docs at the commit it is built from)", args.join(" ")));
            assert!(out.status.success(), "git {} in {}: {}", args.join(" "), manifest.display(), String::from_utf8_lossy(&out.stderr));
            String::from_utf8(out.stdout).expect("git prints UTF-8").trim().to_string()
        };
        // Rebuild when the checkout moves to another commit.
        for path in ["HEAD", "packed-refs"] {
            println!("cargo:rerun-if-changed={}", git(&["rev-parse", "--path-format=absolute", "--git-path", path]));
        }
        let head_ref = git(&["rev-parse", "--symbolic-full-name", "HEAD"]);
        if head_ref.starts_with("refs/") {
            println!("cargo:rerun-if-changed={}", git(&["rev-parse", "--path-format=absolute", "--git-path", &head_ref]));
        }
        let head = git(&["rev-parse", "HEAD"]);
        commit(&head).unwrap_or_else(|| panic!("git rev-parse HEAD printed {head:?}, not a commit id"))
    };
    println!("cargo:rustc-env=QM_DOC_REV={rev}");
}
