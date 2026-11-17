//! Node definitions (spec/nodes.md). Upstream: Pixel Composer `scripts/node_*`, `shaders/sh_*` (MIT).

use crate::catalog::{Default as D, InputDef as I, NodeDef, OutputDef as O, Remap};
use crate::eval::{Ctx, NodeError};
use crate::shader;
use crate::surface::{self, Surface};
use crate::types::{Type as T, Value};
use std::sync::Arc;

mod anim;
mod color;
mod compose;
mod filter;
mod generate;
mod transform;
mod value;

type R = Result<Vec<Value>, NodeError>;

const SURFACE_OUT: &[O] = &[O { name: "surface_out", ty: T::Surface }];

pub fn dim(v: &Value) -> [u32; 2] {
    match v {
        Value::Dimension(d) => *d,
        _ => [1, 1],
    }
}

fn out(s: Surface) -> R {
    Ok(vec![Value::Surface(Arc::new(s))])
}

fn surf(v: &Value) -> &Arc<Surface> {
    v.surface().expect("required surface")
}

/// Snap cos/sin of multiples of 90° to exact values.
pub fn cos_sin(deg: f64) -> (f32, f32) {
    let r = deg.rem_euclid(360.0);
    if r % 90.0 == 0.0 {
        return [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][(r / 90.0) as usize];
    }
    let (s, c) = r.to_radians().sin_cos();
    (c as f32, s as f32)
}

// ---------- solid ----------

const SOLID_IN: &[I] = &[
    I::new("dimension", T::Dimension, D::ProjectSize, "Output size; absent means the project size."),
    I::new("color", T::Color, D::Color("#ffffff"), "Fill colour."),
];

fn solid(ctx: &mut Ctx, v: &[Value]) -> R {
    out(shader::run(ctx, v, "fn shade(px: vec2<i32>) -> vec4<f32> { return p.color; }", &[], dim(&v[0]))?)
}

// ---------- noise ----------

const NOISE_IN: &[I] = &[
    I::new("dimension", T::Dimension, D::ProjectSize, "Output size; absent means the project size."),
    I::new("seed", T::Int, D::Int(0), "Random seed."),
    I::new("color_mode", T::Enum, D::Enum("greyscale"), "Grey, or random channels in RGB or HSV.").options(&["greyscale", "rgb", "hsv"]),
    I::new("color_r_range", T::Vec2, D::Vec2([0.0, 1.0]), "Range of red (or hue).").range(0.0, 1.0),
    I::new("color_g_range", T::Vec2, D::Vec2([0.0, 1.0]), "Range of green (or saturation).").range(0.0, 1.0),
    I::new("color_b_range", T::Vec2, D::Vec2([0.0, 1.0]), "Range of blue (or value).").range(0.0, 1.0),
];

const NOISE_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let h = hash2(px.x, px.y, p.seed.x);
    if (p.color_mode.x == 0u) {
        let g = f32(h >> 24u) / 255.0;
        return vec4<f32>(g, g, g, 1.0);
    }
    let u = vec3<f32>(unit(pcg(h)), unit(pcg(h + 1u)), unit(pcg(h + 2u)));
    let lo = vec3<f32>(p.color_r_range.x, p.color_g_range.x, p.color_b_range.x);
    let hi = vec3<f32>(p.color_r_range.y, p.color_g_range.y, p.color_b_range.y);
    let v = lo + u * (hi - lo);
    if (p.color_mode.x == 1u) { return vec4<f32>(v, 1.0); }
    return vec4<f32>(hsv2rgb(v), 1.0);
}
";

fn noise(ctx: &mut Ctx, v: &[Value]) -> R {
    out(shader::run(ctx, v, NOISE_WGSL, &[], dim(&v[0]))?)
}

// ---------- transform ----------

