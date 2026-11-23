//! The main menu's label lettering, read from the player's own `shell_MAINMENU_text` and
//! `shell_MAINMENU_exit` textures, taken apart into the stencil's parts so labels retail never drew can
//! be set in the same lettering.
//!
//! Each texture holds every label twice, in bands one under the other: first the dark state (dark fill,
//! light outline), then the bright state (bright fill, dark outline), the letters at the same columns in
//! both. A label is a row of stencil letters; each letter is a few separate parts (a stem, two halves, a
//! half and two quarters, …) with a gap of one or two texels between them, and the outline is one even
//! band around the whole fill. So:
//!
//! - a letter is a group of fill columns whose gaps are at most [`STENCIL_GAP`]; the space between letters
//!   is wider;
//! - a part is a 4-connected region of fill inside a letter, carrying its texels from both states;
//! - the outline is not a part: it is regrown around the assembled fill from [`Outline`], measured from
//!   the retail bands (colour and alpha by distance from the fill).

use crate::game::GameStack;
use mercs2_formats::texture::TexFormat;

/// The widest gap inside a letter: wider gaps separate letters.
pub const STENCIL_GAP: usize = 3;
/// Texels with alpha at or below this are empty.
pub const ALPHA: u8 = 128;
/// The farthest an outline texel is from the fill, in texels.
pub const OUTLINE_REACH: usize = 10;

/// An RGBA image, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub px: Vec<[u8; 4]>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Image {
        Image { width, height, px: vec![[0; 4]; width * height] }
    }

    pub fn at(&self, x: usize, y: usize) -> [u8; 4] {
        self.px[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, t: [u8; 4]) {
        self.px[y * self.width + x] = t;
    }
}

/// The top mip of texture `name` from `game`, decoded.
pub fn read(game: &mut GameStack, name: &str) -> Result<Image, String> {
    let hash = mercs2_formats::hash::pandemic_hash_m2(name);
    let td = game.texture(hash).ok_or_else(|| format!("no texture {name} (0x{hash:08X})"))?;
    let fourcc: &[u8; 4] = match td.format {
        TexFormat::Bc1 => b"DXT1",
        TexFormat::Bc3 => b"DXT5",
    };
    let (w, h) = (td.width as usize, td.height as usize);
    let px = mercs2_formats::texture_encode::decode_dxt_surface(fourcc, w.div_ceil(4), h.div_ceil(4), &td.mip0)?;
    let bw = w.div_ceil(4) * 4;
    // The decoder's rows are whole blocks wide; keep the texture's own width.
    let px = (0..h).flat_map(|y| px[y * bw..y * bw + w].to_vec()).collect();
    Ok(Image { width: w, height: h, px })
}

/// Runs of `[start, end)` over `0..n` where `ink(i)` holds.
fn runs(n: usize, ink: impl Fn(usize) -> bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for i in 0..=n {
        match (start, i < n && ink(i)) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                out.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// The bands of rows that hold any texel with alpha above `alpha`: one per line of lettering.
pub fn bands(img: &Image, alpha: u8) -> Vec<(usize, usize)> {
    runs(img.height, |y| (0..img.width).any(|x| img.at(x, y)[3] > alpha))
}

/// The runs of columns in rows `band` that hold any texel with alpha above `alpha`.
pub fn columns(img: &Image, band: (usize, usize), alpha: u8) -> Vec<(usize, usize)> {
    runs(img.width, |x| (band.0..band.1).any(|y| img.at(x, y)[3] > alpha))
}

/// A texel's luminance, 0..=255 (Rec. 601 weights).
pub fn luma(t: [u8; 4]) -> u8 {
    ((u32::from(t[0]) * 299 + u32::from(t[1]) * 587 + u32::from(t[2]) * 114) / 1000) as u8
}

/// Which layer of the lettering a texel belongs to: the fill, either darker or brighter than `luma`,
/// at alpha above `alpha`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fill {
    Dark { alpha: u8, luma: u8 },
    Bright { alpha: u8, luma: u8 },
}

impl Fill {
    pub fn holds(&self, t: [u8; 4]) -> bool {
        match *self {
            Fill::Dark { alpha, luma: l } => t[3] > alpha && luma(t) < l,
            Fill::Bright { alpha, luma: l } => t[3] > alpha && luma(t) > l,
        }
    }
}

/// The runs of columns in rows `band` that hold any texel of `fill`: the letters, without the outline
/// that joins them.
pub fn fill_columns(img: &Image, band: (usize, usize), fill: Fill) -> Vec<(usize, usize)> {
    runs(img.width, |x| (band.0..band.1).any(|y| fill.holds(img.at(x, y))))
}

/// The two states' fills: the dark state's fill is below this luma, the bright state's above
/// [`BRIGHT_LUMA`]. In the dark bands the fill sits at luma 0–95 and the outline at 176–207, with the
/// fill's anti-aliased edge between; 80 keeps that edge out, so the gaps between letters stay at 4
/// texels or more (at 128 they close to 3 and NEW GAME reads as 5 letters).
pub const DARK_LUMA: u8 = 80;
pub const BRIGHT_LUMA: u8 = 64;

/// The two states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum State {
    Dark,
    Bright,
}

