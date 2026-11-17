//! M3 filters (spec/nodes.md#m3-nodes).

use super::{R, out, surf};
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::shader;
use crate::surface::Surface;
use crate::types::{Type as T, Value};
use std::sync::Arc;

const SURFACE_IN: I = I::required("surface_in", T::Surface, "Input image.");
const BW: &str = r##"["#000000","#ffffff"]"##;

fn same_size(ctx: &mut Ctx, v: &[Value], body: &str) -> R {
    let s = surf(&v[0]);
    out(shader::run(ctx, v, body, &[], [s.width, s.height])?)
}

/// Gaussian kernel parameters for a blur size (spec: radius max(ceil(size) − 1, 0), σ = max(size, 1)/3).
pub(super) fn kernel(size: f64) -> (i32, f64) {
    (((size.ceil() as i32) - 1).max(0), size.max(1.0) / 3.0)
}

// ---------- blur ----------

pub const BLUR_IN: &[I] = &[
    SURFACE_IN,
    I::new("size", T::Number, D::Number(8.0), "Blur size in pixels.").min(0.0),
    I::new("override_color", T::Bool, D::Bool(false), "Replace colour, keep the blurred alpha."),
    I::new("color", T::Color, D::Color("#000000"), "Colour for override_color."),
];

const BLUR_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let dir = vec2<i32>(i32(p.blur_pass.x), i32(p.blur_pass.y));
    let r = i32(p.blur_pass.z);
    let sg = p.blur_pass.w;
    var acc = vec3<f32>(0.0);
    var wa = 0.0;
    var ws = 0.0;
    for (var i = -r; i <= r; i++) {
        let w = exp(-f32(i * i) / (2.0 * sg * sg));
        let c = load_surface_in(px + dir * i);
        acc += w * c.a * c.rgb;
        wa += w * c.a;
        ws += w;
    }
    if (wa <= 0.0) { return vec4<f32>(0.0); }
    var res = vec4<f32>(acc / wa, wa / ws);
    if (dir.y == 1 && p.override_color.x == 1u) { res = vec4<f32>(p.color.rgb, res.a * p.color.a); }
    return res;
}
";

pub fn blur(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let (r, sigma) = kernel(v[1].num());
    let size = [s.width, s.height];
    let h = shader::run(ctx, v, BLUR_WGSL, &[("blur_pass", [1.0, 0.0, r as f32, sigma as f32])], size)?;
    let mut v2 = v.to_vec();
    v2[0] = Value::Surface(Arc::new(h));
    out(shader::run(ctx, &v2, BLUR_WGSL, &[("blur_pass", [0.0, 1.0, r as f32, sigma as f32])], size)?)
}

// ---------- outline ----------

pub const OUTLINE_IN: &[I] = &[
    SURFACE_IN,
    I::new("position", T::Enum, D::Enum("outside"), "Draw inside or outside the shape.").options(&["inside", "outside"]),
    I::new("profile", T::Enum, D::Enum("circle"), "Distance metric.").options(&["circle", "square", "diamond"]),
    I::new("width", T::Number, D::Number(0.0), "Outline width in pixels.").min(0.0),
    I::new("start", T::Int, D::Int(0), "Gap between the shape and the outline.").min(0.0),
    I::new("threshold", T::Number, D::Number(1.0), "Alpha threshold for solid/empty.").range(0.0, 1.0),
    I::new("color", T::Color, D::Color("#ffffff"), "Outline colour."),
];

const OUTLINE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let base = load_surface_in(px);
    let th = p.threshold.x;
    let outside = p.position.x == 1u;
    if ((outside && !(base.a < th)) || (!outside && !(base.a > 1.0 - th))) { return base; }
    let w = floor(p.width.x);
    let st = f32(p.start.x);
    if (w <= 0.0) { return base; }
    let R = i32(st + w);
    for (var dy = -R; dy <= R; dy++) {
        for (var dx = -R; dx <= R; dx++) {
            if (dx == 0 && dy == 0) { continue; }
            let fx = f32(abs(dx));
            let fy = f32(abs(dy));
            var d = sqrt(fx * fx + fy * fy);
            if (p.profile.x == 1u) { d = max(fx, fy); } else if (p.profile.x == 2u) { d = fx + fy; }
            if (d > st && d <= st + w) {
                let c = load_surface_in(px + vec2<i32>(dx, dy));
                if ((outside && c.a > 1.0 - th) || (!outside && c.a < th)) { return p.color; }
            }
        }
    }
    return base;
}
";

