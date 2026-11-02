//! **Tree foliage** (`foliage`, UCFX `type_hash` `0x34612F86`, ASET `type_id` 20): the one chunk that
//! places every tree of a level. A species is a world template, a billboard, and a quadtree of positions.
//!
//! # Container layout
//!
//! One leaf row, `tree`, holding `{u32 version, u32 species_count}` and then the species back to back:
//!
//! ```text
//! u32 template            world template key                                        batch +0x6d0
//! u32 atlas_columns, u32 atlas_rows   billboard tiles                               batch +0x6e4/+0x6e8
//! f32 authored_range      entity range; the loader stores 0.75 of it                batch +0x6d4
//! u8  flag                                                                          batch +0x6ec
//! f32 × 3                                                                           batch +0x6f0..
//! u32 × 25                billboard basis; word 24 is its view count                batch +0x650..+0x6b0
//! u32 × 2                                                                           batch +0x6dc/+0x6e0
//! u32 vertex_count, u32 index_count
//! vertex_count × 20 B     {f32 × 3, f32 × 2}       present when both counts are non-zero
//! index_count × u16                                present when both counts are non-zero
//! material                modelled billboard's     present when both counts are non-zero
//! material                billboard's
//! u32 bit_bytes, bytes    the quadtree's bit stream, padded to 4
//! u32 bit_count
//! f32 × 6                 root box
//! u32 × 8                 root node
//! u32 × 63, u32 × 3, u32 × 21    the tree's per-level tables
//! ```
//!
//! The order is the retail loader's: `FUN_004a0c40` reads everything up to the materials, and
//! `FUN_004a01e0` the tree.
//!
//! # The position tree
//!
//! A balanced binary tree packed into the bit stream in depth-first order, read through `FUN_0049fd70`
//! (one split) and `FUN_0049f700` (one leaf). Bits run most-significant first in little-endian `u32`s.
//!
//! A split at depth `d` is one quantized plane: `axis = axes[d]`, `q = read(leaf_words[axis])`, and with
//! `step = extent[axis] / (2^width − 1)` the first child's `max[axis]` is `min[axis] + (q + 1)·step` and
//! the second's `min[axis]` is `min[axis] + q·step`, so the children overlap by one step. The first
//! child follows the split in the stream. The second follows the first's whole subtree, which is why a
//! node carries the counts that size it, split between its children with the odd one going to the first
//! or second as [`Tree::leaves`] shows:
//!
//! ```text
//! root[1]  leaves under the node
//! root[2]  of those, leaves on the shallower of the two leaf levels
//! root[3]  splits on the last split level
//! root[4]  bits of the splits above that level
//! root[5..8]  bits of a deep leaf, of a shallow leaf, of a last-level split
//! ```
//!
//! A node is a leaf when `root[3] + root[4]` have reached 0. A leaf is
//! `{4 bits flags, x, y, z, 4 bits yaw}`, with `x`, `y` and `z` as wide as `levels` says for its depth,
//! quantized to the box the splits above it left: `min + v · extent / (2^width − 1)`. `FUN_004a2ce0`
//! counts the leaves, and retail's `+0x00` holds that count plus 1.
//!
//! Every position and yaw of the 112 tree entities in two retail captures is a leaf of its species
//! (`mercs2_probe foliage-check`), within 0.3 mm.
//!
//! # Drawing
//!
//! A tree within [`Species::range`] of a viewer is an entity of [`Species::template`] (`FUN_004a2ef0`).
//! The rest are `PgBillboardTreeVP`: each of the species' [`Species::views`] is a quad turned about +Y to
//! face the camera, and the view angle less the tree's yaw picks two neighbouring atlas tiles to blend.

use crate::terrainmesh::{decode_material, f32_at, leaf, parse_tree, u16_at, u32_at, Material};
use crate::texture::MtrlMaterial;

/// UCFX `type_hash` of the `foliage` chunk.
pub const TYPE_HASH: u32 = 0x3461_2F86;

/// `FUN_004a0c40`'s scale on the authored range.
const RANGE_SCALE: f32 = 0.75;
const BASIS_WORDS: usize = 25;

/// A decoded `foliage` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Foliage {
    pub version: u32,
    pub species: Vec<Species>,
}