impl State {
    pub fn fill(self) -> Fill {
        match self {
            State::Dark => Fill::Dark { alpha: ALPHA, luma: DARK_LUMA },
            State::Bright => Fill::Bright { alpha: ALPHA, luma: BRIGHT_LUMA },
        }
    }
}

/// One part of a letter: its box in the texture (dark state's rows), its fill mask, and its texels in
/// both states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
    /// Row-major over the box: whether the texel is this part's fill.
    pub mask: Vec<bool>,
    pub dark: Vec<[u8; 4]>,
    pub bright: Vec<[u8; 4]>,
}

/// One letter of a line: its character, its columns, and its parts left to right, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    pub ch: char,
    pub x0: usize,
    pub x1: usize,
    pub parts: Vec<Part>,
}

/// One label of a texture: its text, its two bands, and its letters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub dark: (usize, usize),
    pub bright: (usize, usize),
    pub letters: Vec<Letter>,
}

/// The labels of `img`, whose texts are `texts` in band order: each label's dark band then its bright
/// band. Each label must split into exactly its letters, and every letter into parts present in both
/// states.
pub fn lines(img: &Image, texts: &[&str]) -> Result<Vec<Line>, String> {
    let all = bands(img, ALPHA);
    if all.len() != texts.len() * 2 {
        return Err(format!("{} bands of lettering; {} labels need {}", all.len(), texts.len(), texts.len() * 2));
    }
    let mut out = Vec::new();
    for (k, text) in texts.iter().enumerate() {
        let (dark, bright) = (all[2 * k], all[2 * k + 1]);
        let fail = |e: String| format!("{text:?}: {e}");
        // The bright band may be a texel taller or shorter; the parts are read at the dark band's rows,
        // shifted by the bands' offset.
        let shift = bright.0.checked_sub(dark.0).ok_or_else(|| fail("the bright band is above the dark one".into()))?;
        let runs = fill_columns(img, dark, State::Dark.fill());
        // Runs of fill grouped into letters: each group is a list of runs, split where a gap is wider than a
        // stencil gap.
        let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
        for r in runs {
            match groups.last_mut() {
                Some(g) if r.0 - g.last().map_or(r.0, |l| l.1) <= STENCIL_GAP => g.push(r),
                _ => groups.push(vec![r]),
            }
        }
        let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
        // Two letters set close (retail kerns P against A in PARTIE) read as one: while there are fewer letters
        // than the text has, split at the widest gap inside any letter.
        while groups.len() < chars.len() {
            let Some((g, at, _)) = groups
                .iter()
                .enumerate()
                .flat_map(|(g, runs)| runs.windows(2).enumerate().map(move |(i, w)| (g, i + 1, w[1].0 - w[0].1)))
                .max_by_key(|&(_, _, gap)| gap)
            else {
                break;
            };
            let tail = groups[g].split_off(at);
            groups.insert(g + 1, tail);
        }
        let groups: Vec<(usize, usize)> = groups.iter().map(|g| (g[0].0, g[g.len() - 1].1)).collect();
        if groups.len() != chars.len() {
            return Err(fail(format!("{} letters found, {} in the text: {groups:?}", groups.len(), chars.len())));
        }
        let mut letters = Vec::new();
        for (&ch, &(x0, x1)) in chars.iter().zip(&groups) {
            let parts = parts(img, dark, shift, x0, x1).map_err(|e| fail(format!("{ch}: {e}")))?;
            letters.push(Letter { ch, x0, x1, parts });
        }
        out.push(Line { text: text.to_string(), dark, bright, letters });
    }
    Ok(out)
}

