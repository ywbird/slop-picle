//! M3 generators (spec/nodes.md#m3-nodes).

use super::{R, cos_sin, dim, out, GEN_MASK, UV_MAP, UV_MIX};
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::shader;
use crate::types::{Type as T, Value};

const DIMENSION: I = I::new("dimension", T::Dimension, D::ProjectSize, "Output size; absent means the project size.");
const BLACK_WHITE: &str = r##"{"type":"rgb","keys":[{"time":0,"color":"#000000"},{"time":1,"color":"#ffffff"}]}"##;

// ---------- gradient ----------

pub const GRADIENT_IN: &[I] = &[
    DIMENSION,
    I::new("gradient", T::Gradient, D::Json(BLACK_WHITE), "Colours along the gradient."),
    I::new("shift", T::Number, D::Number(0.0), "Shift along the gradient.").range(-2.0, 2.0),
    I::new("scale", T::Number, D::Number(1.0), "Gradient scale.").range(0.0, 5.0),
    I::new("loop", T::Enum, D::Enum("none"), "Repeat beyond 0..1.").options(&["none", "loop", "pingpong"]),
    I::new("type", T::Enum, D::Enum("linear"), "Gradient shape.").options(&["linear", "circular", "radial"]),
    I::new("angle", T::Number, D::Number(0.0), "Degrees."),
    I::new("radius", T::Number, D::Number(0.5), "Circular radius."),
    I::new("center", T::Vec2, D::Vec2([0.5, 0.5]), "Centre, as a fraction of the size."),
    I::new("uniform_ratio", T::Bool, D::Bool(true), "Keep circles round on non-square outputs."),
    UV_MAP,
    UV_MIX,
    GEN_MASK,
];

const GRADIENT_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.out_info.xy);
    let uv = (vec2<f32>(px) + 0.5) / size;
    let d = uv - p.center.xy;
    let a = radians(p.angle.x);
    let tau = 6.283185307179586;
    var t = 0.0;
    if (p.type_.x == 0u) {
        t = 0.5 + d.x * cos(a) - d.y * sin(a);
    } else if (p.type_.x == 1u) {
        var asp = vec2<f32>(1.0);
        if (p.uniform_ratio.x == 1u) { asp = vec2<f32>(size.x / size.y, 1.0); }
        t = length(d * asp) / (p.radius.x * sqrt(2.0));
    } else {
        let ang = atan2(d.y, d.x) + a;
        t = (ang - floor(ang / tau) * tau) / tau;
    }
    t = (t + p.shift.x - 0.5) / p.scale.x + 0.5;
    if (p.loop_.x == 1u) { t = fract(fract(t) + 1.0); }
    else if (p.loop_.x == 2u) { t = 1.0 - abs(t - 2.0 * floor(t / 2.0) - 1.0); }
    return eval_gradient(t);
}
";

pub fn gradient(ctx: &mut Ctx, v: &[Value]) -> R {
    out(shader::run(ctx, v, GRADIENT_WGSL, &[], dim(&v[0]))?)
}

// ---------- shape ----------

pub const SHAPE_IN: &[I] = &[
    DIMENSION,
    I::new("shape", T::Enum, D::Enum("rectangle"), "Shape to draw.").options(&["rectangle", "ellipse"]),
    I::new("center", T::Vec2, D::Vec2([0.5, 0.5]), "Centre, as a fraction of the size."),
    I::new("half_size", T::Vec2, D::Vec2([0.5, 0.5]), "Half extent, as a fraction of the size."),
    I::new("shape_rotation", T::Number, D::Number(0.0), "Degrees, counter-clockwise."),
    I::new("shape_color", T::Color, D::Color("#ffffff"), "Fill colour."),
    I::new("background", T::Enum, D::Enum("none"), "Outside the shape: transparent or bg_color.").options(&["none", "solid"]),
    I::new("bg_color", T::Color, D::Color("#000000"), "Background colour when background is solid."),
    UV_MAP,
    UV_MIX,
    GEN_MASK,
];

const SHAPE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.out_info.xy);
    let d = vec2<f32>(px) + 0.5 - p.center.xy * size;
    let q = vec2<f32>(d.x * p.rot.x - d.y * p.rot.y, d.x * p.rot.y + d.y * p.rot.x);
    let h = p.half_size.xy * size;
    var inside = false;
    if (p.shape.x == 0u) {
        inside = abs(q.x) <= h.x && abs(q.y) <= h.y;
    } else {
        let e = q / h;
        inside = dot(e, e) <= 1.0;
    }
    if (inside) { return p.shape_color; }
    if (p.background.x == 1u) { return p.bg_color; }
    return vec4<f32>(0.0);
}
";

pub fn shape(ctx: &mut Ctx, v: &[Value]) -> R {
    let (c, s) = cos_sin(-v[4].num());
    out(shader::run(ctx, v, SHAPE_WGSL, &[("rot", [c, s, 0.0, 0.0])], dim(&v[0]))?)
}

// ---------- cellular ----------

