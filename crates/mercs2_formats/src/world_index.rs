//! Layer 1 — World Block Index (the foundation of the streaming engine).
//!
//! Catalogs EVERY block in `vz.wad`: `block_index`, name (from PTHS), class (from the
//! block's ASET entries / name), LOD tier + variant (name-suffix parse), destruction/faction
//! state overlay (for `vz_state_*`), and spatial extent (grid formula for c3/lrterrain, or the
//! AABB of a placement block's Transform positions). See `docs/modernization/world_streaming_spec.md`
//! §1/§4/§5/§10. This is DATA/format logic — engine-agnostic — so it lives in `mercs2_formats`.
//!
//! It reads directly through `mercs2_formats::ffcs::FfcsArchive` + `&mut File` (the same primitives
//! the engine's `wad` wrapper uses); no dependency on the engine crate is needed. Cheap-extent
//! blocks (c3 / lrterrain, computed purely from the name/grid formula) are indexed eagerly; the
//! placement-block AABBs (which require decompressing + walking COMP records) are computed lazily on
//! first query and cached.
//!
//! Coordinates are ENTIRELY native game space (LEFT-HANDED, +Y up); no flips (spec §"Coordinate space").

use crate::ffcs::FfcsArchive;
use crate::placement::load_placements;
use crate::sges::decompress_block;
use crate::types::{TYPE_ID_ANIMATION, TYPE_ID_MODEL, TYPE_ID_TEXTURE};
use crate::ucfx::parse_block_entry_table;
use std::collections::HashMap;
use std::fs::File;

/// pandemic_hash_m2("model") — the UCFX model-container `type_hash` a c3/model block carries.
pub const MODEL_TYPE_HASH: u32 = 0x5B72_4250;
/// The placement-composite (layer) `type_hash` (`0xE6B81A54`).
pub const LAYER_FORMAT_HASH: u32 = 0xE6B8_1A54;

// -- c3 streaming-cell grid (ported EXACTLY from mercs2_engine::main.rs `load_c3_cells`
//    / mercs2_c3_grid.py GRID_LOGIC_VERSION 3). Anchor: c30123 -> (-2156.25, -3783.75). --
const C3_CELL_ID_BASE: u32 = 30001;
const C3_GRID_COLS: u32 = 100;
const C3_WORLD_MIN: f32 = -3900.0;
const C3_CELL_SIZE: f32 = (3850.0 - C3_WORLD_MIN) / C3_GRID_COLS as f32; // 77.5 m

// -- low_res_terrain tile grid (ported from mercs2_formats::terrain). --
const LRT_GRID: usize = 20;
const LRT_TILE_SPAN_M: f32 = 400.0;
const LRT_ORIGIN_M: f32 = -3800.0;

/// A world-space axis-aligned bounding box (native game space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn from_center_half(cx: f32, cz: f32, half: f32) -> Aabb {
        Aabb {
            min: [cx - half, f32::NEG_INFINITY, cz - half],
            max: [cx + half, f32::INFINITY, cz + half],
        }
    }

    /// XZ overlap between this box and a query disc of `radius` around `(x, z)` (Y ignored —
    /// streaming proximity is horizontal). Treats the disc as its bounding square (cheap + correct
    /// for "blocks near").
    pub fn overlaps_xz(&self, x: f32, z: f32, radius: f32) -> bool {
        x + radius >= self.min[0]
            && x - radius <= self.max[0]
            && z + radius >= self.min[2]
            && z - radius <= self.max[2]
    }

    fn union(&mut self, p: [f32; 3]) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
}

/// Coarse content class of a block, derived from its ASET entries + name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockClass {
    Model,
    C3Cell,
    LayersStatic,
    VzStateOverlay,
    LowResTerrain,
    Texture,
    Animation,
    Other,
}