/// The parts of the letter in columns `x0..x1` of rows `band`, each with its texels in both states (the
/// bright state `shift` rows down).
///
/// Which parts there are is read from the dark fill alone: its 4-connected regions, split exactly at the
/// stencil gaps. What a part covers is more: its dark fill, every hole that fill closes around (the
/// grain's light streaks, which are bright fill), and a rim one texel out where either state holds fill
/// (the fill's anti-aliased edge). A stencil gap runs out to the letter's edge, so it is never a hole.
fn parts(img: &Image, band: (usize, usize), shift: usize, x0: usize, x1: usize) -> Result<Vec<Part>, String> {
    let (w, h) = (x1 - x0, band.1 - band.0);
    let (dark, bright) = (State::Dark.fill(), State::Bright.fill());
    let either = |x: usize, y: usize| {
        dark.holds(img.at(x0 + x, band.0 + y)) || (band.0 + y + shift < img.height && bright.holds(img.at(x0 + x, band.0 + y + shift)))
    };
    let seeds = seed_parts(w, h, |x, y| dark.holds(img.at(x0 + x, band.0 + y)));
    let mut out = Vec::new();
    for cells in &seeds {
        let mut covered = vec![false; w * h];
        for &(x, y) in cells {
            covered[y * w + x] = true;
        }
        // Holes: what the outside of the letter's box cannot reach through texels off this part.
        let mut outside = vec![false; w * h];
        let mut stack: Vec<(usize, usize)> = (0..w).flat_map(|x| [(x, 0), (x, h - 1)]).chain((0..h).flat_map(|y| [(0, y), (w - 1, y)])).filter(|&(x, y)| !covered[y * w + x]).collect();
        for &(x, y) in &stack {
            outside[y * w + x] = true;
        }
        while let Some((x, y)) = stack.pop() {
            for (nx, ny) in neighbours(x, y, w, h) {
                if !covered[ny * w + nx] && !outside[ny * w + nx] {
                    outside[ny * w + nx] = true;
                    stack.push((nx, ny));
                }
            }
        }
        let others: std::collections::HashSet<(usize, usize)> = seeds.iter().filter(|c| !std::ptr::eq(*c, cells)).flatten().copied().collect();
        let mut cover: Vec<(usize, usize)> = cells.clone();
        for y in 0..h {
            for x in 0..w {
                if covered[y * w + x] {
                    continue;
                }
                let hole = !outside[y * w + x];
                let rim = either(x, y) && !others.contains(&(x, y)) && neighbours(x, y, w, h).any(|(nx, ny)| covered[ny * w + nx]);
                if hole || rim {
                    cover.push((x, y));
                }
            }
        }
        let (lx, hx) = (cover.iter().map(|c| c.0).min().unwrap_or(0), cover.iter().map(|c| c.0).max().unwrap_or(0) + 1);
        let (ly, hy) = (cover.iter().map(|c| c.1).min().unwrap_or(0), cover.iter().map(|c| c.1).max().unwrap_or(0) + 1);
        let (pw, ph) = (hx - lx, hy - ly);
        let mut part = Part {
            x: x0 + lx,
            y: band.0 + ly,
            width: pw,
            height: ph,
            mask: vec![false; pw * ph],
            dark: vec![[0; 4]; pw * ph],
            bright: vec![[0; 4]; pw * ph],
        };
        for &(x, y) in &cover {
            let i = (y - ly) * pw + (x - lx);
            part.mask[i] = true;
            part.dark[i] = img.at(x0 + x, band.0 + y);
            part.bright[i] = img.at(x0 + x, band.0 + y + shift);
        }
        out.push(part);
    }
    if out.is_empty() {
        return Err("no fill".into());
    }
    out.sort_by_key(|p| (p.x, p.y));
    Ok(out)
}

/// The 4-neighbours of `(x, y)` inside a `w` by `h` box.
fn neighbours(x: usize, y: usize, w: usize, h: usize) -> impl Iterator<Item = (usize, usize)> {
    [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)].into_iter().filter(move |&(nx, ny)| nx < w && ny < h)
}

/// The 4-connected regions of `ink` in a `w` by `h` box, larger than [`SPECK`] texels (smaller ones are
/// grain, not parts).
fn seed_parts(w: usize, h: usize, ink: impl Fn(usize, usize) -> bool) -> Vec<Vec<(usize, usize)>> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    for sy in 0..h {
        for sx in 0..w {
            if seen[sy * w + sx] || !ink(sx, sy) {
                continue;
            }
            seen[sy * w + sx] = true;
            let mut stack = vec![(sx, sy)];
            let mut cells = Vec::new();
            while let Some((x, y)) = stack.pop() {
                cells.push((x, y));
                for (nx, ny) in neighbours(x, y, w, h) {
                    if !seen[ny * w + nx] && ink(nx, ny) {
                        seen[ny * w + nx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if cells.len() > SPECK {
                out.push(cells);
            }
        }
    }
    out
}

/// The most texels a speck of grain has; a part has more.
pub const SPECK: usize = 12;

/// A state's outline: by distance from the fill (index 0 = one texel away), its mean colour and alpha,
/// measured from retail bands.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub by_distance: Vec<[u8; 4]>,
}

/// Each texel's distance to the nearest texel of `fill` (Euclidean, rounded up), up to
/// [`OUTLINE_REACH`]; 0 on the fill, `None` farther.
fn distances(width: usize, height: usize, fill: &[bool]) -> Vec<Option<usize>> {
    let r = OUTLINE_REACH as isize;
    let mut out = vec![None; width * height];
    for y in 0..height {
        for x in 0..width {
            let mut best: Option<f64> = None;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x as isize + dx, y as isize + dy);
                    if nx < 0 || ny < 0 || nx >= width as isize || ny >= height as isize || !fill[ny as usize * width + nx as usize] {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f64).sqrt();
                    if best.is_none_or(|b| d < b) {
                        best = Some(d);
                    }
                }
            }
            out[y * width + x] = best.filter(|d| *d <= OUTLINE_REACH as f64).map(|d| d.ceil() as usize);
        }
    }
    out
}

