use crate::catalog::{Default, InputDef};
use crate::surface::{self, Surface};
use serde_json::{Value as Json, json};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Number,
    Int,
    Bool,
    Color,
    Vec2,
    Dimension,
    Padding,
    Palette,
    Gradient,
    Enum,
    Str,
    Path,
    Surface,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Number => "number",
            Type::Int => "int",
            Type::Bool => "bool",
            Type::Color => "color",
            Type::Vec2 => "vec2",
            Type::Dimension => "dimension",
            Type::Padding => "padding",
            Type::Palette => "palette",
            Type::Gradient => "gradient",
            Type::Enum => "enum",
            Type::Str => "string",
            Type::Path => "path",
            Type::Surface => "surface",
        }
    }
}

/// Whether an output of type `out` may feed an input of type `input`.
pub fn fits(out: Type, input: Type) -> bool {
    out == input || (out == Type::Int && input == Type::Number)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Int(i64),
    Bool(bool),
    Color([u8; 4]),
    Vec2([f64; 2]),
    Dimension([u32; 2]),
    /// right, top, left, bottom
    Padding([u32; 4]),
    Palette(Vec<[u8; 4]>),
    Gradient(Gradient),
    /// Index into the input's options.
    Enum(usize),
    Str(String),
    Path(String),
    Surface(Arc<Surface>),
    /// An absent optional input without a default (an unconnected optional surface).
    None,
}

impl Value {
    pub fn into_surface(self) -> Option<Surface> {
        match self {
            Value::Surface(s) => Some(Arc::unwrap_or_clone(s)),
            _ => None,
        }
    }

    pub fn surface(&self) -> Option<&Arc<Surface>> {
        match self {
            Value::Surface(s) => Some(s),
            _ => None,
        }
    }

    pub fn ty(&self) -> Option<Type> {
        Some(match self {
            Value::Number(_) => Type::Number,
            Value::Int(_) => Type::Int,
            Value::Bool(_) => Type::Bool,
            Value::Color(_) => Type::Color,
            Value::Vec2(_) => Type::Vec2,
            Value::Dimension(_) => Type::Dimension,
            Value::Padding(_) => Type::Padding,
            Value::Palette(_) => Type::Palette,
            Value::Gradient(_) => Type::Gradient,
            Value::Enum(_) => Type::Enum,
            Value::Str(_) => Type::Str,
            Value::Path(_) => Type::Path,
            Value::Surface(_) => Type::Surface,
            Value::None => return None,
        })
    }

    pub fn num(&self) -> f64 {
        match self {
            Value::Number(v) => *v,
            Value::Int(v) => *v as f64,
            _ => 0.0,
        }
    }

    pub fn vec2(&self) -> [f64; 2] {
        match self {
            Value::Vec2(v) => *v,
            _ => [0.0; 2],
        }
    }

    /// The value as JSON, for display (surfaces become a size note).
    pub fn to_json(&self, def: Option<&InputDef>) -> Json {
        match self {
            Value::Number(v) => json!(v),
            Value::Int(v) => json!(v),
            Value::Bool(v) => json!(v),
            Value::Color(c) => json!(format_color(*c)),
            Value::Vec2(v) => json!(v),
            Value::Dimension(v) => json!(v),
            Value::Padding(v) => json!(v),
            Value::Palette(p) => json!(p.iter().map(|c| format_color(*c)).collect::<Vec<_>>()),
            Value::Gradient(g) => g.to_json(),
            Value::Enum(i) => def.and_then(|d| d.options.get(*i)).map_or(json!(i), |s| json!(s)),
            Value::Str(s) | Value::Path(s) => json!(s),
            Value::Surface(s) => json!(format!("<surface {}x{}>", s.width, s.height)),
            Value::None => Json::Null,
        }
    }
}

pub const MAX_PALETTE: usize = 256;
pub const MAX_KEYS: usize = 16;

/// Upstream gradient: keys sorted by time; blend 0 = rgb, 1 = constant (spec/types.md#gradient-evaluation).
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub constant: bool,
    pub keys: Vec<(f64, [u8; 4])>,
}

