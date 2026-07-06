//! Scaleform **GFx** (`.gfx`) / SWF movie parser — enough to inventory a movie's tag stream and
//! detect the authorable features (shape fills incl. **gradients** and **bitmaps**, embedded vs
//! imported fonts, embedded bitmaps, sprites, buttons, edit-text, morph/video, AS3, GFx-extension
//! tags).
//!
//! This backs the `gfx_golden` tool, which turns our inventory of extracted retail movies
//! (`output/gfx_movies/`) into a **golden reference set**: for every SWF/GFx feature, the report
//! names the real shipping movie that demonstrates it, so a community authoring tool (e.g.
//! `gfxforge`) can byte-validate a new emitter against a movie the game actually loads.
//!
//! A `.gfx` is a SWF tag stream with a GFx header: magic `GFX`/`CFX` (`C` = zlib-compressed, the
//! form retail ships) — the SWF cousins `FWS`/`CWS` are also accepted. After the 8-byte header
//! (magic[3] + `u8 version` + `u32 file_length`) comes the (optionally zlib-inflated) body:
//! a frame-size `RECT`, `u16` frame-rate, `u16` frame-count, then the tag records.
//!
//! Scope: the container + the top-level tag stream are parsed exactly; shape bodies are parsed only
//! as far as the **fill-style array** (all that's needed to flag gradient/bitmap fills). Anything
//! deeper (edge/curve records, timeline semantics) is out of scope and intentionally not guessed.

use std::collections::BTreeMap;
use std::io::Read;

/// A parsed `.gfx`/SWF movie: container header + the flat top-level tag list.
#[derive(Debug, Clone)]
pub struct GfxMovie {
    /// 3-byte magic as ASCII (`GFX`/`CFX`/`FWS`/`CWS`).
    pub magic: [u8; 3],
    /// Header version byte (retail GFx = `8`).
    pub version: u8,
    /// Whether the body was zlib-compressed (`CFX`/`CWS`).
    pub compressed: bool,
    /// Declared uncompressed file length (header `u32`).
    pub file_length: u32,
    /// Frame size in twips: `[x_min, x_max, y_min, y_max]` (÷20 = pixels).
    pub frame_size: [i32; 4],
    /// Frame rate (8.8 fixed → f32).
    pub frame_rate: f32,
    /// Frame count.
    pub frame_count: u16,
    /// Every tag in stream order: `(code, body_length)`.
    pub tags: Vec<(u16, usize)>,
    /// The decompressed body (header stripped) — kept so `features()` can re-read tag bodies.
    body: Vec<u8>,
    /// Byte offset in `body` of each tag's *body* (parallel to `tags`).
    tag_body_offsets: Vec<usize>,
}

