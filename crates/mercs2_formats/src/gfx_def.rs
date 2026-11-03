//! The definitions of a Scaleform **GFx 2.0.48** movie: what a player needs from its tag stream.
//!
//! [`MovieDef::decode`] turns a [`GfxMovie`] into characters (shapes, morphs, sprites, edit text,
//! fonts, external images) and a root [`Timeline`]. The tag set is the one the 83 retail movies use
//! (`docs/reverse_engineer/gfx_authoring_feature_spec.md` §1); any other tag is an error, so a movie
//! is never played with a piece missing.
//!
//! The GFx extension tags are read as their loaders read them (`FUN_007d6120` for 1001): retail
//! embeds no bitmaps, every image is a texture named by the tag.

use std::collections::{BTreeMap, HashMap};

use crate::gfx::{Cursor, GfxMovie};

pub type Rgba = [u8; 4];

/// A 2×3 transform: `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`, translation in twips.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 };

    /// `self` applied after `inner`.
    pub fn then(&self, inner: &Matrix) -> Matrix {
        Matrix {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            tx: self.a * inner.tx + self.c * inner.ty + self.tx,
            ty: self.b * inner.tx + self.d * inner.ty + self.ty,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> [f32; 2] {
        [self.a * x + self.c * y + self.tx, self.b * x + self.d * y + self.ty]
    }

    pub fn inverse(&self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det == 0.0 {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Matrix { a, b, c, d, tx: -(a * self.tx + c * self.ty), ty: -(b * self.tx + d * self.ty) })
    }

    pub fn lerp(&self, end: &Matrix, t: f32) -> Matrix {
        let mix = |s: f32, e: f32| s + (e - s) * t;
        Matrix {
            a: mix(self.a, end.a),
            b: mix(self.b, end.b),
            c: mix(self.c, end.c),
            d: mix(self.d, end.d),
            tx: mix(self.tx, end.tx),
            ty: mix(self.ty, end.ty),
        }
    }
}

/// A color transform: `out = in · mul + add`, `add` in 0..1 of full scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cxform {
    pub mul: [f32; 4],
    pub add: [f32; 4],
}

impl Cxform {
    pub const IDENTITY: Cxform = Cxform { mul: [1.0; 4], add: [0.0; 4] };

    /// `self` applied after `inner`.
    pub fn then(&self, inner: &Cxform) -> Cxform {
        let mut out = Cxform::IDENTITY;
        for k in 0..4 {
            out.mul[k] = self.mul[k] * inner.mul[k];
            out.add[k] = self.mul[k] * inner.add[k] + self.add[k];
        }
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    /// Flash 8 focal radial; the point is in `Gradient::focal`.
    Focal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    pub matrix: Matrix,
    /// 0 pad, 1 reflect, 2 repeat.
    pub spread: u8,
    /// 0 normal, 1 linear RGB.
    pub interpolation: u8,
    pub stops: Vec<(u8, Rgba)>,
    pub focal: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Fill {
    Solid(Rgba),
    Gradient(Gradient),
    /// `id` names a [`Character::Image`] (or a gradient image). `clamp` = clipped, `smooth` = filtered.
    Bitmap { id: u16, matrix: Matrix, clamp: bool, smooth: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub width: u16,
    pub color: Rgba,
    /// `DefineShape4` / `DefineMorphShape2` only: 0 round, 1 none, 2 square.
    pub start_cap: u8,
    pub end_cap: u8,
    /// 0 round, 1 bevel, 2 miter.
    pub join: u8,
    pub miter: f32,
    pub fill: Option<Fill>,
    pub no_close: bool,
}

/// One shape record, deltas in twips.
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Style {
        move_to: Option<(i32, i32)>,
        fill0: Option<u32>,
        fill1: Option<u32>,
        line: Option<u32>,
        /// New style arrays; every later index is into these.
        styles: Option<(Vec<Fill>, Vec<Line>)>,
    },
    Line {
        dx: i32,
        dy: i32,
    },
    Curve {
        cx: i32,
        cy: i32,
        ax: i32,
        ay: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub bounds: [i32; 4],
    pub fills: Vec<Fill>,
    pub lines: Vec<Line>,
    pub records: Vec<Record>,
}

/// A shape tween: two shapes sharing one style table, read at a ratio.
#[derive(Clone, Debug, PartialEq)]
pub struct Morph {
    pub fills: Vec<(Fill, Fill)>,
    pub lines: Vec<(Line, Line)>,
    pub start: Vec<Record>,
    pub end: Vec<Record>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EditText {
    pub bounds: [i32; 4],
    pub word_wrap: bool,
    pub multiline: bool,
    pub password: bool,
    pub read_only: bool,
    pub no_select: bool,
    pub border: bool,
    pub html: bool,
    pub outlines: bool,
    pub auto_size: bool,
    pub font: Option<u16>,
    pub height: u16,
    pub color: Option<Rgba>,
    pub max_length: Option<u16>,
    /// `(align, left margin, right margin, indent, leading)`; align 0 left, 1 right, 2 center, 3 justify.
    pub layout: Option<(u8, u16, u16, u16, i16)>,
    pub variable: String,
    pub text: String,
}

/// `GFx_DefineExternalImage`: `{id, format, w, h, export name, file name}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalImage {
    pub format: u16,
    pub width: u16,
    pub height: u16,
    pub export_name: String,
    pub file_name: String,
}

/// `GFx_DefineExternalGradientImage`: a gradient baked to a texture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalGradient {
    pub id: u16,
    pub format: u16,
    pub size: u16,
    pub file_name: String,
}

/// A font tag kept whole until text needs its glyph table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontTag {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A `DefineFont3` as the exporter leaves it: outlines stripped, glyphs in a texture.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Font {
    pub name: String,
    pub codes: Vec<u16>,
    /// Layout in font units of 20480 to the em.
    pub ascent: i16,
    pub descent: i16,
    pub leading: i16,
    pub advances: Vec<i16>,
}

impl FontTag {
    pub fn decode(&self) -> Result<Font, String> {
        let mut cur = Cursor::new(&self.bytes, 0);
        cur.u16()?;
        let flags = cur.u8()?;
        cur.u8()?;
        let name = pstring(&mut cur)?;
        let count = cur.u16()? as usize;
        let name = name.trim_end_matches('\0').to_string();
        let mut font = Font { name, ..Font::default() };
        if count == 0 {
            return Ok(font);
        }
        if flags & 0x0C != 0x04 {
            return Err(format!("font {}: flags 0x{flags:02x} (wide offsets or narrow codes)", font.name));
        }
        // The exporter keeps one empty offset, so the code table sits 4 bytes on.
        let (first, codes_at) = (cur.u16()?, cur.u16()?);
        if (first, codes_at) != (0, 4) {
            return Err(format!("font {}: an offset table of ({first}, {codes_at}), so it has outlines", font.name));
        }
        for _ in 0..count {
            font.codes.push(cur.u16()?);
        }
        if flags & 0x80 != 0 {
            font.ascent = i16le(&mut cur)?;
            font.descent = i16le(&mut cur)?;
            font.leading = i16le(&mut cur)?;
            for _ in 0..count {
                font.advances.push(i16le(&mut cur)?);
            }
        }
        Ok(font)
    }
}

/// A glyph's place in a font texture, in texture coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureGlyph {
    /// `[left, top, right, bottom]`.
    pub uv: [f32; 4],
    /// Where the pen sits for this glyph.
    pub origin: [f32; 2],
}

