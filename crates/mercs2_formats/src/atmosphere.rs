//! Atmosphere / sky / HDR-bloom parameter model — faithful to the game's
//! `Graphics.Atmosphere.*` runtime namespace.
//!
//! In Mercenaries 2 the sky/atmosphere and the HDR tone-map + bloom post chain are NOT a WAD
//! chunk: the world's Lua *assembles* the look by calling a named key/value engine API,
//! bracketed by `Begin()` / `End()` (see `docs/mercs2-luacd/src/resident/mrxbootstrap.lua`
//! `SetDefaultAtmosphere`, and the `airstrike_atomsphere_*` FX overrides). The engine implements
//! those setters. This module is the engine-side data model + a parser that ingests exactly that
//! command stream, so `mercs2_engine` can drive its sky/HDR/bloom from the authentic parameters.
//!
//! The namespace (verified across the decompiled Lua corpus + the `.rdata` tunable-name strings in
//! `docs/mercs2-pdb-analysis/rendering-shaders.md`):
//!   float setters   — `SetValue("f…", x)` : fBloom*, fAtmosphereForce/Limit, fLightIntensity, fTimeRestore
//!   color setters   — `SetColorValue("ui…", r,g,b,a)` (0..255) : uiAmbientColor, uiAmbientCube0..5,
//!                     uiGradient0_Color2, uiGradient1_Color1, uiRimColor
//!   typed setters   — SetTime, SetTimeSpeed, SetLightIntensity, SetAmbientColor(3×0..1),
//!                     SetAmbientCube(18×0..1), SetSky("name"),
//!                     SetInscatteringMultiplier / SetExtinctionMultiplier /
//!                     SetBetaRayMultiplier / SetBetaMieMultiplier / SetHenyeyGreensteinConst
//!
//! The `SetBeta*` / `SetHenyeyGreensteinConst` / `SetInscattering/Extinction` setters reveal a
//! Rayleigh/Mie analytic scattering sky (the same family as Preetham/Nishita): `beta_ray`/`beta_mie`
//! scattering coefficients, a Henyey–Greenstein Mie phase asymmetry `g`, and inscatter/extinction
//! multipliers. Those drive the sky shader; the `fBloom*` set drives the HDR post chain.

use std::f32::consts::PI;

/// Rayleigh/Mie analytic-scattering sky parameters (`SetBeta*` / `SetHenyeyGreensteinConst` /
/// `SetInscatteringMultiplier` / `SetExtinctionMultiplier`). Defaults = `mrxbootstrap.lua`
/// `SetDefaultAtmosphere` (the base non-VZ "afternoon" look).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterParams {
    pub beta_ray: f32,          // SetBetaRayMultiplier    (Rayleigh; blue-biased)  default 0.001
    pub beta_mie: f32,          // SetBetaMieMultiplier     (Mie; haze/sun glow)     default 0.01
    pub henyey_greenstein: f32, // SetHenyeyGreensteinConst (Mie phase asymmetry g)  default 0.9
    pub inscattering: f32,      // SetInscatteringMultiplier                          default 50.0
    pub extinction: f32,        // SetExtinctionMultiplier                            default 0.8
}

impl Default for ScatterParams {
    fn default() -> Self {
        // mrxbootstrap SetDefaultAtmosphere (base game, non-VZ).
        ScatterParams { beta_ray: 0.001, beta_mie: 0.01, henyey_greenstein: 0.9, inscattering: 50.0, extinction: 0.8 }
    }
}

/// HDR tone-map + bloom tunables (the `fBloom*` namespace). Defaults are a representative daytime
/// preset drawn from `verify_flash.lua` (the engine's true `.rdata` defaults are not in this build;
/// these read well and are trivially overridden by any world's `SetValue` calls).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BloomParams {
    pub amount: f32,               // fBloomAmount
    pub multiplier: f32,           // fBloomMultiplier
    pub threshold: f32,            // fBloomThreshold          (bright-pass cutoff)
    pub blur_radius: f32,          // fBloomBlurRadius          (0..1 → gaussian sigma scale)
    pub target_luminance: f32,     // fBloomTargetLuminance     (auto-exposure target)
    pub contrast_multiplier: f32,  // fBloomContastMultiplier   (sic — verbatim binary misspelling)
    pub contrast_limit: f32,       // fBloomContastLimit        (sic)
    pub adaptive_luminance_scale: f32,   // fBloomAdaptiveLuminanceScale
    pub adaptive_luminance_percent: f32, // fBloomAdaptiveLuminancePercent
}