/// Feature inventory of a movie — the golden-set signal. Every field is a count of the tags /
/// shapes that exercise that feature.
#[derive(Debug, Clone, Default)]
pub struct Features {
    /// Histogram of every tag code → occurrences.
    pub tag_counts: BTreeMap<u16, u32>,
    // -- shapes ------------------------------------------------------------
    pub shapes: u32,
    /// Shapes whose fill-style array contains a linear/radial gradient fill (type 0x10/0x12/0x13).
    pub shapes_with_gradient: u32,
    /// Shapes with a bitmap fill (type 0x40–0x43) — a texture-backed fill.
    pub shapes_with_bitmap: u32,
    /// Shapes using a Flash-8 focal-radial gradient (fill type 0x13).
    pub shapes_with_focal_gradient: u32,
    /// Shapes whose fill-style array could not be fully parsed (unknown fill type / truncation).
    pub shapes_parse_incomplete: u32,
    // -- fonts / text ------------------------------------------------------
    /// Embedded font glyph definitions (DefineFont / DefineFont2 / DefineFont3).
    pub embedded_fonts: u32,
    /// `ImportAssets`/`ImportAssets2` tags (the retail shared-font-lib import path).
    pub imports: u32,
    /// `ExportAssets` tags (a movie that *exports* symbols by name — e.g. the font lib).
    pub exports: u32,
    pub edit_texts: u32,
    // -- bitmaps -----------------------------------------------------------
    /// Embedded bitmap tags (DefineBits* — JPEG/lossless pixels stored *inside* the movie).
    pub embedded_bitmaps: u32,
    // -- structure / scripting --------------------------------------------
    pub sprites: u32,
    pub buttons: u32,
    /// DefineMorphShape / DefineMorphShape2 (tweened shapes).
    pub morph_shapes: u32,
    /// DefineVideoStream (video).
    pub videos: u32,
    /// `DoAction`/`DoInitAction` (AVM1 / AS2 bytecode blocks).
    pub do_action: u32,
    /// `DoABC` (AS3) — **should be zero** for GFx 2.x / AVM1; a non-zero here means the movie
    /// carries AS3, which GFx 2.0.48 does not run.
    pub do_abc: u32,
    // -- GFx extension -----------------------------------------------------
    /// GFx-extension tags (code ≥ 1000): ExporterInfo, external-image / gradient-image, font-texture
    /// info, etc. The exact codes present are reported so the golden set *discovers* which GFx tags
    /// the runtime uses, rather than assuming them.
    pub gfx_ext_tags: u32,
}

/// A byte cursor with MSB-first bit reading, for `RECT`/`MATRIX` (SWF bit-packed) fields.
struct Cursor<'a> {
    data: &'a [u8],
    /// Byte position.
    pos: usize,
    /// Bit position within the current byte (0 = MSB), 0..8; `0` means byte-aligned.
    bit: u8,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Cursor { data, pos, bit: 0 }
    }
    fn align(&mut self) {
        if self.bit != 0 {
            self.bit = 0;
            self.pos += 1;
        }
    }
    fn u8(&mut self) -> Result<u8, String> {
        self.align();
        let b = *self.data.get(self.pos).ok_or("eof:u8")?;
        self.pos += 1;
        Ok(b)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes([self.u8()?, self.u8()?]))
    }
    /// Read `n` unsigned bits (MSB-first). `n` ≤ 32.
    fn ubits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..n {
            let byte = *self.data.get(self.pos).ok_or("eof:bits")?;
            let bit = (byte >> (7 - self.bit)) & 1;
            v = (v << 1) | bit as u32;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Ok(v)
    }
    /// Skip a bit-packed `RECT` (nbits header + 4 fields), then byte-align.
    fn skip_rect(&mut self) -> Result<(), String> {
        let nbits = self.ubits(5)?;
        self.ubits(nbits * 4)?;
        self.align();
        Ok(())
    }
    /// Read a `RECT` into `[xmin,xmax,ymin,ymax]` (twips), then byte-align.
    fn read_rect(&mut self) -> Result<[i32; 4], String> {
        let nbits = self.ubits(5)?;
        let sext = |v: u32, n: u32| -> i32 {
            if n == 0 {
                0
            } else if v & (1 << (n - 1)) != 0 {
                (v as i32) - (1 << n)
            } else {
                v as i32
            }
        };
        let mut r = [0i32; 4];
        for slot in &mut r {
            *slot = sext(self.ubits(nbits)?, nbits);
        }
        self.align();
        Ok(r)
    }
    /// Skip a bit-packed `MATRIX`, then byte-align.
    fn skip_matrix(&mut self) -> Result<(), String> {
        if self.ubits(1)? == 1 {
            let n = self.ubits(5)?;
            self.ubits(n * 2)?;
        }
        if self.ubits(1)? == 1 {
            let n = self.ubits(5)?;
            self.ubits(n * 2)?;
        }
        let nt = self.ubits(5)?;
        self.ubits(nt * 2)?;
        self.align();
        Ok(())
    }
}

// -- SWF / GFx tag codes we name ------------------------------------------------------------------

