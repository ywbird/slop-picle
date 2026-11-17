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
";

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
        Type::Str | Type::Path | Type::Surface | Type::Palette | Type::Gradient => None,
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
            if (p.{n}_info.y == 1u) {{ return p.{n}_colors[i - 1]; }}
            let f = (t - p.{n}_times[i - 1].x) / (ti - p.{n}_times[i - 1].x);
            return mix(p.{n}_colors[i - 1], p.{n}_colors[i], f);
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
        "struct Params {{\n{}\n}}\n@group(0) @binding(0) var<uniform> p: Params;\n{bindings}{loaders}{HELPERS}{body}{ENTRY}",
        fields.join("\n")
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
                push_u(&mut b, [g.keys.len() as u32, g.constant as u32, 0, 0]);
                for i in 0..MAX_KEYS {
                    push_f(&mut b, [g.keys.get(i).map_or(2.0, |k| k.0 as f32), 0.0, 0.0, 0.0]);
                }
                for i in 0..MAX_KEYS {
                    push_f(&mut b, g.keys.get(i).map_or([0.0; 4], |k| k.1.map(|x| x as f32 / 255.0)));
                }
            }
            (Type::Color, Value::Color(c)) => push_f(&mut b, c.map(|x| x as f32 / 255.0)),
            (Type::Vec2, Value::Vec2([x, y])) => push_f(&mut b, [*x as f32, *y as f32, 0.0, 0.0]),
            (Type::Str | Type::Path, _) => {}
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