pub const CELLULAR_IN: &[I] = &[
    DIMENSION,
    I::new("seed", T::Int, D::Int(0), "Random seed."),
    I::new("pattern", T::Enum, D::Enum("tiled"), "Tiled wraps at the cell count.").options(&["tiled", "uniform"]),
    I::new("phase", T::Number, D::Number(0.0), "Moves feature points; degrees."),
    I::new("randomness", T::Number, D::Number(1.0), "0 puts points at cell centres.").range(0.0, 1.0),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Origin, as a fraction of the size."),
    I::new("rotation", T::Number, D::Number(0.0), "Degrees."),
    I::new("scale", T::Number, D::Number(4.0), "Cells across the width.").min(0.0),
    I::new("contrast", T::Number, D::Number(1.0), "Contrast around middle.").range(0.0, 4.0),
    I::new("middle", T::Number, D::Number(0.5), "Contrast pivot.").range(0.0, 1.0),
    I::new("inverted", T::Bool, D::Bool(false), "Invert the result."),
    UV_MAP,
    UV_MIX,
    GEN_MASK,
];

const CELLULAR_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.out_info.xy);
    let uv = (vec2<f32>(px) + 0.5) / size;
    let n = vec2<f32>(uv.x, uv.y * size.y / size.x);
    let tiled = p.pattern.x == 0u;
    var s = p.scale.x;
    if (tiled) { s = floor(s); }
    let d = n - p.position.xy;
    let st = vec2<f32>(d.x * p.rot.x - d.y * p.rot.y, d.x * p.rot.y + d.y * p.rot.x) * s;
    let i = floor(st);
    let f = st - i;
    var md = 2.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let k = vec2<f32>(f32(x), f32(y));
            var cell = vec2<i32>(i + k);
            if (tiled && s >= 1.0) {
                let si = i32(s);
                cell = ((cell % si) + si) % si;
            }
            let h = hash2(cell.x, cell.y, p.seed.x);
            let r = vec2<f32>(unit(h), unit(pcg(h)));
            let pt = 0.5 + 0.5 * sin(6.283185307179586 * fract(r + p.phase.x / 360.0)) * p.randomness.x;
            md = min(md, length(k + pt - f));
        }
    }
    var v = p.middle.x + (md - p.middle.x) * p.contrast.x;
    if (p.inverted.x == 1u) { v = 1.0 - v; }
    return vec4<f32>(v, v, v, 1.0);
}
";

pub fn cellular(ctx: &mut Ctx, v: &[Value]) -> R {
    let (c, s) = cos_sin(v[6].num());
    out(shader::run(ctx, v, CELLULAR_WGSL, &[("rot", [c, s, 0.0, 0.0])], dim(&v[0]))?)
}

// ---------- checker ----------

pub const CHECKER_IN: &[I] = &[
    DIMENSION,
    I::new("size", T::Number, D::Number(0.5), "Cell size, as a fraction of the width.").min(0.0),
    I::new("angle", T::Number, D::Number(0.0), "Degrees."),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Origin, as a fraction of the size."),
    I::new("color_1", T::Color, D::Color("#ffffff"), "Odd cells."),
    I::new("color_2", T::Color, D::Color("#000000"), "Even cells."),
    UV_MAP,
    UV_MIX,
    GEN_MASK,
];

const CHECKER_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.out_info.xy);
    let d = vec2<f32>(px) + 0.5 - p.position.xy * size;
    let q = vec2<f32>(d.x * p.rot.x - d.y * p.rot.y, d.x * p.rot.y + d.y * p.rot.x);
    let cell = p.size.x * size.x;
    let k = i32(floor(q.x / cell)) + i32(floor(q.y / cell));
    if ((k & 1) == 0) { return p.color_2; }
    return p.color_1;
}
";

pub fn checker(ctx: &mut Ctx, v: &[Value]) -> R {
    if v[1].num() <= 0.0 {
        return Err("size must be > 0".into());
    }
    let (c, s) = cos_sin(v[2].num());
    out(shader::run(ctx, v, CHECKER_WGSL, &[("rot", [c, s, 0.0, 0.0])], dim(&v[0]))?)
}

// ---------- stripe ----------

pub const STRIPE_IN: &[I] = &[
    DIMENSION,
    I::new("size", T::Number, D::Number(0.25), "Stripe period, as a fraction of the height.").min(0.0),
    I::new("strip_ratio", T::Number, D::Number(0.5), "Share of color_2 in each period.").range(0.0, 1.0),
    I::new("angle", T::Number, D::Number(0.0), "Degrees."),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Origin, as a fraction of the size."),
    I::new("progress", T::Number, D::Number(0.5), "Phase of the stripes.").range(0.0, 1.0),
    I::new("color_1", T::Color, D::Color("#ffffff"), "First colour."),
    I::new("color_2", T::Color, D::Color("#000000"), "Second colour."),
    UV_MAP,
    UV_MIX,
    GEN_MASK,
];

const STRIPE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.out_info.xy);
    let d = vec2<f32>(px) + 0.5 - p.position.xy * size;
    let t = (d.x * p.rot.x - d.y * p.rot.y) / (p.size.x * size.y);
    let s = fract(t + p.progress.x);
    if (s >= p.strip_ratio.x + 0.001) { return p.color_1; }
    return p.color_2;
}
";

pub fn stripe(ctx: &mut Ctx, v: &[Value]) -> R {
    if v[1].num() <= 0.0 {
        return Err("size must be > 0".into());
    }
    let (c, s) = cos_sin(v[3].num());
    out(shader::run(ctx, v, STRIPE_WGSL, &[("rot", [c, s, 0.0, 0.0])], dim(&v[0]))?)
}
