//! End-to-end checks of `mercs2_formats::be_to_le::audio::convert_wavebank_data`: the retail Xbox and
//! PS3 wavebank fixtures must convert into a PC body that this crate's `wave::WavebankFile::parse`
//! accepts, with the clips' sample counts preserved and non-silent PCM16 payloads from ffmpeg.
//!
//! They live here rather than in `mercs2_formats` because they need this crate's PC parser, and
//! `mercs2_formats` cannot depend on `mercs2_audio` (which depends on it). The fixtures are
//! `mercs2_formats`' own (`tests/fixtures/`), checked by sha256 before use.

use mercs2_formats::be_to_le::audio::{convert_wavebank_data, find_ffmpeg};
use sha2::{Digest, Sha256};

const RETAIL_FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../mercs2_formats/tests/fixtures/xbox_wavebank_emb_block3322_be.bin"
));
const RETAIL_FIXTURE_SHA256: &str = "a278e393501d254548a7245c401449574c1c16e8c3b032e0a568bd0eaf34d00a";

const RETAIL_FIXTURE_PS3: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../mercs2_formats/tests/fixtures/ps3_wavebank_emb_block4243_be.bin"
));
const RETAIL_FIXTURE_PS3_SHA256: &str = "e9d8967c91c953700a327d9e307eacf7b994cc5f093ebd6b7937cae075fc66a1";

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}

fn assert_fixture_provenance() {
    assert_eq!(sha256_hex(RETAIL_FIXTURE), RETAIL_FIXTURE_SHA256);
    assert_eq!(RETAIL_FIXTURE.len(), 0x8000);
}

fn assert_fixture_provenance_ps3() {
    assert_eq!(sha256_hex(RETAIL_FIXTURE_PS3), RETAIL_FIXTURE_PS3_SHA256);
    assert_eq!(RETAIL_FIXTURE_PS3.len(), 0x6F80);
}

/// End-to-end: `convert_wavebank_data` on the retail Xbox fixture must
/// produce a PC body that `mercs2_audio::wave::WavebankFile::parse`
/// accepts, with the two clips' sample counts and sample rates
/// preserved and non-silent PCM16 payloads from ffmpeg.
#[test]
fn retail_wavebank_block_3322_roundtrips_through_pc_parser() {
    assert_fixture_provenance();
    if find_ffmpeg().is_err() {
        panic!(
            "ffmpeg not resolvable — set MERCS2_FFMPEG or install under tools/ffmpeg/bin/"
        );
    }

    let pc = convert_wavebank_data(RETAIL_FIXTURE).expect("Xbox→PC convert succeeds");

    assert_eq!(pc.len() % mercs2_audio::wave::BLOB_ALIGN, 0);
    assert_eq!(
        u32::from_le_bytes(pc[0..4].try_into().unwrap()),
        mercs2_audio::wave::TABLE_VERSION
    );
    assert_eq!(
        u32::from_le_bytes(pc[4..8].try_into().unwrap()),
        0x9996_B5A6
    );
    assert_eq!(u16::from_le_bytes(pc[8..10].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(pc[10..12].try_into().unwrap()), 0);

    let file = mercs2_audio::wave::WavebankFile::parse(&pc).expect("PC parse accepts output");
    assert_eq!(file.bank_hash, 0x9996_B5A6);
    assert_eq!(file.records.len(), 2);

    let expected = [
        (0x8E16_4121u32, 55_296u32),
        (0x0C0E_B8B6u32, 51_968u32),
    ];
    for (rec, (xclip, xframes)) in file.records.iter().zip(expected.iter()) {
        assert_eq!(rec.clip_hash, *xclip);
        assert_eq!(rec.channels, 1);
        assert_eq!(rec.format, mercs2_audio::wave::BYTES_PER_SAMPLE_PCM16);
        assert_eq!(rec.sample_rate, 44100);
        assert_eq!(rec.frames, *xframes);
        let bytes = match &rec.data {
            mercs2_audio::wave::WaveData::Embedded(b) => b,
            mercs2_audio::wave::WaveData::Streamed { .. } => {
                panic!("embedded fixture produced a streamed record")
            }
        };
        assert_eq!(bytes.len(), *xframes as usize * 2);
        assert!(
            bytes.iter().any(|&b| b != 0),
            "ffmpeg-decoded PCM16 should not be all-zero"
        );
    }
}

/// End-to-end: `convert_wavebank_data` on the retail PS3 fixture must
/// produce a PC body that `mercs2_audio::wave::WavebankFile::parse`
/// accepts, with the two clips’ sample counts preserved and
/// non-silent PCM16 payloads from ffmpeg’s native MP3 decoder.
#[test]
fn retail_ps3_wavebank_block_4243_roundtrips_through_pc_parser() {
    assert_fixture_provenance_ps3();
    if find_ffmpeg().is_err() {
        panic!(
            "ffmpeg not resolvable — set MERCS2_FFMPEG or install under tools/ffmpeg/bin/"
        );
    }

    let pc = convert_wavebank_data(RETAIL_FIXTURE_PS3).expect("PS3→PC convert succeeds");

    assert_eq!(pc.len() % mercs2_audio::wave::BLOB_ALIGN, 0);
    assert_eq!(
        u32::from_le_bytes(pc[0..4].try_into().unwrap()),
        mercs2_audio::wave::TABLE_VERSION
    );
    assert_eq!(
        u32::from_le_bytes(pc[4..8].try_into().unwrap()),
        0x9996_B5A6
    );
    assert_eq!(u16::from_le_bytes(pc[8..10].try_into().unwrap()), 2);
    assert_eq!(u16::from_le_bytes(pc[10..12].try_into().unwrap()), 0);

    let file = mercs2_audio::wave::WavebankFile::parse(&pc).expect("PC parse accepts output");
    assert_eq!(file.bank_hash, 0x9996_B5A6);
    assert_eq!(file.records.len(), 2);

    let expected = [
        (0x8E16_4121u32, 56_448u32),
        (0x0C0E_B8B6u32, 52_992u32),
    ];
    for (rec, (xclip, xframes)) in file.records.iter().zip(expected.iter()) {
        assert_eq!(rec.clip_hash, *xclip);
        assert_eq!(rec.channels, 1);
        assert_eq!(rec.format, mercs2_audio::wave::BYTES_PER_SAMPLE_PCM16);
        assert_eq!(rec.sample_rate, 44100);
        assert_eq!(rec.frames, *xframes);
        let bytes = match &rec.data {
            mercs2_audio::wave::WaveData::Embedded(b) => b,
            mercs2_audio::wave::WaveData::Streamed { .. } => {
                panic!("embedded fixture produced a streamed record")
            }
        };
        assert_eq!(bytes.len(), *xframes as usize * 2);
        assert!(
            bytes.iter().any(|&b| b != 0),
            "ffmpeg-decoded PCM16 should not be all-zero"
        );
    }
}
