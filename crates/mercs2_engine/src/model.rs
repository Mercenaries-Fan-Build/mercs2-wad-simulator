//! A model, assembled from the parts the WAD scatters across blocks.
//!
//! A vehicle is not one container. Its geometry is split across a chain of blocks named
//! `<model>_P00N_Q(3-N)`, coarsest first — the same c3 LOD-block scheme that carries a texture's
//! higher mips (see [`crate::wad::extract_texture_hires`]). Only the RESIDENT (coarsest) block ships
//! the object itself:
//!
//! | chunk | resident `P000` | streamed `P001` / `P002` |
//! |---|---|---|
//! | `HIER` skeleton, `SEGM` segments, `MTRL` materials, `PHY2`, `SWIT`/`CEXE` destruction machine | yes | **absent** |
//! | `GEOM`/`MESH`/`PRMG`/`PRMT`/`IBUF` geometry, `INDX` | yes (coarse meshes) | yes (finer meshes) |
//!
//! So the object is `geometry(rung) x INDX(rung) x SEGM/HIER/MTRL/machine(resident)`. The resident
//! `SEGM` is the **master segment table for the whole chain** — `ch_veh_tank_ztz98` has 130 records
//! serving 12 coarse + 35 + 63 groups — and each rung's `INDX[group]` names a row in it. Resolving a
//! rung against its own (missing) SEGM is what made every vehicle render as a low-poly proxy: the
//! tank's resident block holds 4,435 triangles, its `_P002_Q1` block holds 28,620.
//!
//! The LOD masks partition cleanly once joined — resident owns rungs 4-6, `P001` owns 2-3, `P002`
//! owns 0-1 — so the three-clause draw gate needs no special case: `view_state` picks the rung,
//! `node_enable` does destruction. Characters ship a single block and no chain.

use crate::mesh::{self, DrawGroup, Vertex};
use crate::render_state::RenderState;
use crate::wad::{self, Wad};
use mercs2_formats::model_cubeize::{ModelHeader, SegRec};
use mercs2_formats::orchestrator::{HierNode, StateMachine};

/// One rung of the chain: the geometry from a single block, already bound to the resident skeleton.
pub struct Rung {
    /// `0` = resident/coarsest, higher = finer (the `N` in `_P00N_`).
    pub level: u8,
    pub block: u16,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Draw groups, each carrying the node + LOD mask from the RESIDENT `SEGM` record its `INDX`
    /// row named.
    pub draws: Vec<DrawGroup>,
}

impl Rung {
    /// The LOD rungs this block's geometry actually serves (union of its groups' masks).
    pub fn lod_bits(&self) -> u8 {
        self.draws.iter().fold(0u8, |a, d| a | d.lod_mask)
    }

    pub fn triangles(&self) -> u32 {
        self.draws.iter().map(|d| d.index_count / 3).sum()
    }
}

/// The whole object: the resident block's identity, plus every geometry rung the WAD ships for it.
pub struct Model {
    pub name_hash: u32,
    /// The resident container — the authority for skeleton, segments, materials, physics, machine.
    pub resident: Vec<u8>,
    pub header: Option<ModelHeader>,
    pub hier: Vec<HierNode>,
    /// Master segment table: every rung's `INDX` indexes THIS.
    pub segm: Vec<SegRec>,
    pub machine: Option<StateMachine>,
    /// Coarsest first.
    pub rungs: Vec<Rung>,
}

impl Model {
    /// Load and assemble a model by asset hash: walk its LOD-block chain, bind every rung's groups
    /// through the resident `SEGM`/`HIER`/`MTRL`.
    pub fn load(wad: &mut Wad, name_hash: u32) -> Result<Model, String> {
        let lods = wad::extract_model_lods(wad, name_hash)?;
        let resident = lods[0].container.clone();

        let mut rungs = Vec::new();
        for l in &lods {
            // The resident rung binds against itself; finer rungs borrow its SEGM/HIER/MTRL.
            let res = if l.level == lods[0].level { None } else { Some(resident.as_slice()) };
            let (vertices, indices, draws, _) = mesh::build_indexed_rung(&l.container, res, None)?;
            rungs.push(Rung { level: l.level, block: l.block, vertices, indices, draws });
        }

        Ok(Model {
            name_hash,
            header: mercs2_formats::model_cubeize::parse_model_header(&resident),
            hier: mercs2_formats::orchestrator::parse_hier(&resident),
            segm: mercs2_formats::model_cubeize::parse_segm(&resident),
            machine: mercs2_formats::orchestrator::parse_state_machine(&resident),
            rungs,
            resident,
        })
    }

    /// LOD rung count from the model header (`+0x34`), i.e. `maxLOD`.
    pub fn lod_count(&self) -> u32 {
        self.header.as_ref().map(|h| h.lod_count).unwrap_or(1)
    }

    /// Every draw group in the object that survives the render gate, as `(rung, group)`.
    ///
    /// There is no "pick the right rung" step — that would be a rule the engine doesn't have. Once a
    /// block is resident its segments join one pool, and the per-segment gate does the selecting:
    /// clause 2 (`view_state` vs the segment's LOD mask) and clause 3 (`node_enable`, the destruction
    /// machine). The masks partition the tiers across the chain precisely so this works — on the tank
    /// the resident block's segments claim rungs 4-6, `P001` claims 2-3, `P002` claims 0-1, and the
    /// handful of all-tier (`0x7f`) segments draw at every tier from whichever block ships them.
    ///
    /// A rung whose block isn't loaded simply contributes nothing, which is exactly what happens in
    /// the game when an object is too far away to have streamed its fine geometry.
    pub fn visible_draws<'a>(
        &'a self,
        state: &'a RenderState,
    ) -> impl Iterator<Item = (&'a Rung, &'a DrawGroup)> + 'a {
        self.rungs.iter().flat_map(move |r| {
            r.draws
                .iter()
                .filter(move |d| state.segment_visible(d.lod_mask, d.node))
                .map(move |d| (r, d))
        })
    }

    /// Total triangles across the whole chain (all rungs) — the "raid array" reassembled.
    pub fn triangles(&self) -> u32 {
        self.rungs.iter().map(|r| r.triangles()).sum()
    }
}
