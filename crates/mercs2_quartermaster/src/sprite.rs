//! FX sprites: `add_fx_sprite` images drawn into the free square of the `vfx` atlas, and the
//! `fxdict` records that name their rectangles.
//!
//! Every particle the game draws samples ONE texture, the `vfx` atlas `0x89E211AF` (a 2048² DXT5 of
//! 10 mips in `blocks\VZ\resident_P000_Q3.block`), inside the rectangle that the `fxdict`
//! `0x86BF6C5B` record of its frame key gives ([`FxRect`]). A new sprite is texels in the atlas and
//! a record that names them.
//!
//! [`pack`] places a set's sprites. The same set gives the same bytes whatever order its Shipments
//! come in:
//!
//! 1. **The free square** ([`free_square`]): the largest square of the base atlas, aligned to its own
//!    side, that no `fxdict` record lies over and whose every mip-0 texel has alpha 0; the first in
//!    row-major order among squares of that side. The base atlas is the set's `vfx` repaint when it
//!    has one ([`Atlas::repaint`]), else the game's. On retail it is the 512² square at (1536, 0).
//! 2. **Placement** ([`place`]): a sprite's width and height are each a power of two from 4 to 512,
//!    and it is allocated the square of its larger side. Sprites are placed largest allocation
//!    first, then by key as u32, each into the smallest free quadtree cell that holds it (the first
//!    in row-major order among cells of that side), split by quarters down to its allocation with
//!    the top-left quarter kept. A sprite sits at its cell's top-left corner, so its x is a multiple
//!    of its width and its y of its height. Sprites that need more than the square holds are an
//!    error that lists every sprite and the space left.
//! 3. **Drawing** ([`draw`]): the sprites are composed onto transparent black over the square, the
//!    square's mips are box-filtered from it alone, and each level is BC3-encoded and written over
//!    the base body at the square's blocks. Where the square at a level is narrower than a block,
//!    the block is decoded, the square's texels are written, and the block is encoded again; where
//!    it is narrower than a texel, that texel is the mean of the four texels of the level above.
//!    The body keeps its length ([`mercs2_formats::texture::replace_body`]).
//! 4. **Records** ([`records`]): each sprite's rectangle, `u = x / S`, `v = 1 − (y + h) / S` (the
//!    record measures `v` from the bottom), `w / S`, `h / S`, joined to the base records and sorted
//!    by key as `i32` ([`mercs2_formats::fxdict::sort_fxdict`]).
//!
//! Images carry straight alpha: the effect pixel shader `PgFXFP` multiplies the sampled colour by
//! the vertex colour and writes `rgb × a`.

use std::collections::BTreeMap;
use std::path::Path;

use mercs2_formats::fxdict::{sort_fxdict, FxRect};
use mercs2_formats::texture::{parse_texture_container, replace_body, TexFormat};
use mercs2_formats::texture_encode::{box_down, decode_bc3_block, encode_bc3, mip_chain, mip_count};

/// The `vfx` atlas: `pandemic_hash_m2("vfx")`, the texture `PgFX` requests at init (`FUN_0048a170`).
pub const VFX_ATLAS: u32 = 0x89E2_11AF;

/// The smallest sprite side: one BC3 block.
pub const MIN_SIDE: usize = 4;
/// The largest sprite side.
pub const MAX_SIDE: usize = 512;

/// An image: straight RGBA as `f32` in 0..=255, row-major from the top row.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<f32>,
}

/// Read a PNG as an [`Image`].
pub fn read_png(path: &Path) -> Result<Image, String> {
    let rgba = crate::build::read_png_rgba(path)?;
    Ok(Image { width: rgba.width, height: rgba.height, rgba: rgba.pixels })
}

/// Read a sprite image: a PNG whose width and height are each a power of two from [`MIN_SIDE`] to
/// [`MAX_SIDE`].
pub fn read_sprite(path: &Path) -> Result<Image, String> {
    let image = read_png(path)?;
    for (what, side) in [("width", image.width), ("height", image.height)] {
        if !side.is_power_of_two() || !(MIN_SIDE..=MAX_SIDE).contains(&side) {
            return Err(format!(
                "{}: the image {what} is {side}; a sprite's width and height are each a power of two \
                 from {MIN_SIDE} to {MAX_SIDE}",
                path.display()
            ));
        }
    }
    Ok(image)
}

