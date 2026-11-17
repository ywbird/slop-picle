//! M3 colour nodes (spec/nodes.md#m3-nodes).

use super::{R, out, surf, CHANNEL, INVERT_MASK, MASK, MASK_FEATHER, MIX};
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::shader;
use crate::types::{Type as T, Value};

const SURFACE_IN: I = I::required("surface_in", T::Surface, "Input image.");
const BW: &str = r##"["#000000","#ffffff"]"##;
const BLACK_WHITE: &str = r##"{"type":"rgb","keys":[{"time":0,"color":"#000000"},{"time":1,"color":"#ffffff"}]}"##;

fn same_size(ctx: &mut Ctx, v: &[Value], body: &str) -> R {
    let s = surf(&v[0]);
    out(shader::run(ctx, v, body, &[], [s.width, s.height])?)
}

pub const COLOR_ADJUST_IN: &[I] = &[
    SURFACE_IN,
    I::new("brightness", T::Number, D::Number(0.0), "Added to rgb.").range(-1.0, 1.0),
    I::new("contrast", T::Number, D::Number(0.5), "0.5 keeps contrast.").range(0.0, 1.0),
    I::new("exposure", T::Number, D::Number(1.0), "Multiplier.").range(0.0, 4.0),
    I::new("hue", T::Number, D::Number(0.0), "Hue shift (full turn = 1).").range(-1.0, 1.0),
    I::new("saturation", T::Number, D::Number(0.0), "Saturation change.").range(-1.0, 1.0),
    I::new("value", T::Number, D::Number(0.0), "Value change.").range(-1.0, 1.0),
    I::new("blend", T::Color, D::Color("#ffffff"), "Colour mixed in by blend_amount."),
    I::new("blend_amount", T::Number, D::Number(0.0), "Mix towards blend.").range(0.0, 1.0),
    I::new("alpha", T::Number, D::Number(1.0), "Alpha multiplier.").range(0.0, 1.0),
];

// Ported from upstream shaders/sh_color_adjust.fsh (blend mode normal).
const COLOR_ADJUST_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let col = load_surface_in(px);
    let one = vec4<f32>(1.0);
    let zero = vec4<f32>(0.0);
    var c = clamp(0.5 + (p.contrast.x * 2.0) * (col - 0.5), zero, one);
    c = clamp(c + vec4<f32>(p.brightness.x, p.brightness.x, p.brightness.x, 0.0), zero, one);
    c = clamp(c * p.exposure.x, zero, one);
    var hsv = rgb2hsv(c.rgb);
    hsv.x = fract(fract(hsv.x + p.hue.x) + 1.0);
    hsv.z = clamp((hsv.z + p.value.x) * (1.0 + p.saturation.x * hsv.y * 0.5), 0.0, 1.0);
    hsv.y = clamp(hsv.y * (p.saturation.x + 1.0), 0.0, 1.0);
    var rgb = clamp(hsv2rgb(hsv), vec3<f32>(0.0), vec3<f32>(1.0));
    rgb = mix(rgb, p.blend.rgb, p.blend_amount.x);
    return vec4<f32>(rgb, col.a * p.alpha.x);
}
";

pub fn color_adjust(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, COLOR_ADJUST_WGSL)
}

pub const INVERT_IN: &[I] = &[SURFACE_IN, I::new("include_alpha", T::Bool, D::Bool(false), "Invert alpha too."), MASK, MIX, INVERT_MASK, MASK_FEATHER, CHANNEL];

const INVERT_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let c = load_surface_in(px);
    if (p.include_alpha.x == 1u) { return 1.0 - c; }
    return vec4<f32>(1.0 - c.rgb, c.a);
}
";

pub fn invert(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, INVERT_WGSL)
}

