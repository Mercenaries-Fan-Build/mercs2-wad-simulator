//! `AssetRegistry` — block residency + hash-keyed chunk registries, modelled on the retail engine.
//!
//! The retail engine does NOT resolve assets one-at-a-time. It streams **blocks**; a block going
//! resident registers *every* chunk it carries into global, hash-keyed open-addressing tables, and
//! objects then resolve a name hash against whatever is currently resident. That is why an asset's
//! dependencies live in *other* blocks (`oc_veh_helicopter_md500`'s model is in block 3350; its
//! textures `0x22101D86` / `0xFB385BF0` are in blocks 2976 / 2977) and still bind at runtime.
//!
//! Two rules recovered from the decomp, both load-bearing, and easy to get backwards:
//!
//! 1. **Registry insert is get-or-create — FIRST wins.** `FUN_004cc130` probes the pool and, on an
//!    occupied slot, returns the existing cell and creates nothing. A second block carrying the same
//!    chunk hash is *ignored*. (Verified live: the probe `FUN_008242b0` is `slot = key % size` with
//!    an 8-way unrolled linear scan; table base in `ESI`.)
//! 2. **The WAD overlay stack is a different layer, and it IS last-wins.** Which *block* to make
//!    resident for a hash is resolved by walking `base + vz-patch.wad + …` in reverse. Once a block
//!    is resident, its chunks register first-wins. The two compose exactly as retail does: a patch
//!    WAD's block is chosen first, so its chunks land in the registry before the base's.
//!
//! Divergence from retail, deliberate: retail never evicts registry entries and carries no owning-block
//! id, so a stale reference resolves to a null sentinel and is dereferenced unchecked (that is the
//! AV at `0x47AA5C`). We track the owning block, evict its chunks with it, and return `None`.
//!
//! See `docs/modernization/model_render_gate_spec.md` §2b.

use crate::wad::{self, Wad};
use mercs2_formats::ucfx::parse_block_entry_table;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Default cap on simultaneously-resident blocks. Blocks are frequently multi-MB decompressed, and
/// `vz.wad` has ~9.4k of them, so unbounded residency is not an option for us even though retail's
/// budget lives in its streaming manager (`mgr+0x4c368`). Eviction is FIFO.
pub const DEFAULT_MAX_RESIDENT_BLOCKS: usize = 32;

/// Where a registered chunk lives: the resident block that owns it, and its span within that block's
/// decompressed bytes. Carrying the owner is what lets us evict coherently (retail cannot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkRef {
    /// Index into the WAD stack (0 = base, 1.. = overlays in load order).
    pub src: usize,
    pub block: u16,
    pub start: usize,
    pub end: usize,
}

/// What a `make_resident` call did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResidentReport {
    /// The block was already resident; nothing was decompressed or registered.
    pub already_resident: bool,
    /// Chunks this block contributed to the registry.
    pub registered: usize,
    /// Chunks this block carried whose `(type_hash, name_hash)` was already registered by an
    /// earlier block, and were therefore IGNORED (first-wins).
    pub shadowed: usize,
    /// Blocks evicted to stay under the residency cap.
    pub evicted: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegistryStats {
    pub resident_blocks: usize,
    pub registered_chunks: usize,
    pub shadowed_total: usize,
    pub evicted_total: usize,
}

pub struct AssetRegistry {
    /// Decompressed bytes of each resident block, keyed by `(src, block)`.
    resident: HashMap<(usize, u16), Arc<Vec<u8>>>,
    /// Residency order, oldest first — FIFO eviction.
    order: VecDeque<(usize, u16)>,
    /// The global registry: `(type_hash, name_hash) -> chunk`. First writer wins.
    chunks: HashMap<(u32, u32), ChunkRef>,
    max_resident: usize,
    shadowed_total: usize,
    evicted_total: usize,
}

impl Default for AssetRegistry {
    fn default() -> Self {
        AssetRegistry::with_capacity(DEFAULT_MAX_RESIDENT_BLOCKS)
    }
}

