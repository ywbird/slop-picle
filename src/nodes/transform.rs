//! M3 transforms (spec/nodes.md#m3-nodes).

use super::{R, dim, out, surf, CHANNEL, INVERT_MASK, MASK, MASK_FEATHER, MIX};
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::shader;
use crate::surface;
use crate::types::{Type as T, Value};

const SURFACE_IN: I = I::required("surface_in", T::Surface, "Input image.");

fn checked(w: f64, h: f64) -> Result<[u32; 2], String> {
    let (w, h) = (w as i64, h as i64);
    if w < 1 || h < 1 || !surface::size_ok(w as u32, h as u32) {
        return Err(format!("output would be {w}x{h}, outside the size limits"));
    }
    Ok([w as u32, h as u32])
}

// ---------- scale ----------

pub const SCALE_IN: &[I] = &[
    SURFACE_IN,
    I::new("mode", T::Enum, D::Enum("upscale"), "Scale by a factor, or to a size.").options(&["upscale", "scale_to_size"]),
    I::new("scale", T::Number, D::Number(1.0), "Factor for upscale mode."),
    I::new("target_dimension", T::Dimension, D::ProjectSize, "Size for scale_to_size mode."),
    I::new("fit_mode", T::Enum, D::Enum("stretch"), "scale_to_size: stretch, or keep the aspect within or around the target.")
        .options(&["stretch", "keep_aspect_minimum", "keep_aspect_maximum"]),
];

const SCALE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let o = vec2<f32>(p.out_info.xy);
    let i = vec2<f32>(p.surface_in_info.xy);
    return load_surface_in(vec2<i32>(floor((vec2<f32>(px) + 0.5) * i / o)));
}
";

pub fn scale(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let size = match v[1] {
        Value::Enum(1) if v[4] == Value::Enum(0) => dim(&v[3]),
        Value::Enum(1) => {
            let [tw, th] = dim(&v[3]);
            let (kx, ky) = (tw as f64 / s.width as f64, th as f64 / s.height as f64);
            let k = if v[4] == Value::Enum(1) { kx.min(ky) } else { kx.max(ky) };
            checked((s.width as f64 * k).round(), (s.height as f64 * k).round())?
        }
        _ => {
            let k = v[2].num();
            if k <= 0.0 {
                return Err("scale must be > 0".into());
            }
            checked((s.width as f64 * k).round(), (s.height as f64 * k).round())?
        }
    };
    out(shader::run(ctx, v, SCALE_WGSL, &[], size)?)
}

// ---------- crop ----------

pub const CROP_IN: &[I] = &[
    SURFACE_IN,
    I::new("aspect_ratio", T::Enum, D::Enum("any"), "Crop freely, or to an aspect ratio.").options(&["any", "manual", "1:1", "3:2", "4:3", "16:9"]),
    I::new("ratio", T::Vec2, D::Vec2([1.0, 1.0]), "Width and height of a manual aspect ratio."),
    I::new("crop", T::Padding, D::Json("[0,0,0,0]"), "Pixels removed (aspect any): right, top, left, bottom."),
    I::new("fit_mode", T::Enum, D::Enum("manual"), "With an aspect ratio: centre and width, or fit the width, height or both.")
        .options(&["manual", "width", "height", "minimum"]),
    I::new("center", T::Vec2, D::Vec2([0.5, 0.5]), "Centre of a manual crop, as a fraction of the input size."),
    I::new("width", T::Number, D::Number(8.0), "Width of a manual crop in pixels."),
];

const CROP_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    return load_surface_in(px + vec2<i32>(p.crop_at.xy));
}
";

pub fn crop(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let (w, h) = (s.width as f64, s.height as f64);
    let (at, size) = if v[1] == Value::Enum(0) {
        let Value::Padding([r, t, l, b]) = v[3] else { unreachable!() };
        ([l as f64, t as f64], checked(w - r as f64 - l as f64, h - t as f64 - b as f64)?)
    } else {
        let r = match v[1] {
            Value::Enum(1) => v[2].vec2()[0] / v[2].vec2()[1],
            Value::Enum(3) => 3.0 / 2.0,
            Value::Enum(4) => 4.0 / 3.0,
            Value::Enum(5) => 16.0 / 9.0,
            _ => 1.0,
        };
        // Upstream multiplies by the ratio for width and minimum fits, and divides for the other two.
        let (cw, ch, at) = match &v[4] {
            Value::Enum(0) => {
                let cw = v[6].num().abs();
                let ch = (cw / r).ceil();
                let c = v[5].vec2();
                (cw, ch, [(c[0] * w - cw / 2.0).round(), (c[1] * h - ch / 2.0).round()])
            }
            fit => {
                let (cw, ch) = match fit {
                    Value::Enum(1) => (w, w * r),
                    Value::Enum(2) => (h / r, h),
                    _ => (w.min(h * r), w.min(h * r) * r),
                };
                (cw, ch, [(w / 2.0 - cw / 2.0).round(), (h / 2.0 - ch / 2.0).round()])
            }
        };
        (at, checked(cw.trunc().max(1.0), ch.trunc().max(1.0))?)
    };
    out(shader::run(ctx, v, CROP_WGSL, &[("crop_at", [at[0] as f32, at[1] as f32, 0.0, 0.0])], size)?)
}

