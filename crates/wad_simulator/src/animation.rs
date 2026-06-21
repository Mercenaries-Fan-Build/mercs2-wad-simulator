//! Animation / Havok packfile structural validation.

use crate::consume::ConsumeResult;

const HAVOK_MAGIC_1: &[u8; 4] = b"\x57\xE0\xE0\x57";
const HAVOK_MAGIC_2: &[u8; 4] = b"\xD0\x11\xCE\xFA";

pub fn consume_animation(_container: &[u8], data_body: Option<&[u8]>, label: &str) -> ConsumeResult {
    let body = data_body.unwrap_or(&[]);
    let consumed = !body.is_empty();
    let mut issues = Vec::new();
    let mut structural_violations = 0u32;

    if consumed {
        // Walk EVERY Havok packfile magic in the body, not just the first. An
        // animgroup embeds multiple packfile headers (the canonical top-level
        // packfile plus nested per-animation headers); checking only the first
        // both misses real corruption and can report a non-canonical embedded
        // header. The endianness byte lives at magic+17 (layoutRules[1]).
        let mut searched = 0usize;
        while let Some(rel) = find_havok_header(&body[searched..]) {
            let offset = searched + rel;
            if offset + 18 <= body.len() {
                let le_flag = body[offset + 17];
                if le_flag != 1 {
                    issues.push(format!(
                        "{label}: Havok packfile @+{offset} endianness byte = {le_flag} (expected 1 = LE)"
                    ));
                    structural_violations += 1;
                }
            }
            searched = offset + 8;
            if searched + 18 > body.len() {
                break;
            }
        }
    }

    ConsumeResult {
        consumed,
        issues,
        structural_violations,
        ..Default::default()
    }
}

fn find_havok_header(data: &[u8]) -> Option<usize> {
    if data.len() < 18 {
        return None;
    }
    for i in 0..data.len().saturating_sub(17) {
        if &data[i..i + 4] == HAVOK_MAGIC_1 || &data[i..i + 4] == HAVOK_MAGIC_2 {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_animation_empty_body() {
        let container = b"UCFX".to_vec();
        let result = consume_animation(&container, None, "test_anim");

        assert!(!result.consumed);
        assert!(result.issues.is_empty());
        assert_eq!(result.structural_violations, 0);
    }

    #[test]
    fn consume_animation_empty_data_body() {
        let container = b"UCFX".to_vec();
        let body = vec![];
        let result = consume_animation(&container, Some(&body), "test_anim");

        assert!(!result.consumed);
    }

    #[test]
    fn consume_animation_valid_havok_le() {
        let mut body = vec![0u8; 50];
        // Write first Havok magic
        body[0..4].copy_from_slice(HAVOK_MAGIC_1);
        // Write LE flag at offset 17
        body[17] = 1;

        let result = consume_animation(&[], Some(&body), "test_anim");

        assert!(result.consumed);
        assert!(result.issues.is_empty());
        assert_eq!(result.structural_violations, 0);
    }

    #[test]
    fn consume_animation_invalid_endianness() {
        let mut body = vec![0u8; 50];
        body[0..4].copy_from_slice(HAVOK_MAGIC_1);
        body[17] = 0;  // Wrong endianness flag

        let result = consume_animation(&[], Some(&body), "test_anim");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert!(result.issues[0].contains("endianness byte = 0"));
        assert_eq!(result.structural_violations, 1);
    }

    #[test]
    fn consume_animation_havok_magic_2() {
        let mut body = vec![0u8; 50];
        body[0..4].copy_from_slice(HAVOK_MAGIC_2);
        body[17] = 1;

        let result = consume_animation(&[], Some(&body), "test_anim");

        assert!(result.consumed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn consume_animation_multiple_headers() {
        let mut body = vec![0u8; 100];
        // First header at offset 0
        body[0..4].copy_from_slice(HAVOK_MAGIC_1);
        body[17] = 1;
        // Second header at offset 40
        body[40..44].copy_from_slice(HAVOK_MAGIC_1);
        body[40 + 17] = 1;

        let result = consume_animation(&[], Some(&body), "test_anim");

        assert!(result.consumed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn consume_animation_multiple_headers_mixed_endian() {
        let mut body = vec![0u8; 100];
        // First header OK
        body[0..4].copy_from_slice(HAVOK_MAGIC_1);
        body[17] = 1;
        // Second header with wrong endianness
        body[40..44].copy_from_slice(HAVOK_MAGIC_1);
        body[40 + 17] = 2;

        let result = consume_animation(&[], Some(&body), "test_anim");

        assert!(result.consumed);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.structural_violations, 1);
    }

    #[test]
    fn find_havok_header_first_magic() {
        let mut data = vec![0u8; 30];
        data[0..4].copy_from_slice(HAVOK_MAGIC_1);

        let pos = find_havok_header(&data);
        assert_eq!(pos, Some(0));
    }

    #[test]
    fn find_havok_header_offset() {
        let mut data = vec![0u8; 50];
        data[10..14].copy_from_slice(HAVOK_MAGIC_1);

        let pos = find_havok_header(&data);
        assert_eq!(pos, Some(10));
    }

    #[test]
    fn find_havok_header_second_magic() {
        let mut data = vec![0u8; 50];
        data[5..9].copy_from_slice(HAVOK_MAGIC_2);

        let pos = find_havok_header(&data);
        assert_eq!(pos, Some(5));
    }

    #[test]
    fn find_havok_header_not_found() {
        let data = vec![0u8; 30];
        let pos = find_havok_header(&data);
        assert_eq!(pos, None);
    }

    #[test]
    fn find_havok_header_too_small() {
        let data = vec![0u8; 10];
        let pos = find_havok_header(&data);
        assert_eq!(pos, None);
    }

    #[test]
    fn find_havok_header_exactly_18_bytes() {
        let mut data = vec![0u8; 18];
        data[0..4].copy_from_slice(HAVOK_MAGIC_1);

        let pos = find_havok_header(&data);
        assert_eq!(pos, Some(0));
    }

    #[test]
    fn find_havok_header_at_boundary() {
        let mut data = vec![0u8; 36];  // Need 18 bytes after position 18
        data[18..22].copy_from_slice(HAVOK_MAGIC_1);

        let pos = find_havok_header(&data);
        assert_eq!(pos, Some(18));
    }

    #[test]
    fn consume_animation_label_in_error() {
        let mut body = vec![0u8; 50];
        body[0..4].copy_from_slice(HAVOK_MAGIC_1);
        body[17] = 99;

        let result = consume_animation(&[], Some(&body), "my_animation_name");

        assert!(result.issues[0].contains("my_animation_name"));
    }

    #[test]
    fn consume_animation_havok_near_end() {
        let mut body = vec![0u8; 40];
        body[21..25].copy_from_slice(HAVOK_MAGIC_1);
        body[38] = 1;  // Endianness flag at 21 + 17 = 38

        let result = consume_animation(&[], Some(&body), "test");

        // Should find the header even though it's near the end
        assert!(result.consumed);
    }
}

