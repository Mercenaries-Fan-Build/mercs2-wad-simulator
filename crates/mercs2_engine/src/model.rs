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
    /// Bounds and rig for this rung's geometry (the rig comes from the resident skeleton).
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
            let (vertices, indices, mut draws, stats) =
                mesh::build_indexed_rung(&l.container, res, None)?;
            // The builder sees one container; only here do we know which rung it is. Exporters rely
            // on this to keep the detail levels apart (they all re-author the same HIER nodes).
            for d in &mut draws {
                d.rung = l.level;
            }
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
/// So a coarser draw loses a tier when a finer rung's draws on its node, at that tier, cover what it
/// draws. Covering matters: a finer block can also ADD to a node. `pmcoutpost_interior_hq` carries its
/// walls and floor on node 6 as 75,686 triangles at every tier, and its `P001` puts 2,848 triangles of
/// trim on the same node at tiers 0 and 1. Retail draws both: its render object reads `view_state` 3 in
/// the `23fe7f38_cracked-cruise/pmc` capture, with the walls on screen. No `SEGM` row spans two blocks,
/// and the triangle ratio across the 914 overlapping (node, tier) pairs in retail is a continuum, so
/// the measure is the draws' bounds (INFERRED; retail's own rule for the van is not decoded).
///
/// Baking it into `lod_mask` means every downstream consumer (the draw gate, the workshop, the scene)
/// gets the right answer without knowing rungs exist.
///
/// Geometry bound to no node (`node < 0`) is never superseded — nothing re-authors it.
fn apply_supersede(rungs: &mut [Rung]) {
    let bounds_of = |r: &Rung, d: &DrawGroup| {
        let range = d.index_start as usize..(d.index_start + d.index_count) as usize;
        let mut b = (glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN));
        for &i in &r.indices[range] {
            let p = glam::Vec3::from(r.vertices[i as usize].pos);
            b = (b.0.min(p), b.1.max(p));
        }
        b
    };
    // (node, tier) -> each rung's bounds of what it draws there.
    let mut covered: HashMap<(i16, u8), HashMap<u8, Bounds>> = HashMap::new();
    for r in rungs.iter() {
        for d in r.draws.iter().filter(|d| d.node >= 0) {
            let b = bounds_of(r, d);
            for tier in (0..8u8).filter(|t| d.lod_mask & (1 << t) != 0) {
                let e = covered.entry((d.node, tier)).or_default().entry(r.level).or_insert(b);
                *e = (e.0.min(b.0), e.1.max(b.1));
            }
        }
    }
    let mut cleared: Vec<(usize, usize, u8)> = Vec::new();
    for (ri, r) in rungs.iter().enumerate() {
        for (di, d) in r.draws.iter().enumerate().filter(|(_, d)| d.node >= 0) {
            let b = bounds_of(r, d);
            for tier in (0..8u8).filter(|t| d.lod_mask & (1 << t) != 0) {
                let finer = covered[&(d.node, tier)].iter().filter(|(level, _)| **level > r.level);
                if finer.into_iter().any(|(_, f)| covers(*f, b)) {
                    cleared.push((ri, di, tier));
                }
            }
        }
    }
    for (ri, di, tier) in cleared {
        rungs[ri].draws[di].lod_mask &= !(1 << tier);
    }
}

/// An axis-aligned box: min, max.
type Bounds = (glam::Vec3, glam::Vec3);

/// How much of a coarse draw's extent, per axis, a finer rung must span to stand in for it.
const COVERED_EXTENT: f32 = 0.9;

/// Whether `fine` spans at least [`COVERED_EXTENT`] of `coarse` on every axis.
fn covers(fine: Bounds, coarse: Bounds) -> bool {
    (0..3).all(|k| {
        let extent = coarse.1[k] - coarse.0[k];
        fine.1[k].min(coarse.1[k]) - fine.0[k].max(coarse.0[k]) >= COVERED_EXTENT * extent
    })
}

#[cfg(test)]
mod supersede_tests {
    use super::*;

    fn rung(level: u8, tris: &[(i16, u8, [f32; 3], [f32; 3])]) -> Rung {
        let (mut vertices, mut indices, mut draws) = (Vec::new(), Vec::new(), Vec::new());
        for &(node, lod_mask, lo, hi) in tris {
            let base = vertices.len() as u32;
            let index_start = indices.len() as u32;
            for pos in [lo, hi, [lo[0], hi[1], lo[2]]] {
                vertices.push(Vertex { pos, color: [1.0; 4], uv: [0.0; 2], normal: [0.0, 1.0, 0.0], tangent: [1.0, 0.0, 0.0, 1.0], joints: [0; 4], weights: [255, 0, 0, 0] });
            }
            indices.extend([base, base + 1, base + 2]);
            draws.push(DrawGroup { index_start, index_count: 3, lod_mask, node, ..Default::default() });
        }
        let stats = mesh::ModelStats {
            meshes: 0,
            vertices: 0,
            skipped: 0,
            bbox_min: [0.0; 3],
            bbox_max: [0.0; 3],
            fit_center: [0.0; 3],
            fit_scale: 1.0,
            bones: Vec::new(),
            rig: Vec::new(),
        };
        Rung { level, block: 0, vertices, indices, draws, stats }
    }

    /// The van: the finer block re-authors the same panel, so the coarse one gives up that tier.
    #[test]
    fn a_finer_rung_over_the_same_surface_takes_the_tier() {
        let mut r = [
            rung(0, &[(3, 0x03, [0.0; 3], [2.0, 1.0, 4.0])]),
            rung(1, &[(3, 0x01, [-0.01, 0.0, 0.0], [2.0, 1.01, 3.95])]),
        ];
        apply_supersede(&mut r);
        assert_eq!(r[0].draws[0].lod_mask, 0x02);
        assert_eq!(r[1].draws[0].lod_mask, 0x01);
    }

    /// The PMC hall: a little trim on a node takes nothing from the walls it sits on.
    #[test]
    fn a_finer_rung_adding_to_a_node_takes_nothing() {
        let mut r = [
            rung(0, &[(6, 0x07, [-85.0, 0.0, -88.0], [99.0, 35.0, 41.0])]),
            rung(1, &[(6, 0x03, [10.0, 2.0, 10.0], [14.0, 4.0, 12.0])]),
        ];
        apply_supersede(&mut r);
        assert_eq!(r[0].draws[0].lod_mask, 0x07);
        assert_eq!(r[1].draws[0].lod_mask, 0x03);
    }

    #[test]
    fn a_draw_on_no_node_is_never_superseded() {
        let mut r = [rung(0, &[(-1, 0x01, [0.0; 3], [1.0; 3])]), rung(1, &[(-1, 0x01, [0.0; 3], [1.0; 3])])];
        apply_supersede(&mut r);
        assert_eq!(r[0].draws[0].lod_mask, 0x01);
    }
}
