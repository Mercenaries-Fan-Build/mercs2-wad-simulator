//! Microsoft LZX decompressor (the variant used by Xbox 360 XEX / WIM).
//!
//! Decodes a single continuous stream (no CAB chunk framing: the XEX block layer has already
//! concatenated the chunks), following libmspack's `lzxd.c`:
//!
//! - Output is produced in 32,768-byte frames. A frame never ends inside a symbol, and after each
//!   frame the input bitstream is realigned to the next 16-bit word.
//! - An UNCOMPRESSED block header aligns to the next 16-bit word, skipping a whole word when the
//!   bitstream is already aligned, then reads R0/R1/R2 as raw little-endian bytes. An odd-length
//!   uncompressed block is followed by one padding byte.
//! - Intel E8 call translation applies only to frames that end after the stream has started
//!   using it: a VERBATIM/ALIGNED main tree that gives literal `0xE8` a length, or any
//!   UNCOMPRESSED block.
//!
//! Every malformed input is an `Err`: an unsupported window size, an incomplete or
//! over-subscribed Huffman table, a match that runs past its block or frame, a match that reaches
//! before the start of the output, or a truncated stream.

const FRAME_SIZE: usize = 32_768;

const NUM_PRIMARY_LENGTHS: u32 = 7;
const MIN_MATCH: u32 = 2;
const NUM_CHARS: usize = 256;
const PRETREE_NUM: usize = 20;
const ALIGNED_NUM: usize = 8;
const LENGTH_NUM: usize = 249;

const VERBATIM: u32 = 1;
const ALIGNED: u32 = 2;
const UNCOMPRESSED: u32 = 3;

/// Position slots per window size, for window bits 15..=21 (libmspack `position_slots`).
fn position_slots(window_bits: u32) -> Option<usize> {
    match window_bits {
        15 => Some(30),
        16 => Some(32),
        17 => Some(34),
        18 => Some(36),
        19 => Some(38),
        20 => Some(42),
        21 => Some(50),
        _ => None,
    }
}

/// Extra offset bits for a position slot.
fn extra_bits(slot: usize) -> u32 {
    if slot >= 36 {
        17
    } else {
        // 0,0,0,0,1,1,2,2,...,16,16
        (slot.saturating_sub(2) / 2) as u32
    }
}

/// Base match offset (before the `- 2` adjustment) for every position slot up to `n`.
fn position_bases(n: usize) -> Vec<u32> {
    let mut base = Vec::with_capacity(n);
    let mut acc = 0u32;
    for slot in 0..n {
        base.push(acc);
        acc += 1u32 << extra_bits(slot);
    }
    base
}

/// 16-bit little-endian words; bits served MSB-first within each word.
struct BitReader<'a> {
    d: &'a [u8],
    pos: usize,
    buf: u64,
    n: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { d: data, pos: 0, buf: 0, n: 0 }
    }

    /// Load 16-bit words until at least `need` bits are buffered. Reading at or past the end of
    /// the input is an error, except for the single padding word libmspack supplies at EOF.
    fn ensure(&mut self, need: u32) -> Result<(), String> {
        while self.n < need {
            let p = self.pos;
            let l = self.d.len();
            let w: u64 = if p + 1 < l {
                (self.d[p] as u64) | ((self.d[p + 1] as u64) << 8)
            } else if p < l {
                self.d[p] as u64
            } else if p < l + 2 {
                0
            } else {
                return Err(format!("LZX stream truncated at byte {p}"));
            };
            self.pos = p + 2;
            self.buf = (self.buf << 16) | w;
            self.n += 16;
        }
        Ok(())
    }

    fn bits(&mut self, k: u32) -> Result<u32, String> {
        if k == 0 {
            return Ok(0);
        }
        self.ensure(k)?;
        self.n -= k;
        let v = ((self.buf >> self.n) & ((1u64 << k) - 1)) as u32;
        self.buf &= (1u64 << self.n) - 1;
        Ok(v)
    }

    /// Drop the buffered bits that precede the next 16-bit word boundary of the input.
    fn align_frame(&mut self) -> Result<(), String> {
        if self.n > 0 {
            self.ensure(16)?;
            let drop = self.n & 15;
            self.bits(drop)?;
        }
        Ok(())
    }

    /// UNCOMPRESSED block header alignment: 1 to 16 bits are dropped, then the reader is
    /// byte-addressed at `pos`.
    fn align_uncompressed(&mut self) -> Result<(), String> {
        if self.n == 0 {
            self.ensure(16)?;
        }
        self.n = 0;
        self.buf = 0;
        Ok(())
    }

    fn take_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        let end = self.pos + len;
        if end > self.d.len() {
            return Err(format!(
                "LZX stream truncated: {len} raw bytes at {} exceed the {}-byte input",
                self.pos,
                self.d.len()
            ));
        }
        let s = &self.d[self.pos..end];
        self.pos = end;
        Ok(s)
    }
}