/// `GFx_FontTextureInfo`: one atlas and which font glyph sits where in it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FontTexture {
    pub file_name: String,
    pub width: u16,
    pub height: u16,
    /// The em size the glyphs were rasterized at, in pixels.
    pub nominal: u16,
    pub glyphs: Vec<TextureGlyph>,
    /// `(font id, [(font glyph, texture glyph)])`.
    pub fonts: Vec<(u16, Vec<(u16, u16)>)>,
}

impl FontTexture {
    pub fn decode(bytes: &[u8]) -> Result<FontTexture, String> {
        let mut cur = Cursor::new(bytes, 0);
        u32le(&mut cur)?;
        cur.u16()?;
        let file_name = pstring(&mut cur)?;
        let (width, height) = (cur.u16()?, cur.u16()?);
        cur.u8()?;
        let nominal = cur.u16()?;
        let f32le = |cur: &mut Cursor| -> Result<f32, String> { Ok(f32::from_bits(u32le(cur)?)) };
        let mut out = FontTexture { file_name, width, height, nominal, ..FontTexture::default() };
        for _ in 0..cur.u16()? {
            let uv = [f32le(&mut cur)?, f32le(&mut cur)?, f32le(&mut cur)?, f32le(&mut cur)?];
            out.glyphs.push(TextureGlyph { uv, origin: [f32le(&mut cur)?, f32le(&mut cur)?] });
        }
        for _ in 0..cur.u16()? {
            let id = cur.u16()?;
            let mut pairs = Vec::new();
            for _ in 0..cur.u16()? {
                pairs.push((cur.u16()?, cur.u16()?));
            }
            out.fonts.push((id, pairs));
        }
        if cur.pos != bytes.len() {
            return Err(format!("{}: {} bytes past the font table", out.file_name, bytes.len() - cur.pos));
        }
        Ok(out)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Character {
    Shape(Shape),
    Morph(Morph),
    Sprite(Timeline),
    EditText(EditText),
    Font(FontTag),
    Image(ExternalImage),
}

/// `PlaceObject2`/`PlaceObject3`: each field only when the tag sets it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Place {
    pub depth: u16,
    /// Moves the object already at `depth` instead of placing a new one.
    pub is_move: bool,
    pub character: Option<u16>,
    pub matrix: Option<Matrix>,
    pub cxform: Option<Cxform>,
    pub ratio: Option<u16>,
    pub name: Option<String>,
    pub clip_depth: Option<u16>,
    pub blend_mode: Option<u8>,
    pub filters: Vec<u8>,
    pub clip_actions: Vec<ClipAction>,
}

/// `onClipEvent` handler of a placed sprite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipAction {
    pub events: u32,
    pub key: u8,
    pub code: Vec<u8>,
}

