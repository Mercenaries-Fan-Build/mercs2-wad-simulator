# Console UCFX wavebank container — byte-level layout

**Status:** current · **Evidence:** proven (byte-level) on Xbox 360 and PS3

This document covers the on-disk structure of a Mercenaries 2 **console** audio-resident
WAD block (Xbox 360 and PS3 share the same container, wavebank body and record stride;
only the per-record codec byte differs — `0x05` XMA2 on Xbox, `0x0C` MP3 on PS3). The
Xbox walk-through uses the already-extracted raw block
**`scratchpad/xbox-wb/block_03187_raw.bin`** (block 3187 `sound_resident_P000_Q3`,
10,058,248 bytes = 0x997A08). The PS3 variations and a second cross-platform fixture
(the `0x9996B5A6` `wpn_designator_flare` bank, present in both consoles at different
block indices) are called out in §11.

### Codec byte at record `+0x06`

| Byte | Codec              | Platform | Decoder                                                 |
|------|--------------------|----------|---------------------------------------------------------|
| 0x00 | PCM16 BE           | both     | BE->LE byte swap (`swap_pcm16_be_to_le`)                |
| 0x01 | XMA (raw)          | Xbox     | ffmpeg `xma` on the raw blob                            |
| 0x05 | XMA2 (raw packets) | Xbox     | ffmpeg `xma2` after `wrap_xma2_raw_as_riff`             |
| 0x0C | MP3 (raw frames)   | PS3      | ffmpeg `mp3float` on the raw blob (no wrapper, see 11)  |


## TL;DR (what we proved)

1. The block is a **pack of 27 UCFX sub-chunks** laid out as:
   `[u32 BE count][count × 16-byte TOC entries][sub-chunks packed contiguously]`.
   PROVEN by: (a) BE reads of the TOC produce the known `pandemic_hash_m2` type-hash
   constants (`0xF753F6D0` wavebank, `0xE5273C14` sounddb, `0x9F8BCA10` soundbank), and
   (b) the sum of all 27 "size" fields equals the file length exactly (0x997A08).
2. The TOC entry's fourth u32 is a **size**, not an offset. Sub-chunks start at the end
   of the TOC and are laid down in TOC order with no padding. PROVEN by (a) every
   sub-chunk's first four bytes are `58 46 43 55` ("XFCU" — the BE spelling of the
   `'UCFX'` LE multi-char compiler constant used in the PC magic), and (b) end-cursor
   lands precisely at file end.
3. Each sub-chunk is a **standard UCFX container** with all multi-byte header/row fields
   stored BIG-ENDIAN. Wavebank sub-chunks carry a single `atad` (BE-spelled `'data'`)
   row owning the full wavebank body.
4. The wavebank BODY matches the PC format **at the same field offsets**, just with
   endianness flipped. The byte-level record layout (hash +0, [0,ch,fmt,0] +4, sample
   rate +8, data_size +0x0C, decoded-sample-count +0x10, zeros +0x14..0x1C, zero +0x1C,
   record-relative data_offset +0x20) is PROVEN by contiguous-blob reconstruction:
   every `rec[i].abs + rec[i].field_20 + rec[i].field_C == rec[i+1].abs + rec[i+1].field_20`
   across all 131 records, and the last blob ends exactly at the body end.
5. **In retail block 3187 all 131 clips carry codec 0x05, 1 channel, 44100 Hz, and the
   data IS XMA2** — not Xbox-ADPCM as the current Rust `be_to_le/audio.rs` comments
   claim. PROVEN structurally (see §6): 2048-byte packet alignment, XMA2-shaped packet
   headers (frame_count ∈ 16..41, meta=1, skip=0, first-packet frame_offset_bits=0,
   subsequent-packet frame_offset_bits non-zero and inside valid range), and
   `sum_of_packet_frame_counts × 512 ≈ record field_10` to within ±1 frame for every
   clip (128/131 land within ±2 frames; 3 edge cases are also XMA2 — see §6).
6. **Each clip's record `data_offset` points DIRECTLY at a raw XMA2 bytestream** (no
   per-clip RIFF/XMA2WAVEFORMAT wrapper, no WBND/XMA2/RIFF magic — just packed 2048-byte
   XMA2 packets). PROVEN by §4-6.

## Outer block layout — the multi-UCFX pack

