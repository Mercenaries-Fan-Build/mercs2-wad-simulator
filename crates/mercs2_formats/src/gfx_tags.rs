//! The tags a movie's layout is made of, read into their stored values and written back to the same
//! bytes: `PlaceObject2` (26), `PlaceObject3` (70), `RemoveObject2` (28), `ShowFrame` (1),
//! `FrameLabel` (43), `DefineEditText` (37), `ExportAssets` (56) and `ImportAssets2` (71).
//!
//! The values are the ones written, not the ones a player uses: a `MATRIX` keeps its field widths and
//! its 16.16 integers, a flag byte's bits that no field stands for are kept, and strings are their
//! bytes. A flag that says a field is present is worked out from the field when written, so adding a
//! field sets its flag. Reading fails when a body is not read to its last byte, when padding bits are
//! set, or when a bit no field models is set; writing fails when a value does not fit its width.
//! [`crate::gfx_def`] turns these into what the player draws. Checked over every instance in every
//! movie of the archive and of the default stack by the workshop's `every_movie_writes_back_as_it_was_read`.

use crate::gfx::Cursor;

pub const SHOW_FRAME: u16 = 1;
pub const PLACE_OBJECT2: u16 = 26;
pub const REMOVE_OBJECT2: u16 = 28;
pub const DEFINE_EDIT_TEXT: u16 = 37;
pub const FRAME_LABEL: u16 = 43;
pub const EXPORT_ASSETS: u16 = 56;
pub const PLACE_OBJECT3: u16 = 70;
pub const IMPORT_ASSETS2: u16 = 71;

/// The codes [`decode`] reads.
pub const TYPED: [u16; 8] = [SHOW_FRAME, PLACE_OBJECT2, REMOVE_OBJECT2, DEFINE_EDIT_TEXT, FRAME_LABEL, EXPORT_ASSETS, PLACE_OBJECT3, IMPORT_ASSETS2];

/// A signed bit-field value and the width it is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bits {
    pub nbits: u8,
    pub values: [i32; 2],
}

/// `MATRIX`: scale and rotate-skew in 16.16, translation in twips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixBits {
    /// `ScaleX`, `ScaleY`.
    pub scale: Option<Bits>,
    /// `RotateSkew0` (the `b` term), `RotateSkew1` (the `c` term).
    pub rotate: Option<Bits>,
    pub translate: Bits,
}

/// `CXFORMWITHALPHA`: multiply terms in 8.8, add terms in units, red, green, blue, alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CxformBits {
    pub nbits: u8,
    pub mul: Option<[i32; 4]>,
    pub add: Option<[i32; 4]>,
}

/// `RECT` in twips, `[xmin, xmax, ymin, ymax]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RectBits {
    pub nbits: u8,
    pub values: [i32; 4],
}

/// `CLIPACTIONS` of a placed sprite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipActions {
    /// The reserved `u16` before the event mask.
    pub reserved: u16,
    /// `AllEventFlags`.
    pub all_events: u32,
    pub records: Vec<ClipRecord>,
}

/// One `CLIPACTIONRECORD`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipRecord {
    pub events: u32,
    /// The key of a `ClipEventKeyPress` (`0x0002_0000`) handler.
    pub key: Option<u8>,
    pub code: Vec<u8>,
}

/// `ClipEventKeyPress`, whose record carries a key code.
const KEY_PRESS: u32 = 0x0002_0000;

/// `PlaceObject2` or `PlaceObject3`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PlaceTag {
    /// `PlaceObject3`.
    pub three: bool,
    /// `PlaceFlagMove`.
    pub is_move: bool,
    pub depth: u16,
    pub character: Option<u16>,
    pub matrix: Option<MatrixBits>,
    pub cxform: Option<CxformBits>,
    pub ratio: Option<u16>,
    pub name: Option<Vec<u8>>,
    pub clip_depth: Option<u16>,
    /// `PlaceObject3`'s `FILTERLIST` as written, its count byte first.
    pub filters: Option<Vec<u8>>,
    pub blend_mode: Option<u8>,
    /// `PlaceObject3`'s `BitmapCache` byte.
    pub cache_as_bitmap: Option<u8>,
    pub clip_actions: Option<ClipActions>,
}

