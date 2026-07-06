// 2D UI overlay pass: screen-space textured/solid quads, instanced, alpha-blended over whatever
// was rendered before it in the same pass (shell menu over the letterboxed plate, debug HUD over
// the world). One instance = one rect; glyphs are rects whose UVs point into the 8x8 bitmap-font
// atlas (a solid rect samples the atlas's reserved all-white cell). Pixel coords, origin top-left.

struct UiScreen {
    // x = surface width px, y = surface height px (z/w unused padding).
    size: vec4<f32>,
};
@group(0) @binding(0) var<uniform> screen: UiScreen;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_samp: sampler;

struct Inst {
    // rect: x, y, w, h in surface pixels (origin top-left).
    @location(0) rect: vec4<f32>,
    // uv: u0, v0, u1, v1 into the font atlas.
    @location(1) uv: vec4<f32>,
    @location(2) color: vec4<f32>,
};

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Inst) -> VSOut {
    // Unit-quad corner from the vertex index — drawn as a 4-vertex TRIANGLE STRIP per instance:
    // vi 0..3 -> (0,0) (1,0) (0,1) (1,1), strip triangles (0,1,2) + (2,1,3).
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    let px = inst.rect.xy + corner * inst.rect.zw;
    // Pixel -> NDC (y down in pixels, up in NDC).
    let ndc = vec2<f32>(px.x / screen.size.x * 2.0 - 1.0, 1.0 - px.y / screen.size.y * 2.0);
    var out: VSOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = mix(inst.uv.xy, inst.uv.zw, corner);
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // R8 atlas: glyph coverage in .r (nearest-sampled — crisp pixel font at integer scales).
    let a = textureSample(atlas_tex, atlas_samp, in.uv).r;
    return vec4<f32>(in.color.rgb, in.color.a * a);
}
