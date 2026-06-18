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
