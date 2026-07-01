// Phase-1e.2b shader: textured + tangent-space normal mapping + directional light.
// Game space is left-handed, +Y up (docs/coordinate_systems.md); MVP built LH.

struct Camera {
    mvp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var t_diffuse: texture_2d<f32>;
@group(1) @binding(1) var t_normal:  texture_2d<f32>;
@group(1) @binding(2) var s_linear:  sampler;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) n: vec3<f32>,
    @location(2) t: vec3<f32>,
    @location(3) b: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) normal: vec3<f32>,
    @location(4) tangent: vec4<f32>,
) -> VSOut {
    var out: VSOut;
    out.clip_pos = cam.mvp * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.n = normal;
    out.t = tangent.xyz;
    out.b = cross(normal, tangent.xyz) * tangent.w; // bitangent, handedness from tangent.w
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let albedo = textureSample(t_diffuse, s_linear, in.uv).rgb;

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
    return vec4<f32>(lit, 1.0);
}
