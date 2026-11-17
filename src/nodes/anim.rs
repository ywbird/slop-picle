//! Animation category: time functions and time remapping (spec/animation.md).

use super::R;
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::shader::{hash2, unit};
use crate::types::{Type as T, Value};

fn int(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        v => v.num() as i64,
    }
}

const SMOOTH: &[&str] = &["cubic_poly", "quadratic_rat", "cubic_rat", "cosine"];

fn smooth(kind: &Value, x: f64) -> f64 {
    match kind {
        Value::Enum(1) => x * x / (2.0 * x * x - 2.0 * x + 1.0),
        Value::Enum(2) => x * x * x / (3.0 * x * x - 3.0 * x + 1.0),
        Value::Enum(3) => 0.5 - 0.5 * (std::f64::consts::PI * x).cos(),
        _ => x * x * (3.0 - 2.0 * x),
    }
}

// ---------- time functions ----------

pub const WIGGLER_IN: &[I] = &[
    I::new("seed", T::Int, D::Int(0), "Random seed."),
    I::new("range", T::Vec2, D::Vec2([0.0, 1.0]), "Output range."),
    I::new("frequency", T::Number, D::Number(4.0), "Wiggles over the animation.").range(1.0, 16.0),
    I::new("clip", T::Int, D::Int(3), "Bit mask: 1 starts and 2 ends at the middle of the range.").range(0.0, 3.0),
];

pub fn wiggler(ctx: &mut Ctx, v: &[Value]) -> R {
    let (seed, range, freq, clip) = (int(&v[0]), v[1].vec2(), v[2].num(), int(&v[3]));
    let total = ctx.frames as f64;
    let f = ctx.frame as f64;
    let w = if freq == 0.0 { 1.0 } else { (total / freq).max(1.0) };
    let lo = (f / w).floor() * w;
    let hi = total.min(lo + w);
    let r = |x: f64| unit(hash2((x * 1000.0).round() as i64, 0, seed));
    let v0 = if clip & 1 != 0 && lo <= 0.0 { 0.5 } else { r(lo) };
    let v1 = if clip & 2 != 0 && hi >= total { 0.5 } else { r(hi) };
    let t = if hi > lo { (f - lo) / (hi - lo) } else { 0.0 };
    let t = (1.0 - (std::f64::consts::PI * t).cos()) / 2.0;
    let m = v0 + (v1 - v0) * t;
    Ok(vec![Value::Number(range[0] + (range[1] - range[0]) * m)])
}

pub const FN_EASE_IN: &[I] = &[
    I::new("range", T::Vec2, D::Vec2([0.0, 1.0]), "Part of the animation (0..1) where the value is not 0."),
    I::new("amount", T::Vec2, D::Vec2([0.1, 0.9]), "Ease-in end and ease-out start, as fractions."),
    I::new("smooth", T::Enum, D::Enum("cubic_poly"), "Easing curve.").options(SMOOTH),
];

pub fn fn_ease(ctx: &mut Ctx, v: &[Value]) -> R {
    let (r, a) = (v[0].vec2(), v[1].vec2());
    let x = ctx.frame as f64 / ctx.frames as f64;
    if x < r[0] || x > r[1] {
        return Ok(vec![Value::Number(0.0)]);
    }
    let eo = 1.0 - a[1];
    let up = if a[0] == 0.0 { 1.0 } else { (x - r[0]) / a[0] };
    let down = if eo == 0.0 { 1.0 } else { 1.0 - (x - (r[1] - eo)) / eo };
    Ok(vec![Value::Number(smooth(&v[2], up.min(down).clamp(0.0, 1.0)))])
}

pub const FN_SMOOTHSTEP_IN: &[I] = &[
    I::new("value", T::Number, D::Number(0.0), "Input, usually 0..1."),
    I::new("type", T::Enum, D::Enum("cubic_poly"), "Curve.").options(SMOOTH),
];

pub fn fn_smoothstep(_: &mut Ctx, v: &[Value]) -> R {
    Ok(vec![Value::Number(smooth(&v[1], v[0].num()))])
}

pub const FN_MATH_IN: &[I] = &[
    I::new("operation", T::Enum, D::Enum("multiply"), "Operation.").options(&["add", "minus", "multiply"]),
    I::new("value_1", T::Number, D::Number(0.0), "First value."),
    I::new("value_2", T::Number, D::Number(0.0), "Second value."),
];

pub fn fn_math(_: &mut Ctx, v: &[Value]) -> R {
    let (a, b) = (v[1].num(), v[2].num());
    Ok(vec![Value::Number(match v[0] {
        Value::Enum(0) => a + b,
        Value::Enum(1) => a - b,
        _ => a * b,
    })])
}

