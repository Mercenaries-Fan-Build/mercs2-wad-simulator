// Scaleform movies. Geometry reaches here in clip space with its color transform on each vertex, so the
// one pipeline covers solids, bitmaps and gradients. `kind`: x mode (0 solid, 1 bitmap, 2 linear,
// 3 radial, 4 focal), y spread (0 pad, 1 reflect, 2 repeat), z focal point.

struct VertexIn {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mul: vec4<f32>,
    @location(3) add: vec4<f32>,
    @location(4) kind: vec3<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mul: vec4<f32>,
    @location(2) add: vec4<f32>,
    @location(3) kind: vec3<f32>,
};

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.mul = in.mul;
    out.add = in.add;
    out.kind = in.kind;
    return out;
}

// Where along a gradient a point of the unit square falls.
fn gradient_position(mode: f32, uv: vec2<f32>, focal: f32) -> f32 {
    if (mode < 2.5) {
        return (uv.x + 1.0) * 0.5;
    }
    if (mode < 3.5) {
        return length(uv);
    }
    // The distance from the focal point to `uv` over its distance to the unit circle along that ray.
    let from_focus = uv - vec2<f32>(focal, 0.0);
    let d = length(from_focus);
    if (d < 0.00001) {
        return 0.0;
    }
    let dir = from_focus / d;
    let along = focal * dir.x;
    let edge = -along + sqrt(max(along * along - focal * focal + 1.0, 0.0));
    return d / edge;
}

fn spread_position(t: f32, spread: f32) -> f32 {
    if (spread > 1.5) {
        return fract(t);
    }
    if (spread > 0.5) {
        return 1.0 - abs(fract(t * 0.5) * 2.0 - 1.0);
    }
    return clamp(t, 0.0, 1.0);
}

fn paint(in: VertexOut) -> vec4<f32> {
    var coord = in.uv;
    if (in.kind.x > 1.5) {
        coord = vec2<f32>(spread_position(gradient_position(in.kind.x, in.uv, in.kind.z), in.kind.y), 0.5);
    }
    let texel = textureSample(image, image_sampler, coord);
    let base = select(texel, vec4<f32>(1.0), in.kind.x < 0.5);
    return clamp(base * in.mul + in.add, vec4<f32>(0.0), vec4<f32>(1.0));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return paint(in);
}

// Multiply and darken: the colour pulled toward white by its alpha, so a transparent part of the
// source leaves the destination as it was.
@fragment
fn fs_toward_white(in: VertexOut) -> @location(0) vec4<f32> {
    let c = paint(in);
    return vec4<f32>((c.rgb - vec3<f32>(1.0)) * c.a + vec3<f32>(1.0), c.a);
}
