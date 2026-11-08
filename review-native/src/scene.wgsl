// The 3D view: shaded parts, B-rep edges, the bed, screenshot markers, and the blit that puts the
// offscreen image into Rune's frame at the viewport's effect slot.

struct Globals {
    view_proj: mat4x4f,
    eye: vec4f,
    // Section plane: points with dot(xyz, p) > w are cut away; xyz is zero when off.
    plane: vec4f,
    // Viewport width and height in pixels, edge width in pixels, 1 in dark mode.
    view: vec4f,
    // Direction toward the key light, world space.
    light: vec4f,
};

struct Obj {
    model: mat4x4f,
    // Linear RGB and opacity.
    color: vec4f,
    // x: highlight, y: edge opacity, z: marker radius in pixels, w: 1 for the ground grid.
    extra: vec4f,
    // Bed: x0, y0, x1, y1. Marker: unused.
    rect: vec4f,
    // Bed: a strip some nozzle can't reach (x0, y0, x1, y1); zero width when none.
    strip: vec4f,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var<uniform> o: Obj;

fn clipped(w: vec3f) -> bool {
    return dot(g.plane.xyz, g.plane.xyz) > 0.5 && dot(g.plane.xyz, w) > g.plane.w;
}

// Screen-door transparency: covers round(alpha * 4) of the four samples, varying by pixel.
fn coverage(alpha: f32, p: vec2f) -> u32 {
    if alpha >= 0.995 {
        return 0xffffffffu;
    }
    let i = vec2u(p) % vec2u(4u);
    let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let d = (bayer[i.y * 4u + i.x] + 0.5) / 16.0;
    let k = u32(clamp(floor(alpha * 4.0 + d), 0.0, 4.0));
    let bits = (1u << k) - 1u;
    let r = (i.x + i.y * 3u) % 4u;
    return ((bits << r) | (bits >> (4u - r))) & 15u;
}

struct MeshOut {
    @builtin(position) pos: vec4f,
    @location(0) world: vec3f,
    @location(1) n: vec3f,
};

@vertex
fn vs_mesh(@location(0) p: vec3f, @location(1) n: vec3f) -> MeshOut {
    var out: MeshOut;
    let w = o.model * vec4f(p, 1.0);
    out.pos = g.view_proj * w;
    out.world = w.xyz;
    out.n = (o.model * vec4f(n, 0.0)).xyz;
    return out;
}

struct Shaded {
    @location(0) color: vec4f,
    @builtin(sample_mask) mask: u32,
};

@fragment
fn fs_mesh(in: MeshOut, @builtin(front_facing) front: bool) -> Shaded {
    if clipped(in.world) {
        discard;
    }
    var out: Shaded;
    out.mask = coverage(o.color.a, in.pos.xy);
    let base = o.color.rgb;
    if !front {
        // The inside of a sectioned solid: the cut face, hatched.
        let s = fract((in.pos.x + in.pos.y) / 9.0) < 0.5;
        out.color = vec4f(base * select(0.22, 0.32, s), 1.0);
        return out;
    }
    let n = normalize(in.n);
    let v = normalize(g.eye.xyz - in.world);
    let l = normalize(g.light.xyz);
    let sky = mix(vec3f(0.30, 0.31, 0.34), vec3f(1.0, 1.0, 1.02), n.z * 0.5 + 0.5);
    let key = max(dot(n, l), 0.0);
    let fill = max(dot(n, v), 0.0);
    let h = normalize(l + v);
    let spec = pow(max(dot(n, h), 0.0), 60.0) * 0.22;
    let rim = pow(1.0 - fill, 3.0) * 0.10;
    var c = base * (sky * 0.42 + key * 0.55 + fill * 0.18) + vec3f(spec) + rim * vec3f(0.55, 0.65, 0.9);
    c = mix(c, c * 1.18 + vec3f(0.04, 0.035, 0.0), o.extra.x);
    out.color = vec4f(c, 1.0);
    return out;
}

struct EdgeOut {
    @builtin(position) pos: vec4f,
    @location(0) world: vec3f,
};

fn lift(w: vec3f) -> vec3f {
    // Pull an edge toward the eye so it wins the depth test against its own faces.
    let to_eye = g.eye.xyz - w;
    return w + to_eye * 0.0025;
}

@vertex
fn vs_edge(@location(0) a: vec3f, @location(1) b: vec3f, @location(2) corner: vec2f) -> EdgeOut {
    var out: EdgeOut;
    let wa = (o.model * vec4f(a, 1.0)).xyz;
    let wb = (o.model * vec4f(b, 1.0)).xyz;
    let ca = g.view_proj * vec4f(lift(wa), 1.0);
    let cb = g.view_proj * vec4f(lift(wb), 1.0);
    let end = corner.x > 0.5;
    out.world = select(wa, wb, end);
    if ca.w <= 1e-3 || cb.w <= 1e-3 {
        out.pos = vec4f(2.0, 2.0, 2.0, 1.0);
        return out;
    }
    let half = g.view.xy * 0.5;
    let sa = ca.xy / ca.w * half;
    let sb = cb.xy / cb.w * half;
    var dir = sb - sa;
    if dot(dir, dir) < 1e-8 {
        dir = vec2f(1.0, 0.0);
    }
    dir = normalize(dir);
    let side = vec2f(-dir.y, dir.x) * corner.y;
    let along = dir * select(-0.5, 0.5, end);
    let c = select(ca, cb, end);
    let off = (side + along) * g.view.z * 0.5 / half;
    out.pos = vec4f(c.xy + off * c.w, c.z, c.w);
    return out;
}

@fragment
fn fs_edge(in: EdgeOut) -> @location(0) vec4f {
    if clipped(in.world) {
        discard;
    }
    let a = o.extra.y * o.color.a;
    let c = select(vec3f(0.035, 0.04, 0.05), vec3f(0.01, 0.012, 0.016), g.view.w > 0.5);
    return vec4f(c * a, a);
}

struct BedOut {
    @builtin(position) pos: vec4f,
    @location(0) xy: vec2f,
};

@vertex
fn vs_bed(@builtin(vertex_index) i: u32) -> BedOut {
    let k = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
    let r = o.rect;
    let xy = mix(r.xy, r.zw, k[i]);
    var out: BedOut;
    out.pos = g.view_proj * vec4f(xy, o.model[3].z, 1.0);
    out.xy = xy;
    return out;
}

fn grid_line(x: vec2f, step: f32, width: f32) -> f32 {
    let f = abs(fract(x / step - 0.5) - 0.5) * step;
    let w = fwidth(x) * width;
    let l = 1.0 - smoothstep(vec2f(0.0), w, f);
    return max(l.x, l.y);
}

@fragment
fn fs_bed(in: BedOut) -> @location(0) vec4f {
    let dark = g.view.w > 0.5;
    let r = o.rect;
    let ground = o.extra.w > 0.5;
    var fade = 1.0;
    if ground {
        let c = (r.xy + r.zw) * 0.5;
        let rad = (r.z - r.x) * 0.5;
        fade = 1.0 - smoothstep(rad * 0.45, rad, length(in.xy - c));
    }
    let base = select(vec3f(0.78, 0.80, 0.83), vec3f(0.032, 0.034, 0.039), dark);
    let line = select(vec3f(0.48, 0.50, 0.54), vec3f(0.10, 0.105, 0.115), dark);
    var a = select(0.55, 0.85, dark) * select(1.0, 0.0, ground);
    var c = base;
    let minor = grid_line(in.xy, 10.0, 1.0) * 0.55;
    let major = grid_line(in.xy, 50.0, 1.4);
    let gl = max(minor, major);
    c = mix(c, line, gl);
    a = max(a, gl * select(0.8, 0.7, dark));
    let s = o.strip;
    if s.z > s.x && in.xy.x >= s.x && in.xy.x <= s.z && in.xy.y >= s.y && in.xy.y <= s.w {
        c = mix(c, vec3f(0.85, 0.22, 0.18), 0.35);
        a = max(a, 0.6);
    }
    if !ground {
        let d = min(min(in.xy.x - r.x, r.z - in.xy.x), min(in.xy.y - r.y, r.w - in.xy.y));
        let w = max(fwidth(in.xy.x), fwidth(in.xy.y)) * 1.5;
        let edge = 1.0 - smoothstep(0.0, w, d);
        c = mix(c, line * 0.8, edge);
        a = max(a, edge);
    }
    a *= o.color.a * fade;
    return vec4f(c * a, a);
}

struct MarkOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vs_mark(@builtin(vertex_index) i: u32) -> MarkOut {
    let k = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
    let c = g.view_proj * vec4f(o.model[3].xyz, 1.0);
    var out: MarkOut;
    let px = o.extra.z * 2.0 / g.view.xy;
    out.pos = vec4f(c.xy + k[i] * px * c.w, 0.0, c.w);
    out.uv = k[i];
    return out;
}

@fragment
fn fs_mark(in: MarkOut) -> @location(0) vec4f {
    let d = length(in.uv);
    let aa = fwidth(d) * 1.2;
    let disc = 1.0 - smoothstep(0.32 - aa, 0.32 + aa, d);
    let ring = (1.0 - smoothstep(0.86 - aa, 0.86 + aa, d)) * smoothstep(0.62 - aa, 0.62 + aa, d);
    let white = (1.0 - smoothstep(0.98 - aa, 0.98 + aa, d)) * (1.0 - ring) * (1.0 - disc) * smoothstep(0.5 - aa, 0.5 + aa, d);
    let a = max(max(disc, ring), white * 0.9);
    let c = mix(vec3f(1.0), o.color.rgb, max(disc, ring) / max(a, 1e-4));
    return vec4f(c * a, a);
}
