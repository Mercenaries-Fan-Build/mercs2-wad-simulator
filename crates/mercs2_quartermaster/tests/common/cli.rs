//! Running the `qm` binary as a subprocess, shared by `cli.rs` and `cli_retail.rs`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub const EXIT_FINDINGS: i32 = 1;

pub fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("qm-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

/// Write a manifest with the given contributions block.
pub fn shipment(dir: &Path, contributions: &str) -> PathBuf {
    std::fs::write(
        dir.join("manifest.yaml"),
        format!(
            "format: 2
shipment: {{ name: cli-test, version: 1.0.0, target: retail }}
contributions:
{contributions}"
        ),
    )
    .unwrap();
    dir.to_path_buf()
}

pub fn qm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qm"))
        .args(args)
        .output()
        .expect("qm must run")
}

pub fn code(out: &Output) -> i32 {
    out.status
        .code()
        .expect("qm must exit normally, not by signal")
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/load_plan")
}

pub fn fixture(rel: &str) -> String {
    fixtures().join(rel).to_string_lossy().into_owned()
}

pub fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

pub fn link(args: &[&str], out: &Path) -> Output {
    let mut all = vec!["link", "--out", out.to_str().unwrap()];
    all.extend_from_slice(args);
    qm(&all)
}