pub fn outline(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, OUTLINE_WGSL)
}

// ---------- shadow (CPU) ----------

pub const SHADOW_IN: &[I] = &[
    SURFACE_IN,
    I::new("color", T::Color, D::Color("#000000"), "Shadow colour."),
    I::new("strength", T::Number, D::Number(0.5), "Shadow opacity.").range(0.0, 2.0),
    I::new("grow", T::Int, D::Int(3), "Dilate the shadow by this many pixels.").range(0.0, 16.0),
    I::new("blur", T::Int, D::Int(3), "Blur size of the shadow.").range(0.0, 16.0),
    I::new("shift", T::Vec2, D::Vec2([0.0, 0.0]), "Shadow offset in pixels."),
];

/// One separable Gaussian pass over an alpha map (outside counts as 0).
pub(super) fn blur_alpha(m: &[f64], w: usize, h: usize, r: i32, sigma: f64, horizontal: bool) -> Vec<f64> {
    let mut out = vec![0.0; m.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut acc, mut ws) = (0.0, 0.0);
            for i in -r..=r {
                let wt = (-((i * i) as f64) / (2.0 * sigma * sigma)).exp();
                let (sx, sy) = if horizontal { (x as i32 + i, y as i32) } else { (x as i32, y as i32 + i) };
                if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                    acc += wt * m[sy as usize * w + sx as usize];
                }
                ws += wt;
            }
            out[y * w + x] = acc / ws;
        }
    }
    out
}

pub fn shadow(_: &mut Ctx, v: &[Value]) -> R {
    let base = surf(&v[0]);
    let (w, h) = (base.width as usize, base.height as usize);
    let Value::Color(color) = v[1] else { unreachable!() };
    let strength = v[2].num();
    let (grow, blur) = (v[3].num() as i32, v[4].num());
    let shift = v[5].vec2();
    let alpha = |x: i32, y: i32| -> f64 {
        if x < 0 || y < 0 || x as usize >= w || y as usize >= h { 0.0 } else { base.pixel(x as u32, y as u32)[3] as f64 / 255.0 }
    };
    let mut m = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let sx = (x as f64 + 0.5 - shift[0]).floor() as i32;
            let sy = (y as f64 + 0.5 - shift[1]).floor() as i32;
            let mut a: f64 = 0.0;
            for dy in -grow..=grow {
                for dx in -grow..=grow {
                    a = a.max(alpha(sx + dx, sy + dy));
                }
            }
            m[y * w + x] = a;
        }
    }
    let (r, sigma) = kernel(blur);
    let m = blur_alpha(&blur_alpha(&m, w, h, r, sigma, true), w, h, r, sigma, false);
    let col = color.map(|c| c as f64 / 255.0);
    let mut s = Surface::new(base.width, base.height);
    for y in 0..h {
        for x in 0..w {
            let b = base.pixel(x as u32, y as u32).map(|c| c as f64 / 255.0);
            let shad = [col[0], col[1], col[2], col[3] * m[y * w + x]].map(|c| c * strength);
            let i = (y * w + x) * 4;
            for c in 0..4 {
                let o = shad[c] * (1.0 - b[3]) + b[c] * b[3];
                s.data[i + c] = (o.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    out(s)
}

// ---------- posterize ----------

pub const POSTERIZE_IN: &[I] = &[
    SURFACE_IN,
    I::new("use_palette", T::Bool, D::Bool(true), "Snap to the palette instead of steps."),
    I::new("palette", T::Palette, D::Json(BW), "Target colours."),
    I::new("steps", T::Int, D::Int(4), "Levels per channel without a palette.").range(2.0, 16.0),
    I::new("gamma", T::Number, D::Number(1.0), "Gamma for steps.").range(0.0, 2.0),
    I::new("posterize_alpha", T::Bool, D::Bool(true), "Posterize alpha as well."),
];

// Ported from upstream shaders/sh_posterize.fsh and sh_posterize_palette.fsh (RGB space, range 0..1).
const POSTERIZE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let c = load_surface_in(px);
    let pa = p.posterize_alpha.x == 1u;
    if (p.use_palette.x == 1u) {
        var col = c;
        if (pa) { col = c * c.a; }
        var best = 0;
        var bd = 1e9;
        for (var i = 0; i < i32(p.palette_info.x); i++) {
            let d = distance(p.palette_colors[i].rgb, col.rgb);
            if (d < bd) { bd = d; best = i; }
        }
        var res = p.palette_colors[best];
        if (!pa) { res.a = c.a; }
        return res;
    }
    let g = max(p.gamma.x, 0.0001);
    let steps = f32(p.steps.x);
    var rgb = pow(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(g));
    rgb = pow(floor(rgb * steps) / (steps - 1.0), vec3<f32>(1.0 / g));
    var a = c.a;
    if (pa) { a = 1.0; }
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), a);
}
";