/// One tree species.
#[derive(Debug, Clone, PartialEq)]
pub struct Species {
    /// World template of the entity a tree in range becomes.
    pub template: u32,
    /// Billboard atlas tiles, across and down.
    pub atlas: [u32; 2],
    /// The range as stored; [`Self::range`] is what the game uses.
    pub authored_range: f32,
    pub flag: u8,
    pub params: [f32; 3],
    pub basis: [u32; BASIS_WORDS],
    pub tail_words: [u32; 2],
    /// The modelled billboard, when the species has one.
    pub modelled: Option<Modelled>,
    pub material: Material,
    pub tree: Tree,
}

/// A species' billboard mesh: 20-byte vertices, indices, and its material.
#[derive(Debug, Clone, PartialEq)]
pub struct Modelled {
    pub vertices: Vec<[f32; 5]>,
    pub indices: Vec<u16>,
    pub material: Material,
}

/// A species' position tree exactly as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    pub bits: Vec<u8>,
    pub bit_count: u32,
    pub root_min: [f32; 3],
    pub root_max: [f32; 3],
    pub root: [u32; 8],
    pub levels: Vec<u32>,
    pub leaf_words: [u32; 3],
    pub axes: Vec<u32>,
}

impl Species {
    /// Distance at which a tree is a full entity rather than a billboard.
    pub fn range(&self) -> f32 {
        self.authored_range * RANGE_SCALE
    }

    /// The billboard's quads. Up to 3: `basis` holds 3 rectangles `{x0, y0, x1, y1}` of offsets, then 3
    /// of texture coordinates `{u1, v0, u0, v1}`, then their count.
    pub fn views(&self) -> Vec<BillboardView> {
        let word = |i: usize| f32::from_bits(self.basis[i]);
        (0..self.basis[BASIS_WORDS - 1] as usize)
            .map(|k| {
                let [x0, y0, x1, y1] = [0, 1, 2, 3].map(|i| word(k * 4 + i));
                let [u1, v0, u0, v1] = [0, 1, 2, 3].map(|i| word(12 + k * 4 + i));
                BillboardView {
                    corners: [[x0, y1], [x0, y0], [x1, y0], [x1, y1]],
                    uvs: [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
                }
            })
            .collect()
    }

    /// The billboard's material as the renderer's material record.
    pub fn billboard_material(&self) -> MtrlMaterial {
        let m = &self.material;
        let mut preamble = vec![f32::from_bits(m.word_0)];
        preamble.extend_from_slice(&m.params);
        MtrlMaterial { textures: m.textures.clone(), flags: m.flags, preamble, shader_key: m.tail[0] }
    }
}

/// One tree of a species.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaf {
    pub position: [f32; 3],
    /// The leaf's 4 flag bits as stored: 2 on every retail tree. `FUN_0049f700`'s mask tests them, and
    /// `FUN_0049f900` rewrites them as a tree changes state.
    pub flags: u8,
    /// Yaw as a fraction of a turn, in 15ths: `value / 15`. A tree's entity rotates by this about +Y
    /// (retail: 0.7333 is the quaternion `(0, 0.743, 0, -0.669)`, 264°), and it is the fraction
    /// `PgBillboardTreeVP` subtracts from the view angle.
    pub yaw: f32,
}

/// One billboard quad of a species: a corner's offset (across, up) and its texture coordinate within an
/// atlas tile, as `FUN_004a24a0` builds `vCornerPosOffset` and `vCornerTexCoords`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BillboardView {
    pub corners: [[f32; 2]; 4],
    pub uvs: [[f32; 2]; 4],
}

/// `1 / (2^width − 1)`, the `.rdata` table at `0xB9C538`.
fn step_of(width: u32) -> f32 {
    1.0 / ((1u64 << width) - 1) as f32
}

/// A node on the way down: its box, its depth, the counts that size its subtree, and where its first
/// bit is.
#[derive(Clone, Copy)]
struct Cursor {
    min: [f32; 3],
    max: [f32; 3],
    depth: usize,
    leaves: u32,
    shallow_leaves: u32,
    last_splits: u32,
    upper_bits: u32,
    bit: u32,
}