/// Canonical Huffman decoder: symbols sorted by (length, symbol) with per-length first codes.
struct Huffman {
    /// `count[l]` = number of codes of length `l`.
    count: Vec<u32>,
    /// `first[l]` = first canonical code of length `l`.
    first: Vec<u32>,
    /// `offset[l]` = index in `symbols` of the first symbol of length `l`.
    offset: Vec<usize>,
    symbols: Vec<u32>,
    max_len: u32,
}

impl Huffman {
    /// Build from code lengths. The table must be complete, or every length zero (an empty tree,
    /// which is an error only when a symbol is read from it).
    fn build(lengths: &[u32], what: &str) -> Result<Huffman, String> {
        let max_len = lengths.iter().copied().max().unwrap_or(0);
        let mut count = vec![0u32; max_len as usize + 1];
        for &l in lengths {
            if l > 0 {
                count[l as usize] += 1;
            }
        }
        if max_len > 0 {
            // Kraft sum in units of 2^-max_len must equal exactly 2^max_len.
            let mut kraft: u64 = 0;
            for l in 1..=max_len as usize {
                kraft += (count[l] as u64) << (max_len as usize - l);
            }
            if kraft != 1u64 << max_len {
                return Err(format!(
                    "LZX {what} Huffman table is {} (Kraft sum {kraft} of {})",
                    if kraft > 1u64 << max_len { "over-subscribed" } else { "incomplete" },
                    1u64 << max_len
                ));
            }
        }
        let mut first = vec![0u32; max_len as usize + 1];
        let mut offset = vec![0usize; max_len as usize + 1];
        let mut code = 0u32;
        let mut idx = 0usize;
        for l in 1..=max_len as usize {
            code <<= 1;
            first[l] = code;
            offset[l] = idx;
            code += count[l];
            idx += count[l] as usize;
        }
        let mut symbols = vec![0u32; idx];
        let mut next = offset.clone();
        for (sym, &l) in lengths.iter().enumerate() {
            if l > 0 {
                symbols[next[l as usize]] = sym as u32;
                next[l as usize] += 1;
            }
        }
        Ok(Huffman { count, first, offset, symbols, max_len })
    }

    fn read(&self, br: &mut BitReader<'_>, what: &str) -> Result<u32, String> {
        if self.max_len == 0 {
            return Err(format!("LZX {what} symbol needed but the tree is empty"));
        }
        let mut code = 0u32;
        for l in 1..=self.max_len as usize {
            code = (code << 1) | br.bits(1)?;
            let rel = code.wrapping_sub(self.first[l]);
            if rel < self.count[l] {
                return Ok(self.symbols[self.offset[l] + rel as usize]);
            }
        }
        Err(format!("LZX {what}: invalid Huffman code"))
    }
}

/// Read delta-coded lengths `lens[first..last]` through a fresh pretree.
fn read_lengths(lens: &mut [u32], first: usize, last: usize, br: &mut BitReader<'_>) -> Result<(), String> {
    let mut pre = [0u32; PRETREE_NUM];
    for slot in pre.iter_mut() {
        *slot = br.bits(4)?;
    }
    let pretree = Huffman::build(&pre, "pretree")?;
    let mut i = first;
    let overrun = |i: usize, run: u32| -> Result<(), String> {
        if i + run as usize > last {
            Err(format!("LZX length run of {run} at {i} overruns the table end {last}"))
        } else {
            Ok(())
        }
    };
    while i < last {
        let sym = pretree.read(br, "pretree")?;
        match sym {
            17 => {
                let run = br.bits(4)? + 4;
                overrun(i, run)?;
                for _ in 0..run {
                    lens[i] = 0;
                    i += 1;
                }
            }
            18 => {
                let run = br.bits(5)? + 20;
                overrun(i, run)?;
                for _ in 0..run {
                    lens[i] = 0;
                    i += 1;
                }
            }
            19 => {
                let run = br.bits(1)? + 4;
                overrun(i, run)?;
                let sym2 = pretree.read(br, "pretree")?;
                let val = (lens[i] as i32 - sym2 as i32).rem_euclid(17) as u32;
                for _ in 0..run {
                    lens[i] = val;
                    i += 1;
                }
            }
            _ => {
                lens[i] = (lens[i] as i32 - sym as i32).rem_euclid(17) as u32;
                i += 1;
            }
        }
    }
    Ok(())
}

