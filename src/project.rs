//! The project file: parse, validate, save (spec/project-file.md).

use crate::catalog::{self, NodeDef};
use crate::surface;
use crate::types::{self, Type, Value, fits};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::{Map, Number, Value as Json, json};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const MAX_NODES: usize = 1000;
pub const MAX_LABEL: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Error {
    pub code: String,
    /// Index of the op that failed, for edit batches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    pub message: String,
}

impl Error {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Error { code: code.into(), op: None, node: None, input: None, message: message.into() }
    }
    pub fn at(code: &str, node: &str, input: Option<&str>, message: impl Into<String>) -> Self {
        Error { code: code.into(), op: None, node: Some(node.into()), input: input.map(Into::into), message: message.into() }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (&self.node, &self.input) {
            (Some(n), Some(i)) => write!(f, "{n}.{i}: {}: {}", self.code, self.message),
            (Some(n), None) => write!(f, "{n}: {}: {}", self.code, self.message),
            _ => write!(f, "{}: {}", self.code, self.message),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub width: i64,
    pub height: i64,
    pub frames: i64,
    pub fps: Number,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { width: 32, height: 32, frames: 1, fps: Number::from(12) }
    }
}

impl Settings {
    pub fn fps(&self) -> f64 {
        self.fps.as_f64().unwrap_or(12.0)
    }
    pub fn size(&self) -> [u32; 2] {
        [self.width.clamp(1, u32::MAX as i64) as u32, self.height.clamp(1, u32::MAX as i64) as u32]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub ty: String,
    pub label: Option<String>,
    /// Input values as written: a literal or `{"ref": ...}`.
    pub inputs: IndexMap<String, Json>,
    pub pos: Option<[Number; 2]>,
}

impl Node {
    pub fn new(ty: &str) -> Self {
        Node { ty: ty.into(), label: None, inputs: IndexMap::new(), pos: None }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Project {
    pub settings: Settings,
    pub nodes: IndexMap<String, Node>,
}

pub struct Loaded {
    pub project: Option<Project>,
    pub errors: Vec<Error>,
}

impl Loaded {
    /// Errors that block saving and evaluation (all but `missing_input`).
    pub fn blocking(&self) -> Vec<&Error> {
        self.errors.iter().filter(|e| e.code != "missing_input").collect()
    }
}

pub fn load_str(text: &str) -> Loaded {
    match parse(text) {
        Err(errors) => Loaded { project: None, errors },
        Ok((p, mut errors)) => {
            errors.extend(validate(&p));
            Loaded { project: Some(p), errors }
        }
    }
}

pub fn load(path: &Path) -> std::io::Result<Loaded> {
    Ok(load_str(&std::fs::read_to_string(path)?))
}

fn parse_err(m: impl Into<String>) -> Vec<Error> {
    vec![Error::new("parse_error", m)]
}

fn unknown_key(key: &str, node: Option<&str>, input: Option<&str>) -> Error {
    let m = format!("unknown key '{key}'");
    match node {
        Some(n) => Error::at("unknown_key", n, input, m),
        None => Error::new("unknown_key", m),
    }
}

/// Structural parse. Fatal problems return `Err`; recoverable ones come back with the project.
pub fn parse(text: &str) -> Result<(Project, Vec<Error>), Vec<Error>> {
    let root: Json = serde_json::from_str(text).map_err(|e| parse_err(format!("invalid JSON: {e}")))?;
    let obj = root.as_object().ok_or_else(|| parse_err("the project must be a JSON object"))?;
    if obj.get("slopc").and_then(Json::as_i64) != Some(1) || obj.get("slopc").is_some_and(|v| v.is_f64()) {
        return Err(vec![Error::new("unsupported_version", "\"slopc\" must be 1")]);
    }
    let mut errors = vec![];
    let mut p = Project::default();
    for (k, v) in obj {
        match k.as_str() {
            "slopc" => {}
            "settings" => {
                let s = v.as_object().ok_or_else(|| parse_err("\"settings\" must be an object"))?;
                for (sk, sv) in s {
                    let int = || sv.as_i64().filter(|_| !sv.is_f64()).ok_or_else(|| parse_err(format!("settings.{sk} must be an integer")));
                    match sk.as_str() {
                        "width" => p.settings.width = int()?,
                        "height" => p.settings.height = int()?,
                        "frames" => p.settings.frames = int()?,
                        "fps" => match sv {
                            Json::Number(n) => p.settings.fps = n.clone(),
                            _ => return Err(parse_err("settings.fps must be a number")),
                        },
                        _ => errors.push(unknown_key(&format!("settings.{sk}"), None, None)),
                    }
                }
            }
            "nodes" => {
                let nodes = v.as_object().ok_or_else(|| parse_err("\"nodes\" must be an object"))?;
                for (id, nv) in nodes {
                    let n = nv.as_object().ok_or_else(|| parse_err(format!("node '{id}' must be an object")))?;
                    let ty = n.get("type").and_then(Json::as_str).ok_or_else(|| parse_err(format!("node '{id}' needs a string \"type\"")))?;
                    let mut node = Node::new(ty);
                    for (nk, nv) in n {
                        match nk.as_str() {
                            "type" => {}
                            "label" => match nv.as_str() {
                                Some(s) if s.chars().count() <= MAX_LABEL => node.label = Some(s.into()),
                                Some(_) => errors.push(Error::at("out_of_range", id, None, format!("label is over {MAX_LABEL} characters"))),
                                None => errors.push(Error::at("type_mismatch", id, None, "label must be a string")),
                            },
                            "inputs" => {
                                let m = nv.as_object().ok_or_else(|| parse_err(format!("node '{id}' inputs must be an object")))?;
                                node.inputs = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            }
                            "pos" => match nv.as_array().map(|a| a.as_slice()) {
                                Some([Json::Number(x), Json::Number(y)]) => node.pos = Some([x.clone(), y.clone()]),
                                _ => errors.push(Error::at("type_mismatch", id, None, "pos must be [number, number]")),
                            },
                            _ => errors.push(unknown_key(nk, Some(id), None)),
                        }
                    }
                    p.nodes.insert(id.clone(), node);
                }
            }
            _ => errors.push(unknown_key(k, None, None)),
        }
    }
    Ok((p, errors))
}

pub fn valid_id(id: &str) -> bool {
    let mut c = id.chars();
    c.next().is_some_and(|f| f.is_ascii_lowercase())
        && id.len() <= 64
        && id.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
}

/// A parsed ref: `"node"` or `"node.output"`.
pub fn split_ref(r: &str) -> (&str, Option<&str>) {
    match r.split_once('.') {
        Some((n, o)) => (n, Some(o)),
        None => (r, None),
    }
}

/// What an input value is, before checking it against the catalog.
pub enum Raw<'a> {
    Ref(&'a str),
    BadRef(Error),
    Literal(&'a Json),
}

pub fn classify<'a>(node: &str, input: &str, v: &'a Json) -> Raw<'a> {
    match v.as_object() {
        Some(o) if o.contains_key("ref") => {
            if let Some(k) = o.keys().find(|k| *k != "ref") {
                return Raw::BadRef(unknown_key(k, Some(node), Some(input)));
            }
            match o["ref"].as_str() {
                Some(r) => Raw::Ref(r),
                None => Raw::BadRef(Error::at("type_mismatch", node, Some(input), "ref must be a string")),
            }
        }
        _ => Raw::Literal(v),
    }
}

/// Resolve a ref against the project: the node's def and output index.
fn resolve(p: &Project, r: &str) -> Result<Option<(&'static NodeDef, usize)>, (&'static str, String)> {
    let (id, out) = split_ref(r);
    let node = p.nodes.get(id).ok_or(("dangling_ref", format!("refers to missing node '{id}'")))?;
    let Some(def) = catalog::get(&node.ty) else { return Ok(None) };
    match out {
        None => Ok(Some((def, 0))),
        Some(o) => def
            .output(o)
            .map(|(i, _)| Some((def, i)))
            .ok_or(("unknown_output", format!("node '{id}' ({}) has no output '{o}'", def.ty))),
    }
}

/// The type an output carries: an `any` output has the type of its node's first `any` input's source; None when unknown.
fn output_type(p: &Project, id: &str, def: &'static NodeDef, oi: usize) -> Option<Type> {
    let (mut id, mut def, mut oi) = (id, def, oi);
    for _ in 0..=p.nodes.len() {
        if def.outputs[oi].ty != Type::Any {
            return Some(def.outputs[oi].ty);
        }
        let input = def.inputs.iter().find(|i| i.ty == Type::Any)?;
        let r = p.nodes[id].inputs.get(input.name)?.get("ref")?.as_str()?;
        (def, oi) = resolve(p, r).ok().flatten()?;
        id = split_ref(r).0;
    }
    None // a cycle, reported elsewhere
}

/// Catalog-level validation; collects every error.
pub fn validate(p: &Project) -> Vec<Error> {
    let mut errors = vec![];
    let s = &p.settings;
    for (k, v, lo, hi) in [("width", s.width, 1, 4096), ("height", s.height, 1, 4096), ("frames", s.frames, 1, 10000)] {
        if v < lo || v > hi {
            errors.push(Error::new("out_of_range", format!("settings.{k} is {v}, outside {lo}..{hi}")));
        }
    }
    let fps = s.fps();
    if !(fps > 0.0 && fps <= 240.0) {
        errors.push(Error::new("out_of_range", format!("settings.fps is {fps}, outside 0..240")));
    }
    if s.width >= 1 && s.height >= 1 && (s.width * s.height) as u64 > surface::MAX_AREA {
        errors.push(Error::new("too_large", format!("settings {}x{} is over 2048² pixels", s.width, s.height)));
    }
    if p.nodes.len() > MAX_NODES {
        errors.push(Error::new("too_large", format!("{} nodes, the limit is {MAX_NODES}", p.nodes.len())));
    }
    for (id, node) in &p.nodes {
        if !valid_id(id) {
            errors.push(Error::at("invalid_id", id, None, format!("node id '{id}' must match ^[a-z][a-z0-9_]{{0,63}}$")));
        }
        let Some(def) = catalog::get(&node.ty) else {
            let hint = catalog::suggest(&node.ty);
            let hint = if hint.is_empty() { String::new() } else { format!("; did you mean {}?", hint.join(", ")) };
            errors.push(Error::at("unknown_type", id, None, format!("unknown node type '{}'{hint}", node.ty)));
            continue;
        };
        for (name, v) in &node.inputs {
            let Some((_, idef)) = def.input(name) else {
                errors.push(Error::at("unknown_input", id, Some(name), format!("{} has no input '{name}'", def.ty)));
                continue;
            };
            match classify(id, name, v) {
                Raw::BadRef(e) => errors.push(e),
                Raw::Ref(r) => match resolve(p, r) {
                    Err((code, m)) => errors.push(Error::at(code, id, Some(name), m)),
                    Ok(None) => {}
                    Ok(Some((src, oi))) => {
                        let oty = output_type(p, split_ref(r).0, src, oi);
                        if oty.is_some_and(|oty| !fits(oty, idef.ty)) {
                            let oty = oty.unwrap();
                            errors.push(Error::at(
                                "type_mismatch",
                                id,
                                Some(name),
                                format!("ref '{r}' is {}, input '{name}' expects {}", oty.name(), idef.ty.name()),
                            ));
                        }
                    }
                },
                Raw::Literal(j) if crate::anim::is_anim(j) => {
                    if let Err((code, m)) = crate::anim::parse(idef, j) {
                        errors.push(Error::at(code, id, Some(name), m));
                    }
                }
                Raw::Literal(j) => {
                    if let Err((code, m)) = types::check_literal(idef, j) {
                        errors.push(Error::at(code, id, Some(name), m));
                    }
                }
            }
        }
        for idef in def.inputs.iter().filter(|i| i.required && !node.inputs.contains_key(i.name)) {
            errors.push(Error::at("missing_input", id, Some(idef.name), format!("required input '{}' is not set", idef.name)));
        }
    }
    for id in cycle_nodes(p) {
        errors.push(Error::at("cycle", &id, None, format!("node '{id}' is part of a cycle")));
    }
    errors
}

fn deps<'p>(p: &'p Project, node: &'p Node) -> impl Iterator<Item = &'p str> {
    node.inputs.iter().filter_map(move |(name, v)| match classify("", name, v) {
        Raw::Ref(r) => {
            let id = split_ref(r).0;
            p.nodes.contains_key(id).then_some(id)
        }
        _ => None,
    })
}

/// Nodes that can reach themselves through refs.
fn cycle_nodes(p: &Project) -> Vec<String> {
    let mut out = vec![];
    for (id, node) in &p.nodes {
        let mut seen = std::collections::HashSet::new();
        let mut stack: Vec<&str> = deps(p, node).collect();
        while let Some(n) = stack.pop() {
            if n == id {
                out.push(id.clone());
                break;
            }
            if seen.insert(n) {
                stack.extend(deps(p, &p.nodes[n]));
            }
        }
    }
    out
}

/// A validated input.
#[derive(Debug, Clone, PartialEq)]
pub enum GInput {
    Value(Value),
    Ref { node: String, output: String },
    /// A keyframed input.
    Anim(crate::anim::Anim),
    /// A required input that is not set.
    Missing,
}

pub struct GNode {
    pub def: &'static NodeDef,
    pub inputs: Vec<GInput>,
}

/// A project resolved against the catalog, ready to evaluate.
pub struct Graph {
    pub nodes: IndexMap<String, GNode>,
}

impl Graph {
    pub fn input(&self, node: &str, input: &str) -> Option<&GInput> {
        let n = self.nodes.get(node)?;
        let (i, _) = n.def.input(input)?;
        n.inputs.get(i)
    }