pub const COLOR_REPLACE_IN: &[I] = &[
    SURFACE_IN,
    I::new("from", T::Palette, D::Json(BW), "Colours to find."),
    I::new("to", T::Palette, D::Json(BW), "Replacements, matched by order."),
    I::new("threshold", T::Number, D::Number(0.1), "HSV distance that counts as a match.").range(0.0, 1.0),
    I::new("multiply_alpha", T::Bool, D::Bool(true), "Multiply HSV by alpha before matching."),
    I::new("hard_replace", T::Bool, D::Bool(true), "Replace fully instead of blending by distance."),
    MASK,
    MIX,
    INVERT_MASK,
    MASK_FEATHER,
    CHANNEL,
];

// Ported from upstream shaders/sh_palette_replace.fsh (mode Order).
const COLOR_REPLACE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let col = load_surface_in(px);
    var hsv = rgb2hsv(col.rgb);
    if (p.multiply_alpha.x == 1u) { hsv = hsv * col.a; }
    var min_df = p.threshold.x;
    var fr = -1;
    let nf = i32(p.from_info.x);
    let nt = i32(p.to_info.x);
    for (var i = 0; i < nf; i++) {
        let df = length(hsv - rgb2hsv(p.from_colors[i].rgb));
        if (df < min_df) { min_df = df; fr = i; }
    }
    if (fr < 0) { return col; }
    var ti = 0;
    if (nf > 1) { ti = i32(round(f32(fr) / f32(nf - 1) * f32(nt - 1))); }
    let clr = p.to_colors[ti];
    var res = clr;
    if (p.hard_replace.x == 0u) {
        let rat = min_df / p.threshold.x;
        res = col * rat + clr * (1.0 - rat);
    }
    return vec4<f32>(res.rgb, col.a);
}
";

pub fn color_replace(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, COLOR_REPLACE_WGSL)
}

pub const COLORIZE_IN: &[I] = &[
    SURFACE_IN,
    I::new("gradient", T::Gradient, D::Json(BLACK_WHITE), "Colours mapped from brightness."),
    I::new("gradient_shift", T::Number, D::Number(0.0), "Shift added to brightness.").range(-1.0, 1.0),
    I::new("color_range", T::Vec2, D::Vec2([0.0, 1.0]), "Brightness range mapped to the gradient."),
    I::new("overflow", T::Enum, D::Enum("clamp"), "Outside the range: clamp or loop.").options(&["clamp", "loop"]),
    I::new("multiply_alpha", T::Bool, D::Bool(true), "Multiply the position by alpha."),
    I::new("keep_alpha", T::Bool, D::Bool(true), "Keep the input alpha."),
    MASK,
    MIX,
    INVERT_MASK,
    MASK_FEATHER,
    CHANNEL,
];

// Ported from upstream shaders/sh_colorize.fsh.
const COLORIZE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let col = load_surface_in(px);
    var t = lum(col.rgb) + p.gradient_shift.x;
    t = (t - p.color_range.x) / (p.color_range.y - p.color_range.x);
    if (p.overflow.x == 0u) { t = clamp(t, 0.0, 1.0); } else { t = fract(t); }
    if (p.multiply_alpha.x == 1u) { t = t * col.a; }
    if (t > 1.0) { if (t == floor(t)) { t = 1.0; } else { t = fract(t); } }
    var c = eval_gradient(t);
    if (p.keep_alpha.x == 1u) { c.a = col.a; }
    return c;
}
";

pub fn colorize(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, COLORIZE_WGSL)
}

pub const GREYSCALE_IN: &[I] = &[
    SURFACE_IN,
    I::new("brightness", T::Number, D::Number(0.0), "Added before conversion.").range(-1.0, 1.0),
    I::new("contrast", T::Number, D::Number(1.0), "Multiplier before conversion.").range(-1.0, 4.0),
    MASK,
    MIX,
    INVERT_MASK,
    MASK_FEATHER,
    CHANNEL,
];

// Ported from upstream shaders/sh_greyscale.fsh.
const GREYSCALE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let col = load_surface_in(px);
    let rgb = (col.rgb + vec3<f32>(p.brightness.x)) * p.contrast.x;
    let l = lum(rgb) * col.a;
    return vec4<f32>(l, l, l, col.a);
}
";

pub fn greyscale(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, GREYSCALE_WGSL)
}
