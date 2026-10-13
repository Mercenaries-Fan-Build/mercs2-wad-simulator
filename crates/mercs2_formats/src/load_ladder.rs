//! The world-load ladder behind m2-sdk.dll's `m2_loadtrigger` (`m2/m2_loadtrigger.h`): loadprobe's
//! `phases::LADDER`, the table the game-log scorer uses, so the DLL and loadprobe agree on every phase
//! index. [`c_header`] is the text of the C mods' `m2/load_ladder.gen.h`, which mercs2_mod_runtime's build
//! writes beside the import library.

use loadprobe::phases::{ENTERED_WORLD_IDX, LADDER, REACHED_WORLD_IDX};

pub fn count() -> usize {
    LADDER.len()
}

/// The phase's name, or `None` for an index outside the ladder.
pub fn phase_name(idx: i32) -> Option<&'static str> {
    LADDER.iter().find(|p| i32::try_from(p.idx).ok() == Some(idx)).map(|p| p.name)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The highest phase whose marker appears in `msg` (case-sensitive substring), as loadprobe scores a line.
pub fn match_phase(msg: &[u8]) -> Option<usize> {
    LADDER.iter().filter(|p| p.matches.iter().any(|m| contains(msg, m.as_bytes()))).map(|p| p.idx).max()
}

pub const REACHED_WORLD: usize = REACHED_WORLD_IDX;
pub const ENTERED_WORLD: usize = ENTERED_WORLD_IDX;

/// `m2/load_ladder.gen.h`.
pub fn c_header() -> String {
    format!(
        "/* Generated from loadprobe::phases::LADDER by mercs2_mod_runtime's build, which writes it beside
 * libm2-sdk.dll.a. Do not edit: the m2-sdk Shipment's build fails when this copy differs. */
#ifndef M2_LOAD_LADDER_GEN_H
#define M2_LOAD_LADDER_GEN_H

#define M2_LADDER_COUNT {count}
#define M2_PHASE_REACHED_WORLD_IDX {REACHED_WORLD_IDX}
#define M2_PHASE_ENTERED_WORLD_IDX {ENTERED_WORLD_IDX}

typedef struct {{
    int idx;
    const char* name;
    const char* const* matches;
    int match_count;
}} M2LoadPhase;

/* The table itself lives in the DLL and is not exported. Mods use the M2_PHASE_* constants above and
 * m2_loadtrigger_phase_name. */

#endif /* M2_LOAD_LADDER_GEN_H */
",
        count = LADDER.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_indices_are_their_positions() {
        for (i, p) in LADDER.iter().enumerate() {
            assert_eq!(p.idx, i);
        }
    }

    #[test]
    fn names_and_matching() {
        assert_eq!(count(), 21);
        assert_eq!(phase_name(0), Some("Process init"));
        assert_eq!(phase_name(20), Some("World fully loaded (GlobalExit)"));
        assert_eq!(phase_name(21), None);
        assert_eq!(phase_name(-1), None);
        assert_eq!(match_phase(b"[lua] GlobalExit - Complete"), Some(20));
        assert_eq!(match_phase(b"Playing EA logo"), Some(4));
        assert_eq!(match_phase(b"globalexit - complete"), None);
        assert_eq!(match_phase(b"nothing here"), None);
        assert_eq!(match_phase(b"CreatePlayerCharacter then GlobalEnter - Begin"), Some(13));
    }

    #[test]
    fn the_header_carries_the_ladder_constants() {
        let h = c_header();
        assert!(h.contains("#define M2_LADDER_COUNT 21\n"));
        assert!(h.contains("#define M2_PHASE_REACHED_WORLD_IDX 20\n"));
        assert!(h.contains("#define M2_PHASE_ENTERED_WORLD_IDX 11\n"));
    }
}
