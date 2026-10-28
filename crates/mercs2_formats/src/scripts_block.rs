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
//! To replace a script's compiled bytecode we rebuild its container
//! ([`build_container`]): INFO and DEPS are derived again from the new bytecode,
//! the LuaQ chunk becomes the BINN body, the trailing CSUM is recomputed, and the
//! entry's `chunk_size` follows the new container length. The container model is verified against the real block by an
//! identity round-trip (re-serialize == input) plus a full CSUM re-verification.

use crate::crc32::crc32_mercs2;
use crate::ffcs::Endian;
use crate::hash::pandemic_hash_m2;
use crate::luachunk::{decode_info, Info};
use crate::script_meta::{self, ScriptMeta};
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

    /// Replace entry `idx`'s LuaQ bytecode with `new_luaq`, rebuilding the container: INFO and DEPS
    /// are derived again from the new bytecode ([`script_meta::derive`]), keeping the script's name.
    ///
    /// The engine loads every DEPS entry before it runs a script, so the new DEPS must still contain
    /// every module the old DEPS listed. One that is missing is an error naming it: dropping a load
    /// dependency of a retail script can leave a module its code uses unloaded (memory
    /// `ucfx-script-chunk-replace-header-bug`: an emptied `wifpmcinterior` DEPS wedged the world load).
    pub fn replace_lua(&mut self, idx: usize, new_luaq: &[u8]) -> Result<(), String> {
        let old = &self.entries[idx].bytes;
        let name = container_info(old)?.name;
        let old_deps = container_deps(old)?;
        let meta = script_meta::derive(&name, new_luaq)?;
        let missing: Vec<String> = old_deps
            .iter()
            .filter(|h| !meta.deps.iter().any(|d| d.hash == **h))
            .map(|h| format!("0x{h:08X}"))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "script {name}: the new bytecode no longer imports {} of its retail DEPS ({}); keep every import the script had",
                missing.len(),
                missing.join(", ")
            ));
        }
        self.entries[idx].bytes = build_container(&meta, new_luaq);
        Ok(())
    }

    /// Add a BRAND-NEW script entry carrying `luaq`, named `name`. Returns its index.
    ///
    /// This is what lets a Shipment ship a new resident script (a mod loader) instead of only
    /// appending to an existing one. The engine finds a script by `{pandemic_hash_m2(name), script
    /// type}` across all loaded WADs, the same way it finds the shipped DLC's new scripts.
    ///
    /// **This method only builds the block entry.** The caller MUST also emit the matching ASET row
    /// (type 35, this block, primary `0xFFFF`), or the loader wedges silently when the script is
    /// requested.
    ///
    /// The container is built from `luaq` with INFO and DEPS derived from it
    /// ([`script_meta::derive`]): its own name, globals and inherit counts, and one DEPS entry per
    /// literal `import`/`inherit`, so the engine loads those modules before it runs this one.
    /// `luaq` must be compiled with the bare lowercase `name` as its chunk name (retail's
    /// convention, which `mercs2_luac` follows). Refuses a duplicate name — two entries for one hash
    /// make the import ambiguous.
    pub fn add_script(&mut self, name: &str, luaq: &[u8]) -> Result<usize, String> {
        let h = pandemic_hash_m2(name);
        if self.entries.iter().any(|e| e.name_hash == h) {
            return Err(format!(
                "a container named {name:?} (0x{h:08X}) already exists; use replace_lua to edit it"
            ));
        }
        let meta = script_meta::derive(name, luaq)?;
        self.entries.push(Entry {
            name_hash: h,
            type_hash: crate::types::TYPE_HASH_SCRIPT,
            field_c: SCRIPT_FIELD_C,
            bytes: build_container(&meta, luaq),
        });
        Ok(self.entries.len() - 1)
    }
}

/// The block-table `field_c` of every retail script entry (the `script_meta_retail` test holds this).
pub const SCRIPT_FIELD_C: u32 = 0;

/// A script container built from its INFO/DEPS metadata and LuaQ chunk, laid out as retail lays one
/// out:
///
/// ```text
/// "UCFX"  u32 data_base = 20 + 20 × n   u32 0   u32 0   u32 n (descriptor count)
/// n × { tag, u32 body offset from data_base, u32 body size, u32 descriptors after this one, u32 0 }
/// INFO body, DEPS body (when present), BINN body = the LuaQ chunk — packed, no padding
/// "CSUM"  u32 crc32_mercs2 of every byte before it
/// ```
pub fn build_container(meta: &ScriptMeta, luaq: &[u8]) -> Vec<u8> {
    let mut bodies: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"INFO", meta.info_body())];
    if let Some(deps) = meta.deps_body() {
        bodies.push((b"DEPS", deps));
    }
    bodies.push((b"BINN", luaq.to_vec()));
    let n = bodies.len();
    let data_base = 20 + 20 * n;
    let mut out = Vec::with_capacity(data_base + bodies.iter().map(|(_, b)| b.len()).sum::<usize>() + 8);
    out.extend_from_slice(b"UCFX");
    out.extend_from_slice(&(data_base as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    let mut offset = 0usize;
    for (d, (tag, body)) in bodies.iter().enumerate() {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&((n - 1 - d) as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        offset += body.len();
    }
    for (_, body) in &bodies {
        out.extend_from_slice(body);
    }
    let csum = crc32_mercs2(&out);
    out.extend_from_slice(b"CSUM");
    out.extend_from_slice(&csum.to_le_bytes());
    out
}

/// The body of a container's chunk tagged `tag`, if it has one.
pub fn container_chunk<'c>(c: &'c [u8], tag: &[u8; 4]) -> Result<Option<&'c [u8]>, String> {
    if c.len() < 20 || &c[0..4] != b"UCFX" {
        return Err("not a UCFX container".into());
    }
    let data_base = rd_u32(c, 4) as usize;
    let n_desc = rd_u32(c, 16) as usize;
    for d in 0..n_desc {
        let off = 20 + d * 20;
        if off + 20 > c.len() {
            return Err("descriptor table overruns container".into());
        }
        if &c[off..off + 4] == tag {
            let start = data_base + rd_u32(c, off + 4) as usize;
            let end = start + rd_u32(c, off + 8) as usize;
            if end > c.len() {
                return Err(format!("{} body overruns container", String::from_utf8_lossy(tag)));
            }
            return Ok(Some(&c[start..end]));
        }
    }
    Ok(None)
}

/// A container's decoded INFO record.
pub fn container_info(c: &[u8]) -> Result<Info, String> {
    let body = container_chunk(c, b"INFO")?.ok_or("no INFO descriptor")?;
    decode_info(body, Endian::Little).map_err(|e| e.to_string())
}

/// A container's DEPS hashes; empty when it has no DEPS chunk.
pub fn container_deps(c: &[u8]) -> Result<Vec<u32>, String> {
    let Some(body) = container_chunk(c, b"DEPS")? else {
        return Ok(Vec::new());
    };
    let n = *body.first().ok_or("empty DEPS body")? as usize;
    if body.len() != 1 + 4 * n {
        return Err(format!("DEPS body is {} bytes for {n} entries", body.len()));
    }
    Ok((0..n).map(|i| rd_u32(body, 1 + 4 * i)).collect())
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
