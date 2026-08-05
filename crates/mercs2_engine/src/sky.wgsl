// Sky / atmosphere pass — the engine realization of the game's PgSky / PgSun / PgMoon / PgCloud
// stack, rendered analytically from the recovered `Graphics.Atmosphere.*` parameters
// (mercs2_formats::atmosphere): Rayleigh/Mie scattering dome (SetBetaRay/Mie, HenyeyGreenstein,
// Inscattering/Extinction), a sun disc (PgSun), a night moon (PgMoon), and a procedural scrolling
// cloud layer (PgCloud — the retail cloud pass renders cQuad0..5 billboards into a screen-space RT;
// recovering the exact PgCloudRender bytecode is confirm-live — id->name is unresolved — so the
// cloud LAYER is reimplemented analytically and tuned to look, per the plan's analytic-fallback).
// Fullscreen triangle at the far plane (depth writes off) so world geometry draws over it. Output is
// HDR (linear, may exceed 1 near the sun/moon so bloom catches it). Game space is left-handed, +Y up.

struct Sky {
    inv_view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,  // xyz = direction toward the sun (unit), w = sun-disc intensity
    horizon: vec4<f32>,  // rgb = horizon color, w = light_intensity (HDR scale)
    zenith:  vec4<f32>,  // rgb = zenith color,  w = Henyey-Greenstein g (Mie asymmetry)
    scatter: vec4<f32>,  // x = beta_ray, y = beta_mie, z = inscattering, w = extinction
    moon:    vec4<f32>,  // xyz = direction toward the moon (unit), w = moon brightness (0 by day)
    cloud:   vec4<f32>,  // x = coverage 0..1, y = scroll time (s), z = density, w = night factor 0..1
};
@group(0) @binding(0) var<uniform> sky: Sky;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
    // Fullscreen triangle from the vertex index, no vertex buffer:
    // vi 0/1/2 -> ndc (-1,-1) / (3,-1) / (-1,3).
    var out: VSOut;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    let ndc = uv * 2.0 - 1.0;
    // z = w so post-divide depth is 1.0 (far plane); LessEqual lets it pass the depth clear.
    out.clip_pos = vec4<f32>(ndc, 1.0, 1.0);
    out.ndc = ndc;
    return out;
}

const PI: f32 = 3.14159265;

// Henyey-Greenstein phase function (Mie forward-scatter lobe toward the sun).
fn hg_phase(cos_theta: f32, g: f32) -> f32 {
    let g2 = g * g;
    let denom = pow(max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4), 1.5);
    return (1.0 - g2) / (4.0 * PI * denom);
}