const TRANSFORM_IN: &[I] = &[
    I::required("surface_in", T::Surface, "Image to transform."),
    I::new("render_mode", T::Enum, D::Enum("normal"), "Outside the image: transparent, or wrap around.").options(&["normal", "wrap"]),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Where the anchor lands, as a fraction of the output size."),
    I::new("anchor", T::Vec2, D::Vec2([0.5, 0.5]), "Pivot, as a fraction of the input size."),
    I::new("rotation", T::Number, D::Number(0.0), "Degrees, counter-clockwise."),
    I::new("scale", T::Vec2, D::Vec2([1.0, 1.0]), "Scale per axis; negative mirrors."),
    I::new("alpha", T::Number, D::Number(1.0), "Opacity multiplier.").range(0.0, 1.0),
];

const TRANSFORM_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let size = vec2<f32>(p.surface_in_info.xy);
    let d = vec2<f32>(px) + 0.5 - p.position.xy * size;
    let c = p.rot.x;
    let s = p.rot.y;
    let r = vec2<f32>(d.x * c - d.y * s, d.x * s + d.y * c);
    let u = p.anchor.xy * size + r / p.scale.xy;
    var q = vec2<i32>(floor(u));
    if (p.render_mode.x == 1u) {
        let si = vec2<i32>(p.surface_in_info.xy);
        q = ((q % si) + si) % si;
    }
    var col = load_surface_in(q);
    col.a = col.a * p.alpha.x;
    return col;
}
";

fn transform(ctx: &mut Ctx, v: &[Value]) -> R {
    let src = surf(&v[0]);
    let scale = v[5].vec2();
    if scale[0] == 0.0 || scale[1] == 0.0 {
        return Err("scale components must not be 0".into());
    }
    let (c, s) = cos_sin(v[4].num());
    out(shader::run(ctx, v, TRANSFORM_WGSL, &[("rot", [c, s, 0.0, 0.0])], [src.width, src.height])?)
}

// ---------- blend ----------

pub const BLEND_MODES: &[&str] = &[
    "normal", "replace", "multiply", "color_burn", "linear_burn", "minimum", "add", "screen", "color_dodge", "maximum", "overlay", "soft_light", "hard_light",
    "vivid_light", "linear_light", "pin_light", "difference", "exclusion", "subtract", "divide", "hue", "saturation", "luminosity", "equal", "unequal",
];

const BLEND_IN: &[I] = &[
    I::required("background", T::Surface, "Bottom image, stretched to the output size."),
    I::new("foreground", T::Surface, D::None, "Top image, placed by fill_mode. Absent: output is the background."),
    I::new("mask", T::Surface, D::None, "Where to blend: mean of rgb times alpha, stretched to the output size."),
    I::new("swap", T::Bool, D::Bool(false), "Swap background and foreground."),
    I::new("invert_mask", T::Bool, D::Bool(false), "Invert the mask."),
    I::new("mask_feather", T::Number, D::Number(1.0), "Blur the mask by this size; 1 is none.").range(1.0, 16.0),
    I::new("output_dimension", T::Enum, D::Enum("background"), "Output size from this input, the largest, or constant_dimension.")
        .options(&["background", "foreground", "mask", "maximum", "constant"]),
    I::new("constant_dimension", T::Dimension, D::ProjectSize, "Output size when output_dimension is constant."),
    I::new("blend_mode", T::Enum, D::Enum("normal"), "Blend mode (upstream formulas).").options(BLEND_MODES),
    I::new("opacity", T::Number, D::Number(1.0), "Foreground opacity.").range(0.0, 1.0),
    I::new("preserve_alpha", T::Bool, D::Bool(false), "Keep the background's alpha."),
    I::new("fill_mode", T::Enum, D::Enum("none"), "Foreground placed at position, stretched, or tiled.").options(&["none", "stretch", "tile"]),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Foreground centre, as a fraction of the output size."),
];