/// Why a sprite name cannot be used: empty, or written as a bare `0xHHHHHHHH` hash. `None` when it
/// can. The name is hashed into the frame key, and an effect frame written `0xHHHHHHHH` is that
/// hash, so a name in that form would name a key other than the one it hashes to.
pub fn name_refusal(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("the sprite name is empty; its hash is the frame key an effect's TEXT names".into());
    }
    crate::manifest::bare_hash(name).map(|h| {
        format!(
            "the sprite name {name:?} is written as a hash; an effect frame written so names the key \
             0x{h:08X} itself, and the sprite is filed under the hash of its name, \
             0x{:08X}. Give the sprite a name",
            mercs2_formats::hash::pandemic_hash_m2(name)
        )
    })
}

/// The atlas a set draws into: a square, power-of-two DXT5 texture with its whole mip chain.
#[derive(Debug, Clone)]
pub struct Atlas {
    /// The container the body is written back into.
    container: Vec<u8>,
    /// Width and height, in texels.
    pub size: usize,
    /// Mip levels, from `texture_encode::mip_count`.
    pub mips: usize,
    /// The whole mip chain, level 0 first.
    pub body: Vec<u8>,
}

impl Atlas {
    /// Read an atlas container. Anything but a square, power-of-two DXT5 that carries its whole mip
    /// chain is an error.
    pub fn parse(container: &[u8]) -> Result<Atlas, String> {
        let tex = parse_texture_container(container).map_err(|e| format!("the vfx atlas: {e}"))?;
        if tex.format != TexFormat::Bc3 {
            return Err("the vfx atlas is not DXT5".into());
        }
        let size = tex.width as usize;
        if tex.height as usize != size || !size.is_power_of_two() || size < MIN_SIDE {
            return Err(format!("the vfx atlas is {}x{}; it is a square power of two", tex.width, tex.height));
        }
        let mips = mip_count(size, size);
        let atlas = Atlas { container: container.to_vec(), size, mips, body: tex.all_mips };
        let want = atlas.level_offset(mips);
        if atlas.body.len() != want {
            return Err(format!(
                "the vfx atlas body is {} bytes; a {size}² DXT5 of {mips} mips is {want}",
                atlas.body.len()
            ));
        }
        if atlas.container()? != container {
            return Err("the vfx atlas does not re-write through its own body".into());
        }
        Ok(atlas)
    }

    /// The atlas with `image` as its texels: `image` is the atlas's size, and its mip chain is
    /// box-filtered and BC3-encoded whole.
    pub fn repaint(&self, image: &Image) -> Result<Atlas, String> {
        if image.width != self.size || image.height != self.size {
            return Err(format!(
                "the repaint is {}x{}; the vfx atlas is {s}x{s}, and a repaint keeps its size",
                image.width,
                image.height,
                s = self.size
            ));
        }
        let body = mip_chain(self.size, self.size, 4, &image.rgba, encode_bc3);
        if body.len() != self.body.len() {
            return Err(format!("the repaint encodes to {} bytes, not the atlas's {}", body.len(), self.body.len()));
        }
        Ok(Atlas { body, ..self.clone() })
    }

    /// The container with this body.
    pub fn container(&self) -> Result<Vec<u8>, String> {
        replace_body(&self.container, &self.body)
    }

    /// The side of mip `level`.
    fn side(&self, level: usize) -> usize {
        self.size >> level
    }

    /// The byte offset of mip `level` in the body; `level == mips` is the body's length.
    fn level_offset(&self, level: usize) -> usize {
        (0..level).map(|l| (self.side(l) / 4) * (self.side(l) / 4) * 16).sum()
    }