/// Intel E8 call translation over one frame, in place (libmspack's per-frame decode).
fn e8_translate(frame: &mut [u8], frame_start: usize, filesize: i32) {
    if frame.len() <= 10 {
        return;
    }
    let end = frame.len() - 10;
    let mut i = 0usize;
    while i < end {
        if frame[i] != 0xE8 {
            i += 1;
            continue;
        }
        let curpos = (frame_start + i) as i64;
        let abs_off = i32::from_le_bytes(frame[i + 1..i + 5].try_into().unwrap()) as i64;
        if abs_off >= -curpos && abs_off < filesize as i64 {
            let rel = if abs_off >= 0 { abs_off - curpos } else { abs_off + filesize as i64 };
            frame[i + 1..i + 5].copy_from_slice(&(rel as u32).to_le_bytes());
        }
        i += 5;
    }
}

/// Decompress an LZX stream of `window_size` (a power of two, 2^15..=2^21) to exactly
/// `out_size` bytes.
pub fn lzx_decompress(data: &[u8], window_size: u32, out_size: usize) -> Result<Vec<u8>, String> {
    if !window_size.is_power_of_two() {
        return Err(format!("LZX window size 0x{window_size:x} is not a power of two"));
    }
    let window_bits = window_size.ilog2();
    let posn_slots = position_slots(window_bits)
        .ok_or_else(|| format!("LZX window size 2^{window_bits} is outside the supported 2^15..=2^21"))?;
    let pos_base = position_bases(posn_slots);
    let main_elements = NUM_CHARS + 8 * posn_slots;

    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::with_capacity(out_size);
    if out_size == 0 {
        return Ok(out);
    }

    let (mut r0, mut r1, mut r2) = (1u32, 1u32, 1u32);
    let mut main_len = vec![0u32; main_elements];
    let mut length_len = vec![0u32; LENGTH_NUM];
    let mut main_tree: Option<Huffman> = None;
    let mut length_tree: Option<Huffman> = None;
    let mut aligned_tree: Option<Huffman> = None;

    let e8 = br.bits(1)?;
    let intel_filesize: i32 = if e8 != 0 {
        let hi = br.bits(16)?;
        let lo = br.bits(16)?;
        ((hi << 16) | lo) as i32
    } else {
        0
    };
    let mut intel_started = false;
    // Index of the first frame that ends with E8 translation started; frames from here on (and
    // below frame 32768) are translated once decoding is complete.
    let mut e8_from_frame: Option<usize> = None;

    let mut block_type = 0u32;
    let mut block_length = 0usize;
    let mut block_remaining = 0usize;

    let mut frame_index = 0usize;
    let mut frame_start = 0usize;
    while frame_start < out_size {
        let frame_end = frame_start + FRAME_SIZE.min(out_size - frame_start);

        while out.len() < frame_end {
            if block_remaining == 0 {
                if block_type == UNCOMPRESSED && block_length & 1 != 0 {
                    br.take_bytes(1)?;
                }
                block_type = br.bits(3)?;
                let hi = br.bits(16)?;
                let lo = br.bits(8)?;
                block_length = ((hi << 8) | lo) as usize;
                block_remaining = block_length;
                match block_type {
                    VERBATIM | ALIGNED => {
                        aligned_tree = if block_type == ALIGNED {
                            let mut alen = [0u32; ALIGNED_NUM];
                            for slot in alen.iter_mut() {
                                *slot = br.bits(3)?;
                            }
                            Some(Huffman::build(&alen, "aligned")?)
                        } else {
                            None
                        };
                        read_lengths(&mut main_len, 0, NUM_CHARS, &mut br)?;
                        read_lengths(&mut main_len, NUM_CHARS, main_elements, &mut br)?;
                        main_tree = Some(Huffman::build(&main_len, "main")?);
                        if main_len[0xE8] != 0 {
                            intel_started = true;
                        }
                        read_lengths(&mut length_len, 0, LENGTH_NUM, &mut br)?;
                        length_tree = Some(Huffman::build(&length_len, "length")?);
                    }
                    UNCOMPRESSED => {
                        intel_started = true;
                        br.align_uncompressed()?;
                        let r = br.take_bytes(12)?;
                        r0 = u32::from_le_bytes(r[0..4].try_into().unwrap());
                        r1 = u32::from_le_bytes(r[4..8].try_into().unwrap());
                        r2 = u32::from_le_bytes(r[8..12].try_into().unwrap());
                    }
                    other => return Err(format!("LZX bad block type {other} at output {}", out.len())),
                }
            }

            let this_run = block_remaining.min(frame_end - out.len());
            let run_start = out.len();
            match block_type {
                VERBATIM | ALIGNED => {
                    let main = main_tree.as_ref().expect("main tree built with the block header");
                    let lengths = length_tree.as_ref().expect("length tree built with the block header");
                    while out.len() - run_start < this_run {
                        let sym = main.read(&mut br, "main")?;
                        if (sym as usize) < NUM_CHARS {
                            out.push(sym as u8);
                            continue;
                        }
                        let element = sym - NUM_CHARS as u32;
                        let mut match_len = element & NUM_PRIMARY_LENGTHS;
                        if match_len == NUM_PRIMARY_LENGTHS {
                            match_len += lengths.read(&mut br, "length")?;
                        }
                        match_len += MIN_MATCH;

                        let slot = (element >> 3) as usize;
                        let match_off = match slot {
                            0 => r0,
                            1 => {
                                let o = r1;
                                r1 = r0;
                                r0 = o;
                                o
                            }
                            2 => {
                                let o = r2;
                                r2 = r0;
                                r0 = o;
                                o
                            }
                            _ => {
                                let extra = extra_bits(slot);
                                let mut off = pos_base[slot] - 2;
                                match (&aligned_tree, extra) {
                                    (Some(aligned), e) if e >= 3 => {
                                        if e > 3 {
                                            off += br.bits(e - 3)? << 3;
                                        }
                                        off += aligned.read(&mut br, "aligned")?;
                                    }
                                    (_, 0) => {}
                                    (_, e) => off += br.bits(e)?,
                                }
                                r2 = r1;
                                r1 = r0;
                                r0 = off;
                                off
                            }
                        };

                        let produced = out.len() - run_start;
                        if produced + match_len as usize > this_run {
                            return Err(format!(
                                "LZX match of {match_len} at output {} runs past its {} (block remaining {}, frame ends at {frame_end})",
                                out.len(),
                                if this_run == block_remaining { "block" } else { "frame" },
                                block_remaining - produced
                            ));
                        }
                        let off = match_off as usize;
                        if off == 0 || off > out.len() {
                            return Err(format!("LZX match offset {off} at output {} reaches before the start", out.len()));
                        }
                        let mut src = out.len() - off;
                        for _ in 0..match_len {
                            let b = out[src];
                            out.push(b);
                            src += 1;
                        }
                    }
                }
                UNCOMPRESSED => {
                    let raw = br.take_bytes(this_run)?;
                    out.extend_from_slice(raw);
                }
                _ => unreachable!("block type validated with the header"),
            }
            block_remaining -= this_run;
        }

        br.align_frame()?;
        if intel_started && e8_from_frame.is_none() {
            e8_from_frame = Some(frame_index);
        }
        frame_index += 1;
        frame_start = frame_end;
    }

    if intel_filesize != 0 {
        if let Some(from) = e8_from_frame {
            let frames = frame_index.min(32_768);
            for f in from..frames {
                let start = f * FRAME_SIZE;
                let end = (start + FRAME_SIZE).min(out.len());
                e8_translate(&mut out[start..end], start, intel_filesize);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bit writer matching the reader: MSB-first bits packed into 16-bit little-endian words.
    struct BitWriter {
        out: Vec<u8>,
        acc: u32,
        n: u32,
    }

    impl BitWriter {
        fn new() -> Self {
            BitWriter { out: Vec::new(), acc: 0, n: 0 }
        }
        fn put(&mut self, v: u32, k: u32) {
            for i in (0..k).rev() {
                self.acc = (self.acc << 1) | ((v >> i) & 1);
                self.n += 1;
                if self.n == 16 {
                    self.out.extend_from_slice(&(self.acc as u16).to_le_bytes());
                    self.acc = 0;
                    self.n = 0;
                }
            }
        }
        /// Pad to the next word (no-op when aligned).
        fn flush(&mut self) {
            if self.n > 0 {
                self.put(0, 16 - self.n);
            }
        }
        fn raw(&mut self, b: &[u8]) {
            assert_eq!(self.n, 0, "raw bytes must start word-aligned");
            self.out.extend_from_slice(b);
        }
    }

    /// One UNCOMPRESSED block header in the encoder's layout: type + length, then 1..=16 bits of
    /// alignment, then R0/R1/R2.
    fn uncompressed_header(w: &mut BitWriter, len: u32) {
        w.put(UNCOMPRESSED, 3);
        w.put(len >> 8, 16);
        w.put(len & 0xFF, 8);
        if w.n == 0 {
            w.put(0, 16);
        } else {
            w.flush();
        }
        w.raw(&[1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn non_power_of_two_window_is_rejected() {
        assert!(lzx_decompress(&[0u8; 32], 0, 1).is_err());
        assert!(lzx_decompress(&[0u8; 32], 0x9000, 1).is_err());
    }

    #[test]
    fn window_outside_supported_range_is_rejected() {
        let err = lzx_decompress(&[0u8; 32], 1 << 14, 1).unwrap_err();
        assert!(err.contains("2^14"), "{err}");
    }

    #[test]
    fn zero_output_is_empty_result() {
        let out = lzx_decompress(&[0u8; 4], 0x8000, 0).expect("empty target");
        assert!(out.is_empty());
    }

    #[test]
    fn uncompressed_out_of_bounds_errors() {
        // UNCOMPRESSED block claiming 0xFFFFFF bytes over a tiny input.
        let mut w = BitWriter::new();
        w.put(0, 1);
        uncompressed_header(&mut w, 0xFF_FFFF);
        w.raw(&[0u8; 4]);
        assert!(lzx_decompress(&w.out, 0x8000, 0x1000).is_err());
    }

    #[test]
    fn uncompressed_alignment_skips_a_whole_word_when_aligned() {
        let data = [0xAAu8, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        // Exactly one word consumed: nothing buffered, so alignment skips the next whole word.
        let mut br = BitReader::new(&data);
        br.bits(16).unwrap();
        br.align_uncompressed().unwrap();
        assert_eq!(br.pos, 4);
        // A partial word buffered: alignment drops only those bits.
        let mut br = BitReader::new(&data);
        br.bits(5).unwrap();
        br.align_uncompressed().unwrap();
        assert_eq!(br.pos, 2);
    }

    #[test]
    fn frame_alignment_drops_only_the_partial_word() {
        let data = [0x34u8, 0x12, 0x78, 0x56];
        let mut br = BitReader::new(&data);
        br.bits(3).unwrap();
        br.align_frame().unwrap();
        assert_eq!(br.bits(16).unwrap(), 0x5678);
        // Already aligned: nothing is dropped.
        let mut br = BitReader::new(&data);
        br.bits(16).unwrap();
        br.align_frame().unwrap();
        assert_eq!(br.bits(16).unwrap(), 0x5678);
    }

    #[test]
    fn uncompressed_blocks_round_trip_across_frames_and_odd_padding() {
        // Two uncompressed blocks: an odd-length one ending mid-frame (followed by its padding
        // byte), then one spanning the 32,768-byte frame boundary.
        let a: Vec<u8> = (0..1001u32).map(|i| (i * 7) as u8).collect();
        let b: Vec<u8> = (0..40_000u32).map(|i| (i * 13 + 5) as u8).collect();
        let mut w = BitWriter::new();
        w.put(0, 1);
        uncompressed_header(&mut w, a.len() as u32);
        w.raw(&a);
        w.raw(&[0]); // odd-length padding
        uncompressed_header(&mut w, b.len() as u32);
        w.raw(&b);
        let out = lzx_decompress(&w.out, 0x8000, a.len() + b.len()).expect("decodes");
        assert_eq!(&out[..a.len()], &a[..]);
        assert_eq!(&out[a.len()..], &b[..]);
    }

    #[test]
    fn incomplete_huffman_table_is_rejected() {
        // Pretree with a single length-1 code is incomplete.
        let mut lens = [0u32; PRETREE_NUM];
        lens[0] = 1;
        assert!(Huffman::build(&lens, "pretree").is_err());
        // Two length-1 codes are complete; all zero is the empty tree.
        lens[1] = 1;
        assert!(Huffman::build(&lens, "pretree").is_ok());
        assert!(Huffman::build(&[0u32; 8], "aligned").is_ok());
    }

    #[test]
    fn extra_bits_and_bases_follow_the_lzx_tables() {
        let expect = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 15, 16, 16, 17, 17];
        for (slot, &e) in expect.iter().enumerate() {
            assert_eq!(extra_bits(slot), e, "slot {slot}");
        }
        let base = position_bases(8);
        assert_eq!(base, vec![0, 1, 2, 3, 4, 6, 8, 12]);
    }
}