/// Upstream `shaders/sh_blend_*` as one function, `mode` indexing `BLEND_MODES` (spec/nodes.md#blend--compose).
pub const BLEND_FN: &str = "
fn blend_div(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    return select(a / b, sign(a) * 1e30, b == vec4<f32>(0.0));
}
fn blend_rgb2hsl(c: vec3<f32>) -> vec3<f32> {
    let mn = min(c.r, min(c.g, c.b));
    let mx = max(c.r, max(c.g, c.b));
    let l = (mx + mn) / 2.0;
    if (mx <= mn) { return vec3<f32>(0.0, 0.0, l); }
    let d = mx - mn;
    let s = select(d / (2.0 - (mx + mn)), d / (mx + mn), l < 0.5);
    var h: f32;
    if (c.r == mx) { h = (c.g - c.b) / d; } else if (c.g == mx) { h = 2.0 + (c.b - c.r) / d; } else { h = 4.0 + (c.r - c.g) / d; }
    if (h < 0.0) { h += 6.0; }
    return vec3<f32>(h / 6.0, s, l);
}
fn blend_hue2rgb(m1: f32, m2: f32, h0: f32) -> f32 {
    var h = h0;
    if (h < 0.0) { h += 1.0; } else if (h > 1.0) { h -= 1.0; }
    if (6.0 * h < 1.0) { return m1 + (m2 - m1) * h * 6.0; }
    if (2.0 * h < 1.0) { return m2; }
    if (3.0 * h < 2.0) { return m1 + (m2 - m1) * (2.0 / 3.0 - h) * 6.0; }
    return m1;
}
fn blend_hsl2rgb(c: vec3<f32>) -> vec3<f32> {
    if (c.y == 0.0) { return vec3<f32>(c.z); }
    let m2 = select(c.z + c.y - c.z * c.y, c.z * (1.0 + c.y), c.z <= 0.5);
    let m1 = 2.0 * c.z - m2;
    return vec3<f32>(blend_hue2rgb(m1, m2, c.x + 1.0 / 3.0), blend_hue2rgb(m1, m2, c.x), blend_hue2rgb(m1, m2, c.x - 1.0 / 3.0));
}
fn blend_px(B0: vec4<f32>, F0: vec4<f32>, o: f32, m: f32, mode: u32, pa: bool) -> vec4<f32> {
    var B = B0;
    var F = F0;
    let om = o * m;
    var res = B;
    var al = 0.0;
    var divide = true;
    switch mode {
        case 0u: {
            F.a = F.a * om;
            al = F.a + B.a * (1.0 - F.a);
            if (al == 0.0) { return vec4<f32>(0.0); }
            res = (F * F.a + B * B.a * (1.0 - F.a)) / al;
            res.a = al;
            divide = false;
        }
        case 1u: {
            res = mix(B, F, om);
            divide = false;
        }
        case 2u: {
            res = mix(B, B * F, om);
            al = F.a + B.a * (1.0 - F.a);
        }
        case 5u, 9u: {
            F = F * om;
            res = select(max(B, F), min(B, F), mode == 5u);
            divide = false;
        }
        case 6u, 18u: {
            B = vec4<f32>(B.rgb * B.a, B.a);
            F.a = F.a * om;
            F = vec4<f32>(F.rgb * F.a, F.a);
            al = F.a + B.a * (1.0 - F.a);
            if (mode == 6u) { res = B + F; } else { res = mix(B, B - F, select(o, F.a, pa)); }
        }
        case 7u: {
            F = vec4<f32>(F.rgb * F.a, F.a);
            res = mix(B, vec4<f32>(1.0) - (vec4<f32>(1.0) - B) * (vec4<f32>(1.0) - F), om);
            al = F.a + B.a * (1.0 - F.a);
        }
        case 10u: {
            F.a = F.a * om;
            var X = 2.0 * F * B;
            if (lum(F.rgb) > 0.5) { X = vec4<f32>(1.0) - (vec4<f32>(1.0) - 2.0 * (F - 0.5)) * (vec4<f32>(1.0) - B); }
            res = mix(B, X, select(o, F.a, pa));
            al = F.a + B.a * (1.0 - F.a);
        }
        case 16u: {
            return clamp(mix(B, vec4<f32>(abs(B.rgb - F.rgb), 1.0), om), vec4<f32>(0.0), vec4<f32>(1.0));
        }
        case 23u, 24u: {
            res = vec4<f32>(select(0.0, om, all(B == F) == (mode == 23u)));
            divide = false;
        }
        case 20u, 21u, 22u: {
            al = F.a + B.a * (1.0 - F.a);
            let t = F.a * om;
            var rgb: vec3<f32>;
            if (mode == 20u) {
                let h0 = rgb2hsv(B.rgb);
                let h1 = rgb2hsv(F.rgb);
                rgb = mix(B.rgb, hsv2rgb(vec3<f32>(h1.x, h0.y, h0.z)), t);
            } else if (mode == 21u) {
                var h0 = rgb2hsv(B.rgb);
                h0.y = mix(h0.y, rgb2hsv(F.rgb).y, t);
                rgb = hsv2rgb(h0);
            } else {
                var h0 = blend_rgb2hsl(B.rgb);
                h0.z = mix(h0.z, blend_rgb2hsl(F.rgb).z, t);
                rgb = blend_hsl2rgb(h0);
            }
            res = vec4<f32>(rgb, B.a);
        }
        default: {
            F.a = F.a * om;
            let one = vec4<f32>(1.0);
            let L = lum(F.rgb) > 0.5;
            var X: vec4<f32>;
            switch mode {
                case 3u: { X = one - blend_div(one - B, F); }
                case 4u: { X = B + F - one; }
                case 8u: { X = blend_div(F, one - B); }
                case 11u: { X = select(B * (F + 0.5), one - (one - B) * (one - (F - 0.5)), L); }
                case 12u: { X = select(2.0 * B * F, one - (one - B) * (one - 2.0 * (F - 0.5)), L); }
                case 13u: { X = select(blend_div(B, one - 2.0 * F), one - (one - B) * (2.0 * (F - 0.5)), L); }
                case 14u: { X = B + 2.0 * F - one; }
                case 15u: { X = select(min(B, 2.0 * F), max(B, 2.0 * (F - 0.5)), L); }
                case 17u: { X = 0.5 - 2.0 * (B - 0.5) * (F - 0.5); }
                default: { X = blend_div(B, F); }
            }
            let mx = B * (1.0 - o) + X * o;
            res = mix(B, mx, select(o, F.a, pa));
            al = F.a + B.a * (1.0 - F.a);
        }
    }
    if (divide) {
        if (al == 0.0) { return vec4<f32>(0.0); }
        res = vec4<f32>(res.rgb / al, res.a);
    }
    if (pa) { res.a = B.a; }
    return clamp(res, vec4<f32>(0.0), vec4<f32>(1.0));
}
";