    /// The byte offset of block `(bx, by)` of mip `level`.
    fn block_offset(&self, level: usize, bx: usize, by: usize) -> usize {
        self.level_offset(level) + (by * (self.side(level) / 4) + bx) * 16
    }

    /// The texel `(x, y)` of mip `level`, decoded, as `f32` in 0..=255.
    fn texel(&self, level: usize, x: usize, y: usize) -> [f32; 4] {
        let o = self.block_offset(level, x / 4, y / 4);
        let t = decode_bc3_block(&self.body[o..o + 16])[(y % 4) * 4 + x % 4];
        [t[0] as f32, t[1] as f32, t[2] as f32, t[3] as f32]
    }

    /// The alpha of every mip-0 texel, row-major.
    pub fn alpha(&self) -> Vec<u8> {
        let n = self.size;
        let mut out = vec![0u8; n * n];
        for by in 0..n / 4 {
            for bx in 0..n / 4 {
                let o = self.block_offset(0, bx, by);
                let texels = decode_bc3_block(&self.body[o..o + 16]);
                for ty in 0..4 {
                    for tx in 0..4 {
                        out[(by * 4 + ty) * n + bx * 4 + tx] = texels[ty * 4 + tx][3];
                    }
                }
            }
        }
        out
    }
}

/// A square of the atlas, in mip-0 texels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square {
    pub x: usize,
    pub y: usize,
    pub side: usize,
}

impl std::fmt::Display for Square {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{} at ({}, {})", self.side, self.side, self.x, self.y)
    }
}

/// The mip-0 texels a record's rectangle overlaps, as `(x0, y0, x1, y1)`, end-exclusive and clipped
/// to the atlas. The top edge is `1 − v − h`.
fn record_texels(r: &FxRect, size: usize) -> (usize, usize, usize, usize) {
    let s = size as f64;
    let (u, v, w, h) = (r.u as f64, r.v as f64, r.w as f64, r.h as f64);
    let top = 1.0 - v - h;
    let clip = |p: f64| p.clamp(0.0, s) as usize;
    (clip((u * s).floor()), clip((top * s).floor()), clip(((u + w) * s).ceil()), clip(((top + h) * s).ceil()))
}

/// The free square of `atlas` under `records` (module docs, step 1). `None` when not even a
/// [`MIN_SIDE`] square is free.
pub fn free_square(atlas: &Atlas, records: &[FxRect]) -> Option<Square> {
    let n = atlas.size;
    let mut blocked: Vec<bool> = atlas.alpha().into_iter().map(|a| a != 0).collect();
    for r in records {
        let (x0, y0, x1, y1) = record_texels(r, n);
        for y in y0..y1 {
            for x in x0..x1 {
                blocked[y * n + x] = true;
            }
        }
    }
    // Summed-area table of blocked texels.
    let mut sat = vec![0u32; (n + 1) * (n + 1)];
    for y in 0..n {
        let mut row = 0u32;
        for x in 0..n {
            row += u32::from(blocked[y * n + x]);
            sat[(y + 1) * (n + 1) + x + 1] = sat[y * (n + 1) + x + 1] + row;
        }
    }
    let count = |x: usize, y: usize, s: usize| {
        let at = |xx: usize, yy: usize| sat[yy * (n + 1) + xx] as i64;
        at(x + s, y + s) - at(x, y + s) - at(x + s, y) + at(x, y)
    };
    let mut side = n;
    while side >= MIN_SIDE {
        for y in (0..n).step_by(side) {
            for x in (0..n).step_by(side) {
                if count(x, y, side) == 0 {
                    return Some(Square { x, y, side });
                }
            }
        }
        side /= 2;
    }
    None
}

/// A sprite to place: its frame key, its size, and how it is named in messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub key: u32,
    pub width: usize,
    pub height: usize,
    pub label: String,
}

impl Want {
    fn allocation(&self) -> usize {
        self.width.max(self.height)
    }
}

