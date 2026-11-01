// The retail effect sprite: `PgFXVP` + `PgFXFP` (shaderforge records 346, 344), from the `vfx` atlas.
//
//   D      = right * c.x + up * c.y            c in (+-0.5, +-0.5)
//   offset = D * cos + cross(D, forward) * sin
//   pos    = centre + offset * size
//   colour = rgb * abs(intensity), alpha unchanged
//   write  = intensity < 0 ? 0 : 1
//   out    = (rgb * a, a * write)
//
// So a negative intensity writes alpha 0, which adds under the premultiplied blend the pipeline sets
// (One, OneMinusSrcAlpha). The blend factors are INFERRED from this premultiply. The retail inscatter
// term and `atten` are not ported.

struct Camera {
    view_proj: mat4x4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    // xyz: camera forward; w: 1 when the target encodes sRGB, whose blend needs the values decoded.
    cam_forward: vec4<f32>,
};

@group(0) @binding(0) var<uniform> cam: Camera;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) write_alpha: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @location(0) center: vec3<f32>,
    @location(1) size: f32,
    @location(2) color: vec4<f32>,
    @location(3) rect: vec4<f32>,
    @location(4) sin_cos: vec2<f32>,
    @location(5) intensity: f32,
) -> VsOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5,  0.5),
        vec2<f32>(-0.5,  0.5),
    );
    let c = corners[vid];
    let d = cam.cam_right.xyz * c.x + cam.cam_up.xyz * c.y;
    let offset = d * sin_cos.y + cross(d, cam.cam_forward.xyz) * sin_cos.x;
    var out: VsOut;
    out.clip = cam.view_proj * vec4<f32>(center + offset * size, 1.0);
    out.uv = rect.xy + rect.zw * vec2<f32>(c.x + 0.5, 0.5 - c.y);
    out.color = vec4<f32>(color.rgb * abs(intensity), color.a);
    out.write_alpha = select(1.0, 0.0, intensity < 0.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, atlas_sampler, in.uv);
    let a = t.a * in.color.a;
    var rgb = clamp(t.rgb * in.color.rgb, vec3<f32>(0.00001), vec3<f32>(30000.0)) * a;
    if (cam.cam_forward.w > 0.5) {
        rgb = pow(rgb, vec3<f32>(2.2));
    }
    return vec4<f32>(rgb, a * in.write_alpha);
}