/// The outline of `state`, measured over `lines` of `img`: every texel off the fill, by its distance
/// from the fill, averaged (colour weighted by alpha).
pub fn measure_outline(img: &Image, lines: &[Line], state: State) -> Outline {
    let mut sum = vec![[0u64; 5]; OUTLINE_REACH + 1];
    for line in lines {
        let band = match state {
            State::Dark => line.dark,
            State::Bright => line.bright,
        };
        let (w, h) = (img.width, band.1 - band.0);
        // The fill is the parts' masks, as the rebuild places them.
        let mut fill = vec![false; w * h];
        for p in line.letters.iter().flat_map(|l| &l.parts) {
            for py in 0..p.height {
                for px in 0..p.width {
                    let y = p.y - line.dark.0 + py;
                    if p.mask[py * p.width + px] && y < h {
                        fill[y * w + p.x + px] = true;
                    }
                }
            }
        }
        let dist = distances(w, h, &fill);
        for y in 0..h {
            for x in 0..w {
                if let Some(d) = dist[y * w + x].filter(|d| *d > 0) {
                    let t = img.at(x, band.0 + y);
                    let a = u64::from(t[3]);
                    let s = &mut sum[d];
                    for k in 0..3 {
                        s[k] += u64::from(t[k]) * a;
                    }
                    s[3] += a;
                    s[4] += 1;
                }
            }
        }
    }
    let by_distance = sum[1..]
        .iter()
        .map(|s| {
            if s[4] == 0 || s[3] == 0 {
                return [0, 0, 0, 0];
            }
            [(s[0] / s[3]) as u8, (s[1] / s[3]) as u8, (s[2] / s[3]) as u8, (s[3] / s[4]) as u8]
        })
        .collect();
    Outline { by_distance }
}

/// One part placed: where its box's top-left goes on the canvas, and whether it is mirrored left-right
/// or top-bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed<'a> {
    pub part: &'a Part,
    pub x: isize,
    pub y: isize,
    pub mirror_x: bool,
    pub mirror_y: bool,
}

/// Parts set on a `width` by `height` canvas in `state`, with the outline regrown around their fill.
pub fn compose(width: usize, height: usize, placed: &[Placed<'_>], state: State, outline: &Outline) -> Image {
    let mut img = Image::new(width, height);
    let mut fill = vec![false; width * height];
    for p in placed {
        let texels = match state {
            State::Dark => &p.part.dark,
            State::Bright => &p.part.bright,
        };
        for py in 0..p.part.height {
            for px in 0..p.part.width {
                let sx = if p.mirror_x { p.part.width - 1 - px } else { px };
                let sy = if p.mirror_y { p.part.height - 1 - py } else { py };
                let i = sy * p.part.width + sx;
                if !p.part.mask[i] {
                    continue;
                }
                let (x, y) = (p.x + px as isize, p.y + py as isize);
                if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
                    continue;
                }
                img.set(x as usize, y as usize, texels[i]);
                fill[y as usize * width + x as usize] = true;
            }
        }
    }
    let dist = distances(width, height, &fill);
    for y in 0..height {
        for x in 0..width {
            if let Some(d) = dist[y * width + x].filter(|d| *d > 0) {
                img.set(x, y, outline.by_distance[d - 1]);
            }
        }
    }
    img
}

