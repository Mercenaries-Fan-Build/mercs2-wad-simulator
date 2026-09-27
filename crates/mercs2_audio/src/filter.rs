//! The biquad low-pass filter a wave carries when its cue has a kind-9 event — `PalSoundWaveDX8`'s
//! effect object at wave `+0x2C`, created by `FUN_00839db0`, constructed by `FUN_0083f2d0` (vtable
//! `0x00BE2678`), read from the disassembly of the unpacked PC exe.
//!
//! * **Parameters** (`SetParam`, `0x0083F670`), each from a value `v` the cue's kind-9 record
//!   evaluates (cue `+0x84` / `+0x88`, handed over by the cue parameter object at `+0x7C` every
//!   instance update, `FUN_0083e5c0`):
//!   `cutoff = ((100 − nyquist) × −1) × (v − 1) + nyquist` (parameter 0) and
//!   `q = ((0.70710677 − 2) × −1) × (v − 1) + 2` (parameter 1). Either marks the coefficients stale.
//!   (The code below negates where the exe multiplies by −1; in IEEE arithmetic the two are exact
//!   and give the same bits.)
//! * **Coefficients** (`FUN_0083f430`, when stale): `w = (cutoff / rate) × π`,
//!   `k = (f32) tan((f64) w)`, `kk = (f32) pow((f64) k, 2)`, `qk = q × k`, `d = (qk + kk) + 1`,
//!   `n = 1 / d`, `b0 = b2 = n × kk`, `b1 = b0 × 2`, `a1 = ((kk − 1) × n) × 2`,
//!   `a2 = (d − qk × 2) × n` — all single precision in that order.
//! * **Processing** (`FUN_0083f430`): over `channels` runs of `count` int32 samples, run `c` starting
//!   at `buffer + c × count`, with one history per run:
//!   `y = trunc(((((x₂ × b2) + (x₁ × b1)) + x × b0) − y₁ × a1) − y₂ × a2)` (`cvttss2si`), then
//!   `x₂ ← x₁, y₂ ← y₁, x₁ ← x, y₁ ← y`. A rate that differs from the last one sets the Nyquist
//!   frequency to `rate × 0.5` and marks the coefficients stale first.
//!
//! The constructor sets `q = 1.4142135` (`DAT_017D4268` in the runtime image), rate 44,100 and
//! cutoff = Nyquist = 22,050 (`DAT_00BEAD90`, `DAT_00BEAD8C`), and computes the coefficients.

/// Histories kept (the constructor zeroes eight).
const HISTORIES: usize = 8;

/// `cvttss2si`: truncation toward zero, `i32::MIN` when out of range or NaN.
fn trunc_i32(v: f32) -> i32 {
    if v.is_nan() || !(-2_147_483_648.0..2_147_483_648.0).contains(&v) {
        i32::MIN
    } else {
        v as i32
    }
}

/// The filter's state.
#[derive(Clone, Debug, PartialEq)]
pub struct Biquad {
    cutoff: f32,
    q: f32,
    rate: f32,
    nyquist: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    stale: bool,
    /// Per run: `[x₁, x₂, y₁, y₂]`.
    history: [[i32; 4]; HISTORIES],
}

impl Default for Biquad {
    fn default() -> Self {
        Biquad::new()
    }
}

impl Biquad {
    /// `FUN_0083f2d0`.
    pub fn new() -> Biquad {
        let mut f = Biquad {
            cutoff: 22050.0,
            q: f32::from_bits(0x3FB5_04F3),
            rate: 44100.0,
            nyquist: 22050.0,
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            stale: false,
            history: [[0; 4]; HISTORIES],
        };
        f.coefficients();
        f
    }

    /// `SetParam` (`0x0083F670`): parameter 0 sets the cutoff, 1 the resonance term; others do nothing.
    pub fn set_param(&mut self, param: u32, v: f32) {
        match param {
            0 => {
                self.cutoff = -(100.0f32 - self.nyquist) * (v - 1.0) + self.nyquist;
                self.stale = true;
            }
            1 => {
                self.q = -(f32::from_bits(0x3F35_04F3) - 2.0) * (v - 1.0) + 2.0;
                self.stale = true;
            }
            _ => {}
        }
    }

    /// The coefficients `(b0, b1, b2, a1, a2)`.
    pub fn coefficients_now(&self) -> (f32, f32, f32, f32, f32) {
        (self.b0, self.b1, self.b2, self.a1, self.a2)
    }

    /// The cutoff and resonance term.
    pub fn params(&self) -> (f32, f32) {
        (self.cutoff, self.q)
    }

