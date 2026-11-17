//! Shader-node helper: WGSL prelude and uniforms generated from the catalog entry (spec/evaluation.md#gpu).

use crate::catalog::NodeDef;
use crate::eval::{Ctx, NodeError};
use crate::gpu::ENTRY;
use crate::surface::Surface;
use crate::types::{MAX_KEYS, MAX_PALETTE, Type, Value};

const HELPERS: &str = "
fn pcg(v: u32) -> u32 {
    let state = v * 747796405u + 2891336453u;
    let word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}
fn hash2(x: i32, y: i32, seed: i32) -> u32 {
    return pcg(bitcast<u32>(x) ^ pcg(bitcast<u32>(y) ^ pcg(bitcast<u32>(seed))));
}
fn unit(h: u32) -> f32 { return f32(h >> 8u) / 16777216.0; }
fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)); }
fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let q0 = mix(vec4<f32>(c.bg, K.wz), vec4<f32>(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(q0.xyw, c.r), vec4<f32>(c.r, q0.yzx), step(q0.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 0.0000000001;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}
fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let q = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(q - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}
fn grad_hue(a0: f32, a1: f32, t: f32, inv: bool) -> f32 {
    let da = fract(a1 - a0);
    var ds = fract(2.0 * da) - da;
    if (inv) { ds -= sign(ds); }
    return a0 + ds * t;
}
fn grad_oklab(c: vec3<f32>) -> vec3<f32> {
    let m = mat3x3<f32>(0.4121656120, 0.2118591070, 0.0883097947, 0.5362752080, 0.6807189584, 0.2818474174, 0.0514575653, 0.1074065790, 0.6302613616);
    return pow(max(vec3<f32>(0.0), m * pow(c, vec3<f32>(2.2))), vec3<f32>(1.0 / 3.0));
}
fn grad_unoklab(k: vec3<f32>) -> vec3<f32> {
    let m = mat3x3<f32>(4.0767245293, -1.2681437731, -0.0041119885, -3.3072168827, 2.6093323231, -0.7034763098, 0.2307590544, -0.3411344290, 1.7068625689);
    return pow(max(vec3<f32>(0.0), m * (k * k * k)), vec3<f32>(1.0 / 2.2));
}
fn grad_cmyk(c: vec3<f32>) -> vec4<f32> {
    let k = 1.0 - max(max(c.r, c.g), c.b);
    if (k >= 1.0) { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }
    return vec4<f32>((vec3<f32>(1.0) - c - k) / (1.0 - k), k);
}
// Upstream gradientEval blends between two keys (spec/types.md#gradient-evaluation); `kind` indexes GRADIENT_TYPES.
fn grad_mix(c0: vec4<f32>, c1: vec4<f32>, f: f32, kind: u32) -> vec4<f32> {
    let a = mix(c0.a, c1.a, f);
    switch kind {
        case 1u: { return c0; }
        case 2u, 3u: {
            let h0 = rgb2hsv(c0.rgb);
            let h1 = rgb2hsv(c1.rgb);
            return vec4<f32>(hsv2rgb(vec3<f32>(grad_hue(h0.x, h1.x, f, kind == 3u), mix(h0.yz, h1.yz, f))), a);
        }
        case 4u: { return vec4<f32>(grad_unoklab(mix(grad_oklab(c0.rgb), grad_oklab(c1.rgb), f)), a); }
        case 5u: {
            let q = mix(grad_cmyk(c0.rgb), grad_cmyk(c1.rgb), f);
            return vec4<f32>((vec3<f32>(1.0) - q.xyz) * (1.0 - q.w), a);
        }
        default: { return vec4<f32>(mix(c0.rgb, c1.rgb, f), a); }
    }
}
";

/// `ENTRY` for generators with a UV map: each pixel is drawn as the pixel the map points to (spec/nodes.md#generator-modifiers).
const UV_ENTRY: &str = "
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(pos.xy);
    if (p.uv_map_info.z == 0u) { return shade(px); }
    let size = vec2<f32>(p.out_info.xy);
    let m = load_uv_map(vec2<i32>(floor((vec2<f32>(px) + 0.5) * vec2<f32>(p.uv_map_info.xy) / size)));
    let uv = mix((vec2<f32>(px) + 0.5) / size, vec2<f32>(m.r, 1.0 - m.g), p.uv_mix.x);
    var c = shade(vec2<i32>(floor(uv * size)));
    c.a = c.a * m.a;
    return c;
}
";

/// CPU copies of the shader hash (spec/nodes.md#shared-definitions).
pub fn hash(v: u32) -> u32 {
    let state = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277803737);
    (word >> 22) ^ word
}