impl BlockClass {
    pub fn name(&self) -> &'static str {
        match self {
            BlockClass::Model => "Model",
            BlockClass::C3Cell => "C3Cell",
            BlockClass::LayersStatic => "LayersStatic",
            BlockClass::VzStateOverlay => "VzStateOverlay",
            BlockClass::LowResTerrain => "LowResTerrain",
            BlockClass::Texture => "Texture",
            BlockClass::Animation => "Animation",
            BlockClass::Other => "Other",
        }
    }
}

/// LOD-chain information parsed from a block name.
///
/// **On-disk reality (VERIFIED against retail vz.wad — differs from spec §5's literal wording):**
/// every c3 block name LEADS with a `c3####` token; the finer LOD is encoded by how DEEP the
/// hyphen-chain goes, NOT by a separate block named leading with `c2`/`c1`/`c0`. A bare `c30010`
/// is the coarse (c3-only) representation; `c30015-c20105-c11222-c00939` is the fine
/// (down-to-c0) representation of that quadtree path. So:
/// - `tier` = the FINEST tier the chain reaches (the LAST token's tier): 0 (c0, finest) .. 3
///   (c3, coarsest). A bare `c3####` block has tier 3.
/// - `base_cell_id` = the leading c3 streaming-cell id (the anchor for `lod_chain`).
/// - `chain` = the full cell-id chain coarse->fine (c3..c0) as it appears in the name.
/// - `p`/`q` = the `_P###_Q#` variant/quality suffix.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LodInfo {
    pub tier: Option<u8>,
    pub base_cell_id: Option<u32>,
    /// The full chain of cell ids present in the name, coarse->fine (c3..c0), when it is a chain.
    pub chain: Vec<u32>,
    pub p: Option<u16>,
    pub q: Option<u8>,
}

/// A destruction / mission / faction state overlay parsed from a `vz_state_*` name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateOverlay {
    /// Lowercased trailing state token: `pristine`/`destroyed`/`hostiles`/`captured`/`staging`/
    /// `defenses`/`post`/… (None if the name has no recognized trailing state token).
    pub state: Option<String>,
    /// Lowercased leading faction token: `chi`/`pir`/`gur`/`oil`/`all`/`pmc`/`jet`/`mec` (None if
    /// the name does not begin with a recognized faction token).
    pub faction: Option<String>,
    /// The full lowercased body between `vz_state_` and the `_P###_Q#` suffix (the overlay id).
    pub body: String,
}

/// Per-block asset summary: counts by ASET type_id, and the primary model asset hash(es) the block
/// carries (so callers can `extract_container` later — geometry is NOT extracted here).
#[derive(Debug, Clone, Default)]
pub struct AssetSummary {
    /// type_id -> count of ASET entries (primary + sub) owned by this block.
    pub by_type: HashMap<u32, u32>,
    /// Primary model ASET asset hashes owned by this block.
    pub model_hashes: Vec<u32>,
}

/// One catalogued block.
#[derive(Debug, Clone)]
pub struct BlockEntry {
    pub block_index: u16,
    /// Base asset name (PTHS path stripped to the file stem, minus the `_P###_Q#` suffix).
    pub name: String,
    /// Full PTHS path.
    pub path: String,
    pub class: BlockClass,
    pub lod: LodInfo,
    pub state: Option<StateOverlay>,
    /// World-space extent (None for blocks with no natural spatial extent — global textures/anims).
    /// For placement blocks (LayersStatic / VzStateOverlay) this is computed lazily; see
    /// `WorldIndex::extent_of`.
    pub extent: Option<Aabb>,
    pub assets: AssetSummary,
    /// True when this block's `model`-format container was proven present in its entry table (c3
    /// cells and model blocks that actually carry drawable geometry).
    pub has_model_geometry: bool,
}

/// The queryable world block index.
pub struct WorldIndex {
    pub blocks: Vec<BlockEntry>,
    /// 400 low_res_terrain tile extents (block 3121's per-tile squares), row-major.
    pub lrterrain_tiles: Vec<Aabb>,
    /// Block index of the low_res_terrain block, if found.
    pub lrterrain_block: Option<u16>,
    /// Lazily-filled placement-block AABB cache: block_index -> Some(extent) | None (empty/failed).
    placement_extent_cache: HashMap<u16, Option<Aabb>>,
}

