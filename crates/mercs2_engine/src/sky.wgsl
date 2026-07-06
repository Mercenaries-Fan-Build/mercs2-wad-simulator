// Sky / atmosphere pass — analytic Rayleigh/Mie gradient dome + sun disc, driven by the game's
// PgSky/PgSun scattering parameters (SetBetaRayMultiplier / SetBetaMieMultiplier /
// SetHenyeyGreensteinConst / SetInscatteringMultiplier / SetExtinctionMultiplier — see
// mercs2_formats::atmosphere). Rendered as a fullscreen triangle at the far plane (depth writes
// off), so world geometry draws over it. Output is HDR (linear, may exceed 1 near the sun so the
// bloom bright-pass catches it). Game space is left-handed, +Y up.

struct Sky {
    inv_view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,  // xyz = direction toward the sun (unit), w = sun-disc intensity
    horizon: vec4<f32>,  // rgb = horizon color, w = light_intensity (HDR scale)
    zenith:  vec4<f32>,  // rgb = zenith color,  w = Henyey-Greenstein g (Mie asymmetry)
    scatter: vec4<f32>,  // x = beta_ray, y = beta_mie, z = inscattering, w = extinction
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

    // Rayleigh gradient: horizon -> zenith. A softer curve near the horizon reads as haze; the
    // beta_ray term deepens the blue with altitude (more air mass scattered out low, blue up high).
    let grad = smoothstep(0.0, 0.55, up);
    var col = mix(sky.horizon.rgb, sky.zenith.rgb, grad);
    // Extra Rayleigh blue-lift with altitude, scaled by beta_ray * inscattering.
    let rayleigh = clamp(beta_ray * inscatter * 20.0 * grad, 0.0, 1.0);
    col += vec3<f32>(0.05, 0.12, 0.30) * rayleigh;

    // Mie inscatter: broad forward glow around the sun (haze halo), Henyey-Greenstein weighted.
    let cos_sun = dot(ray, normalize(sky.sun_dir.xyz));
    let mie = beta_mie * inscatter * hg_phase(cos_sun, g);
    let sun_col = vec3<f32>(1.0, 0.86, 0.65);
    col += sun_col * mie;

    // Sun disc: a tight high-intensity core (HDR) so the bloom pass blooms it.
    let disc = pow(max(cos_sun, 0.0), 2200.0);
    col += sun_col * disc * sky.sun_dir.w;

    // Overall HDR exposure by the key-light intensity.
    col *= light;
    return vec4<f32>(col, 1.0);
}
