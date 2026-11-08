// Puts the offscreen 3D image into Rune's pass at the viewport's effect slot, and makes screenshots.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct Blit {
    // The screenshot's background colour.
    bg: vec4f,
    // The screenshot's share of the view's width (it leaves out the area under the sidebar).
    crop: vec4f,
};
@group(0) @binding(2) var<uniform> u: Blit;

struct BlitOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    var out: BlitOut;
    out.pos = vec4f(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2f(p.x, 1.0 - p.y);
    return out;
}

fn encode(c: vec3f) -> vec3f {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3f(0.0)), vec3f(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3f(0.0031308));
}

@fragment
fn fs_blit_linear(in: BlitOut) -> @location(0) vec4f {
    return textureSample(src, smp, in.uv);
}

@fragment
fn fs_blit_srgb(in: BlitOut) -> @location(0) vec4f {
    let c = textureSample(src, smp, in.uv);
    if c.a <= 0.0 {
        return vec4f(0.0);
    }
    return vec4f(encode(c.rgb / c.a) * c.a, c.a);
}

// The screenshot: the view over the background, sRGB-encoded into an Rgba8Unorm target.
@fragment
fn fs_shot(in: BlitOut) -> @location(0) vec4f {
    let c = textureSample(src, smp, vec2f(in.uv.x * u.crop.x, in.uv.y));
    let top = u.bg.rgb;
    return vec4f(encode(c.rgb + top * (1.0 - c.a)), 1.0);
}
