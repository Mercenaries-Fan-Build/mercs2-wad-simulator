// Translucent water surface (PgWaterFP/VP screenshot-match — a flat lit plane with depth-based
// deepening + distance fog, not the retail reflection/refraction stack). One flat quad per wet
// watermap cell at its surface height; blended over the scene with the scene's fog aesthetic.

struct Uni {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,     // xyz = camera world pos
    shallow: vec4<f32>,     // rgb = shallow water tint, a = base alpha
    deep: vec4<f32>,        // rgb = deep water tint, a = fresnel alpha boost
    fog: vec4<f32>,         // rgb = fog color, a = fog density
    params: vec4<f32>,      // x = fog start (m), y = time (s), z/w unused
};

@group(0) @binding(0) var<uniform> U: Uni;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_water(@location(0) pos: vec3<f32>) -> VsOut {
    var o: VsOut;
    o.world = pos;
    o.clip = U.view_proj * vec4<f32>(pos, 1.0);
    return o;
}

@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let to_cam = U.cam_pos.xyz - in.world;
    let dist = length(to_cam);
    let view_dir = to_cam / max(dist, 1e-3);

    // Fresnel: grazing angles (view near-parallel to the flat +Y surface) read the deep/reflective
    // tint and go more opaque; looking straight down is more transparent and shallow-tinted.
    let ndotv = clamp(view_dir.y, 0.0, 1.0);
    let fresnel = pow(1.0 - ndotv, 4.0);

    // Two cheap animated wave bands ripple the surface tone so it is not a dead flat colour.
    let t = U.params.y;
    let ripple = 0.5 + 0.5 * sin(in.world.x * 0.15 + t * 1.3) * sin(in.world.z * 0.13 - t * 1.1);

    var color = mix(U.shallow.rgb, U.deep.rgb, fresnel);
    color = color * (0.85 + 0.15 * ripple);
    var alpha = clamp(U.shallow.a + U.deep.a * fresnel, 0.0, 0.95);

    // Distance fog to match the scene (exp2), so the far water dissolves into the horizon like the
    // terrain does. fog.a = density, params.x = start distance.
    let fog_d = max(dist - U.params.x, 0.0) * U.fog.a;
    let fog_amount = 1.0 - exp2(-fog_d * fog_d);
    color = mix(color, U.fog.rgb, clamp(fog_amount, 0.0, 1.0));

    return vec4<f32>(color, alpha);
}
