//! Engine-verified structural invariants for individual UCFX chunk tags.
//!
//! Each rule below was derived by disassembling the chunk's handler in
//! `output/patched/Mercenaries2.exe` (image base 0x00400000). These run on EVERY
//! container regardless of asset type (a single descriptor-table walk) and report
//! malformed chunks. The engine overflow-guards its count-driven allocations
//! (`seto/neg/or` saturates an overflowing size to 0xFFFFFFFF → the allocator
//! fails gracefully), so a violation here is a converter/data defect worth
//! surfacing but is NOT the silent heap-overrun hazard MTRL is — hence ADVISORY
//! (routed to `structural_advisory`, like the AREA checks).
//!
//! Verified handlers (see docs/ucfx_tag_registry.md):
//!   * renderable consumer @0x004a4c40 reads each array chunk as `count * record`
//!     bytes, `count` from the 0x10-byte renderable INFO:
//!       - PTCH @0x4a4cbe : record 0x38 (56)   [count @esi+0x20 → buf @esi+0x24]
//!       - INST @0x4a4e51 : record 0x18 (24)   [count @esi+0x28 → buf @esi+0x2c]
//!       - PTMS @0x4a4e78 : record 0x08 ( 8)   [count @esi+0x30 → buf @esi+0x34]
//!   * POFF @0x4a9cf2 reads a fixed 0xC (Vec3) offset into @esi+0x30.
//!   * PTYP @0x491ba9 reads a single flags byte (bit0→+0x205, bit1→+0x206).
//!
//!   * PHY2 (@0x4a845f) is a u32 header prefix + an embedded Havok 5.5 packfile
//!     (magic located by *search*, not at offset 0) + a trailing wrapper. We do
//!     not trust it blindly nor naively check a magic at offset 0 (that
//!     false-fires on retail); instead we RECALCULATE it via
//!     `ucfx_byteswap::havok::validate_phy2`, which locates the packfile and
//!     verifies it carries what the BE→LE converter requires (length + version +
//!     `__classnames__`). A magic-less PHY2 is a valid legacy form (not flagged).

use mercs2_formats::ffcs::read_u32_le;
use ucfx_byteswap::havok::{validate_phy2, Phy2Check};

#[derive(Default)]
pub struct ChunkInvariantResult {
    pub issues: Vec<String>,
    /// Advisory structural violations (engine overflow-guards these allocations).
    pub violations: u32,
}

