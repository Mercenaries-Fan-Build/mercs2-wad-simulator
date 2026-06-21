//! Engine-accurate soundbank consumption (LoadSoundBank path).

use mercs2_formats::safe_slice::{AccessResult, SafeSlice};

pub const HEADER_SIZE: usize = 32;

/// u8x4 flag offsets relative to each record start in section A.
/// Derived from cross-platform evidence (stride-116 banks): offsets where
/// xbox bytes == pc bytes (endian-invariant).
/// NOTE: Per-stride evidence shows these vary by stride. This is the
/// stride-116 set matching the Python production converter.
pub const U8X4_RECORD_RELATIVE: &[usize] = &[12, 20, 44];

#[derive(Debug)]
pub struct LoadedSoundbank {
    pub self_hash: u32,
    pub sub_count: u16,
    pub sub_count2: u16,
    pub section_b_hashes: Vec<u32>,
    pub section_d_hashes: Vec<u32>,
    pub unresolved_hashes: Vec<u32>,
    pub issues: Vec<String>,
}

pub fn consume_soundbank(
    body: &SafeSlice,
    resolve_hash: &dyn Fn(u32) -> bool,
) -> AccessResult<LoadedSoundbank> {
    let mut issues = Vec::new();

    if body.len() < HEADER_SIZE {
        issues.push(format!("body too short: {}", body.len()));
    }

    let self_hash = body.read_u32_le(4, "self_hash")?;
    let sub_count = body.read_u16_le(8, "sub_count")?;
    let sub_count2 = body.read_u16_le(10, "sub_count2")?;
    let data_start = body.read_u32_le(16, "data_start")? as usize;
    let section_off1 = body.read_u32_le(20, "section_off1")? as usize;
    let section_off2 = body.read_u32_le(24, "section_off2")? as usize;
    let section_off3 = body.read_u32_le(28, "section_off3")? as usize;

    // P2-7: sub_count upper bound
    if sub_count > 1024 {
        issues.push(format!("sub_count {sub_count} exceeds upper bound 1024"));
    }

    // P2-7: Section A size > 0 when sub_count > 0
    if sub_count > 0 && section_off1 <= data_start {
        issues.push(format!(
            "section_a size is 0 (data_start={data_start}, section_off1={section_off1}) but sub_count={sub_count}"
        ));
    }

    if !(data_start <= section_off1 && section_off1 <= section_off2 && section_off2 <= section_off3) {
        issues.push(format!(
            "section offsets not monotonic: {data_start} {section_off1} {section_off2} {section_off3}"
        ));
    }
    if section_off3 > body.len() {
        issues.push(format!("section_off3 0x{section_off3:X} > body 0x{:X}", body.len()));
    }

    // Section A — exercise ALL records via SafeSlice
    if sub_count > 0 && data_start > 0 && section_off1 > data_start {
        let _sec_a = body.subslice(data_start, section_off1, "section_a")?;
        if (section_off1 - data_start) % sub_count as usize == 0 {
            let record_size = (section_off1 - data_start) / sub_count as usize;

            // P2-7: record stride sanity
            if record_size < 48 {
                issues.push(format!(
                    "section_a record stride {record_size} < 48 (suspiciously small)"
                ));
            } else if record_size != 116 && record_size != 118 {
                issues.push(format!(
                    "section_a record stride {record_size} not in known set {{116, 118}}"
                ));
            }
            for r in 0..sub_count as usize {
                let rec_off = data_start + r * record_size;
                for &fo in U8X4_RECORD_RELATIVE {
                    if fo + 4 <= record_size {
                        let _ = body.read_u8(rec_off + fo, &format!("rec[{r}].u8x4+{fo}"))?;
                    }
                }
                if record_size >= 4 {
                    let h = body.read_u32_le(rec_off, &format!("rec[{r}].hash0"))?;
                    if h > 0x1000_0000 && h != 0xFFFF_FFFF {
                        if !resolve_hash(h) {
                            // tracked below via section tables
                        }
                    }
                }
            }
        }
    }

    // Section B — index table (pointer-like u32s)
    let mut section_b_hashes = Vec::new();
    if section_off2 > section_off1 && sub_count > 0 {
        let sec_b = body.subslice(section_off1, section_off2, "section_b")?;
        let n = sub_count as usize;
        for i in 0..n {
            let off = i * 4;
            if off + 4 > sec_b.len() {
                break;
            }
            let v = sec_b.read_u32_le(off, &format!("index_b[{i}]"))?;
            if v > 0x1000_0000 && v != 0xFFFF_FFFF {
                section_b_hashes.push(v);
            }
        }
    }

    // Section C — exercise record area (different layout from A; no u8x4 protection)
    if section_off3 > section_off2 && sub_count2 > 0 {
        let _sec_c = body.subslice(section_off2, section_off3, "section_c")?;
        let sec_c_size = section_off3 - section_off2;
        if sec_c_size % sub_count2 as usize == 0 {
            let record_size_c = sec_c_size / sub_count2 as usize;
            for r in 0..sub_count2 as usize {
                let rec_off = section_off2 + r * record_size_c;
                if rec_off + 4 <= body.len() {
                    let _ = body.read_u32_le(rec_off, &format!("sec_c[{r}].word0"))?;
                }
            }
        }
    }

    // Section D
    let mut section_d_hashes = Vec::new();
    if body.len() > section_off3 && sub_count2 > 0 {
        let sec_d = body.subslice(section_off3, body.len(), "section_d")?;
        let n = sub_count2 as usize;
        for i in 0..n {
            let off = i * 4;
            if off + 4 > sec_d.len() {
                break;
            }
            let v = sec_d.read_u32_le(off, &format!("index_d[{i}]"))?;
            if v > 0x1000_0000 && v != 0xFFFF_FFFF {
                section_d_hashes.push(v);
            }
        }
    }

    let mut unresolved_hashes = Vec::new();
    for h in section_b_hashes.iter().chain(section_d_hashes.iter()) {
        if !resolve_hash(*h) {
            unresolved_hashes.push(*h);
        }
    }

    if !unresolved_hashes.is_empty() {
        issues.push(format!(
            "unresolved wavebank clip hashes: {}",
            unresolved_hashes
                .iter()
                .map(|h| format!("0x{h:08X}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    Ok(LoadedSoundbank {
        self_hash,
        sub_count,
        sub_count2,
        section_b_hashes,
        section_d_hashes,
        unresolved_hashes,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn golden_dir() -> PathBuf {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("..");
        p.push("testdata");
        p.push("audio_endian");
        p
    }

    fn load_golden_pc(hash_hex: &str) -> Vec<u8> {
        let path = golden_dir().join(format!("soundbank_{hash_hex}_pc.bin"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
    }

    fn always_resolve(_h: u32) -> bool {
        true
    }

    #[test]
    #[ignore]  // Requires golden test files
    fn consume_golden_4b8ab553_no_panic() {
        let data = load_golden_pc("4B8AB553");
        let slice = SafeSlice::new(data, "soundbank_4B8AB553");
        let result = consume_soundbank(&slice, &always_resolve);
        assert!(result.is_ok(), "consume_soundbank failed: {:?}", result.err());
        let sb = result.unwrap();
        assert_eq!(sb.sub_count, 9);
    }

    #[test]
    #[ignore]  // Requires golden test files
    fn consume_golden_84701c9a_stride_116() {
        let data = load_golden_pc("84701C9A");
        let slice = SafeSlice::new(data, "soundbank_84701C9A");
        let result = consume_soundbank(&slice, &always_resolve);
        assert!(result.is_ok());
        let sb = result.unwrap();
        assert_eq!(sb.sub_count, 12);
    }

    #[test]
    #[ignore]  // Requires golden test files
    fn consume_golden_c1bdeead_stride_118() {
        let data = load_golden_pc("C1BDEEAD");
        let slice = SafeSlice::new(data, "soundbank_C1BDEEAD");
        let result = consume_soundbank(&slice, &always_resolve);
        assert!(result.is_ok());
        let sb = result.unwrap();
        assert_eq!(sb.sub_count, 24);
    }

    #[test]
    fn u8x4_offsets_are_record_relative() {
        // Verify the constant contains relative offsets < 48
        for &off in U8X4_RECORD_RELATIVE {
            assert!(off < 48, "U8X4_RECORD_RELATIVE contains {off} >= 48");
            assert_eq!(off % 4, 0, "U8X4_RECORD_RELATIVE {off} not 4-aligned");
        }
    }

    #[test]
    #[ignore]  // Requires golden test files
    fn u8x4_fields_are_endian_invariant() {
        let pc = load_golden_pc("4B8AB553");
        let xbox_path = golden_dir().join("soundbank_4B8AB553_xbox.bin");
        let xbox = std::fs::read(&xbox_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", xbox_path.display()));

        let slice_pc = SafeSlice::new(pc.clone(), "pc");
        let data_start = slice_pc.read_u32_le(16, "ds").unwrap() as usize;
        let section_off1 = slice_pc.read_u32_le(20, "off1").unwrap() as usize;
        let sub_count = slice_pc.read_u16_le(8, "sc").unwrap() as usize;

        let sec_a = section_off1 - data_start;
        assert_eq!(sec_a % sub_count, 0);
        let record_size = sec_a / sub_count;
        assert_eq!(record_size, 116);

        // At u8x4 offsets within each record, xbox bytes == pc bytes
        for r in 0..sub_count {
            let rec_off = data_start + r * record_size;
            for &rel in U8X4_RECORD_RELATIVE {
                let pos = rec_off + rel;
                assert_eq!(
                    &xbox[pos..pos + 4],
                    &pc[pos..pos + 4],
                    "rec[{r}]+{rel}: xbox != pc (not endian-invariant)"
                );
            }
        }
    }

    #[test]
    fn soundbank_header_size_constant() {
        assert_eq!(HEADER_SIZE, 32);
    }

    #[test]
    fn u8x4_record_relative_non_empty() {
        assert!(!U8X4_RECORD_RELATIVE.is_empty());
    }

    #[test]
    fn loaded_soundbank_construction() {
        let sb = LoadedSoundbank {
            self_hash: 0x11111111,
            sub_count: 5,
            sub_count2: 3,
            section_b_hashes: vec![0x22222222, 0x33333333],
            section_d_hashes: vec![0x44444444],
            unresolved_hashes: vec![],
            issues: vec!["test issue".to_string()],
        };

        assert_eq!(sb.self_hash, 0x11111111);
        assert_eq!(sb.sub_count, 5);
        assert_eq!(sb.sub_count2, 3);
        assert_eq!(sb.section_b_hashes.len(), 2);
        assert_eq!(sb.section_d_hashes.len(), 1);
        assert_eq!(sb.issues.len(), 1);
    }
}