/// Human name for a tag code (standard SWF + the GFx-extension range). Unknown codes get `"?"`.
pub fn tag_name(code: u16) -> &'static str {
    match code {
        0 => "End",
        1 => "ShowFrame",
        2 => "DefineShape",
        4 => "PlaceObject",
        5 => "RemoveObject",
        6 => "DefineBits",
        7 => "DefineButton",
        8 => "JPEGTables",
        9 => "SetBackgroundColor",
        10 => "DefineFont",
        11 => "DefineText",
        12 => "DoAction",
        13 => "DefineFontInfo",
        20 => "DefineBitsLossless",
        21 => "DefineBitsJPEG2",
        22 => "DefineShape2",
        24 => "Protect",
        25 => "PathsArePostscript",
        26 => "PlaceObject2",
        28 => "RemoveObject2",
        32 => "DefineShape3",
        33 => "DefineText2",
        34 => "DefineButton2",
        35 => "DefineBitsJPEG3",
        36 => "DefineBitsLossless2",
        37 => "DefineEditText",
        39 => "DefineSprite",
        43 => "FrameLabel",
        46 => "DefineMorphShape",
        48 => "DefineFont2",
        56 => "ExportAssets",
        57 => "ImportAssets",
        59 => "DoInitAction",
        60 => "DefineVideoStream",
        61 => "VideoFrame",
        62 => "DefineFontInfo2",
        69 => "FileAttributes",
        70 => "PlaceObject3",
        71 => "ImportAssets2",
        73 => "DefineFontAlignZones",
        74 => "CSMTextSettings",
        75 => "DefineFont3",
        76 => "SymbolClass",
        82 => "DoABC",
        83 => "DefineShape4",
        84 => "DefineMorphShape2",
        86 => "DefineSceneAndFrameLabelData",
        88 => "DefineFontName",
        // GFx extension range (Scaleform-specific). Exact roles differ by SDK; the golden report
        // surfaces which codes actually appear rather than over-claiming.
        1000 => "GFx_ExporterInfo",
        1001 => "GFx_DefineExternalImage",
        1002 => "GFx_FontTextureInfo",
        1003 => "GFx_DefineExternalGradientImage",
        1004 => "GFx_DefineSubImage",
        1005 => "GFx_DefineExternalSound",
        1006 => "GFx_DefineExternalStreamSound",
        _ => "?",
    }
}

impl GfxMovie {
    /// Parse the container + top-level tag stream. Zlib-inflates a `CFX`/`CWS` body.
    pub fn parse(data: &[u8]) -> Result<GfxMovie, String> {
        if data.len() < 8 {
            return Err("too small for header".into());
        }
        let magic = [data[0], data[1], data[2]];
        let compressed = matches!(&magic, b"CFX" | b"CWS");
        let is_gfx = matches!(&magic, b"GFX" | b"CFX");
        if !is_gfx && !matches!(&magic, b"FWS" | b"CWS") {
            return Err(format!(
                "bad magic {:?} (want GFX/CFX/FWS/CWS)",
                String::from_utf8_lossy(&magic)
            ));
        }
        let version = data[3];
        let file_length = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        let body: Vec<u8> = if compressed {
            let mut out = Vec::with_capacity(file_length.saturating_sub(8) as usize);
            flate2::read::ZlibDecoder::new(&data[8..])
                .read_to_end(&mut out)
                .map_err(|e| format!("zlib inflate: {e}"))?;
            out
        } else {
            data[8..].to_vec()
        };

        let mut cur = Cursor::new(&body, 0);
        let frame_size = cur.read_rect()?;
        let frame_rate = cur.u16()? as f32 / 256.0;
        let frame_count = cur.u16()?;

        // Tag stream: u16 {code<<6 | len}; len==0x3F → u32 long length.
        let mut tags = Vec::new();
        let mut tag_body_offsets = Vec::new();
        let mut off = cur.pos;
        while off + 2 <= body.len() {
            let rh = u16::from_le_bytes([body[off], body[off + 1]]) as usize;
            off += 2;
            let code = (rh >> 6) as u16;
            let mut len = rh & 0x3F;
            if len == 0x3F {
                if off + 4 > body.len() {
                    return Err("truncated long-tag length".into());
                }
                len = u32::from_le_bytes([body[off], body[off + 1], body[off + 2], body[off + 3]])
                    as usize;
                off += 4;
            }
            if off + len > body.len() {
                return Err(format!("tag {code} body overruns ({} > {})", off + len, body.len()));
            }
            tags.push((code, len));
            tag_body_offsets.push(off);
            off += len;
            if code == 0 {
                break; // End
            }
        }

        Ok(GfxMovie {
            magic,
            version,
            compressed,
            file_length,
            frame_size,
            frame_rate,
            frame_count,
            tags,
            body,
            tag_body_offsets,
        })
    }