/// How far `a` is from `b` over the same size: the mean absolute difference per channel (alpha
/// premultiplied, so empty texels compare equal whatever their colour) and the share of texels off by more
/// than `tolerance` in any channel.
pub fn difference(a: &Image, b: &Image, tolerance: u8) -> Result<(f64, f64), String> {
    if (a.width, a.height) != (b.width, b.height) {
        return Err(format!("{}x{} against {}x{}", a.width, a.height, b.width, b.height));
    }
    let pre = |t: [u8; 4]| -> [i32; 4] {
        let a = i32::from(t[3]);
        [i32::from(t[0]) * a / 255, i32::from(t[1]) * a / 255, i32::from(t[2]) * a / 255, a]
    };
    let (mut total, mut off) = (0u64, 0usize);
    for (p, q) in a.px.iter().zip(&b.px) {
        let (p, q) = (pre(*p), pre(*q));
        let d: Vec<i32> = (0..4).map(|k| (p[k] - q[k]).abs()).collect();
        total += d.iter().map(|&v| v as u64).sum::<u64>();
        off += usize::from(d.iter().any(|&v| v > i32::from(tolerance)));
    }
    let n = a.px.len().max(1);
    Ok((total as f64 / (n * 4) as f64, off as f64 / n as f64))
}

impl Part {
    /// The rectangle `[x0, y0, x1)`×`[y0, y1)` of this part's box, as a part of its own.
    pub fn crop(&self, r: [usize; 4]) -> Result<Part, String> {
        let [x0, y0, x1, y1] = r;
        if x0 >= x1 || y0 >= y1 || x1 > self.width || y1 > self.height {
            return Err(format!("crop {r:?} is not inside a {}x{} part", self.width, self.height));
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let pick = |v: &[bool]| (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| v[y * self.width + x]).collect::<Vec<_>>();
        let pick_t = |v: &[[u8; 4]]| (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| v[y * self.width + x]).collect::<Vec<_>>();
        Ok(Part { x: self.x + x0, y: self.y + y0, width: w, height: h, mask: pick(&self.mask), dark: pick_t(&self.dark), bright: pick_t(&self.bright) })
    }
}

/// One part of a recipe: part `part` of the first retail letter `letter`, optionally cropped and
/// mirrored, its box's top-left at `(x, y)` in the letter (y from the top of the cap height).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRef {
    pub letter: char,
    pub part: usize,
    pub mirror_x: bool,
    pub mirror_y: bool,
    pub crop: Option<[usize; 4]>,
    /// The part whose texels fill this one's shape (tiled from its box's top-left), so a part can carry
    /// the grain of the part it joins.
    pub texels_from: Option<(char, usize)>,
    pub x: isize,
    pub y: isize,
}

/// A letter made of retail parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub ch: char,
    pub parts: Vec<PartRef>,
}

impl Recipe {
    /// `H=N.0@0,0 N.2@20,0 E.2[0,0,9,4]~N.2@10,11`: each part as `<letter>.<part>`, then `m` to mirror it
    /// left-right and `v` top-bottom, an optional crop `[x0,y0,x1,y1]`, an optional `~<letter>.<part>`
    /// whose texels fill it, and `@x,y`.
    pub fn parse(s: &str) -> Result<Recipe, String> {
        let (ch, rest) = s.split_once('=').ok_or_else(|| format!("{s:?}: no `=`"))?;
        let mut chars = ch.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else { return Err(format!("{s:?}: {ch:?} is not one character")) };
        let mut parts = Vec::new();
        for item in rest.split_whitespace() {
            let bad = |why: &str| format!("{s:?}: {item:?}: {why}");
            let (head, at) = item.split_once('@').ok_or_else(|| bad("no `@x,y`"))?;
            let (x, y) = at.split_once(',').ok_or_else(|| bad("`@x,y`"))?;
            let (x, y) = (x.parse::<isize>().map_err(|_| bad("x"))?, y.parse::<isize>().map_err(|_| bad("y"))?);
            let (head, texels_from) = match head.split_once('~') {
                Some((h, from)) => {
                    let (l, p) = from.split_once('.').ok_or_else(|| bad("`~<letter>.<part>`"))?;
                    let mut lc = l.chars();
                    let (Some(l), None) = (lc.next(), lc.next()) else { return Err(bad("the texel letter is not one character")) };
                    (h, Some((l, p.parse::<usize>().map_err(|_| bad("texel part number"))?)))
                }
                None => (head, None),
            };
            let (head, crop) = match head.split_once('[') {
                Some((h, c)) => {
                    let c = c.strip_suffix(']').ok_or_else(|| bad("crop has no `]`"))?;
                    let v: Vec<usize> = c.split(',').map(|n| n.parse::<usize>().map_err(|_| bad("crop"))).collect::<Result<_, _>>()?;
                    let [a, b, c, d] = v[..] else { return Err(bad("crop needs 4 numbers")) };
                    (h, Some([a, b, c, d]))
                }
                None => (head, None),
            };
            let (letter, part) = head.split_once('.').ok_or_else(|| bad("`<letter>.<part>`"))?;
            let mut lc = letter.chars();
            let (Some(letter), None) = (lc.next(), lc.next()) else { return Err(bad("the letter is not one character")) };
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            let flags = &part[digits.len()..];
            parts.push(PartRef {
                letter,
                part: digits.parse().map_err(|_| bad("part number"))?,
                mirror_x: flags.contains('m'),
                mirror_y: flags.contains('v'),
                crop,
                texels_from,
                x,
                y,
            });
        }
        if parts.is_empty() {
            return Err(format!("{s:?}: no parts"));
        }
        Ok(Recipe { ch, parts })
    }
}