impl Gradient {
    pub fn to_json(&self) -> Json {
        json!({
            "type": if self.constant { "constant" } else { "rgb" },
            "keys": self.keys.iter().map(|(t, c)| json!({ "time": t, "color": format_color(*c) })).collect::<Vec<_>>(),
        })
    }

    pub fn eval(&self, t: f64) -> [f64; 4] {
        let f = |c: [u8; 4]| c.map(|x| x as f64 / 255.0);
        let first = self.keys[0];
        if t <= first.0 {
            return f(first.1);
        }
        for w in self.keys.windows(2) {
            let ((t0, c0), (t1, c1)) = (w[0], w[1]);
            if t1 == t {
                return f(c1);
            }
            if t1 > t {
                if self.constant {
                    return f(c0);
                }
                let k = (t - t0) / (t1 - t0);
                let (a, b) = (f(c0), f(c1));
                return [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * k);
            }
        }
        f(self.keys[self.keys.len() - 1].1)
    }
}

pub fn format_color(c: [u8; 4]) -> String {
    if c[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])
    }
}

/// Parse `#rrggbb` or `#rrggbbaa`.
pub fn parse_color(s: &str) -> Option<[u8; 4]> {
    let hex = s.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let b = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    Some([b(0), b(2), b(4), if hex.len() == 8 { b(6) } else { 255 }])
}

