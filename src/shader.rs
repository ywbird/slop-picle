//! Shader-node helper: WGSL prelude and uniforms generated from the catalog entry (spec/evaluation.md#gpu).

use crate::catalog::NodeDef;
use crate::eval::{Ctx, NodeError};
use crate::gpu::ENTRY;
use crate::surface::Surface;
use crate::types::{Type, Value};

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
";

fn field_type(t: Type) -> Option<&'static str> {
    match t {
        Type::Number | Type::Color | Type::Vec2 => Some("vec4<f32>"),
        Type::Int => Some("vec4<i32>"),
        Type::Bool | Type::Enum | Type::Dimension => Some("vec4<u32>"),
        Type::Str | Type::Path | Type::Surface => None,
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
        } else if let Some(t) = field_type(i.ty) {
            fields.push(format!("    {}: {t},", i.name));
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
