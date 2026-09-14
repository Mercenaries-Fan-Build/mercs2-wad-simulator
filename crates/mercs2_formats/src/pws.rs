//! Carve embedded RIFF/WAVE and OggS payloads out of a Mercenaries 2 `.pws`
//! (or arbitrary blob bank).
//!
//! Ported from `tools/pws_extractor.py`. This module deliberately does NOT
//! decode audio — the IMA-ADPCM decoder lives in
//! [`mercs2_audio::wave`](../../../mercs2_audio/src/wave.rs) and the
//! Xbox→PC transcoder in [`crate::be_to_le::audio`]. Here we only slice
//! payloads out of the container.
//!
//! # Layout notes
//!
//! - **RIFF/WAVE** — real container: `"RIFF" <u32 le size> "WAVE" ...`.
//!   Total on-disk length is `8 + size`.
//! - **OggS** — the shipped banks do not always carry a length field, so
//!   the Python reference caps a carve at 512 KiB from the magic; we do
//!   the same for behavioural parity.
//!
//! # Public API
//!
//! - [`ClipKind`] — which magic matched.
//! - [`WaveMeta`] — optional metadata parsed out of a `fmt `/`data` pair.
//! - [`CarvedClip`] — one carved payload (offset/size/kind + optional meta).
//! - [`carve`] — scan a whole buffer and return every distinct payload.

/// Which magic identified the carve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    /// `RIFF`/`WAVE` container (size known from the RIFF header).
    Riff,
    /// `OggS` page start (length is a 512 KiB cap, not authoritative).
    Ogg,
}

impl ClipKind {
    /// Short lowercase label matching the Python reference (`"riff"` / `"ogg"`).
    pub fn label(self) -> &'static str {
        match self {
            ClipKind::Riff => "riff",
            ClipKind::Ogg => "ogg",
        }
    }
}

/// Metadata parsed from a WAVE `fmt `/`data` pair.
///
/// Every field mirrors what `parse_wave_metadata_from_riff` populated in
/// the Python extractor. `duration_seconds` is only present when the RIFF
/// carried a `data` chunk AND `byte_rate > 0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveMeta {
    pub audio_format: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub byte_rate: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
    pub duration_seconds: Option<f64>,
}

/// One carved payload — a byte range inside the source buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct CarvedClip {
    /// Byte offset of the magic inside the source buffer.
    pub offset: usize,
    /// Total carved length in bytes (including the magic).
    pub size: usize,
    /// Which magic identified this carve.
    pub kind: ClipKind,
    /// WAVE metadata, when [`kind`](Self::kind) is [`ClipKind::Riff`] and
    /// the RIFF is a well-formed WAVE with a `fmt ` chunk.
    pub wave: Option<WaveMeta>,
}

/// Same cap the Python reference uses for OggS carves (512 KiB).
const OGG_CAP: usize = 524_288;

