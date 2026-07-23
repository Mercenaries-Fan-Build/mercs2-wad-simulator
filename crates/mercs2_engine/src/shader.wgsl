// Phase-1e.2c shader: textured + tangent-space normal mapping + Blinn-Phong specular + a sun/ambient
// term + a forward array of up to 32 dynamic point lights (radius/distance attenuated).
// Game space is left-handed, +Y up (docs/coordinate_systems.md); MVP built LH.

// Distance fog is a PLACEHOLDER for the game's real PgSky/PgSun/PgCloud shader stack.
struct Camera {
    mvp: mat4x4<f32>,             // model -> clip (fit * view * proj folded)
    model: mat4x4<f32>,          // model -> WORLD (for world-space position + normals; lights live here)
    cam_pos: vec4<f32>,          // xyz = camera world position (for specular view vector)
    fog_color_density: vec4<f32>, // rgb = fog color, w = density
    fog_misc: vec4<f32>,          // x = fog enable 0/1, y = fog start distance, z = prelit, w = sun intensity
    overlay: vec4<f32>,           // x = model alpha (1 = opaque), yzw reserved
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
    count: vec4<u32>,            // x = active point-light count; y = ShaderLevel gate (0/1/2/3)
    items: array<GpuLight, 32>,
};
@group(3) @binding(0) var<uniform> lights: Lights;

// 4-cascade directional shadow ATLAS (1024×4096 = four stacked 1024² tiles), folded into group 3 so
// the shader stays within wgpu's 4-GROUP limit. A depth-only render of the scene from the key light,
// per cascade, PCF-sampled to cast real cast-shadows (faithful to FUN_00755d90 + the 4× RenderShadow
// emit; shadow_code_map.md §1/§4).
@group(3) @binding(1) var shadow_map: texture_depth_2d;
@group(3) @binding(2) var shadow_cmp: sampler_comparison;
// 4 cascade light view-projs (LH look_at_lh * orthographic_lh, NO cam X-flip) + a params vec4
// (x = shadow-strength floor, z = cascade count, w = configured 0/1).
struct ShadowParams {
    cascades: array<mat4x4<f32>, 4>,
    params: vec4<f32>,
};
@group(3) @binding(3) var<uniform> shadowp: ShadowParams;

// Per-pixel SPOT light set (`_sl` / `_pl_sl` class). Evaluated only when the ShaderLevel gate admits
// spots (level >= 2). pos_range: xyz pos, w range. dir_cos: xyz cone axis, w = cos(outer half-angle).
// params: x = cos(inner half-angle).
struct SpotLight {
    pos_range: vec4<f32>,
    color_intensity: vec4<f32>,
    dir_cos: vec4<f32>,
    params: vec4<f32>,
};
struct Spots {
    count: vec4<u32>,           // x = active spot count
    items: array<SpotLight, 16>,
};
@group(3) @binding(4) var<uniform> spots: Spots;

// 3×3 PCF sample of ONE atlas tile (cascade `c`). `uv_tile` is the fragment's [0,1] UV within the
// tile; the atlas stacks 4 tiles vertically, so tile `c` maps to atlas-v in [c*0.25, (c+1)*0.25].
// PCF texel steps: 1/1024 in u, 1/4096 in v (a whole atlas is 1024×4096). No derivatives → valid under
// early-out control flow.
fn sample_cascade(uv_tile: vec2<f32>, c: i32, ref_depth: f32) -> f32 {
    let base_v = f32(c) * 0.25;
    let tx = 1.0 / 1024.0;
    let ty = 1.0 / 4096.0;
    var sum = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let auv = vec2<f32>(uv_tile.x + f32(x) * tx, base_v + uv_tile.y * 0.25 + f32(y) * ty);
            sum += textureSampleCompareLevel(shadow_map, shadow_cmp, auv, ref_depth);
        }
    }
    return sum / 9.0;
}