    /// Frame size in pixels `[w, h]` (twips ÷ 20).
    pub fn stage_px(&self) -> [f32; 2] {
        [
            (self.frame_size[1] - self.frame_size[0]) as f32 / 20.0,
            (self.frame_size[3] - self.frame_size[2]) as f32 / 20.0,
        ]
    }

    /// Compute the feature inventory (deeper per-tag parse for shape fills).
    pub fn features(&self) -> Features {
        let mut f = Features::default();
        for (i, &(code, len)) in self.tags.iter().enumerate() {
            *f.tag_counts.entry(code).or_insert(0) += 1;
            let body_off = self.tag_body_offsets[i];
            let tag = &self.body[body_off..body_off + len];
            match code {
                2 | 22 | 32 | 83 => {
                    f.shapes += 1;
                    let has_alpha = code == 32 || code == 83; // DefineShape3/4 = RGBA
                    let extended_count = code != 2; // DefineShape2+ allow 0xFF-extended counts
                    match scan_shape_fills(tag, has_alpha, extended_count) {
                        Ok(fills) => {
                            if fills.gradient {
                                f.shapes_with_gradient += 1;
                            }
                            if fills.bitmap {
                                f.shapes_with_bitmap += 1;
                            }
                            if fills.focal {
                                f.shapes_with_focal_gradient += 1;
                            }
                        }
                        Err(_) => f.shapes_parse_incomplete += 1,
                    }
                }
                10 | 48 | 75 => f.embedded_fonts += 1,
                57 | 71 => f.imports += 1,
                56 => f.exports += 1,
                37 => f.edit_texts += 1,
                6 | 20 | 21 | 35 | 36 => f.embedded_bitmaps += 1,
                39 => f.sprites += 1,
                7 | 34 => f.buttons += 1,
                46 | 84 => f.morph_shapes += 1,
                60 => f.videos += 1,
                12 | 59 => f.do_action += 1,
                82 => f.do_abc += 1,
                c if c >= 1000 => f.gfx_ext_tags += 1,
                _ => {}
            }
        }
        f
    }
}

/// Result of scanning a shape's fill-style array.
struct ShapeFills {
    gradient: bool,
    bitmap: bool,
    focal: bool,
}

