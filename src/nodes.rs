//! Node definitions (spec/nodes.md). Upstream: Pixel Composer `scripts/node_*`, `shaders/sh_*` (MIT).

use crate::catalog::{Default as D, InputDef as I, NodeDef, OutputDef as O};
use crate::eval::{Ctx, NodeError};
use crate::shader;
use crate::surface::Surface;
use crate::types::{Type as T, Value};
use std::sync::Arc;

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
fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let q = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(q - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}
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

pub const BLEND_MODES: &[&str] = &["normal", "replace", "multiply", "add", "screen"];

const BLEND_IN: &[I] = &[
    I::required("background", T::Surface, "Bottom image; sets the output size."),
    I::new("foreground", T::Surface, D::None, "Top image, centred on position. Absent: output is the background."),
    I::new("blend_mode", T::Enum, D::Enum("normal"), "Blend mode (upstream formulas).").options(BLEND_MODES),
    I::new("opacity", T::Number, D::Number(1.0), "Foreground opacity.").range(0.0, 1.0),
    I::new("preserve_alpha", T::Bool, D::Bool(false), "Keep the background's alpha."),
    I::new("position", T::Vec2, D::Vec2([0.5, 0.5]), "Foreground centre, as a fraction of the background size."),
];

// Ported from upstream shaders/sh_blend_{normal,replace,multiply,add,screen}.fsh.
const BLEND_WGSL: &str = "
fn shade(px: vec2<i32>) -> vec4<f32> {
    let B = load_background(px);
    if (p.foreground_info.z == 0u) { return B; }
    let F = load_foreground(px - vec2<i32>(p.fg_off.xy));
    let o = p.opacity.x;
    let mode = p.blend_mode.x;
    var res: vec4<f32>;
    var fa = F.a;
    if (mode == 0u || mode == 3u) { fa = F.a * o; }
    let al = fa + B.a * (1.0 - fa);
    if (mode == 0u) {
        if (al == 0.0) { return vec4<f32>(0.0); }
        res = vec4<f32>((F.rgb * fa + B.rgb * B.a * (1.0 - fa)) / al, al);
    } else if (mode == 1u) {
        res = mix(B, F, o);
    } else {
        if (mode == 2u) {
            res = mix(B, B * F, o);
        } else if (mode == 3u) {
            res = vec4<f32>(B.rgb * B.a + F.rgb * fa, B.a + fa);
        } else {
            let F2 = vec4<f32>(F.rgb * F.a, F.a);
            res = mix(B, vec4<f32>(1.0) - (vec4<f32>(1.0) - B) * (vec4<f32>(1.0) - F2), o);
        }
        if (al == 0.0) { return vec4<f32>(0.0); }
        res = vec4<f32>(res.rgb / al, res.a);
    }
    if (p.preserve_alpha.x == 1u) { res.a = B.a; }
    return clamp(res, vec4<f32>(0.0), vec4<f32>(1.0));
}
";

fn blend(ctx: &mut Ctx, v: &[Value]) -> R {
    let bg = surf(&v[0]).clone();
    let off = match v[1].surface() {
        Some(fg) => {
            let pos = v[5].vec2();
            [
                (pos[0] * bg.width as f64 - fg.width as f64 / 2.0).floor() as f32,
                (pos[1] * bg.height as f64 - fg.height as f64 / 2.0).floor() as f32,
            ]
        }
        None => [0.0, 0.0],
    };
    out(shader::run(ctx, v, BLEND_WGSL, &[("fg_off", [off[0], off[1], 0.0, 0.0])], [bg.width, bg.height])?)
}

// ---------- image ----------

const IMAGE_IN: &[I] = &[I::required("path", T::Path, "PNG file, relative to the project file.")];

fn image(ctx: &mut Ctx, v: &[Value]) -> R {
    let Value::Path(rel) = &v[0] else { unreachable!() };
    let bytes = std::fs::read(ctx.path(rel)).map_err(|e| format!("cannot read '{rel}': {e}"))?;
    out(Surface::from_png(&bytes).map_err(|e| format!("'{rel}': {e}"))?)
}

// ---------- output ----------

const OUTPUT_IN: &[I] = &[I::required("surface_in", T::Surface, "What `slopc render` writes.")];

fn output(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![v[0].clone()])
}

pub static ALL: &[NodeDef] = &[
    NodeDef { ty: "image", category: "io", description: "Read a PNG file.", inputs: IMAGE_IN, outputs: SURFACE_OUT, run: image, reads_frame: false },
    NodeDef { ty: "output", category: "io", description: "Mark the image `slopc render` writes.", inputs: OUTPUT_IN, outputs: SURFACE_OUT, run: output, reads_frame: false },
    NodeDef { ty: "solid", category: "generate", description: "Fill a surface with one colour.", inputs: SOLID_IN, outputs: SURFACE_OUT, run: solid, reads_frame: false },
    NodeDef { ty: "noise", category: "generate", description: "White noise, one random value per pixel.", inputs: NOISE_IN, outputs: SURFACE_OUT, run: noise, reads_frame: false },
    NodeDef {
        ty: "transform",
        category: "transform",
        description: "Move, rotate and scale an image around an anchor.",
        inputs: TRANSFORM_IN,
        outputs: SURFACE_OUT,
        run: transform,
        reads_frame: false,
    },
    NodeDef { ty: "blend", category: "compose", description: "Draw a foreground over a background with a blend mode.", inputs: BLEND_IN, outputs: SURFACE_OUT, run: blend, reads_frame: false },
];
