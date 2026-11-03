//! Bink audio, the DCT path every shipped track uses (`SPEC-audio.md` §4–§8).

use crate::container::AudioTrack;

/// Band edges in Hz, `0x1802E018` (§4).
const FREQ: [u32; 25] = [
    0, 100, 200, 300, 400, 510, 630, 770, 920, 1080, 1270, 1480, 1720, 2000, 2320, 2700, 3150, 3700, 4400, 5300, 6400, 7700, 9500,
    12000, 15500,
];

/// Run lengths in units of 8 coefficients, `0x1802E314` (§5.5).
const RUNS: [usize; 16] = [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64];

/// Band quantisers as f32 bit patterns, `0x1802E328` (§5.4).
const QUANT: [u32; 96] = [
    0x3f800000, 0x3f95253b, 0x3fadc8b9, 0x3fca7e34, 0x3febf1cf, 0x4009760e, 0x40202b68, 0x403aa11e, 0x405975d6, 0x407d626c, 0x40939f23,
    0x40ac022f, 0x40c86c93, 0x40e988af, 0x41080e85, 0x411e887a, 0x4138b8fb, 0x41573d0f, 0x417acbaf, 0x41921d06, 0x41aa404a, 0x41c6605b,
    0x41e725de, 0x4206aaa8, 0x421ce9d4, 0x4236d5d5, 0x42550a18, 0x42783bb8, 0x42909edc, 0x42a882fd, 0x42c4597f, 0x42e4c94a, 0x43054a6e,
    0x431b4f6a, 0x4334f79e, 0x4352dce1, 0x4375b274, 0x438f2499, 0x43a6ca3d, 0x43c257ef, 0x43e272e4, 0x4403edce, 0x4419b931, 0x44331e4a,
    0x4450b55c, 0x44732fd3, 0x448dae34, 0x44a515fe, 0x44c05b9f, 0x44e0229b, 0x450294bd, 0x4518271f, 0x453149cc, 0x454e937a, 0x4570b3c2,
    0x458c3ba2, 0x45a36634, 0x45be6480, 0x45ddd85f, 0x46013f33, 0x46169929, 0x462f7a18, 0x464c772b, 0x466e3e31, 0x468accd9, 0x46a1bad4,
    0x46bc7285, 0x46db9420, 0x46ffda4c, 0x47150f44, 0x472daf20, 0x474a6061, 0x476bcf0e, 0x478961cf, 0x47a013d1, 0x47ba85a1, 0x47d955cf,
    0x47fd3d1a, 0x48138965, 0x482be8d9, 0x48484f0e, 0x4869664a, 0x4887fa7b, 0x489e7121, 0x48b89dc6, 0x48d71d5c, 0x48faa6bf, 0x49120781,
    0x492a2736, 0x49464323, 0x496703d3, 0x498696d3, 0x499cd2b7, 0x49b6bae7, 0x49d4eab7, 0x49f81728,
];

/// The LSB-first reader over little-endian 32-bit words of one block (§5.1).
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    buf: u32,
    avail: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits { data, at: 0, buf: 0, avail: 0 }
    }

    fn word(&mut self) -> Result<u32, String> {
        let w = self.data.get(self.at..self.at + 4).ok_or("an audio block reads past its record")?;
        self.at += 4;
        Ok(u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
    }

    fn read(&mut self, n: u32) -> Result<u32, String> {
        let mask = ((1u64 << n) - 1) as u32;
        if self.avail < n {
            let w = self.word()?;
            let value = ((((w as u64) << self.avail) | self.buf as u64) as u32) & mask;
            self.buf = ((w as u64) >> (n - self.avail)) as u32;
            self.avail += 32 - n;
            Ok(value)
        } else {
            let value = self.buf & mask;
            self.buf = ((self.buf as u64) >> n) as u32;
            self.avail -= n;
            Ok(value)
        }
    }

    /// §5.3: a 23-bit mantissa, a 5-bit exponent and a sign.
    fn float29(&mut self) -> Result<f32, String> {
        let v = self.read(29)?;
        let e = v & 31;
        if e >= 24 {
            return Err(format!("an audio coefficient with exponent {e}, past the DLL's table"));
        }
        let f = ((v >> 5) & 0x7f_ffff) as f32 * f32::from_bits(0x3400_0000 + (e << 23));
        Ok(if v & 0x1000_0000 != 0 { -f } else { f })
    }
}

/// One track's decoder state (`FUN_180136E0`).
pub struct AudioDecoder {
    n: usize,
    channels: usize,
    bands: Vec<usize>,
    scale: f32,
    first_block: bool,
    /// The tail of the last block, `T / 16` samples.
    prev: Vec<i16>,
    /// The rotation table, `FUN_18019F60`.
    c: Vec<f32>,
    /// The complex stage's twiddles, `e^{+2πik/(N/2)}`.
    twiddles: Vec<(f32, f32)>,
}

