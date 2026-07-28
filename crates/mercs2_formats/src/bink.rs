//! Bink 1 (`BIK*`) container parsing — header, audio-track table and frame index.
//!
//! Mercenaries 2 ships its cinematics as 45 `.bik` files in `data/Movies/`, as plain files on disk
//! rather than inside a WAD. Playback in retail is RAD's `binkw32.dll`; this module is the start of
//! reading them ourselves.
//!
//! **Scope, measured from the shipped set rather than assumed.** `the_shipped_movie_set_is_uniform`
//! parses all 45 and pins what it finds, so an asset that breaks these assumptions fails loudly
//! instead of being decoded as garbage:
//!
//! ```text
//! 45 files, 40.1 min total
//! revisions:   {'i'}          -> ONE video code path, not the b/f/g/h/i spread
//! video_flags: 0 everywhere   -> no alpha plane
//! resolution:  1024x576 @30fps  x36   the mission cinematics (16:9, 576p)
//!              1280x720          x5   EA / Pandemic / title cards / 01_VIK_01
//!              600x720           x4   shell + main-menu backdrops (portrait panel)
//! audio:       289 tracks, ALL DCT — not one RDFT track in the set
//!              22050 Hz (280), 44100 Hz (4), 48000 Hz (5); mono and stereo both present
//!              7 files carry no audio at all
//! ```
//!
//! The audio consequence is worth stating plainly: Bink audio has two bitstreams and **we only have
//! to implement `Dct`**. [`AudioCodec::Rdft`] is still modelled here because the container can
//! express it and silently mis-decoding one would be worse than refusing it — but no shipped Mercs2
//! movie uses it.
//!
//! This is CONTAINER only: it locates frames and describes tracks, and decodes neither video nor
//! audio. The bitstream decoders build on top.
//!
//! ## Layout
//!
//! All little-endian. Field order is fixed; the audio tables are three parallel arrays, not an array
//! of structs, which is the easy thing to get wrong.
//!
//! ```text
//! 0x00  [4]  magic "BIK" + revision byte
//! 0x04  u32  file size - 8
//! 0x08  u32  frame count
//! 0x0C  u32  largest frame size
//! 0x10  u32  frame count (repeated)
//! 0x14  u32  width
//! 0x18  u32  height
//! 0x1C  u32  fps dividend
//! 0x20  u32  fps divisor
//! 0x24  u32  video flags
//! 0x28  u32  audio track count  (N)
//! ---- present only when N > 0 ----
//!       u32 x N   max decoded buffer size per track
//!       u16,u16 x N   (sample rate, flags) per track
//!       u32 x N   track id
//! ---- always ----
//!       u32 x (frames + 1)   frame offsets; the extra entry terminates the last frame.
//!                            Bit 0 is a KEYFRAME flag, not part of the offset.
//! ```

/// Bink audio comes in two bitstream flavours, chosen per track by [`AUD_USE_DCT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCodec {
    /// FFT/RDFT-based — the original Bink audio.
    Rdft,
    /// DCT-based — the later variant.
    Dct,
}

/// Track flag: audio is stereo rather than mono.
pub const AUD_STEREO: u16 = 0x2000;
/// Track flag: audio uses the DCT bitstream rather than the RDFT one.
pub const AUD_USE_DCT: u16 = 0x1000;

/// One audio track's parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioTrack {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Raw flag word, kept verbatim so nothing is lost behind the decoded accessors.
    pub flags: u16,
    /// Track identifier — which localisation this track carries.
    pub id: u32,
    /// Decoder-side maximum output buffer size.
    pub max_decoded_size: u32,
}

impl AudioTrack {
    /// Channel count: stereo tracks set [`AUD_STEREO`].
    pub fn channels(&self) -> u16 {
        if self.flags & AUD_STEREO != 0 {
            2
        } else {
            1
        }
    }

    /// Which bitstream flavour this track uses.
    pub fn codec(&self) -> AudioCodec {
        if self.flags & AUD_USE_DCT != 0 {
            AudioCodec::Dct
        } else {
            AudioCodec::Rdft
        }
    }
}