pub fn posterize(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, POSTERIZE_WGSL)
}

// ---------- dither ----------

pub const DITHER_IN: &[I] = &[
    SURFACE_IN,
    I::new("seed", T::Int, D::Int(0), "Seed for white_noise."),
    I::new("pattern", T::Enum, D::Enum("bayer_2x2"), "Threshold pattern.").options(&["bayer_2x2", "bayer_4x4", "bayer_8x8", "white_noise"]),
    I::new("invert", T::Bool, D::Bool(false), "Invert the pattern."),
    I::new("contrast", T::Number, D::Number(1.0), "Pattern contrast.").range(1.0, 5.0),
    I::new("color_type", T::Enum, D::Enum("palette"), "Quantise to steps or to a palette.").options(&["greyscale", "palette"]),
    I::new("steps", T::Int, D::Int(4), "Levels for greyscale.").range(2.0, 16.0),
    I::new("palette", T::Palette, D::Json(BW), "Colours for palette."),
];

// Ported from upstream shaders/sh_dither.fsh (RGB distance instead of LAB).
const DITHER_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let c = load_surface_in(px);
    var c1: vec4<f32>;
    var c2: vec4<f32>;
    var exact = false;
    var res = c;
    if (p.color_type.x == 0u) {
        let s = f32(p.steps.x);
        c1 = vec4<f32>(floor(c.rgb * s) / s, c.a);
        c2 = vec4<f32>(ceil(c.rgb * s) / s, c.a);
        exact = distance(c, c1) < 0.05;
    } else {
        var i1 = 0;
        var d1v = 99.0;
        var i2 = 0;
        var d2v = 99.0;
        for (var i = 0; i < i32(p.palette_info.x); i++) {
            let pc = p.palette_colors[i];
            let d = distance(pc.rgb, c.rgb);
            if (d <= 0.001) {
                exact = true;
                res = vec4<f32>(pc.rgb, c.a);
            } else if (d < d1v) {
                d2v = d1v; i2 = i1; d1v = d; i1 = i;
            } else if (d < d2v) {
                d2v = d; i2 = i;
            }
        }
        c1 = p.palette_colors[i1];
        c2 = p.palette_colors[i2];
    }
    if (exact) { return res; }
    let d1 = distance(c.rgb, c1.rgb);
    let d2 = distance(c.rgb, c2.rgb);
    let r = (d1 / (d1 + d2) - 0.5) * p.contrast.x + 0.5;
    var b2 = array<f32, 4>(0.0, 2.0, 3.0, 1.0);
    var b4 = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    var b8 = array<f32, 64>(
        0.0, 32.0, 8.0, 40.0, 2.0, 34.0, 10.0, 42.0, 48.0, 16.0, 56.0, 24.0, 50.0, 18.0, 58.0, 26.0,
        12.0, 44.0, 4.0, 36.0, 14.0, 46.0, 6.0, 38.0, 60.0, 28.0, 52.0, 20.0, 62.0, 30.0, 54.0, 22.0,
        3.0, 35.0, 11.0, 43.0, 1.0, 33.0, 9.0, 41.0, 51.0, 19.0, 59.0, 27.0, 49.0, 17.0, 57.0, 25.0,
        15.0, 47.0, 7.0, 39.0, 13.0, 45.0, 5.0, 37.0, 63.0, 31.0, 55.0, 23.0, 61.0, 29.0, 53.0, 21.0);
    var v = 0.0;
    let pat = p.pattern.x;
    if (pat == 0u) { v = b2[(px.y % 2) * 2 + px.x % 2] / 3.0; }
    else if (pat == 1u) { v = b4[(px.y % 4) * 4 + px.x % 4] / 15.0; }
    else if (pat == 2u) { v = b8[(px.y % 8) * 8 + px.x % 8] / 63.0; }
    else { v = unit(hash2(px.x, px.y, p.seed.x)); }
    if (p.invert.x == 1u) { v = 1.0 - v; }
    if (r < v) { return c1; }
    return c2;
}
";