// ---- name / LOD / state parsing -------------------------------------------------------------

/// Strip a PTHS path to the file stem (`blocks\VZ\foo_P000_Q3.block` -> `foo_P000_Q3`).
fn path_stem(path: &str) -> &str {
    let after_sep = path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(path);
    after_sep.strip_suffix(".block").unwrap_or(after_sep)
}

/// Parse a `_P###_Q#` suffix from a stem; returns (base_name_without_suffix, p, q).
/// If no suffix is present, returns (stem, None, None).
fn split_pq_suffix(stem: &str) -> (&str, Option<u16>, Option<u8>) {
    // Look for the last `_P` that is followed by digits then `_Q` then digits, at the tail.
    let bytes = stem.as_bytes();
    // find `_Q` near the end
    if let Some(q_rel) = stem.rfind("_Q") {
        let qtail = &stem[q_rel + 2..];
        if !qtail.is_empty() && qtail.bytes().all(|b| b.is_ascii_digit()) {
            // find the `_P###` immediately before it
            let head = &stem[..q_rel];
            if let Some(p_rel) = head.rfind("_P") {
                let ptail = &head[p_rel + 2..];
                if !ptail.is_empty() && ptail.bytes().all(|b| b.is_ascii_digit()) {
                    let p = ptail.parse::<u16>().ok();
                    let q = qtail.parse::<u8>().ok();
                    let _ = bytes;
                    return (&stem[..p_rel], p, q);
                }
            }
        }
    }
    (stem, None, None)
}

/// Parse the leading `c#####` cell tokens of a stem into (tier, cell_id) pairs.
/// A c3-chain name looks like `c30015-c20105-c11222-c00939`; a bare cell is `c30010`.
/// The token is `c` + tier-digit + 4 digits (c3->cell, c2/c1/c0->refined ids).
fn parse_cell_chain(base: &str) -> Vec<(u8, u32)> {
    let mut out = Vec::new();
    for tok in base.split(|c| c == '-' || c == '_') {
        let b = tok.as_bytes();
        if b.len() >= 6
            && (b[0] == b'c' || b[0] == b'C')
            && (b[1] == b'0' || b[1] == b'1' || b[1] == b'2' || b[1] == b'3')
            && b[2..6].iter().all(|c| c.is_ascii_digit())
        {
            let tier = (b[1] - b'0') as u8;
            if let Ok(n) = tok[2..6].parse::<u32>() {
                // Reconstruct the full numeric id from the "cTNNNN" form: e.g. c30015 -> 30015.
                let id = (tier as u32) * 10000 + n;
                out.push((tier, id));
            }
        } else {
            // stop at the first non-cell token so we don't misparse arbitrary words
            break;
        }
    }
    out
}

/// c3 streaming-cell id (e.g. 30123) from the c3 grid slot; matches `load_c3_cells`.
/// The name id `c30123` == slot; cell id = base-1 + slot-suffix. Here the numeric id
/// already IS `30123` when the token is `c30123`, so we map slot -> cell id.
fn c3_cell_id_from_c3_numeric(c3_numeric: u32) -> u32 {
    // c3_numeric is e.g. 30123 (from token c30123). The slot suffix is the last 4 digits (0123).
    let slot = c3_numeric % 10000;
    C3_CELL_ID_BASE - 1 + slot
}

/// Game-space (x, z) centre of a c3 streaming cell (metres). Grid carries no height.
pub fn c3_cell_centre(cell_id: u32) -> (f32, f32) {
    let linear = cell_id.saturating_sub(C3_CELL_ID_BASE);
    let (row, col) = (linear / C3_GRID_COLS, linear % C3_GRID_COLS);
    let x = C3_WORLD_MIN + (col as f32 + 0.5) * C3_CELL_SIZE;
    let z = C3_WORLD_MIN + (row as f32 + 0.5) * C3_CELL_SIZE;
    (x, z)
}