/// The parts of `recipe`, read from the first letter of `lines` each names: where they go, cropped.
pub fn recipe_parts(lines: &[Line], recipe: &Recipe) -> Result<Vec<(Part, PartRef)>, String> {
    let find = |ch: char, i: usize| -> Result<&Part, String> {
        let letter = lines.iter().flat_map(|l| &l.letters).find(|l| l.ch == ch).ok_or_else(|| format!("{}: no retail letter {ch}", recipe.ch))?;
        letter.parts.get(i).ok_or_else(|| format!("{}: {ch} has {} parts, no #{i}", recipe.ch, letter.parts.len()))
    };
    let mut out = Vec::new();
    for r in &recipe.parts {
        let part = find(r.letter, r.part)?;
        let mut part = match r.crop {
            Some(c) => part.crop(c)?,
            None => part.clone(),
        };
        if let Some((ch, i)) = r.texels_from {
            let from = find(ch, i)?;
            // Only texels of the source's own fill: tile over the cells it covers, row by row.
            let cells: Vec<(usize, usize)> = (0..from.height).flat_map(|y| (0..from.width).map(move |x| (x, y))).filter(|&(x, y)| from.mask[y * from.width + x]).collect();
            if cells.is_empty() {
                return Err(format!("{}: {ch}.{i} has no fill to take texels from", recipe.ch));
            }
            for y in 0..part.height {
                for x in 0..part.width {
                    let (sx, sy) = (x % from.width, y % from.height);
                    let (sx, sy) = if from.mask[sy * from.width + sx] { (sx, sy) } else { cells[(y * part.width + x) % cells.len()] };
                    part.dark[y * part.width + x] = from.dark[sy * from.width + sx];
                    part.bright[y * part.width + x] = from.bright[sy * from.width + sx];
                }
            }
        }
        out.push((part, r.clone()));
    }
    Ok(out)
}

/// A letter's SVG (`data/lettering`, the cap from y = 0 to 1000 units) drawn with its cap `cap` pixels
/// high: its coverage, 0..=255, row-major, and its size.
pub fn rasterize(svg: &[u8], cap: f32) -> Result<(usize, usize, Vec<u8>), String> {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).map_err(|e| format!("SVG: {e}"))?;
    let size = tree.size();
    // The SVG's user unit is its pixel: 1000 of them are the cap height.
    let scale = cap / 1000.0;
    let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).round() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("an empty letter")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let coverage = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    Ok((w as usize, h as usize, coverage))
}

/// The cap height of the lettering, in texels.
pub const CAP: usize = 27;
/// Every letter SVG's room above the cap (accents) and below the baseline (a cedilla), in texels: a letter
/// drawn by [`rasterize`] has its cap line on row `SVG_ABOVE`.
pub const SVG_ABOVE: usize = 10;
pub const SVG_BELOW: usize = 6;

// `LETTERS`, from `data/lettering` (build.rs).
include!(concat!(env!("OUT_DIR"), "/lettering.rs"));

/// The SVG of `ch`'s letter in qm's lettering (`data/lettering/U+XXXX.svg`), if it has one.
pub fn letter(ch: char) -> Option<Vec<u8>> {
    LETTERS.binary_search_by_key(&ch, |(c, _)| *c).ok().map(|i| LETTERS[i].1.to_vec())
}

/// One column of retail fill, the full cap high, in both states: the grain new letters are filled with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrainColumn {
    pub dark: Vec<[u8; 4]>,
    pub bright: Vec<[u8; 4]>,
}

/// Every column of every part of `lines` that is fill from the cap line down for [`CAP`] texels: retail's
/// grain, in its own vertical streaks, with each state's texels at the same place.
pub fn grain_columns(lines: &[Line]) -> Vec<GrainColumn> {
    let mut out = Vec::new();
    for line in lines {
        let cap_top = cap_line(line);
        for p in line.letters.iter().flat_map(|l| &l.parts) {
            if p.y > cap_top || p.y + p.height < cap_top + CAP {
                continue;
            }
            for x in 0..p.width {
                let rows: Vec<usize> = (cap_top - p.y..cap_top - p.y + CAP).collect();
                if rows.iter().all(|&y| p.mask[y * p.width + x]) {
                    out.push(GrainColumn { dark: rows.iter().map(|&y| p.dark[y * p.width + x]).collect(), bright: rows.iter().map(|&y| p.bright[y * p.width + x]).collect() });
                }
            }
        }
    }
    out
}

