// PLACEHOLDER sky pass for the game's real PgSky/PgSun/PgCloud shader stack:
// a fullscreen-triangle gradient dome (horizon -> zenith) + a subtle sun glow.
// Drawn first, at depth 1.0 (far plane), depth writes off, so geometry covers it.
// Game space is left-handed, +Y up (docs/coordinate_systems.md).

struct Sky {
    inv_view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>, // xyz = direction toward the sun (normalized), w reserved
    params: vec4<f32>,  // rgb = horizon color (matches the fog color), w reserved
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
    // z = w so the post-divide depth is 1.0 (far plane); LessEqual lets it pass the clear.
    out.clip_pos = vec4<f32>(ndc, 1.0, 1.0);
    out.ndc = ndc;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Reconstruct the world-space view ray by unprojecting this pixel at the near
    // and far clip planes (wgpu depth range 0..1).
    let p_near = sky.inv_view_proj * vec4<f32>(in.ndc, 0.0, 1.0);
    let p_far = sky.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let ray = normalize(p_far.xyz / p_far.w - p_near.xyz / p_near.w);

    // Gradient dome: horizon color (= fog color) blending to a deeper-blue zenith.
    // Below the horizon stays at the horizon color (max(ray.y, 0)).
    let horizon = sky.params.rgb;
    let zenith = vec3<f32>(0.18, 0.32, 0.55);
    let t = smoothstep(0.0, 0.45, max(ray.y, 0.0));
    var rgb = mix(horizon, zenith, t);

    // Simple sun disc: tight white-ish glow toward sun_dir, kept subtle.
    let s = pow(max(dot(ray, sky.sun_dir.xyz), 0.0), 512.0);
    rgb += vec3<f32>(1.0, 0.95, 0.85) * s * 0.8;

    return vec4<f32>(rgb, 1.0);
}