/// `DefineEditText`'s paragraph layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextLayout {
    /// 0 left, 1 right, 2 center, 3 justify.
    pub align: u8,
    pub left: u16,
    pub right: u16,
    pub indent: u16,
    pub leading: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTextTag {
    pub id: u16,
    pub bounds: RectBits,
    pub word_wrap: bool,
    pub multiline: bool,
    pub password: bool,
    pub read_only: bool,
    pub auto_size: bool,
    pub no_select: bool,
    pub border: bool,
    pub was_static: bool,
    pub html: bool,
    pub use_outlines: bool,
    pub font: Option<u16>,
    pub font_class: Option<Vec<u8>>,
    /// Present exactly when a font or a font class is.
    pub height: Option<u16>,
    pub color: Option<[u8; 4]>,
    pub max_length: Option<u16>,
    pub layout: Option<TextLayout>,
    pub variable: Vec<u8>,
    pub text: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameLabelTag {
    pub name: Vec<u8>,
    /// Followed by the `NamedAnchor` byte, 1.
    pub anchor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportTag {
    pub url: Vec<u8>,
    /// The two reserved bytes after the url.
    pub reserved: [u8; 2],
    /// `(character id, name)`.
    pub assets: Vec<(u16, Vec<u8>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Typed {
    ShowFrame,
    Place(PlaceTag),
    Remove { depth: u16 },
    EditText(EditTextTag),
    Label(FrameLabelTag),
    /// `(character id, name)`.
    Export(Vec<(u16, Vec<u8>)>),
    Import(ImportTag),
}

/// Read `body` as tag `code`; `None` for a code not in [`TYPED`].
pub fn decode(code: u16, body: &[u8]) -> Result<Option<Typed>, String> {
    let mut cur = Cursor::new(body, 0);
    let typed = match code {
        SHOW_FRAME => Typed::ShowFrame,
        PLACE_OBJECT2 | PLACE_OBJECT3 => Typed::Place(place(&mut cur, code == PLACE_OBJECT3)?),
        REMOVE_OBJECT2 => Typed::Remove { depth: cur.u16()? },
        DEFINE_EDIT_TEXT => Typed::EditText(edit_text(&mut cur)?),
        FRAME_LABEL => {
            let name = cbytes(&mut cur)?;
            let anchor = match body.len() - cur.pos {
                0 => false,
                1 if cur.u8()? == 1 => true,
                _ => return Err(format!("{} bytes after the label, not a NamedAnchor byte of 1", body.len() - name.len() - 1)),
            };
            Typed::Label(FrameLabelTag { name, anchor })
        }
        EXPORT_ASSETS => Typed::Export(assets(&mut cur)?),
        IMPORT_ASSETS2 => {
            let url = cbytes(&mut cur)?;
            let reserved = [cur.u8()?, cur.u8()?];
            Typed::Import(ImportTag { url, reserved, assets: assets(&mut cur)? })
        }
        _ => return Ok(None),
    };
    if cur.pos != body.len() {
        return Err(format!("tag {code}: read {} of its {} bytes", cur.pos, body.len()));
    }
    Ok(Some(typed))
}

impl Typed {
    /// The tag code this is written as.
    pub fn code(&self) -> u16 {
        match self {
            Typed::ShowFrame => SHOW_FRAME,
            Typed::Place(p) if p.three => PLACE_OBJECT3,
            Typed::Place(_) => PLACE_OBJECT2,
            Typed::Remove { .. } => REMOVE_OBJECT2,
            Typed::EditText(_) => DEFINE_EDIT_TEXT,
            Typed::Label(_) => FRAME_LABEL,
            Typed::Export(_) => EXPORT_ASSETS,
            Typed::Import(_) => IMPORT_ASSETS2,
        }
    }

    /// The tag's body.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut w = BitWriter::default();
        match self {
            Typed::ShowFrame => {}
            Typed::Place(p) => write_place(&mut w, p)?,
            Typed::Remove { depth } => w.u16(*depth),
            Typed::EditText(t) => write_edit_text(&mut w, t)?,
            Typed::Label(l) => {
                w.cstring(&l.name)?;
                if l.anchor {
                    w.u8(1);
                }
            }
            Typed::Export(list) => write_assets(&mut w, list)?,
            Typed::Import(i) => {
                w.cstring(&i.url)?;
                w.u8(i.reserved[0]);
                w.u8(i.reserved[1]);
                write_assets(&mut w, &i.assets)?;
            }
        }
        Ok(w.out)
    }
}

/// A string's bytes up to its terminating zero.
fn cbytes(cur: &mut Cursor) -> Result<Vec<u8>, String> {
    let rest = cur.data.get(cur.pos..).ok_or("a string past the body")?;
    let len = rest.iter().position(|b| *b == 0).ok_or("a string with no terminating zero")?;
    let out = rest[..len].to_vec();
    cur.pos += len + 1;
    Ok(out)
}

fn u32le(cur: &mut Cursor) -> Result<u32, String> {
    let lo = cur.u16()? as u32;
    Ok(lo | (cur.u16()? as u32) << 16)
}

fn sbits(cur: &mut Cursor, n: u8) -> Result<i32, String> {
    if n == 0 {
        return Ok(0);
    }
    let v = cur.ubits(n as u32)?;
    Ok(if n < 32 && v & (1 << (n - 1)) != 0 { (v as i32) - (1i32 << n) } else { v as i32 })
}

fn bits(cur: &mut Cursor, width: u32) -> Result<Bits, String> {
    let nbits = cur.ubits(width)? as u8;
    Ok(Bits { nbits, values: [sbits(cur, nbits)?, sbits(cur, nbits)?] })
}

/// A `MATRIX` from an aligned cursor, which it leaves aligned.
pub(crate) fn matrix(cur: &mut Cursor) -> Result<MatrixBits, String> {
    let scale = if cur.ubits(1)? == 1 { Some(bits(cur, 5)?) } else { None };
    let rotate = if cur.ubits(1)? == 1 { Some(bits(cur, 5)?) } else { None };
    let translate = bits(cur, 5)?;
    cur.align_zero()?;
    Ok(MatrixBits { scale, rotate, translate })
}

fn cxform(cur: &mut Cursor) -> Result<CxformBits, String> {
    let has_add = cur.ubits(1)? == 1;
    let has_mul = cur.ubits(1)? == 1;
    let nbits = cur.ubits(4)? as u8;
    let mut four = || -> Result<[i32; 4], String> { Ok([sbits(cur, nbits)?, sbits(cur, nbits)?, sbits(cur, nbits)?, sbits(cur, nbits)?]) };
    let mul = if has_mul { Some(four()?) } else { None };
    let add = if has_add { Some(four()?) } else { None };
    cur.align_zero()?;
    Ok(CxformBits { nbits, mul, add })
}

fn rect(cur: &mut Cursor) -> Result<RectBits, String> {
    let nbits = cur.ubits(5)? as u8;
    let values = [sbits(cur, nbits)?, sbits(cur, nbits)?, sbits(cur, nbits)?, sbits(cur, nbits)?];
    cur.align_zero()?;
    Ok(RectBits { nbits, values })
}

/// A `FILTERLIST` as written: its count, then each filter's id and body.
fn filter_list(cur: &mut Cursor) -> Result<Vec<u8>, String> {
    let start = cur.pos;
    for _ in 0..cur.u8()? {
        let size = match cur.u8()? {
            0 => 23,
            1 => 9,
            2 => 15,
            3 => 27,
            6 => 80,
            other => return Err(format!("filter kind {other}, whose length is not modelled")),
        };
        cur.pos += size;
    }
    cur.data.get(start..cur.pos).map(<[u8]>::to_vec).ok_or_else(|| "the filter list runs past the body".to_string())
}

fn clip_actions(cur: &mut Cursor) -> Result<ClipActions, String> {
    let reserved = cur.u16()?;
    let all_events = u32le(cur)?;
    let mut records = Vec::new();
    loop {
        let events = u32le(cur)?;
        if events == 0 {
            return Ok(ClipActions { reserved, all_events, records });
        }
        let mut size = u32le(cur)? as usize;
        let key = if events & KEY_PRESS != 0 {
            size = size.checked_sub(1).ok_or("a key-press clip action with no room for its key")?;
            Some(cur.u8()?)
        } else {
            None
        };
        let code = cur.data.get(cur.pos..cur.pos + size).ok_or("a clip action runs past the body")?.to_vec();
        cur.pos += size;
        records.push(ClipRecord { events, key, code });
    }
}

/// `PlaceObject3` flag bits no field here stands for: `HasClassName` (`0x08`), `HasImage` (`0x10`)
/// and the reserved top three.
const PLACE3_UNMODELLED: u8 = 0xF8;

fn place(cur: &mut Cursor, three: bool) -> Result<PlaceTag, String> {
    let flags = cur.u8()?;
    let flags3 = if three { cur.u8()? } else { 0 };
    if flags3 & PLACE3_UNMODELLED != 0 {
        return Err(format!("PlaceObject3 flags 0x{flags3:02x}: a class name, an image or a reserved bit"));
    }
    let mut p = PlaceTag { three, is_move: flags & 0x01 != 0, depth: cur.u16()?, ..PlaceTag::default() };
    if flags & 0x02 != 0 {
        p.character = Some(cur.u16()?);
    }
    if flags & 0x04 != 0 {
        p.matrix = Some(matrix(cur)?);
    }
    if flags & 0x08 != 0 {
        p.cxform = Some(cxform(cur)?);
    }
    if flags & 0x10 != 0 {
        p.ratio = Some(cur.u16()?);
    }
    if flags & 0x20 != 0 {
        p.name = Some(cbytes(cur)?);
    }
    if flags & 0x40 != 0 {
        p.clip_depth = Some(cur.u16()?);
    }
    if flags3 & 0x01 != 0 {
        p.filters = Some(filter_list(cur)?);
    }
    if flags3 & 0x02 != 0 {
        p.blend_mode = Some(cur.u8()?);
    }
    if flags3 & 0x04 != 0 {
        p.cache_as_bitmap = Some(cur.u8()?);
    }
    if flags & 0x80 != 0 {
        p.clip_actions = Some(clip_actions(cur)?);
    }
    Ok(p)
}

fn edit_text(cur: &mut Cursor) -> Result<EditTextTag, String> {
    let id = cur.u16()?;
    let bounds = rect(cur)?;
    let (a, b) = (cur.u8()?, cur.u8()?);
    let font = if a & 0x01 != 0 { Some(cur.u16()?) } else { None };
    let font_class = if b & 0x80 != 0 { Some(cbytes(cur)?) } else { None };
    let height = if font.is_some() || font_class.is_some() { Some(cur.u16()?) } else { None };
    let color = if a & 0x04 != 0 { Some([cur.u8()?, cur.u8()?, cur.u8()?, cur.u8()?]) } else { None };
    let max_length = if a & 0x02 != 0 { Some(cur.u16()?) } else { None };
    let layout = if b & 0x20 != 0 {
        Some(TextLayout { align: cur.u8()?, left: cur.u16()?, right: cur.u16()?, indent: cur.u16()?, leading: cur.u16()? as i16 })
    } else {
        None
    };
    let variable = cbytes(cur)?;
    let text = if a & 0x80 != 0 { Some(cbytes(cur)?) } else { None };
    Ok(EditTextTag {
        id,
        bounds,
        word_wrap: a & 0x40 != 0,
        multiline: a & 0x20 != 0,
        password: a & 0x10 != 0,
        read_only: a & 0x08 != 0,
        auto_size: b & 0x40 != 0,
        no_select: b & 0x10 != 0,
        border: b & 0x08 != 0,
        was_static: b & 0x04 != 0,
        html: b & 0x02 != 0,
        use_outlines: b & 0x01 != 0,
        font,
        font_class,
        height,
        color,
        max_length,
        layout,
        variable,
        text,
    })
}

fn assets(cur: &mut Cursor) -> Result<Vec<(u16, Vec<u8>)>, String> {
    let count = cur.u16()?;
    (0..count).map(|_| Ok((cur.u16()?, cbytes(cur)?))).collect()
}

/// Bits written most significant first, as SWF packs them.
#[derive(Default)]
struct BitWriter {
    out: Vec<u8>,
    /// Bits used in the last byte; 0 when aligned.
    bit: u8,
}

impl BitWriter {
    fn ubits(&mut self, n: u8, v: u32) {
        for i in (0..n as u32).rev() {
            if self.bit == 0 {
                self.out.push(0);
            }
            let last = self.out.last_mut().expect("a byte was pushed");
            *last |= (((v >> i) & 1) as u8) << (7 - self.bit);
            self.bit = (self.bit + 1) % 8;
        }
    }

    fn sbits(&mut self, n: u8, v: i32) -> Result<(), String> {
        let fits = match n {
            0 => v == 0,
            32.. => true,
            n => (-(1i64 << (n - 1))..(1i64 << (n - 1))).contains(&(v as i64)),
        };
        if !fits {
            return Err(format!("{v} does not fit {n} signed bits"));
        }
        self.ubits(n, v as u32);
        Ok(())
    }

    fn width(&mut self, field: u8, nbits: u8) -> Result<(), String> {
        if nbits as u32 >= 1 << field {
            return Err(format!("a width of {nbits} does not fit its {field}-bit field"));
        }
        self.ubits(field, nbits as u32);
        Ok(())
    }

    fn align(&mut self) {
        self.bit = 0;
    }

    fn u8(&mut self, v: u8) {
        self.align();
        self.out.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.align();
        self.out.extend(v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.align();
        self.out.extend(v.to_le_bytes());
    }

    fn cstring(&mut self, s: &[u8]) -> Result<(), String> {
        if s.contains(&0) {
            return Err(format!("{:?} holds a zero byte, which would end it early", String::from_utf8_lossy(s)));
        }
        self.align();
        self.out.extend(s);
        self.out.push(0);
        Ok(())
    }

    fn bits(&mut self, field: u8, b: &Bits) -> Result<(), String> {
        self.width(field, b.nbits)?;
        for v in b.values {
            self.sbits(b.nbits, v)?;
        }
        Ok(())
    }
}

fn write_matrix(w: &mut BitWriter, m: &MatrixBits) -> Result<(), String> {
    w.align();
    for part in [&m.scale, &m.rotate] {
        w.ubits(1, part.is_some() as u32);
        if let Some(b) = part {
            w.bits(5, b)?;
        }
    }
    w.bits(5, &m.translate)?;
    w.align();
    Ok(())
}

fn write_cxform(w: &mut BitWriter, c: &CxformBits) -> Result<(), String> {
    w.align();
    w.ubits(1, c.add.is_some() as u32);
    w.ubits(1, c.mul.is_some() as u32);
    w.width(4, c.nbits)?;
    for terms in [&c.mul, &c.add].into_iter().flatten() {
        for v in terms {
            w.sbits(c.nbits, *v)?;
        }
    }
    w.align();
    Ok(())
}

fn write_rect(w: &mut BitWriter, r: &RectBits) -> Result<(), String> {
    w.align();
    w.width(5, r.nbits)?;
    for v in r.values {
        w.sbits(r.nbits, v)?;
    }
    w.align();
    Ok(())
}

fn write_place(w: &mut BitWriter, p: &PlaceTag) -> Result<(), String> {
    if !p.three && (p.filters.is_some() || p.blend_mode.is_some() || p.cache_as_bitmap.is_some()) {
        return Err(format!("depth {}: filters, a blend mode or a bitmap cache need PlaceObject3", p.depth));
    }
    let flag = |on: bool, bit: u8| if on { bit } else { 0 };
    w.u8(flag(p.clip_actions.is_some(), 0x80)
        | flag(p.clip_depth.is_some(), 0x40)
        | flag(p.name.is_some(), 0x20)
        | flag(p.ratio.is_some(), 0x10)
        | flag(p.cxform.is_some(), 0x08)
        | flag(p.matrix.is_some(), 0x04)
        | flag(p.character.is_some(), 0x02)
        | flag(p.is_move, 0x01));
    if p.three {
        w.u8(flag(p.cache_as_bitmap.is_some(), 0x04) | flag(p.blend_mode.is_some(), 0x02) | flag(p.filters.is_some(), 0x01));
    }
    w.u16(p.depth);
    if let Some(c) = p.character {
        w.u16(c);
    }
    if let Some(m) = &p.matrix {
        write_matrix(w, m)?;
    }
    if let Some(c) = &p.cxform {
        write_cxform(w, c)?;
    }
    if let Some(r) = p.ratio {
        w.u16(r);
    }
    if let Some(n) = &p.name {
        w.cstring(n)?;
    }
    if let Some(d) = p.clip_depth {
        w.u16(d);
    }
    if let Some(f) = &p.filters {
        w.align();
        w.out.extend(f);
    }
    if let Some(b) = p.blend_mode {
        w.u8(b);
    }
    if let Some(c) = p.cache_as_bitmap {
        w.u8(c);
    }
    if let Some(a) = &p.clip_actions {
        w.u16(a.reserved);
        w.u32(a.all_events);
        for r in &a.records {
            if r.key.is_some() != (r.events & KEY_PRESS != 0) {
                return Err(format!("depth {}: a clip action's key must be there exactly when it handles a key press", p.depth));
            }
            w.u32(r.events);
            w.u32((r.code.len() + r.key.is_some() as usize) as u32);
            if let Some(k) = r.key {
                w.u8(k);
            }
            w.out.extend(&r.code);
        }
        w.u32(0);
    }
    Ok(())
}

fn write_edit_text(w: &mut BitWriter, t: &EditTextTag) -> Result<(), String> {
    if t.height.is_some() != (t.font.is_some() || t.font_class.is_some()) {
        return Err(format!("edit text {}: a height goes with a font or a font class, and only then", t.id));
    }
    let flag = |on: bool, bit: u8| if on { bit } else { 0 };
    w.u16(t.id);
    write_rect(w, &t.bounds)?;
    w.u8(flag(t.text.is_some(), 0x80)
        | flag(t.word_wrap, 0x40)
        | flag(t.multiline, 0x20)
        | flag(t.password, 0x10)
        | flag(t.read_only, 0x08)
        | flag(t.color.is_some(), 0x04)
        | flag(t.max_length.is_some(), 0x02)
        | flag(t.font.is_some(), 0x01));
    w.u8(flag(t.font_class.is_some(), 0x80)
        | flag(t.auto_size, 0x40)
        | flag(t.layout.is_some(), 0x20)
        | flag(t.no_select, 0x10)
        | flag(t.border, 0x08)
        | flag(t.was_static, 0x04)
        | flag(t.html, 0x02)
        | flag(t.use_outlines, 0x01));
    if let Some(f) = t.font {
        w.u16(f);
    }
    if let Some(c) = &t.font_class {
        w.cstring(c)?;
    }
    if let Some(h) = t.height {
        w.u16(h);
    }
    if let Some(c) = t.color {
        for v in c {
            w.u8(v);
        }
    }
    if let Some(m) = t.max_length {
        w.u16(m);
    }
    if let Some(l) = t.layout {
        w.u8(l.align);
        w.u16(l.left);
        w.u16(l.right);
        w.u16(l.indent);
        w.u16(l.leading as u16);
    }
    w.cstring(&t.variable)?;
    if let Some(s) = &t.text {
        w.cstring(s)?;
    }
    Ok(())
}

fn write_assets(w: &mut BitWriter, list: &[(u16, Vec<u8>)]) -> Result<(), String> {
    let count = u16::try_from(list.len()).map_err(|_| format!("{} assets do not fit a u16 count", list.len()))?;
    w.u16(count);
    for (id, name) in list {
        w.u16(*id);
        w.cstring(name)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn again(code: u16, body: &[u8]) -> Typed {
        let typed = decode(code, body).expect("decode").expect("a typed code");
        assert_eq!(typed.encode().expect("encode"), body, "tag {code} writes back as read");
        typed
    }

    /// A move with a matrix whose scale is written 18 bits wide, though 17 would hold it, and a
    /// colour transform with only add terms.
    #[test]
    fn a_placement_keeps_its_field_widths() {
        let mut w = BitWriter::default();
        let m = MatrixBits {
            scale: Some(Bits { nbits: 18, values: [65536, -65536] }),
            rotate: None,
            translate: Bits { nbits: 12, values: [-200, 1000] },
        };
        let c = CxformBits { nbits: 9, mul: None, add: Some([-255, 0, 12, 255]) };
        let p = PlaceTag { is_move: true, depth: 7, matrix: Some(m), cxform: Some(c), name: Some(b"button6_mc".to_vec()), ..PlaceTag::default() };
        write_place(&mut w, &p).expect("write");
        let Typed::Place(read) = again(PLACE_OBJECT2, &w.out) else { panic!("a placement") };
        assert_eq!(read, p);
    }

    #[test]
    fn a_value_wider_than_its_field_is_refused() {
        let p = PlaceTag {
            depth: 1,
            matrix: Some(MatrixBits { scale: None, rotate: None, translate: Bits { nbits: 4, values: [8, 0] } }),
            ..PlaceTag::default()
        };
        assert!(Typed::Place(p).encode().is_err(), "8 needs 5 signed bits");
    }

    #[test]
    fn set_padding_bits_are_refused() {
        // Has-scale 0, has-rotate 0, translate width 0: seven bits, then a set padding bit.
        assert!(decode(PLACE_OBJECT2, &[0x04, 1, 0, 0x01]).is_err());
        again(PLACE_OBJECT2, &[0x04, 1, 0, 0x00]);
    }

    #[test]
    fn a_label_an_export_and_an_import_write_back() {
        let Typed::Label(l) = again(FRAME_LABEL, b"_over\0\x01") else { panic!("a label") };
        assert!(l.anchor);
        again(FRAME_LABEL, b"_off\0");
        assert!(decode(FRAME_LABEL, b"_off\0\x02").is_err(), "an anchor byte is 1");
        again(EXPORT_ASSETS, b"\x02\x00\x05\x00flag\0\x09\x00b\0");
        again(IMPORT_ASSETS2, b"gfxfontlib.swf\0\x01\x00\x01\x00\x03\x00$Font\0");
        assert!(decode(REMOVE_OBJECT2, &[1, 0, 0]).is_err(), "a body read short of its end");
    }

    #[test]
    fn an_edit_text_keeps_every_flag() {
        let t = EditTextTag {
            id: 3,
            bounds: RectBits { nbits: 15, values: [-40, 4000, -40, 400] },
            word_wrap: true,
            multiline: false,
            password: false,
            read_only: true,
            auto_size: false,
            no_select: true,
            border: false,
            was_static: true,
            html: true,
            use_outlines: true,
            font: Some(2),
            font_class: None,
            height: Some(240),
            color: Some([255, 255, 255, 255]),
            max_length: None,
            layout: Some(TextLayout { align: 2, left: 0, right: 0, indent: 0, leading: -40 }),
            variable: b"_root.name".to_vec(),
            text: Some(b"[0x1234ABCD]".to_vec()),
        };
        let body = Typed::EditText(t.clone()).encode().expect("encode");
        assert_eq!(again(DEFINE_EDIT_TEXT, &body), Typed::EditText(t));
    }
}