/// A line's cap line: the top most of its letters share (round letters overshoot it).
pub fn cap_line(line: &Line) -> usize {
    let mut tops: Vec<usize> = line.letters.iter().map(|l| l.parts.iter().map(|p| p.y).min().unwrap_or(line.dark.0)).collect();
    tops.sort_unstable();
    tops.get(tops.len() / 2).copied().unwrap_or(line.dark.0)
}

/// How a label is set: the gap between letters and the width of a space, in texels, the rows above the
/// cap line and below the baseline (room for the ring), and how strong the grain's streaks are (1.0 is
/// retail's columns as they are; less moves each texel toward its state's mean fill colour).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Setting {
    pub gap: f32,
    pub space: f32,
    pub above: usize,
    pub below: usize,
    pub grain: f32,
}

/// Retail's spacing (letters 5 apart, a space 22, the cap 4 rows below the band's top and 5 above its
/// bottom) with the grain at half strength: retail's columns at full strength read heavier on our letters
/// than retail's labels do, and half is closest to them (user's choice 2026-11-21; also the smallest
/// difference from retail's OPTIONS of 1.0, 0.75 and 0.5).
pub const RETAIL_SETTING: Setting = Setting { gap: 5.0, space: 22.0, above: 4, below: 5, grain: 0.5 };

/// The mean fill colour of `grain` in each state.
fn grain_means(grain: &[GrainColumn]) -> ([f32; 4], [f32; 4]) {
    let mean = |pick: &dyn Fn(&GrainColumn) -> &Vec<[u8; 4]>| {
        let mut s = [0f64; 4];
        let mut n = 0f64;
        for t in grain.iter().flat_map(|c| pick(c).iter()) {
            for k in 0..4 {
                s[k] += f64::from(t[k]);
            }
            n += 1.0;
        }
        s.map(|v| (v / n.max(1.0)) as f32)
    };
    (mean(&|c| &c.dark), mean(&|c| &c.bright))
}

/// A small deterministic hash, so a label is filled with the same grain every time it is set.
fn mix(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// A label set in both states, one size: its letters start [`OUTLINE_REACH`] texels in from the left and
/// end as far in from the right, and the top of their cap is row `cap_row`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetLabel {
    pub dark: Image,
    pub bright: Image,
    pub cap_row: usize,
}

