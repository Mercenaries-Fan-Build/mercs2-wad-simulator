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
use std::collections::HashMap;
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
    /// Bounds/rig/prelit for this rung's geometry (the rig comes from the resident skeleton).
    pub stats: crate::mesh::ModelStats,
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
            let (vertices, indices, draws, stats) =
                mesh::build_indexed_rung(&l.container, res, None)?;
            rungs.push(Rung { level: l.level, block: l.block, vertices, indices, draws, stats });
        }
        apply_supersede(&mut rungs);

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
    /// The rungs REFINE each other, they do not sum. The resident block is a complete, self-contained
    /// low-detail model covering every tier — the fallback for an object whose finer blocks haven't
    /// streamed — and each finer block SUPERSEDES it for the nodes it re-authors. The car van's body
    /// (node 3) exists as 736 triangles in the resident block and 9,360 in `P001`; drawing both puts
    /// two detail levels of the same panel in the same space.
    ///
    /// So selection is **per node, per tier: the finest loaded rung that covers it wins**, and only
    /// then do the gate's clauses apply. This mirrors the texture chain exactly — a finer mip block
    /// replaces the top of the resident chain rather than adding to it (see
    /// [`crate::wad::extract_texture_hires`]).
    ///
    /// A rung whose block hasn't streamed simply isn't in `rungs`, and the coarser version takes over
    /// on its own — which is what the game shows you as an object recedes.
    pub fn visible_draws<'a>(&'a self, state: &RenderState) -> Vec<(&'a Rung, &'a DrawGroup)> {
        let mut out = Vec::new();
        for r in &self.rungs {
            for d in r.draws.iter().filter(|d| state.segment_visible(d.lod_mask, d.node)) {
                out.push((r, d));
            }
        }
        out
    }

    /// Total triangles across the whole chain (all rungs) — the "raid array" reassembled.
    pub fn triangles(&self) -> u32 {
        self.rungs.iter().map(|r| r.triangles()).sum()
    }

    /// Flatten every rung into ONE vertex/index buffer with rebased draw groups — the shape both the
    /// renderer and the workshop want to upload. Masks already carry the supersede resolution, so a
    /// consumer just runs the normal draw gate and gets the right tier's geometry.
    pub fn flatten(&self) -> (Vec<Vertex>, Vec<u32>, Vec<DrawGroup>, mesh::ModelStats) {
        let (mut verts, mut indices, mut draws) = (Vec::new(), Vec::new(), Vec::new());
        let mut stats: Option<mesh::ModelStats> = None;
        for r in &self.rungs {
            let (vbase, ibase) = (verts.len() as u32, indices.len() as u32);
            verts.extend_from_slice(&r.vertices);
            indices.extend(r.indices.iter().map(|x| x + vbase));
            draws.extend(r.draws.iter().cloned().map(|mut g| {
                g.index_start += ibase;
                g
            }));
            match &mut stats {
                Some(s) => s.absorb(&r.stats),
                None => stats = Some(r.stats.clone()),
            }
        }
        let mut stats = stats.unwrap_or_else(|| self.rungs[0].stats.clone());
        stats.vertices = verts.len();
        (verts, indices, draws, stats)
    }
}

/// Resolve the overlap between LOD rungs, by clearing tier bits a finer block has taken over.
///
/// The rungs REFINE each other, they do not sum. The resident block is a complete, self-contained
/// low-detail model spanning every tier — the fallback for an object whose finer blocks haven't
/// streamed in — and each finer block RE-AUTHORS some of those nodes at the near tiers. The car
/// van's body sits on node 3 as **736 triangles in the resident block and 9,360 in `P001`**, both
/// masked for tier 0. Draw both and you get two detail levels of the same panel fighting for the
/// same pixels; on that car it was 11,604 of 19,107 triangles drawn twice.
///
/// So for each (node, tier), only the FINEST rung that covers it survives — the coarser block's bit
/// for that tier is cleared. Baking it into `lod_mask` means every downstream consumer (the draw
/// gate, the workshop, the scene) gets the right answer without knowing rungs exist. This mirrors
/// the texture chain: a finer mip block replaces the top of the resident chain rather than adding
/// to it (see [`crate::wad::extract_texture_hires`]).
///
/// Geometry bound to no node (`node < 0`) is never superseded — nothing re-authors it.
fn apply_supersede(rungs: &mut [Rung]) {
    // (node, tier) -> finest rung level that carries it.
    let mut finest: HashMap<(i16, u8), u8> = HashMap::new();
    for r in rungs.iter() {
        for d in r.draws.iter().filter(|d| d.node >= 0) {
            for tier in 0..8u8 {
                if d.lod_mask & (1 << tier) != 0 {
                    let e = finest.entry((d.node, tier)).or_insert(r.level);
                    *e = (*e).max(r.level);
                }
            }
        }
    }
    for r in rungs.iter_mut() {
        let level = r.level;
        for d in r.draws.iter_mut().filter(|d| d.node >= 0) {
            for tier in 0..8u8 {
                let bit = 1u8 << tier;
                if d.lod_mask & bit != 0
                    && finest.get(&(d.node, tier)).copied().unwrap_or(level) > level
                {
                    d.lod_mask &= !bit;
                }
            }
        }
    }
}
