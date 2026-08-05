// Projected-decal pass — the engine realization of the retail decal draw (PgDecalVP / PgDecal2FP,
// decal_code_map §2). Each live decal from the bounded pool is projected as a depth-tested oriented
// quad (built CPU-side from the pool instance's position / surface normal / tangent / size), blended
// over the composited scene where geometry is in front of it. The retail path samples the
// `decalNormal` / `decalParam` material maps (data-only bind slots) selected by the `decaltable`
// row; those textures are confirm-live (id->name / resident-block capture), so the per-category look
// here is a procedural stand-in keyed by the recovered category index — the projection + fade + depth
// test (the mechanism) are faithful; the exact decal artwork is the confirm-live remainder.

struct Cam { view_proj: mat4x4<f32> };
@group(0) @binding(0) var<uniform> cam: Cam;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,       // centred quad UV, -1..1
    @location(1) category: f32,       // recovered decal category (0..4)
    @location(2) alpha: f32,          // pool fade alpha
};

@vertex
fn vs_decal(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) category: f32,
    @location(3) alpha: f32,
) -> VSOut {
    var out: VSOut;
    out.clip_pos = cam.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.category = category;
    out.alpha = alpha;
    return out;
}

@fragment
fn fs_decal(in: VSOut) -> @location(0) vec4<f32> {
    let r = length(in.uv);
    // Radial footprint falloff (1 at centre → 0 at the quad edge); cull outside the disc.
    let falloff = smoothstep(1.0, 0.45, r);
    if (falloff <= 0.001) { discard; }

    let cat = i32(in.category + 0.5);
    var color = vec3<f32>(0.0);
    var a = falloff;
    if (cat == 0) {
        // BulletHole: dark pockmark with a bright-ish impact rim.
        let hole = smoothstep(0.55, 0.15, r);
        let rim = smoothstep(0.15, 0.55, r) * smoothstep(0.9, 0.55, r);
        color = mix(vec3<f32>(0.20, 0.19, 0.18), vec3<f32>(0.03, 0.03, 0.03), hole);
        a = max(hole, rim * 0.6) * falloff;
    } else if (cat == 1) {
        // Blood: dark-red splat, denser at the centre.
        color = vec3<f32>(0.35, 0.02, 0.02);
        a = smoothstep(1.0, 0.2, r) * falloff;
    } else if (cat == 2) {
        // Scorch: soft dark burn.
        color = vec3<f32>(0.04, 0.035, 0.03);
        a = falloff * 0.85;
    } else if (cat == 3) {
        // TireTrack: dark streak elongated along the tangent (uv.x) axis.
        let streak = smoothstep(0.55, 0.0, abs(in.uv.y));
        color = vec3<f32>(0.05, 0.05, 0.05);
        a = streak * smoothstep(1.0, 0.2, abs(in.uv.x)) * 0.8;
    } else {
        // DamageShadow: broad, even darkening (the super damage-shadow projection).
        color = vec3<f32>(0.02, 0.02, 0.02);
        a = falloff * 0.7;
    }

    return vec4<f32>(color, clamp(a, 0.0, 1.0) * in.alpha);
}
