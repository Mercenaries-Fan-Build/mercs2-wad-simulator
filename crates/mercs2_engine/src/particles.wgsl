// Camera-facing billboard particles (registry §7 FX runtime).
//
// One draw call expands N instances into camera-facing quads: 6 vertices per instance generated
// from @builtin(vertex_index), offset in world space along the camera's right/up basis (supplied in
// the uniform) so every quad faces the viewer. The fragment stage applies a soft radial falloff so
// particles read as round puffs rather than hard squares. Blending (additive vs alpha) is chosen by
// the pipeline, not the shader — both pipelines share this module.

struct Camera {
    view_proj: mat4x4<f32>,
    cam_right: vec4<f32>, // world-space camera right (xyz)
    cam_up: vec4<f32>,    // world-space camera up (xyz)
};

@group(0) @binding(0) var<uniform> cam: Camera;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) center: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
) -> VsOut {
    // Two triangles: (-1,-1)(1,-1)(1,1) and (-1,-1)(1,1)(-1,1).
    var offs = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    let o = offs[vid];
    let half = size * 0.5;
    let world = center
        + cam.cam_right.xyz * (o.x * half)
        + cam.cam_up.xyz * (o.y * half);
    var out: VsOut;
    out.clip = cam.view_proj * vec4<f32>(world, 1.0);
    out.uv = o; // -1..1 across the quad
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Soft round sprite: alpha falls off with radial distance from the quad centre.
    let r = length(in.uv);
    let falloff = clamp(1.0 - r, 0.0, 1.0);
    let a = in.color.a * falloff * falloff;
    // Straight (non-premultiplied) RGBA; the additive pipeline uses (SrcAlpha, One) so the alpha
    // weights the contribution, the alpha pipeline uses (SrcAlpha, OneMinusSrcAlpha).
    return vec4<f32>(in.color.rgb, a);
}
