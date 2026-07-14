//! Build-side editing of a `scripts_vz`-style block (a flat table of UCFX
//! containers, each wrapping one Lua chunk).
//!
//! Block layout (decompressed), see `docs/modding_deep_dive.md` §4.6 / §5.2:
//! ```text
//!   u32 entry_count
//!   entry_count × { u32 name_hash, u32 type_hash(=0x42498680), u32 field_c, u32 chunk_size }
//!   container[0]  (chunk_size[0] bytes: UCFX header + INFO/DEPS/BINN + LuaQ + CSUM trailer)
//!   container[1]  ...
//! ```
//! Each container ends with an 8-byte `CSUM` trailer = `"CSUM"` + CRC-32/JAMCRC
//! (`crc32_mercs2`) over every byte from the `UCFX` tag up to (not including)
//! the `CSUM` tag.
//!
//! To replace a script's compiled bytecode we: swap the LuaQ tail of the BINN
//! body, fix the BINN descriptor's `body_size` and the BINN metadata
//! `bytecode_size`, recompute the trailing CSUM, and update the entry's
//! `chunk_size`. The container model is verified against the real block by an
//! identity round-trip (re-serialize == input) plus a full CSUM re-verification.

use crate::crc32::crc32_mercs2;
use crate::hash::pandemic_hash_m2;
use crate::ucfx::walk_decompressed_block;

const LUAQ_SIG: &[u8; 4] = b"\x1bLua";

#[derive(Clone)]
pub struct Entry {
    pub name_hash: u32,
    pub type_hash: u32,
    pub field_c: u32,
    /// Raw container bytes: `[UCFX .. CSUM trailer]`. `chunk_size` == `bytes.len()`.
    pub bytes: Vec<u8>,
}

pub struct ScriptsBlock {
    pub entries: Vec<Entry>,
}

fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

impl ScriptsBlock {
    /// Parse a decompressed scripts block into per-entry containers.
    pub fn parse(block: &[u8]) -> Result<Self, String> {
        let (parsed, issues) = walk_decompressed_block(block, "scripts_vz");
        if let Some(first) = issues.first() {
            return Err(format!("walk issue: {} — {}", first.context, first.detail));
        }
        if parsed.entries.len() != parsed.containers.len() {
            return Err(format!(
                "entry/container count mismatch: {} vs {}",
                parsed.entries.len(),
                parsed.containers.len()
            ));
        }
        let entries = parsed
            .entries
            .iter()
            .zip(parsed.containers.into_iter())
            .map(|(e, bytes)| Entry {
                name_hash: e.name_hash,
                type_hash: e.type_hash,
                field_c: e.field_c,
                bytes,
            })
            .collect();
        Ok(Self { entries })
    }

    /// Re-emit the full decompressed block (header table + containers).
    pub fn serialize(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        for e in &self.entries {
            out.extend_from_slice(&e.name_hash.to_le_bytes());
            out.extend_from_slice(&e.type_hash.to_le_bytes());
            out.extend_from_slice(&e.field_c.to_le_bytes());
            out.extend_from_slice(&(e.bytes.len() as u32).to_le_bytes());
        }
        for e in &self.entries {
            out.extend_from_slice(&e.bytes);
        }
        out
    }

    /// Index of the entry whose name hashes (pandemic_hash_m2) to `name`.
    pub fn find_by_name(&self, name: &str) -> Option<usize> {
        let h = pandemic_hash_m2(name);
        self.entries.iter().position(|e| e.name_hash == h)
    }

    /// Verify every container's trailing CSUM == JAMCRC over `[UCFX..pre-CSUM]`.
    /// Returns the count verified, or the first mismatch.
    pub fn verify_csums(&self) -> Result<usize, String> {
        for (i, e) in self.entries.iter().enumerate() {
            csum_check(&e.bytes).map_err(|m| format!("entry {i}: {m}"))?;
        }
        Ok(self.entries.len())
    }

    /// Extract the raw LuaQ bytecode of entry `idx`.
    pub fn extract_lua(&self, idx: usize) -> Result<Vec<u8>, String> {
        let c = &self.entries[idx].bytes;
        let lay = parse_container(c)?;
        Ok(c[lay.luaq_off..lay.csum_off].to_vec())
    }

