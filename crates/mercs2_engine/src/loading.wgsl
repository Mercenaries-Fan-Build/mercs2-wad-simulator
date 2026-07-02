// Loading-screen spinner: an anti-aliased ring arc rotating about the screen centre, drawn as a
// fullscreen triangle (same vertex-index trick as sky.wgsl) over the scene's dark clear color
// while `load_world_data` runs on the background thread. When the real shell.wad loading plate
// is bound (art aspect > 0), it is drawn letterboxed behind the spinner; the bars keep the
// dark clear color.

struct Loading {
    params: vec4<f32>, // x = time (s), y = aspect (w/h), z = art aspect (0 = no art), w = progress 0..1
};
@group(0) @binding(0) var<uniform> loading: Loading;
@group(1) @binding(0) var art_tex: texture_2d<f32>;
@group(1) @binding(2) var art_samp: sampler;

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VSOut {
    // Fullscreen triangle from the vertex index, no vertex buffer:
    // vi 0/1/2 -> ndc (-1,-1) / (3,-1) / (-1,3).
    var out: VSOut;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    let ndc = uv * 2.0 - 1.0;
    out.clip_pos = vec4<f32>(ndc, 1.0, 1.0);
    out.ndc = ndc;
    return out;
}

const TAU: f32 = 6.28318530718;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let t = loading.params.x;
    let aspect = loading.params.y;
    let art_aspect = loading.params.z;
    // Aspect-corrected coords in units of half the window height, so the ring stays circular.
    let p = in.ndc * vec2<f32>(aspect, 1.0);
    // Aspect-correct letterbox: fit the art rect inside the screen (half-extents in the same
    // units as `p`), sample inside it, keep the dark clear color as the bars.
    let fit_a = max(art_aspect, 1e-3);
    let k = min(1.0, aspect / fit_a);
    let q = p / vec2<f32>(fit_a * k, k); // -1..1 across the art rect
    let uv = vec2<f32>(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5);
    let art = textureSample(art_tex, art_samp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0))).rgb;
    let inside = step(abs(q.x), 1.0) * step(abs(q.y), 1.0) * step(0.5, art_aspect);
    // Staged load-progress fill inside the plate's empty bar frame. Frame measured from
    // lti_precache1 (2048x1024): outer x 664..1353 y 683..720, interior x 668..1349 y 690..713;
    // minus the crop origin (384,148) and inset ~2 px so the frame stays visible, the fill rect
    // in art-space UV (over the 1280x720 crop) is:
    let bar_min = vec2<f32>(286.0 / 1280.0, 544.0 / 720.0);
    let bar_max = vec2<f32>(964.0 / 1280.0, 564.0 / 720.0);
    let progress = clamp(loading.params.w, 0.0, 1.0);
    let in_bar = step(bar_min.x, uv.x) * step(uv.x, bar_max.x)
        * step(bar_min.y, uv.y) * step(uv.y, bar_max.y);
    // Left-to-right fill with a soft head, in the plate's bright frame gold (sampled RGB 222,170,49).
    let head = mix(bar_min.x, bar_max.x, progress);
    let fill = in_bar * inside * (1.0 - smoothstep(head - 0.003, head, uv.x));
    let gold = vec3<f32>(0.87, 0.667, 0.192);
    // Spinner: centred while there's no art; once the plate is bound, park it just right of the
    // bar (art-space) at ~60% size. Invert the uv mapping to get the ring centre in `p` units.
    let ring_uv = vec2<f32>(0.785, (bar_min.y + bar_max.y) * 0.5);
    let ring_q = vec2<f32>(ring_uv.x * 2.0 - 1.0, 1.0 - ring_uv.y * 2.0);
    let ring_c = ring_q * vec2<f32>(fit_a * k, k) * step(0.5, art_aspect);
    let pr = p - ring_c;
    // Ring radius ~0.06 of the min window dimension (the min dimension = 2·min(aspect, 1) in
    // these units), with a thin anti-aliased ring profile.
    let radius = 0.12 * mix(1.0, 0.6, step(0.5, art_aspect)) * min(aspect, 1.0);
    let thick = radius * 0.18;
    let aa = 0.004;
    let ring = 1.0 - smoothstep(thick - aa, thick + aa, abs(length(pr) - radius));
    // ~270° arc rotating at ~1.5 rev/s: `a` = fraction around the circle from the arc head,
    // visible over [0, 0.75] with smoothstepped ends.
    let a = fract((atan2(pr.y, pr.x) + t * 1.5 * TAU) / TAU);
    let arc = smoothstep(0.0, 0.02, a) * (1.0 - smoothstep(0.73, 0.75, a));
    // Soft white-ish spinner over the (letterboxed art or) dark clear color.
    let bg = mix(mix(vec3<f32>(0.02, 0.02, 0.04), art, inside), gold, fill * 0.9);
    let rgb = mix(bg, vec3<f32>(0.85, 0.88, 0.92), ring * arc);
    return vec4<f32>(rgb, 1.0);
}
