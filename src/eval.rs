//! Graph evaluation with a content-keyed cache (spec/evaluation.md).

use crate::catalog::NodeDef;
use crate::gpu::Gpu;
use crate::project::{self, Error, GInput, Graph, Project};
use crate::surface;
use crate::types::{Type, Value};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub enum NodeError {
    Eval(String),
    Gpu(String),
}

impl From<String> for NodeError {
    fn from(s: String) -> Self {
        NodeError::Eval(s)
    }
}

impl From<&str> for NodeError {
    fn from(s: &str) -> Self {
        NodeError::Eval(s.into())
    }
}

/// What a node can see while running.
pub struct Ctx<'a> {
    pub def: &'static NodeDef,
    pub project_size: [u32; 2],
    pub frame: i64,
    /// Total frames in the project.
    pub frames: i64,
    pub dir: &'a Path,
    /// When set, files read by nodes must resolve inside this (canonical) directory.
    pub root: Option<&'a Path>,
}

impl Ctx<'_> {
    pub fn gpu(&self) -> Result<&'static Gpu, NodeError> {
        Gpu::get().map_err(NodeError::Gpu)
    }
    /// A file path from a `path` input, relative to the project file and checked against the root.
    pub fn path(&self, rel: &str) -> Result<PathBuf, NodeError> {
        let p = self.dir.join(rel);
        if let Some(root) = self.root {
            let real = p.canonicalize().map_err(|e| format!("cannot read '{rel}': {e}"))?;
            if !real.starts_with(root) {
                return Err(format!("'{rel}' is outside the root").into());
            }
        }
        Ok(p)
    }
}

struct Entry {
    outputs: Arc<Vec<Value>>,
    bytes: usize,
    used: u64,
}

pub struct Evaluator {
    /// Restrict files read by nodes to this canonical directory (MCP and server).
    pub root: Option<PathBuf>,
    cache: HashMap<u64, Entry>,
    cache_bytes: usize,
    limit: usize,
    tick: u64,
    last: Vec<String>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-call state: node keys and outputs computed during this evaluation.
struct Run<'a> {
    graph: &'a Graph,
    dir: &'a Path,
    frame: i64,
    frames: i64,
    fps: f64,
    size: [u32; 2],
    keys: HashMap<String, u64>,
    outputs: HashMap<String, Arc<Vec<Value>>>,
}