// 4-cascade PCF shadow factor for a WORLD-space fragment: 1 = fully lit, 0 = fully shadowed. Picks the
// SMALLEST (tightest) cascade whose light-clip box contains the fragment (crisp near, wide far), then
// PCF-samples that cascade's atlas tile. Fragments outside every cascade read as lit (a blob fallback
// grounds those casters instead — see scene.rs record_blob).
fn shadow_factor(wpos: vec3<f32>) -> f32 {
    let bias = 0.0015; // constant depth bias vs acne (pairs with the pipeline slope bias)
    for (var c = 0; c < 4; c = c + 1) {
        let lc = shadowp.cascades[c] * vec4<f32>(wpos, 1.0);
        if (lc.w <= 0.0) { continue; }
        let ndc = lc.xyz / lc.w;
        // orthographic_lh maps z to [0,1] (wgpu depth range), so ndc.z is the compare ref directly.
        let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
        if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || ndc.z > 1.0 || ndc.z < 0.0) {
            continue; // not in this cascade — try the next (wider) one
        }
        return sample_cascade(uv, c, ndc.z - bias);
    }
    return 1.0; // outside all cascades -> treat as lit
}

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

    // Fixed world-space key light (upper-front-right) + ambient fill — the sun term. PRELIT static
    // geometry (interiors/buildings) bakes its lighting into the vertex COLOR, so the exterior sun
    // must NOT be stamped over it — the baked term IS the lighting. `fog_misc.z` (prelit flag,
    // 1 = baked) selects: baked geometry uses `albedo` (baked light) alone; dynamic geometry
    // (characters/props, white vertex color) keeps the sun+ambient key light unchanged. Dynamic point
    // lights (below) add to both.
    let prelit = cam.fog_misc.z;
    let sun_dir = normalize(vec3<f32>(0.4, 0.7, -0.5));
    // Sun intensity (`fog_misc.w`) and ambient (`cam_pos.w`) are SCENE-controlled: the exterior sets a
    // bright sun; the INTERIOR sets sun = 0 (no sun indoors) and a higher ambient fill, so the room is
    // lit only by baked vertex colour + the interior point lights, not a phantom outdoor sun.
    let sun_i = cam.fog_misc.w;
    let ambient = cam.cam_pos.w;
    let sun_ndl = max(dot(N, sun_dir), 0.0);
    // Split lighting into an unshadowed FLOOR (ambient / baked) and the shadowable DIRECT term, so the
    // shadow map darkens only direct light — shadowed areas keep the floor and never go pure black.
    // Prelit static geometry bakes its lighting into vertex COLOR, so its baked albedo IS the floor and
    // it takes no sun key light; dynamic geometry (characters/props) uses ambient*albedo as the floor
    // plus the sun key light (when the scene enables it) as its direct term.
    // Baked vertex lighting reads bright (hall mean ~0.79); scale the prelit baked term down so
    // interiors aren't washed out. (0.7 = baked-lighting brightness knob.)
    let baked_scale = 0.21;
    let ambient_floor = mix(albedo * ambient, albedo * baked_scale, prelit);
    var direct = albedo * (sun_i * sun_ndl) * (1.0 - prelit);
    if (sun_ndl > 0.0 && prelit < 0.5 && sun_i > 0.0) {
        let sun_h = normalize(sun_dir + V);
        direct += spec_mask * pow(max(dot(N, sun_h), 0.0), spec_power);
    }

    // Per-pixel dynamic lights, gated by the ShaderLevel light-class permutation (the `DAT_00dfc345`
    // gate → the `.sho` variant the exe registers per material): count.y = 0 base (none) / 1 `_pl`
    // (point) / 2 `_sl` (spot) / 3 `_pl_sl` (both). We realize the four compiled permutations as a
    // runtime branch on the level (same visible result; one pipeline).
    // NOTE: the exact per-pixel falloff/cone math is VMX128 in the exe and does not decode from the PPC
    // dump — the Blinn-Phong + windowed falloff + smoothstep cone below are a faithful reconstruction.
    // CONFIRM-LIVE: break the `_pl`/`_sl` fragment shaders in x32dbg to recover the exact attenuation
    // curve, cone falloff, and specular model.
    let shader_level = lights.count.y;
    let do_point = (shader_level == 1u) || (shader_level == 3u); // `_pl` / `_pl_sl`
    let do_spot  = (shader_level == 2u) || (shader_level == 3u); // `_sl` / `_pl_sl`

    // `_pl` — dynamic POINT lights: the nearest N (uploaded per frame). Smooth radius falloff +
    // Blinn-Phong. These add to the shadowable direct term (a caster occluding the key light darkens
    // them too).
    if (do_point) {
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
            direct += albedo * lcol * (linten * ndl * att);
            if (ndl > 0.0) {
                let H = normalize(Ld + V);
                let ndh = max(dot(N, H), 0.0);
                direct += spec_mask * lcol * (linten * att * pow(ndh, spec_power));
            }
        }
    }

    // `_sl` — dynamic SPOT lights: point falloff × a smooth cone gate between the inner/outer half-angle
    // cosines. CONFIRM-LIVE: the exact cone attenuation curve is VMX128-only in the exe.
    if (do_spot) {
        let scount = min(spots.count.x, 16u);
        for (var i = 0u; i < scount; i = i + 1u) {
            let sp = spots.items[i].pos_range.xyz;
            let sr = max(spots.items[i].pos_range.w, 1e-3);
            let scol = spots.items[i].color_intensity.rgb;
            let sinten = spots.items[i].color_intensity.w;
            let axis = spots.items[i].dir_cos.xyz;    // cone axis (spot points along this)
            let cos_outer = spots.items[i].dir_cos.w;
            let cos_inner = spots.items[i].params.x;
            let d = sp - in.wpos;
            let dist = length(d);
            if (dist >= sr) { continue; }
            let Ld = d / max(dist, 1e-4);
            // Cone gate: cos of the angle between the light->fragment dir and the spot axis. `-Ld` is
            // the light-to-fragment direction; compare against the axis it points along.
            let cang = dot(-Ld, normalize(axis));
            let cone = smoothstep(cos_outer, cos_inner, cang);
            if (cone <= 0.0) { continue; }
            let x = dist / sr;
            let atten = (1.0 - x * x);
            let att = atten * atten * cone;
            let ndl = max(dot(N, Ld), 0.0);
            direct += albedo * scol * (sinten * ndl * att);
            if (ndl > 0.0) {
                let H = normalize(Ld + V);
                let ndh = max(dot(N, H), 0.0);
                direct += spec_mask * scol * (sinten * att * pow(ndh, spec_power));
            }
        }
    }

    // Directional shadow — ONLY when there is a sun (sun_i > 0). Indoors the sun is off, so there is no
    // directional light and therefore NO directional shadow (a shadow with no sun reads as a phantom
    // noon sun). Outdoors the 4-cascade atlas (dynamic casters only) darkens the result, clamped to a
    // floor (shadowp.params.x, default 0.35) so shadowed areas darken rather than blacken.
    var shadow = 1.0;
    if (sun_i > 0.0) {
        shadow = max(shadow_factor(in.wpos), shadowp.params.x);
    }
    var lit = (ambient_floor + direct) * shadow;

    // Distance fog (PLACEHOLDER for PgSky/PgSun/PgCloud): exponential falloff past the start distance.
    var rgb = lit;
    if (cam.fog_misc.x > 0.5) {
        let f = clamp(
            1.0 - exp(-cam.fog_color_density.w * max(in.view_depth - cam.fog_misc.y, 0.0)),
            0.0, 1.0,
        );
        rgb = mix(rgb, cam.fog_color_density.rgb, f);
    }
    return vec4<f32>(rgb, cam.overlay.x);
}