/// low_res_terrain tile centre for grid (row, col).
fn lrterrain_tile_centre(row: usize, col: usize) -> (f32, f32) {
    (
        LRT_ORIGIN_M + col as f32 * LRT_TILE_SPAN_M,
        LRT_ORIGIN_M + row as f32 * LRT_TILE_SPAN_M,
    )
}

const FACTIONS: &[&str] = &["chi", "pir", "gur", "oil", "all", "pmc", "jet", "mec"];
const STATE_TOKENS: &[&str] = &[
    "pristine",
    "destroyed",
    "hostiles",
    "captured",
    "staging",
    "defenses",
    "post",
    "precrash",
    "traffic",
    "deliverables",
];

/// Parse a `vz_state_*` base name into a StateOverlay.
fn parse_vz_state(base: &str) -> StateOverlay {
    let body = base
        .strip_prefix("vz_state_")
        .or_else(|| base.strip_prefix("VZ_STATE_"))
        .unwrap_or(base)
        .to_lowercase();
    // faction = leading token if it begins with a known faction prefix.
    let faction = FACTIONS
        .iter()
        .find(|f| body.starts_with(*f))
        .map(|f| f.to_string());
    // state = trailing `_token` if that token is a known state word.
    let state = body
        .rsplit('_')
        .next()
        .filter(|t| STATE_TOKENS.contains(t))
        .map(|t| t.to_string());
    StateOverlay {
        state,
        faction,
        body,
    }
}

// ---- classification --------------------------------------------------------------------------

/// Derive the block class from the base name + the ASET type histogram + whether the block's entry
/// table carries a model-format container.
fn derive_class(
    base: &str,
    by_type: &HashMap<u32, u32>,
    is_c3: bool,
    has_model_geometry: bool,
    is_lrterrain: bool,
    is_layers_static: bool,
    is_vz_state: bool,
) -> BlockClass {
    if is_lrterrain {
        return BlockClass::LowResTerrain;
    }
    if is_layers_static {
        return BlockClass::LayersStatic;
    }
    if is_vz_state {
        return BlockClass::VzStateOverlay;
    }
    if is_c3 {
        return BlockClass::C3Cell;
    }
    // Fall back to the dominant ASET type of the block.
    let model = *by_type.get(&TYPE_ID_MODEL).unwrap_or(&0);
    let texture = *by_type.get(&TYPE_ID_TEXTURE).unwrap_or(&0);
    let anim = *by_type.get(&TYPE_ID_ANIMATION).unwrap_or(&0);
    if model > 0 || has_model_geometry {
        return BlockClass::Model;
    }
    if texture > 0 && texture >= anim {
        return BlockClass::Texture;
    }
    if anim > 0 {
        return BlockClass::Animation;
    }
    let _ = base;
    BlockClass::Other
}

