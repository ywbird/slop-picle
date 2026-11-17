//! M3 value nodes, evaluated on the CPU (spec/nodes.md#m3-nodes).

use super::R;
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::types::{Type as T, Value};

pub const NUMBER_IN: &[I] = &[I::new("value", T::Number, D::Number(0.0), "The number.")];

pub fn number(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![Value::Number(v[0].num())])
}

pub const COLOR_IN: &[I] = &[I::new("color", T::Color, D::Color("#ffffff"), "The colour.")];

pub fn color(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![v[0].clone()])
}

pub const PALETTE_IN: &[I] = &[I::new("palette", T::Palette, D::Json(r##"["#000000","#ffffff"]"##), "The colours.")];

pub fn palette(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![v[0].clone()])
}

pub const MATH_OPS: &[&str] = &[
    "add", "subtract", "multiply", "divide", "power", "root", "sin", "cos", "tan", "modulo", "floor", "ceil", "round", "abs", "fract", "snap",
    "log", "max", "min",
];

pub const MATH_IN: &[I] = &[
    I::new("type", T::Enum, D::Enum("add"), "Operation.").options(MATH_OPS),
    I::new("a", T::Number, D::Number(0.0), "First operand."),
    I::new("b", T::Number, D::Number(0.0), "Second operand."),
    I::new("angle", T::Enum, D::Enum("degree"), "Unit for trigonometry.").options(&["radians", "degree"]),
    I::new("to_integer", T::Bool, D::Bool(false), "Round the result."),
];

pub fn math(_: &mut Ctx, v: &[Value]) -> R {
    let Value::Enum(op) = v[0] else { unreachable!() };
    let (a, b) = (v[1].num(), v[2].num());
    let rad = |x: f64| if v[3] == Value::Enum(1) { x.to_radians() } else { x };
    let r = match MATH_OPS[op] {
        "add" => a + b,
        "subtract" => a - b,
        "multiply" => a * b,
        "divide" => if b == 0.0 { 0.0 } else { a / b },
        "power" => a.powf(b),
        "root" => a.powf(1.0 / b),
        "sin" => rad(a).sin(),
        "cos" => rad(a).cos(),
        "tan" => rad(a).tan(),
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
    Ok(vec![Value::Number(if v[4] == Value::Bool(true) { r.round() } else { r })])
}

pub const COUNTER_IN: &[I] = &[
    I::new("mode", T::Enum, D::Enum("frame_count"), "Count frames, or progress 0..speed.").options(&["frame_count", "animation_progress"]),
    I::new("start", T::Number, D::Number(1.0), "Value at frame 0 (frame_count)."),
    I::new("speed", T::Number, D::Number(1.0), "Increment per frame."),
];

pub fn counter(ctx: &mut Ctx, v: &[Value]) -> R {
    let f = ctx.frame as f64;
    let r = match v[0] {
        Value::Enum(0) => v[1].num() + f * v[2].num(),
        _ if ctx.frames <= 1 => 0.0,
        _ => f / (ctx.frames - 1) as f64 * v[2].num(),
    };
    Ok(vec![Value::Number(r)])
}
