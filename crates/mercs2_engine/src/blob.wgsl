// Blob / contact-shadow fallback (`FUN_00853710`, shadow_code_map.md §5). Flat radial-falloff dark
// discs in the world XZ plane, drawn under casters the 4-cascade atlas doesn't cover (beyond shadow
// distance). The pipeline uses a darken blend (out = dst * (1 - a)); this shader outputs rgb = 0 and
// a = darkness * radial-falloff, so a blob softly darkens the ground under the object. The exact
// `ShadowK` darkness constant + projection are confirm-live (the blob render is vtable-reached in the
// exe); this is a faithful reconstruction of the visible behaviour.

// group 0: the frame's (handedness-flipped) camera view-projection — the same matrix the geometry
// pass uses, so blob quads register with the world.
@group(0) @binding(0) var<uniform> view_proj: mat4x4<f32>;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,        // centred quad coord (-1..1) for the radial falloff
    @location(1) darkness: f32,        // per-blob ShadowK
};

@vertex
fn vs_blob(
    @location(0) pos: vec3<f32>,       // world position (XZ plane, Y = ground)
    @location(1) uv: vec2<f32>,
    @location(2) darkness: f32,
) -> VSOut {
    var out: VSOut;
    out.clip_pos = view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.darkness = darkness;
    return out;
}

@fragment
fn fs_blob(in: VSOut) -> @location(0) vec4<f32> {
    // Radial falloff: opaque at the centre, fading to 0 at the disc edge (r = 1). Squared for a soft
    // penumbra-like edge.
    let r = length(in.uv);
    let f = clamp(1.0 - r, 0.0, 1.0);
    let a = in.darkness * f * f;
    // rgb is ignored by the darken blend (srcFactor = Zero); alpha carries the darkening amount.
    return vec4<f32>(0.0, 0.0, 0.0, a);
}