impl Default for BloomParams {
    fn default() -> Self {
        BloomParams {
            amount: 0.5,
            multiplier: 0.8,
            threshold: 0.8,
            blur_radius: 0.5,
            target_luminance: 1.5,
            contrast_multiplier: 0.925,
            contrast_limit: 0.5,
            adaptive_luminance_scale: 15.0,
            adaptive_luminance_percent: 0.49,
        }
    }
}

/// The full atmosphere state a world assembles via `Graphics.Atmosphere.*`. One value per key in
/// the namespace; unknown keys are ignored on parse so the model is forward-compatible.
#[derive(Clone, Debug, PartialEq)]
pub struct Atmosphere {
    pub time: f32,              // SetTime           0..1 time of day (0.3 = afternoon default)
    pub time_speed: f32,        // SetTimeSpeed      day/night advance rate (0 = frozen)
    pub time_restore: f32,      // fTimeRestore      seconds to lerp back after an FX override
    pub light_intensity: f32,   // SetLightIntensity / fLightIntensity  (global sun/key scale)
    pub ambient_color: [f32; 3],    // SetAmbientColor / uiAmbientColor
    pub ambient_cube: [[f32; 3]; 6],// SetAmbientCube (6 faces ×RGB) — irradiance environment
    pub atmosphere_force: f32,  // fAtmosphereForce  (particulate/ash density force)
    pub atmosphere_limit: f32,  // fAtmosphereLimit  (max scatter distance)
    pub gradient0_color2: [f32; 4], // uiGradient0_Color2  (sky gradient band)
    pub gradient1_color1: [f32; 4], // uiGradient1_Color1  (sky gradient band)
    pub rim_color: [f32; 4],        // uiRimColor
    pub scatter: ScatterParams,
    pub bloom: BloomParams,
    pub sky_preset: Option<String>, // SetSky("afternoon" | "Maracaibo" | …)
}

impl Default for Atmosphere {
    fn default() -> Self {
        // mrxbootstrap.lua SetDefaultAtmosphere — the base-game default look.
        Atmosphere {
            time: 0.3,
            time_speed: 0.0,
            time_restore: 1.0,
            light_intensity: 1.0,
            ambient_color: [0.45, 0.45, 0.45],
            ambient_cube: [
                [0.42, 0.44, 0.49],
                [0.40, 0.48, 0.47],
                [0.60, 0.67, 0.68],
                [0.31, 0.27, 0.12],
                [0.30, 0.35, 0.40],
                [0.45, 0.47, 0.37],
            ],
            atmosphere_force: 0.0,
            atmosphere_limit: 100.0,
            gradient0_color2: [0.0, 0.0, 1.0, 0.0],
            gradient1_color1: [0.0, 0.0, 1.0, 0.0],
            rim_color: [0.5, 0.5, 0.5, 1.0],
            scatter: ScatterParams::default(),
            bloom: BloomParams::default(),
            sky_preset: Some("afternoon".to_string()),
        }
    }
}

impl Atmosphere {
    /// Apply one `SetValue("fKey", v)` float setter. Unknown keys ignored (returns false).
    pub fn set_value(&mut self, key: &str, v: f32) -> bool {
        match key {
            "fBloomAmount" => self.bloom.amount = v,
            "fBloomMultiplier" => self.bloom.multiplier = v,
            "fBloomThreshold" => self.bloom.threshold = v,
            "fBloomBlurRadius" => self.bloom.blur_radius = v,
            "fBloomTargetLuminance" => self.bloom.target_luminance = v,
            "fBloomContastMultiplier" => self.bloom.contrast_multiplier = v, // (sic)
            "fBloomContastLimit" => self.bloom.contrast_limit = v,           // (sic)
            "fBloomAdaptiveLuminanceScale" => self.bloom.adaptive_luminance_scale = v,
            "fBloomAdaptiveLuminancePercent" => self.bloom.adaptive_luminance_percent = v,
            "fAtmosphereForce" => self.atmosphere_force = v,
            "fAtmosphereLimit" => self.atmosphere_limit = v,
            "fLightIntensity" => self.light_intensity = v,
            "fTimeRestore" => self.time_restore = v,
            _ => return false,
        }
        true
    }