// --- value-noise fBm for the cloud layer (cheap, tileable enough for a scrolling sky) ---
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f); // smoothstep interpolant
    let a = hash2(i + vec2<f32>(0.0, 0.0));
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var amp = 0.5;
    var sum = 0.0;
    for (var o = 0; o < 5; o = o + 1) {
        sum = sum + amp * vnoise(p);
        p = p * 2.02;
        amp = amp * 0.5;
    }
    return sum;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Reconstruct the world-space view ray by unprojecting at the near & far clip planes.
    let p_near = sky.inv_view_proj * vec4<f32>(in.ndc, 0.0, 1.0);
    let p_far = sky.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let ray = normalize(p_far.xyz / p_far.w - p_near.xyz / p_near.w);

    let up = max(ray.y, 0.0);
    let light = max(sky.horizon.w, 0.05);
    let beta_ray = sky.scatter.x;
    let beta_mie = sky.scatter.y;
    let inscatter = sky.scatter.z;
    let g = clamp(sky.zenith.w, -0.95, 0.95);
    let night = clamp(sky.cloud.w, 0.0, 1.0);

    // Rayleigh gradient: horizon -> zenith. A softer curve near the horizon reads as haze; the
    // beta_ray term deepens the blue with altitude. At night the whole dome darkens toward a deep
    // blue so the moon + stars read (PgSky's time-of-day tint, realized as a night lerp).
    let grad = smoothstep(0.0, 0.55, up);
    let horizon_c = mix(sky.horizon.rgb, sky.horizon.rgb * 0.06 + vec3<f32>(0.01, 0.02, 0.05), night);
    let zenith_c = mix(sky.zenith.rgb, vec3<f32>(0.01, 0.02, 0.06), night);
    var col = mix(horizon_c, zenith_c, grad);
    // Extra Rayleigh blue-lift with altitude, scaled by beta_ray * inscattering.
    let rayleigh = clamp(beta_ray * inscatter * 20.0 * grad, 0.0, 1.0);
    col += vec3<f32>(0.05, 0.12, 0.30) * rayleigh * (1.0 - night);

    // Mie inscatter: broad forward glow around the sun (haze halo), Henyey-Greenstein weighted.
    let cos_sun = dot(ray, normalize(sky.sun_dir.xyz));
    let mie = beta_mie * inscatter * hg_phase(cos_sun, g);
    let sun_col = vec3<f32>(1.0, 0.86, 0.65);
    col += sun_col * mie * (1.0 - night * 0.85);

    // --- PgCloud layer: scrolling fBm, sun/moon-lit, only where sky is visible (up > 0). ---
    if (up > 0.02) {
        // Project the ray onto a virtual cloud plane at a fixed height; scale so clouds sit at a
        // plausible apparent size, and scroll with wind over time.
        let plane = ray.xz / max(ray.y, 0.15);
        let scroll = vec2<f32>(sky.cloud.y * 0.010, sky.cloud.y * 0.004);
        let uv = plane * 0.9 + scroll;
        var d = fbm(uv);
        d = fbm(uv + vec2<f32>(d, d) * 0.6); // domain-warp for softer, billowing shapes
        // Coverage: higher `coverage` lowers the threshold so more sky fills in.
        let cover = clamp(sky.cloud.x, 0.0, 0.95);
        let edge = 1.0 - cover;
        var cloud_a = smoothstep(edge, edge + 0.25, d) * clamp(sky.cloud.z, 0.0, 2.0);
        // Fade clouds out toward the horizon (atmospheric extinction) and where the dome is thin.
        cloud_a *= smoothstep(0.02, 0.30, up);
        // Cloud shading: bright sun-lit tops by day, dim blue-grey by night; a touch of forward
        // scatter brightens clouds near the sun/moon.
        let key = mix(normalize(sky.sun_dir.xyz), normalize(sky.moon.xyz), night);
        let toward = max(dot(ray, key), 0.0);
        let day_c = mix(vec3<f32>(0.55, 0.57, 0.62), vec3<f32>(1.0, 0.97, 0.90), toward);
        let night_c = mix(vec3<f32>(0.05, 0.06, 0.10), vec3<f32>(0.30, 0.32, 0.40), toward);
        let cloud_c = mix(day_c, night_c, night) * light;
        col = mix(col, cloud_c, clamp(cloud_a, 0.0, 1.0));
    }

    // Sun disc: a tight high-intensity core (HDR) so the bloom pass blooms it (dimmed at night).
    let disc = pow(max(cos_sun, 0.0), 2200.0);
    col += sun_col * disc * sky.sun_dir.w * (1.0 - night);

    // --- PgMoon: a soft disc + halo opposite the sun, brightening at night. ---
    if (sky.moon.w > 0.001) {
        let cos_moon = dot(ray, normalize(sky.moon.xyz));
        let moon_col = vec3<f32>(0.80, 0.85, 1.0);
        let moon_disc = pow(max(cos_moon, 0.0), 3500.0);
        let moon_halo = pow(max(cos_moon, 0.0), 40.0) * 0.05;
        col += moon_col * (moon_disc + moon_halo) * sky.moon.w;
    }

    // Overall HDR exposure by the key-light intensity.
    col *= light;
    return vec4<f32>(col, 1.0);
}
