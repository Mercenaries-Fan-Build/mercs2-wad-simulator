//! Integration tests for the Xbox-DOH side-oracle flow of `dlc_port`.
//!
//! These tests exercise the oracle's resolution contract end-to-end without
//! pulling in a full retail WAD: synthetic `OracleBlock` fixtures feed the
//! `XboxDohOracle::from_blocks` probe constructor (byte-identical dispatch
//! semantics to the real `load` path), and the real `convert_block` error-
//! classification helper (`is_ps3_compact_decl_rejection`) is driven against
//! the exact error string the live `convert_decl_ps3_compact` arm emits for
//! a known PS3 DLC01 descriptor.
//!
//! What is NOT in-scope here: composing a complete BE SCFF + Xbox DOH pair
//! on disk and running the `dlc_port` binary against them. Doing that
//! correctly would require re-emitting a valid FFCS header, INDX / ASET /
//! PTHS tables, segs-compressed BE UCFX bodies and a crafted PS3 compact
//! `decl` descriptor whose primary name_hash has a matching Xbox-DOH
//! counterpart. That build-up is one order of magnitude more code than the
//! dispatch logic it would cover. The real retail end-to-end run is
//! documented in `docs/_dlc_port_xbox_doh_side_oracle.md` and in the
//! scratchpad under `dlc_port_side_oracle/`.

use std::path::PathBuf;

use dlc_port::xbox_doh_oracle::{
    is_ps3_compact_decl_rejection, primary_name_hash_be, OracleBlock, XboxDohOracle,
    PS3_COMPACT_DECL_MARKER,
};

/// The exact `convert_decl_ps3_compact` output for one terrain descriptor
/// from the PS3 DLC01 SCFF (`03 00 00 03 04 06 03 04 06 0A 02 01`,
/// stride 14 ground). This string is what the real `convert_block` returns
/// to `dlc_port` for every one of the 170 terrain PS3-compact-decl
/// rejections; anchoring the test on it locks the dispatcher's classifier
/// against future reword drift.
const LIVE_TERRAIN_REJECTION: &str = "convert_block: PS3 compact decl detected (bytes=`0300000304060304060a0201`, \
     terrain stride-14 ground (xbox-oracle: COLOR@8 D3DCOLOR + NORMAL@12 F16x4)): \
     the PS3 DLC ships a non-interleaved multi-stream vertex layout; one PS3 STRM group / `decl` describes \
     ONE attribute slice. The PC engine requires a single-stream interleaved \
     D3DVERTEXELEMENT9 array.";

/// A one-mesh PS3 compact rejection: `03 00 08 02` is the shared mesh/foliage
/// stride-4 pattern that appears 4,728 times in the DLC01 corpus.
const LIVE_MESH_REJECTION: &str = "convert_block: PS3 compact decl detected (bytes=`03000802`, \
     mesh/foliage compact stride-4 (ambiguous: maps to 10 Xbox decls)): \
     the PS3 DLC ships a non-interleaved multi-stream vertex layout;";

fn oracle_block_for(name_hash: u32, type_hash: u32, path: &str, le_body_len: usize) -> OracleBlock {
    // The LE body is opaque to the oracle: the dispatcher clones it verbatim
    // into the output PatchBlock. Any non-zero length that round-trips
    // through sges is sufficient to prove the hand-off.
    OracleBlock {
        primary_name_hash: name_hash,
        primary_type_hash: type_hash,
        xbox_path: path.to_string(),
        le_decompressed: vec![0xA5; le_body_len],
    }
}

#[test]
fn ps3_compact_decl_marker_is_the_live_error_phrase() {
    // Belt-and-braces: fail fast if the classifier phrase drifts away from
    // the live error text the walker actually emits. If this test ever
    // flips red, update xbox_doh_oracle::PS3_COMPACT_DECL_MARKER AND the
    // scope note in docs/_dlc_port_xbox_doh_side_oracle.md at the same time.
    assert!(LIVE_TERRAIN_REJECTION.contains(PS3_COMPACT_DECL_MARKER));
    assert!(LIVE_MESH_REJECTION.contains(PS3_COMPACT_DECL_MARKER));
    assert!(is_ps3_compact_decl_rejection(LIVE_TERRAIN_REJECTION));
    assert!(is_ps3_compact_decl_rejection(LIVE_MESH_REJECTION));
}

#[test]
fn other_convert_block_rejections_are_left_on_the_skip_path() {
    // Shared with the Xbox DOH input (docs/_dlc01_pipeline_readiness.md §4):
    // these 8 blocks fail on both platforms. The oracle must NOT claim them.
    assert!(!is_ps3_compact_decl_rejection(
        "convert_block: wavebank transcode: wavebank records_offset 56, expected 40"
    ));
    assert!(!is_ps3_compact_decl_rejection(
        "convert_block: unluac.jar not found"
    ));
    assert!(!is_ps3_compact_decl_rejection("Block too small"));
    assert!(!is_ps3_compact_decl_rejection(
        "Entry 42 container exceeds block (offset=1234, size=567)"
    ));
}

