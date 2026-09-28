//! The linker's hermetic tests: what it can be checked on without a game install.
//!
//! The tests that link against the retail scripts blocks — including the one that installs two
//! script mods and checks both survive — are game-gated and live in `link_retail.rs`.

mod common {
    pub mod corpus;
}

use common::corpus::corpus_root;
use mercs2_quartermaster::link;

#[test]
fn the_corpus_lookup_finds_a_vz_script_and_reports_what_it_tried() {
    let Some(corpus) = corpus_root() else { return };
    let found = link::base_source_path(&corpus, link::VZ_SOURCE_DIRS, "wifpmcinterior")
        .expect("wifpmcinterior is in vz/");
    assert!(
        found.ends_with("vz/wifpmcinterior.lua"),
        "{}",
        found.display()
    );

    let tried = link::base_source_path(&corpus, link::VZ_SOURCE_DIRS, "definitely_not_a_script").unwrap_err();
    assert!(
        tried.len() >= 3,
        "should report every location it searched: {tried:?}"
    );
}

/// The subfolders are searched in the order given, and only those: `mrxsound` is in both
/// `resident/` and `shell/`, and the `vz.wad` order finds the resident copy while the shell's finds
/// the shell copy; a `vz/` script is not found from `shell/`, and every path tried comes back.
#[test]
fn the_corpus_lookup_searches_the_given_subfolders_in_order() {
    let Some(corpus) = corpus_root() else { return };
    let vz = link::base_source_path(&corpus, link::VZ_SOURCE_DIRS, "mrxsound").expect("mrxsound for vz.wad");
    assert!(vz.ends_with("resident/mrxsound.lua"), "{}", vz.display());
    let shell = link::base_source_path(&corpus, link::SHELL_SOURCE_DIRS, "mrxsound").expect("mrxsound for shell.wad");
    assert!(shell.ends_with("shell/mrxsound.lua"), "{}", shell.display());
    let source = std::fs::read_to_string(&shell).unwrap();
    assert!(
        source.contains("function EnterShellState()") && source.contains("function ExitShellState()"),
        "the front end's trampoline host defines both functions it wraps"
    );

    let tried = link::base_source_path(&corpus, link::SHELL_SOURCE_DIRS, "wifpmcinterior").unwrap_err();
    assert!(tried[0].ends_with("shell/wifpmcinterior.lua"), "{tried:?}");
    assert!(!tried.iter().any(|p| p.ends_with("vz/wifpmcinterior.lua")), "{tried:?}");
}

/// The retail load sites the sound sessions are read from ([`sound::FRONT_END_SOUNDBANK_LOADS`],
/// [`sound::GAMEPLAY_SOUNDBANK_LOADS`]) are the literal `LoadSoundBank` calls of the corpus's
/// `MrxSound.EnterShellState` in `shell/` and `MrxSoundBootstrap.LoadBanks` in `resident/`, but the
/// `vo_*` banks, in order.
#[test]
fn the_sound_load_sites_are_the_corpus_calls() {
    use mercs2_quartermaster::sound;
    let Some(corpus) = corpus_root() else { return };
    let calls = |file: &str, function: &str| -> Vec<String> {
        let text = std::fs::read_to_string(corpus.join(file)).unwrap();
        let start = text.find(&format!("function {function}()")).unwrap_or_else(|| panic!("{function} in {file}"));
        let body = &text[start..start + text[start..].find("\nend\n").expect("the function ends")];
        body.lines()
            .filter_map(|l| l.trim().strip_prefix("MrxSoundBanks.LoadSoundBank(\""))
            .map(|rest| rest[..rest.find('"').unwrap()].to_string())
            .filter(|b| !sound::is_vo_bank(b))
            .collect()
    };
    assert_eq!(calls("shell/mrxsound.lua", "EnterShellState"), sound::FRONT_END_SOUNDBANK_LOADS);
    assert_eq!(calls("resident/mrxsoundbootstrap.lua", "LoadBanks"), sound::GAMEPLAY_SOUNDBANK_LOADS);

    // No other shell script loads a bank: `MrxSoundBanks` itself is the call path, `MrxSound` the
    // one caller.
    let mut scripts = 0;
    for entry in std::fs::read_dir(corpus.join("shell")).unwrap() {
        let path = entry.unwrap().path();
        scripts += 1;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == "mrxsound.lua" || name == "mrxsoundbanks.lua" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for call in ["LoadSoundBank(", "LoadWaveBank(", "LoadTempBank(", "LoadBankWithCallback(", "Sound.LoadBank("] {
            assert!(!text.contains(call), "{name} calls {call}");
        }
    }
    assert_eq!(scripts, 28, "the shell's 28 scripts");
}