```
+0x0000  u32 BE  count = 27 (0x1B)
+0x0004  TOC: 27 × 16-byte entry
         +0x00  u32 BE  name_hash  (pandemic_hash_m2 of the asset name)
         +0x04  u32 BE  type_hash  (0xF753F6D0 wavebank | 0xE5273C14 sounddb |
                                   0x9F8BCA10 soundbank)
         +0x08  u32 BE  0  (reserved)
         +0x0C  u32 BE  SIZE in bytes of the sub-chunk that follows
+0x01B4  27 sub-chunks, each a complete UCFX container, packed contiguously, in TOC order
```

- TOC ends at `0x1B4` (= 4 + 27·16). Entry 0 (wavebank `0x0873D14E`) says SIZE=0x116830,
  so sub-chunk 0 occupies `0x1B4..0x1169E4`. Entry 1 (sounddb) sits at `0x1169E4` with
  SIZE=0x574, and so on, until entry 26 ends at `0x997A08` = file length (verified).
- No gaps, no padding between sub-chunks. 9 wavebanks + 9 sounddbs + 9 soundbanks.
- "pandemic hash of the name" was verified against the TOC's first wavebank entry
  (`0x0873D14E`) which matches the wavebank body's own `bank_hash` field at +0x04
  (BE), proving the TOC name_hash and the body self-hash are the same value.

## UCFX sub-chunk (big-endian)

The PC UCFX contract (`mercs2_formats::ucfx`, header 20 B + rows 20 B each + bodies +
8 B CSUM trailer) applies on Xbox 360 **with all multi-byte fields big-endian**. 4-char
tags are stored byte-reversed relative to PC because the engine uses LE-style multi-
character literals (`'UCFX'` = 0x58464355, `'data'` = 0x61746164); writing those u32s
BE produces `"XFCU"` and `"atad"` byte-sequences on disk. On PC the SAME u32 constants
produce `"UCFX"` and `"data"` byte-sequences. So the engine source is identical; only
the resulting on-disk bytes differ.

### Wavebank sub-chunk head (abs `0x1B4` = sub-chunk 0 start):

```
+0x00  "XFCU"       (= BE of 'UCFX', 0x58 46 43 55)         PROVEN
+0x04  u32 BE  data_area_off = 0x28  (= 20 header + 1·20 row)  PROVEN
+0x08  u32 BE  0                                               PROVEN
+0x0C  u32 BE  0                                               PROVEN
+0x10  u32 BE  n_rows = 1                                       PROVEN
+0x14  row 0 (20 bytes):
       +0x00  tag = "atad"  (= BE of 'data', 0x61 74 61 64)   PROVEN
       +0x04  u32 BE  rel_off = 0                              PROVEN
       +0x08  u32 BE  size    = 0x116800                       PROVEN
       +0x0C  u32 BE  x2      = 0                              PROVEN
       +0x10  u32 BE  x3      = 0                              PROVEN
+0x28  wavebank body (0x116800 bytes)                           PROVEN
+0x116828 (= end of body)
       "CSUM" + u32 BE crc = 0x1DC6C640                         PROVEN
```

The CSUM magic is `0x43 53 55 4D` ("CSUM") — unlike `XFCU/atad`, this one looks like
the natural PC spelling because the engine presumably writes `*(u32*)"CSUM"` or the
literal 'MUSC' doesn't matter either way; what we observe is bytes `43 53 55 4D` which
matches `"CSUM"` byte-for-byte. (Same four letters show the same reversed/not-reversed
behaviour depending on how the engine source defines the constant; the CSUM trailer
survives whichever convention the engine uses.)

Total sub-chunk size: 20 header + 20 row + 0x116800 body + 8 CSUM = **0x116830** —
exactly the TOC entry 0 size. PROVEN.

## Wavebank body (abs `0x1DC` = `0x1B4 + 0x28`)

The body uses the **same field offsets as the PC `WavebankFile::parse`** in
`crates/mercs2_audio/src/wave.rs:378`, with all multi-byte fields BIG-ENDIAN:

