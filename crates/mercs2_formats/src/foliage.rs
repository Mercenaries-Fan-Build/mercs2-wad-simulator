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

use crate::terrainmesh::{decode_material, f32_at, leaf, parse_tree, u16_at, u32_at, Material};

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