#[test]
fn lookup_resolves_the_ps3_primary_name_hash_to_the_xbox_doh_body() {
    // A composed Xbox DOH holding three blocks keyed by distinct primary
    // name_hashes. The PS3 side rejects a block whose primary name_hash
    // matches the middle Xbox entry; the dispatcher must find it.
    let oracle = XboxDohOracle::from_blocks(
        PathBuf::from("test_fixture_xbox.doh"),
        vec![
            oracle_block_for(
                0x1111_1111,
                0x5B72_4250, // model
                "blocks\\dlc01\\first_P000_Q3.block",
                256,
            ),
            oracle_block_for(
                0x2222_2222,
                0x7C56_9307, // terrain
                "blocks\\dlc01\\dlc01_terrain_r00_c00_P000_Q3.block",
                1024,
            ),
            oracle_block_for(
                0x3333_3333,
                0x600B_904E, // foliage
                "blocks\\dlc01\\dlc01_caicara_foliage_P000_Q3.block",
                512,
            ),
        ],
    );

    assert_eq!(oracle.indexed_count(), 3);

    let hit = oracle
        .lookup(0x2222_2222)
        .expect("terrain primary name_hash must resolve against the oracle");
    assert_eq!(hit.primary_name_hash, 0x2222_2222);
    assert_eq!(hit.primary_type_hash, 0x7C56_9307);
    assert_eq!(
        hit.xbox_path,
        "blocks\\dlc01\\dlc01_terrain_r00_c00_P000_Q3.block"
    );
    assert_eq!(hit.le_decompressed.len(), 1024);
    // The dispatcher clones `le_decompressed` into the output PatchBlock
    // verbatim — verify the sentinel byte is preserved so a future refactor
    // cannot slip in a mutation.
    assert!(hit.le_decompressed.iter().all(|&b| b == 0xA5));
}

#[test]
fn lookup_miss_returns_none_so_the_dispatcher_can_fail_the_run() {
    let oracle = XboxDohOracle::from_blocks(
        PathBuf::from("test_fixture_xbox.doh"),
        vec![oracle_block_for(
            0xAAAA_AAAA,
            0x5B72_4250,
            "blocks\\dlc01\\only_P000_Q3.block",
            64,
        )],
    );
    // A hash the oracle does not know about yields `None`. The dispatcher
    // in main.rs turns a `None` into a whole-run `Err(...)` with the hash,
    // the PS3 path and the Xbox DOH path. The whole point of the loud-fail
    // contract is that this `None` NEVER becomes a silent drop.
    assert!(oracle.lookup(0xBBBB_BBBB).is_none());
}

#[test]
fn primary_name_hash_be_recovers_the_lookup_key_from_a_ps3_block_body() {
    // Construct a minimal BE UCFX block body the way `convert_block` reads
    // it: entry_count (BE u32) at offset 0, then one entry (16 B) whose
    // first field is the BE name_hash. This is exactly the slice the
    // dispatcher hands to `primary_name_hash_be` after `decompress_be_sges`
    // produces the raw BE bytes.
    let mut be_block = Vec::new();
    be_block.extend_from_slice(&1u32.to_be_bytes()); // entry_count = 1
    be_block.extend_from_slice(&0xCAFE_F00Du32.to_be_bytes()); // name_hash
    be_block.extend_from_slice(&0x5B72_4250u32.to_be_bytes()); // type_hash
    be_block.extend_from_slice(&0u32.to_be_bytes()); // field_c
    be_block.extend_from_slice(&0u32.to_be_bytes()); // chunk_size

    assert_eq!(primary_name_hash_be(&be_block), Some(0xCAFE_F00D));
}

#[test]
fn primary_name_hash_be_refuses_an_undersized_be_body() {
    // 19 bytes — one short of the 4-byte header + 16-byte entry minimum
    // `convert_block` reads. The dispatcher must not fabricate a hash from
    // a truncated body: it fails loud instead.
    assert!(primary_name_hash_be(&vec![0u8; 19]).is_none());
}

#[test]
fn first_wins_on_a_duplicate_primary_name_hash_in_the_xbox_doh() {
    // A legitimate Xbox DOH ships zero duplicates on the primary slot, but
    // the oracle must still be deterministic if a bad container is handed
    // in. First-wins mirrors the engine's own name-registry semantics
    // (`FUN_00826050` first-writer-wins, see
    // memory/no-arbitrary-hashes.md).
    let oracle = XboxDohOracle::from_blocks(
        PathBuf::from("test_fixture_xbox.doh"),
        vec![
            oracle_block_for(0xC0DE_BEEF, 0x5B72_4250, "blocks\\dlc01\\first.block", 64),
            oracle_block_for(
                0xC0DE_BEEF,
                0x5B72_4250,
                "blocks\\dlc01\\second_but_dropped.block",
                128,
            ),
        ],
    );
    assert_eq!(oracle.indexed_count(), 1);
    let hit = oracle.lookup(0xC0DE_BEEF).unwrap();
    assert_eq!(hit.xbox_path, "blocks\\dlc01\\first.block");
    assert_eq!(hit.le_decompressed.len(), 64);
}

#[test]
fn blocks_with_zero_primary_name_hash_are_unindexable() {
    // `name_hash == 0` is the empty-string hash and must never be the key:
    // every un-named block would otherwise collide onto one entry.
    let oracle = XboxDohOracle::from_blocks(
        PathBuf::from("test_fixture_xbox.doh"),
        vec![
            oracle_block_for(0x0000_0000, 0, "blocks\\dlc01\\zero_hash.block", 32),
            oracle_block_for(0x1234_5678, 0x5B72_4250, "blocks\\dlc01\\valid.block", 32),
        ],
    );
    assert_eq!(oracle.indexed_count(), 1);
    assert!(oracle.lookup(0x1234_5678).is_some());
    assert!(oracle.lookup(0x0000_0000).is_none());
}