const BLEND_SHADE: &str = "
fn stretch(px: vec2<i32>, src_size: vec2<u32>) -> vec2<i32> {
    return vec2<i32>(floor((vec2<f32>(px) + 0.5) * vec2<f32>(src_size) / vec2<f32>(p.out_info.xy)));
}
fn shade(px: vec2<i32>) -> vec4<f32> {
    let B = load_background(stretch(px, p.background_info.xy));
    if (p.foreground_info.z == 0u) { return B; }
    let fs = vec2<i32>(p.foreground_info.xy);
    var F: vec4<f32>;
    switch p.fill_mode.x {
        case 1u: { F = load_foreground(stretch(px, p.foreground_info.xy)); }
        case 2u: { F = load_foreground(((px % fs) + fs) % fs); }
        default: { F = load_foreground(px - vec2<i32>(p.fg_off.xy)); }
    }
    var m = 1.0;
    if (p.mask_info.z == 1u) { m = load_mask(stretch(px, p.mask_info.xy)).r; }
    return blend_px(B, F, p.opacity.x, m, p.blend_mode.x, p.preserve_alpha.x == 1u);
}
";

/// Mask strength (mean of rgb, inverted if asked, times alpha), feathered, as an opaque grey image.
fn mask_strength(m: &Surface, invert: bool, feather: f64) -> Surface {
    let (w, h) = (m.width as usize, m.height as usize);
    let mut s: Vec<f64> = m
        .data
        .chunks_exact(4)
        .map(|p| {
            let mean = (p[0] as f64 + p[1] as f64 + p[2] as f64) / 765.0;
            (if invert { 1.0 - mean } else { mean }) * p[3] as f64 / 255.0
        })
        .collect();
    let (r, sigma) = filter::kernel(feather);
    if r > 0 {
        s = filter::blur_alpha(&filter::blur_alpha(&s, w, h, r, sigma, true), w, h, r, sigma, false);
    }
    let mut out = Surface::new(m.width, m.height);
    for (px, v) in out.data.chunks_exact_mut(4).zip(s) {
        let g = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        px.copy_from_slice(&[g, g, g, 255]);
    }
    out
}

