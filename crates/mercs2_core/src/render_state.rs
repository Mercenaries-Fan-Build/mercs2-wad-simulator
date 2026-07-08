//! Global **render / post-FX state** the presentation Lua namespaces drive (`Atmosphere`, `Bloom`,
//! `Graphics`, `Fade`). These cfuncs set the parameters the renderer consumes each frame — sky/scatter
//! params, HDR bloom knobs, gamma/shadow distance, screen fades. The engine owns this parameter state;
//! the render passes read it. This module is that owned state (the rasterization is a separate concern
//! in `mercs2_engine`), so every `Set*`→`Get*` round-trips for real.

use std::collections::HashMap;

/// Sky / atmosphere / scattering parameters. The shipped Lua overwhelmingly drives this through the
/// **generic** `Atmosphere.SetValue(key, v)` / `SetColorValue(key, rgba)` / `SetIntValue(key, i)` API
/// (109 / 78 / n call sites) rather than the typed setters, so the canonical store is three keyed maps;
/// the typed setters (`SetLightIntensity`, `SetAmbientColor`, …) route into the same maps under a
/// canonical key.
#[derive(Clone, Debug, Default)]
pub struct AtmosphereState {
    /// `Begin`/`End` — whether an atmosphere edit block / override is active.
    pub active: bool,
    /// Time-of-day (`SetTime`) and its advance rate (`SetTimeSpeed`).
    pub time: f32,
    pub time_speed: f32,
    /// Named scalar parameters (`SetValue`/`GetValue`).
    pub values: HashMap<String, f32>,
    /// Named color parameters (`SetColorValue`/`GetColorValue`) — RGBA.
    pub colors: HashMap<String, [f32; 4]>,
    /// Named integer parameters (`SetIntValue`/`GetIntValue`).
    pub ints: HashMap<String, i64>,
}

impl AtmosphereState {
    pub fn set_value(&mut self, key: &str, v: f32) {
        self.values.insert(key.to_string(), v);
    }
    pub fn value(&self, key: &str) -> f32 {
        self.values.get(key).copied().unwrap_or(0.0)
    }
    pub fn set_color(&mut self, key: &str, rgba: [f32; 4]) {
        self.colors.insert(key.to_string(), rgba);
    }
    pub fn color(&self, key: &str) -> [f32; 4] {
        self.colors.get(key).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])
    }
    pub fn set_int(&mut self, key: &str, v: i64) {
        self.ints.insert(key.to_string(), v);
    }
    pub fn int(&self, key: &str) -> i64 {
        self.ints.get(key).copied().unwrap_or(0)
    }
}

/// HDR bloom parameters (`Bloom.*`).
#[derive(Clone, Debug)]
pub struct BloomState {
    pub blur_radius: f32,
    pub threshold: f32,
    pub multiplier: f32,
    pub amount: f32,
    pub target_luminance: f32,
    pub adaptive_luminance_percent: f32,
    pub adaptive_luminance_scale: f32,
}

impl Default for BloomState {
    fn default() -> Self {
        BloomState {
            blur_radius: 1.0,
            threshold: 1.0,
            multiplier: 1.0,
            amount: 1.0,
            target_luminance: 0.5,
            adaptive_luminance_percent: 1.0,
            adaptive_luminance_scale: 1.0,
        }
    }
}

/// Global graphics settings (`Graphics.*`).
#[derive(Clone, Debug)]
pub struct GraphicsState {
    pub gamma: f32,
    pub shadow_base_distance: f32,
    pub screen_ratio: f32,
    /// `SetBoundaryEffect` — the out-of-bounds screen effect strength (0 = off).
    pub boundary_effect: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState { gamma: 1.0, shadow_base_distance: 0.0, screen_ratio: 16.0 / 9.0, boundary_effect: 0.0 }
    }
}

/// Screen-fade colors (`Fade.*`): each is an RGBA the compositor lerps toward.
#[derive(Clone, Debug, Default)]
pub struct FadeState {
    pub ambient_top: [f32; 4],
    pub ambient_sides: [f32; 4],
    pub terrain: [f32; 4],
    pub camera_fade: [f32; 4],
}

/// The aggregate render/post-FX parameter state the presentation namespaces drive.
#[derive(Clone, Debug, Default)]
pub struct RenderState {
    pub atmosphere: AtmosphereState,
    pub bloom: BloomState,
    pub graphics: GraphicsState,
    pub fade: FadeState,
}

impl RenderState {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atmosphere_keyed_params_roundtrip() {
        let mut a = AtmosphereState::default();
        a.set_value("fog_density", 0.35);
        a.set_color("sky_top", [0.2, 0.4, 0.9, 1.0]);
        a.set_int("weather", 3);
        assert_eq!(a.value("fog_density"), 0.35);
        assert_eq!(a.color("sky_top"), [0.2, 0.4, 0.9, 1.0]);
        assert_eq!(a.int("weather"), 3);
        // unset keys read the neutral default
        assert_eq!(a.value("nope"), 0.0);
    }

    #[test]
    fn bloom_and_graphics_defaults() {
        let r = RenderState::new();
        assert_eq!(r.bloom.threshold, 1.0);
        assert_eq!(r.graphics.gamma, 1.0);
    }
}