impl WorldIndex {
    /// Build the full block index from an FFCS archive + its backing file. Cheap-extent blocks
    /// (c3/lrterrain, by name/grid formula) get extents eagerly; placement-block AABBs are computed
    /// lazily on first `extent_of`/`blocks_near` query and cached.
    pub fn build(archive: &FfcsArchive, file: &mut File) -> WorldIndex {
        // 1) ASET histogram + model hashes, keyed by owning block index.
        let mut by_block_types: HashMap<u16, HashMap<u32, u32>> = HashMap::new();
        let mut by_block_models: HashMap<u16, Vec<u32>> = HashMap::new();
        for e in &archive.aset {
            let blk = e.block_index();
            *by_block_types
                .entry(blk)
                .or_default()
                .entry(e.type_id)
                .or_insert(0) += 1;
            if e.type_id == TYPE_ID_MODEL && e.is_primary() {
                by_block_models.entry(blk).or_default().push(e.asset_hash);
            }
        }

        let n = archive.indx.len();
        let mut blocks: Vec<BlockEntry> = Vec::with_capacity(n);

        // Identify the low_res_terrain block (by name; verified by ASET type later).
        let mut lrterrain_block: Option<u16> = None;

        for block_index in 0..n as u16 {
            let path = archive
                .paths
                .get(block_index as usize)
                .cloned()
                .unwrap_or_default();
            let stem = path_stem(&path);
            let (base, p, q) = split_pq_suffix(stem);
            let base_lc = base.to_lowercase();

            let by_type = by_block_types.get(&block_index).cloned().unwrap_or_default();
            let model_hashes = by_block_models.get(&block_index).cloned().unwrap_or_default();

            // Name-driven flags.
            let is_lrterrain = base_lc.contains("low_res_terrain");
            let is_layers_static = base_lc.contains("layers_static");
            let is_vz_state = base_lc.starts_with("vz_state") || base_lc.contains("vz_state_");

            // c3 cell? leading token is `c3####`.
            let cells = parse_cell_chain(base);
            let is_c3 = cells.first().map(|(t, _)| *t == 3).unwrap_or(false)
                && !is_vz_state
                && !is_layers_static
                && !is_lrterrain;

            // Model geometry present? cheap check via ASET model type OR (for c3) entry-table probe.
            // We avoid decompressing here except a cheap head-peek for c3 blocks (which may carry
            // baked model geometry without a model-type ASET row).
            let mut has_model_geometry = *by_type.get(&TYPE_ID_MODEL).unwrap_or(&0) > 0;
            if is_c3 && !has_model_geometry {
                if let Ok(head) = crate::sges::decompress_block_head(
                    file,
                    &archive.indx,
                    block_index,
                    16384,
                ) {
                    let (_c, entries) = parse_block_entry_table(&head);
                    has_model_geometry =
                        entries.iter().any(|e| e.type_hash == MODEL_TYPE_HASH);
                }
            }

            let class = derive_class(
                &base_lc,
                &by_type,
                is_c3,
                has_model_geometry,
                is_lrterrain,
                is_layers_static,
                is_vz_state,
            );

            if class == BlockClass::LowResTerrain && lrterrain_block.is_none() {
                lrterrain_block = Some(block_index);
            }

            // LOD info.
            let mut lod = LodInfo {
                p,
                q,
                ..Default::default()
            };
            if !cells.is_empty() {
                // tier = the FINEST tier the chain reaches (last token). Bare c3#### => tier 3.
                lod.tier = Some(cells.last().unwrap().0);
                lod.chain = cells.iter().map(|(_, id)| *id).collect();
                // base cell id = the c3 (tier-3) id if present, else the first token.
                lod.base_cell_id = cells
                    .iter()
                    .find(|(t, _)| *t == 3)
                    .map(|(_, id)| c3_cell_id_from_c3_numeric(*id))
                    .or_else(|| Some(c3_cell_id_from_c3_numeric(cells[0].1)));
            }

            // State overlay for vz_state.
            let state = if is_vz_state {
                Some(parse_vz_state(base))
            } else {
                None
            };

            // Eager extent for c3 (grid square). Placement/lrterrain extents handled elsewhere.
            let extent = if is_c3 {
                lod.base_cell_id.map(|cid| {
                    let (cx, cz) = c3_cell_centre(cid);
                    Aabb::from_center_half(cx, cz, C3_CELL_SIZE * 0.5)
                })
            } else {
                None
            };

            let assets = AssetSummary {
                by_type,
                model_hashes,
            };

            blocks.push(BlockEntry {
                block_index,
                name: base.to_string(),
                path,
                class,
                lod,
                state,
                extent,
                assets,
                has_model_geometry,
            });
        }

        // Precompute the 400 lrterrain tile extents (block 3121's per-tile squares).
        let mut lrterrain_tiles = Vec::with_capacity(LRT_GRID * LRT_GRID);
        for row in 0..LRT_GRID {
            for col in 0..LRT_GRID {
                let (cx, cz) = lrterrain_tile_centre(row, col);
                lrterrain_tiles.push(Aabb::from_center_half(
                    cx,
                    cz,
                    LRT_TILE_SPAN_M * 0.5,
                ));
            }
        }

        WorldIndex {
            blocks,
            lrterrain_tiles,
            lrterrain_block,
            placement_extent_cache: HashMap::new(),
        }
    }