/// A placed sprite, in mip-0 texels of the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub key: u32,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// Place `wants` in `square` (module docs, step 2). The error lists every sprite, its allocation,
/// the square's area, and the space left when the first sprite that does not fit came up.
pub fn place(square: Square, wants: &[Want]) -> Result<Vec<Placed>, String> {
    let mut order: Vec<&Want> = wants.iter().collect();
    order.sort_by(|a, b| b.allocation().cmp(&a.allocation()).then(a.key.cmp(&b.key)));
    let mut free = vec![square];
    let mut used = 0usize;
    let mut out = Vec::with_capacity(order.len());
    for w in &order {
        let a = w.allocation();
        let pick = free
            .iter()
            .enumerate()
            .filter(|(_, c)| c.side >= a)
            .min_by_key(|(_, c)| (c.side, c.y, c.x))
            .map(|(i, _)| i);
        let Some(i) = pick else {
            let listed: Vec<String> = order
                .iter()
                .map(|w| format!("{} {}x{} (allocated {a}x{a})", w.label, w.width, w.height, a = w.allocation()))
                .collect();
            let need: usize = order.iter().map(|w| w.allocation() * w.allocation()).sum();
            let area = square.side * square.side;
            return Err(format!(
                "the sprites need {need} texels and the free square {square} of the base atlas holds \
                 {area}; {} did not fit with {} texels left. The sprites, largest first: {}",
                w.label,
                area - used,
                listed.join(", ")
            ));
        };
        let mut cell = free.remove(i);
        while cell.side > a {
            let h = cell.side / 2;
            free.push(Square { x: cell.x + h, y: cell.y, side: h });
            free.push(Square { x: cell.x, y: cell.y + h, side: h });
            free.push(Square { x: cell.x + h, y: cell.y + h, side: h });
            cell.side = h;
        }
        used += a * a;
        out.push(Placed { key: w.key, x: cell.x, y: cell.y, width: w.width, height: w.height });
    }
    Ok(out)
}

/// Draw `placed` into `atlas` over `square` (module docs, step 3). `images` holds each placed key's
/// image.
pub fn draw(atlas: &mut Atlas, square: Square, placed: &[Placed], images: &BTreeMap<u32, &Image>) -> Result<(), String> {
    let s = square.side;
    if square.x % s != 0 || square.y % s != 0 || square.x + s > atlas.size || square.y + s > atlas.size {
        return Err(format!("the square {square} is not a self-aligned square of the {}² atlas", atlas.size));
    }
    let mut zone = vec![0f32; s * s * 4];
    for p in placed {
        let image = images.get(&p.key).ok_or_else(|| format!("no image for sprite 0x{:08X}", p.key))?;
        if (image.width, image.height) != (p.width, p.height) {
            return Err(format!("sprite 0x{:08X} is placed {}x{} but its image is {}x{}", p.key, p.width, p.height, image.width, image.height));
        }
        let (ox, oy) = (p.x - square.x, p.y - square.y);
        for y in 0..p.height {
            let src = y * p.width * 4;
            let dst = ((oy + y) * s + ox) * 4;
            zone[dst..dst + p.width * 4].copy_from_slice(&image.rgba[src..src + p.width * 4]);
        }
    }

    // `cur` is the square at the current level: `side × side` texels at (zx, zy), where `side` is at
    // least one texel.
    let mut cur = zone;
    let mut cur_side = s;
    for level in 0..atlas.mips {
        let (zx, zy) = (square.x >> level, square.y >> level);
        if level > 0 {
            if cur_side > 1 {
                let (nw, _, down) = box_down(cur_side, cur_side, 4, &cur);
                cur = down;
                cur_side = nw;
            } else {
                // Narrower than a texel: the mean of the four texels of the level above, the
                // square's own (`cur`) and three of the body.
                let (px, py) = (zx * 2, zy * 2);
                let (sx, sy) = (square.x >> (level - 1), square.y >> (level - 1));
                let mut sum = [0f32; 4];
                for (x, y) in [(px, py), (px + 1, py), (px, py + 1), (px + 1, py + 1)] {
                    let t = if (x, y) == (sx, sy) { [cur[0], cur[1], cur[2], cur[3]] } else { atlas.texel(level - 1, x, y) };
                    for k in 0..4 {
                        sum[k] += t[k];
                    }
                }
                cur = sum.iter().map(|v| v * 0.25).collect();
            }
        }
        if cur_side % 4 == 0 {
            let blocks = encode_bc3(cur_side, cur_side, &cur);
            let row_bytes = (cur_side / 4) * 16;
            for r in 0..cur_side / 4 {
                let o = atlas.block_offset(level, zx / 4, zy / 4 + r);
                atlas.body[o..o + row_bytes].copy_from_slice(&blocks[r * row_bytes..(r + 1) * row_bytes]);
            }
        } else {
            let o = atlas.block_offset(level, zx / 4, zy / 4);
            let mut px = [0f32; 64];
            for (i, t) in decode_bc3_block(&atlas.body[o..o + 16]).iter().enumerate() {
                for k in 0..4 {
                    px[i * 4 + k] = t[k] as f32;
                }
            }
            for y in 0..cur_side {
                for x in 0..cur_side {
                    let d = ((zy % 4 + y) * 4 + zx % 4 + x) * 4;
                    let srci = (y * cur_side + x) * 4;
                    px[d..d + 4].copy_from_slice(&cur[srci..srci + 4]);
                }
            }
            atlas.body[o..o + 16].copy_from_slice(&encode_bc3(4, 4, &px));
        }
    }
    Ok(())
}