impl Tree {
    /// `width` bits at `bit`, as `FUN_0049f700` reads a field: most significant first, from two
    /// consecutive little-endian words.
    fn read(&self, bit: u32, width: u32) -> Result<u32, String> {
        if bit + width > self.bit_count {
            return Err(format!("{width} bits at {bit} run past the {} of the tree", self.bit_count));
        }
        let word = |i: usize| -> u32 {
            match self.bits.get(i * 4..i * 4 + 4) {
                Some(w) => u32::from_le_bytes([w[0], w[1], w[2], w[3]]),
                None => 0,
            }
        };
        let i = (bit >> 5) as usize;
        let joined = (word(i) as u64) << 32 | word(i + 1) as u64;
        Ok((joined >> (64 - (bit & 31) - width)) as u32 & ((1u32 << width) - 1))
    }

    /// The bits of a subtree with these counts, as `FUN_0049fd70` skips it.
    fn subtree_bits(&self, leaves: u32, shallow_leaves: u32, last_splits: u32, upper_bits: u32) -> u32 {
        let [deep, shallow, split] = [self.root[5], self.root[6], self.root[7]];
        shallow * shallow_leaves + split * last_splits + deep * (leaves - shallow_leaves) + upper_bits
    }

    fn walk(&self, c: Cursor, out: &mut Vec<Leaf>) -> Result<(), String> {
        if c.last_splits + c.upper_bits == 0 {
            let flags = self.read(c.bit, 4)?;
            let mut bit = c.bit + 4;
            let mut position = [0.0; 3];
            for (k, p) in position.iter_mut().enumerate() {
                let width = *self.levels.get(c.depth * 3 + k).ok_or_else(|| format!("no level {}", c.depth))?;
                let v = self.read(bit, width)?;
                *p = c.min[k] + v as f32 * ((c.max[k] - c.min[k]) * step_of(width));
                bit += width;
            }
            let yaw = self.read(bit, 4)? as f32 * step_of(4);
            out.push(Leaf { position, flags: flags as u8, yaw });
            return Ok(());
        }
        let axis = *self.axes.get(c.depth).ok_or_else(|| format!("no axis at depth {}", c.depth))? as usize;
        let width = *self.leaf_words.get(axis).ok_or_else(|| format!("axis {axis}"))?;
        let q = self.read(c.bit, width)?;
        let step = (c.max[axis] - c.min[axis]) * step_of(width);
        let (mut first, mut second) = (c, c);
        first.max[axis] = c.min[axis] + (q + 1) as f32 * step;
        second.min[axis] = c.min[axis] + q as f32 * step;
        let upper_bits = if c.upper_bits != 0 { (c.upper_bits - width) >> 1 } else { 0 };
        let first_splits = if c.upper_bits != 0 { c.last_splits.div_ceil(2) } else { 0 };
        first.leaves = c.leaves.div_ceil(2);
        first.shallow_leaves = c.shallow_leaves >> 1;
        first.last_splits = first_splits;
        second.leaves = c.leaves >> 1;
        second.shallow_leaves = c.shallow_leaves.div_ceil(2);
        second.last_splits = c.last_splits >> 1;
        first.upper_bits = upper_bits;
        second.upper_bits = upper_bits;
        first.depth = c.depth + 1;
        second.depth = c.depth + 1;
        first.bit = c.bit + width;
        second.bit = first.bit + self.subtree_bits(first.leaves, first.shallow_leaves, first.last_splits, upper_bits);
        self.walk(first, out)?;
        self.walk(second, out)
    }

    /// Every tree of the species, in stream order.
    pub fn leaves(&self) -> Result<Vec<Leaf>, String> {
        let mut out = Vec::with_capacity(self.root[1] as usize);
        let root = Cursor {
            min: self.root_min,
            max: self.root_max,
            depth: 0,
            leaves: self.root[1],
            shallow_leaves: self.root[2],
            last_splits: self.root[3],
            upper_bits: self.root[4],
            bit: 0,
        };
        self.walk(root, &mut out)?;
        if out.len() != self.root[1] as usize {
            return Err(format!("{} leaves, the root says {}", out.len(), self.root[1]));
        }
        let held = self.subtree_bits(root.leaves, root.shallow_leaves, root.last_splits, root.upper_bits);
        if held != self.bit_count {
            return Err(format!("the root sizes {held} bits, the stream holds {}", self.bit_count));
        }
        Ok(out)
    }
}