    /// Apply one `SetColorValue("uiKey", r,g,b,a)` (components 0..255) color setter. Ignored if
    /// unknown (returns false). Stored normalised to 0..1.
    pub fn set_color_value(&mut self, key: &str, r: f32, g: f32, b: f32, a: f32) -> bool {
        let c = [r / 255.0, g / 255.0, b / 255.0, a / 255.0];
        match key {
            "uiAmbientColor" => self.ambient_color = [c[0], c[1], c[2]],
            "uiAmbientCube0" => self.ambient_cube[0] = [c[0], c[1], c[2]],
            "uiAmbientCube1" => self.ambient_cube[1] = [c[0], c[1], c[2]],
            "uiAmbientCube2" => self.ambient_cube[2] = [c[0], c[1], c[2]],
            "uiAmbientCube3" => self.ambient_cube[3] = [c[0], c[1], c[2]],
            "uiAmbientCube4" => self.ambient_cube[4] = [c[0], c[1], c[2]],
            "uiAmbientCube5" => self.ambient_cube[5] = [c[0], c[1], c[2]],
            "uiGradient0_Color2" => self.gradient0_color2 = c,
            "uiGradient1_Color1" => self.gradient1_color1 = c,
            "uiRimColor" => self.rim_color = c,
            _ => return false,
        }
        true
    }

    /// Ingest a block of `Graphics.Atmosphere.*(…)` command lines — exactly the form a world's Lua
    /// issues — mutating this atmosphere. Lines that are not Atmosphere setters are skipped; this is
    /// deliberately permissive so a full Lua function body can be fed in verbatim.
    pub fn apply_script(&mut self, src: &str) {
        for line in src.lines() {
            self.apply_line(line);
        }
    }

    /// Parse a fresh atmosphere from a command block (starts from [`Default`]).
    pub fn parse_script(src: &str) -> Atmosphere {
        let mut a = Atmosphere::default();
        a.apply_script(src);
        a
    }

    /// Apply a single line if it is a `Graphics.Atmosphere.<Method>(args)` call. Returns true if the
    /// line was a recognised setter that changed state.
    pub fn apply_line(&mut self, line: &str) -> bool {
        let Some(rest) = line.trim().strip_prefix("Graphics.Atmosphere.") else { return false };
        let Some(open) = rest.find('(') else { return false };
        let method = rest[..open].trim();
        let Some(close) = rest.rfind(')') else { return false };
        let args = parse_args(&rest[open + 1..close]);
        let f = |i: usize| -> f32 { args.get(i).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0) };
        match method {
            "SetValue" if args.len() >= 2 => return self.set_value(unquote(&args[0]), f(1)),
            "SetColorValue" if args.len() >= 5 => {
                return self.set_color_value(unquote(&args[0]), f(1), f(2), f(3), f(4))
            }
            "SetTime" => self.time = f(0),
            "SetTimeSpeed" => self.time_speed = f(0),
            "SetLightIntensity" => self.light_intensity = f(0),
            "SetAmbientColor" if args.len() >= 3 => self.ambient_color = [f(0), f(1), f(2)],
            "SetAmbientCube" if args.len() >= 18 => {
                for face in 0..6 {
                    self.ambient_cube[face] = [f(face * 3), f(face * 3 + 1), f(face * 3 + 2)];
                }
            }
            "SetBetaRayMultiplier" => self.scatter.beta_ray = f(0),
            "SetBetaMieMultiplier" => self.scatter.beta_mie = f(0),
            "SetHenyeyGreensteinConst" => self.scatter.henyey_greenstein = f(0),
            "SetInscatteringMultiplier" => self.scatter.inscattering = f(0),
            "SetExtinctionMultiplier" => self.scatter.extinction = f(0),
            "SetSky" if !args.is_empty() => self.sky_preset = Some(unquote(&args[0]).to_string()),
            _ => return false,
        }
        true
    }

    /// Unit direction TOWARD the sun for the current time-of-day. `time` 0..1 maps a day arc
    /// (sunrise 0 → noon 0.5 → sunset 1); a slight +Z bias keeps the sun generally in front of the
    /// default camera. Game space is left-handed, +Y up.
    pub fn sun_dir(&self) -> [f32; 3] {
        let day = self.time.clamp(0.0, 1.0) * PI; // 0..PI across the day
        let up = day.sin().max(0.05); // elevation (never fully below horizon)
        let horiz = day.cos(); // +X in the morning → -X in the evening
        normalize3([horiz, up, 0.35])
    }

    /// Auto-exposure approximation: the game runs an adaptive-luminance loop (`PgAdaptiveLuminanceFP`)
    /// converging scene luminance toward `fBloomTargetLuminance`. We approximate the steady state with
    /// a single exposure that scales by the key-light intensity and the target-luminance ratio.
    pub fn exposure(&self) -> f32 {
        // Higher target luminance → brighter image; more key light → already bright, so less exposure.
        (self.bloom.target_luminance / self.light_intensity.max(0.05)).clamp(0.15, 8.0)
    }
}