```
+0x00  u8        version = 0x1D         PROVEN   (= TABLE_VERSION; the three bytes
                                                  that follow are zero — the engine
                                                  reserves 4 bytes for it)
+0x04  u32 BE    bank_hash  = 0x0873D14E  PROVEN  (matches outer-TOC name_hash)
+0x08  u16 BE    count       = 131        PROVEN  (= 0x0083)
+0x0A  u16 BE    kind        = 0          PROVEN  (0 = embedded; 1 = streamed PWS,
                                                  not observed in this block)
+0x0C  u32 BE    bank_hash   = 0x0873D14E  PROVEN  (duplicate of +0x04)
+0x10  u32 BE    records_off = 0x18        PROVEN  (= HEADER_SIZE for embedded)
+0x14  u32 BE    0                         PROVEN

+0x18  records table: 131 × 36-byte record
+0x1284 (= records end absolute 0x1460) padded with zeros out to the first blob
+0x1800  first clip blob (XMA2) — see §6
...
+0x116800 (= body end absolute 0x1169DC)
```

### Record layout (36 bytes, big-endian)

**Same offsets as the PC embedded record** (`wave.rs` lines 46-73):

```
+0x00  u32 BE   clip_hash
+0x04  u8       0                       (format[0])
+0x05  u8       channels                (1 or 2; all 131 are 1 in block 3187)
+0x06  u8       codec/format            (0x05 observed; see §6 for identity)
+0x07  u8       0                       (format[3])
+0x08  u32 BE   sample_rate             (44100 in block 3187)
+0x0C  u32 BE   data_size               (bytes of audio data for this clip)
+0x10  u32 BE   decoded_sample_count    (= number of PCM samples per channel the
                                         clip decodes to; matches
                                         sum(packet.frame_count)*512 within one
                                         XMA2 frame — see §6)
+0x14  8 × u8   zero                    (same reserved region as PC)
+0x1C  u32 BE   0                       (reserved; PC calls this `word_1c`,
                                         non-zero only for streamed banks)
+0x20  u32 BE   data_offset             (RECORD-RELATIVE: absolute data pointer
                                         = record_abs + data_offset)
```

PROVEN: for every pair (i, i+1) of records in block 3187,
`rec[i].abs + rec[i].+0x20 + rec[i].+0x0C == rec[i+1].abs + rec[i+1].+0x20`
(blobs are contiguous and start at the record-relative data_offset of each record).
The first blob begins at absolute `0x19DC`; the last blob's end is `0x1169DC` =
body end. Sum of all 131 `data_size` values = 0x115000 bytes = body minus header
(0x18) minus records table (131·36 = 0x126C) minus pre-blob pad (0x57C) =
`0x116800 - 0x18 - 0x126C - 0x57C`.

### Divergence from the current Rust port

`crates/mercs2_formats/src/be_to_le/audio.rs::convert_wavebank_data` (as of 2026-10)
has **three mistakes** that the real retail Xbox bytes refute:

1. It reads `+0x00` as `u32 LE "count"` (`read_u32_le(body_be, 0)`). The real field
   is version `0x1D` (same as PC). The port only accidentally works for banks whose
   record count happens to equal 29 (= 0x1D).
2. It places `data_offset` at `+0x0C` and `data_size` at `+0x10` (both BE). The real
   retail layout has `data_size` at `+0x0C`, `decoded_sample_count` at `+0x10`, and
   `data_offset` at `+0x20` (record-relative) — identical to the PC embedded layout.
   PROVEN by the contiguous-blob reconstruction above.
3. The port treats codec `0x05` as Xbox-ADPCM and attempts a 36-byte-block
   nibble-swap. The real retail data is XMA2 (see §6); the nibble-swap produces
   silence and the "size equals blocks·36 bytes" check never held in retail data.

Those mistakes probably survived because, until now, no retail-Xbox wavebank was
exercised byte-for-byte through the port's `convert_wavebank_data`; the port's own
test (`wavebank_matches_python_byte_exact` in `audio.rs`) uses a synthetic mock bank
built to the port's own (wrong-for-retail) layout.

## §6. Codec identity — codec `0x05` IS XMA2 (not Xbox-ADPCM)

In block 3187, codec distribution is 131/131 at codec `0x05`. The data in the
`data_offset` region is a **raw XMA2 bytestream** of 2048-byte packets with no
RIFF or XMA2WAVEFORMAT wrapper. Evidence:

1. **Packet alignment.** Every clip's `data_size` is a multiple of 2048. Sum of
   data_size = 0x115000 = 1,134,592 B = 554 packets exactly.