/// A read position in a `tree` body.
struct Stream<'a> {
    b: &'a [u8],
    p: usize,
}

impl Stream<'_> {
    fn need(&self, n: usize, what: &str) -> Result<(), String> {
        if self.p + n > self.b.len() {
            return Err(format!("{what} runs past the body at {}", self.p));
        }
        Ok(())
    }

    fn u32(&mut self, what: &str) -> Result<u32, String> {
        self.need(4, what)?;
        self.p += 4;
        Ok(u32_at(self.b, self.p - 4))
    }

    fn f32(&mut self, what: &str) -> Result<f32, String> {
        self.need(4, what)?;
        self.p += 4;
        Ok(f32_at(self.b, self.p - 4))
    }

    fn u32s<const N: usize>(&mut self, what: &str) -> Result<[u32; N], String> {
        let mut out = [0; N];
        for v in &mut out {
            *v = self.u32(what)?;
        }
        Ok(out)
    }

    fn material(&mut self, ctx: &str) -> Result<Material, String> {
        let (material, end) = decode_material(self.b, self.p, ctx)?;
        self.p = end;
        Ok(material)
    }

    fn species(&mut self) -> Result<Species, String> {
        let template = self.u32("template")?;
        let atlas = self.u32s::<2>("atlas")?;
        let authored_range = self.f32("range")?;
        self.need(1, "flag")?;
        let flag = self.b[self.p];
        self.p += 1;
        let params = [self.f32("params")?, self.f32("params")?, self.f32("params")?];
        let basis = self.u32s::<BASIS_WORDS>("basis")?;
        let tail_words = self.u32s::<2>("tail words")?;
        let vertex_count = self.u32("vertex count")? as usize;
        let index_count = self.u32("index count")? as usize;
        let modelled = if vertex_count != 0 && index_count != 0 {
            self.need(vertex_count * 20 + index_count * 2, "billboard mesh")?;
            let vertices = (0..vertex_count)
                .map(|v| std::array::from_fn(|k| f32_at(self.b, self.p + v * 20 + k * 4)))
                .collect();
            self.p += vertex_count * 20;
            let indices = (0..index_count).map(|i| u16_at(self.b, self.p + i * 2)).collect();
            self.p += index_count * 2;
            Some(Modelled { vertices, indices, material: self.material("modelled billboard")? })
        } else {
            None
        };
        let material = self.material("billboard")?;
        let tree = self.tree()?;
        Ok(Species {
            template,
            atlas,
            authored_range,
            flag,
            params,
            basis,
            tail_words,
            modelled,
            material,
            tree,
        })
    }

    fn tree(&mut self) -> Result<Tree, String> {
        let byte_count = self.u32("bit stream length")? as usize;
        self.need(byte_count, "bit stream")?;
        let bits = self.b[self.p..self.p + byte_count].to_vec();
        self.p += byte_count;
        let bit_count = self.u32("bit count")?;
        let root_min = [self.f32("root box")?, self.f32("root box")?, self.f32("root box")?];
        let root_max = [self.f32("root box")?, self.f32("root box")?, self.f32("root box")?];
        let root = self.u32s::<8>("root node")?;
        let levels = self.u32s::<63>("level table")?.to_vec();
        let leaf_words = self.u32s::<3>("leaf table")?;
        let axes = self.u32s::<21>("axis table")?.to_vec();
        Ok(Tree { bits, bit_count, root_min, root_max, root, levels, leaf_words, axes })
    }
}

impl Foliage {
    /// Parse a whole `foliage` container.
    pub fn decode(container: &[u8]) -> Result<Foliage, String> {
        let roots = parse_tree(container)?;
        let [root] = roots.as_slice() else {
            return Err(format!("foliage: {} root rows, expected 1", roots.len()));
        };
        let mut s = Stream { b: leaf(root, "foliage")?, p: 0 };
        let version = s.u32("version")?;
        let count = s.u32("species count")?;
        let species = (0..count)
            .map(|i| s.species().map_err(|e| format!("species {i}: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        if s.p != s.b.len() {
            return Err(format!("foliage: {} bytes after the last species", s.b.len() - s.p));
        }
        Ok(Foliage { version, species })
    }
}