fn blend(ctx: &mut Ctx, v: &[Value]) -> R {
    let mut v = v.to_vec();
    if v[3] == Value::Bool(true) {
        v.swap(0, 1);
    }
    let size = |i: usize| v[i].surface().map_or([1, 1], |s| [s.width, s.height]);
    let out_size = match v[6] {
        Value::Enum(0) => size(0),
        Value::Enum(1) => size(1),
        Value::Enum(2) => size(2),
        Value::Enum(3) => [0, 1, 2].map(size).into_iter().fold([1, 1], |a, s| [a[0].max(s[0]), a[1].max(s[1])]),
        _ => dim(&v[7]),
    };
    if !surface::size_ok(out_size[0], out_size[1]) {
        return Err(format!("output would be {}x{}, outside the size limits", out_size[0], out_size[1]).into());
    }
    if v[0].surface().is_none() {
        return out(Surface::new(out_size[0], out_size[1]));
    }
    if let Some(m) = v[2].surface() {
        v[2] = Value::Surface(Arc::new(mask_strength(m, v[4] == Value::Bool(true), v[5].num())));
    }
    let off = match v[1].surface() {
        Some(fg) => {
            let pos = v[12].vec2();
            [(pos[0] * out_size[0] as f64 - fg.width as f64 / 2.0).floor() as f32, (pos[1] * out_size[1] as f64 - fg.height as f64 / 2.0).floor() as f32]
        }
        None => [0.0, 0.0],
    };
    out(shader::run(ctx, &v, &format!("{BLEND_FN}{BLEND_SHADE}"), &[("fg_off", [off[0], off[1], 0.0, 0.0])], out_size)?)
}

// ---------- image ----------

const IMAGE_IN: &[I] = &[I::required("path", T::Path, "PNG file, relative to the project file.")];

fn image(ctx: &mut Ctx, v: &[Value]) -> R {
    let Value::Path(rel) = &v[0] else { unreachable!() };
    let bytes = std::fs::read(ctx.path(rel)?).map_err(|e| format!("cannot read '{rel}': {e}"))?;
    out(Surface::from_png(&bytes).map_err(|e| format!("'{rel}': {e}"))?)
}

// ---------- output ----------

const OUTPUT_IN: &[I] = &[I::required("surface_in", T::Surface, "What `slopc render` writes.")];

fn output(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![v[0].clone()])
}

const fn node(ty: &'static str, category: &'static str, description: &'static str, inputs: &'static [I], run: crate::catalog::RunFn) -> NodeDef {
    NodeDef { ty, category, description, inputs, outputs: SURFACE_OUT, run, reads_frame: false, remap: None }
}

const fn value_node(ty: &'static str, description: &'static str, inputs: &'static [I], outputs: &'static [O], run: crate::catalog::RunFn) -> NodeDef {
    NodeDef { ty, category: "value", description, inputs, outputs, run, reads_frame: false, remap: None }
}

