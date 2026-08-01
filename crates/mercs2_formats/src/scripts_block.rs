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

    /// Like [`Self::find_by_name`], but only matches an entry that is actually a **script**.
    ///
    /// `scripts_vz` is 114 containers that are all Lua, so there the type check is redundant. The
    /// **resident** block is not: it carries ~240 Lua chunks among ~7,000 entries of other types.
    /// Matching on `name_hash` alone there would let a 32-bit collision splice compiled Lua into a
    /// texture — which produces a corrupt block rather than an error, so it must be impossible by
    /// construction rather than merely unlikely.
    pub fn find_script_by_name(&self, name: &str) -> Option<usize> {
        let h = pandemic_hash_m2(name);
        self.entries
            .iter()
            .position(|e| e.name_hash == h && e.type_hash == crate::types::TYPE_HASH_SCRIPT)
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
        self.entries[idx].bytes = rebuild_container_with_luaq(&self.entries[idx].bytes, new_luaq)?;
        Ok(())
    }

    /// Add a BRAND-NEW script entry carrying `luaq`, named `name`. Returns its index.
    ///
    /// This is what lets a Shipment ship a new resident script (a mod loader) instead of only
    /// appending to an existing one. The recipe is not speculative — it is the one the shipped DLC
    /// uses. Confirmed from four angles:
    ///
    /// * the exe's own bootstrap: `import(m)` → `_SYS._IMPORT(getfenv(2), m)`, and the engine
    ///   resolves a module **by ASET hash across all loaded WADs**
    ///   (`docs/vanilla_mission_lifecycle_analysis.md` Open-Q1);
    /// * every one of the 114 retail `scripts_vz` scripts carries a primary type-35 ASET row;
    /// * `dlc_aset_normalize.py` adds `script_aset_entry(pandemic_hash_m2(name))` for each NEW DLC
    ///   script precisely so `import`/`dynamic_import` can find it — a shipped, working case;
    /// * the heli experiment's documented lesson is this same recipe; its hangs were a MISSING ASET
    ///   row, use of the *resident* block (worldentity double-registration) and a DEPS cycle — all
    ///   avoided here by targeting `scripts_vz` and resolving via `import` (no DEPS edge).
    ///
    /// **This method only builds the block entry.** The caller MUST also emit the matching ASET row
    /// (type 35, this block, primary `0xFFFF`), or the loader wedges silently when the import is
    /// resolved — that is step 4, the one that bites.
    ///
    /// The container is CLONED from an existing INFO/BINN script in this block, then its LuaQ is
    /// swapped, so it reuses the exact container shape the engine already accepts. `luaq` must be
    /// compiled with the BARE `name` as its chunk name (retail's convention, which `mercs2_luac`
    /// follows). Refuses a duplicate name — two entries for one hash make the import ambiguous.
    pub fn add_script(&mut self, name: &str, luaq: &[u8]) -> Result<usize, String> {
        let h = pandemic_hash_m2(name);
        if self.entries.iter().any(|e| e.name_hash == h) {
            return Err(format!(
                "a container named {name:?} (0x{h:08X}) already exists; use replace_lua to edit it"
            ));
        }
        // A plain INFO/BINN script (no DEPS): the new script declares no dependencies — its load
        // timing is controlled by whoever imports it, not by a DEPS edge.
        let (field_c, template) = self
            .entries
            .iter()
            .find(|e| {
                e.type_hash == crate::types::TYPE_HASH_SCRIPT && !container_has_chunk(&e.bytes, b"DEPS")
            })
            .map(|e| (e.field_c, e.bytes.clone()))
            .ok_or("no plain INFO/BINN script in this block to use as a template")?;
        let bytes = rebuild_container_with_luaq(&template, luaq)?;
        self.entries.push(Entry {
            name_hash: h,
            type_hash: crate::types::TYPE_HASH_SCRIPT,
            field_c,
            bytes,
        });
        Ok(self.entries.len() - 1)
    }
}

/// True if a UCFX container's descriptor table carries a chunk tagged `tag`.
fn container_has_chunk(c: &[u8], tag: &[u8; 4]) -> bool {
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return false;
    }
    let n_desc = rd_u32(c, 16) as usize;
    (0..n_desc).any(|d| {
        let off = 20 + d * 20;
        off + 4 <= c.len() && &c[off..off + 4] == tag
    })
}

/// Swap a container's LuaQ tail for `new_luaq`, fixing the BINN `body_size` and re-stamping the
/// CSUM. Shared by [`ScriptsBlock::replace_lua`] and [`ScriptsBlock::add_script`].
fn rebuild_container_with_luaq(c: &[u8], new_luaq: &[u8]) -> Result<Vec<u8>, String> {
    let lay = parse_container(c)?;
    if lay.binn_body_size as usize != lay.luaq_len {
        return Err(format!(
            "BINN.body_size ({}) != LuaQ length ({}); metadata-bearing BINN not yet supported",
            lay.binn_body_size, lay.luaq_len
        ));
    }
    let mut nc = Vec::with_capacity(lay.luaq_off + new_luaq.len() + 8);
    nc.extend_from_slice(&c[..lay.luaq_off]);
    nc.extend_from_slice(new_luaq);
    let bs_off = lay.binn_desc_off + 8; // BINN descriptor body_size
    nc[bs_off..bs_off + 4].copy_from_slice(&(new_luaq.len() as u32).to_le_bytes());
    let csum = crc32_mercs2(&nc);
    nc.extend_from_slice(b"CSUM");
    nc.extend_from_slice(&csum.to_le_bytes());
    Ok(nc)
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