/// The records of `placed` in a `size`² atlas (module docs, step 4).
pub fn records(size: usize, placed: &[Placed]) -> Vec<FxRect> {
    let s = size as f32;
    placed
        .iter()
        .map(|p| FxRect {
            key: p.key,
            u: p.x as f32 / s,
            v: (size - p.y - p.height) as f32 / s,
            w: p.width as f32 / s,
            h: p.height as f32 / s,
        })
        .collect()
}

/// A sprite of a set: its key, its image and how it is named in messages.
pub struct Sprite<'a> {
    pub key: u32,
    pub label: String,
    pub image: &'a Image,
}

/// What a set's sprites come to: the atlas with every sprite drawn, the records (the base's and the
/// sprites', sorted), the free square and where each sprite went.
pub struct Packed {
    pub atlas: Atlas,
    pub records: Vec<FxRect>,
    pub square: Square,
    pub placed: Vec<Placed>,
}

/// Why a set's sprites do not pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    /// The base atlas has no free square.
    NoFreeSquare,
    /// The sprites need more than the free square holds; the message lists them.
    Overflow(String),
    /// A record or the drawing failed; the message says what.
    Other(String),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::NoFreeSquare => write!(
                f,
                "the base atlas has no free square: every {MIN_SIDE}x{MIN_SIDE} square of it, aligned to \
                 its side, lies under an fxdict record or has a texel with alpha"
            ),
            PackError::Overflow(m) | PackError::Other(m) => write!(f, "{m}"),
        }
    }
}