/// ACES filmic tone-map (Narkowicz fit), per channel. Maps HDR (0..∞) → LDR (0..1).
pub fn tonemap_aces(x: f32) -> f32 {
    let (a, b, c, d, e) = (2.51, 0.03, 2.43, 0.59, 0.14);
    ((x * (a * x + b)) / (x * (c * x + d) + e)).clamp(0.0, 1.0)
}

/// Reinhard tone-map, per channel: `x / (1 + x)`.
pub fn tonemap_reinhard(x: f32) -> f32 {
    x / (1.0 + x)
}

/// Bright-pass used by the bloom bright extract: soft-knee threshold with a contrast lift, matching
/// the shader's CPU-side reference. `luma` is the pixel luminance, `color` the linear rgb.
/// Returns the rgb contribution kept for bloom.
pub fn bright_pass(color: [f32; 3], luma: f32, threshold: f32, contrast_mult: f32, contrast_limit: f32) -> [f32; 3] {
    // Soft knee around the threshold, then a contrast lift clamped by contrast_limit.
    let knee = (luma - threshold).max(0.0);
    let w = knee / (knee + 0.25); // 0 at threshold, →1 well above it (soft shoulder)
    let lift = 1.0 + (contrast_mult - 1.0).clamp(-contrast_limit, contrast_limit);
    [color[0] * w * lift, color[1] * w * lift, color[2] * w * lift]
}

