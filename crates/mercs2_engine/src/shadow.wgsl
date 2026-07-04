// Depth-only directional shadow pass. Renders the scene geometry from the key light's point of view
// into a Depth32Float shadow map; the main shader (shader.wgsl) PCF-samples it to cast real shadows.
//
// The skinning here MUST match vs_main's linear-blend skinning byte-for-byte — any divergence would
// make the character's shadow a distorted bind-pose blob. Only the final projection differs: instead
// of the camera MVP we project the world position with the light view-proj. No fragment stage — depth
// is the only output.

// Reuse the camera uniform (group 0) purely for `model` (model -> WORLD). Binding 0 is already
// VERTEX-visible in camera_bgl (vs_main reads cam.mvp there), so no layout change is needed.
struct Camera {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
    cam_pos: vec4<f32>,
    fog_color_density: vec4<f32>,
    fog_misc: vec4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

// Group 1: the light view-proj for this frame (its own small bind group; the shadow pass needs no
// textures, so group 1 is free here — unlike the color pass where group 1 is the material).
struct ShadowVp { mat: mat4x4<f32> };
@group(1) @binding(0) var<uniform> light: ShadowVp;

// Group 2: the per-entity bone palette (same storage buffer the color pass skins with).
@group(2) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

@vertex
fn vs_shadow(
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) joints: vec4<u32>,   // BLENDINDICES
    @location(6) weights: vec4<f32>,  // BLENDWEIGHT
) -> @builtin(position) vec4<f32> {
    // Linear blend skinning in model space — VERBATIM from vs_main (position only; the shadow map
    // needs no normals/tangents/uv).
    var wsum = weights.x + weights.y + weights.z + weights.w;
    if (wsum <= 0.0) { wsum = 1.0; }
    var js = array<u32, 4>(joints.x, joints.y, joints.z, joints.w);
    var ws = array<f32, 4>(weights.x, weights.y, weights.z, weights.w);
    var skinned = vec4<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let w = ws[k] / wsum;
        if (w <= 0.0) { continue; }
        let m = bones[js[k]];
        skinned += w * (m * vec4<f32>(pos, 1.0));
    }
    // model -> WORLD (fit is folded into cam.model upstream), then WORLD -> light clip space.
    let world = cam.model * vec4<f32>(skinned.xyz, 1.0);
    return light.mat * world;
}
