//! ActionTable / named-registry overflow check (type 0x207359C7, type_id 11).
//!
//! The engine processes these tables in FUN_0067cfb0 by building a FIXED 1024-slot
//! per-row hash table (open-addressing, mask 0x3FF). A table with more than 1024
//! rows fills the table and the next linear probe at 0x0067D130 spins forever —
//! the deterministic world-load livelock.
//!
//! A table is laid out `UCFX → INFO → TYPE → VALU{ header, N dimension-name
//! strings, then value rows of N u32 each }`. This consumer computes a static
//! per-table VALU row estimate:
//!
//!   rows = (VALU value-region u32 count) / dimension_count
//!
//! SCOPE / KNOWN LIMITATION (2026-06-11, corrected after live + decompile RE):
//! This catches a *mod that SHIPS an oversized ActionTable* in its own WAD — a real
//! and worth-guarding case — and it passes the base game clean (no false positives).
//! It does **not** model the overflow that hangs the actual deployed DLC: the engine
//! builds the runtime table in FUN_0067c780 with count = (INFO-chunk u16) − 1 (live
//! value 3075, NOT the VALU row count), then FUN_0067cfb0 dedups `count` entries into
//! the 1024 slots. The deployed patch carries NO ActionTable at all, so the engine
//! uses the BASE table — base-alone processes the same count and works, which means
//! the DLC overflows by changing the *distinct-key result* of dedup'ing that same
//! table. The dedup key must therefore resolve against the loaded asset set, so the
//! true overflow is a function of the FULL OVERLAY, not any single table's row count.
//! The accumulating overlay-aware check (resolve each row's key across base+patch+
//! siblings, count distinct, flag >1024) is pending the key-resolution RE.
//! It applies to every type-0x207359C7 table; ones without the VALU/dimension layout
//! fall through to the structural consumer unchanged.

use crate::consume::{consume_structural, ConsumeResult};

/// Engine's fixed per-row table size (the 0x3FF probe mask in FUN_0067cfb0).
pub const ACTION_TABLE_CAPACITY: usize = 1024;

/// Named-registry tables CONFIRMED to be processed by a fixed 1024-slot table
/// (FUN_0067cfb0, registered as a vtable handler for these specific tables).
/// Other type-0x207359C7 registries (Sounds, VehicleAnimationLookup, …) have
/// their own, larger handlers — the BASE game ships them with >1024 rows and
/// loads fine — so they must NOT be flagged at 1024. Add a hash here only after
/// confirming (live) that the table goes through the 1024-slot path.
///   0x6802C321 = pandemic_hash_m2("ActionTable")
const TABLES_CAPPED_AT_1024: &[u32] = &[0x6802C321];

fn read_u16_le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

fn read_u32_le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

/// Registry header read from a stance/named-registry nested UCFX container.
pub struct RegistryHeader {
    pub key_dims: usize,
    pub total_dims: usize,
    /// Row count the engine processes (and dedups into the 1024-slot table).
    pub count: usize,
}

/// Parse the row count of a stance/named-registry table from its `INFO` chunk.
///
/// The container is a nested UCFX: header(20) + descriptor table(N×20) + data area.
/// The `INFO` descriptor's 6-byte body is the dims triple
/// `[u16 keyDims][u16 totalDims][u16 count]`; `count` is the number of value rows
/// the engine feeds into FUN_0067cfb0's fixed 1024-slot table. This reads it
/// directly rather than deriving it from the VALU value-region size — and since the
/// converter now swaps the INFO u16s per-field, the value is the true row count
/// (a blanket u32 swap used to transpose the dims and leave the count big-endian,
/// turning 1036 into 3076).
pub fn registry_header(container: &[u8]) -> Option<RegistryHeader> {
    if container.len() < 20 || &container[0..4] != b"UCFX" {
        return None;
    }
    let ndesc = read_u32_le(container, 16) as usize;
    if ndesc == 0 || ndesc > 64 {
        return None;
    }
    let data_area = 20 + ndesc * 20;
    if data_area > container.len() {
        return None;
    }
    for i in 0..ndesc {
        let off = 20 + i * 20;
        if &container[off..off + 4] != b"INFO" {
            continue;
        }
        let row_u0 = read_u32_le(container, off + 4) as usize;
        let body_size = read_u32_le(container, off + 8) as usize;
        let info = data_area + row_u0;
        if body_size >= 6 && info + 6 <= container.len() {
            return Some(RegistryHeader {
                key_dims: read_u16_le(container, info) as usize,
                total_dims: read_u16_le(container, info + 2) as usize,
                count: read_u16_le(container, info + 4) as usize,
            });
        }
    }
    None
}

pub fn consume_action_table(
    asset_hash: u32,
    container: &[u8],
    data_body: Option<&[u8]>,
    label: &str,
) -> ConsumeResult {
    let mut r = consume_structural(container, data_body, label);

    // Only tables on the fixed 1024-slot path are subject to the overflow.
    if !TABLES_CAPPED_AT_1024.contains(&asset_hash) {
        return r;
    }

    if let Some(h) = registry_header(container) {
        if h.count > ACTION_TABLE_CAPACITY {
            r.issues.push(format!(
                "{label}: registry count={} > {ACTION_TABLE_CAPACITY}-slot engine table \
                 (keyDims={}, totalDims={}) — overflows FUN_0067cfb0's fixed 1024-slot table \
                 -> world-load livelock (linear-probe 0x0067D130). Needs runtime table \
                 expansion to next_pow2({}).",
                h.count, h.key_dims, h.total_dims, h.count
            ));
            r.structural_violations += 1;
        }
    }
    r
}