pub fn dither(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, DITHER_WGSL)
}

// ---------- threshold ----------

pub const THRESHOLD_IN: &[I] = &[
    SURFACE_IN,
    I::new("brightness", T::Bool, D::Bool(false), "Threshold brightness into black/white."),
    I::new("brightness_threshold", T::Number, D::Number(0.5), "Brightness edge.").range(0.0, 1.0),
    I::new("brightness_smoothness", T::Number, D::Number(0.0), "Soft edge width.").range(0.0, 1.0),
    I::new("brightness_invert", T::Bool, D::Bool(false), "Invert the brightness result."),
    I::new("alpha", T::Bool, D::Bool(false), "Threshold alpha."),
    I::new("alpha_threshold", T::Number, D::Number(0.5), "Alpha edge.").range(0.0, 1.0),
    I::new("alpha_smoothness", T::Number, D::Number(0.0), "Soft edge width.").range(0.0, 1.0),
    I::new("alpha_invert", T::Bool, D::Bool(false), "Invert the alpha result."),
];

const THRESHOLD_WGSL: &str = "
fn thr(v: f32, t: f32, s: f32) -> f32 {
    if (s == 0.0) { return select(0.0, 1.0, v >= t); }
    return smoothstep(t - s, t + s, v);
}
fn shade(px: vec2<i32>) -> vec4<f32> {
    var c = load_surface_in(px);
    if (p.brightness.x == 1u) {
        var b = thr(lum(c.rgb), p.brightness_threshold.x, p.brightness_smoothness.x);
        if (p.brightness_invert.x == 1u) { b = 1.0 - b; }
        c = vec4<f32>(b, b, b, c.a);
    }
    if (p.alpha.x == 1u) {
        var a = thr(c.a, p.alpha_threshold.x, p.alpha_smoothness.x);
        if (p.alpha_invert.x == 1u) { a = 1.0 - a; }
        c.a = a;
    }
    return c;
}
";

pub fn threshold(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, THRESHOLD_WGSL)
}

// ---------- displace ----------

pub const DISPLACE_IN: &[I] = &[
    SURFACE_IN,
    I::new("displace_map", T::Surface, D::None, "Brightness map driving the displacement."),
    I::new("strength", T::Number, D::Number(1.0), "Displacement multiplier."),
    I::new("mid_value", T::Number, D::Number(0.5), "Map brightness meaning no displacement.").range(0.0, 1.0),
    I::new("position", T::Vec2, D::Vec2([1.0, 0.0]), "Displacement vector, as a fraction of the size."),
];

const DISPLACE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    if (p.displace_map_info.z == 0u) { return load_surface_in(px); }
    let size = vec2<f32>(p.surface_in_info.xy);
    let uv = (vec2<f32>(px) + 0.5) / size;
    let m = lum(load_displace_map(vec2<i32>(floor(uv * vec2<f32>(p.displace_map_info.xy)))).rgb);
    let off = p.position.xy * size * (m - p.mid_value.x) * p.strength.x;
    return load_surface_in(vec2<i32>(floor(vec2<f32>(px) + 0.5 + off)));
}
";

pub fn displace(ctx: &mut Ctx, v: &[Value]) -> R {
    same_size(ctx, v, DISPLACE_WGSL)
}