/// Parse a `DefineShape*` body only as far as the fill-style array, flagging gradient/bitmap fills.
/// Layout: `u16 shape_id`, bounds `RECT`, then `FILLSTYLEARRAY` (count `u8`, or `0xFF`→`u16` for
/// DefineShape2+). Each `FILLSTYLE`: type `u8`; solid = RGB(A); gradient (0x10/0x12/0x13) = `MATRIX`
/// + gradient records; bitmap (0x40–0x43) = `u16 id` + `MATRIX`.
fn scan_shape_fills(tag: &[u8], has_alpha: bool, extended_count: bool) -> Result<ShapeFills, String> {
    let color_len = if has_alpha { 4 } else { 3 };
    let mut cur = Cursor::new(tag, 0);
    let _shape_id = cur.u16()?;
    cur.skip_rect()?;
    // Fill-style count.
    let mut count = cur.u8()? as usize;
    if extended_count && count == 0xFF {
        count = cur.u16()? as usize;
    }
    let mut out = ShapeFills { gradient: false, bitmap: false, focal: false };
    for _ in 0..count {
        let ty = cur.u8()?;
        match ty {
            0x00 => {
                // Solid: skip the color.
                for _ in 0..color_len {
                    cur.u8()?;
                }
            }
            0x10 | 0x12 | 0x13 => {
                out.gradient = true;
                if ty == 0x13 {
                    out.focal = true;
                }
                cur.skip_matrix()?;
                let nrec = (cur.u8()? & 0x0F) as usize; // NumGradients (low nibble)
                for _ in 0..nrec {
                    cur.u8()?; // ratio
                    for _ in 0..color_len {
                        cur.u8()?;
                    }
                }
                if ty == 0x13 {
                    cur.u16()?; // focal point (FIXED8)
                }
            }
            0x40 | 0x41 | 0x42 | 0x43 => {
                out.bitmap = true;
                cur.u16()?; // bitmap id
                cur.skip_matrix()?;
            }
            other => return Err(format!("unknown fill type 0x{other:02x}")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal uncompressed GFX with one solid + one linear-gradient fill DefineShape3, to prove
    /// the container walk + fill-style detection (no external data needed).
    #[test]
    fn detects_gradient_and_solid() {
        // --- build a DefineShape3 body: id + tiny rect + 2 fill styles (solid, linear gradient) ---
        let mut shape = Vec::new();
        shape.extend_from_slice(&1u16.to_le_bytes()); // shape id
        shape.push(0b00000_000); // RECT: nbits=0 → 0-length bounds, byte-aligned
        shape.push(2); // fill style count = 2
        // fill 1: solid RGBA
        shape.push(0x00);
        shape.extend_from_slice(&[10, 20, 30, 255]);
        // fill 2: linear gradient (0x10): identity MATRIX (0x00 = no scale/rotate, nt=0) + 2 records
        shape.push(0x10);
        shape.push(0x00); // MATRIX: hasScale=0, hasRotate=0, ntbits=0 → 1 byte, byte-aligned
        shape.push(0x02); // NumGradients=2
        shape.extend_from_slice(&[0, 0, 0, 0, 255]); // record 0: ratio + RGBA
        shape.extend_from_slice(&[255, 255, 255, 255, 255]); // record 1

        // --- wrap the DefineShape3 (code 32) + End (code 0) in a tag stream ---
        let mut body = Vec::new();
        body.push(0b00000_000); // frame RECT nbits=0
        body.extend_from_slice(&(30u16 << 8).to_le_bytes()); // frame rate 30.0
        body.extend_from_slice(&1u16.to_le_bytes()); // frame count
        let tag = |code: u16, b: &[u8], out: &mut Vec<u8>| {
            assert!(b.len() < 0x3F);
            out.extend_from_slice(&(((code << 6) | b.len() as u16)).to_le_bytes());
            out.extend_from_slice(b);
        };
        tag(32, &shape, &mut body);
        tag(0, &[], &mut body);

        let mut file = Vec::new();
        file.extend_from_slice(b"GFX");
        file.push(8);
        file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
        file.extend_from_slice(&body);

        let m = GfxMovie::parse(&file).expect("parse");
        assert_eq!(&m.magic, b"GFX");
        assert!(!m.compressed);
        assert_eq!(m.frame_rate, 30.0);
        let f = m.features();
        assert_eq!(f.shapes, 1);
        assert_eq!(f.shapes_with_gradient, 1);
        assert_eq!(f.shapes_with_bitmap, 0);
        assert_eq!(f.shapes_parse_incomplete, 0);
        assert_eq!(*f.tag_counts.get(&32).unwrap(), 1);
    }
}