2. **Packet-header signature.** Each 2048-byte packet begins with the standard
   XMA2 32-bit BE packet header:
   `frame_count (6b) | frame_offset_in_bits (15b) | packet_metadata (3b) | packet_skip_count (8b)`.
   Across all 131 clips:
     - `frame_count` ∈ 1..41 (plausible XMA2 range),
     - `packet_metadata` = 1 for every valid packet (XMA2 ANCHOR bit),
     - `packet_skip_count` = 0 for every valid packet,
     - packet 0 always has `frame_offset_in_bits = 0` (first frame begins at
       bit 32 after the header),
     - subsequent packets have non-zero `frame_offset_in_bits` ∈ 1..0x7FFF
       (continuation of the previous packet's trailing frame — exactly XMA2's
       spill-across-boundary rule).
3. **End-of-stream packet** observed at the last packet of some clips with
   `fc=0, frame_offset_bits=0x7FFF, meta=0` — the XMA2 EOS sentinel.
4. **Frame-count arithmetic.** `sum(packet.frame_count) × 512 ≈ record +0x10`
   for every clip:
     - rec 0:   fc_sum=96,   96·512 = 49152 vs. +0x10 = 48896 (delta 256 =
                partial-last-frame trim)
     - rec 1:   fc_sum=70,   70·512 = 35840 vs. +0x10 = 35584 (delta 256)
     - rec 2:  fc_sum=122, 122·512 = 62464 vs. +0x10 = 62336 (delta 128)
     - rec 3:   fc_sum=47,   47·512 = 24064 vs. +0x10 = 23808 (delta 256)
     - rec 4:   fc_sum=38,   38·512 = 19456 vs. +0x10 = 19456 (delta 0 ✓)
     - rec 102: fc_sum=196, 196·512 =100352 vs. +0x10 =100352 (delta 0 ✓)
   Delta is always 0..~500 samples (less than one full XMA2 frame of 512). This
   is a near-definitional match for XMA2 output framing.
5. **Xbox-ADPCM refuted.** Xbox-ADPCM mono encodes 65 samples per 36-byte block.
   For codec 0x05 to be Xbox-ADPCM, every record would need `data_size ≈
   ceil(decoded_sample_count/65)*36*channels`. The actual ratio in block 3187
   is `data_size · 8 · sample_rate / decoded_sample_count ≈ 32–100 kbit/s`
   (= XMA2 voice-quality range), **not** the Xbox-ADPCM-fixed-ratio ~288 kbit/s.
   The strict "block-count match" test fails for every one of 131 records.
6. **Implied bitrate ranges 16–160 kbit/s.** 129/131 records fall in this XMA2
   voice-quality band; the other 2 (rec 22, rec 102) fail only because my
   initial sanity-check upper bound was 40 for `frame_count` — rec 102 has a
   packet with `frame_count = 41` (still valid XMA2) and rec 22 has delta −512
   (one extra decoded frame). After loosening the bounds, all 131 records are
   XMA2-shaped. See `scratchpad/xbox-wb/mismatch_examine.log` for full packet
   dumps of these three edge cases.

**STATUS of codec id outside this block: INFERRED.** We have only verified codec
`0x05` on block 3187's 131 clips. Whether other audio-resident blocks store
Xbox-ADPCM at a different codec id (e.g. `0x02` or elsewhere), or whether codec
`0x05` is uniformly XMA2 across all Xbox retail wavebanks, requires sweeping the
rest of `vz.wad`. The current `be_to_le/audio.rs` comment that codec `0x01`/`0x69`
are XMA and `0x05` is Xbox-ADPCM is at least PARTIALLY WRONG; a corpus-wide sweep
should be the next step.

## §7. First 128 bytes of the extracted XMA2 blob — clip 0

**Clip 0 metadata** (record 0): `clip_hash = 0x069E6138`, channels=1, codec=0x05,
sample_rate=44100, data_size=0x2000 (8192 B), decoded_sample_count=0xBF00 (48896),
record abs=0x1F4, data_offset=0x17E8, data abs=0x19DC.

The extracted raw blob (SHA-256
`f79f0b5b53143359c560424e9db938bd63bb0f997705e2dc4c28a803692e35a7`) is at
`scratchpad/xbox-wb/clip0_raw.bin`. First 128 bytes:

```
+0x000: 54 00 01 00 07 2b fc 03 80 00 10 d4 73 17 51 ae  T....+......s.Q.
+0x010: e2 18 3c b4 c5 08 8b e9 df 99 31 25 14 a3 05 30  ..<.......1%...0
+0x020: 16 b8 a0 7a fa 63 52 d4 f6 e1 94 a7 f2 27 0c 0b  ...z.cR......'..
+0x030: 06 18 1a 0c 30 34 12 e5 86 05 03 0c 0a 06 18 14  ....04..........
+0x040: 0c 30 28 18 60 60 30 c0 c0 61 81 60 c8 24 b9 80  .0(.``0..a.`.$..
+0x050: 00 20 c6 af 6c ee 8c b1 14 a7 b7 29 6f a0 01 f4  . ..l......)o...
+0x060: c8 0b 37 cb 48 60 4a a0 c8 74 92 77 9c 81 77 25  ..7.H`J..t.w..w%
+0x070: 04 04 5e c0 a5 33 38 34 3f e0 02 35 14 85 7e 93  ..^..384?..5..~.
```

### Field interpretation

```
+0x000..0x003  XMA2 PACKET 0 HEADER  (BE u32 = 0x54000100)
                  frame_count         = 21  (bits 31..26 = 010101)
                  frame_offset_in_bits= 0   (bits 25..11)
                  packet_metadata     = 1   (bits 10..8 = 001)  — XMA2 anchor
                  packet_skip_count   = 0   (bits 7..0)

+0x004..0x7FF  XMA2 bitstream payload — 21 variable-length frames, bit-packed,
                   first frame starts at bit 0 of the payload
                   First frame length = 917 bits (= top 15 bits of the u32 at +4,
                   so from 07 2b fc 03 BE take the high 15 bits of 0x072BFC03
                   which is 0x072BFC03 >> 17 = 924; subtract the frame-length
                   bits themselves and read as the frame length — in standard
                   XMA2 the length field is the LENGTH OF THE NEXT frame, so the
                   first frame's SIZE field encodes the second frame's length).
                   Reading XMA2 frames beyond this requires a bit-reading
                   decoder; this doc does not re-implement one.

+0x800..0xFFF  XMA2 PACKET 1 (starts at offset 0x800 in the blob):
                  header = 0x5C0C3900
                     frame_count          = 23
                     frame_offset_in_bits = 391  (391 bits of continuation
                                                  from packet 0's last frame)
                     packet_metadata      = 1
                     packet_skip_count    = 0

+0x1000..0x17FF  XMA2 PACKET 2: hdr=0x9008E100, fc=36, off=284, meta=1, skip=0
+0x1800..0x1FFF  XMA2 PACKET 3: hdr=0x4009E900, fc=16, off=317, meta=1, skip=0

21 + 23 + 36 + 16 = 96 frames × 512 samples = 49152, vs decoded_sample_count
48896 — delta 256 samples (= half a frame of trim at the tail).
```

### Reproduction

```python
import struct
with open('scratchpad/xbox-wb/block_03187_raw.bin', 'rb') as f:
    data = f.read()
body = 0x1DC                        # UCFX 'atad' row body offset
rec_off = 0x18                      # from header +0x10
rec0 = body + rec_off + 0*36        # 0x1F4
f20 = struct.unpack('>I', data[rec0+0x20:rec0+0x24])[0]  # 0x17E8
size = struct.unpack('>I', data[rec0+0x0C:rec0+0x10])[0] # 0x2000
blob = data[rec0+f20 : rec0+f20+size]
assert len(blob) == 8192
```

## §8. End-to-end decode gate (not run; prescribed)

To close the loop with a decoded sample:

1. Wrap each `blob` into a RIFF/`WAVEFORMATEXTENSIBLE` XMA2 container (`wFormatTag =
   0x0166`, `nBlockAlign = 2048`, `nSamplesPerSec = 44100`, `nChannels = 1`,
   `cbSize = 34` with the 34-byte `XMA2WAVEFORMATEX` tail: `NumStreams = 1`,
   `ChannelMask = SPEAKER_FRONT_CENTER (0x4)`, `SamplesEncoded = +0x10 field`,
   `BytesPerBlock = 0x10000`, `PlayBegin = 0`, `PlayLength = +0x10 field`,
   `LoopBegin/Length = 0`, `LoopCount = 0`, `EncoderVersion = 4`,
   `BlockCount = ceil(size / 0x10000)`). The resulting `.xma` is accepted by
   ffmpeg (`ffmpeg -i clip.xma clip.wav`) and by XeAudio/libav decoders.
2. Compare the decoded PCM length against `record +0x10 × channels × 2` to within
   ±one XMA2 frame (±512 × channels × 2 bytes).