// ---------- flip ----------

pub const FLIP_IN: &[I] = &[SURFACE_IN, I::new("axis", T::Int, D::Int(1), "Bit mask: 1 = x, 2 = y.").range(0.0, 3.0), MASK, MIX, INVERT_MASK, MASK_FEATHER, CHANNEL];

const FLIP_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let s = vec2<i32>(p.surface_in_info.xy);
    var q = px;
    if ((p.axis.x & 1) != 0) { q.x = s.x - 1 - q.x; }
    if ((p.axis.x & 2) != 0) { q.y = s.y - 1 - q.y; }
    return load_surface_in(q);
}
";

pub fn flip(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    out(shader::run(ctx, v, FLIP_WGSL, &[], [s.width, s.height])?)
}

// ---------- offset ----------

pub const OFFSET_IN: &[I] = &[
    SURFACE_IN,
    I::new("x_offset", T::Number, D::Number(0.0), "Horizontal shift as a fraction of the width (wraps).").range(-1.0, 1.0),
    I::new("y_offset", T::Number, D::Number(0.0), "Vertical shift as a fraction of the height (wraps).").range(-1.0, 1.0),
    I::new("angle", T::Number, D::Number(0.0), "Turn the shift by this many degrees."),
    MASK,
    MIX,
    INVERT_MASK,
    MASK_FEATHER,
];

const OFFSET_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let s = vec2<f32>(p.surface_in_info.xy);
    let d = -vec2<f32>(p.x_offset.x, p.y_offset.x);
    let uv = (vec2<f32>(px) + 0.5) / s + p.rot.z * vec2<f32>(d.x * p.rot.x - d.y * p.rot.y, d.x * p.rot.y + d.y * p.rot.x);
    return load_surface_in(vec2<i32>(floor(fract(uv) * s)));
}
";

pub fn offset(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let (c, si) = super::cos_sin(v[3].num());
    let k = 1.0 / c.abs().max(si.abs());
    out(shader::run(ctx, v, OFFSET_WGSL, &[("rot", [c, si, k, 0.0])], [s.width, s.height])?)
}

// ---------- downscale ----------

pub const DOWNSCALE_IN: &[I] = &[
    SURFACE_IN,
    I::new("mode", T::Enum, D::Enum("mix"), "How a block becomes one pixel.").options(&["mix", "max", "min"]),
    I::new("downscale", T::Number, D::Number(1.0), "Block size in pixels.").min(1.0),
    I::new("multiply_alpha", T::Bool, D::Bool(false), "Multiply each sample's colour by its alpha first."),
];

// Ported from upstream shaders/sh_downscale.fsh.
const DOWNSCALE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let s = p.downscale.x;
    let isz = vec2<f32>(p.surface_in_info.xy);
    let base = vec2<f32>(px) * s + 0.5;
    let mode = p.mode_.x;
    var cc = vec4<f32>(0.0);
    if (mode == 2u) { cc = vec4<f32>(1.0); }
    var aa = 0.0;
    var n = 0.0;
    for (var i = 0.0; i < s; i += 1.0) {
        for (var j = 0.0; j < s; j += 1.0) {
            let sp = base + vec2<f32>(i, j);
            if (sp.x > isz.x || sp.y > isz.y) { continue; }
            var c = load_surface_in(vec2<i32>(floor(sp)));
            if (p.multiply_alpha.x == 1u) { c = vec4<f32>(c.rgb * c.a, c.a); }
            n += 1.0;
            if (mode == 0u) { cc += c * c.a; aa += c.a; }
            else if (mode == 1u) { cc = max(cc, c); }
            else { cc = min(cc, c); }
        }
    }
    if (mode == 0u && aa > 0.0) { cc = vec4<f32>(cc.rgb / aa, cc.a / n); }
    if (mode == 2u) {
        let uv = (vec2<f32>(px) + 0.5) / vec2<f32>(p.out_info.xy);
        cc.a = load_surface_in(vec2<i32>(floor(uv * isz))).a;
    }
    return cc;
}
";

pub fn downscale(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let k = v[2].num();
    let size = [((s.width as f64 / k).ceil() as u32).max(1), ((s.height as f64 / k).ceil() as u32).max(1)];
    out(shader::run(ctx, v, DOWNSCALE_WGSL, &[], size)?)
}