/// Walk a UCFX container's descriptor table once and apply per-tag structural
/// rules to every leaf chunk body. See module docs for the verified invariants.
pub fn validate_chunk_invariants(container: &[u8], label: &str) -> ChunkInvariantResult {
    let mut r = ChunkInvariantResult::default();
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return r;
    }
    let data_area_off = read_u32_le(container, 4) as usize;
    let n_desc = read_u32_le(container, 16) as usize;
    let max_desc = container.len().saturating_sub(20) / 20;
    if n_desc > max_desc {
        return r;
    }

    for i in 0..n_desc {
        let row = 20 + i * 20;
        if row + 20 > container.len() {
            break;
        }
        let tag = &container[row..row + 4];
        let row_u0 = read_u32_le(container, row + 4) as usize;
        if row_u0 == 0xFFFF_FFFF {
            continue; // container header, not a leaf body
        }
        let body_size = read_u32_le(container, row + 8) as usize;
        let body_start = if data_area_off > 0 {
            data_area_off + row_u0
        } else {
            8 + row_u0
        };
        if body_start + body_size > container.len() {
            continue; // bounds already reported elsewhere
        }
        let body = &container[body_start..body_start + body_size];

        match tag {
            b"PTCH" => record_aligned(&mut r, label, "PTCH", body.len(), 0x38),
            b"INST" => record_aligned(&mut r, label, "INST", body.len(), 0x18),
            b"PTMS" => record_aligned(&mut r, label, "PTMS", body.len(), 0x08),
            b"POFF" => min_size(&mut r, label, "POFF", body.len(), 0x0c, "Vec3 offset"),
            b"PTYP" => min_size(&mut r, label, "PTYP", body.len(), 1, "flags byte"),
            // Mesh/anim tail, confirmed in all_functions_decomp.txt.
            b"BSHI" => record_aligned(&mut r, label, "BSHI", body.len(), 2),
            b"ASTO" => min_size(&mut r, label, "ASTO", body.len(), 4, "u32 count @FUN_0067c780"),
            b"MINF" => min_size(&mut r, label, "MINF", body.len(), 6, "u32 hash + u16 @FUN_0068e5d0"),
            // DECL is context-dependent: the ECS-template DECL is 0x24-record
            // (FUN_0045dbb0), but DECL in other asset types (e.g. material/resident
            // blocks) has a different layout — a context-blind record-align check
            // false-fired on retail (block 3185, 10000-byte DECL). Registered.
            // High-frequency effect/mesh chunks (>100 occurrences in vz.wad), each
            // verified against its engine handler (see docs/ucfx_tag_registry.md).
            // The engine overflow-guards the count-driven allocations, so a short
            // body is an over-read/truncation signal, not a heap overflow — advisory.
            b"NODE" => min_size(&mut r, label, "NODE", body.len(), 8, "u32 hash + u32 child-count @0x4cf48b"),
            b"TRFM" => min_size(&mut r, label, "TRFM", body.len(), 64, "4x4 transform matrix @0x48cd09"),
            b"COLR" => min_size(&mut r, label, "COLR", body.len(), 0xc8, "200-byte colour palette @0x4930e5"),
            b"EMTR" => min_size(&mut r, label, "EMTR", body.len(), 2, "u16 emitter count @0x492402"),
            b"ATRB" => min_size(&mut r, label, "ATRB", body.len(), 4, "inner-hash sub-dispatch @0x492b1c"),
            b"FRCE" => min_size(&mut r, label, "FRCE", body.len(), 4, "inner-hash sub-dispatch @0x491c93"),
            b"TEXT" => min_size(&mut r, label, "TEXT", body.len(), 4, "leading u32 @0x492fab"),
            // ECS entity-template ref arrays (0x45f4xx–0x45f9xx): count×4 u32 refs,
            // count from an INFO field, alloc overflow-guarded.
            b"DAMG" => record_aligned(&mut r, label, "DAMG", body.len(), 4),
            b"DEBR" => record_aligned(&mut r, label, "DEBR", body.len(), 4),
            b"PART" => record_aligned(&mut r, label, "PART", body.len(), 4),
            b"SOUN" => record_aligned(&mut r, label, "SOUN", body.len(), 4),
            // TREE: variable-length records (4×u32 + u16 sub-count + sub_count×u16,
            // FUN_0045f3f0); 0x34 is the in-memory alloc size, NOT an on-disk stride —
            // so there is no fixed body-size invariant (Registered, like MANM).
            // KEYS @0x4640a8: u32 count header then count×8 keyframe records.
            b"KEYS" => header_records(&mut r, label, "KEYS", body.len(), 4, 8),
            // Anim/sequence (0x67c–0x68e): VALU is a 4-aligned value blob; TRCK has
            // a 12-byte inline header then count×4 arrays; MANM is a fixed 0x34 struct.
            b"VALU" => record_aligned(&mut r, label, "VALU", body.len(), 4),
            b"TRCK" => min_size(&mut r, label, "TRCK", body.len(), 12, "3×u32 inline header @0x68e7c3"),
            // MANM: 0x34 is the in-memory alloc, NOT the body read (retail bodies are
            // 16 bytes) — no confirmed self-contained body invariant. Registered.
            b"PHY2" => {
                if let Phy2Check::Malformed(why) = validate_phy2(body) {
                    if r.issues.len() < 8 {
                        r.issues.push(format!(
                            "{label}: PHY2 {why} — embedded Havok packfile would fail BE→LE conversion"
                        ));
                    }
                    r.violations += 1;
                }
            }
            _ => {}
        }
    }
    r
}

/// The engine reads `count * record` bytes; a body that is not a whole number of
/// records means the converter mis-sized the chunk (count would not match).
fn record_aligned(r: &mut ChunkInvariantResult, label: &str, tag: &str, len: usize, record: usize) {
    if len % record != 0 {
        if r.issues.len() < 8 {
            r.issues.push(format!(
                "{label}: {tag} body {len} bytes not a multiple of {record}-byte record \
                 (engine reads count×{record})"
            ));
        }
        r.violations += 1;
    }
}