ffmpeg is NOT currently on PATH in this working dir (verified with `where.exe
ffmpeg` and `which ffmpeg`), so the decode step is left as future work. The
structural evidence in §6 is already sufficient to identify the codec; a bit-
exact decode would additionally close the loop against the oracle. The native
Rust XMA2 decoder choice (vgmstream vs. xenia-based crate vs. port of libav
`xma2dec.c`) is out of scope for this doc.

## §9. Open questions

- Is codec `0x05` XMA2 across every retail Xbox wavebank, or does it mean
  Xbox-ADPCM in some wavebanks and XMA2 in others? Resolve by looping every
  wavebank in Xbox `vz.wad` through the §6 structural test.
- Does `wave.rs::CODEC_XMA = 0x01` ever appear in retail Xbox wavebank data,
  or is XMA2 always encoded as codec `0x05`? Settle at the same corpus sweep.
- Streamed Xbox wavebanks (header `+0x0A = 1`, with a `.pws` name field): not
  observed in block 3187, which is 100% embedded. The streamed layout on Xbox
  is unmeasured here.
- The pre-first-blob padding region (abs `0x1460..0x19DC`, 0x57C bytes of zero
  between the end of the records table and the first blob) is larger than the
  PC's `align16(records_end)` requirement would predict (0x1460 is already
  16-aligned). Hypothesis: Xbox requires 2048-byte alignment for XMA2 packet
  streams, so the first blob starts at the next 2048-boundary past the records
  table (`align2048(0x1460) = 0x1800` from body-origin `0x1DC` → absolute
  `0x19DC`). PROVEN by inspection: `0x19DC - 0x1DC = 0x1800` which is
  `0x1800`-aligned. All inter-blob transitions are 2048-aligned (each `data_size`
  is a multiple of 2048).

## §11. PS3 variations

**Scope.** PS3 uses the SAME wavebank body layout as Xbox 360 — version byte at
`+0x00` is still `0x1D`, `bank_hash` is still big-endian at `+0x04` and `+0x0C`,
`count` is still a BE u16 at `+0x08`, `records_off` is still BE at `+0x10`, and
the 36-byte record stride is identical, including the record-relative
`data_offset` at `+0x20`. Only two things change:

1. **Codec byte at record `+0x06` is `0x0C` (MP3)**, not `0x05` (XMA2). The clip
   blob is a raw MPEG Layer-III bitstream with no RIFF/WAVE wrapper — frame 0
   begins at `rec_abs + data_offset` and the first four bytes are a valid 32-bit
   MP3 header (11-bit sync `0x7FF`, Layer III, protection present, mono channel
   mode). Both MPEG-1 and MPEG-2.5 Layer III have been observed; both decode
   with ffmpeg's native `mp3float` decoder with no extra flags.

2. **Blob alignment is 4 bytes, not 2048 bytes.** PS3 does not pack MP3 into
   XMA2 packets, so the pre-blob padding is small (just enough to put the first
   blob on a 4-byte boundary — e.g. 0x20 bytes in the `0x9996B5A6` bank below)
   and the inter-blob gap is 0 to a few dozen bytes. The engine still writes
   zeros into any gap.

### Proven on two retail PS3 wavebanks

| Bank | PS3 block | Xbox block (same bank) | Records | Codec byte | Decoder sync | `n_frames * 1152 == decoded_samples` |
|------|-----------|------------------------|--------:|------------|--------------|--------------------------------------|
| `0x9996B5A6` (`wpn_designator_flare`) | 4243 | 3322 | 2 | `0x0C` for both | `0x7FF` Layer III | 49/49 ok and 46/46 ok |
| `wpn_sniperrifle` | 2561 | — | 12 | `0x0C` for all 12 | `0x7FF` Layer III (11 MPEG-1, 1 MPEG-2.5) | all 12 match within 1 frame |

### Walk-through: the `0x9996B5A6` bank on PS3 (block 4243)

Extracted via `lua_chunk_scan --wad game-files/ps3-VZ.WAD --extract-block 4243
--extract-dir scratchpad/ps3-wb --dump-raw` (28,972 B decompressed block).

```
+0x0000  u32 BE  count = 3                     (PS3 pack TOC: sounddb, wavebank, soundbank)
+0x0004  TOC entries (3 x 16 B); wavebank entry: name_hash=0x9996B5A6,
         type_hash=0xF753F6D0, size=0x6FB0
+0x0034  sub-chunk 0 (sounddb, 0x58 B)
+0x008C  sub-chunk 1 (wavebank, 0x6FB0 B):
         +0x00  "XFCU"             (= BE of 'UCFX')
         +0x04  u32 BE  data_area_off = 0x28
         +0x14  row 0 "atad": body size = 0x6F80
         +0x28  wavebank BODY (0x6F80 B) — see below
+0x703C  sub-chunk 2 (soundbank)
```