    /// Number of catalogued blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Look up a block by index.
    pub fn block(&self, block_index: u16) -> Option<&BlockEntry> {
        self.blocks.get(block_index as usize)
    }

    /// All blocks of a class.
    pub fn by_class(&self, class: BlockClass) -> impl Iterator<Item = &BlockEntry> {
        self.blocks.iter().filter(move |b| b.class == class)
    }

    /// The 4 LOD tiers (index 3=c3 coarsest .. 0=c0 finest) of the c3 streaming cell `base_cell_id`.
    /// A block fills slot `tier` (= the finest tier its name-chain reaches) when its leading c3 id
    /// resolves to `base_cell_id`. Bare-cell blocks (`c3####`) fill slot 3; a
    /// `c3####-c2###-c1###-c0###` chain fills slot 0. (First match per slot wins.)
    pub fn lod_chain(&self, base_cell_id: u32) -> [Option<&BlockEntry>; 4] {
        let mut out: [Option<&BlockEntry>; 4] = [None, None, None, None];
        for b in &self.blocks {
            if b.lod.base_cell_id == Some(base_cell_id) {
                if let Some(t) = b.lod.tier {
                    let ti = t as usize;
                    if ti < 4 && out[ti].is_none() {
                        out[ti] = Some(b);
                    }
                }
            }
        }
        out
    }

    /// vz_state overlays matching a faction and/or state (both optional; None = any).
    pub fn overlays_for_state<'a>(
        &'a self,
        faction: Option<&'a str>,
        state: Option<&'a str>,
    ) -> impl Iterator<Item = &'a BlockEntry> + 'a {
        let f = faction.map(|s| s.to_lowercase());
        let s = state.map(|s| s.to_lowercase());
        self.blocks.iter().filter(move |b| {
            let Some(ov) = &b.state else { return false };
            f.as_ref().map_or(true, |ff| ov.faction.as_deref() == Some(ff.as_str()))
                && s.as_ref().map_or(true, |ss| ov.state.as_deref() == Some(ss.as_str()))
        })
    }

    /// Resolve a block's spatial extent, computing (and caching) placement-block AABBs on demand.
    /// c3/lrterrain extents are already eager. Placement blocks (LayersStatic / VzStateOverlay)
    /// decompress + min/max their Transform positions here. `file`/`archive` are needed only for the
    /// lazy placement path; if the block already has an eager extent this ignores them.
    pub fn extent_of(
        &mut self,
        block_index: u16,
        archive: &FfcsArchive,
        file: &mut File,
    ) -> Option<Aabb> {
        let (class, eager) = {
            let b = self.blocks.get(block_index as usize)?;
            (b.class, b.extent)
        };
        if let Some(e) = eager {
            return Some(e);
        }
        if !matches!(class, BlockClass::LayersStatic | BlockClass::VzStateOverlay) {
            return None;
        }
        if let Some(cached) = self.placement_extent_cache.get(&block_index) {
            return *cached;
        }
        let extent = compute_placement_extent(archive, file, block_index);
        self.placement_extent_cache.insert(block_index, extent);
        if let Some(b) = self.blocks.get_mut(block_index as usize) {
            b.extent = extent;
        }
        extent
    }

    /// Blocks whose extent overlaps a disc of `radius` around `(x, z)`. Placement-block AABBs are
    /// computed + cached lazily as they are tested (pass `archive`/`file` for that). c3/lrterrain
    /// extents are already resolved. Returns owned indices to avoid borrow conflicts with the lazy
    /// compute; call `block(idx)` for the entry.
    pub fn blocks_near(
        &mut self,
        x: f32,
        z: f32,
        radius: f32,
        archive: &FfcsArchive,
        file: &mut File,
    ) -> Vec<u16> {
        let indices: Vec<u16> = self.blocks.iter().map(|b| b.block_index).collect();
        let mut out = Vec::new();
        for bi in indices {
            let ext = self.extent_of(bi, archive, file);
            if let Some(e) = ext {
                if e.overlaps_xz(x, z, radius) {
                    out.push(bi);
                }
            }
        }
        out
    }
}

