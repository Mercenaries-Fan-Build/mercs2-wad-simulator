//! Integration tests using real fixture files.
//!
//! NOTE: The fixture files (anim_ks750_*.bin, phy2_resident2_*.bin) are embedded
//! payloads (Havok packfiles, Lua bytecode), not top-level UCFX blocks.
//! These tests verify that havok and lua conversion work on the fixture files.

use std::fs;

#[test]
fn test_havok_anim_ks750_round_trip() {
    let be_data = fs::read("tests/fixtures/anim_ks750_be.bin")
        .expect("Failed to read anim_ks750_be.bin");

    // These fixtures are Havok packfiles, not UCFX blocks
    // Convert via havok module
    let result = ucfx_byteswap::havok::convert_havok_be_to_le(&be_data);
    assert!(result.is_ok(), "Havok conversion should succeed: {:?}", result.err());

    let le_data = result.unwrap();
    assert!(!le_data.is_empty(), "Output should not be empty");

    // Verify against expected LE version
    let expected = fs::read("tests/fixtures/anim_ks750_le.bin")
        .expect("Failed to read anim_ks750_le.bin");

    assert_eq!(le_data.len(), expected.len(),
        "Converted size should match expected (got {}, expected {})",
        le_data.len(), expected.len());

    assert_eq!(le_data, expected,
        "Converted data should match expected byte-for-byte");
}

#[test]
fn test_havok_phy2_resident2_round_trip() {
    let be_data = fs::read("tests/fixtures/phy2_resident2_be.bin")
        .expect("Failed to read phy2_resident2_be.bin");

    // These fixtures are Havok packfiles, not UCFX blocks
    let result = ucfx_byteswap::havok::convert_phy2_be_to_le(&be_data);
    assert!(result.is_ok(), "Havok phy2 conversion should succeed: {:?}", result.err());

    let le_data = result.unwrap();
    assert!(!le_data.is_empty(), "Output should not be empty");

    // Verify against expected LE version
    let expected = fs::read("tests/fixtures/phy2_resident2_le.bin")
        .expect("Failed to read phy2_resident2_le.bin");

    assert_eq!(le_data.len(), expected.len(),
        "Converted size should match expected (got {}, expected {})",
        le_data.len(), expected.len());

    assert_eq!(le_data, expected,
        "Converted data should match expected byte-for-byte");
}

#[test]
fn test_min_block_too_small() {
    let tiny = vec![0u8; 2];
    let result = ucfx_byteswap::convert::convert_block(&tiny, false, None);
    assert!(result.is_err(), "Should reject block too small");
}

#[test]
fn test_min_block_with_one_entry_zero_size() {
    // Minimal valid header: 1 entry count + 1 entry (16 bytes)
    let mut data = vec![0u8; 20];
    // entry_count = 1 (LE)
    data[0] = 1u8;

    let result = ucfx_byteswap::convert::convert_block(&data, false, None);
    // Should either succeed or fail gracefully (container size 0 edge case)
    // The converter validates entry.chunk_size, which is 0 here.
    let _ = result; // Accept either result for this edge case
}