impl AudioDecoder {
    pub fn new(track: &AudioTrack) -> Result<AudioDecoder, String> {
        if !track.is_dct() {
            return Err(format!("track {} uses flags {:#x}: only the DCT path is decoded", track.id, track.flags));
        }
        if track.flags & 0x4000_0000 == 0 {
            return Err(format!("track {} is 8-bit, which no shipped movie uses", track.id));
        }
        let rate = track.rate();
        let channels = track.channels() as usize;
        let n = if rate >= 44100 {
            2048
        } else if rate >= 22050 {
            1024
        } else {
            512
        };
        let half = (n >> 1) as u64;
        let hr = ((rate + 1) >> 1) as u64;
        let nb = FREQ.iter().position(|&f| hr <= f as u64).unwrap_or(25);
        let mut bands: Vec<usize> = FREQ[..nb].iter().map(|&f| ((f as u64 * half) / hr).max(1) as usize).collect();
        bands.push(half as usize);
        let total = n * channels;
        let nch = n >> 1;
        let delta = std::f64::consts::FRAC_PI_4 / nch as f64;
        let mut c = vec![0.0f32; n];
        c[0] = (delta * nch as f64).cos() as f32;
        c[nch] = 0.5 * c[0];
        for j in 1..nch {
            c[j] = ((j as f64 * delta).cos() * 0.5) as f32;
            c[n - j] = ((j as f64 * delta).sin() * 0.5) as f32;
        }
        let points = n / 2;
        let twiddles = (0..points / 2)
            .map(|k| {
                let a = 2.0 * std::f64::consts::PI * k as f64 / points as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .collect();
        Ok(AudioDecoder {
            n,
            channels,
            bands,
            scale: (2.0 / (n as f64).sqrt()) as f32,
            first_block: true,
            prev: vec![0; total / 16],
            c,
            twiddles,
        })
    }

    /// Decode one frame's record for this track: its declared byte count of 16-bit samples, the
    /// channels interleaved (§8). An empty record is no audio this frame.
    pub fn decode_record(&mut self, record: &[u8]) -> Result<Vec<i16>, String> {
        if record.is_empty() {
            return Ok(Vec::new());
        }
        let declared = record.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or("an audio record shorter than its count")?;
        let mut remaining = declared as usize / 2;
        let mut out = Vec::with_capacity(remaining);
        let mut at = 4;
        while remaining != 0 {
            let (samples, consumed) = self.decode_block(&record[at..])?;
            let take = samples.len().min(remaining);
            out.extend_from_slice(&samples[..take]);
            remaining -= take;
            at += consumed;
        }
        Ok(out)
    }

    /// One block: its samples after the overlap is held back, and the bytes it used (§5–§7).
    fn decode_block(&mut self, data: &[u8]) -> Result<(Vec<i16>, usize), String> {
        let n = self.n;
        let mut bits = Bits::new(data);
        bits.read(2)?;
        let mut planes = vec![vec![0.0f32; n]; self.channels];
        for plane in planes.iter_mut() {
            plane[0] = bits.float29()?;
            plane[1] = bits.float29()?;
            let mut quant = Vec::with_capacity(self.bands.len() - 1);
            for _ in 0..self.bands.len() - 1 {
                quant.push(f32::from_bits(QUANT[bits.read(8)?.min(95) as usize]));
            }
            self.coefficients(&mut bits, &quant, plane)?;
            self.transform(plane);
        }
        let total = n * self.channels;
        let mut out = vec![0i16; total];
        let pcm = |x: f32| -> i16 {
            let r = (x as f64 * self.scale as f64).round_ties_even();
            r.clamp(-32768.0, 32767.0) as i16
        };
        for i in 0..n {
            for (ch, plane) in planes.iter().enumerate() {
                out[i * self.channels + ch] = pcm(plane[i]);
            }
        }
        let m = total / 16;
        if self.first_block {
            self.first_block = false;
        } else {
            for i in 0..m {
                let s = self.prev[i] as i32 * (m - i) as i32 + out[i] as i32 * i as i32;
                out[i] = ((s as u32) / m as u32) as u16 as i16;
            }
        }
        self.prev.copy_from_slice(&out[total - m..]);
        out.truncate(total - m);
        Ok((out, bits.at))
    }

    /// §5.5.
    fn coefficients(&self, bits: &mut Bits, quant: &[f32], coeff: &mut [f32]) -> Result<(), String> {
        let n = self.n;
        let (mut i, mut k, mut q) = (2usize, 0usize, 0.0f32);
        while i < n {
            let end = if bits.read(1)? == 0 { i + 8 } else { i + 8 * RUNS[bits.read(4)? as usize] }.min(n);
            let width = bits.read(4)?;
            if width == 0 {
                coeff[i..end].fill(0.0);
                i = end;
                while k < quant.len() && 2 * self.bands[k] < end {
                    q = quant[k];
                    k += 1;
                }
            } else {
                while i < end {
                    if k < quant.len() && i == 2 * self.bands[k] {
                        q = quant[k];
                        k += 1;
                    }
                    let v = bits.read(width)?;
                    coeff[i] = if v == 0 {
                        0.0
                    } else {
                        let sign = if bits.read(1)? == 1 { -q } else { q };
                        sign * v as f32
                    };
                    i += 1;
                }
            }
        }
        Ok(())
    }

    /// §6.1: `x[k] = Σ coeff[j]·cos(π·j·(k+½)/N)`, by the DLL's chain.
    fn transform(&self, a: &mut [f32]) {
        let n = self.n;
        let nc = n;
        let c = &self.c;
        let m = n >> 1;
        // Pre-rotation, `FUN_1801E360`.
        for j in 1..m {
            let k = n - j;
            let (wkr, wki) = (c[j] - c[nc - j], c[j] + c[nc - j]);
            let t = wki * a[j] - wkr * a[k];
            a[j] = wkr * a[j] + wki * a[k];
            a[k] = t;
        }
        a[m] *= c[0];
        // The complex stage: `Z_k = Σ z_j·e^{+2πijk/(N/2)}`, natural order.
        self.fft(a);
        // Post-rotation, `FUN_1801E1F0`.
        let ks = 2 * nc / m;
        let mut kk = 0;
        for j in (2..m).step_by(2) {
            let k = n - j;
            kk += ks;
            let (wkr, wki) = (0.5 - c[nc - kk], c[kk]);
            let (xr, xi) = (a[j] - a[k], a[j + 1] + a[k + 1]);
            let (yr, yi) = (wkr * xr - wki * xi, wkr * xi + wki * xr);
            a[j] -= yr;
            a[j + 1] -= yi;
            a[k] += yr;
            a[k + 1] -= yi;
        }
        // The final butterfly.
        let t = a[0] - a[1];
        a[0] += a[1];
        for j in (2..n).step_by(2) {
            a[j - 1] = a[j] - a[j + 1];
            a[j] += a[j + 1];
        }
        a[n - 1] = t;
    }

    /// An in-place radix-2 DFT of the `N/2` complex values `a[2j] + i·a[2j+1]`, positive exponent.
    fn fft(&self, a: &mut [f32]) {
        let points = self.n / 2;
        let bits = points.trailing_zeros();
        for i in 0..points {
            let j = i.reverse_bits() >> (usize::BITS - bits);
            if j > i {
                a.swap(2 * i, 2 * j);
                a.swap(2 * i + 1, 2 * j + 1);
            }
        }
        let mut size = 2;
        while size <= points {
            let step = points / size;
            for start in (0..points).step_by(size) {
                for k in 0..size / 2 {
                    let (wr, wi) = self.twiddles[k * step];
                    let (p, q) = (start + k, start + k + size / 2);
                    let (br, bi) = (a[2 * q], a[2 * q + 1]);
                    let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                    let (ar, ai) = (a[2 * p], a[2 * p + 1]);
                    a[2 * p] = ar + tr;
                    a[2 * p + 1] = ai + ti;
                    a[2 * q] = ar - tr;
                    a[2 * q + 1] = ai - ti;
                }
            }
            size *= 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoder(rate: u32, stereo: bool) -> AudioDecoder {
        let flags = 0x5000_0000 | rate | if stereo { 0x2000_0000 } else { 0 };
        AudioDecoder::new(&AudioTrack { max_buffer: 0, flags, id: 0 }).unwrap()
    }

    #[test]
    fn the_transform_is_the_spec_closed_form() {
        let d = decoder(22050, false);
        let n = d.n;
        let input: Vec<f32> = (0..n).map(|j| ((j * 37 % 101) as f32 - 50.0) * if j < 64 { 10.0 } else { 0.1 }).collect();
        let mut fast = input.clone();
        d.transform(&mut fast);
        for k in [0, 1, 7, 255, 511, 512, 900, n - 1] {
            let slow: f64 = (0..n).map(|j| input[j] as f64 * (std::f64::consts::PI * j as f64 * (k as f64 + 0.5) / n as f64).cos()).sum();
            assert!((fast[k] as f64 - slow).abs() < 0.05, "x[{k}] = {} by the chain, {slow} by the sum", fast[k]);
        }
    }

    #[test]
    fn bands_and_scale_match_the_spec_table() {
        let d = decoder(22050, false);
        assert_eq!(d.bands, [1, 4, 9, 13, 18, 23, 29, 35, 42, 50, 58, 68, 79, 92, 107, 125, 146, 171, 204, 246, 297, 357, 441, 512]);
        assert_eq!(d.scale, 0.0625);
        let s = decoder(48000, true);
        assert_eq!(s.bands.len(), 26);
        assert_eq!(s.prev.len(), 256);
    }

    #[test]
    fn the_reader_is_lsb_first_over_words() {
        let mut b = Bits::new(&[0b1011_0101, 0, 0, 0x80, 0xff, 0, 0, 0]);
        assert_eq!(b.read(1).unwrap(), 1);
        assert_eq!(b.read(3).unwrap(), 0b010);
        assert_eq!(b.read(28).unwrap(), 0x800_000b);
        assert_eq!(b.read(8).unwrap(), 0xff);
        assert_eq!(b.at, 8);
    }
}