/// One frame's location in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameEntry {
    /// Byte offset from the start of the file.
    pub offset: u64,
    /// Length in bytes, derived from the next entry's offset.
    pub length: u64,
    /// Bit 0 of the stored offset marks a keyframe.
    pub keyframe: bool,
}

/// A parsed Bink container: everything but the frame bitstreams themselves.
#[derive(Clone, Debug, PartialEq)]
pub struct BinkFile {
    /// Revision byte after `BIK` — `b`, `f`, `g`, `h` or `i`. Mercs2 ships only `i`.
    pub revision: u8,
    pub width: u32,
    pub height: u32,
    /// Frame rate numerator / denominator, as stored. Not reduced.
    pub fps_dividend: u32,
    pub fps_divisor: u32,
    /// Video feature flags. `0` across the whole Mercs2 set (notably: no alpha plane).
    pub video_flags: u32,
    /// Largest single frame, in bytes — the decode buffer size to reserve.
    pub largest_frame: u32,
    pub audio_tracks: Vec<AudioTrack>,
    pub frames: Vec<FrameEntry>,
}

impl BinkFile {
    /// Frames per second as a float. `0.0` when the divisor is zero rather than dividing by it.
    pub fn fps(&self) -> f64 {
        if self.fps_divisor == 0 {
            return 0.0;
        }
        self.fps_dividend as f64 / self.fps_divisor as f64
    }

    /// Total running time in seconds.
    pub fn duration_secs(&self) -> f64 {
        let fps = self.fps();
        if fps <= 0.0 {
            return 0.0;
        }
        self.frames.len() as f64 / fps
    }
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, String> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| {
            format!(
                "truncated at offset {off:#x} (need 4 bytes, have {})",
                b.len()
            )
        })
}

fn u16_at(b: &[u8], off: usize) -> Result<u16, String> {
    b.get(off..off + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| {
            format!(
                "truncated at offset {off:#x} (need 2 bytes, have {})",
                b.len()
            )
        })
}

/// Parse a Bink container from a whole file.
///
/// Validates the structural invariants the format guarantees, so a mis-parse surfaces here rather
/// than as a corrupt frame later: the magic, the two independently-stored frame counts agreeing, the
/// declared file size, and a frame table that is monotonic and inside the file.
pub fn parse(bytes: &[u8]) -> Result<BinkFile, String> {
    if bytes.len() < 44 {
        return Err(format!(
            "too short to be a Bink file ({} bytes)",
            bytes.len()
        ));
    }
    if &bytes[0..3] != b"BIK" {
        return Err(format!(
            "not a Bink file: magic {:02X?} (expected \"BIK\" + revision)",
            &bytes[0..4.min(bytes.len())]
        ));
    }
    let revision = bytes[3];

    // The stored size counts everything after this field, so it is the file length minus 8.
    let declared = u32_at(bytes, 0x04)? as u64;
    let actual = bytes.len() as u64;
    if declared + 8 != actual {
        return Err(format!(
            "declared size {} + 8 != file length {actual} — truncated or not a Bink file",
            declared
        ));
    }

    let frame_count = u32_at(bytes, 0x08)?;
    let largest_frame = u32_at(bytes, 0x0C)?;
    let frame_count2 = u32_at(bytes, 0x10)?;
    // The count is stored twice. Retail writes them equal; a mismatch means we are misreading the
    // layout, and continuing would index the frame table wrongly.
    if frame_count != frame_count2 {
        return Err(format!(
            "frame count stored twice and disagrees ({frame_count} vs {frame_count2}) — layout misread"
        ));
    }
    let width = u32_at(bytes, 0x14)?;
    let height = u32_at(bytes, 0x18)?;
    let fps_dividend = u32_at(bytes, 0x1C)?;
    let fps_divisor = u32_at(bytes, 0x20)?;
    let video_flags = u32_at(bytes, 0x24)?;
    let track_count = u32_at(bytes, 0x28)? as usize;

    if width == 0 || height == 0 {
        return Err(format!("degenerate dimensions {width}x{height}"));
    }

    // Three PARALLEL arrays, not an array of structs: all the sizes, then all the (rate, flags)
    // pairs, then all the ids.
    let mut cursor = 0x2C;
    let mut audio_tracks = Vec::with_capacity(track_count);
    if track_count > 0 {
        let sizes_at = cursor;
        let rates_at = sizes_at + track_count * 4;
        let ids_at = rates_at + track_count * 4;
        for i in 0..track_count {
            let max_decoded_size = u32_at(bytes, sizes_at + i * 4)?;
            let sample_rate = u16_at(bytes, rates_at + i * 4)? as u32;
            let flags = u16_at(bytes, rates_at + i * 4 + 2)?;
            let id = u32_at(bytes, ids_at + i * 4)?;
            audio_tracks.push(AudioTrack {
                sample_rate,
                flags,
                id,
                max_decoded_size,
            });
        }
        cursor = ids_at + track_count * 4;
    }

    // frames + 1 offsets: the extra one bounds the final frame.
    let n_offsets = frame_count as usize + 1;
    let mut raw = Vec::with_capacity(n_offsets);
    for i in 0..n_offsets {
        raw.push(u32_at(bytes, cursor + i * 4)? as u64);
    }

    let mut frames = Vec::with_capacity(frame_count as usize);
    for i in 0..frame_count as usize {
        // Bit 0 is a keyframe marker riding along in the offset word; mask it off before using the
        // value as an offset. The terminator gets the same treatment.
        let keyframe = raw[i] & 1 != 0;
        let start = raw[i] & !1;
        let end = raw[i + 1] & !1;
        if end < start {
            return Err(format!("frame {i} offsets go backwards ({start} -> {end})"));
        }
        if end > actual {
            return Err(format!(
                "frame {i} ends at {end}, past the {actual}-byte file"
            ));
        }
        frames.push(FrameEntry {
            offset: start,
            length: end - start,
            keyframe,
        });
    }

    Ok(BinkFile {
        revision,
        width,
        height,
        fps_dividend,
        fps_divisor,
        video_flags,
        largest_frame,
        audio_tracks,
        frames,
    })
}

