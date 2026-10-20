//! `aset_target_crack` must search 3-word bodies when no `--anchor` is given.
//!
//! The unanchored depth-3 shape has three free slots; the slot filler once carried only two words
//! and every such run panicked. `--words-only` reads no names file, so this needs no game data.

use std::process::Command;

#[test]
fn unanchored_depth3_finds_a_three_word_name() {
    // pandemic_hash_m2("al_hum_soldier_elitea_ub") == 0x037CDC9B: prefix `al_hum` + three body words.
    let out = Command::new(env!("CARGO_BIN_EXE_aset_target_crack"))
        .args(["--target", "0x037CDC9B", "--prefix", "al_hum", "--words-only", "--depth", "3"])
        .args(["--word", "soldier", "--word", "elitea", "--word", "ub", "--word", "head"])
        .output()
        .expect("run aset_target_crack");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "aset_target_crack exited {:?}:\n{stderr}", out.status.code());
    assert!(
        stderr.contains("HIT  0x037CDC9B  al_hum_soldier_elitea_ub"),
        "no hit for the known 3-word name:\n{stderr}"
    );
}
