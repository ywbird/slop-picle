//! The single description of every node type (spec/types.md#catalog).

use crate::eval::{Ctx, NodeError};
use crate::types::{Type, Value};
use serde_json::{Map, Value as Json, json};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Default {
    /// Required input, or an optional surface.
    None,
    Number(f64),
    Int(i64),
    Bool(bool),
    Color(&'static str),
    Vec2([f64; 2]),
    Enum(&'static str),
    Str(&'static str),
    /// `dimension` inputs: the project's size (`null` in JSON).
    ProjectSize,
}

#[derive(Debug)]
pub struct InputDef {
    pub name: &'static str,
    pub ty: Type,
    pub required: bool,
    pub default: Default,
    /// Inclusive range for number and int inputs; per component for vec2.
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub options: &'static [&'static str],
    pub description: &'static str,
}

impl InputDef {
    pub const fn new(name: &'static str, ty: Type, default: Default, description: &'static str) -> Self {
        InputDef { name, ty, required: false, default, min: None, max: None, options: &[], description }
    }
    pub const fn required(name: &'static str, ty: Type, description: &'static str) -> Self {
        InputDef { required: true, ..Self::new(name, ty, Default::None, description) }
    }
    pub const fn range(self, min: f64, max: f64) -> Self {
        InputDef { min: Some(min), max: Some(max), ..self }
    }
    pub const fn min(self, min: f64) -> Self {
        InputDef { min: Some(min), ..self }
    }
    pub const fn options(self, options: &'static [&'static str]) -> Self {
        InputDef { options, ..self }
    }
}

#[derive(Debug)]
pub struct OutputDef {
    pub name: &'static str,
    pub ty: Type,
}

pub type RunFn = fn(&mut Ctx, &[Value]) -> Result<Vec<Value>, NodeError>;

pub struct NodeDef {
    pub ty: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub inputs: &'static [InputDef],
    pub outputs: &'static [OutputDef],
    pub run: RunFn,
    /// The node's output depends on the frame number.
    pub reads_frame: bool,
}

impl NodeDef {
    pub fn input(&self, name: &str) -> Option<(usize, &InputDef)> {
        self.inputs.iter().enumerate().find(|(_, i)| i.name == name)
    }
    pub fn output(&self, name: &str) -> Option<(usize, &OutputDef)> {
        self.outputs.iter().enumerate().find(|(_, o)| o.name == name)
    }
}

pub fn all() -> &'static [NodeDef] {
    crate::nodes::ALL
}

pub fn get(ty: &str) -> Option<&'static NodeDef> {
    all().iter().find(|d| d.ty == ty)
}

pub fn default_json(d: Default) -> Option<Json> {
    Some(match d {
        Default::None => return None,
        Default::Number(v) => json!(v),
        Default::Int(v) => json!(v),
        Default::Bool(v) => json!(v),
        Default::Color(s) | Default::Enum(s) | Default::Str(s) => json!(s),
        Default::Vec2(v) => json!(v),
        Default::ProjectSize => Json::Null,
    })
}

pub fn input_json(i: &InputDef) -> Json {
    let mut m = Map::new();
    m.insert("name".into(), json!(i.name));
    m.insert("type".into(), json!(i.ty.name()));
    if i.required {
        m.insert("required".into(), json!(true));
    }
    if let Some(d) = default_json(i.default) {
        m.insert("default".into(), d);
    }
    if let Some(v) = i.min {
        m.insert("min".into(), json!(v));
    }
    if let Some(v) = i.max {
        m.insert("max".into(), json!(v));
    }
    if !i.options.is_empty() {
        m.insert("options".into(), json!(i.options));
    }
    m.insert("description".into(), json!(i.description));
    Json::Object(m)
}

pub fn def_json(d: &NodeDef) -> Json {
    json!({
        "type": d.ty,
        "category": d.category,
        "description": d.description,
        "inputs": d.inputs.iter().map(input_json).collect::<Vec<_>>(),
        "outputs": d.outputs.iter().map(|o| json!({ "name": o.name, "type": o.ty.name() })).collect::<Vec<_>>(),
    })
}

/// Catalog types closest to `name`, for "did you mean" hints.
pub fn suggest(name: &str) -> Vec<&'static str> {
    let mut scored: Vec<(usize, &'static str)> = all().iter().map(|d| (edit_distance(name, d.ty), d.ty)).collect();
    scored.sort();
    scored.into_iter().filter(|(d, _)| *d <= 3).take(3).map(|(_, t)| t).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + (ca != *cb) as usize).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