pub fn hash2(x: i64, y: i64, seed: i64) -> u32 {
    hash((x as u32) ^ hash((y as u32) ^ hash(seed as u32)))
}

pub fn unit(h: u32) -> f64 {
    (h >> 8) as f64 / 16777216.0
}

/// WGSL keywords and reserved words that may collide with input names; such fields get a trailing `_`.
const RESERVED: &[&str] = &["loop", "type", "override", "default", "const", "enable", "alias", "mod", "filter", "sample", "from", "mode"];

pub fn field(name: &str) -> String {
    if RESERVED.contains(&name) { format!("{name}_") } else { name.to_string() }
}

fn field_type(t: Type) -> Option<&'static str> {
    match t {
        Type::Number | Type::Color | Type::Vec2 => Some("vec4<f32>"),
        Type::Int => Some("vec4<i32>"),
        Type::Bool | Type::Enum | Type::Dimension | Type::Padding => Some("vec4<u32>"),
        Type::Str | Type::Path | Type::Surface | Type::Palette | Type::Gradient | Type::Curve | Type::Any => None,
    }
}

/// The full WGSL module for a node: prelude + `body` (which defines `shade`) + entry points.
pub fn module(def: &NodeDef, extras: &[&str], body: &str) -> String {
    let mut fields = vec!["    out_info: vec4<u32>,".to_string()];
    let mut bindings = String::new();
    let mut loaders = String::new();
    let mut binding = 1;
    for i in def.inputs {
        if i.ty == Type::Surface {
            let n = i.name;
            fields.push(format!("    {n}_info: vec4<u32>,"));
            bindings.push_str(&format!("@group(0) @binding({binding}) var t_{n}: texture_2d<f32>;\n"));
            loaders.push_str(&format!(
                "fn load_{n}(q: vec2<i32>) -> vec4<f32> {{\n    let s = vec2<i32>(p.{n}_info.xy);\n    if (q.x < 0 || q.y < 0 || q.x >= s.x || q.y >= s.y) {{ return vec4<f32>(0.0); }}\n    return textureLoad(t_{n}, q, 0);\n}}\n"
            ));
            binding += 1;
        } else if i.ty == Type::Palette {
            fields.push(format!("    {0}_info: vec4<u32>,\n    {0}_colors: array<vec4<f32>, {MAX_PALETTE}>,", i.name));
        } else if i.ty == Type::Gradient {
            let n = i.name;
            fields.push(format!("    {n}_info: vec4<u32>,\n    {n}_times: array<vec4<f32>, {MAX_KEYS}>,\n    {n}_colors: array<vec4<f32>, {MAX_KEYS}>,"));
            loaders.push_str(&format!(
                "fn eval_{n}(t: f32) -> vec4<f32> {{
    let n = i32(p.{n}_info.x);
    if (t <= p.{n}_times[0].x) {{ return p.{n}_colors[0]; }}
    for (var i = 1; i < n; i++) {{
        let ti = p.{n}_times[i].x;
        if (ti == t) {{ return p.{n}_colors[i]; }}
        if (ti > t) {{
            let f = (t - p.{n}_times[i - 1].x) / (ti - p.{n}_times[i - 1].x);
            return grad_mix(p.{n}_colors[i - 1], p.{n}_colors[i], f, p.{n}_info.y);
        }}
    }}
    return p.{n}_colors[n - 1];
}}
"
            ));
        } else if let Some(t) = field_type(i.ty) {
            fields.push(format!("    {}: {t},", field(i.name)));
        }
    }
    for e in extras {
        fields.push(format!("    {e}: vec4<f32>,"));
    }
    format!(
        "struct Params {{\n{}\n}}\n@group(0) @binding(0) var<uniform> p: Params;\n{bindings}{loaders}{HELPERS}{body}{}",
        fields.join("\n"),
        if def.input("uv_map").is_some() { UV_ENTRY } else { ENTRY }
    )
}

