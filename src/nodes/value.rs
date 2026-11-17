//! M3 value nodes, evaluated on the CPU (spec/nodes.md#m3-nodes).

use super::R;
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::project::GInput;
use crate::types::{Type as T, Value};

pub const NUMBER_IN: &[I] = &[I::new("value", T::Number, D::Number(0.0), "The number."), I::new("integer", T::Bool, D::Bool(false), "Round the number.")];

pub fn number(_: &mut Ctx, v: &[Value]) -> R {
    let x = v[0].num();
    Ok(vec![Value::Number(if v[1] == Value::Bool(true) { x.round() } else { x })])
}

pub const COLOR_IN: &[I] = &[I::new("color", T::Color, D::Color("#ffffff"), "The colour.")];

pub fn color(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![v[0].clone()])
}

pub const PALETTE_IN: &[I] = &[
    I::new("palette", T::Palette, D::Json(r##"["#000000","#ffffff"]"##), "The colours."),
    I::new("trim_range", T::Vec2, D::Vec2([0.0, 1.0]), "Keep only this part of the palette, as fractions.").range(0.0, 1.0),
];

pub fn palette(_: &mut Ctx, v: &[Value]) -> R {
    let Value::Palette(p) = &v[0] else { unreachable!() };
    let r = v[1].vec2().map(|x| x.clamp(0.0, 1.0));
    let n = p.len() as f64;
    let (st, en) = ((r[0].min(r[1]) * n).floor() as usize, (r[0].max(r[1]) * n).floor() as usize);
    if st >= en {
        return Err("trim_range keeps no colour".into());
    }
    Ok(vec![Value::Palette(p[st..en].to_vec())])
}

pub const MATH_OPS: &[&str] = &[
    "add", "subtract", "multiply", "divide", "power", "root", "sin", "cos", "tan", "modulo", "floor", "ceil", "round", "lerp", "abs", "clamp",
    "snap", "fract", "map", "log", "max", "min",
];

pub const MATH_IN: &[I] = &[
    I::new("type", T::Enum, D::Enum("add"), "Operation.").options(MATH_OPS),
    I::new("a", T::Number, D::Number(0.0), "First operand."),
    I::new("b", T::Number, D::Number(0.0), "Second operand."),
    I::new("amount", T::Number, D::Number(0.0), "lerp: the fraction; clamp: the maximum."),
    I::new("from", T::Vec2, D::Vec2([0.0, 1.0]), "map: the input range."),
    I::new("to", T::Vec2, D::Vec2([0.0, 1.0]), "map: the output range."),
    I::new("angle", T::Enum, D::Enum("degree"), "Unit for trigonometry.").options(&["radians", "degree"]),
    I::new("to_integer", T::Bool, D::Bool(false), "Round the result."),
    I::new("clamp_output", T::Bool, D::Bool(false), "map: keep the result inside the output range."),
];

pub fn math(_: &mut Ctx, v: &[Value]) -> R {
    let Value::Enum(op) = v[0] else { unreachable!() };
    let (a, b, c) = (v[1].num(), v[2].num(), v[3].num());
    let rad = |x: f64| if v[6] == Value::Enum(1) { x.to_radians() } else { x };
    let r = match MATH_OPS[op] {
        "add" => a + b,
        "subtract" => a - b,
        "multiply" => a * b,
        "divide" => if b == 0.0 { 0.0 } else { a / b },
        "power" => a.powf(b),
        "root" => if b <= 0.0 { 0.0 } else { a.powf(1.0 / b) },
        "sin" => rad(a).sin() * b,
        "cos" => rad(a).cos() * b,
        "tan" => rad(a).tan() * b,
        "lerp" => a + (b - a) * c,
        "clamp" => a.max(b).min(c),
        "map" => {
            let (f, t) = (v[4].vec2(), v[5].vec2());
            let m = t[0] + (t[1] - t[0]) * (a - f[0]) / (f[1] - f[0]);
            if v[8] == Value::Bool(true) { m.max(t[0]).min(t[1]) } else { m }
        }
        "modulo" => if b == 0.0 { a } else { a - b * (a / b).trunc() },
        "floor" => a.floor(),
        "ceil" => a.ceil(),
        "round" => a.round(),
        "abs" => a.abs(),
        "fract" => a - a.floor(),
        "snap" => if b == 0.0 { a } else { (a / b).round() * b },
        "log" => a.ln() / b.ln(),
        "max" => a.max(b),
        _ => a.min(b),
    };
    if !r.is_finite() {
        return Err(format!("{} of {a} and {b} is not a finite number", MATH_OPS[op]).into());
    }
    Ok(vec![Value::Number(if v[7] == Value::Bool(true) { r.round() } else { r })])
}

pub const COUNTER_IN: &[I] = &[
    I::new("mode", T::Enum, D::Enum("frame_count"), "Count frames, or progress 0..speed.").options(&["frame_count", "animation_progress"]),
    I::new("start", T::Number, D::Number(1.0), "Value at frame 0 (frame_count)."),
    I::new("speed", T::Number, D::Number(1.0), "Increment per frame."),
    I::new("async", T::Bool, D::Bool(false), "Count from the last frame where reset is on."),
    I::new("reset", T::Bool, D::Bool(false), "Restart the count here (with async)."),
];

pub fn counter(ctx: &mut Ctx, v: &[Value]) -> R {
    let reset_at = |g: i64| match &ctx.inputs[4] {
        GInput::Anim(a) => a.value_at(g, ctx.frames, T::Bool) == Value::Bool(true),
        _ => g == ctx.frame && v[4] == Value::Bool(true),
    };
    let r = if v[3] == Value::Bool(true) { (0..=ctx.frame).rev().find(|&g| reset_at(g)).unwrap_or(0) } else { 0 };
    let f = (ctx.frame - r) as f64;
    let r = match v[0] {
        Value::Enum(0) => v[1].num() + f * v[2].num(),
        _ if ctx.frames <= 1 => 0.0,
        _ => f / (ctx.frames - 1) as f64 * v[2].num(),
    };
    Ok(vec![Value::Number(r)])
}
