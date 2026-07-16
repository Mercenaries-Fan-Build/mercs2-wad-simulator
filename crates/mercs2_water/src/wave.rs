//! Animated water-surface waves — the displacement the static watermap height doesn't carry.
//!
//! # Provenance & honesty boundary
//! WILDSTAR-sourced shape (`WSWater::CalcWaveOffsets`, recovered from the Saboteur Xbox360 devkit —
//! `docs/reverse_engineer/saboteur_mercs2_crossval_render_physics.md`): the exe drives its surface from
//! **two summed time-animated sinusoids** — `cos/sin((time + phase) · freq) · amp` plus a second
//! `cos/sin(time · freq) · amp2` component — i.e. a 2-component sinusoidal wave field, not an FFT ocean.
//! That structure (2 components, phase advancing with time) is what this models.
//!
//! **Bounded:** `WSWater::GetHeight`/`GetVelocity` — which sample the field per-position — are VMX128
//! (SIMD) bodies that did not decode, so the exact per-position sampling and the authored amp/freq
//! constants are **not** recovered. The directional-sinusoid sampling and the default tunables here are
//! ours (`// CONFIRM-LIVE:`), chosen to match the existing surface look; the *shape* (2 summed animated
//! sinusoids) is WildStar's. Water level itself still comes from the authored watermap — this is the
//! displacement on top.

/// One directional sinusoid: `amp · sin((x,z)·dir · freq + t · speed)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveComponent {
    /// Vertical amplitude (m).
    pub amp: f32,
    /// Spatial frequency (radians per metre along `dir`).
    pub freq: f32,
    /// Phase speed (radians/second) — the `time · freq` term WildStar advances each frame.
    pub speed: f32,
    /// Unit travel direction in world XZ.
    pub dir: [f32; 2],
}

impl WaveComponent {
    fn height(&self, x: f32, z: f32, t: f32) -> f32 {
        let phase = (x * self.dir[0] + z * self.dir[1]) * self.freq + t * self.speed;
        self.amp * phase.sin()
    }
}

/// The 2-component animated wave field (`WSWater::CalcWaveOffsets` shape). Sampled by BOTH the water
/// render (surface displacement) and the height query (so swimmers/boats bob on the same surface the
/// player sees) — one field, one source of truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveModel {
    pub primary: WaveComponent,
    pub secondary: WaveComponent,
}

impl Default for WaveModel {
    fn default() -> Self {
        // `// CONFIRM-LIVE:` amp/freq/speed are ours (the authored values sit behind GetHeight's SIMD
        // body). Tuned to a gentle open-water swell that matches the existing surface ripple rates.
        WaveModel {
            primary: WaveComponent { amp: 0.13, freq: 0.15, speed: 1.3, dir: [1.0, 0.0] },
            secondary: WaveComponent { amp: 0.07, freq: 0.31, speed: -1.1, dir: [0.0, 1.0] },
        }
    }
}

impl WaveModel {
    /// A dead-flat field (no displacement) — for interiors/pools, or to disable waves in a test.
    pub fn flat() -> Self {
        WaveModel {
            primary: WaveComponent { amp: 0.0, freq: 0.0, speed: 0.0, dir: [1.0, 0.0] },
            secondary: WaveComponent { amp: 0.0, freq: 0.0, speed: 0.0, dir: [0.0, 1.0] },
        }
    }

    /// Vertical displacement (m) to add to the watermap surface height at world `(x, z)` and time `t`.
    /// **Must stay in lockstep with the same sum in `water.wgsl`** or the drawn surface and the
    /// physics/swim surface diverge.
    pub fn height_offset(&self, x: f32, z: f32, t: f32) -> f32 {
        self.primary.height(x, z, t) + self.secondary.height(x, z, t)
    }

    /// Peak-to-trough bound of the field — the most the surface can deviate from the watermap level.
    pub fn max_amplitude(&self) -> f32 {
        self.primary.amp.abs() + self.secondary.amp.abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_field_never_displaces() {
        let w = WaveModel::flat();
        for t in [0.0f32, 1.0, 7.5] {
            assert_eq!(w.height_offset(12.0, -3.0, t), 0.0);
        }
    }

    #[test]
    fn field_is_bounded_by_amplitude_and_animates() {
        let w = WaveModel::default();
        let bound = w.max_amplitude();
        // Never exceeds the summed amplitude, anywhere/anywhen.
        for i in 0..200 {
            let t = i as f32 * 0.1;
            let h = w.height_offset(t * 3.0, t * -1.7, t);
            assert!(h.abs() <= bound + 1e-5, "h={h} exceeded bound {bound}");
        }
        // The same point moves over time (the surface is animated, not static).
        let a = w.height_offset(5.0, 5.0, 0.0);
        let b = w.height_offset(5.0, 5.0, 1.0);
        assert!((a - b).abs() > 1e-3, "surface should animate: {a} vs {b}");
    }

    #[test]
    fn two_components_sum() {
        // With the secondary zeroed, the field is exactly the primary sinusoid.
        let mut w = WaveModel::default();
        w.secondary.amp = 0.0;
        let expect = w.primary.amp * ((5.0f32 * 0.15) + 2.0 * 1.3).sin();
        assert!((w.height_offset(5.0, 0.0, 2.0) - expect).abs() < 1e-5);
    }
}