    /// A literal or animated input's value at a frame (None for refs and missing inputs).
    pub fn input_at(&self, node: &str, input: &str, frame: i64, frames: i64) -> Option<Value> {
        let n = self.nodes.get(node)?;
        let (i, def) = n.def.input(input)?;
        match &n.inputs[i] {
            GInput::Value(v) => Some(v.clone()),
            GInput::Anim(a) => Some(a.value_at(frame, frames, def.ty)),
            _ => None,
        }
    }
}

/// Resolve a project; fails on any error except `missing_input`.
pub fn graph(p: &Project) -> Result<Graph, Vec<Error>> {
    let errors: Vec<Error> = validate(p).into_iter().filter(|e| e.code != "missing_input").collect();
    if !errors.is_empty() {
        return Err(errors);
    }
    let size = p.settings.size();
    let mut nodes = IndexMap::new();
    for (id, node) in &p.nodes {
        let def = catalog::get(&node.ty).unwrap();
        let inputs = def
            .inputs
            .iter()
            .map(|idef| match node.inputs.get(idef.name) {
                None if idef.required => GInput::Missing,
                None => GInput::Value(types::default_value(idef, size).unwrap_or(Value::None)),
                Some(v) => match classify(id, idef.name, v) {
                    Raw::Ref(r) => {
                        let (src, out) = split_ref(r);
                        let sdef = catalog::get(&p.nodes[src].ty).unwrap();
                        GInput::Ref { node: src.into(), output: out.unwrap_or(sdef.outputs[0].name).into() }
                    }
                    Raw::Literal(j) if crate::anim::is_anim(j) => GInput::Anim(crate::anim::parse(idef, j).unwrap()),
                    Raw::Literal(j) => GInput::Value(types::check_literal(idef, j).unwrap()),
                    Raw::BadRef(_) => unreachable!("validated"),
                },
            })
            .collect();
        nodes.insert(id.clone(), GNode { def, inputs });
    }
    Ok(Graph { nodes })
}

impl Project {
    pub fn to_json(&self) -> Json {
        let s = &self.settings;
        let nodes: Map<String, Json> = self
            .nodes
            .iter()
            .map(|(id, n)| {
                let mut m = Map::new();
                m.insert("type".into(), json!(n.ty));
                if let Some(l) = &n.label {
                    m.insert("label".into(), json!(l));
                }
                if !n.inputs.is_empty() {
                    m.insert("inputs".into(), Json::Object(n.inputs.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
                }
                if let Some(pos) = &n.pos {
                    m.insert("pos".into(), json!(pos));
                }
                (id.clone(), Json::Object(m))
            })
            .collect();
        json!({
            "slopc": 1,
            "settings": { "width": s.width, "height": s.height, "frames": s.frames, "fps": s.fps },
            "nodes": nodes,
        })
    }

    /// Canonical file form: 2-space indent, `\n`, trailing newline.
    pub fn to_canonical_string(&self) -> String {
        let mut s = serde_json::to_string_pretty(&self.to_json()).unwrap();
        s.push('\n');
        s
    }
}

#[derive(Debug)]
pub enum SaveError {
    Invalid(Vec<Error>),
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Invalid(e) => write!(f, "project is invalid: {}", e.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")),
            SaveError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

pub fn lock_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".lock");
    PathBuf::from(s)
}

/// Hold the project's sidecar lock for the duration of `f` (read-modify-write).
pub fn with_lock<T>(path: &Path, f: impl FnOnce() -> T) -> std::io::Result<T> {
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(lock_path(path))?;
    lock.lock()?;
    let out = f();
    lock.unlock()?;
    Ok(out)
}

/// Write `bytes` to `path` atomically: temp file in the same directory, fsync, rename.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_atomic_with(path, bytes, |a, b| std::fs::rename(a, b))
}

#[doc(hidden)]
pub fn write_atomic_with(path: &Path, bytes: &[u8], rename: impl Fn(&Path, &Path) -> std::io::Result<()>) -> std::io::Result<()> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}-{nanos}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Validate and save under the project's lock. `missing_input` errors do not block saving.
pub fn save(path: &Path, p: &Project) -> Result<(), SaveError> {
    let errors: Vec<Error> = validate(p).into_iter().filter(|e| e.code != "missing_input").collect();
    if !errors.is_empty() {
        return Err(SaveError::Invalid(errors));
    }
    with_lock(path, || write_atomic(path, p.to_canonical_string().as_bytes()))??;
    Ok(())
}