impl AssetRegistry {
    pub fn with_capacity(max_resident: usize) -> AssetRegistry {
        AssetRegistry {
            resident: HashMap::new(),
            order: VecDeque::new(),
            chunks: HashMap::new(),
            max_resident: max_resident.max(1),
            shadowed_total: 0,
            evicted_total: 0,
        }
    }

    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            resident_blocks: self.resident.len(),
            registered_chunks: self.chunks.len(),
            shadowed_total: self.shadowed_total,
            evicted_total: self.evicted_total,
        }
    }

    pub fn is_resident(&self, src: usize, block: u16) -> bool {
        self.resident.contains_key(&(src, block))
    }

    /// Look up a chunk that is already registered. No I/O, no residency change.
    pub fn lookup(&self, type_hash: u32, name_hash: u32) -> Option<ChunkRef> {
        self.chunks.get(&(type_hash, name_hash)).copied()
    }

    /// The registered chunk of a `type_hash`, whatever its name — for the **singleton** classes
    /// (`watermap`, `materialtable`) the engine addresses by type. Returns `(name_hash, ChunkRef)`.
    /// Already-registered only; no I/O. See [`resolve_singleton`](Self::resolve_singleton).
    ///
    /// A singleton class has exactly one chunk in the archive, so the scan is unambiguous; if a class
    /// ever carried several, the lowest name hash is picked so the answer is at least deterministic.
    pub fn lookup_singleton(&self, type_hash: u32) -> Option<(u32, ChunkRef)> {
        self.chunks
            .iter()
            .filter(|((t, _), _)| *t == type_hash)
            .min_by_key(|((_, n), _)| *n)
            .map(|((_, n), c)| (*n, *c))
    }

    /// The bytes of a registered chunk. `None` if its owning block has since been evicted.
    pub fn slice(&self, c: ChunkRef) -> Option<&[u8]> {
        self.resident.get(&(c.src, c.block))?.get(c.start..c.end)
    }

    /// Register every chunk of an already-decompressed block. Split out from [`Self::make_resident`]
    /// so the first-wins and eviction rules are unit-testable without a real archive.
    pub fn register_block_bytes(
        &mut self,
        src: usize,
        block: u16,
        bytes: Arc<Vec<u8>>,
    ) -> ResidentReport {
        if self.resident.contains_key(&(src, block)) {
            return ResidentReport { already_resident: true, ..Default::default() };
        }
        let (count, entries) = parse_block_entry_table(&bytes);
        let mut off = 4 + count as usize * 16;
        let (mut registered, mut shadowed) = (0usize, 0usize);
        for e in &entries {
            let end = match off.checked_add(e.chunk_size as usize) {
                Some(end) if end <= bytes.len() => end,
                // A truncated/oversized entry means the rest of the table can't be trusted either:
                // spans are sequential, so one bad size desynchronises every following offset.
                _ => break,
            };
            match self.chunks.entry((e.type_hash, e.name_hash)) {
                Entry::Vacant(v) => {
                    v.insert(ChunkRef { src, block, start: off, end });
                    registered += 1;
                }
                // FIRST-WINS: retail `FUN_004cc130` returns the occupied cell and creates nothing.
                Entry::Occupied(_) => shadowed += 1,
            }
            off = end;
        }
        self.resident.insert((src, block), bytes);
        self.order.push_back((src, block));
        self.shadowed_total += shadowed;
        let evicted = self.enforce_cap((src, block));
        ResidentReport { already_resident: false, registered, shadowed, evicted }
    }

    /// Decompress `block` from `wads[src]` and register its chunks. No-op if already resident.
    pub fn make_resident(
        &mut self,
        wads: &mut [Wad],
        src: usize,
        block: u16,
    ) -> Result<ResidentReport, String> {
        if self.resident.contains_key(&(src, block)) {
            return Ok(ResidentReport { already_resident: true, ..Default::default() });
        }
        let w = wads.get_mut(src).ok_or_else(|| format!("no wad at src {src}"))?;
        let dec = wad::decompress_block_index(w, block)?;
        Ok(self.register_block_bytes(src, block, Arc::new(dec)))
    }

    /// Drop a block's residency and every chunk it owns. Retail has no such path — it leaks registry
    /// entries and faults on stale handles. We unregister coherently so lookups return `None`.
    pub fn unload_block(&mut self, src: usize, block: u16) -> bool {
        if self.resident.remove(&(src, block)).is_none() {
            return false;
        }
        self.order.retain(|k| *k != (src, block));
        self.chunks.retain(|_, c| !(c.src == src && c.block == block));
        true
    }

    /// Resolve `(type_hash, name_hash)` to a chunk, streaming its block in on demand.
    ///
    /// Resident → return the registered chunk. Not resident → find the owning block via the ASET
    /// table, walking the overlay stack in REVERSE so a patch WAD's row wins (retail's last-opened
    /// rule), make it resident, and look up again. Genuinely absent → `None` (retail: null sentinel).
    ///
    /// Two passes over the stack: first considering only ASET rows whose `type_id` matches the
    /// wanted `type_hash`, then any row for the hash. A hash can own rows of several types
    /// (`0x9FCAE910` is both a type-19 model and a type-27 texture), and some containers are only
    /// reachable through a row of a different type — the untyped pass is what the old
    /// `extract_container_typed` did implicitly.
    pub fn resolve(
        &mut self,
        wads: &mut [Wad],
        type_hash: u32,
        name_hash: u32,
    ) -> Option<ChunkRef> {
        if let Some(c) = self.lookup(type_hash, name_hash) {
            return Some(c);
        }
        let type_id = mercs2_formats::types::type_id_for_type_hash(type_hash);
        for want in [type_id, None] {
            for src in (0..wads.len()).rev() {
                let Some(block) = aset_block(&wads[src], name_hash, want) else { continue };
                if self.make_resident(wads, src, block).is_err() {
                    continue;
                }
                if let Some(c) = self.lookup(type_hash, name_hash) {
                    return Some(c);
                }
            }
            if type_id.is_none() {
                break; // the typed pass was already the untyped pass
            }
        }
        None
    }

    /// Resolve a **singleton** asset class by `type_hash` alone, streaming its block in on demand.
    ///
    /// The name-keyed [`resolve`](Self::resolve) cannot reach `watermap`/`materialtable`: their ASET row
    /// names the resident group that holds them, not the chunk, so there is no name to ask for. Retail
    /// gets them for free — the resident block is always in memory and its entry table is walked by
    /// type. We reproduce that: find the blocks carrying an ASET row of this class's `type_id`
    /// ([`wad::aset_blocks_for_type_id`]), make them resident (which registers every chunk they carry,
    /// exactly as retail's block-streaming does), and take the one row of the wanted type.
    ///
    /// Walks the WAD stack in REVERSE, so an overlay's copy wins — the same last-opened rule as
    /// [`resolve`](Self::resolve). Returns `(name_hash, ChunkRef)`; `None` if no open archive has one.
    pub fn resolve_singleton(
        &mut self,
        wads: &mut [Wad],
        type_hash: u32,
    ) -> Option<(u32, ChunkRef)> {
        if let Some(hit) = self.lookup_singleton(type_hash) {
            return Some(hit);
        }
        let type_id = mercs2_formats::types::type_id_for_type_hash(type_hash)?;
        for src in (0..wads.len()).rev() {
            for block in wad::aset_blocks_for_type_id(&wads[src], type_id) {
                if self.make_resident(wads, src, block).is_err() {
                    continue;
                }
                if let Some(hit) = self.lookup_singleton(type_hash) {
                    return Some(hit);
                }
            }
        }
        None
    }

    /// Evict oldest blocks until under the cap, never evicting `protect`.
    fn enforce_cap(&mut self, protect: (usize, u16)) -> usize {
        let mut evicted = 0;
        while self.resident.len() > self.max_resident {
            let Some(&victim) = self.order.iter().find(|k| **k != protect) else { break };
            self.unload_block(victim.0, victim.1);
            evicted += 1;
        }
        self.evicted_total += evicted;
        evicted
    }
}