/// Rec.709 luminance of a linear rgb color.
pub fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Normalised, symmetric 1-D Gaussian kernel of the given radius (in taps each side). Sums to 1.
/// `sigma` derived from the radius; used to build the separable blur weights CPU-side (and to
/// validate the shader's hard-coded weights track a real gaussian).
pub fn gaussian_kernel(radius: usize, sigma: f32) -> Vec<f32> {
    let sigma = sigma.max(1e-3);
    let n = radius * 2 + 1;
    let mut k = vec![0f32; n];
    let mut sum = 0.0;
    for i in 0..n {
        let x = i as f32 - radius as f32;
        let w = (-(x * x) / (2.0 * sigma * sigma)).exp();
        k[i] = w;
        sum += w;
    }
    for w in &mut k {
        *w /= sum;
    }
    k
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

/// Split a comma-separated argument list, respecting double-quoted strings, trimming whitespace.
fn parse_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in s.chars() {
        match ch {
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            ',' if !in_str => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_mrxbootstrap_afternoon() {
        let a = Atmosphere::default();
        assert_eq!(a.time, 0.3);
        assert_eq!(a.scatter.beta_ray, 0.001);
        assert_eq!(a.scatter.beta_mie, 0.01);
        assert_eq!(a.scatter.henyey_greenstein, 0.9);
        assert_eq!(a.scatter.inscattering, 50.0);
        assert_eq!(a.scatter.extinction, 0.8);
        assert_eq!(a.ambient_color, [0.45, 0.45, 0.45]);
        assert_eq!(a.sky_preset.as_deref(), Some("afternoon"));
    }

    #[test]
    fn parses_mrxbootstrap_default_block() {
        // Verbatim from docs/mercs2-luacd/src/resident/mrxbootstrap.lua SetDefaultAtmosphere.
        let src = r#"
  Graphics.Atmosphere.Begin()
  Graphics.Atmosphere.SetTime(0.3)
  Graphics.Atmosphere.SetSky("afternoon")
  Graphics.Atmosphere.SetTimeSpeed(0)
  Graphics.Atmosphere.SetAmbientCube(0.42, 0.44, 0.49, 0.4, 0.48, 0.47, 0.6, 0.67, 0.68, 0.31, 0.27, 0.12, 0.3, 0.35, 0.4, 0.45, 0.47, 0.37)
  Graphics.Atmosphere.SetAmbientColor(0.45, 0.45, 0.45)
  Graphics.Atmosphere.SetLightIntensity(1)
  Graphics.Atmosphere.SetInscatteringMultiplier(50)
  Graphics.Atmosphere.SetExtinctionMultiplier(0.8)
  Graphics.Atmosphere.SetBetaRayMultiplier(0.001)
  Graphics.Atmosphere.SetBetaMieMultiplier(0.01)
  Graphics.Atmosphere.SetHenyeyGreensteinConst(0.9)
  Graphics.Atmosphere.End(8)
"#;
        // Start from a NON-default to prove the parser actually sets each field.
        let mut a = Atmosphere {
            time: 9.0,
            light_intensity: 9.0,
            ambient_color: [9.0, 9.0, 9.0],
            scatter: ScatterParams { beta_ray: 9.0, beta_mie: 9.0, henyey_greenstein: 9.0, inscattering: 9.0, extinction: 9.0 },
            ..Atmosphere::default()
        };
        a.apply_script(src);
        assert_eq!(a.time, 0.3);
        assert_eq!(a.time_speed, 0.0);
        assert_eq!(a.light_intensity, 1.0);
        assert_eq!(a.ambient_color, [0.45, 0.45, 0.45]);
        assert_eq!(a.ambient_cube[0], [0.42, 0.44, 0.49]);
        assert_eq!(a.ambient_cube[5], [0.45, 0.47, 0.37]);
        assert_eq!(a.scatter.beta_ray, 0.001);
        assert_eq!(a.scatter.beta_mie, 0.01);
        assert_eq!(a.scatter.henyey_greenstein, 0.9);
        assert_eq!(a.scatter.inscattering, 50.0);
        assert_eq!(a.scatter.extinction, 0.8);
        assert_eq!(a.sky_preset.as_deref(), Some("afternoon"));
    }

    #[test]
    fn parses_verify_flash_bloom_preset() {
        // Verbatim bloom block from docs/mercs2-luacd/src/resident/verify_flash.lua.
        let src = r#"
  Graphics.Atmosphere.SetValue("fBloomAdaptiveLuminancePercent", 0.49)
  Graphics.Atmosphere.SetValue("fBloomAdaptiveLuminanceScale", 15)
  Graphics.Atmosphere.SetValue("fBloomAmount", 0.5)
  Graphics.Atmosphere.SetValue("fBloomBlurRadius", 0.5)
  Graphics.Atmosphere.SetValue("fBloomContastLimit", 0.5)
  Graphics.Atmosphere.SetValue("fBloomContastMultiplier", 0.925)
  Graphics.Atmosphere.SetValue("fBloomMultiplier", 0.8)
  Graphics.Atmosphere.SetValue("fBloomTargetLuminance", 1.5)
  Graphics.Atmosphere.SetValue("fBloomThreshold", 0.8)
  Graphics.Atmosphere.SetColorValue("uiGradient0_Color2", 0, 0, 255, 0)
  Graphics.Atmosphere.SetColorValue("uiRimColor", 128, 128, 128, 255)
"#;
        let a = Atmosphere::parse_script(src);
        assert_eq!(a.bloom.adaptive_luminance_percent, 0.49);
        assert_eq!(a.bloom.adaptive_luminance_scale, 15.0);
        assert_eq!(a.bloom.amount, 0.5);
        assert_eq!(a.bloom.blur_radius, 0.5);
        assert_eq!(a.bloom.contrast_limit, 0.5);
        assert_eq!(a.bloom.contrast_multiplier, 0.925);
        assert_eq!(a.bloom.multiplier, 0.8);
        assert_eq!(a.bloom.target_luminance, 1.5);
        assert_eq!(a.bloom.threshold, 0.8);
        assert_eq!(a.gradient0_color2, [0.0, 0.0, 1.0, 0.0]);
        // 128/255 ≈ 0.50196
        assert!((a.rim_color[0] - 0.50196).abs() < 1e-4);
        assert_eq!(a.rim_color[3], 1.0);
    }

    #[test]
    fn set_value_reports_unknown_keys() {
        let mut a = Atmosphere::default();
        assert!(a.set_value("fBloomAmount", 2.0));
        assert_eq!(a.bloom.amount, 2.0);
        assert!(!a.set_value("fNotAThing", 1.0));
        assert!(!a.set_color_value("uiNope", 1.0, 1.0, 1.0, 1.0));
    }

    #[test]
    fn ignores_non_atmosphere_lines() {
        let mut a = Atmosphere::default();
        assert!(!a.apply_line("import(\"MrxUtil\")"));
        assert!(!a.apply_line("  Event.Create(Event.TimerRelative, {0.1}, _GraphicsAto, {guid})"));
        assert!(!a.apply_line("function OnActivate(guid)"));
        assert!(a.apply_line("  Graphics.Atmosphere.SetValue(\"fBloomAmount\", 1.25)"));
        assert_eq!(a.bloom.amount, 1.25);
    }

    #[test]
    fn sun_dir_is_unit_and_tracks_time() {
        let mut a = Atmosphere::default();
        for &t in &[0.0, 0.25, 0.3, 0.5, 0.75, 1.0] {
            a.time = t;
            let d = a.sun_dir();
            let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert!((len - 1.0).abs() < 1e-4, "sun_dir not unit at t={t}");
            assert!(d[1] > 0.0, "sun below horizon at t={t}");
        }
        // Noon (0.5) sun higher than mid-afternoon default (0.3).
        a.time = 0.5;
        let noon = a.sun_dir()[1];
        a.time = 0.3;
        let aft = a.sun_dir()[1];
        assert!(noon > aft);
    }

    #[test]
    fn tonemaps_are_monotonic_and_bounded() {
        let mut prev_a = -1.0;
        let mut prev_r = -1.0;
        for i in 0..200 {
            let x = i as f32 * 0.1;
            let a = tonemap_aces(x);
            let r = tonemap_reinhard(x);
            assert!((0.0..=1.0).contains(&a));
            assert!((0.0..=1.0).contains(&r));
            assert!(a >= prev_a - 1e-6, "ACES not monotonic");
            assert!(r >= prev_r - 1e-6, "Reinhard not monotonic");
            prev_a = a;
            prev_r = r;
        }
        assert!(tonemap_aces(0.0).abs() < 1e-6);
        assert!(tonemap_reinhard(0.0).abs() < 1e-6);
    }

    #[test]
    fn bright_pass_gates_on_threshold() {
        // Below threshold → suppressed; above → passes with contrast lift.
        let dim = bright_pass([0.3, 0.3, 0.3], 0.3, 0.8, 0.925, 0.5);
        assert!(luminance(dim) < 0.01, "dim pixel should be gated out");
        let bright = bright_pass([2.0, 2.0, 2.0], 2.0, 0.8, 0.925, 0.5);
        assert!(luminance(bright) > 1.0, "bright pixel should pass");
    }

    #[test]
    fn gaussian_kernel_normalised_and_symmetric() {
        let k = gaussian_kernel(4, 2.0);
        assert_eq!(k.len(), 9);
        let sum: f32 = k.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "kernel must sum to 1");
        for i in 0..4 {
            assert!((k[i] - k[8 - i]).abs() < 1e-6, "kernel must be symmetric");
        }
        assert!(k[4] > k[0], "center weight must dominate");
    }
}
