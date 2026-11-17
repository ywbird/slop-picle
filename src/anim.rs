//! Keyframed inputs (spec/animation.md). Upstream: scripts/node_keyframe, scripts/curve_bezier_function.

use crate::catalog::InputDef;
use crate::types::{self, LiteralError, Type, Value};
use serde_json::Value as Json;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ease {
    Linear,
    Bezier,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnEnd {
    Hold,
    Loop,
    Ping,
    Wrap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Key {
    pub time: i64,
    pub value: Value,
    pub ease_in: [f64; 2],
    pub ease_out: [f64; 2],
    pub ease_in_type: Ease,
    pub ease_out_type: Ease,
}

/// An animated input, keys sorted by time.
#[derive(Clone, Debug, PartialEq)]
pub struct Anim {
    pub keys: Vec<Key>,
    pub on_end: OnEnd,
}

pub fn is_anim(j: &Json) -> bool {
    j.as_object().is_some_and(|o| o.contains_key("anim"))
}

/// Types that may be animated.
pub fn animatable(t: Type) -> bool {
    t != Type::Surface
}

fn lerpable(t: Type) -> bool {
    matches!(t, Type::Number | Type::Int | Type::Color | Type::Vec2 | Type::Dimension | Type::Padding)
}

fn ease_of(j: Option<&Json>, def: &InputDef) -> Result<Ease, LiteralError> {
    match j.map(|v| v.as_str()) {
        None => Ok(Ease::Linear),
        Some(Some("linear")) => Ok(Ease::Linear),
        Some(Some("bezier")) => Ok(Ease::Bezier),
        Some(Some("cut")) => Ok(Ease::Cut),
        Some(Some(s)) => Err(("invalid_enum", format!("input '{}': ease type '{s}', expected linear, bezier or cut", def.name))),
        Some(None) => Err(("type_mismatch", format!("input '{}': ease type must be a string", def.name))),
    }
}

fn handle(j: Option<&Json>, default: [f64; 2], def: &InputDef) -> Result<[f64; 2], LiteralError> {
    let Some(j) = j else { return Ok(default) };
    match j.as_array().map(|a| a.as_slice()) {
        Some([x, y]) if x.is_number() && y.is_number() => Ok([x.as_f64().unwrap(), y.as_f64().unwrap()]),
        _ => Err(("type_mismatch", format!("input '{}': ease handles are [number, number]", def.name))),
    }
}

/// Check an animation object against the input definition.
pub fn parse(def: &InputDef, j: &Json) -> Result<Anim, LiteralError> {
    let bad = |m: String| ("type_mismatch", m);
    if !animatable(def.ty) {
        return Err(bad(format!("input '{}' is a surface and cannot be animated", def.name)));
    }
    let o = j.as_object().ok_or_else(|| bad(format!("input '{}': animation must be an object", def.name)))?;
    if let Some(k) = o.keys().find(|k| *k != "anim" && *k != "on_end") {
        return Err(("unknown_key", format!("input '{}': unknown animation key '{k}'", def.name)));
    }
    let on_end = match o.get("on_end").map(|v| v.as_str()) {
        None | Some(Some("hold")) => OnEnd::Hold,
        Some(Some("loop")) => OnEnd::Loop,
        Some(Some("ping")) => OnEnd::Ping,
        Some(Some("wrap")) => OnEnd::Wrap,
        Some(Some(s)) => return Err(("invalid_enum", format!("input '{}': on_end '{s}', expected hold, loop, ping or wrap", def.name))),
        Some(None) => return Err(bad(format!("input '{}': on_end must be a string", def.name))),
    };
    let arr = o["anim"].as_array().ok_or_else(|| bad(format!("input '{}': anim must be an array of keys", def.name)))?;
    if arr.is_empty() {
        return Err(("out_of_range", format!("input '{}': an animation needs at least one key", def.name)));
    }
    let mut keys = Vec::with_capacity(arr.len());
    for k in arr {
        let k = k.as_object().ok_or_else(|| bad(format!("input '{}': a key must be an object", def.name)))?;
        if let Some(x) = k.keys().find(|x| !["time", "value", "ease_in", "ease_out", "ease_in_type", "ease_out_type"].contains(&x.as_str())) {
            return Err(("unknown_key", format!("input '{}': unknown key field '{x}'", def.name)));
        }
        let time = k.get("time").and_then(|t| t.as_f64()).filter(|t| t.fract() == 0.0).ok_or_else(|| bad(format!("input '{}': key time must be an integer frame", def.name)))?
            as i64;
        if time < 0 {
            return Err(("out_of_range", format!("input '{}': key time {time} is negative", def.name)));
        }
        if keys.iter().any(|x: &Key| x.time == time) {
            return Err(("out_of_range", format!("input '{}': two keys at frame {time}", def.name)));
        }
        let value = types::check_literal(def, k.get("value").ok_or_else(|| bad(format!("input '{}': key at {time} has no value", def.name)))?)?;
        keys.push(Key {
            time,
            value,
            ease_in: handle(k.get("ease_in"), [0.0, 1.0], def)?,
            ease_out: handle(k.get("ease_out"), [0.0, 0.0], def)?,
            ease_in_type: ease_of(k.get("ease_in_type"), def)?,
            ease_out_type: ease_of(k.get("ease_out_type"), def)?,
        });
    }
    keys.sort_by_key(|k| k.time);
    Ok(Anim { keys, on_end })
}

/// Cubic bezier with end points 0 and 1 (upstream `eval_curve_segment_t`).
fn bezier_t(y0: f64, ay: f64, by: f64, y1: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * y0 + 3.0 * u * u * t * ay + 3.0 * u * t * t * by + t * t * t * y1
}

/// The bezier branch of upstream `KeyframeInterpolate`: `eo` is the outgoing handle, `ei` the incoming one.
pub fn bezier_ease(eo: [f64; 2], ei: [f64; 2], rat: f64) -> f64 {
    if rat == 0.0 || rat == 1.0 {
        return rat;
    }
    let (mut eox, mut eoy, mut eix, mut eiy) = (eo[0], eo[1], ei[0], ei[1]);
    if eox > 1.0 && eix > 1.0 {
        let total = eox.min(eix);
        eox /= total;
        eix /= total;
    }
    eiy -= (eox - 1.0).clamp(0.0, 1.0);
    eox = eox.clamp(0.0, 0.9);
    eoy += (eix - 1.0).clamp(0.0, 1.0);
    eix = eix.clamp(0.0, 0.9);
    // upstream eval_curve_segment_x(0, eox, eoy, 1 - eix, eiy, 1, rat)
    let (ax, bx) = (eox, 1.0 - eix);
    let mut t = rat;
    for _ in 0..8 {
        let u = 1.0 - t;
        let ft = 3.0 * u * u * t * ax + 3.0 * u * t * t * bx + t * t * t;
        if (ft - rat).abs() < 0.0001 {
            break;
        }
        let d = 3.0 * u * u * ax + 6.0 * u * t * (bx - ax) + 3.0 * t * t * (1.0 - bx);
        t -= (ft - rat) / d;
    }
    bezier_t(0.0, eoy, eiy, 1.0, t)
}

fn ease(from: &Key, to: &Key, rat: f64) -> f64 {
    if to.ease_in_type == Ease::Linear && from.ease_out_type == Ease::Linear {
        return rat;
    }
    bezier_ease(from.ease_out, to.ease_in, rat)
}

fn lerp_value(a: &Value, b: &Value, e: f64) -> Value {
    let l = |x: f64, y: f64| x + (y - x) * e;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => Value::Number(l(*x, *y)),
        (Value::Int(x), Value::Int(y)) => Value::Int(l(*x as f64, *y as f64).round() as i64),
        (Value::Vec2(x), Value::Vec2(y)) => Value::Vec2([l(x[0], y[0]), l(x[1], y[1])]),
        (Value::Dimension(x), Value::Dimension(y)) => Value::Dimension([0, 1].map(|i| l(x[i] as f64, y[i] as f64).round() as u32)),
        (Value::Padding(x), Value::Padding(y)) => Value::Padding([0, 1, 2, 3].map(|i| l(x[i] as f64, y[i] as f64).round() as u32)),
        (Value::Color(x), Value::Color(y)) => Value::Color([0, 1, 2, 3].map(|i| l(x[i] as f64, y[i] as f64).round() as u8)),
        _ => a.clone(),
    }
}

fn interpolate(from: &Key, to: &Key, rat: f64, ty: Type) -> Value {
    if to.ease_in_type == Ease::Cut {
        return from.value.clone();
    }
    if from.ease_out_type == Ease::Cut {
        return to.value.clone();
    }
    if !lerpable(ty) {
        return from.value.clone();
    }
    lerp_value(&from.value, &to.value, ease(from, to, rat))
}

impl Anim {
    /// The value at frame `t` of a project with `frames` frames (upstream `valueAnimator.getValue`).
    pub fn value_at(&self, t: i64, frames: i64, ty: Type) -> Value {
        let k = &self.keys;
        if k.len() == 1 {
            return k[0].value.clone();
        }
        let (first, last) = (k[0].time, k[k.len() - 1].time);
        let dura = last - first;
        let mut t = t;
        if t > last {
            match self.on_end {
                OnEnd::Loop => t = first + (t - last).rem_euclid(dura + 1),
                OnEnd::Ping if dura > 0 => {
                    let p = (t - first).rem_euclid(dura * 2);
                    t = if p < dura { first + p } else { first + dura * 2 - p };
                }
                _ => {}
            }
        }
        let (lk, fk) = (&k[k.len() - 1], &k[0]);
        if t < first {
            if self.on_end == OnEnd::Wrap {
                let rat = (frames - last + t) as f64 / (frames - last + first) as f64;
                return interpolate(lk, fk, rat, ty);
            }
            return fk.value.clone();
        }
        if t >= last {
            if self.on_end == OnEnd::Wrap {
                let rat = (t - last) as f64 / (frames - last + first) as f64;
                return interpolate(lk, fk, rat, ty);
            }
            return lk.value.clone();
        }
        if let Some(key) = k.iter().find(|x| x.time == t) {
            return key.value.clone();
        }
        let i = k.iter().rposition(|x| x.time < t).unwrap();
        let (from, to) = (&k[i], &k[i + 1]);
        let rat = (t - from.time) as f64 / (to.time - from.time) as f64;
        interpolate(from, to, rat, ty)
    }
}