/// The display and script tags of one frame, in stream order.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Place(Place),
    Remove { depth: u16 },
    Action(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Timeline {
    /// The count the header declares.
    pub declared_frames: u16,
    pub frames: Vec<Vec<Control>>,
    /// Frame label → zero-based frame.
    pub labels: BTreeMap<String, u16>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MovieDef {
    /// Stage rectangle in twips, `[xmin, xmax, ymin, ymax]`.
    pub stage: [i32; 4],
    pub frame_rate: f32,
    pub background: Option<Rgba>,
    pub characters: HashMap<u16, Character>,
    pub timeline: Timeline,
    /// Export name → character.
    pub exports: BTreeMap<String, u16>,
    /// `(source url, character id, name)`.
    pub imports: Vec<(String, u16, String)>,
    pub gradients: Vec<ExternalGradient>,
    /// `GFx_FontTextureInfo` bodies, decoded with fonts.
    pub font_textures: Vec<Vec<u8>>,
    /// The name `GFx_ExporterInfo` carries, `shell` for `SHELL.gfx`.
    pub exporter_name: String,
}

fn sbits(cur: &mut Cursor, n: u32) -> Result<i32, String> {
    if n == 0 {
        return Ok(0);
    }
    let v = cur.ubits(n)?;
    Ok(if n < 32 && v & (1 << (n - 1)) != 0 { (v as i32) - (1i32 << n) } else { v as i32 })
}

fn u32le(cur: &mut Cursor) -> Result<u32, String> {
    let lo = cur.u16()? as u32;
    Ok(lo | (cur.u16()? as u32) << 16)
}

fn i16le(cur: &mut Cursor) -> Result<i16, String> {
    Ok(cur.u16()? as i16)
}

fn cstring(cur: &mut Cursor) -> Result<String, String> {
    cur.align();
    let rest = cur.data.get(cur.pos..).ok_or("eof:string")?;
    let len = rest.iter().position(|b| *b == 0).ok_or("unterminated string")?;
    let s = String::from_utf8_lossy(&rest[..len]).into_owned();
    cur.pos += len + 1;
    Ok(s)
}

/// A GFx string: `u8` length, then the bytes.
fn pstring(cur: &mut Cursor) -> Result<String, String> {
    let len = cur.u8()? as usize;
    let bytes = cur.data.get(cur.pos..cur.pos + len).ok_or("eof:gfx string")?;
    let s = String::from_utf8_lossy(bytes).into_owned();
    cur.pos += len;
    Ok(s)
}

fn take_rest(cur: &mut Cursor) -> Vec<u8> {
    cur.align();
    let out = cur.data[cur.pos.min(cur.data.len())..].to_vec();
    cur.pos = cur.data.len();
    out
}

fn matrix(cur: &mut Cursor) -> Result<Matrix, String> {
    cur.align();
    let mut m = Matrix::IDENTITY;
    if cur.ubits(1)? == 1 {
        let n = cur.ubits(5)?;
        m.a = sbits(cur, n)? as f32 / 65536.0;
        m.d = sbits(cur, n)? as f32 / 65536.0;
    }
    if cur.ubits(1)? == 1 {
        let n = cur.ubits(5)?;
        m.b = sbits(cur, n)? as f32 / 65536.0;
        m.c = sbits(cur, n)? as f32 / 65536.0;
    }
    let n = cur.ubits(5)?;
    m.tx = sbits(cur, n)? as f32;
    m.ty = sbits(cur, n)? as f32;
    cur.align();
    Ok(m)
}

fn cxform(cur: &mut Cursor, alpha: bool) -> Result<Cxform, String> {
    cur.align();
    let has_add = cur.ubits(1)? == 1;
    let has_mul = cur.ubits(1)? == 1;
    let n = cur.ubits(4)?;
    let channels = if alpha { 4 } else { 3 };
    let mut out = Cxform::IDENTITY;
    if has_mul {
        for k in 0..channels {
            out.mul[k] = sbits(cur, n)? as f32 / 256.0;
        }
    }
    if has_add {
        for k in 0..channels {
            out.add[k] = sbits(cur, n)? as f32 / 255.0;
        }
    }
    cur.align();
    Ok(out)
}

fn rgb(cur: &mut Cursor) -> Result<Rgba, String> {
    Ok([cur.u8()?, cur.u8()?, cur.u8()?, 255])
}

fn rgba(cur: &mut Cursor) -> Result<Rgba, String> {
    Ok([cur.u8()?, cur.u8()?, cur.u8()?, cur.u8()?])
}

fn color(cur: &mut Cursor, alpha: bool) -> Result<Rgba, String> {
    if alpha {
        rgba(cur)
    } else {
        rgb(cur)
    }
}

fn bitmap_flags(kind: u8) -> (bool, bool) {
    // 0x40 tiled, 0x41 clipped, 0x42 tiled hard, 0x43 clipped hard.
    (kind & 1 != 0, kind & 2 == 0)
}

fn gradient_kind(kind: u8) -> GradientKind {
    match kind {
        0x10 => GradientKind::Linear,
        0x12 => GradientKind::Radial,
        _ => GradientKind::Focal,
    }
}

fn fill(cur: &mut Cursor, alpha: bool) -> Result<Fill, String> {
    let kind = cur.u8()?;
    match kind {
        0x00 => Ok(Fill::Solid(color(cur, alpha)?)),
        0x10 | 0x12 | 0x13 => {
            let matrix = matrix(cur)?;
            let head = cur.u8()?;
            let mut stops = Vec::new();
            for _ in 0..(head & 0x0F) {
                let ratio = cur.u8()?;
                stops.push((ratio, color(cur, alpha)?));
            }
            let focal = if kind == 0x13 { i16le(cur)? as f32 / 256.0 } else { 0.0 };
            Ok(Fill::Gradient(Gradient {
                kind: gradient_kind(kind),
                matrix,
                spread: head >> 6,
                interpolation: (head >> 4) & 3,
                stops,
                focal,
            }))
        }
        0x40..=0x43 => {
            let id = cur.u16()?;
            let matrix = matrix(cur)?;
            let (clamp, smooth) = bitmap_flags(kind);
            Ok(Fill::Bitmap { id, matrix, clamp, smooth })
        }
        other => Err(format!("unknown fill type 0x{other:02x}")),
    }
}

fn count(cur: &mut Cursor, extended: bool) -> Result<usize, String> {
    let n = cur.u8()? as usize;
    if extended && n == 0xFF {
        Ok(cur.u16()? as usize)
    } else {
        Ok(n)
    }
}

/// The flags of a `LINESTYLE2`.
struct LineFlags {
    start_cap: u8,
    join: u8,
    has_fill: bool,
    no_close: bool,
    end_cap: u8,
}

fn line_flags(cur: &mut Cursor) -> Result<LineFlags, String> {
    let a = cur.u8()?;
    let b = cur.u8()?;
    Ok(LineFlags {
        start_cap: a >> 6,
        join: (a >> 4) & 3,
        has_fill: a & 8 != 0,
        no_close: b & 4 != 0,
        end_cap: b & 3,
    })
}

fn styles(cur: &mut Cursor, version: u8) -> Result<(Vec<Fill>, Vec<Line>), String> {
    let alpha = version >= 3;
    let mut fills = Vec::new();
    for _ in 0..count(cur, version >= 2)? {
        fills.push(fill(cur, alpha)?);
    }
    let mut lines = Vec::new();
    for _ in 0..count(cur, true)? {
        let width = cur.u16()?;
        if version == 4 {
            let f = line_flags(cur)?;
            let miter = if f.join == 2 { cur.u16()? as f32 / 256.0 } else { 3.0 };
            let (line_color, line_fill) = if f.has_fill { ([0; 4], Some(fill(cur, true)?)) } else { (rgba(cur)?, None) };
            lines.push(Line {
                width,
                color: line_color,
                start_cap: f.start_cap,
                end_cap: f.end_cap,
                join: f.join,
                miter,
                fill: line_fill,
                no_close: f.no_close,
            });
        } else {
            lines.push(Line {
                width,
                color: color(cur, alpha)?,
                start_cap: 0,
                end_cap: 0,
                join: 0,
                miter: 3.0,
                fill: None,
                no_close: false,
            });
        }
    }
    Ok((fills, lines))
}

fn records(cur: &mut Cursor, version: u8) -> Result<Vec<Record>, String> {
    cur.align();
    let mut fill_bits = cur.ubits(4)?;
    let mut line_bits = cur.ubits(4)?;
    let mut out = Vec::new();
    loop {
        if cur.ubits(1)? == 0 {
            let flags = cur.ubits(5)?;
            if flags == 0 {
                cur.align();
                return Ok(out);
            }
            let move_to = if flags & 1 != 0 {
                let n = cur.ubits(5)?;
                let x = sbits(cur, n)?;
                Some((x, sbits(cur, n)?))
            } else {
                None
            };
            let fill0 = if flags & 2 != 0 { Some(cur.ubits(fill_bits)?) } else { None };
            let fill1 = if flags & 4 != 0 { Some(cur.ubits(fill_bits)?) } else { None };
            let line = if flags & 8 != 0 { Some(cur.ubits(line_bits)?) } else { None };
            let new_styles = if flags & 16 != 0 {
                cur.align();
                let table = styles(cur, version)?;
                fill_bits = cur.ubits(4)?;
                line_bits = cur.ubits(4)?;
                Some(table)
            } else {
                None
            };
            out.push(Record::Style { move_to, fill0, fill1, line, styles: new_styles });
        } else if cur.ubits(1)? == 1 {
            let n = cur.ubits(4)? + 2;
            let (dx, dy) = if cur.ubits(1)? == 1 {
                let x = sbits(cur, n)?;
                (x, sbits(cur, n)?)
            } else if cur.ubits(1)? == 1 {
                (0, sbits(cur, n)?)
            } else {
                (sbits(cur, n)?, 0)
            };
            out.push(Record::Line { dx, dy });
        } else {
            let n = cur.ubits(4)? + 2;
            let cx = sbits(cur, n)?;
            let cy = sbits(cur, n)?;
            let ax = sbits(cur, n)?;
            out.push(Record::Curve { cx, cy, ax, ay: sbits(cur, n)? });
        }
    }
}

fn shape(tag: &[u8], version: u8) -> Result<(u16, Shape), String> {
    let mut cur = Cursor::new(tag, 0);
    let id = cur.u16()?;
    let bounds = cur.read_rect()?;
    if version == 4 {
        cur.read_rect()?;
        cur.u8()?;
    }
    let (fills, lines) = styles(&mut cur, version)?;
    let records = records(&mut cur, version)?;
    Ok((id, Shape { bounds, fills, lines, records }))
}

fn morph_fill(cur: &mut Cursor, version: u8) -> Result<(Fill, Fill), String> {
    let kind = cur.u8()?;
    match kind {
        0x00 => Ok((Fill::Solid(rgba(cur)?), Fill::Solid(rgba(cur)?))),
        0x10 | 0x12 | 0x13 => {
            let start_matrix = matrix(cur)?;
            let end_matrix = matrix(cur)?;
            let head = cur.u8()?;
            let (mut start, mut end) = (Vec::new(), Vec::new());
            for _ in 0..(head & 0x0F) {
                let ratio = cur.u8()?;
                start.push((ratio, rgba(cur)?));
                let ratio = cur.u8()?;
                end.push((ratio, rgba(cur)?));
            }
            let (focal0, focal1) = if kind == 0x13 && version == 2 {
                let f0 = i16le(cur)? as f32 / 256.0;
                (f0, i16le(cur)? as f32 / 256.0)
            } else {
                (0.0, 0.0)
            };
            let gradient = |matrix, stops, focal| {
                Fill::Gradient(Gradient {
                    kind: gradient_kind(kind),
                    matrix,
                    spread: head >> 6,
                    interpolation: (head >> 4) & 3,
                    stops,
                    focal,
                })
            };
            Ok((gradient(start_matrix, start, focal0), gradient(end_matrix, end, focal1)))
        }
        0x40..=0x43 => {
            let id = cur.u16()?;
            let (clamp, smooth) = bitmap_flags(kind);
            let start = Fill::Bitmap { id, matrix: matrix(cur)?, clamp, smooth };
            let end = Fill::Bitmap { id, matrix: matrix(cur)?, clamp, smooth };
            Ok((start, end))
        }
        other => Err(format!("unknown morph fill type 0x{other:02x}")),
    }
}

fn morph(tag: &[u8], version: u8) -> Result<(u16, Morph), String> {
    let mut cur = Cursor::new(tag, 0);
    let id = cur.u16()?;
    cur.read_rect()?;
    cur.read_rect()?;
    if version == 2 {
        cur.read_rect()?;
        cur.read_rect()?;
        cur.u8()?;
    }
    let offset = u32le(&mut cur)? as usize;
    let end_at = cur.pos + offset;
    let mut fills = Vec::new();
    for _ in 0..count(&mut cur, true)? {
        fills.push(morph_fill(&mut cur, version)?);
    }
    let mut lines = Vec::new();
    for _ in 0..count(&mut cur, true)? {
        let (w0, w1) = (cur.u16()?, cur.u16()?);
        let plain = |width: u16, color: Rgba, f: &Option<LineFlags>, miter: f32, fill: Option<Fill>| Line {
            width,
            color,
            start_cap: f.as_ref().map_or(0, |f| f.start_cap),
            end_cap: f.as_ref().map_or(0, |f| f.end_cap),
            join: f.as_ref().map_or(0, |f| f.join),
            miter,
            fill,
            no_close: f.as_ref().is_some_and(|f| f.no_close),
        };
        if version == 2 {
            let f = line_flags(&mut cur)?;
            let miter = if f.join == 2 { cur.u16()? as f32 / 256.0 } else { 3.0 };
            let f = Some(f);
            if f.as_ref().is_some_and(|f| f.has_fill) {
                let (a, b) = morph_fill(&mut cur, version)?;
                lines.push((plain(w0, [0; 4], &f, miter, Some(a)), plain(w1, [0; 4], &f, miter, Some(b))));
            } else {
                let (c0, c1) = (rgba(&mut cur)?, rgba(&mut cur)?);
                lines.push((plain(w0, c0, &f, miter, None), plain(w1, c1, &f, miter, None)));
            }
        } else {
            let (c0, c1) = (rgba(&mut cur)?, rgba(&mut cur)?);
            lines.push((plain(w0, c0, &None, 3.0, None), plain(w1, c1, &None, 3.0, None)));
        }
    }
    let start = records(&mut cur, 3)?;
    let mut end_cur = Cursor::new(tag, end_at);
    let end = records(&mut end_cur, 3)?;
    Ok((id, Morph { fills, lines, start, end }))
}

fn edit_text(tag: &[u8]) -> Result<(u16, EditText), String> {
    let mut cur = Cursor::new(tag, 0);
    let id = cur.u16()?;
    let bounds = cur.read_rect()?;
    let a = cur.u8()?;
    let b = cur.u8()?;
    let mut out = EditText {
        bounds,
        word_wrap: a & 0x40 != 0,
        multiline: a & 0x20 != 0,
        password: a & 0x10 != 0,
        read_only: a & 0x08 != 0,
        no_select: b & 0x10 != 0,
        border: b & 0x08 != 0,
        html: b & 0x02 != 0,
        outlines: b & 0x01 != 0,
        auto_size: b & 0x40 != 0,
        ..EditText::default()
    };
    if a & 0x01 != 0 {
        out.font = Some(cur.u16()?);
    }
    if b & 0x80 != 0 {
        cstring(&mut cur)?;
    }
    if a & 0x01 != 0 {
        out.height = cur.u16()?;
    }
    if a & 0x04 != 0 {
        out.color = Some(rgba(&mut cur)?);
    }
    if a & 0x02 != 0 {
        out.max_length = Some(cur.u16()?);
    }
    if b & 0x20 != 0 {
        let align = cur.u8()?;
        let (left, right, indent) = (cur.u16()?, cur.u16()?, cur.u16()?);
        out.layout = Some((align, left, right, indent, i16le(&mut cur)?));
    }
    out.variable = cstring(&mut cur)?;
    if a & 0x80 != 0 {
        out.text = cstring(&mut cur)?;
    }
    Ok((id, out))
}

fn clip_actions(cur: &mut Cursor) -> Result<Vec<ClipAction>, String> {
    cur.u16()?;
    u32le(cur)?;
    let mut out = Vec::new();
    loop {
        let events = u32le(cur)?;
        if events == 0 {
            return Ok(out);
        }
        let mut size = u32le(cur)? as usize;
        let key = if events & 0x0002_0000 != 0 {
            size = size.checked_sub(1).ok_or("empty key press clip action")?;
            cur.u8()?
        } else {
            0
        };
        let code = cur.data.get(cur.pos..cur.pos + size).ok_or("eof:clip action")?.to_vec();
        cur.pos += size;
        out.push(ClipAction { events, key, code });
    }
}

fn filter_list(cur: &mut Cursor) -> Result<Vec<u8>, String> {
    let start = cur.pos;
    for _ in 0..cur.u8()? {
        let size = match cur.u8()? {
            0 => 23,
            1 => 9,
            2 => 15,
            3 => 27,
            6 => 80,
            other => return Err(format!("filter kind {other} in a PlaceObject3")),
        };
        cur.pos += size;
    }
    Ok(cur.data[start..cur.pos].to_vec())
}

fn place(tag: &[u8], three: bool) -> Result<Place, String> {
    let mut cur = Cursor::new(tag, 0);
    let flags = cur.u8()?;
    let flags3 = if three { cur.u8()? } else { 0 };
    let mut out = Place { depth: cur.u16()?, is_move: flags & 1 != 0, ..Place::default() };
    if flags3 & 0x08 != 0 || (flags & 2 != 0 && flags3 & 0x10 != 0) {
        cstring(&mut cur)?;
    }
    if flags & 2 != 0 {
        out.character = Some(cur.u16()?);
    }
    if flags & 4 != 0 {
        out.matrix = Some(matrix(&mut cur)?);
    }
    if flags & 8 != 0 {
        out.cxform = Some(cxform(&mut cur, true)?);
    }
    if flags & 0x10 != 0 {
        out.ratio = Some(cur.u16()?);
    }
    if flags & 0x20 != 0 {
        out.name = Some(cstring(&mut cur)?);
    }
    if flags & 0x40 != 0 {
        out.clip_depth = Some(cur.u16()?);
    }
    if flags3 & 0x01 != 0 {
        out.filters = filter_list(&mut cur)?;
    }
    if flags3 & 0x02 != 0 {
        out.blend_mode = Some(cur.u8()?);
    }
    if flags3 & 0x04 != 0 {
        cur.u8()?;
    }
    if flags & 0x80 != 0 {
        out.clip_actions = clip_actions(&mut cur)?;
    }
    Ok(out)
}

/// Add a control or structural tag to `line`, true when the tag was one.
fn timeline_tag(code: u16, body: &[u8], line: &mut Timeline, frame: &mut Vec<Control>) -> Result<bool, String> {
    match code {
        1 => line.frames.push(std::mem::take(frame)),
        12 => frame.push(Control::Action(body.to_vec())),
        26 => frame.push(Control::Place(place(body, false)?)),
        70 => frame.push(Control::Place(place(body, true)?)),
        28 => frame.push(Control::Remove { depth: u16::from_le_bytes([body[0], body[1]]) }),
        43 => {
            let mut cur = Cursor::new(body, 0);
            line.labels.insert(cstring(&mut cur)?, line.frames.len() as u16);
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn sprite(tag: &[u8]) -> Result<(u16, Timeline), String> {
    let mut cur = Cursor::new(tag, 0);
    let id = cur.u16()?;
    let mut line = Timeline { declared_frames: cur.u16()?, ..Timeline::default() };
    let mut frame = Vec::new();
    while cur.pos < tag.len() {
        let head = cur.u16()?;
        let (code, mut len) = (head >> 6, (head & 0x3F) as usize);
        if len == 0x3F {
            len = u32le(&mut cur)? as usize;
        }
        let body = tag.get(cur.pos..cur.pos + len).ok_or_else(|| format!("sprite {id}: tag {code} overruns"))?;
        cur.pos += len;
        if code == 0 {
            break;
        }
        if !timeline_tag(code, body, &mut line, &mut frame)? {
            return Err(format!("sprite {id}: unhandled tag {code} ({})", crate::gfx::tag_name(code)));
        }
    }
    if !frame.is_empty() {
        line.frames.push(frame);
    }
    Ok((id, line))
}

impl MovieDef {
    pub fn decode(movie: &GfxMovie) -> Result<MovieDef, String> {
        let mut out = MovieDef {
            stage: movie.frame_size,
            frame_rate: movie.frame_rate,
            timeline: Timeline { declared_frames: movie.frame_count, ..Timeline::default() },
            ..MovieDef::default()
        };
        let mut frame = Vec::new();
        for (code, body) in movie.tag_bodies() {
            out.tag(code, body, &mut frame).map_err(|e| format!("tag {code} ({}): {e}", crate::gfx::tag_name(code)))?;
        }
        if !frame.is_empty() {
            out.timeline.frames.push(frame);
        }
        Ok(out)
    }

    fn tag(&mut self, code: u16, body: &[u8], frame: &mut Vec<Control>) -> Result<(), String> {
        if timeline_tag(code, body, &mut self.timeline, frame)? {
            return Ok(());
        }
        let mut cur = Cursor::new(body, 0);
        match code {
            0 | 24 | 69 | 74 => {}
            2 | 22 | 32 | 83 => {
                let version = match code {
                    2 => 1,
                    22 => 2,
                    32 => 3,
                    _ => 4,
                };
                let (id, s) = shape(body, version)?;
                self.characters.insert(id, Character::Shape(s));
            }
            46 | 84 => {
                let (id, m) = morph(body, if code == 46 { 1 } else { 2 })?;
                self.characters.insert(id, Character::Morph(m));
            }
            9 => self.background = Some(rgb(&mut cur)?),
            37 => {
                let (id, t) = edit_text(body)?;
                self.characters.insert(id, Character::EditText(t));
            }
            39 => {
                let (id, t) = sprite(body)?;
                self.characters.insert(id, Character::Sprite(t));
            }
            56 => {
                for _ in 0..cur.u16()? {
                    let id = cur.u16()?;
                    self.exports.insert(cstring(&mut cur)?, id);
                }
            }
            71 => {
                let url = cstring(&mut cur)?;
                cur.u16()?;
                for _ in 0..cur.u16()? {
                    let id = cur.u16()?;
                    self.imports.push((url.clone(), id, cstring(&mut cur)?));
                }
            }
            75 => {
                let id = cur.u16()?;
                cur.u16()?;
                let name = pstring(&mut cur)?;
                self.characters.insert(id, Character::Font(FontTag { name, bytes: body.to_vec() }));
            }
            1000 => {
                cur.u16()?;
                u32le(&mut cur)?;
                cur.u16()?;
                pstring(&mut cur)?;
                self.exporter_name = pstring(&mut cur)?;
            }
            1001 => {
                let id = cur.u16()?;
                let format = cur.u16()?;
                let width = cur.u16()?;
                let height = cur.u16()?;
                let export_name = pstring(&mut cur)?;
                // `snatch_moment` and both fanfares store the one name.
                let file_name = if cur.pos < body.len() { pstring(&mut cur)? } else { export_name.clone() };
                self.characters.insert(id, Character::Image(ExternalImage { format, width, height, export_name, file_name }));
            }
            1002 => self.font_textures.push(take_rest(&mut cur)),
            1003 => {
                let id = cur.u16()?;
                let format = cur.u16()?;
                let size = cur.u16()?;
                self.gradients.push(ExternalGradient { id, format, size, file_name: pstring(&mut cur)? });
            }
            // Its three retail instances (`snatch_moment`, both fanfares) are these same 4 bytes and name
            // no image; the name `GFx_DefineSubImage` was never proven. Anything else stops the decode.
            1004 if body == [1, 0, 0, 0] => {}
            other => return Err(format!("unhandled tag {other}: {body:02x?}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_composes_like_nested_clips() {
        let parent = Matrix { a: 2.0, tx: 100.0, ..Matrix::IDENTITY };
        let child = Matrix { d: 3.0, tx: 10.0, ty: 5.0, ..Matrix::IDENTITY };
        assert_eq!(parent.then(&child).apply(1.0, 1.0), parent.apply(child.apply(1.0, 1.0)[0], child.apply(1.0, 1.0)[1]));
    }

    #[test]
    fn inverse_undoes_a_matrix() {
        let m = Matrix { a: 2.0, b: 1.0, c: -1.0, d: 3.0, tx: 7.0, ty: -4.0 };
        let [x, y] = m.apply(5.0, 9.0);
        let [bx, by] = m.inverse().expect("invertible").apply(x, y);
        assert!((bx - 5.0).abs() < 1e-4 && (by - 9.0).abs() < 1e-4, "{bx} {by}");
    }

    #[test]
    fn a_color_transform_composes_outer_over_inner() {
        let outer = Cxform { mul: [0.5; 4], add: [0.25; 4] };
        let inner = Cxform { mul: [0.5; 4], add: [0.5; 4] };
        let both = outer.then(&inner);
        assert_eq!(both.mul[0], 0.25);
        assert_eq!(both.add[0], 0.5);
    }

    #[test]
    fn a_shape_with_a_curve_decodes_its_records() {
        // A hand-assembled DefineShape3 body: id 7, empty bounds, one solid fill, no lines, then
        // move + straight + curve + end, checked field by field.
        let mut bits = Vec::<bool>::new();
        let mut push = |v: u32, n: u32| {
            for k in (0..n).rev() {
                bits.push(v >> k & 1 == 1);
            }
        };
        push(1, 4); // fill bits
        push(0, 4); // line bits
        push(0, 1); // style change
        push(0b00101, 5); // fill1 + move
        push(4, 5);
        push(3, 4);
        push(5, 4);
        push(1, 1); // fill1 = 1
        push(1, 1); // edge
        push(1, 1); // straight
        push(1, 4); // 3 bits
        push(0, 1); // not general
        push(1, 1); // vertical
        push(0b011, 3); // dy 3
        push(1, 1); // edge
        push(0, 1); // curve
        push(1, 4); // 3 bits
        for v in [1, 2, 3, 0] {
            push(v, 3);
        }
        push(0, 1); // style
        push(0, 5); // end
        let mut record_bytes = vec![0u8; bits.len().div_ceil(8)];
        for (i, b) in bits.iter().enumerate() {
            if *b {
                record_bytes[i / 8] |= 0x80 >> (i % 8);
            }
        }
        let mut tag = vec![7, 0, 0x00]; // id, RECT with 0 bits
        tag.extend([1, 0x00, 10, 20, 30, 40]); // one solid fill
        tag.push(0); // no lines
        tag.extend(record_bytes);
        let (id, s) = shape(&tag, 3).expect("decode");
        assert_eq!(id, 7);
        assert_eq!(s.fills, vec![Fill::Solid([10, 20, 30, 40])]);
        assert_eq!(
            s.records,
            vec![
                Record::Style { move_to: Some((3, 5)), fill0: None, fill1: Some(1), line: None, styles: None },
                Record::Line { dx: 0, dy: 3 },
                Record::Curve { cx: 1, cy: 2, ax: 3, ay: 0 },
            ]
        );
    }

    #[test]
    fn an_external_image_reads_its_names() {
        let mut def = MovieDef::default();
        let mut frame = Vec::new();
        let mut tag = vec![1, 0, 13, 0, 0, 2, 0, 2];
        tag.push(3);
        tag.extend(b"abc");
        tag.push(7);
        tag.extend(b"abc.tga");
        def.tag(1001, &tag, &mut frame).expect("decode");
        assert_eq!(
            def.characters[&1],
            Character::Image(ExternalImage {
                format: 13,
                width: 512,
                height: 512,
                export_name: "abc".into(),
                file_name: "abc.tga".into(),
            })
        );
    }

    #[test]
    fn an_unknown_tag_is_an_error() {
        let mut def = MovieDef::default();
        assert!(def.tag(500, &[], &mut Vec::new()).is_err());
    }

    #[test]
    fn labels_name_the_frame_they_sit_in() {
        let mut line = Timeline::default();
        let mut frame = Vec::new();
        timeline_tag(1, &[], &mut line, &mut frame).expect("show frame");
        timeline_tag(43, b"here\0", &mut line, &mut frame).expect("label");
        assert_eq!(line.labels["here"], 1);
    }
}