const fn remap_node(ty: &'static str, description: &'static str, inputs: &'static [I], output: &'static str, remap: crate::catalog::RemapFn) -> NodeDef {
    NodeDef {
        ty,
        category: "animation",
        description,
        inputs,
        outputs: match output.as_bytes() {
            b"output" => &[O { name: "output", ty: T::Surface }],
            b"surface_out" => SURFACE_OUT,
            _ => &[O { name: "surface", ty: T::Surface }],
        },
        run: anim::no_run,
        reads_frame: true,
        remap: Some(Remap::Frame(remap)),
    }
}

pub static ALL: &[NodeDef] = &[
    NodeDef { ty: "image", category: "io", description: "Read a PNG file.", inputs: IMAGE_IN, outputs: SURFACE_OUT, run: image, reads_frame: false, remap: None },
    NodeDef { ty: "output", category: "io", description: "Mark the image `slopc render` writes.", inputs: OUTPUT_IN, outputs: SURFACE_OUT, run: output, reads_frame: false, remap: None },
    NodeDef { ty: "solid", category: "generate", description: "Fill a surface with one colour.", inputs: SOLID_IN, outputs: SURFACE_OUT, run: solid, reads_frame: false, remap: None },
    NodeDef { ty: "noise", category: "generate", description: "White noise, one random value per pixel.", inputs: NOISE_IN, outputs: SURFACE_OUT, run: noise, reads_frame: false, remap: None },
    NodeDef {
        ty: "transform",
        category: "transform",
        description: "Move, rotate and scale an image around an anchor.",
        inputs: TRANSFORM_IN,
        outputs: SURFACE_OUT,
        run: transform,
        reads_frame: false,
        remap: None,
    },
    NodeDef { ty: "blend", category: "compose", description: "Draw a foreground over a background with a blend mode.", inputs: BLEND_IN, outputs: SURFACE_OUT, run: blend, reads_frame: false, remap: None },
    NodeDef {
        remap: Some(Remap::Frames(compose::sheet_frames)),
        ..node("render_sprite_sheet", "compose", "Lay out the frames of an animation in one image.", compose::SHEET_IN, compose::render_sprite_sheet)
    },
    node("gradient", "generate", "Draw a linear, circular or radial gradient.", generate::GRADIENT_IN, generate::gradient),
    NodeDef { outputs: &[O { name: "colored", ty: T::Surface }], ..node("shape", "generate", "Draw a rectangle or ellipse.", generate::SHAPE_IN, generate::shape) },
    node("cellular", "generate", "Cellular (Worley) noise: distance to the nearest feature point.", generate::CELLULAR_IN, generate::cellular),
    node("checker", "generate", "Checkerboard pattern.", generate::CHECKER_IN, generate::checker),
    node("stripe", "generate", "Alternating stripes.", generate::STRIPE_IN, generate::stripe),
    node("scale", "transform", "Resize with nearest-neighbour sampling.", transform::SCALE_IN, transform::scale),
    node("crop", "transform", "Remove pixels from the edges.", transform::CROP_IN, transform::crop),
    node("flip", "transform", "Mirror horizontally and/or vertically.", transform::FLIP_IN, transform::flip),
    node("offset", "transform", "Shift with wrap-around.", transform::OFFSET_IN, transform::offset),
    node("downscale", "transform", "Shrink by merging blocks of pixels.", transform::DOWNSCALE_IN, transform::downscale),
    node("blur", "filter", "Gaussian blur.", filter::BLUR_IN, filter::blur),
    node("outline", "filter", "Draw an outline inside or outside opaque areas.", filter::OUTLINE_IN, filter::outline),
    node("shadow", "filter", "Drop shadow behind opaque areas.", filter::SHADOW_IN, filter::shadow),
    node("posterize", "filter", "Reduce colours to a palette or a number of steps.", filter::POSTERIZE_IN, filter::posterize),
    node("dither", "filter", "Ordered or noise dithering between two nearest colours.", filter::DITHER_IN, filter::dither),
    node("threshold", "filter", "Turn brightness and/or alpha into hard or soft steps.", filter::THRESHOLD_IN, filter::threshold),
    node("displace", "filter", "Move pixels by the brightness of a map.", filter::DISPLACE_IN, filter::displace),
    node("color_adjust", "color", "Brightness, contrast, exposure, HSV and colour blend.", color::COLOR_ADJUST_IN, color::color_adjust),
    node("invert", "color", "Invert colours.", color::INVERT_IN, color::invert),
    node("color_replace", "color", "Replace palette colours with others.", color::COLOR_REPLACE_IN, color::color_replace),
    node("colorize", "color", "Map brightness to a gradient (gradient map).", color::COLORIZE_IN, color::colorize),
    node("greyscale", "color", "Convert to grey.", color::GREYSCALE_IN, color::greyscale),
    value_node("number", "A number.", value::NUMBER_IN, &[O { name: "number", ty: T::Number }], value::number),
    value_node("color", "A colour.", value::COLOR_IN, &[O { name: "color", ty: T::Color }], value::color),
    value_node("palette", "A list of colours.", value::PALETTE_IN, &[O { name: "palette", ty: T::Palette }], value::palette),
    value_node("math", "Arithmetic on two numbers.", value::MATH_IN, &[O { name: "result", ty: T::Number }], value::math),
    NodeDef { reads_frame: true, category: "animation", ..value_node("wiggler", "Smooth random value over time.", anim::WIGGLER_IN, &[O { name: "output", ty: T::Number }], anim::wiggler) },
    NodeDef { reads_frame: true, category: "animation", ..value_node("fn_ease", "Ease in and out over the animation.", anim::FN_EASE_IN, &[O { name: "output", ty: T::Number }], anim::fn_ease) },
    NodeDef { category: "animation", ..value_node("fn_smoothstep", "Smoothstep curve of a value.", anim::FN_SMOOTHSTEP_IN, &[O { name: "output", ty: T::Number }], anim::fn_smoothstep) },
    NodeDef { category: "animation", ..value_node("fn_math", "Add, subtract or multiply two values.", anim::FN_MATH_IN, &[O { name: "output", ty: T::Number }], anim::fn_math) },
    remap_node("revert", "Play an animation backwards.", anim::REVERT_IN, "output", anim::revert_frame),
    remap_node("anim_speed", "Change an animation's playback speed.", anim::ANIM_SPEED_IN, "surface_out", anim::anim_speed_frame),
    remap_node("rate_remap", "Resample an animation to a lower frame rate.", anim::RATE_REMAP_IN, "surface", anim::rate_remap_frame),
    remap_node("delay", "Delay an animation by some frames.", anim::DELAY_IN, "surface", anim::delay_frame),
    remap_node("anim_loop", "Loop part of an animation.", anim::ANIM_LOOP_IN, "surface", anim::anim_loop_frame),
    remap_node("stagger", "Delay an animation by a curve over its elements (one element without arrays).", anim::STAGGER_IN, "surface", anim::stagger_frame),
    NodeDef {
        ty: "delay_value",
        category: "animation",
        description: "Delay any value by some frames.",
        inputs: anim::DELAY_VALUE_IN,
        outputs: &[O { name: "value", ty: T::Any }],
        run: anim::delay_value_default,
        reads_frame: true,
        remap: Some(Remap::Frame(anim::delay_value_frame)),
    },
    NodeDef { reads_frame: true, category: "animation", ..value_node("anim_curve", "Evaluate a curve at a progress or over the animation.", anim::ANIM_CURVE_IN, &[O { name: "curve", ty: T::Number }], anim::anim_curve) },
    NodeDef { reads_frame: true, ..value_node("counter", "Frame index or animation progress.", value::COUNTER_IN, &[O { name: "value", ty: T::Number }], value::counter) },
];