/// Pack `sprites` into `base` beside `base_records` (module docs).
pub fn pack(base: &Atlas, base_records: &[FxRect], sprites: &[Sprite<'_>]) -> Result<Packed, PackError> {
    let square = free_square(base, base_records).ok_or(PackError::NoFreeSquare)?;
    let wants: Vec<Want> = sprites
        .iter()
        .map(|s| Want { key: s.key, width: s.image.width, height: s.image.height, label: s.label.clone() })
        .collect();
    let placed = place(square, &wants).map_err(PackError::Overflow)?;
    let images: BTreeMap<u32, &Image> = sprites.iter().map(|s| (s.key, s.image)).collect();
    let mut atlas = base.clone();
    draw(&mut atlas, square, &placed, &images).map_err(PackError::Other)?;
    let mut all = base_records.to_vec();
    all.extend(records(atlas.size, &placed));
    sort_fxdict(&mut all).map_err(PackError::Other)?;
    Ok(Packed { atlas, records: all, square, placed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::texture_encode::ucfx_texture;

    /// A `size`² DXT5 atlas container, every texel `rgba`.
    fn atlas(size: usize, rgba: [f32; 4]) -> Atlas {
        let px: Vec<f32> = (0..size * size).flat_map(|_| rgba).collect();
        let body = mip_chain(size, size, 4, &px, encode_bc3);
        Atlas::parse(&ucfx_texture("t", size, size, b"DXT5", &body)).expect("the test atlas parses")
    }

    fn image(w: usize, h: usize, rgba: [f32; 4]) -> Image {
        Image { width: w, height: h, rgba: (0..w * h).flat_map(|_| rgba).collect() }
    }

    fn want(key: u32, w: usize, h: usize) -> Want {
        Want { key, width: w, height: h, label: format!("s{key}") }
    }

    #[test]
    fn placement_does_not_depend_on_the_order_sprites_come_in() {
        let sq = Square { x: 64, y: 0, side: 64 };
        let a = vec![want(3, 16, 16), want(1, 32, 8), want(2, 16, 16), want(9, 4, 4), want(7, 8, 32)];
        let mut b = a.clone();
        b.reverse();
        let pa = place(sq, &a).unwrap();
        let pb = place(sq, &b).unwrap();
        assert_eq!(pa, pb);
        // Largest allocation first, ties by key: 1 (32x8) then 7 (8x32), then 2, 3, then 9.
        assert_eq!(pa.iter().map(|p| p.key).collect::<Vec<_>>(), vec![1, 7, 2, 3, 9]);
        assert_eq!((pa[0].x, pa[0].y), (64, 0));
        assert_eq!((pa[1].x, pa[1].y), (96, 0));
        assert_eq!((pa[2].x, pa[2].y), (64, 32));
        assert_eq!((pa[3].x, pa[3].y), (80, 32));
        assert_eq!((pa[4].x, pa[4].y), (64, 48));
    }

    #[test]
    fn every_sprite_is_aligned_to_its_own_width_and_height_and_none_overlap() {
        let sq = Square { x: 512, y: 0, side: 512 };
        let mut wants = Vec::new();
        for (i, (w, h)) in [(256, 256), (128, 32), (64, 64), (4, 8), (8, 4), (16, 128), (32, 32), (4, 4)].iter().enumerate() {
            wants.push(want(i as u32 * 7 + 1, *w, *h));
        }
        let placed = place(sq, &wants).unwrap();
        for p in &placed {
            assert_eq!(p.x % p.width, 0, "{p:?}");
            assert_eq!(p.y % p.height, 0, "{p:?}");
            assert!(p.x >= sq.x && p.y >= sq.y && p.x + p.width <= sq.x + sq.side && p.y + p.height <= sq.y + sq.side);
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let apart = a.x + a.width <= b.x || b.x + b.width <= a.x || a.y + a.height <= b.y || b.y + b.height <= a.y;
                assert!(apart, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn sprites_that_need_more_than_the_square_are_refused_with_every_size_and_the_space_left() {
        let sq = Square { x: 0, y: 0, side: 64 };
        let e = place(sq, &[want(1, 64, 64), want(2, 4, 4)]).unwrap_err();
        assert!(e.contains("need 4112 texels"), "{e}");
        assert!(e.contains("holds 4096"), "{e}");
        assert!(e.contains("s2 did not fit with 0 texels left"), "{e}");
        assert!(e.contains("s1 64x64 (allocated 64x64), s2 4x4 (allocated 4x4)"), "{e}");
        // Exactly full fits.
        assert_eq!(place(sq, &[want(1, 32, 32), want(2, 32, 32), want(3, 32, 32), want(4, 32, 32)]).unwrap().len(), 4);
    }

    #[test]
    fn the_free_square_is_the_largest_aligned_transparent_square_under_no_record() {
        // Opaque everywhere but the top-right quarter of a 64² atlas, which is transparent.
        let size = 64;
        let mut px = vec![255f32; size * size * 4];
        for y in 0..32 {
            for x in 32..64 {
                px[(y * size + x) * 4 + 3] = 0.0;
            }
        }
        let body = mip_chain(size, size, 4, &px, encode_bc3);
        let a = Atlas::parse(&ucfx_texture("t", size, size, b"DXT5", &body)).unwrap();
        assert_eq!(free_square(&a, &[]), Some(Square { x: 32, y: 0, side: 32 }));
        // A record over the square's top-left 16² (u = 0.5, top = 0, so v = 1 - 0.25) halves it,
        // and the first free 16² in row-major order is the one to its right.
        let r = FxRect { key: 1, u: 0.5, v: 0.75, w: 0.25, h: 0.25 };
        assert_eq!(free_square(&a, &[r]), Some(Square { x: 48, y: 0, side: 16 }));
        // Fully opaque: none.
        assert_eq!(free_square(&atlas(64, [255.0; 4]), &[]), None);
    }

    #[test]
    fn a_32_square_drawn_into_a_64_atlas_rewrites_only_its_blocks_and_the_coarse_mixed_ones() {
        let base = atlas(64, [10.0, 20.0, 30.0, 0.0]);
        let sq = Square { x: 32, y: 0, side: 32 };
        let img = image(16, 16, [255.0, 255.0, 255.0, 255.0]);
        let placed = vec![Placed { key: 5, x: 32, y: 0, width: 16, height: 16 }];
        let images: BTreeMap<u32, &Image> = [(5, &img)].into();
        let mut drawn = base.clone();
        draw(&mut drawn, sq, &placed, &images).unwrap();
        assert_eq!(drawn.body.len(), base.body.len());
        // Mips 64², 32², 16², 8², 4²: the square is 32², 16², 8², one 4² block, then 2² in the one
        // block of the last level.
        assert_eq!(base.mips, 5);
        for level in 0..base.mips {
            let n = base.size >> level;
            let side = (sq.side >> level).max(1);
            let (zx, zy) = (sq.x >> level, sq.y >> level);
            for by in 0..n / 4 {
                for bx in 0..n / 4 {
                    let o = base.block_offset(level, bx, by);
                    let in_zone = bx * 4 + 3 >= zx && bx * 4 < zx + side && by * 4 + 3 >= zy && by * 4 < zy + side;
                    let same = base.body[o..o + 16] == drawn.body[o..o + 16];
                    assert_eq!(!same, in_zone, "level {level} block ({bx}, {by})");
                }
            }
        }
        // Mip 0: the sprite is white and opaque at (32..48, 0..16); the rest of the square is
        // transparent black; outside the square the base is untouched.
        for (x, y, want) in [(32, 0, [255.0, 255.0, 255.0, 255.0]), (47, 15, [255.0; 4]), (48, 0, [0.0; 4]), (32, 16, [0.0; 4])] {
            assert_eq!(drawn.texel(0, x, y), want, "({x}, {y})");
        }
        assert_eq!(drawn.texel(0, 0, 0), base.texel(0, 0, 0));
        // Mip 3 (8²): the square is 4² at (4, 0), one whole block, a quarter white.
        let t = drawn.texel(3, 4, 0);
        assert_eq!(t[3], 255.0);
        let t = drawn.texel(3, 7, 3);
        assert_eq!(t[3], 0.0);
    }

    #[test]
    fn a_square_narrower_than_a_block_rewrites_the_block_that_holds_it() {
        // A 32² atlas of mips 32², 16², 8², 4², and an 8² square at (8, 0).
        let base = atlas(32, [0.0, 0.0, 0.0, 255.0]);
        assert_eq!(base.mips, 4);
        let sq = Square { x: 8, y: 0, side: 8 };
        let img = image(8, 8, [255.0, 255.0, 255.0, 0.0]);
        let images: BTreeMap<u32, &Image> = [(1, &img)].into();
        let mut drawn = base.clone();
        draw(&mut drawn, sq, &[Placed { key: 1, x: 8, y: 0, width: 8, height: 8 }], &images).unwrap();
        // Level 2 (8²): the square is 2² at (2, 0), inside block (0, 0) with base texels around it.
        assert_eq!(drawn.texel(2, 2, 0)[3], 0.0);
        assert_eq!(drawn.texel(2, 3, 1)[3], 0.0);
        assert_eq!(drawn.texel(2, 0, 0)[3], 255.0);
        assert_eq!(drawn.texel(2, 2, 2)[3], 255.0);
        // Level 3 (4²): the square is one texel at (1, 0).
        assert_eq!(drawn.texel(3, 1, 0)[3], 0.0);
        assert_eq!(drawn.texel(3, 0, 0)[3], 255.0);
    }

    #[test]
    fn records_measure_v_from_the_bottom_and_sort_by_signed_key() {
        let placed = [
            Placed { key: 0x0000_0010, x: 1536, y: 0, width: 64, height: 64 },
            Placed { key: 0x8000_0001, x: 1600, y: 0, width: 32, height: 16 },
        ];
        let r = records(2048, &placed);
        assert_eq!(r[0], FxRect { key: 0x10, u: 0.75, v: 1.0 - 64.0 / 2048.0, w: 64.0 / 2048.0, h: 64.0 / 2048.0 });
        assert_eq!(r[0].top(), 0.0);
        assert_eq!(r[1].u * 2048.0, 1600.0);
        assert_eq!(r[1].v * 2048.0, 2048.0 - 16.0);
        let base = atlas(64, [0.0; 4]);
        let img = image(4, 4, [255.0; 4]);
        let sprites = [
            Sprite { key: 0x0000_0002, label: "a".into(), image: &img },
            Sprite { key: 0xF000_0000, label: "b".into(), image: &img },
        ];
        let base_records = [FxRect { key: 0x7000_0000, u: 0.0, v: 0.0, w: 0.0625, h: 0.0625 }];
        let p = pack(&base, &base_records, &sprites).unwrap();
        assert_eq!(p.records.iter().map(|r| r.key).collect::<Vec<_>>(), vec![0xF000_0000, 0x0000_0002, 0x7000_0000]);
        // A sprite under a key the base already has is refused.
        let dup = [Sprite { key: 0x7000_0000, label: "c".into(), image: &img }];
        assert!(matches!(pack(&base, &base_records, &dup), Err(PackError::Other(m)) if m.contains("0x70000000 twice")));
    }

    #[test]
    fn a_repaint_keeps_the_atlas_size_and_a_full_one_leaves_no_free_square() {
        let base = atlas(64, [0.0; 4]);
        assert!(base.repaint(&image(32, 32, [0.0; 4])).unwrap_err().contains("keeps its size"));
        let full = base.repaint(&image(64, 64, [255.0; 4])).unwrap();
        assert_eq!(free_square(&full, &[]), None);
        let img = image(4, 4, [255.0; 4]);
        let sprites = [Sprite { key: 1, label: "a".into(), image: &img }];
        assert_eq!(pack(&full, &[], &sprites).err(), Some(PackError::NoFreeSquare));
    }

    #[test]
    fn sprite_names_and_sizes_are_checked() {
        assert!(name_refusal("").is_some());
        assert!(name_refusal("0x1234ABCD").unwrap().contains("written as a hash"));
        assert_eq!(name_refusal("qm_ring"), None);
        let dir = std::env::temp_dir().join(format!("qm_sprite_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, w: u32, h: u32| {
            let p = dir.join(name);
            let f = std::fs::File::create(&p).unwrap();
            let mut e = png::Encoder::new(std::io::BufWriter::new(f), w, h);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&vec![0u8; (w * h * 4) as usize]).unwrap();
            p
        };
        assert!(read_sprite(&write("ok.png", 64, 8)).is_ok());
        for (w, h) in [(48, 64), (2, 4), (1024, 4)] {
            let e = read_sprite(&write("bad.png", w, h)).unwrap_err();
            assert!(e.contains("a power of two from 4 to 512"), "{w}x{h}: {e}");
        }
        std::fs::write(dir.join("not.png"), b"not a png").unwrap();
        assert!(read_sprite(&dir.join("not.png")).is_err());
        assert!(read_sprite(&dir.join("missing.png")).is_err());
    }
}
