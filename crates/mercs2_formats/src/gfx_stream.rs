//! A Scaleform movie's tag stream as it is written, every byte-level choice kept: [`TagStream::body`]
//! gives back the bytes [`TagStream::parse`] read, so a movie can be taken apart and put back together.
//!
//! What the writer cannot recompute is kept as read: the frame `RECT` (its field width is the
//! exporter's choice), each tag's header form (a body shorter than `0x3F` bytes may still carry the
//! six-byte header), and any bytes after an `End`. Retail movies carry such bytes: each one's
//! inflated body runs on past its `End` to a multiple of 4096, and the header's length counts only
//! through the `End`. Whether that length counts the eight-byte header too is the writing tool's
//! choice ([`TagStream::length_counts_header`]): retail's deflated movies count it, its three
//! uncompressed ones (`fanfare_new_contact`, `fanfare_support_unlocked`, `snatch_moment`) do not, and
//! the uncompressed movies a `vz-patch.wad` adds do. Checked over every movie of the archive and of the
//! default stack by the workshop's `every_movie_writes_back_as_it_was_read`. `DefineSprite` (39) is opened into its own tag
//! stream, which keeps the same choices, so a sprite's timeline can be edited and its length is the
//! one its tags give. The zlib of a `CFX` body is not kept: retail's loader only inflates it, and the
//! compressed bytes depend on the compressor, so a movie is compared by its inflated body.

use std::io::{Read, Write};