    fn coefficients(&mut self) {
        let w = (self.cutoff / self.rate) * std::f32::consts::PI;
        let k = f64::from(w).tan() as f32;
        let kk = f64::from(k).powf(2.0) as f32;
        let qk = self.q * k;
        let d = (qk + kk) + 1.0;
        let n = 1.0 / d;
        let b0 = n * kk;
        self.b0 = b0;
        self.b1 = b0 * 2.0;
        self.b2 = b0;
        self.a1 = ((kk - 1.0) * n) * 2.0;
        self.a2 = (d - qk * 2.0) * n;
        self.stale = false;
    }

    /// `FUN_0083f430`: filter `channels` runs of `count` samples of `buffer` in place, at `rate` Hz.
    /// The buffer grows (with zeros) when a run reaches past its end, as the engine's static buffer
    /// simply holds whatever was there.
    pub fn process(&mut self, buffer: &mut Vec<i32>, count: usize, channels: usize, rate: u32) {
        let r = rate as i32 as f32;
        if r != self.rate {
            self.rate = r;
            self.nyquist = r * 0.5;
            self.stale = true;
        }
        if self.stale {
            self.coefficients();
        }
        if buffer.len() < channels * count {
            buffer.resize(channels * count, 0);
        }
        for c in 0..channels.min(HISTORIES) {
            let h = &mut self.history[c];
            for x in &mut buffer[c * count..(c + 1) * count] {
                let input = *x;
                let acc = (h[1] as f32 * self.b2) + (h[0] as f32 * self.b1);
                let acc = acc + input as f32 * self.b0;
                let acc = acc - h[2] as f32 * self.a1;
                let acc = acc - h[3] as f32 * self.a2;
                let y = trunc_i32(acc);
                *x = y;
                h[1] = h[0];
                h[3] = h[2];
                h[0] = input;
                h[2] = y;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The coefficients, computed step by step as `FUN_0083f430` computes them.
    fn reference(cutoff: f32, q: f32, rate: f32) -> (f32, f32, f32, f32, f32) {
        let w = cutoff / rate;
        let w = w * std::f32::consts::PI;
        let k = (w as f64).tan() as f32;
        let kk = ((k as f64) * (k as f64)) as f32;
        let qk = q * k;
        let d = qk + kk;
        let d = d + 1.0;
        let n = 1.0 / d;
        let b0 = n * kk;
        let a1 = kk - 1.0;
        let a1 = a1 * n;
        let a1 = a1 * 2.0;
        let a2 = d - qk * 2.0;
        let a2 = a2 * n;
        (b0, b0 * 2.0, b0, a1, a2)
    }

    #[test]
    fn construction_and_parameters_follow_the_traced_code() {
        let mut f = Biquad::new();
        assert_eq!(f.params(), (22050.0, f32::from_bits(0x3FB5_04F3)));
        assert_eq!(f.coefficients_now(), reference(22050.0, f32::from_bits(0x3FB5_04F3), 44100.0));
        // D8CE1427's kind-9 curve at x = -1 yields 0.65; its second output keeps its initial 1.0.
        f.set_param(0, 0.65);
        f.set_param(1, 1.0);
        let cutoff = -(100.0f32 - 22050.0) * (0.65 - 1.0) + 22050.0;
        assert_eq!(f.params(), (cutoff, 2.0));
        f.set_param(1, 0.0);
        // v = 0 gives 1/sqrt(2) up to the rounding of the single-precision steps (one ulp above it).
        let q = -(f32::from_bits(0x3F35_04F3) - 2.0) * (0.0 - 1.0) + 2.0;
        assert_eq!(q.to_bits(), 0x3F35_04F4);
        assert_eq!(f.params().1, q);
        let mut buf = vec![0i32; 6];
        f.process(&mut buf, 6, 1, 44100);
        assert_eq!(f.coefficients_now(), reference(cutoff, q, 44100.0));
    }

    #[test]
    fn processing_runs_the_difference_equation_per_run() {
        let mut f = Biquad::new();
        f.set_param(0, 0.5);
        let mut buf: Vec<i32> = vec![1000, -2000, 3000, 0, 500, 7];
        f.process(&mut buf, 6, 1, 44100);
        let (b0, b1, b2, a1, a2) = f.coefficients_now();
        let (mut x1, mut x2, mut y1, mut y2) = (0i32, 0i32, 0i32, 0i32);
        for (i, x) in [1000, -2000, 3000, 0, 500, 7].into_iter().enumerate() {
            let acc = (x2 as f32 * b2) + (x1 as f32 * b1);
            let acc = acc + x as f32 * b0;
            let acc = acc - y1 as f32 * a1;
            let y = (acc - y2 as f32 * a2) as i32;
            assert_eq!(buf[i], y, "sample {i}");
            x2 = x1;
            y2 = y1;
            x1 = x;
            y1 = y;
        }
        // A second run starts at buffer + count and keeps its own history.
        let mut two = vec![100i32; 4];
        let mut g = Biquad::new();
        g.process(&mut two, 2, 2, 44100);
        assert_eq!(two[0], two[2], "each run starts from a zero history");
        assert_eq!(two[1], two[3]);
    }
}
