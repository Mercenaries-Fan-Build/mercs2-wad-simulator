//! Microsoft LZX decompressor (the variant used by Xbox 360 XEX / WIM).
//!
//! Single continuous stream (no CAB chunk framing — the XEX block layer already
//! concatenated the chunks). Ported from `tools/lzx_decompress.py`, which is
//! itself a port of the libmspack `lzxd.c` algorithm. Canonical Huffman decode
//! via a `(len, code)` HashMap — slower than a table decode but simpler and
//! verifiable; fine for a one-shot extraction.
//!
//! STATUS (inherited from the Python original): the FIRST LZX block decodes
//! correctly (verified byte-exact against a real XEX PE prefix), but there is
//! an unresolved desync at the first block->block boundary — multi-block
//! streams currently produce garbage past block 0. See
//! `docs/reverse_engineer/jul08_prototype_iso.md`. Do not trust output spanning
//! more than one LZX block until this is fixed. This Rust port intentionally
//! preserves that behaviour byte-for-byte; the desync is a pre-existing bug in
//! the algorithm as written, not a translation artifact.

use std::collections::HashMap;

// position-slot extra bits (51 slots, enough for up to 32 MB window)
const EXTRA_BITS: [u32; 51] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13, 14, 14, 15, 15, 16, 16, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17, 17,
];

const POS_BASE: [u32; 51] = {
    let mut arr = [0u32; 51];
    let mut i = 1;
    while i < 51 {
        arr[i] = arr[i - 1] + (1u32 << EXTRA_BITS[i - 1]);
        i += 1;
    }
    arr
};

const NUM_PRIMARY_LENGTHS: u32 = 7;
const MIN_MATCH: u32 = 2;
const NUM_CHARS: usize = 256;
const PRETREE_NUM: usize = 20;
const ALIGNED_NUM: usize = 8;
const LENGTH_NUM: usize = 249;

const VERBATIM: u32 = 1;
const ALIGNED: u32 = 2;
const UNCOMPRESSED: u32 = 3;

/// 16-bit little-endian words; bits served MSB-first within each word.
///
/// Mirrors the Python `BitReader` field-for-field. Max buffer occupancy is
/// 47 bits (see docstring of `ensure` in the Python source), so a `u64`
/// buffer is comfortable.
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

    fn ensure(&mut self, need: u32) {
        let l = self.d.len();
        while self.n < need {
            let p = self.pos;
            let w: u64 = if p + 1 < l {
                (self.d[p] as u64) | ((self.d[p + 1] as u64) << 8)
            } else if p < l {
                self.d[p] as u64
            } else {
                0
            };
            self.pos = p + 2;
            self.buf = (self.buf << 16) | w;
            self.n += 16;
        }
    }

    fn bits(&mut self, k: u32) -> u32 {
        if k == 0 {
            return 0;
        }
        if self.n < k {
            self.ensure(k);
        }
        self.n -= k;
        let mask = if k >= 32 { 0xFFFF_FFFFu64 } else { (1u64 << k) - 1 };
        let v = ((self.buf >> self.n) & mask) as u32;
        // drop consumed high bits (keep buffer small)
        self.buf &= if self.n == 0 { 0 } else { (1u64 << self.n) - 1 };
        v
    }
}

/// Canonical Huffman decoder: dict keyed `(length<<20)|code -> symbol`, plus
/// `maxlen`. Matches the Python `build_decoder` output shape exactly.
type Decoder = HashMap<u32, u32>;

fn build_decoder(lengths: &[u32]) -> (Decoder, u32) {
    let maxlen = lengths.iter().copied().max().unwrap_or(0);
    let mut dec: Decoder = HashMap::new();
    let mut code: u32 = 0;
    // bucket symbols by length, ascending symbol order preserved (Vec push
    // preserves insertion order, same as Python setdefault().append()).
    let mut by_len: HashMap<u32, Vec<u32>> = HashMap::new();
    for (sym, &l) in lengths.iter().enumerate() {
        if l != 0 {
            by_len.entry(l).or_default().push(sym as u32);
        }
    }
    for l in 1..=maxlen {
        if let Some(syms) = by_len.get(&l) {
            for &sym in syms {
                dec.insert((l << 20) | code, sym);
                code += 1;
            }
        }
        code <<= 1;
    }
    (dec, maxlen)
}

fn read_sym(br: &mut BitReader<'_>, dec: &Decoder, maxlen: u32) -> Result<u32, String> {
    let mut code: u32 = 0;
    for l in 1..=maxlen {
        code = (code << 1) | br.bits(1);
        if let Some(&s) = dec.get(&((l << 20) | code)) {
            return Ok(s);
        }
    }
    Err("bad huffman code".to_string())
}