/// `DefineSprite`.
pub const DEFINE_SPRITE: u16 = 39;
/// `End`.
pub const END: u16 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagStream {
    /// `GFX`, `CFX`, `FWS` or `CWS`.
    pub magic: [u8; 3],
    pub version: u8,
    /// Whether the header's length counts the header's own eight bytes besides the body through
    /// `End`.
    pub length_counts_header: bool,
    /// The frame `RECT` as written.
    pub frame_rect: Vec<u8>,
    /// 8.8 fixed point.
    pub frame_rate: u16,
    pub frame_count: u16,
    pub tags: Vec<Tag>,
    /// What follows the `End` tag.
    pub trailing: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub code: u16,
    /// Written with the six-byte header though its body fits the two-byte one.
    pub long: bool,
    pub body: TagBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagBody {
    Raw(Vec<u8>),
    /// `DefineSprite`: its character id, frame count and timeline.
    Sprite { id: u16, frame_count: u16, tags: Vec<Tag>, trailing: Vec<u8> },
}

impl Tag {
    /// The body's length as [`write_tags`] writes it.
    pub fn body_len(&self) -> usize {
        match &self.body {
            TagBody::Raw(b) => b.len(),
            TagBody::Sprite { tags, trailing, .. } => {
                let mut b = Vec::new();
                write_tags(tags, &mut b);
                4 + b.len() + trailing.len()
            }
        }
    }
}

/// A `.gfx` file's body after its eight-byte header, inflated when the magic says it is deflated.
pub fn inflate(file: &[u8]) -> Result<Vec<u8>, String> {
    if file.len() < 8 {
        return Err(format!("{} bytes is too small for a movie header", file.len()));
    }
    match &file[..3] {
        b"CFX" | b"CWS" => {
            let mut out = Vec::new();
            flate2::read::ZlibDecoder::new(&file[8..]).read_to_end(&mut out).map_err(|e| format!("zlib inflate: {e}"))?;
            Ok(out)
        }
        b"GFX" | b"FWS" => Ok(file[8..].to_vec()),
        m => Err(format!("bad magic {:?}", String::from_utf8_lossy(m))),
    }
}

impl TagStream {
    /// Read a whole `.gfx` file, inflating a `CFX`/`CWS` body.
    pub fn parse(file: &[u8]) -> Result<TagStream, String> {
        let body = inflate(file)?;
        let magic = [file[0], file[1], file[2]];
        let mut s = TagStream::parse_body(&body)?;
        s.magic = magic;
        s.version = file[3];
        let length = u32::from_le_bytes([file[4], file[5], file[6], file[7]]) as usize;
        let movie = body.len() - s.trailing.len();
        s.length_counts_header = match length {
            l if l == 8 + movie => true,
            l if l == movie => false,
            l => {
                let after = s.trailing.len();
                return Err(format!("the header's length {l} is neither the {movie}-byte body through End nor it and the header ({after} bytes follow End)"));
            }
        };
        Ok(s)
    }

    /// Read an inflated body (the bytes after the eight-byte header). The header fields are left as
    /// an uncompressed `GFX` version 8 whose length counts its header, as the SWF format has it.
    pub fn parse_body(body: &[u8]) -> Result<TagStream, String> {
        let nbits = (*body.first().ok_or("an empty body has no frame RECT")? >> 3) as usize;
        let rect_len = (5 + 4 * nbits).div_ceil(8);
        let frame_rect = body.get(..rect_len).ok_or("the frame RECT overruns the body")?.to_vec();
        let at = rect_len;
        let word = |i: usize| body.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or("the movie header overruns the body");
        let frame_rate = word(at)?;
        let frame_count = word(at + 2)?;
        let (tags, trailing) = parse_tags(&body[at + 4..])?;
        Ok(TagStream { magic: *b"GFX", version: 8, length_counts_header: true, frame_rect, frame_rate, frame_count, tags, trailing })
    }

    /// The frame `RECT` in twips, `[xmin, xmax, ymin, ymax]`.
    pub fn stage(&self) -> Result<[i32; 4], String> {
        crate::gfx::Cursor::new(&self.frame_rect, 0).read_rect()
    }

    /// The stage in pixels, `[width, height]`.
    pub fn stage_px(&self) -> Result<[f32; 2], String> {
        let [xmin, xmax, ymin, ymax] = self.stage()?;
        Ok([(xmax - xmin) as f32 / 20.0, (ymax - ymin) as f32 / 20.0])
    }

    /// Whether the file's body is deflated (`CFX`, `CWS`).
    pub fn compressed(&self) -> bool {
        matches!(&self.magic, b"CFX" | b"CWS")
    }

    /// The inflated body: frame `RECT`, rate, count, tags, trailing bytes.
    pub fn body(&self) -> Vec<u8> {
        let mut out = self.frame_rect.clone();
        out.extend(self.frame_rate.to_le_bytes());
        out.extend(self.frame_count.to_le_bytes());
        write_tags(&self.tags, &mut out);
        out.extend(&self.trailing);
        out
    }

    /// The header's length for this body: the body through `End`, and the header's eight bytes when
    /// [`Self::length_counts_header`].
    pub fn header_length(&self) -> u32 {
        self.length_for(self.body().len())
    }

    fn length_for(&self, body_len: usize) -> u32 {
        let movie = (body_len - self.trailing.len()) as u32;
        if self.length_counts_header {
            8 + movie
        } else {
            movie
        }
    }

    /// The whole file: the header with [`Self::header_length`], the body deflated when the magic says so.
    pub fn file(&self) -> Result<Vec<u8>, String> {
        let body = self.body();
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend(self.magic);
        out.push(self.version);
        out.extend(self.length_for(body.len()).to_le_bytes());
        if matches!(&self.magic, b"CFX" | b"CWS") {
            let mut z = flate2::write::ZlibEncoder::new(out, flate2::Compression::best());
            z.write_all(&body).map_err(|e| format!("zlib deflate: {e}"))?;
            return z.finish().map_err(|e| format!("zlib deflate: {e}"));
        }
        out.extend(body);
        Ok(out)
    }
}

/// A tag stream up to and including its `End`, and what follows it.
pub fn parse_tags(bytes: &[u8]) -> Result<(Vec<Tag>, Vec<u8>), String> {
    let mut tags = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let head = bytes.get(at..at + 2).ok_or_else(|| format!("a tag header overruns at {at}"))?;
        let rh = u16::from_le_bytes([head[0], head[1]]);
        at += 2;
        let code = rh >> 6;
        let short = (rh & 0x3F) as usize;
        let (len, long) = if short == 0x3F {
            let l = bytes.get(at..at + 4).ok_or_else(|| format!("tag {code}'s long length overruns at {at}"))?;
            at += 4;
            (u32::from_le_bytes([l[0], l[1], l[2], l[3]]) as usize, true)
        } else {
            (short, false)
        };
        let raw = bytes.get(at..at + len).ok_or_else(|| format!("tag {code}'s {len}-byte body overruns at {at}"))?;
        at += len;
        // A short length can only say 0..0x3E; a body of 0x3F or more is long by necessity, not choice.
        let long = long && len < 0x3F;
        let body = if code == DEFINE_SPRITE {
            let w = |i: usize| raw.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or("a DefineSprite too short for its header");
            let (inner, trailing) = parse_tags(raw.get(4..).ok_or("a DefineSprite too short for its header")?)?;
            TagBody::Sprite { id: w(0)?, frame_count: w(2)?, tags: inner, trailing }
        } else {
            TagBody::Raw(raw.to_vec())
        };
        tags.push(Tag { code, long, body });
        if code == END {
            return Ok((tags, bytes[at..].to_vec()));
        }
    }
    Ok((tags, Vec::new()))
}