/// The block owning `name_hash` in one archive. `want` restricts to rows of that `type_id`; `None`
/// accepts any row. Primary rows (`sub_entry == 0xFFFF`) win over sub-entry rows — 1,236 of the
/// 3,007 model hashes have no primary row at all and ride as sub-entries of another block.
fn aset_block(w: &Wad, name_hash: u32, want: Option<u32>) -> Option<u16> {
    let rows = wad::aset_types(w, name_hash);
    let matches = |t: u32| want.is_none_or(|w| w == t);
    rows.iter()
        .find(|(t, primary, _)| matches(*t) && *primary)
        .or_else(|| rows.iter().find(|(t, _, _)| matches(*t)))
        .map(|(_, _, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic decompressed block: `u32 count` + `count × 16B` entry rows + chunk payloads.
    fn block(chunks: &[(u32, u32, &[u8])]) -> Arc<Vec<u8>> {
        let mut b = Vec::new();
        b.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        for (name, ty, data) in chunks {
            b.extend_from_slice(&name.to_le_bytes());
            b.extend_from_slice(&ty.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes()); // field_c
            b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        }
        for (_, _, data) in chunks {
            b.extend_from_slice(data);
        }
        Arc::new(b)
    }

    const MODEL: u32 = mercs2_formats::types::TYPE_HASH_MODEL;
    const TEX: u32 = mercs2_formats::types::TYPE_HASH_TEXTURE;

    #[test]
    fn registers_every_chunk_and_slices_it_back() {
        let mut r = AssetRegistry::default();
        let rep = r.register_block_bytes(0, 7, block(&[(0xAA, MODEL, b"mesh"), (0xBB, TEX, b"pixels")]));
        assert_eq!((rep.registered, rep.shadowed), (2, 0));

        let m = r.lookup(MODEL, 0xAA).expect("model registered");
        assert_eq!(r.slice(m), Some(&b"mesh"[..]));
        let t = r.lookup(TEX, 0xBB).expect("texture registered");
        assert_eq!(r.slice(t), Some(&b"pixels"[..]));
        // Same name, different type = a different key. md500 owns both 0x9FCAE910 rows.
        assert_eq!(r.lookup(TEX, 0xAA), None);
    }

    /// A singleton class is addressed by TYPE, not by name — the watermap's chunk name hash is an
    /// authored value nobody can predict, so `lookup(type, name)` is unusable for it and
    /// `lookup(type, pandemic_hash("watermap"))` (the type hash reused as a name) finds nothing.
    #[test]
    fn singletons_are_found_by_type_not_by_name() {
        const WATERMAP: u32 = mercs2_formats::types::TYPE_HASH_WATERMAP;
        let mut r = AssetRegistry::default();
        // The resident block: the watermap under an unrelated authored name, plus block-mates.
        r.register_block_bytes(
            0,
            3185,
            block(&[(0xAA, MODEL, b"mesh"), (0x3A03_2A6F, WATERMAP, b"watr-body")]),
        );

        // The old name-keyed path, with the type hash pressed into service as a name: nothing.
        assert_eq!(r.lookup(WATERMAP, WATERMAP), None);
        // By type: found, and it reports the authored name so callers can log it.
        let (name, c) = r.lookup_singleton(WATERMAP).expect("watermap found by type");
        assert_eq!(name, 0x3A03_2A6F);
        assert_eq!(r.slice(c), Some(&b"watr-body"[..]));
        // A class with nothing of that type registered stays absent rather than matching a neighbour.
        assert_eq!(r.lookup_singleton(TEX), None);
    }

    #[test]
    fn insert_is_first_wins_not_last_wins() {
        // Retail FUN_004cc130: an occupied slot returns the existing cell and creates nothing.
        let mut r = AssetRegistry::default();
        r.register_block_bytes(0, 1, block(&[(0xAA, TEX, b"first")]));
        let rep = r.register_block_bytes(0, 2, block(&[(0xAA, TEX, b"second")]));

        assert_eq!((rep.registered, rep.shadowed), (0, 1));
        let c = r.lookup(TEX, 0xAA).unwrap();
        assert_eq!(c.block, 1, "the FIRST block to register the hash keeps the entry");
        assert_eq!(r.slice(c), Some(&b"first"[..]));
        assert_eq!(r.stats().shadowed_total, 1);
    }

    #[test]
    fn eviction_unregisters_the_evicted_blocks_chunks() {
        // Retail leaks these entries and faults on the stale handle (AV 0x47AA5C). We return None.
        let mut r = AssetRegistry::with_capacity(2);
        r.register_block_bytes(0, 1, block(&[(0x01, MODEL, b"a")]));
        r.register_block_bytes(0, 2, block(&[(0x02, MODEL, b"b")]));
        assert!(r.lookup(MODEL, 0x01).is_some());

        let rep = r.register_block_bytes(0, 3, block(&[(0x03, MODEL, b"c")]));
        assert_eq!(rep.evicted, 1);
        assert!(!r.is_resident(0, 1), "oldest block evicted");
        assert_eq!(r.lookup(MODEL, 0x01), None, "its chunks unregistered with it");
        assert!(r.lookup(MODEL, 0x02).is_some());
        assert!(r.lookup(MODEL, 0x03).is_some(), "the block just registered is never the victim");
    }

    #[test]
    fn re_registering_a_resident_block_is_a_no_op() {
        let mut r = AssetRegistry::default();
        r.register_block_bytes(0, 1, block(&[(0xAA, MODEL, b"x")]));
        let rep = r.register_block_bytes(0, 1, block(&[(0xAA, MODEL, b"x")]));
        assert!(rep.already_resident);
        assert_eq!(rep.registered, 0);
        assert_eq!(r.stats().resident_blocks, 1);
    }

    #[test]
    fn a_truncated_entry_stops_the_walk_instead_of_desyncing() {
        // Spans are sequential: one bad size makes every following offset wrong. Stop, don't guess.
        let mut b = Vec::new();
        b.extend_from_slice(&2u32.to_le_bytes());
        for (name, size) in [(0xAAu32, 4u32), (0xBB, 0xFFFF_FFFF)] {
            b.extend_from_slice(&name.to_le_bytes());
            b.extend_from_slice(&MODEL.to_le_bytes());
            b.extend_from_slice(&0u32.to_le_bytes());
            b.extend_from_slice(&size.to_le_bytes());
        }
        b.extend_from_slice(b"good");
        let mut r = AssetRegistry::default();
        let rep = r.register_block_bytes(0, 1, Arc::new(b));
        assert_eq!(rep.registered, 1);
        assert!(r.lookup(MODEL, 0xAA).is_some());
        assert_eq!(r.lookup(MODEL, 0xBB), None);
    }

    #[test]
    fn unload_of_an_absent_block_is_inert() {
        let mut r = AssetRegistry::default();
        assert!(!r.unload_block(0, 99));
    }
}
