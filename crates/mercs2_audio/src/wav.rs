//! A strict reader for the one WAV shape a wavebank record can embed: uncompressed PCM16, mono or
//! stereo, at any sample rate above zero.
//!
//! A record carries the rate as a free `u32` (`wavebank` record `+0x08`, read verbatim by
//! [`crate::wave`]), and the mix kernel steps through a wave by `(freq << 32) / rate`
//! (`FUN_00839fd0`, [`crate::mixer`]), so no rate is special to the engine. The channel count is 1 or
//! 2 in every retail record, and the embedded format is PCM16 (`format` byte 2).
//!
//! Everything else is refused with the reason: a file that is not RIFF/WAVE, a format tag other than
//! 1 (PCM — `WAVE_FORMAT_EXTENSIBLE` included), a sample width other than 16 bits, a channel count
//! other than 1 or 2, a rate of 0, a data chunk that is empty or ends inside a frame, and a file
//! whose chunks do not add up to its bytes.

use crate::encode::Pcm16;

/// Why a WAV file was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WavError {
    /// The file does not start with `RIFF <size> WAVE`.
    NotRiffWave,
    /// The RIFF size field does not match the file length.
    RiffSize { declared: u32, actual: usize },
    /// A chunk header or body runs past the end of the RIFF body.
    Truncated { chunk: [u8; 4], offset: usize },
    /// No `fmt ` chunk.
    MissingFmt,
    /// No `data` chunk.
    MissingData,
    /// A second `fmt ` or `data` chunk.
    DuplicateChunk { chunk: [u8; 4] },
    /// The `data` chunk precedes the `fmt ` chunk.
    DataBeforeFmt,
    /// The `fmt ` chunk is shorter than the 16 bytes PCM needs.
    FmtTooShort { len: u32 },
    /// The format tag is not 1 (PCM).
    Format { tag: u16 },
    /// The sample width is not 16 bits.
    Bits { bits: u16 },
    /// The channel count is not 1 or 2.
    Channels { channels: u16 },
    /// The sample rate is 0.
    ZeroRate,
    /// The block align is not `channels × 2`.
    BlockAlign { block_align: u16, channels: u16 },
    /// The `data` chunk is empty.
    EmptyData,
    /// The `data` chunk's length is not a whole number of frames.
    PartialFrame { bytes: u32, frame: u16 },
}

impl std::fmt::Display for WavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tag = |t: &[u8; 4]| String::from_utf8_lossy(t).into_owned();
        match self {
            WavError::NotRiffWave => write!(f, "not a RIFF/WAVE file"),
            WavError::RiffSize { declared, actual } => write!(
                f,
                "the RIFF header declares {declared} bytes after it, but the file has {}",
                actual.saturating_sub(8)
            ),
            WavError::Truncated { chunk, offset } => {
                write!(f, "chunk {:?} at byte {offset} runs past the end of the file", tag(chunk))
            }
            WavError::MissingFmt => write!(f, "no `fmt ` chunk"),
            WavError::MissingData => write!(f, "no `data` chunk"),
            WavError::DuplicateChunk { chunk } => write!(f, "a second {:?} chunk", tag(chunk)),
            WavError::DataBeforeFmt => write!(f, "the `data` chunk comes before the `fmt ` chunk"),
            WavError::FmtTooShort { len } => write!(f, "the `fmt ` chunk is {len} bytes; PCM needs 16"),
            WavError::Format { tag } => write!(
                f,
                "format tag {tag:#06X}; only 1 (uncompressed PCM) is accepted — export as plain \
                 16-bit PCM, not extensible, float or compressed"
            ),
            WavError::Bits { bits } => write!(f, "{bits}-bit samples; only 16-bit PCM is accepted"),
            WavError::Channels { channels } => {
                write!(f, "{channels} channels; a wave is mono or stereo")
            }
            WavError::ZeroRate => write!(f, "sample rate 0"),
            WavError::BlockAlign { block_align, channels } => write!(
                f,
                "block align {block_align} does not match {channels} channel(s) of 16-bit samples ({})",
                channels * 2
            ),
            WavError::EmptyData => write!(f, "the `data` chunk is empty"),
            WavError::PartialFrame { bytes, frame } => write!(
                f,
                "the `data` chunk is {bytes} bytes, not a whole number of {frame}-byte frames"
            ),
        }
    }
}