/// A `header` byte prefix (e.g. a u32 count) followed by whole `record`-byte rows.
fn header_records(r: &mut ChunkInvariantResult, label: &str, tag: &str, len: usize, header: usize, record: usize) {
    if len < header || (len - header) % record != 0 {
        if r.issues.len() < 8 {
            r.issues.push(format!(
                "{label}: {tag} body {len} bytes != {header}-byte header + N×{record}-byte records"
            ));
        }
        r.violations += 1;
    }
}

fn min_size(r: &mut ChunkInvariantResult, label: &str, tag: &str, len: usize, min: usize, what: &str) {
    if len < min {
        if r.issues.len() < 8 {
            r.issues.push(format!(
                "{label}: {tag} body {len} bytes < {min} ({what}) — engine over-reads the chunk stream"
            ));
        }
        r.violations += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a UCFX container with a single leaf descriptor (tag, body).
    fn ucfx_with(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let data_area_off = 20 + 20; // header + 1 descriptor
        let mut buf = vec![0u8; data_area_off];
        buf[0..4].copy_from_slice(b"UCFX");
        buf[4..8].copy_from_slice(&(data_area_off as u32).to_le_bytes());
        buf[16..20].copy_from_slice(&1u32.to_le_bytes());
        buf[20..24].copy_from_slice(tag);
        buf[24..28].copy_from_slice(&0u32.to_le_bytes()); // row_u0 = 0 (leaf)
        buf[28..32].copy_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend_from_slice(body);
        buf
    }

    #[test]
    fn ptch_record_alignment() {
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTCH", &vec![0u8; 0x38 * 3]), "t").violations, 0);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTCH", &vec![0u8; 0x38 * 2 + 5]), "t").violations, 1);
        // empty (count 0) is valid
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTCH", &[]), "t").violations, 0);
    }

    #[test]
    fn inst_ptms_alignment() {
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"INST", &vec![0u8; 0x18 * 4]), "t").violations, 0);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"INST", &vec![0u8; 0x18 + 1]), "t").violations, 1);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTMS", &vec![0u8; 8 * 5]), "t").violations, 0);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTMS", &vec![0u8; 7]), "t").violations, 1);
    }

    #[test]
    fn phy2_recalculation() {
        use ucfx_byteswap::havok;
        // No magic → legacy, not flagged.
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PHY2", &[0u8; 32]), "t").violations, 0);
        // magic present but truncated packfile → flagged.
        let mut bad = vec![0u8; 4];
        bad.extend_from_slice(&[0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10]);
        bad.extend_from_slice(&[0u8; 8]);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PHY2", &bad), "t").violations, 1);
        // sanity: the underlying recalculation agrees.
        assert!(matches!(havok::validate_phy2(&bad), havok::Phy2Check::Malformed(_)));
    }

    #[test]
    fn high_frequency_effect_min_sizes() {
        // verified min sizes for the >100-occurrence effect/mesh chunks
        for (tag, ok, bad) in [
            (b"NODE", 8usize, 7usize),
            (b"TRFM", 64, 63),
            (b"COLR", 0xc8, 0xc7),
            (b"EMTR", 2, 1),
            (b"ATRB", 4, 3),
            (b"FRCE", 4, 3),
            (b"TEXT", 4, 3),
        ] {
            assert_eq!(
                validate_chunk_invariants(&ucfx_with(tag, &vec![0u8; ok]), "t").violations,
                0,
                "{} ok",
                std::str::from_utf8(tag).unwrap()
            );
            assert_eq!(
                validate_chunk_invariants(&ucfx_with(tag, &vec![0u8; bad]), "t").violations,
                1,
                "{} bad",
                std::str::from_utf8(tag).unwrap()
            );
        }
    }

    #[test]
    fn poff_ptyp_min_size() {
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"POFF", &vec![0u8; 12]), "t").violations, 0);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"POFF", &vec![0u8; 8]), "t").violations, 1);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTYP", &[0u8]), "t").violations, 0);
        assert_eq!(validate_chunk_invariants(&ucfx_with(b"PTYP", &[]), "t").violations, 1);
    }

}
