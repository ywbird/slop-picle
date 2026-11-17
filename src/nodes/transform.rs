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
        Value::Enum(1) => dim(&v[3]),
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

pub const CROP_IN: &[I] = &[SURFACE_IN, I::new("crop", T::Padding, D::Json("[0,0,0,0]"), "Pixels removed: right, top, left, bottom.")];

const CROP_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    return load_surface_in(px + vec2<i32>(i32(p.crop.z), i32(p.crop.y)));
}
";

pub fn crop(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    let Value::Padding([r, t, l, b]) = v[1] else { unreachable!() };
    let size = checked(s.width as f64 - r as f64 - l as f64, s.height as f64 - t as f64 - b as f64)?;
    out(shader::run(ctx, v, CROP_WGSL, &[], size)?)
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
    MASK,
    MIX,
    INVERT_MASK,
    MASK_FEATHER,
];

const OFFSET_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let s = vec2<f32>(p.surface_in_info.xy);
    let uv = (vec2<f32>(px) + 0.5) / s + vec2<f32>(p.x_offset.x, p.y_offset.x);
    return load_surface_in(vec2<i32>(floor(fract(uv) * s)));
}
";

pub fn offset(ctx: &mut Ctx, v: &[Value]) -> R {
    let s = surf(&v[0]);
    out(shader::run(ctx, v, OFFSET_WGSL, &[], [s.width, s.height])?)
}

// ---------- downscale ----------

pub const DOWNSCALE_IN: &[I] = &[
    SURFACE_IN,
    I::new("mode", T::Enum, D::Enum("mix"), "How a block becomes one pixel.").options(&["mix", "max", "min"]),
    I::new("downscale", T::Number, D::Number(1.0), "Block size in pixels.").min(1.0),
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
            let c = load_surface_in(vec2<i32>(floor(sp)));
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