/// `text` set in the lettering whose letter SVGs `glyph` returns, in both states: each letter drawn at
/// the cap height and placed `setting.gap` after the last (a space `setting.space` wide), its fill taken a
/// column at a time from `grain`, its partly covered edge blended into the ring, and the ring regrown
/// around it from `outlines`. A character with no letter is refused, naming it.
pub fn set_label(
    text: &str,
    glyph: &dyn Fn(char) -> Option<Vec<u8>>,
    grain: &[GrainColumn],
    outlines: (&Outline, &Outline),
    setting: Setting,
) -> Result<SetLabel, String> {
    if grain.is_empty() {
        return Err("no grain columns".into());
    }
    // Each letter's coverage and where it goes.
    let mut placed: Vec<(usize, usize, Vec<u8>, f32)> = Vec::new();
    let mut pen = 0.0f32;
    let mut first = true;
    for ch in text.chars() {
        if ch == ' ' {
            pen += setting.space;
            first = true;
            continue;
        }
        let svg = glyph(ch).ok_or_else(|| format!("no letter for {ch:?} (U+{:04X})", ch as u32))?;
        let (w, h, cov) = rasterize(&svg, CAP as f32)?;
        if !first {
            pen += setting.gap;
        }
        placed.push((w, h, cov, pen));
        pen += w as f32;
        first = false;
    }
    let pad = OUTLINE_REACH;
    let width = pen.ceil() as usize + 2 * pad;
    // How far the label's marks reach above the cap line and below the baseline: the label grows by that.
    let (mut up, mut down) = (0usize, 0usize);
    for (w, h, cov, _) in &placed {
        let rows: Vec<usize> = (0..*h).filter(|&y| cov[y * w..(y + 1) * w].iter().any(|&c| c > 0)).collect();
        if let (Some(&first), Some(&last)) = (rows.first(), rows.last()) {
            up = up.max(SVG_ABOVE.saturating_sub(first));
            down = down.max((last + 1).saturating_sub(SVG_ABOVE + CAP));
        }
    }
    let cap_row = setting.above + up;
    let height = cap_row + CAP + down + setting.below;
    let mut cover = vec![0u8; width * height];
    // Which grain column each texel column of the label is filled from: each letter takes a run of
    // neighbouring retail columns (neighbours share their streaks) from a start its place picks.
    let mut column_of = vec![0usize; width];
    for (i, (w, h, cov, x)) in placed.iter().enumerate() {
        let x0 = pad + x.round() as usize;
        let start = (mix(i as u64 + 1) % grain.len() as u64) as usize;
        for cx in 0..*w {
            if x0 + cx < width {
                column_of[x0 + cx] = (start + cx) % grain.len();
            }
            for cy in 0..*h {
                // The letter's cap line (its row SVG_ABOVE) on the label's.
                let Some(ty) = (cap_row + cy).checked_sub(SVG_ABOVE) else { continue };
                let tx = x0 + cx;
                if tx < width && ty < height {
                    let c = &mut cover[ty * width + tx];
                    *c = (*c).max(cov[cy * w + cx]);
                }
            }
        }
    }
    let solid: Vec<bool> = cover.iter().map(|&c| c >= 128).collect();
    let dist = distances(width, height, &solid);
    let means = grain_means(grain);
    let draw = |state: State, outline: &Outline| -> Image {
        let mean = match state {
            State::Dark => means.0,
            State::Bright => means.1,
        };
        let mut img = Image::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let c = u32::from(cover[y * width + x]);
                let ring = match dist[y * width + x] {
                    Some(d) if d > 0 => outline.by_distance[d - 1],
                    _ => outline.by_distance[0],
                };
                if c == 0 {
                    if dist[y * width + x].is_some_and(|d| d > 0) {
                        img.set(x, y, ring);
                    }
                    continue;
                }
                let col = &grain[column_of[x]];
                // Marks above the cap and below the baseline take the grain's nearest row.
                let row = y.saturating_sub(cap_row).min(CAP - 1);
                let raw = match state {
                    State::Dark => col.dark[row],
                    State::Bright => col.bright[row],
                };
                let fill: [u8; 4] = std::array::from_fn(|k| (mean[k] + setting.grain * (f32::from(raw[k]) - mean[k])).round().clamp(0.0, 255.0) as u8);
                // A partly covered texel is the fill over the ring's nearest colour.
                let blend = |a: u8, b: u8| ((u32::from(a) * c + u32::from(b) * (255 - c)) / 255) as u8;
                img.set(x, y, [blend(fill[0], ring[0]), blend(fill[1], ring[1]), blend(fill[2], ring[2]), blend(fill[3], ring[3])]);
            }
        }
        img
    };
    Ok(SetLabel { dark: draw(State::Dark, outlines.0), bright: draw(State::Bright, outlines.1), cap_row })
}

/// Rows `band` of `img` as an image of their own.
pub fn band_image(img: &Image, band: (usize, usize)) -> Image {
    Image { width: img.width, height: band.1 - band.0, px: img.px[band.0 * img.width..band.1 * img.width].to_vec() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_split_on_gaps() {
        let ink = [0, 1, 1, 0, 0, 1, 0, 1];
        assert_eq!(runs(ink.len(), |i| ink[i] == 1), vec![(1, 3), (5, 6), (7, 8)]);
    }

    #[test]
    fn distances_grow_from_the_fill() {
        let fill = [false, false, true, false, false];
        let d = distances(5, 1, &fill);
        assert_eq!(d, vec![Some(2), Some(1), Some(0), Some(1), Some(2)]);
    }

    /// Every capital has a letter, and every letter draws with its cap at the lettering's cap height.
    #[test]
    fn the_lettering_has_every_capital() {
        for ch in 'A'..='Z' {
            let svg = letter(ch).unwrap_or_else(|| panic!("no letter {ch}"));
            let (_, h, cov) = rasterize(&svg, CAP as f32).expect("draw");
            assert_eq!(h, SVG_ABOVE + CAP + SVG_BELOW, "{ch}'s canvas");
            assert!(cov.iter().any(|&c| c > 0), "{ch} draws nothing");
        }
        assert!(letter('a').is_none() && letter('1').is_none());
        assert!(LETTERS.windows(2).all(|w| w[0].0 < w[1].0), "letters in character order");
    }

    #[test]
    fn a_mirrored_part_lands_flipped() {
        let part = Part { x: 0, y: 0, width: 2, height: 1, mask: vec![true, false], dark: vec![[9, 9, 9, 255], [0; 4]], bright: vec![[0; 4]; 2] };
        let outline = Outline { by_distance: vec![[0; 4]; OUTLINE_REACH] };
        let img = compose(2, 1, &[Placed { part: &part, x: 0, y: 0, mirror_x: true, mirror_y: false }], State::Dark, &outline);
        assert_eq!(img.at(1, 0), [9, 9, 9, 255]);
    }
}