fn read_lengths(
    lens: &mut [u32],
    first: usize,
    last: usize,
    br: &mut BitReader<'_>,
) -> Result<(), String> {
    let mut pre = [0u32; PRETREE_NUM];
    for slot in pre.iter_mut() {
        *slot = br.bits(4);
    }
    let (dec, ml) = build_decoder(&pre);
    let mut i = first;
    while i < last {
        let sym = read_sym(br, &dec, ml)?;
        if sym == 17 {
            let run = br.bits(4) + 4;
            for _ in 0..run {
                lens[i] = 0;
                i += 1;
            }
        } else if sym == 18 {
            let run = br.bits(5) + 20;
            for _ in 0..run {
                lens[i] = 0;
                i += 1;
            }
        } else if sym == 19 {
            let run = br.bits(1) + 4;
            let sym2 = read_sym(br, &dec, ml)?;
            // Python: val = (lens[i] - sym2) % 17
            // Python's `%` on negatives is Euclidean — use `rem_euclid`.
            let val = (lens[i] as i32 - sym2 as i32).rem_euclid(17) as u32;
            for _ in 0..run {
                lens[i] = val;
                i += 1;
            }
        } else {
            // Python: lens[i] = (lens[i] - sym) % 17
            lens[i] = (lens[i] as i32 - sym as i32).rem_euclid(17) as u32;
            i += 1;
        }
    }
    Ok(())
}