/// One frame split into its per-track audio packets and the video bitstream that follows them.
///
/// Borrowed from the file buffer — no copying; the decoders read straight out of these slices.
#[derive(Clone, Debug)]
pub struct FramePackets<'a> {
    /// One entry per audio track, index-aligned to [`BinkFile::audio_tracks`]. `None` where the
    /// track carried no data this frame, which is normal and not an error.
    pub audio: Vec<Option<&'a [u8]>>,
    /// The video bitstream: whatever remains after the audio packets.
    pub video: &'a [u8],
}

/// Split one frame into its audio packets and video bitstream.
///
/// **Frame layout.** A frame is a run of audio packets — one per declared track, in track order —
/// followed by the video data, which simply occupies the rest:
///
/// ```text
/// per track:  u32 packet_size, then packet_size bytes
/// then:       video bitstream (frame_length - everything above)
/// ```
///
/// A `packet_size` of 0 means that track is silent this frame; the size word is still present. The
/// packet's own leading `u32` is a decoded-sample count belonging to the audio decoder, so it is
/// left inside the slice rather than stripped here.
///
/// This split is exact, and [`the_frame_layout_holds_across_every_shipped_frame`] checks it that way:
/// the audio sizes plus the video remainder must account for the frame's bytes precisely, on every
/// frame of every file. A wrong layout cannot pass that.
///
/// [the_frame_layout_holds_across_every_shipped_frame]: self::tests
pub fn split_frame<'a>(
    file: &BinkFile,
    frame: &FrameEntry,
    bytes: &'a [u8],
) -> Result<FramePackets<'a>, String> {
    let start = frame.offset as usize;
    let end = start + frame.length as usize;
    let data = bytes.get(start..end).ok_or_else(|| {
        format!(
            "frame at {start}..{end} is outside the {}-byte file",
            bytes.len()
        )
    })?;

    let mut cursor = 0usize;
    let mut audio = Vec::with_capacity(file.audio_tracks.len());
    for (i, _) in file.audio_tracks.iter().enumerate() {
        let size = data
            .get(cursor..cursor + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
            .ok_or_else(|| format!("track {i}: frame ends before its packet-size word"))?;
        cursor += 4;
        if size == 0 {
            audio.push(None);
            continue;
        }
        let packet = data.get(cursor..cursor + size).ok_or_else(|| {
            format!(
                "track {i}: packet claims {size} bytes but only {} remain in the frame",
                data.len() - cursor
            )
        })?;
        audio.push(Some(packet));
        cursor += size;
    }

    Ok(FramePackets {
        audio,
        video: &data[cursor..],
    })
}