fn hash_value(v: &Value, h: &mut impl Hasher) {
    std::mem::discriminant(v).hash(h);
    match v {
        Value::Number(x) => x.to_bits().hash(h),
        Value::Int(x) => x.hash(h),
        Value::Bool(x) => x.hash(h),
        Value::Color(x) => x.hash(h),
        Value::Vec2(x) => x.map(f64::to_bits).hash(h),
        Value::Dimension(x) => x.hash(h),
        Value::Padding(x) => x.hash(h),
        Value::Palette(x) => x.hash(h),
        Value::Gradient(g) => {
            g.constant.hash(h);
            for (t, c) in &g.keys {
                (t.to_bits(), c).hash(h);
            }
        }
        Value::Enum(x) => x.hash(h),
        Value::Str(x) | Value::Path(x) => x.hash(h),
        Value::Surface(s) => (s.width, s.height, &s.data).hash(h),
        Value::None => {}
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Evaluator { root: None, cache: HashMap::new(), cache_bytes: 0, limit: 256 << 20, tick: 0, last: vec![] }
    }

    /// Node ids executed (not served from cache) by the last `eval`.
    pub fn last_runs(&self) -> &[String] {
        &self.last
    }

    /// Evaluate `target` (`node` or `node.output`) at `frame`.
    pub fn eval(&mut self, p: &Project, dir: &Path, target: &str, frame: i64) -> Result<Value, Error> {
        self.last.clear();
        let graph = project::graph(p).map_err(|mut e| e.remove(0))?;
        let (id, out) = project::split_ref(target);
        let node = graph.nodes.get(id).ok_or_else(|| Error::new("dangling_ref", format!("no node '{id}'")))?;
        let oi = match out {
            None => 0,
            Some(o) => node
                .def
                .output(o)
                .ok_or_else(|| Error::at("unknown_output", id, None, format!("node '{id}' has no output '{o}'")))?
                .0,
        };
        if frame < 0 || frame >= p.settings.frames {
            return Err(Error::new("frame_out_of_range", format!("frame {frame} is outside 0..{}", p.settings.frames)));
        }
        let mut run = Run { graph: &graph, dir, frame, frames: p.settings.frames, fps: p.settings.fps(), size: p.settings.size(), keys: HashMap::new(), outputs: HashMap::new() };
        let outs = self.node(&mut run, id)?;
        Ok(outs[oi].clone())
    }

    /// Cache keys of every node that can be keyed (for thumbnails); nodes with missing inputs are absent.
    pub fn keys(p: &Project, dir: &Path, frame: i64) -> HashMap<String, String> {
        let Ok(graph) = project::graph(p) else { return HashMap::new() };
        let mut run = Run { graph: &graph, dir, frame, frames: p.settings.frames, fps: p.settings.fps(), size: p.settings.size(), keys: HashMap::new(), outputs: HashMap::new() };
        let mut out = HashMap::new();
        for id in graph.nodes.keys() {
            if let Ok(k) = key(&mut run, id) {
                out.insert(id.clone(), format!("{k:016x}"));
            }
        }
        out
    }

    fn node(&mut self, run: &mut Run, id: &str) -> Result<Arc<Vec<Value>>, Error> {
        if let Some(o) = run.outputs.get(id) {
            return Ok(o.clone());
        }
        let k = key(run, id)?;
        self.tick += 1;
        if let Some(e) = self.cache.get_mut(&k) {
            e.used = self.tick;
            let o = e.outputs.clone();
            run.outputs.insert(id.into(), o.clone());
            return Ok(o);
        }
        let gnode = &run.graph.nodes[id];
        let mut values = Vec::with_capacity(gnode.inputs.len());
        let mut remap_source = None;
        for (gi, idef) in gnode.inputs.iter().zip(gnode.def.inputs) {
            values.push(match gi {
                GInput::Value(v) => v.clone(),
                GInput::Anim(a) => a.value_at(run.frame, run.frames, idef.ty),
                GInput::Missing => unreachable!("checked by key"),
                // Time-remap nodes read their surface at another frame, below.
                GInput::Ref { node, output } if gnode.def.remap.is_some() && idef.ty == Type::Surface => {
                    remap_source = Some((node.clone(), output.clone()));
                    Value::None
                }
                GInput::Ref { node, output } => {
                    let src = self.node(run, node)?;
                    let (oi, _) = run.graph.nodes[node.as_str()].def.output(output).unwrap();
                    coerce(&src[oi], idef)
                }
            });
        }
        if let (Some(remap), Some((src, out))) = (gnode.def.remap, remap_source) {
            let oi = run.graph.nodes[src.as_str()].def.output(&out).unwrap().0;
            let outputs = match remap(&values, run.frame, run.frames, run.fps) {
                Some(f) => {
                    let mut sub = Run { graph: run.graph, dir: run.dir, frame: f.clamp(0, run.frames - 1), frames: run.frames, fps: run.fps, size: run.size, keys: HashMap::new(), outputs: HashMap::new() };
                    vec![self.node(&mut sub, &src)?[oi].clone()]
                }
                None => {
                    let s = self.node(run, &src)?[oi].clone();
                    let s = s.surface().expect("surface source");
                    vec![Value::Surface(Arc::new(surface::Surface::new(s.width, s.height)))]
                }
            };
            self.last.push(id.into());
            let outputs = Arc::new(outputs);
            self.insert(k, outputs.clone(), 0);
            run.outputs.insert(id.into(), outputs.clone());
            return Ok(outputs);
        }
        let mut ctx = Ctx { def: gnode.def, project_size: run.size, frame: run.frame, frames: run.frames, dir: run.dir, root: self.root.as_deref() };
        let outputs = (gnode.def.run)(&mut ctx, &values).map_err(|e| match e {
            NodeError::Eval(m) => Error::at("eval_error", id, None, m),
            NodeError::Gpu(m) => Error::at("gpu_unavailable", id, None, m),
        })?;
        let mut bytes = 0;
        for v in &outputs {
            if let Value::Surface(s) = v {
                if !surface::size_ok(s.width, s.height) {
                    return Err(Error::at("eval_error", id, None, format!("output is {}x{}, outside the size limits", s.width, s.height)));
                }
                bytes += s.data.len();
            }
        }
        self.last.push(id.into());
        let outputs = Arc::new(outputs);
        self.insert(k, outputs.clone(), bytes);
        run.outputs.insert(id.into(), outputs.clone());
        Ok(outputs)
    }

    fn insert(&mut self, k: u64, outputs: Arc<Vec<Value>>, bytes: usize) {
        while self.cache_bytes + bytes > self.limit && !self.cache.is_empty() {
            let (&old, _) = self.cache.iter().min_by_key(|(_, e)| e.used).unwrap();
            self.cache_bytes -= self.cache.remove(&old).unwrap().bytes;
        }
        self.cache_bytes += bytes;
        self.cache.insert(k, Entry { outputs, bytes, used: self.tick });
    }
}

/// Clamp ref values into the input's range (ranges are not checkable at load for refs).
fn coerce(v: &Value, idef: &crate::catalog::InputDef) -> Value {
    let clamp = |x: f64| x.clamp(idef.min.unwrap_or(f64::NEG_INFINITY), idef.max.unwrap_or(f64::INFINITY));
    match (v, idef.ty) {
        (Value::Int(i), Type::Number) => Value::Number(clamp(*i as f64)),
        (Value::Number(x), Type::Number) => Value::Number(clamp(*x)),
        (Value::Int(i), Type::Int) => Value::Int(clamp(*i as f64) as i64),
        (Value::Vec2(a), Type::Vec2) => Value::Vec2(a.map(clamp)),
        _ => v.clone(),
    }
}

/// Content key of a node: type, literals, upstream keys, frame if read, contents of read files.
fn key(run: &mut Run, id: &str) -> Result<u64, Error> {
    if let Some(k) = run.keys.get(id) {
        return Ok(*k);
    }
    let gnode = &run.graph.nodes[id];
    let mut h = DefaultHasher::new();
    gnode.def.ty.hash(&mut h);
    if gnode.def.reads_frame {
        run.frame.hash(&mut h);
    }
    for (gi, idef) in gnode.inputs.iter().zip(gnode.def.inputs) {
        match gi {
            GInput::Missing => {
                return Err(Error::at("missing_input", id, Some(idef.name), format!("required input '{}' is not set", idef.name)));
            }
            GInput::Value(_) | GInput::Anim(_) => {
                // An animation is keyed by its value at this frame.
                let v = match gi {
                    GInput::Anim(a) => a.value_at(run.frame, run.frames, idef.ty),
                    GInput::Value(v) => v.clone(),
                    _ => unreachable!(),
                };
                hash_value(&v, &mut h);
                if let Value::Path(rel) = &v {
                    std::fs::read(run.dir.join(rel)).ok().hash(&mut h);
                }
            }
            GInput::Ref { node, output } => {
                key(run, node)?.hash(&mut h);
                output.hash(&mut h);
            }
        }
    }
    let k = h.finish();
    run.keys.insert(id.into(), k);
    Ok(k)
}
