// Phase-1e.2b shader: textured + tangent-space normal mapping + directional light.
// Game space is left-handed, +Y up (docs/coordinate_systems.md); MVP built LH.

// Distance fog is a PLACEHOLDER for the game's real PgSky/PgSun/PgCloud shader stack.
struct Camera {
    mvp: mat4x4<f32>,
    fog_color_density: vec4<f32>, // rgb = fog color, w = density
    fog_misc: vec4<f32>,          // x = fog enable 0/1, y = fog start distance, z/w reserved
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var t_normal:  texture_2d<f32>;
@group(1) @binding(2) var s_linear:  sampler;

// Skinning palette: Skin[b] = InvBind[b]·Pose[b], row-vector, uploaded row-major so WGSL's
// column-major read yields the transpose — i.e. `bones[b] * v` computes the row-vector product
// `v · Skin[b]`. At bind pose every entry is identity (the LBS gate).
@group(2) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) n: vec3<f32>,
    @location(2) t: vec3<f32>,
    @location(3) b: vec3<f32>,
    @location(4) view_depth: f32, // clip.w = view-space depth (LH perspective proj)
    @location(5) color: vec3<f32>, // vertex color (albedo modulation; markers category-tint)
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec4<f32>,
    @location(5) joints: vec4<u32>,   // BLENDINDICES (global bone indices)
    @location(6) weights: vec4<f32>,  // BLENDWEIGHT (Unorm8x4 -> 0..1)
) -> VSOut {
    var out: VSOut;

    // Linear blend skinning in model space. cam.mvp already folds in the fit (centre/scale),
    // view and projection, so skinned positions go straight to clip space.
    var wsum = weights.x + weights.y + weights.z + weights.w;
    if (wsum <= 0.0) { wsum = 1.0; }
    // `var` (not `let`): WGSL only permits dynamic indexing of arrays in the function address space.
    var js = array<u32, 4>(joints.x, joints.y, joints.z, joints.w);
    var ws = array<f32, 4>(weights.x, weights.y, weights.z, weights.w);
    var skinned = vec4<f32>(0.0);
    var nrm = vec3<f32>(0.0);
    var tng = vec3<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let w = ws[k] / wsum;
        if (w <= 0.0) { continue; }
        let m = bones[js[k]];
        let m3 = mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz);
        skinned += w * (m * vec4<f32>(pos, 1.0));
        nrm += w * (m3 * normal);
        tng += w * (m3 * tangent.xyz);
    }

    out.clip_pos = cam.mvp * vec4<f32>(skinned.xyz, 1.0);
    out.view_depth = out.clip_pos.w;
    out.color = color;
    out.uv = uv;
    out.n = nrm;
    out.t = tng;
    out.b = cross(nrm, tng) * tangent.w; // bitangent, handedness from tangent.w
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Vertex color modulates the sampled albedo. Textured geometry (terrain/player/props) authors
    // color = (1,1,1) so this is a no-op there; the untextured placement markers use it (white
    // fallback texture) to category-tint via vertex color.
    let albedo = textureSample(t_diffuse, s_linear, in.uv).rgb * in.color;

    // Normal maps are DXT5nm/swizzled: X in ALPHA (DXT5's 8-bit alpha), Y in GREEN, Z reconstructed.
    let nsamp = textureSample(t_normal, s_linear, in.uv);
    let nx = nsamp.a * 2.0 - 1.0;
    let ny = nsamp.g * 2.0 - 1.0;
    let nz = sqrt(max(1.0 - nx * nx - ny * ny, 0.0));
    let n_tan = vec3<f32>(nx, ny, nz);
    let tbn = mat3x3<f32>(normalize(in.t), normalize(in.b), normalize(in.n));
    let N = normalize(tbn * n_tan);

    // Fixed world-space key light (upper-front-right) + ambient fill.
    let L = normalize(vec3<f32>(0.4, 0.7, -0.5));
    let ndl = max(dot(N, L), 0.0);
    let ambient = 0.35;
    let lit = albedo * (ambient + 0.9 * ndl);

    // Distance fog (PLACEHOLDER for PgSky/PgSun/PgCloud): exponential falloff past the start distance.
    var rgb = lit;
    if (cam.fog_misc.x > 0.5) {
        let f = clamp(
            1.0 - exp(-cam.fog_color_density.w * max(in.view_depth - cam.fog_misc.y, 0.0)),
            0.0, 1.0,
        );
        rgb = mix(rgb, cam.fog_color_density.rgb, f);
    }
    return vec4<f32>(rgb, 1.0);
}