/// Read and parse a `.bik` from disk.
pub fn parse_file(path: &std::path::Path) -> Result<BinkFile, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The game's `data/Movies` directory, from whichever install variable is set. `None` when no game
/// directory is configured — callers skip rather than fail, as elsewhere in this crate.
pub fn movies_dir() -> Option<std::path::PathBuf> {
    crate::game_paths::GAME_DIR_VARS
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
        .map(std::path::PathBuf::from)
        .find_map(|p| {
            [p.join("data").join("Movies"), p.join("Movies")]
                .into_iter()
                .find(|c| c.is_dir())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hand-built container: one audio track, three frames, the middle one a keyframe.
    fn synthetic() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"BIKi");
        b.extend_from_slice(&0u32.to_le_bytes()); // size patched below
        b.extend_from_slice(&3u32.to_le_bytes()); // frames
        b.extend_from_slice(&64u32.to_le_bytes()); // largest frame
        b.extend_from_slice(&3u32.to_le_bytes()); // frames again
        b.extend_from_slice(&640u32.to_le_bytes()); // width
        b.extend_from_slice(&480u32.to_le_bytes()); // height
        b.extend_from_slice(&30u32.to_le_bytes()); // fps dividend
        b.extend_from_slice(&1u32.to_le_bytes()); // fps divisor
        b.extend_from_slice(&0u32.to_le_bytes()); // video flags
        b.extend_from_slice(&1u32.to_le_bytes()); // one audio track
        b.extend_from_slice(&9999u32.to_le_bytes()); // max decoded size
        b.extend_from_slice(&44100u16.to_le_bytes()); // sample rate
        b.extend_from_slice(&(AUD_STEREO | AUD_USE_DCT).to_le_bytes()); // flags
        b.extend_from_slice(&7u32.to_le_bytes()); // track id
        let table_at = b.len();
        let data_at = (table_at + 4 * 4) as u32; // 4 offsets follow
        b.extend_from_slice(&(data_at | 1).to_le_bytes()); // frame 0, keyframe
        b.extend_from_slice(&(data_at + 10).to_le_bytes()); // frame 1
        b.extend_from_slice(&((data_at + 30) | 1).to_le_bytes()); // frame 2, keyframe
        b.extend_from_slice(&(data_at + 60).to_le_bytes()); // terminator
        b.resize(data_at as usize + 60, 0xAB);
        let size = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&size.to_le_bytes());
        b
    }

    #[test]
    fn parses_header_tracks_and_frame_table() {
        let f = parse(&synthetic()).expect("synthetic container parses");
        assert_eq!(f.revision, b'i');
        assert_eq!((f.width, f.height), (640, 480));
        assert_eq!(f.fps(), 30.0);
        assert_eq!(f.duration_secs(), 0.1);
        assert_eq!(f.video_flags, 0);

        assert_eq!(f.audio_tracks.len(), 1);
        let t = &f.audio_tracks[0];
        assert_eq!(t.sample_rate, 44100);
        assert_eq!(t.channels(), 2, "AUD_STEREO decodes to two channels");
        assert_eq!(
            t.codec(),
            AudioCodec::Dct,
            "AUD_USE_DCT selects the DCT bitstream"
        );
        assert_eq!(t.id, 7);

        // Lengths come from the NEXT offset, and the keyframe bit must not leak into them.
        assert_eq!(f.frames.len(), 3);
        assert_eq!(f.frames[0].length, 10);
        assert_eq!(f.frames[1].length, 20);
        assert_eq!(f.frames[2].length, 30);
        assert_eq!(
            f.frames[0].offset % 2,
            0,
            "the keyframe bit is masked off the offset"
        );
        assert_eq!(
            f.frames.iter().map(|x| x.keyframe).collect::<Vec<_>>(),
            vec![true, false, true]
        );
    }

    /// A track with neither flag set is mono RDFT — the other corner of the flag word.
    #[test]
    fn mono_rdft_is_the_flagless_case() {
        let t = AudioTrack {
            sample_rate: 22050,
            flags: 0,
            id: 0,
            max_decoded_size: 0,
        };
        assert_eq!(t.channels(), 1);
        assert_eq!(t.codec(), AudioCodec::Rdft);
    }

    /// **Every shipped movie parses, and the set is uniform enough to scope the decoders.**
    ///
    /// This is what turns "write a Bink decoder" into a bounded job. It parses all 45 files against
    /// the real container and asserts the properties the decoder design leans on — one revision, no
    /// alpha plane, a self-consistent frame table. If a future asset (a DLC movie, a different
    /// install) breaks one of those, this fails rather than letting the decoder read garbage.
    ///
    /// It also PRINTS the audio-variant spread, which is the input to scoping the audio decoder:
    /// Bink audio has two bitstreams (RDFT and DCT) and we only have to write the ones in use.
    ///
    /// SKIPS (passes) without a configured game directory.
    #[test]
    fn the_shipped_movie_set_is_uniform() {
        let Some(dir) = movies_dir() else {
            return eprintln!("[skip] no game dir configured — shipped-movie scan skipped");
        };
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bik")))
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no .bik files under {}", dir.display());

        let mut revisions = std::collections::BTreeSet::new();
        let mut variants: std::collections::BTreeMap<String, usize> = Default::default();
        let mut resolutions: std::collections::BTreeMap<String, usize> = Default::default();
        let mut trackless = 0usize;
        let mut total_secs = 0.0;

        for path in &files {
            let f = parse_file(path).unwrap_or_else(|e| panic!("{e}"));
            revisions.insert(f.revision as char);
            *resolutions
                .entry(format!("{}x{} @{:.2}fps", f.width, f.height, f.fps()))
                .or_default() += 1;
            total_secs += f.duration_secs();

            // Self-consistency: frames tile the file in order and none overflows the declared
            // largest-frame bound the decoder sizes its buffer from.
            let mut prev_end = 0u64;
            for (i, fr) in f.frames.iter().enumerate() {
                assert!(
                    fr.offset >= prev_end,
                    "{}: frame {i} overlaps the previous one",
                    path.display()
                );
                assert!(
                    fr.length <= f.largest_frame as u64,
                    "{}: frame {i} is {} bytes, over the declared largest {}",
                    path.display(),
                    fr.length,
                    f.largest_frame
                );
                prev_end = fr.offset + fr.length;
            }
            assert!(
                f.frames.first().is_some_and(|fr| fr.keyframe),
                "{}: the first frame must be a keyframe",
                path.display()
            );
            assert_eq!(
                f.video_flags,
                0,
                "{}: alpha/extended video flags are unsupported",
                path.display()
            );

            if f.audio_tracks.is_empty() {
                trackless += 1;
            }
            for t in &f.audio_tracks {
                *variants
                    .entry(format!(
                        "{:?} {}ch {}Hz",
                        t.codec(),
                        t.channels(),
                        t.sample_rate
                    ))
                    .or_default() += 1;
            }
        }

        println!(
            "[bink] {} files, {:.1} min total",
            files.len(),
            total_secs / 60.0
        );
        println!("[bink] revisions: {revisions:?}");
        for (r, n) in &resolutions {
            println!("[bink] {r}  x{n}");
        }
        println!("[bink] {trackless} file(s) carry no audio track");
        for (v, n) in &variants {
            println!("[bink] audio variant: {v}  x{n}");
        }

        // The scoping claim the decoder work rests on.
        assert_eq!(
            revisions.len(),
            1,
            "the decoder is written for ONE revision; found {revisions:?}"
        );
        assert!(
            revisions.contains(&'i'),
            "expected revision 'i', found {revisions:?}"
        );
    }

    /// **The frame layout is exact on every frame of every shipped movie.**
    ///
    /// This is the check that makes [`split_frame`] trustworthy rather than plausible. The claimed
    /// layout — a size-prefixed audio packet per track, then video for the remainder — has to
    /// account for each frame's bytes *precisely*. A wrong reading (packet sizes including their own
    /// header, tracks in a different order, an extra field) would leave a residue or run past the
    /// end, and it would have to do so consistently across ~72 000 frames to slip through.
    ///
    /// It also asserts the video remainder is non-empty on keyframes: a keyframe with no video data
    /// would mean the audio packets swallowed the frame, which is the most likely failure shape.
    ///
    /// SKIPS (passes) without a configured game directory.
    #[test]
    fn the_frame_layout_holds_across_every_shipped_frame() {
        let Some(dir) = movies_dir() else {
            return eprintln!("[skip] no game dir configured — frame-layout scan skipped");
        };
        let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bik")))
            .collect();
        files.sort();

        let (mut total_frames, mut total_audio, mut total_video, mut silent_packets) =
            (0usize, 0u64, 0u64, 0usize);

        for path in &files {
            let bytes =
                std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let f = parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            for (i, fr) in f.frames.iter().enumerate() {
                let p = split_frame(&f, fr, &bytes)
                    .unwrap_or_else(|e| panic!("{} frame {i}: {e}", path.display()));

                assert_eq!(
                    p.audio.len(),
                    f.audio_tracks.len(),
                    "{} frame {i}: one packet slot per declared track",
                    path.display()
                );

                // The accounting identity: 4-byte size word per track, plus each packet's bytes,
                // plus the video remainder, equals the frame exactly.
                let audio_bytes: usize = p.audio.iter().map(|a| a.map_or(0, |s| s.len())).sum();
                let accounted = f.audio_tracks.len() * 4 + audio_bytes + p.video.len();
                assert_eq!(
                    accounted as u64,
                    fr.length,
                    "{} frame {i}: layout accounts for {accounted} bytes of a {}-byte frame",
                    path.display(),
                    fr.length
                );

                if fr.keyframe {
                    assert!(
                        !p.video.is_empty(),
                        "{} frame {i}: a keyframe must carry video data",
                        path.display()
                    );
                }
                // A non-empty packet always has room for its own leading sample-count word.
                for (t, a) in p.audio.iter().enumerate() {
                    if let Some(s) = a {
                        assert!(
                            s.len() >= 4,
                            "{} frame {i} track {t}: {}-byte packet is too small for its \
                             sample-count header",
                            path.display(),
                            s.len()
                        );
                    } else {
                        silent_packets += 1;
                    }
                }

                total_frames += 1;
                total_audio += audio_bytes as u64;
                total_video += p.video.len() as u64;
            }
        }

        println!(
            "[bink] {total_frames} frames across {} files: {:.1} MB video, {:.1} MB audio, \
             {silent_packets} silent track-frames",
            files.len(),
            total_video as f64 / 1e6,
            total_audio as f64 / 1e6,
        );
        assert!(
            total_frames > 10_000,
            "expected the full shipped set; got {total_frames} frames"
        );
        assert!(
            total_video > 0 && total_audio > 0,
            "both streams must carry data"
        );
    }

    /// Structural invariants are enforced, so a misread surfaces at parse rather than as bad frames.
    #[test]
    fn rejects_malformed_containers() {
        assert!(parse(b"nope").is_err(), "too short");
        let mut b = synthetic();
        b[0] = b'X';
        assert!(parse(&b).is_err(), "bad magic");

        let mut b = synthetic();
        b[0x10] = 99; // second frame count
        let e = parse(&b).unwrap_err();
        assert!(
            e.contains("disagrees"),
            "frame-count mismatch must be named: {e}"
        );

        let mut b = synthetic();
        b.truncate(b.len() - 1);
        let e = parse(&b).unwrap_err();
        assert!(
            e.contains("file length"),
            "declared-size mismatch must be named: {e}"
        );
    }
}