/// Decompress a placement block and return the AABB of its Transform positions (None if it has no
/// placements or decompression fails).
fn compute_placement_extent(
    archive: &FfcsArchive,
    file: &mut File,
    block_index: u16,
) -> Option<Aabb> {
    let dec = decompress_block(file, &archive.indx, block_index).ok()?;
    let placements = load_placements(&dec).ok()?;
    let mut it = placements.iter();
    let first = it.next()?;
    let mut aabb = Aabb {
        min: first.pos,
        max: first.pos,
    };
    for p in it {
        aabb.union(p.pos);
    }
    Some(aabb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c3_grid_anchor() {
        // Spec §4 anchor: c30123 -> (-2156.25, -3783.75).
        let cid = c3_cell_id_from_c3_numeric(30123); // token c30123
        let (x, z) = c3_cell_centre(cid);
        assert!((x - (-2156.25)).abs() < 0.01, "x={x}");
        assert!((z - (-3783.75)).abs() < 0.01, "z={z}");
    }

    #[test]
    fn c3_cell_size_is_77_5() {
        assert!((C3_CELL_SIZE - 77.5).abs() < 1e-4);
    }

    #[test]
    fn split_pq_suffix_parses() {
        let (base, p, q) = split_pq_suffix("c30010_P000_Q3");
        assert_eq!(base, "c30010");
        assert_eq!(p, Some(0));
        assert_eq!(q, Some(3));
        // no suffix
        let (base2, p2, q2) = split_pq_suffix("resident");
        assert_eq!(base2, "resident");
        assert_eq!(p2, None);
        assert_eq!(q2, None);
    }

    #[test]
    fn parse_bare_c3_cell() {
        let cells = parse_cell_chain("c30010");
        assert_eq!(cells, vec![(3u8, 30010u32)]);
    }

    #[test]
    fn parse_full_c3_chain() {
        let cells = parse_cell_chain("c30015-c20105-c11222-c00939");
        assert_eq!(
            cells,
            vec![(3, 30015), (2, 20105), (1, 11222), (0, 939)]
        );
        // Finest tier reached = last token's tier (0 = c0). Leading c3 id anchors the cell.
        assert_eq!(cells.last().unwrap().0, 0);
        assert_eq!(cells.first().unwrap().0, 3);
    }

    #[test]
    fn parse_vz_state_faction_and_state() {
        let ov = parse_vz_state("vz_state_ChiCon002_Bridge_Destroyed");
        assert_eq!(ov.faction.as_deref(), Some("chi"));
        assert_eq!(ov.state.as_deref(), Some("destroyed"));
        assert_eq!(ov.body, "chicon002_bridge_destroyed");

        let ov2 = parse_vz_state("vz_state_OilJob001_Pristine");
        assert_eq!(ov2.faction.as_deref(), Some("oil"));
        assert_eq!(ov2.state.as_deref(), Some("pristine"));
    }

    #[test]
    fn path_stem_strips() {
        assert_eq!(path_stem("blocks\\VZ\\c30010_P000_Q3.block"), "c30010_P000_Q3");
        assert_eq!(path_stem("blocks/vz/resident_P000_Q3.block"), "resident_P000_Q3");
    }

    #[test]
    fn aabb_overlap_xz() {
        let a = Aabb::from_center_half(100.0, 200.0, 40.0); // [60..140] x [160..240]
        assert!(a.overlaps_xz(0.0, 0.0, 300.0));
        assert!(a.overlaps_xz(120.0, 220.0, 1.0));
        assert!(!a.overlaps_xz(1000.0, 1000.0, 50.0));
    }

    #[test]
    fn lrterrain_tile_grid() {
        // spec §4: center = (-3800 + col*400, -3800 + row*400).
        assert_eq!(lrterrain_tile_centre(0, 0), (-3800.0, -3800.0));
        assert_eq!(lrterrain_tile_centre(19, 19), (3800.0, 3800.0));
    }
}