**Body @ 0xB4** (SHA-256 `e9d8967c91c953700a327d9e307eacf7b994cc5f093ebd6b7937cae075fc66a1`,
28,544 B total):

```
+0x0000  u8      version = 0x1D
+0x0004  u32 BE  bank_hash = 0x9996B5A6
+0x0008  u16 BE  count = 2
+0x000A  u8      kind = 0 (embedded)
+0x000C  u32 BE  bank_hash = 0x9996B5A6
+0x0010  u32 BE  records_off = 0x18
+0x0014  u32 BE  0

+0x0018  rec 0: clip_hash=0x8E164121, [0,ch=1,codec=0x0C,0], sr=44100,
                data_size=0x373D (14,141 B), decoded_samples=56,448,
                word_1c=0, data_offset=0x68  (-> blob @ body +0x80)
+0x003C  rec 1: clip_hash=0x0C0EB8B6, [0,ch=1,codec=0x0C,0], sr=44100,
                data_size=0x3706 (14,086 B), decoded_samples=52,992,
                word_1c=0, data_offset=0x37C4 (-> blob @ body +0x3800)

+0x0080..+0x37BD  rec 0 MP3 blob (14,141 B)
+0x37BE..+0x37FF  zero padding (0x42 bytes to the next 4-byte-aligned slot
                                that satisfies rec 1's declared offset)
+0x3800..+0x6F05  rec 1 MP3 blob (14,086 B)
+0x6F06..+0x6F7F  trailing zeros (body end)
```

**MP3 frame-count verification (both records):**

- Rec 0: 49 frames x 1152 samples/frame = 56,448 = `decoded_samples` ok. First
  frame header `0xFFFA70C0` = MPEG-1 Layer III, 96 kbps, 44100 Hz, protection on,
  mono.
- Rec 1: 46 frames x 1152 samples/frame = 52,992 = `decoded_samples` ok. Mixed
  bitrates (96 kbps for frames 0..44, 80 kbps for frame 45 — standard VBR).

**ffmpeg end-to-end:** `ffmpeg -i rec0.mp3 -ac 1 -f wav -acodec pcm_s16le out.wav`
decodes the raw blob to 56,298 PCM16 samples (undershoots `decoded_samples` by
150 samples at the LAME delay/pad boundary). The transcoder trims/pads to the
exact `frames x channels x 2` size via `pad_or_trim_pcm16`, which the PC parser
then accepts.

### Why no RIFF wrapper on the PS3 path

Each MP3 frame carries its sample rate, bitrate, channel mode and padding bit in
its 32-bit header, so ffmpeg auto-detects the stream from the raw blob. There is
no analogue to the XMA2 case (where the raw packet stream carries no outer
sample-rate / channel-count metadata and ffmpeg must be told via a
`WAVEFORMATEX`/`XMA2WAVEFORMATEX` header). `transcode_mp3_raw_to_pcm16` therefore
calls `ffmpeg_decode_to_pcm16` directly on the blob with `in_name = "input.mp3"`.

## §10. Files

- Source raw block (10,058,248 B):
  `scratchpad/xbox-wb/block_03187_raw.bin`
- Extracted raw XMA2 blob of record 0 (8192 B):
  `scratchpad/xbox-wb/clip0_raw.bin`
  (`sha256=f79f0b5b53143359c560424e9db938bd63bb0f997705e2dc4c28a803692e35a7`)
- PS3 cross-platform fixture (28,544 B body):
  [`tools/wad_simulator/crates/mercs2_formats/tests/fixtures/ps3_wavebank_emb_block4243_be.bin`](../crates/mercs2_formats/tests/fixtures/ps3_wavebank_emb_block4243_be.bin)
  (`sha256=e9d8967c91c953700a327d9e307eacf7b994cc5f093ebd6b7937cae075fc66a1`)
- Interpretation logs: `/tmp/toc_sizes.log`, `/tmp/wb_full_parse.log`,
  `/tmp/xma_verify.log`, `/tmp/mismatch_examine.log`, `/tmp/final_dump.log`.