/// Write `tags` as a stream: each header in the form it was read in, a sprite's length from its tags.
pub fn write_tags(tags: &[Tag], out: &mut Vec<u8>) {
    for tag in tags {
        let body = match &tag.body {
            TagBody::Raw(b) => std::borrow::Cow::Borrowed(b.as_slice()),
            TagBody::Sprite { id, frame_count, tags, trailing } => {
                let mut b = Vec::new();
                b.extend(id.to_le_bytes());
                b.extend(frame_count.to_le_bytes());
                write_tags(tags, &mut b);
                b.extend(trailing);
                std::borrow::Cow::Owned(b)
            }
        };
        if tag.long || body.len() >= 0x3F {
            out.extend(((tag.code << 6) | 0x3F).to_le_bytes());
            out.extend((body.len() as u32).to_le_bytes());
        } else {
            out.extend(((tag.code << 6) | body.len() as u16).to_le_bytes());
        }
        out.extend(body.iter());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body with a short tag written long, a sprite holding a frame, an `End`, and a stray byte
    /// after it comes back byte for byte; a sprite whose timeline grows writes its new length.
    #[test]
    fn a_body_writes_back_as_it_was_read() {
        let mut body = vec![0x50, 0, 0, 0, 0, 0]; // RECT: 10-bit fields (5 + 40 bits → 6 bytes), all zero
        body.extend([0x00, 0x1E, 0x02, 0x00]); // 30 fps, 2 frames
        body.extend([(43 << 6 | 0x3F) as u8, ((43 << 6 | 0x3F) >> 8) as u8, 2, 0, 0, 0, b'a', 0]); // FrameLabel "a", long
        let sprite_inner: Vec<u8> = [vec![0x40, 0x00], vec![0x00, 0x00]].concat(); // ShowFrame, End
        let mut sprite = vec![7, 0, 1, 0];
        sprite.extend(&sprite_inner);
        body.extend((((39u16) << 6) | sprite.len() as u16).to_le_bytes());
        body.extend(&sprite);
        body.extend([0x40, 0x00, 0x00, 0x00, 0xAB]); // ShowFrame, End, trailing
        let s = TagStream::parse_body(&body).expect("parse");
        assert!(s.tags[0].long, "the short FrameLabel kept its long header");
        assert_eq!(s.trailing, vec![0xAB]);
        assert_eq!(s.body(), body);

        let mut grown = s.clone();
        let TagBody::Sprite { tags, .. } = &mut grown.tags[1].body else { panic!("tag 1 is the sprite") };
        tags.insert(0, Tag { code: 1, long: false, body: TagBody::Raw(Vec::new()) });
        let again = TagStream::parse_body(&grown.body()).expect("reparse");
        assert_eq!(again, grown);
        assert_eq!(grown.header_length(), s.header_length() + 2, "one more two-byte ShowFrame");
        assert_eq!(s.header_length() as usize, 8 + body.len() - 1, "the length stops at End and counts the header");

        // A header whose length leaves its own eight bytes out keeps doing so.
        let mut file = b"GFX\x08".to_vec();
        file.extend(((body.len() - 1) as u32).to_le_bytes());
        file.extend(&body);
        let read = TagStream::parse(&file).expect("parse file");
        assert!(!read.length_counts_header);
        assert_eq!(read.file().expect("write"), file);
        file[4] = file[4].wrapping_add(3);
        assert!(TagStream::parse(&file).is_err(), "a length that fits neither form is refused");
    }
}