impl std::error::Error for WavError {}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Read a PCM16 WAV file into interleaved samples.
pub fn read_pcm16_wav(bytes: &[u8]) -> Result<Pcm16, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotRiffWave);
    }
    let declared = u32_at(bytes, 4);
    if declared as usize != bytes.len() - 8 {
        return Err(WavError::RiffSize { declared, actual: bytes.len() });
    }

    // (channels, rate, block align) once `fmt ` is read.
    let mut fmt: Option<(u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut pos = 12;
    while pos < bytes.len() {
        if pos + 8 > bytes.len() {
            let mut id = [0u8; 4];
            let have = bytes.len() - pos;
            id[..have.min(4)].copy_from_slice(&bytes[pos..pos + have.min(4)]);
            return Err(WavError::Truncated { chunk: id, offset: pos });
        }
        let id: [u8; 4] = [bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]];
        let len = u32_at(bytes, pos + 4);
        let body_start = pos + 8;
        let body_end = body_start
            .checked_add(len as usize)
            .filter(|&e| e <= bytes.len())
            .ok_or(WavError::Truncated { chunk: id, offset: pos })?;
        let body = &bytes[body_start..body_end];
        match &id {
            b"fmt " => {
                if fmt.is_some() {
                    return Err(WavError::DuplicateChunk { chunk: id });
                }
                if len < 16 {
                    return Err(WavError::FmtTooShort { len });
                }
                let tag = u16_at(body, 0);
                let channels = u16_at(body, 2);
                let rate = u32_at(body, 4);
                let block_align = u16_at(body, 12);
                let bits = u16_at(body, 14);
                if tag != 1 {
                    return Err(WavError::Format { tag });
                }
                if bits != 16 {
                    return Err(WavError::Bits { bits });
                }
                if channels != 1 && channels != 2 {
                    return Err(WavError::Channels { channels });
                }
                if rate == 0 {
                    return Err(WavError::ZeroRate);
                }
                if block_align != channels * 2 {
                    return Err(WavError::BlockAlign { block_align, channels });
                }
                fmt = Some((channels, rate, block_align));
            }
            b"data" => {
                if data.is_some() {
                    return Err(WavError::DuplicateChunk { chunk: id });
                }
                if fmt.is_none() {
                    return Err(WavError::DataBeforeFmt);
                }
                data = Some(body);
            }
            _ => {}
        }
        // Chunk bodies are padded to an even length; the pad byte is not counted in `len`.
        pos = body_end + (len as usize & 1);
        if pos > bytes.len() {
            return Err(WavError::Truncated { chunk: id, offset: body_end });
        }
    }

    let (channels, sample_rate, block_align) = fmt.ok_or(WavError::MissingFmt)?;
    let data = data.ok_or(WavError::MissingData)?;
    if data.is_empty() {
        return Err(WavError::EmptyData);
    }
    if data.len() % block_align as usize != 0 {
        return Err(WavError::PartialFrame { bytes: data.len() as u32, frame: block_align });
    }
    let samples = data.chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]])).collect();
    Ok(Pcm16 { channels: channels as u8, sample_rate, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV with the given format fields and data bytes, plus optional extra chunks before `data`.
    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8], extra: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&tag.to_le_bytes());
        body.extend_from_slice(&channels.to_le_bytes());
        body.extend_from_slice(&rate.to_le_bytes());
        body.extend_from_slice(&(rate * block_align as u32).to_le_bytes());
        body.extend_from_slice(&block_align.to_le_bytes());
        body.extend_from_slice(&bits.to_le_bytes());
        for (id, b) in extra {
            body.extend_from_slice(*id);
            body.extend_from_slice(&(b.len() as u32).to_le_bytes());
            body.extend_from_slice(b);
            if b.len() % 2 == 1 {
                body.push(0);
            }
        }
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    #[test]
    fn mono_and_stereo_pcm16_read_verbatim() {
        let s = [0i16, 1, -1, i16::MAX, i16::MIN, 1234];
        let mono = read_pcm16_wav(&wav(1, 1, 22050, 16, &pcm(&s), &[])).expect("mono");
        assert_eq!(mono, Pcm16 { channels: 1, sample_rate: 22050, samples: s.to_vec() });
        let stereo = read_pcm16_wav(&wav(1, 2, 44100, 16, &pcm(&s), &[])).expect("stereo");
        assert_eq!(stereo, Pcm16 { channels: 2, sample_rate: 44100, samples: s.to_vec() });
    }

    #[test]
    fn any_nonzero_rate_is_accepted_and_unknown_chunks_are_passed_over() {
        let s = [5i16, 6];
        let odd = read_pcm16_wav(&wav(1, 1, 11_111, 16, &pcm(&s), &[(b"LIST", b"abc")])).expect("reads");
        assert_eq!(odd.sample_rate, 11_111);
        assert_eq!(odd.samples, s);
    }

    #[test]
    fn every_unusable_file_is_refused_with_its_reason() {
        let s = pcm(&[1, 2, 3, 4]);
        assert_eq!(read_pcm16_wav(b"not a wav at all"), Err(WavError::NotRiffWave));
        let mut riffx = wav(1, 1, 22050, 16, &s, &[]);
        riffx[8..12].copy_from_slice(b"AVI ");
        assert_eq!(read_pcm16_wav(&riffx), Err(WavError::NotRiffWave));
        assert_eq!(read_pcm16_wav(&wav(3, 1, 22050, 16, &s, &[])), Err(WavError::Format { tag: 3 }));
        assert_eq!(
            read_pcm16_wav(&wav(0xFFFE, 2, 22050, 16, &s, &[])),
            Err(WavError::Format { tag: 0xFFFE })
        );
        assert_eq!(read_pcm16_wav(&wav(1, 1, 22050, 8, &s, &[])), Err(WavError::Bits { bits: 8 }));
        assert_eq!(read_pcm16_wav(&wav(1, 1, 22050, 24, &s[..3], &[])), Err(WavError::Bits { bits: 24 }));
        assert_eq!(read_pcm16_wav(&wav(1, 6, 22050, 16, &s, &[])), Err(WavError::Channels { channels: 6 }));
        assert_eq!(read_pcm16_wav(&wav(1, 0, 22050, 16, &s, &[])), Err(WavError::Channels { channels: 0 }));
        assert_eq!(read_pcm16_wav(&wav(1, 1, 0, 16, &s, &[])), Err(WavError::ZeroRate));
        assert_eq!(read_pcm16_wav(&wav(1, 1, 22050, 16, &[], &[])), Err(WavError::EmptyData));
        assert_eq!(
            read_pcm16_wav(&wav(1, 2, 22050, 16, &s[..6], &[])),
            Err(WavError::PartialFrame { bytes: 6, frame: 4 })
        );
        assert_eq!(
            read_pcm16_wav(&wav(1, 1, 22050, 16, &s[..3], &[])),
            Err(WavError::PartialFrame { bytes: 3, frame: 2 })
        );
    }

    #[test]
    fn structural_damage_is_refused() {
        let s = pcm(&[1, 2]);
        let good = wav(1, 1, 22050, 16, &s, &[]);

        let mut short = good.clone();
        short.truncate(short.len() - 2);
        assert!(matches!(read_pcm16_wav(&short), Err(WavError::RiffSize { .. })));

        // Fix the RIFF size so the damage is the chunk's own.
        let mut cut = good.clone();
        cut.truncate(cut.len() - 2);
        let n = (cut.len() - 8) as u32;
        cut[4..8].copy_from_slice(&n.to_le_bytes());
        assert!(matches!(read_pcm16_wav(&cut), Err(WavError::Truncated { chunk, .. }) if &chunk == b"data"));

        let mut bad_align = good.clone();
        bad_align[32..34].copy_from_slice(&4u16.to_le_bytes());
        assert_eq!(read_pcm16_wav(&bad_align), Err(WavError::BlockAlign { block_align: 4, channels: 1 }));

        // No data chunk: rename it.
        let mut no_data = good.clone();
        no_data[36..40].copy_from_slice(b"junk");
        assert_eq!(read_pcm16_wav(&no_data), Err(WavError::MissingData));

        // No fmt chunk: rename it, and the data chunk comes first.
        let mut no_fmt = good.clone();
        no_fmt[12..16].copy_from_slice(b"junk");
        assert_eq!(read_pcm16_wav(&no_fmt), Err(WavError::DataBeforeFmt));

        let dup = wav(1, 1, 22050, 16, &s, &[(b"data", &s)]);
        assert_eq!(read_pcm16_wav(&dup), Err(WavError::DuplicateChunk { chunk: *b"data" }));

        let mut short_fmt = good;
        short_fmt[16..20].copy_from_slice(&14u32.to_le_bytes());
        assert_eq!(read_pcm16_wav(&short_fmt), Err(WavError::FmtTooShort { len: 14 }));
    }
}