/// Every offset in `data` where `needle` starts, in ascending order.
///
/// Mirrors the Python `find_all`: the search stride is +1 so overlapping
/// hits are not skipped (relevant for the OggS scan on packed banks).
fn find_all(data: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > data.len() {
        return out;
    }
    let last = data.len() - needle.len();
    let mut i = 0;
    while i <= last {
        if &data[i..i + needle.len()] == needle {
            out.push(i);
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn read_u16_le(buf: &[u8], off: usize) -> Option<u16> {
    let s = buf.get(off..off + 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
    let s = buf.get(off..off + 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Total on-disk RIFF length (header + declared payload), or `None` if the
/// offset is not a plausible RIFF header.
fn riff_chunk_size(data: &[u8], riff_off: usize) -> Option<usize> {
    if riff_off.checked_add(12)? > data.len() {
        return None;
    }
    if &data[riff_off..riff_off + 4] != b"RIFF" {
        return None;
    }
    let sz = read_u32_le(data, riff_off + 4)? as usize;
    // RIFF header (8 bytes: "RIFF" + size dword) + declared payload.
    Some(8usize.checked_add(sz)?)
}

/// Parse `fmt `/`data` out of a full RIFF/WAVE blob. Returns `None` for
/// any RIFF that is not WAVE, or a WAVE with no usable `fmt ` chunk.
fn parse_wave_metadata(riff_blob: &[u8]) -> Option<WaveMeta> {
    if riff_blob.len() < 12
        || &riff_blob[0..4] != b"RIFF"
        || &riff_blob[8..12] != b"WAVE"
    {
        return None;
    }
    let mut pos = 12usize;
    let mut audio_format: Option<u16> = None;
    let mut channels: Option<u16> = None;
    let mut sample_rate: Option<u32> = None;
    let mut byte_rate: Option<u32> = None;
    let mut block_align: Option<u16> = None;
    let mut bits_per_sample: Option<u16> = None;
    let mut data_bytes: Option<u32> = None;
    while pos + 8 <= riff_blob.len() {
        let cid = &riff_blob[pos..pos + 4];
        let csize = read_u32_le(riff_blob, pos + 4)? as usize;
        pos += 8;
        if pos.checked_add(csize)? > riff_blob.len() {
            break;
        }
        if cid == b"fmt " && csize >= 16 {
            audio_format = read_u16_le(riff_blob, pos);
            channels = read_u16_le(riff_blob, pos + 2);
            sample_rate = read_u32_le(riff_blob, pos + 4);
            byte_rate = read_u32_le(riff_blob, pos + 8);
            block_align = read_u16_le(riff_blob, pos + 12);
            bits_per_sample = read_u16_le(riff_blob, pos + 14);
        } else if cid == b"data" {
            data_bytes = Some(csize as u32);
        }
        // Chunks are word-padded on disk.
        pos += csize + (csize & 1);
    }
    let audio_format = audio_format?;
    let channels = channels?;
    let sample_rate = sample_rate?;
    let bits_per_sample = bits_per_sample?;
    let byte_rate_v = byte_rate.unwrap_or(0);
    let block_align_v = block_align.unwrap_or(0);
    let duration_seconds = match (data_bytes, byte_rate_v) {
        (Some(db), br) if br > 0 => {
            // Round to 6 decimal places to match the Python `round(_, 6)`.
            let secs = f64::from(db) / f64::from(br);
            Some((secs * 1_000_000.0).round() / 1_000_000.0)
        }
        _ => None,
    };
    Some(WaveMeta {
        audio_format,
        channels,
        sample_rate,
        byte_rate: byte_rate_v,
        block_align: block_align_v,
        bits_per_sample,
        duration_seconds,
    })
}

/// Scan `bytes` for every embedded RIFF and OggS payload.
///
/// The scan is order-preserving in the Python sense: RIFF hits first (in
/// ascending offset order), then OggS hits. Duplicate `(offset, size)`
/// pairs are dropped, and any carve shorter than 16 bytes or that would
/// overrun the buffer is skipped.
pub fn carve(bytes: &[u8]) -> Vec<CarvedClip> {
    let mut hits: Vec<CarvedClip> = Vec::new();
    let mut used: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();

    for &(magic, kind) in &[
        (&b"RIFF"[..], ClipKind::Riff),
        (&b"OggS"[..], ClipKind::Ogg),
    ] {
        for off in find_all(bytes, magic) {
            let length = match kind {
                ClipKind::Riff => match riff_chunk_size(bytes, off) {
                    Some(l) => l,
                    None => continue,
                },
                ClipKind::Ogg => {
                    let end = bytes.len().min(off.saturating_add(OGG_CAP));
                    end - off
                }
            };
            if length < 16 {
                continue;
            }
            let end = match off.checked_add(length) {
                Some(e) => e,
                None => continue,
            };
            if end > bytes.len() {
                continue;
            }
            let key = (off, length);
            if !used.insert(key) {
                continue;
            }
            let wave = match kind {
                ClipKind::Riff => parse_wave_metadata(&bytes[off..end]),
                ClipKind::Ogg => None,
            };
            hits.push(CarvedClip {
                offset: off,
                size: length,
                kind,
                wave,
            });
        }
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wave(sample_rate: u32, channels: u16, data: &[u8]) -> Vec<u8> {
        // "RIFF" <u32 size> "WAVE" "fmt " <u32 16> <fmt body 16 bytes> "data" <u32 dsize> <data...>
        let byte_rate: u32 = sample_rate * u32::from(channels) * 2;
        let block_align: u16 = channels * 2;
        let mut out: Vec<u8> = Vec::new();
        out.extend_from_slice(b"RIFF");
        let riff_size = 4 + 8 + 16 + 8 + data.len();
        out.extend_from_slice(&(riff_size as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // audio_format = PCM
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes()); // bits_per_sample
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn carves_single_wave() {
        let wav = make_wave(44100, 1, &[0u8; 88_200]); // 1 second mono
        let mut buf = vec![0u8; 32];
        buf.extend_from_slice(&wav);
        buf.extend_from_slice(&[0u8; 16]);
        let hits = carve(&buf);
        assert_eq!(hits.len(), 1);
        let h = &hits[0];
        assert_eq!(h.kind, ClipKind::Riff);
        assert_eq!(h.offset, 32);
        assert_eq!(h.size, wav.len());
        let w = h.wave.expect("wave metadata");
        assert_eq!(w.channels, 1);
        assert_eq!(w.sample_rate, 44100);
        assert_eq!(w.byte_rate, 88_200);
        assert_eq!(w.bits_per_sample, 16);
        assert_eq!(w.duration_seconds, Some(1.0));
    }

    #[test]
    fn carves_ogg_with_cap() {
        // OggS + junk; length should be end-of-buffer (below the 512 KiB cap).
        let mut buf = vec![0u8; 8];
        buf.extend_from_slice(b"OggS");
        buf.extend_from_slice(&[0u8; 100]);
        let hits = carve(&buf);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, ClipKind::Ogg);
        assert_eq!(hits[0].offset, 8);
        assert_eq!(hits[0].size, 104);
        assert!(hits[0].wave.is_none());
    }

    #[test]
    fn skips_riff_that_overruns() {
        // Valid magic, but declared size exceeds the buffer.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&1_000_000u32.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        assert!(carve(&buf).is_empty());
    }

    #[test]
    fn skips_too_short() {
        let buf = b"OggS".to_vec(); // length 4 < 16
        assert!(carve(&buf).is_empty());
    }

    #[test]
    fn deduplicates_identical_hits() {
        // Same offset can only be visited once per magic; if a later
        // pass produced the identical (offset,size) key it's dropped.
        let wav = make_wave(22050, 2, &[0u8; 64]);
        let hits = carve(&wav);
        assert_eq!(hits.len(), 1);
    }
}
