//! A character kit renders as one whole outfit, close-up geometry included.
//!
//! The allied elite kit `0x4D4F42B5` keeps its close-up (rung 0, `lod_mask 0x01`) geometry in a
//! finer `_P001` block, numbered from 0 again. `--render`'s draw-count gate used to be read from the
//! resident block alone, so every finer-block group past the resident's count was dropped and only
//! the heads rendered. These run the real binary over the retail data.
//!
//! Game-gated: built by the `retail` feature (`cargo xtask retail-test`), reads the retail vz.wad
//! named by the repo-root `.mercs2-local.toml`, and fails if it is absent.

use std::process::Command;

const ELITE_KIT: &str = "0x4D4F42B5";

fn retail_wad() -> String {
    let path = mercs2_formats::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(path.is_file(), "{} is not a file", path.display());
    path.to_string_lossy().into_owned()
}

fn workshop(args: &[&str]) -> String {
    let wad = retail_wad();
    let out = Command::new(env!("CARGO_BIN_EXE_mercs2_workshop"))
        .args(["--wad", &wad, "--no-auto-patch"])
        .args(args)
        .output()
        .expect("run mercs2_workshop");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "mercs2_workshop {args:?} exited {:?}\nstdout:\n{stdout}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    stdout
}

#[test]
fn jungle_elite_renders_its_close_up_body() {
    let dir = std::env::temp_dir().join("mercs2_outfit_render_test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let out = dir.join("jungle0");
    let stdout = workshop(&[
        "--render",
        ELITE_KIT,
        "--render-outfit",
        "Jungle Elite#0",
        "--render-out",
        &out.to_string_lossy(),
    ]);
    assert!(stdout.contains("parts [3, 5, 7, 8, 10, 12, 13]"), "{stdout}");
    assert!(stdout.contains("drawing 13 of 87 groups"), "{stdout}");
    // The upper body (node 71) and lower body (node 69) at rung 0 — finer-block groups.
    for node in [71, 69] {
        let drawn = stdout
            .lines()
            .any(|l| l.contains("lod 0x01") && l.contains(&format!("node {node:>4}")) && !l.contains("GATED"));
        assert!(drawn, "node {node} is not drawn at lod 0x01:\n{stdout}");
    }
}

#[test]
fn elite_kit_lists_the_exe_outfits() {
    let stdout = workshop(&["--render", ELITE_KIT, "--render-outfits"]);
    assert!(stdout.contains("\"Jungle Elite\": profile 0 (0x351013C5), 4 outfit(s)"), "{stdout}");
    assert!(stdout.contains("\"Night Vision Elite\": profile 1 (0xF79D3014), 20 outfit(s)"), "{stdout}");
}