    /// Replace entry `idx`'s LuaQ bytecode with `new_luaq`. LuaQ is the last body
    /// before the CSUM trailer, so only the BINN descriptor `body_size`, the
    /// CSUM, and the entry `chunk_size` change — the UCFX header / descriptor
    /// offsets / INFO+DEPS bodies are untouched.
    pub fn replace_lua(&mut self, idx: usize, new_luaq: &[u8]) -> Result<(), String> {
        let c = &self.entries[idx].bytes;
        let lay = parse_container(c)?;
        if lay.binn_body_size as usize != lay.luaq_len {
            return Err(format!(
                "BINN.body_size ({}) != LuaQ length ({}); metadata-bearing BINN not yet supported",
                lay.binn_body_size, lay.luaq_len
            ));
        }
        // Rebuild: [prefix up to LuaQ] + [new LuaQ], patch body_size, append CSUM.
        let mut nc = Vec::with_capacity(lay.luaq_off + new_luaq.len() + 8);
        nc.extend_from_slice(&c[..lay.luaq_off]);
        nc.extend_from_slice(new_luaq);
        // BINN descriptor body_size lives at binn_desc_off + 8.
        let bs_off = lay.binn_desc_off + 8;
        nc[bs_off..bs_off + 4].copy_from_slice(&(new_luaq.len() as u32).to_le_bytes());
        // Recompute CSUM over [UCFX .. pre-CSUM] and append the 8-byte trailer.
        let csum = crc32_mercs2(&nc);
        nc.extend_from_slice(b"CSUM");
        nc.extend_from_slice(&csum.to_le_bytes());
        self.entries[idx].bytes = nc;
        Ok(())
    }
}

/// Parse a single container and return field offsets we need for editing.
pub struct ContainerLayout {
    pub data_base: usize,
    pub binn_desc_off: usize, // offset of BINN descriptor (tag) within container
    pub binn_body_size: u32,  // BINN descriptor body_size (== LuaQ length per §5.3)
    pub luaq_off: usize,      // offset of \x1bLua within container
    pub luaq_len: usize,      // bytes from luaq_off to start of CSUM trailer
    pub csum_off: usize,      // offset of "CSUM" trailer
    pub stored_csum: u32,
}

pub fn parse_container(c: &[u8]) -> Result<ContainerLayout, String> {
    if c.len() < 28 || &c[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_base = rd_u32(c, 4) as usize;
    let n_desc = rd_u32(c, 16) as usize;
    // Descriptors: 20 bytes each starting at +20 — {tag, row_u0, body_size, u3, u4}.
    let mut binn_desc_off = 0usize;
    let mut binn_body_size = 0u32;
    let mut found = false;
    for d in 0..n_desc {
        let off = 20 + d * 20;
        if off + 20 > c.len() {
            return Err("descriptor table overruns container".into());
        }
        if &c[off..off + 4] == b"BINN" {
            binn_desc_off = off;
            binn_body_size = rd_u32(c, off + 8);
            found = true;
        }
    }
    if !found {
        return Err("no BINN descriptor".into());
    }
    // CSUM trailer = last 8 bytes.
    if c.len() < 8 || &c[c.len() - 8..c.len() - 4] != b"CSUM" {
        return Err("missing CSUM trailer".into());
    }
    let csum_off = c.len() - 8;
    let stored_csum = rd_u32(c, csum_off + 4);
    // LuaQ is the tail of the BINN body, immediately before the CSUM trailer.
    let luaq_off = find_luaq(c).ok_or("no \\x1bLua signature")?;
    let luaq_len = csum_off - luaq_off;
    Ok(ContainerLayout {
        data_base,
        binn_desc_off,
        binn_body_size,
        luaq_off,
        luaq_len,
        csum_off,
        stored_csum,
    })
}

fn find_luaq(c: &[u8]) -> Option<usize> {
    c.windows(4).position(|w| w == LUAQ_SIG)
}

/// Recompute and verify a container's CSUM trailer.
fn csum_check(c: &[u8]) -> Result<(), String> {
    if c.len() < 8 || &c[c.len() - 8..c.len() - 4] != b"CSUM" {
        return Err("missing CSUM trailer".into());
    }
    let stored = rd_u32(c, c.len() - 4);
    let computed = crc32_mercs2(&c[..c.len() - 8]);
    if stored != computed {
        return Err(format!(
            "CSUM mismatch: stored 0x{stored:08X} computed 0x{computed:08X}"
        ));
    }
    Ok(())
}