// ---------- time remapping (evaluated by the evaluator through `remap`) ----------

pub fn no_run(_: &mut Ctx, _: &[Value]) -> R {
    unreachable!("time-remap nodes are evaluated through `remap`")
}

pub const REVERT_IN: &[I] = &[I::required("surface_in", T::Surface, "Animation to play backwards.")];

pub fn revert_frame(_: &[Value], f: i64, total: i64, _: f64) -> Option<i64> {
    Some(total - 1 - f)
}

pub const ANIM_SPEED_IN: &[I] = &[
    I::required("surface_in", T::Surface, "Animation to speed up or slow down."),
    I::new("speed", T::Number, D::Number(1.0), "Playback speed."),
    I::new("overflow", T::Enum, D::Enum("loop"), "Past the last frame.").options(&["loop", "pingpong", "empty"]),
];

pub fn anim_speed_frame(v: &[Value], f: i64, total: i64, _: f64) -> Option<i64> {
    let t = total as f64;
    let mut x = f as f64 * v[1].num();
    match v[2] {
        Value::Enum(0) => x = x.rem_euclid(t),
        Value::Enum(1) => {
            x = x.rem_euclid(2.0 * t + 1.0);
            if x > t {
                x = 2.0 * t - x;
            }
        }
        _ => {
            if x < 0.0 || x >= t {
                return None;
            }
        }
    }
    Some((x.floor() as i64).clamp(0, total - 1))
}

pub const RATE_REMAP_IN: &[I] = &[
    I::required("surface", T::Surface, "Animation to resample."),
    I::new("framerate", T::Number, D::Number(10.0), "Target frames per second.").min(1.0),
    I::new("offset", T::Number, D::Number(0.0), "Frame offset."),
    I::new("loop", T::Bool, D::Bool(true), "Wrap around the animation."),
];

pub fn rate_remap_frame(v: &[Value], f: i64, total: i64, fps: f64) -> Option<i64> {
    let mut x = f as f64 - v[2].num();
    if v[3] == Value::Bool(true) {
        x = x.rem_euclid(total as f64);
    }
    let step = fps / v[1].num();
    Some(((x / step).floor() * step).floor() as i64)
}

pub const DELAY_IN: &[I] = &[
    I::required("surface", T::Surface, "Animation to delay."),
    I::new("frames", T::Int, D::Int(1), "Delay in frames."),
    I::new("overflow", T::Enum, D::Enum("hold"), "Before the first frame.").options(&["hold", "loop", "clear"]),
];

pub fn delay_frame(v: &[Value], f: i64, total: i64, _: f64) -> Option<i64> {
    let x = f - int(&v[1]);
    match v[2] {
        Value::Enum(0) => Some(x.clamp(0, total - 1)),
        Value::Enum(1) => Some(x.rem_euclid(total)),
        _ => (0..total).contains(&x).then_some(x),
    }
}

pub const ANIM_LOOP_IN: &[I] = &[
    I::required("surface", T::Surface, "Animation to loop."),
    I::new("loop_type", T::Enum, D::Enum("loop"), "Repeat or bounce.").options(&["loop", "pingpong"]),
    I::new("loop_start", T::Int, D::Int(1), "First looped frame, counting from 1."),
    I::new("loop_range", T::Int, D::Int(4), "Frames in one loop; 0 means to the end."),
    I::new("infinite", T::Bool, D::Bool(true), "Loop forever."),
    I::new("loop_amount", T::Int, D::Int(1), "Number of loops when not infinite."),
    I::new("pre_loop", T::Enum, D::Enum("passthrough"), "Before the loop.").options(&["passthrough", "empty"]),
    I::new("post_loop", T::Enum, D::Enum("empty"), "After the last loop.").options(&["passthrough", "empty"]),
];

pub fn anim_loop_frame(v: &[Value], f: i64, total: i64, _: f64) -> Option<i64> {
    let start = int(&v[2]) - 1;
    let range = match int(&v[3]) {
        0 => total - int(&v[2]),
        r => r,
    }
    .max(1);
    let pass = |e: &Value| if *e == Value::Enum(0) { Some(f) } else { None };
    if f < start {
        return pass(&v[6]);
    }
    let n = (f - start).div_euclid(range);
    if v[4] != Value::Bool(true) && n > int(&v[5]) {
        return pass(&v[7]);
    }
    let p = (f - start).rem_euclid(range);
    Some(if v[1] == Value::Enum(1) && n % 2 == 1 { start + range - 1 - p } else { start + p })
}
