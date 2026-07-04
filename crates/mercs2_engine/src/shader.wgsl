// Phase-1e.2c shader: textured + tangent-space normal mapping + Blinn-Phong specular + a sun/ambient
// term + a forward array of up to 32 dynamic point lights (radius/distance attenuated).
// Game space is left-handed, +Y up (docs/coordinate_systems.md); MVP built LH.

// Distance fog is a PLACEHOLDER for the game's real PgSky/PgSun/PgCloud shader stack.
struct Camera {
    mvp: mat4x4<f32>,             // model -> clip (fit * view * proj folded)
    model: mat4x4<f32>,          // model -> WORLD (for world-space position + normals; lights live here)
    cam_pos: vec4<f32>,          // xyz = camera world position (for specular view vector)
    fog_color_density: vec4<f32>, // rgb = fog color, w = density
    fog_misc: vec4<f32>,          // x = fog enable 0/1, y = fog start distance, z/w reserved
};
@group(0) @binding(0) var<uniform> cam: Camera;

@group(1) @binding(0) var t_diffuse:  texture_2d<f32>;
@group(1) @binding(1) var t_normal:   texture_2d<f32>;
@group(1) @binding(2) var s_linear:   sampler;
@group(1) @binding(3) var t_specular: texture_2d<f32>; // MTRL slot 1 (`_sm`); black = matte

// Skinning palette: Skin[b] = InvBind[b]·Pose[b], row-vector, uploaded row-major so WGSL's
// column-major read yields the transpose — i.e. `bones[b] * v` computes the row-vector product
// `v · Skin[b]`. At bind pose every entry is identity (the LBS gate).
@group(2) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

// group 3: the per-frame dynamic light set. `count.x` = number of active lights (<= 32); the rest of
// the array is ignored. Positions/radii are in WORLD space, matching `cam.model` and `cam.cam_pos`.
struct GpuLight {
    pos_radius: vec4<f32>,       // xyz = world pos, w = radius
    color_intensity: vec4<f32>,  // rgb = color, w = intensity
};
struct Lights {
    count: vec4<u32>,            // x = active light count
    items: array<GpuLight, 32>,
};
@group(3) @binding(0) var<uniform> lights: Lights;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) n: vec3<f32>,       // WORLD-space normal
    @location(2) t: vec3<f32>,       // WORLD-space tangent
    @location(3) b: vec3<f32>,       // WORLD-space bitangent
    @location(4) view_depth: f32,    // clip.w = view-space depth (LH perspective proj)
    @location(5) color: vec3<f32>,   // vertex color (albedo modulation; markers category-tint)
    @location(6) wpos: vec3<f32>,    // WORLD-space fragment position (point-light attenuation)
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
    // World-space TBN: rotate the model-space skinned basis by the model->world upper 3x3 so the
    // point lights (world space) shade correctly. In the streaming/world path `cam.model` is the
    // entity Transform (fit = identity), so this is true game-world space.
    let mw = mat3x3<f32>(cam.model[0].xyz, cam.model[1].xyz, cam.model[2].xyz);
    out.n = mw * nrm;
    out.t = mw * tng;
    out.b = cross(out.n, out.t) * tangent.w; // bitangent, handedness from tangent.w
    out.wpos = (cam.model * vec4<f32>(skinned.xyz, 1.0)).xyz;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Vertex color modulates the sampled albedo. Textured geometry (terrain/player/props) authors
    // color = (1,1,1) so this is a no-op there; the untextured placement markers use it (white
    // fallback texture) to category-tint via vertex color.
    let tex = textureSample(t_diffuse, s_linear, in.uv);
    // Alpha-tested cutout. Opaque diffuse (DXT1 / authored a=1, and the white fallback a=1) reads
    // alpha ≈ 1 and never discards; DXT5 / 1-bit-alpha diffuse carries the cutout mask, so foliage,
    // railings, fences and window/arch openings in the building shells render see-through instead of
    // as solid quads. (True alpha-BLEND — glass — is a later, sort-order pass.)
    if (tex.a < 0.5) { discard; }
    let albedo = tex.rgb * in.color;

    // Normal maps are DXT5nm/swizzled: X in ALPHA (DXT5's 8-bit alpha), Y in GREEN, Z reconstructed.
    let nsamp = textureSample(t_normal, s_linear, in.uv);
    let nx = nsamp.a * 2.0 - 1.0;
    let ny = nsamp.g * 2.0 - 1.0;
    let nz = sqrt(max(1.0 - nx * nx - ny * ny, 0.0));
    let n_tan = vec3<f32>(nx, ny, nz);
    let tbn = mat3x3<f32>(normalize(in.t), normalize(in.b), normalize(in.n));
    let N = normalize(tbn * n_tan);

    // Specular mask from the `_sm` map (slot 1). Black fallback -> no highlight (matte).
    let spec_mask = textureSample(t_specular, s_linear, in.uv).rgb;
    let spec_power = 48.0; // fixed Blinn-Phong gloss exponent (per-material gloss is a later refinement)

    // View vector (world space) for Blinn-Phong.
    let V = normalize(cam.cam_pos.xyz - in.wpos);

    // Fixed world-space key light (upper-front-right) + ambient fill — the sun term. Kept identical
    // to the previous baseline so the character viewer (no dynamic lights) is unchanged, plus a
    // sun specular lobe gated by the spec map.
    let sun_dir = normalize(vec3<f32>(0.4, 0.7, -0.5));
    let ambient = 0.35;
    let sun_ndl = max(dot(N, sun_dir), 0.0);
    var lit = albedo * (ambient + 0.9 * sun_ndl);
    if (sun_ndl > 0.0) {
        let sun_h = normalize(sun_dir + V);
        lit += spec_mask * pow(max(dot(N, sun_h), 0.0), spec_power);
    }

    // Dynamic point lights: the nearest N (uploaded per frame). Smooth radius falloff + Blinn-Phong.
    let count = min(lights.count.x, 32u);
    for (var i = 0u; i < count; i = i + 1u) {
        let lp = lights.items[i].pos_radius.xyz;
        let lr = max(lights.items[i].pos_radius.w, 1e-3);
        let lcol = lights.items[i].color_intensity.rgb;
        let linten = lights.items[i].color_intensity.w;
        let d = lp - in.wpos;
        let dist = length(d);
        if (dist >= lr) { continue; }
        let Ld = d / max(dist, 1e-4);
        // Windowed inverse-square-ish falloff: (1 - (dist/lr)^2)^2, cheap and edge-clean.
        let x = dist / lr;
        let atten = (1.0 - x * x);
        let att = atten * atten;
        let ndl = max(dot(N, Ld), 0.0);
        lit += albedo * lcol * (linten * ndl * att);
        if (ndl > 0.0) {
            let H = normalize(Ld + V);
            let ndh = max(dot(N, H), 0.0);
            lit += spec_mask * lcol * (linten * att * pow(ndh, spec_power));
        }
    }

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
