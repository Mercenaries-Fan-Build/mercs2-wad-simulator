// HDR post chain: bright-pass -> separable gaussian blur (ping-pong) -> composite + ACES/Reinhard
// tone-map. Faithful to the game's PgBloomCombiner / PgToneMapping / PgBlurH+V / PgDownSample stack
// and the fBloom* tunables (mercs2_formats::atmosphere::BloomParams). All three fragment programs
// share ONE uniform layout (PostU) + ONE two-texture group so the pipelines reuse bind-group
// layouts; where a pass needs only one texture the caller binds the same view to both slots.

struct PostU {
    threshold: f32,      // fBloomThreshold      (bright-pass cutoff)
    contrast_mult: f32,  // fBloomContastMultiplier (sic)
    contrast_limit: f32, // fBloomContastLimit (sic)
    amount: f32,         // fBloomAmount
    multiplier: f32,     // fBloomMultiplier
    exposure: f32,       // adaptive-luminance-derived exposure
    tonemap_mode: f32,   // 0 = ACES, 1 = Reinhard
    _pad0: f32,
    blur_dir: vec2<f32>, // (1,0) horizontal / (0,1) vertical
    texel: vec2<f32>,    // per-tap step in UV = 1/source_size, scaled by fBloomBlurRadius
};
@group(0) @binding(0) var<uniform> u: PostU;

@group(1) @binding(0) var tex_a: texture_2d<f32>; // primary source (hdr / blur src)
@group(1) @binding(1) var tex_b: texture_2d<f32>; // secondary (bloom, composite only)
@group(1) @binding(2) var samp: sampler;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
    var out: VSOut;
    let p = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u)); // 0/1/2 -> (0,0)(2,0)(0,2)
    out.uv = p; // 0..1 (>1 falls outside the triangle, clipped)
    out.clip_pos = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    // Flip Y so uv (0,0) is top-left of the target (matches texture sampling convention).
    out.uv.y = 1.0 - out.uv.y;
    return out;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

// Bright-pass: soft-knee threshold + contrast lift. Mirrors atmosphere::bright_pass (CPU reference).
@fragment
fn fs_bright(in: VSOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex_a, samp, in.uv).rgb;
    let l = luma(c);
    let knee = max(l - u.threshold, 0.0);
    let w = knee / (knee + 0.25);
    let lift = 1.0 + clamp(u.contrast_mult - 1.0, -u.contrast_limit, u.contrast_limit);
    return vec4<f32>(c * w * lift, 1.0);
}

// Separable 9-tap gaussian along blur_dir. Weights are a normalised sigma≈2 kernel
// (validated against atmosphere::gaussian_kernel in the engine tests).
@fragment
fn fs_blur(in: VSOut) -> @location(0) vec4<f32> {
    let w0 = 0.2270270270;
    let w1 = 0.1945945946;
    let w2 = 0.1216216216;
    let w3 = 0.0540540541;
    let w4 = 0.0162162162;
    let step = u.blur_dir * u.texel;
    var acc = textureSample(tex_a, samp, in.uv).rgb * w0;
    acc += textureSample(tex_a, samp, in.uv + step * 1.0).rgb * w1;
    acc += textureSample(tex_a, samp, in.uv - step * 1.0).rgb * w1;
    acc += textureSample(tex_a, samp, in.uv + step * 2.0).rgb * w2;
    acc += textureSample(tex_a, samp, in.uv - step * 2.0).rgb * w2;
    acc += textureSample(tex_a, samp, in.uv + step * 3.0).rgb * w3;
    acc += textureSample(tex_a, samp, in.uv - step * 3.0).rgb * w3;
    acc += textureSample(tex_a, samp, in.uv + step * 4.0).rgb * w4;
    acc += textureSample(tex_a, samp, in.uv - step * 4.0).rgb * w4;
    return vec4<f32>(acc, 1.0);
}

fn tonemap_aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51; let b = 0.03; let c = 2.43; let d = 0.59; let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

// Composite: scene HDR (tex_a) + blurred bloom (tex_b), exposure, tone-map -> LDR.
@fragment
fn fs_composite(in: VSOut) -> @location(0) vec4<f32> {
    let hdr = textureSample(tex_a, samp, in.uv).rgb;
    let bloom = textureSample(tex_b, samp, in.uv).rgb;
    var color = hdr + bloom * (u.amount * u.multiplier);
    color *= u.exposure;
    var mapped: vec3<f32>;
    if (u.tonemap_mode < 0.5) {
        mapped = tonemap_aces(color);
    } else {
        mapped = color / (vec3<f32>(1.0) + color); // Reinhard
    }
    return vec4<f32>(mapped, 1.0);
}