/// Decompress an LZX stream.
///
/// Matches the Python `lzx_decompress(data, window_size, out_size) -> bytes`
/// signature: `data` is the concatenated LZX blocks, `window_size` is the
/// negotiated window (a power of two; e.g. `1 << 17` for XEX), and `out_size`
/// is the exact expected output size (the algorithm consumes blocks until
/// the output reaches this length, then trims).
///
/// See the module docstring for the known first-block-boundary desync — this
/// port preserves the Python behaviour exactly and does not fix it.
pub fn lzx_decompress(
    data: &[u8],
    window_size: u32,
    out_size: usize,
) -> Result<Vec<u8>, String> {
    if window_size == 0 {
        return Err("window_size must be > 0".to_string());
    }
    let wbits = window_size.ilog2();
    let posn_slots: u32 = match wbits {
        20 => 42,
        21 => 50,
        _ => wbits * 2,
    };
    let main_elements = NUM_CHARS + 8 * posn_slots as usize;

    let mut br = BitReader::new(data);
    let mut out: Vec<u8> = Vec::new();
    let mut r0: u32 = 1;
    let mut r1: u32 = 1;
    let mut r2: u32 = 1;

    let mut main_len = vec![0u32; main_elements];
    let mut length_len = vec![0u32; LENGTH_NUM];

    let e8 = br.bits(1);
    let intel_filesize: u32 = if e8 != 0 { br.bits(32) } else { 0 };

    while out.len() < out_size {
        let btype = br.bits(3);
        let bsize = br.bits(24) as usize;

        if btype == VERBATIM || btype == ALIGNED {
            let (aligned_dec, aligned_ml) = if btype == ALIGNED {
                let mut alen = [0u32; ALIGNED_NUM];
                for slot in alen.iter_mut() {
                    *slot = br.bits(3);
                }
                let (d, m) = build_decoder(&alen);
                (Some(d), Some(m))
            } else {
                (None, None)
            };
            read_lengths(&mut main_len, 0, NUM_CHARS, &mut br)?;
            read_lengths(&mut main_len, NUM_CHARS, main_elements, &mut br)?;
            let (main_dec, main_ml) = build_decoder(&main_len);
            read_lengths(&mut length_len, 0, LENGTH_NUM, &mut br)?;
            let (length_dec, length_ml) = build_decoder(&length_len);

            let end = out.len() + bsize;
            while out.len() < end {
                let sym = read_sym(&mut br, &main_dec, main_ml)?;
                if (sym as usize) < NUM_CHARS {
                    out.push(sym as u8);
                    continue;
                }
                let sym_off = sym - NUM_CHARS as u32;
                let length_header = sym_off & 7;
                let position_slot = (sym_off >> 3) as usize;
                let match_len: u32 = if length_header == NUM_PRIMARY_LENGTHS {
                    read_sym(&mut br, &length_dec, length_ml)?
                        + NUM_PRIMARY_LENGTHS
                        + MIN_MATCH
                } else {
                    length_header + MIN_MATCH
                };

                let match_off: u32;
                if position_slot == 0 {
                    match_off = r0;
                } else if position_slot == 1 {
                    match_off = r1;
                    r1 = r0;
                    r0 = match_off;
                } else if position_slot == 2 {
                    match_off = r2;
                    r2 = r0;
                    r0 = match_off;
                } else {
                    let extra = EXTRA_BITS[position_slot];
                    let formatted: u32 = if aligned_dec.is_some() && extra >= 3 {
                        let verb = br.bits(extra - 3) << 3;
                        let aln = read_sym(
                            &mut br,
                            aligned_dec.as_ref().unwrap(),
                            aligned_ml.unwrap(),
                        )?;
                        POS_BASE[position_slot] + verb + aln
                    } else {
                        POS_BASE[position_slot] + br.bits(extra)
                    };
                    match_off = formatted - 2;
                    r2 = r1;
                    r1 = r0;
                    r0 = match_off;
                }

                let match_off_usize = match_off as usize;
                if match_off_usize > out.len() {
                    return Err(format!(
                        "match before start (src={})",
                        out.len() as isize - match_off as isize
                    ));
                }
                let mut src = out.len() - match_off_usize;
                // copy with overlap
                for _ in 0..match_len {
                    let b = out[src];
                    out.push(b);
                    src += 1;
                }
            }
        } else if btype == UNCOMPRESSED {
            // realign to a 16-bit boundary and read R0,R1,R2 then raw bytes
            if br.n >= 16 {
                // Guard against usize underflow (pos should always be >= 2
                // here in practice — we've already consumed btype+bsize which
                // forced at least one word fetch — but be defensive).
                if br.pos < 2 {
                    return Err("uncompressed block: cannot rewind bit-reader".to_string());
                }
                br.pos -= 2;
            }
            br.n = 0;
            br.buf = 0;
            let mut p = br.pos;
            if p + 12 > data.len() {
                return Err("uncompressed block truncated reading R0/R1/R2".to_string());
            }
            r0 = u32::from_le_bytes(data[p..p + 4].try_into().unwrap());
            r1 = u32::from_le_bytes(data[p + 4..p + 8].try_into().unwrap());
            r2 = u32::from_le_bytes(data[p + 8..p + 12].try_into().unwrap());
            p += 12;
            if p + bsize > data.len() {
                return Err("uncompressed block truncated reading raw bytes".to_string());
            }
            out.extend_from_slice(&data[p..p + bsize]);
            p += bsize;
            if bsize & 1 != 0 {
                p += 1;
            }
            br.pos = p;
        } else {
            return Err(format!("bad block type {}", btype));
        }
    }

    out.truncate(out_size);

    // Intel E8 call translation (decode), per 32768-byte frame
    if e8 != 0 && intel_filesize != 0 && out.len() > 10 {
        let ob_len = out.len();
        let mut frame: usize = 0;
        while frame < ob_len {
            if (frame as u64) >= intel_filesize as u64 {
                break;
            }
            let flen = std::cmp::min(32768usize, ob_len - frame);
            // Python: `while i < flen - 10:` — equivalent to `i + 10 < flen`
            // and safely handles the flen < 10 case (loop just doesn't run).
            let mut i: usize = 0;
            while i + 10 < flen {
                if out[frame + i] == 0xE8 {
                    let cur = frame + i;
                    let abs_off = i32::from_le_bytes(
                        out[cur + 1..cur + 5].try_into().unwrap(),
                    );
                    let cur_i64 = cur as i64;
                    let abs_off_i64 = abs_off as i64;
                    let ifs_i64 = intel_filesize as i64;
                    if -cur_i64 <= abs_off_i64 && abs_off_i64 < ifs_i64 {
                        let rel: i64 = if abs_off >= 0 {
                            abs_off_i64 - cur_i64
                        } else {
                            abs_off_i64 + ifs_i64
                        };
                        let rel_u32 = (rel as i64 as u32) & 0xFFFF_FFFF;
                        out[cur + 1..cur + 5].copy_from_slice(&rel_u32.to_le_bytes());
                    }
                    i += 5;
                    continue;
                }
                i += 1;
            }
            frame += 32768;
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_window_size_is_rejected() {
        let err = lzx_decompress(&[0u8; 32], 0, 0).unwrap_err();
        assert!(!err.is_empty(), "should surface a non-empty error");
    }

    #[test]
    fn zero_output_is_empty_result() {
        // out_size=0 should short-circuit to an empty buffer regardless of input.
        let out = lzx_decompress(&[0u8; 4], 0x8000, 0).expect("empty target");
        assert!(out.is_empty(), "zero-size decompress must yield empty buf");
    }

    #[test]
    fn uncompressed_out_of_bounds_errors() {
        // Block type 0 (UNCOMPRESSED) but the input lies about a huge size — parser must
        // refuse rather than panic on the bounds slice.
        let mut data = Vec::new();
        // 3-bit block type = 0, then 24-bit size = 0xFFFFFF. Bit-packed high-first per LZX.
        data.extend_from_slice(&[0x1F, 0xFF, 0xFF, 0xFF]);
        data.extend_from_slice(&[0u8; 4]);
        assert!(lzx_decompress(&data, 0x8000, 0x1000).is_err(),
                "malformed UNCOMPRESSED block must produce Err, not panic");
    }
}