fn push_u(out: &mut Vec<u8>, v: [u32; 4]) {
    out.extend(v.iter().flat_map(|x| x.to_le_bytes()));
}
fn push_f(out: &mut Vec<u8>, v: [f32; 4]) {
    out.extend(v.iter().flat_map(|x| x.to_le_bytes()));
}

/// Uniform bytes matching `module`'s `Params`.
pub fn uniforms(def: &NodeDef, values: &[Value], out: [u32; 2], frame: i64, extras: &[[f32; 4]]) -> Vec<u8> {
    let mut b = vec![];
    push_u(&mut b, [out[0], out[1], frame as u32, 0]);
    for (i, v) in def.inputs.iter().zip(values) {
        match (i.ty, v) {
            (Type::Surface, Value::Surface(s)) => push_u(&mut b, [s.width, s.height, 1, 0]),
            (Type::Surface, _) => push_u(&mut b, [1, 1, 0, 0]),
            (Type::Number, v) => push_f(&mut b, [v.num() as f32, 0.0, 0.0, 0.0]),
            (Type::Int, Value::Int(x)) => b.extend([*x as i32, 0, 0, 0].iter().flat_map(|x| x.to_le_bytes())),
            (Type::Bool, Value::Bool(x)) => push_u(&mut b, [*x as u32, 0, 0, 0]),
            (Type::Enum, Value::Enum(x)) => push_u(&mut b, [*x as u32, 0, 0, 0]),
            (Type::Dimension, Value::Dimension([w, h])) => push_u(&mut b, [*w, *h, 0, 0]),
            (Type::Padding, Value::Padding(v)) => push_u(&mut b, *v),
            (Type::Palette, Value::Palette(cols)) => {
                push_u(&mut b, [cols.len() as u32, 0, 0, 0]);
                for i in 0..MAX_PALETTE {
                    push_f(&mut b, cols.get(i).map_or([0.0; 4], |c| c.map(|x| x as f32 / 255.0)));
                }
            }
            (Type::Gradient, Value::Gradient(g)) => {
                push_u(&mut b, [g.keys.len() as u32, g.kind as u32, 0, 0]);
                for i in 0..MAX_KEYS {
                    push_f(&mut b, [g.keys.get(i).map_or(2.0, |k| k.0 as f32), 0.0, 0.0, 0.0]);
                }
                for i in 0..MAX_KEYS {
                    push_f(&mut b, g.keys.get(i).map_or([0.0; 4], |k| k.1.map(|x| x as f32 / 255.0)));
                }
            }
            (Type::Color, Value::Color(c)) => push_f(&mut b, c.map(|x| x as f32 / 255.0)),
            (Type::Vec2, Value::Vec2([x, y])) => push_f(&mut b, [*x as f32, *y as f32, 0.0, 0.0]),
            (Type::Str | Type::Path | Type::Curve | Type::Any, _) => {}
            (t, v) => panic!("input {} of type {t:?} got {v:?}", i.name),
        }
    }
    for e in extras {
        push_f(&mut b, *e);
    }
    b
}

/// Run a shader node: `body` defines `fn shade(px: vec2<i32>) -> vec4<f32>`.
pub fn run(ctx: &Ctx, values: &[Value], body: &str, extras: &[(&str, [f32; 4])], size: [u32; 2]) -> Result<Surface, NodeError> {
    let gpu = ctx.gpu()?;
    let names: Vec<&str> = extras.iter().map(|e| e.0).collect();
    let vals: Vec<[f32; 4]> = extras.iter().map(|e| e.1).collect();
    let src = module(ctx.def, &names, body);
    let empty = Surface::new(1, 1);
    let textures: Vec<&Surface> = ctx
        .def
        .inputs
        .iter()
        .zip(values)
        .filter(|(i, _)| i.ty == Type::Surface)
        .map(|(_, v)| v.surface().map_or(&empty, |s| s.as_ref()))
        .collect();
    let u = uniforms(ctx.def, values, size, ctx.frame, &vals);
    Ok(gpu.render(ctx.def.ty, &src, &u, &textures, size[0], size[1]))
}
