//! The file: header, audio track tables, frame index, and a frame's split into its audio records and
//! its video data (`SPEC-audio.md` §1).

/// One audio track's three table entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioTrack {
    /// The largest decoded record, in bytes.
    pub max_buffer: u32,
    /// Sample rate in the low 16 bits; bit 28 DCT, bit 29 stereo, bit 30 16-bit, bit 31 RDFT.
    pub flags: u32,
    /// The id a game asks for (`BinkSetSoundTrack`).
    pub id: u32,
}

impl AudioTrack {
    pub fn rate(&self) -> u32 {
        self.flags & 0xffff
    }

    pub fn channels(&self) -> u32 {
        ((self.flags >> 29) & 1) + 1
    }

    /// Whether the track carries audio the DCT path decodes, the only path the shipped files use.
    pub fn is_dct(&self) -> bool {
        self.flags & 0x8000_0000 == 0 && self.flags & 0x1000_0000 != 0
    }
}

/// A movie file, borrowed.
pub struct Movie<'a> {
    bytes: &'a [u8],
    pub frame_count: u32,
    pub width: u32,
    pub height: u32,
    pub fps: (u32, u32),
    /// Header word 36 (`SPEC-video.md` §1.1): `0x100000` alpha plane, `0x20000` grayscale.
    pub video_flags: u32,
    pub tracks: Vec<AudioTrack>,
    /// `frame_count + 1` offsets, the low bit already cleared.
    offsets: Vec<u32>,
}

fn u32_at(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| format!("the file ends before offset {at:#x}"))
}

impl<'a> Movie<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Movie<'a>, String> {
        if bytes.get(..4) != Some(b"BIKi") {
            return Err(format!("not a BIKi movie: magic {:02x?}", bytes.get(..4)));
        }
        let frame_count = u32_at(bytes, 0x08)?;
        let index_count = u32_at(bytes, 0x10)?;
        if index_count < frame_count || index_count > 1_000_000 {
            return Err(format!("a frame index of {index_count} for {frame_count} frames"));
        }
        let n = u32_at(bytes, 0x28)? as usize;
        if n > 0x100 {
            return Err(format!("{n} audio tracks"));
        }
        let tracks = (0..n)
            .map(|t| {
                Ok(AudioTrack {
                    max_buffer: u32_at(bytes, 0x2c + 4 * t)?,
                    flags: u32_at(bytes, 0x2c + 4 * (n + t))?,
                    id: u32_at(bytes, 0x2c + 4 * (2 * n + t))?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let table = 0x2c + 12 * n;
        let offsets = (0..=index_count as usize).map(|i| Ok(u32_at(bytes, table + 4 * i)? & !1)).collect::<Result<Vec<_>, String>>()?;
        Ok(Movie {
            bytes,
            frame_count,
            width: u32_at(bytes, 0x14)?,
            height: u32_at(bytes, 0x18)?,
            fps: (u32_at(bytes, 0x1c)?, u32_at(bytes, 0x20)?),
            video_flags: u32_at(bytes, 0x24)?,
            tracks,
            offsets,
        })
    }

    /// Frame `i`'s bytes.
    pub fn frame(&self, i: u32) -> Result<&'a [u8], String> {
        let (start, end) = (self.offsets.get(i as usize), self.offsets.get(i as usize + 1));
        let (Some(&start), Some(&end)) = (start, end) else {
            return Err(format!("frame {i} of {}", self.frame_count));
        };
        self.bytes.get(start as usize..end as usize).ok_or_else(|| format!("frame {i} runs past the file"))
    }

    /// Frame `i` split into one audio record per track, in track order (empty when the track has no
    /// audio this frame), and the video data after them.
    pub fn split(&self, i: u32) -> Result<(Vec<&'a [u8]>, &'a [u8]), String> {
        let frame = self.frame(i)?;
        let mut at = 0usize;
        let mut records = Vec::with_capacity(self.tracks.len());
        for t in 0..self.tracks.len() {
            let len = u32_at(frame, at).map_err(|e| format!("frame {i} track {t}: {e}"))? as usize;
            let end = at + 4 + len;
            if end > frame.len() {
                return Err(format!("frame {i} track {t}: a record of {len} bytes past the frame's {}", frame.len()));
            }
            records.push(&frame[at + 4..end]);
            at = end;
        }
        Ok((records, &frame[at..]))
    }
}