pub type LiteralError = (&'static str, String);

fn mismatch(def: &InputDef, j: &Json) -> LiteralError {
    ("type_mismatch", format!("input '{}' expects {}, got {j}", def.name, def.ty.name()))
}

fn int_of(j: &Json) -> Option<i64> {
    let f = j.as_f64()?;
    (f.fract() == 0.0 && f >= i32::MIN as f64 && f <= i32::MAX as f64).then_some(f as i64)
}

fn in_range(def: &InputDef, v: f64) -> Result<(), LiteralError> {
    if def.min.is_some_and(|m| v < m) || def.max.is_some_and(|m| v > m) {
        return Err((
            "out_of_range",
            format!("input '{}' is {v}, outside {}..{}", def.name, def.min.unwrap_or(f64::NEG_INFINITY), def.max.unwrap_or(f64::INFINITY)),
        ));
    }
    Ok(())
}

/// Check a JSON literal against an input definition and convert it.
pub fn check_literal(def: &InputDef, j: &Json) -> Result<Value, LiteralError> {
    let bad = || mismatch(def, j);
    match def.ty {
        Type::Number => {
            let v = j.as_f64().filter(|v| v.is_finite()).ok_or_else(bad)?;
            in_range(def, v)?;
            Ok(Value::Number(v))
        }
        Type::Int => {
            let v = int_of(j).ok_or_else(bad)?;
            in_range(def, v as f64)?;
            Ok(Value::Int(v))
        }
        Type::Bool => j.as_bool().map(Value::Bool).ok_or_else(bad),
        Type::Color => j.as_str().and_then(parse_color).map(Value::Color).ok_or_else(bad),
        Type::Vec2 => {
            let a = j.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
            let x = a[0].as_f64().filter(|v| v.is_finite()).ok_or_else(bad)?;
            let y = a[1].as_f64().filter(|v| v.is_finite()).ok_or_else(bad)?;
            in_range(def, x)?;
            in_range(def, y)?;
            Ok(Value::Vec2([x, y]))
        }
        Type::Dimension => {
            let a = j.as_array().filter(|a| a.len() == 2).ok_or_else(bad)?;
            let w = int_of(&a[0]).ok_or_else(bad)?;
            let h = int_of(&a[1]).ok_or_else(bad)?;
            let side = 1..=surface::MAX_SIDE as i64;
            if !side.contains(&w) || !side.contains(&h) {
                return Err(("out_of_range", format!("input '{}' is {w}x{h}; each side must be 1..4096", def.name)));
            }
            if (w * h) as u64 > surface::MAX_AREA {
                return Err(("too_large", format!("input '{}' is {w}x{h}, over 2048² pixels", def.name)));
            }
            Ok(Value::Dimension([w as u32, h as u32]))
        }
        Type::Padding => {
            let a = j.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
            let mut out = [0u32; 4];
            for (o, v) in out.iter_mut().zip(a) {
                let n = int_of(v).ok_or_else(bad)?;
                if n < 0 {
                    return Err(("out_of_range", format!("input '{}': padding values must be ≥ 0", def.name)));
                }
                *o = n as u32;
            }
            Ok(Value::Padding(out))
        }
        Type::Palette => {
            let a = j.as_array().ok_or_else(bad)?;
            if a.is_empty() || a.len() > MAX_PALETTE {
                return Err(("out_of_range", format!("input '{}' needs 1..{MAX_PALETTE} colours, got {}", def.name, a.len())));
            }
            a.iter().map(|c| c.as_str().and_then(parse_color).ok_or_else(bad)).collect::<Result<_, _>>().map(Value::Palette)
        }
        Type::Gradient => {
            let o = j.as_object().filter(|o| !o.contains_key("ref")).ok_or_else(bad)?;
            if let Some(k) = o.keys().find(|k| *k != "type" && *k != "keys") {
                return Err(("type_mismatch", format!("input '{}': unknown gradient key '{k}'", def.name)));
            }
            let constant = match o.get("type").map(|t| t.as_str()) {
                None | Some(Some("rgb")) => false,
                Some(Some("constant")) => true,
                Some(Some(t)) => return Err(("invalid_enum", format!("input '{}': gradient type '{t}', expected rgb or constant", def.name))),
                Some(None) => return Err(bad()),
            };
            let keys = o.get("keys").and_then(Json::as_array).ok_or_else(bad)?;
            if keys.is_empty() || keys.len() > MAX_KEYS {
                return Err(("out_of_range", format!("input '{}' needs 1..{MAX_KEYS} gradient keys", def.name)));
            }
            let mut out = vec![];
            for k in keys {
                let k = k.as_object().ok_or_else(bad)?;
                let t = k.get("time").and_then(Json::as_f64).ok_or_else(bad)?;
                let c = k.get("color").and_then(Json::as_str).and_then(parse_color).ok_or_else(bad)?;
                if !(0.0..=1.0).contains(&t) {
                    return Err(("out_of_range", format!("input '{}': gradient key time {t} outside 0..1", def.name)));
                }
                out.push((t, c));
            }
            out.sort_by(|a, b| a.0.total_cmp(&b.0));
            Ok(Value::Gradient(Gradient { constant, keys: out }))
        }
        Type::Enum => {
            let s = j.as_str().ok_or_else(bad)?;
            let i = def.options.iter().position(|o| *o == s).ok_or_else(|| {
                ("invalid_enum", format!("input '{}' is '{s}', expected one of {}", def.name, def.options.join(", ")))
            })?;
            Ok(Value::Enum(i))
        }
        Type::Str => j.as_str().map(|s| Value::Str(s.into())).ok_or_else(bad),
        Type::Path => j.as_str().map(|s| Value::Path(s.into())).ok_or_else(bad),
        Type::Surface => Err(("type_mismatch", format!("input '{}' is a surface and only accepts a ref", def.name))),
    }
}

/// The default value of an input, with `project` the project's size for dimension inputs.
pub fn default_value(def: &InputDef, project: [u32; 2]) -> Option<Value> {
    Some(match def.default {
        Default::None => return None,
        Default::Number(v) => Value::Number(v),
        Default::Int(v) => Value::Int(v),
        Default::Bool(v) => Value::Bool(v),
        Default::Color(s) => Value::Color(parse_color(s).expect("valid default color")),
        Default::Vec2(v) => Value::Vec2(v),
        Default::Enum(s) => Value::Enum(def.options.iter().position(|o| *o == s).expect("valid default option")),
        Default::Str(s) => Value::Str(s.into()),
        Default::ProjectSize => Value::Dimension(project),
        Default::Json(text) => check_literal(def, &serde_json::from_str(text).expect("valid default JSON")).expect("valid default"),
    })
}
