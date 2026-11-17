//! Edit operations and atomic batches (spec/ops.md).

use crate::catalog;
use crate::project::{self, Error, Node, Project, Raw};
use serde_json::{Map, Number, Value as Json, json};
use std::path::Path;

pub struct Applied {
    pub project: Project,
    pub created: Vec<String>,
    pub warnings: Vec<Error>,
    /// Ops that revert this batch, and ops that redo it (spec/ops.md#undo-data).
    pub undo: Vec<Json>,
    pub redo: Vec<Json>,
}

pub type Outcome = Result<Applied, Vec<Error>>;

fn op_err(i: usize, code: &str, node: Option<&str>, msg: impl Into<String>) -> Error {
    let mut e = match node {
        Some(n) => Error::at(code, n, None, msg),
        None => Error::new(code, msg),
    };
    e.op = Some(i);
    e
}

struct Op<'a> {
    i: usize,
    obj: &'a Map<String, Json>,
}

impl<'a> Op<'a> {
    fn bad(&self, msg: impl Into<String>) -> Error {
        op_err(self.i, "bad_op", None, msg)
    }
    fn allow(&self, fields: &[&str]) -> Result<(), Error> {
        match self.obj.keys().find(|k| *k != "op" && !fields.contains(&k.as_str())) {
            Some(k) => Err(self.bad(format!("unknown field '{k}'"))),
            None => Ok(()),
        }
    }
    fn str(&self, k: &str) -> Result<&'a str, Error> {
        self.obj.get(k).and_then(Json::as_str).ok_or_else(|| self.bad(format!("field '{k}' must be a string")))
    }
    fn pos(&self, v: &Json) -> Result<[Number; 2], Error> {
        match v.as_array().map(|a| a.as_slice()) {
            Some([Json::Number(x), Json::Number(y)]) => Ok([x.clone(), y.clone()]),
            _ => Err(self.bad("pos must be [number, number]")),
        }
    }
    fn label(&self, v: &Json) -> Result<Option<String>, Error> {
        match v {
            Json::Null => Ok(None),
            Json::String(s) => Ok(Some(s.clone())),
            _ => Err(self.bad("label must be a string or null")),
        }
    }
    /// The node named by field `id`.
    fn node<'p>(&self, p: &'p mut Project) -> Result<(String, &'p mut Node), Error> {
        let id = self.str("id")?;
        match p.nodes.get_mut(id) {
            Some(n) => Ok((id.to_string(), n)),
            None => Err(op_err(self.i, "unknown_node", Some(id), format!("no node '{id}'"))),
        }
    }
}

fn ref_target(v: &Json) -> Option<&str> {
    match project::classify("", "", v) {
        Raw::Ref(r) => Some(project::split_ref(r).0),
        _ => None,
    }
}

fn apply_one(p: &mut Project, op: &Op, created: &mut Vec<String>) -> Result<(), Error> {
    let i = op.i;
    match op.str("op")? {
        "add_node" => {
            op.allow(&["type", "id", "inputs", "pos", "label"])?;
            let ty = op.str("type")?;
            if catalog::get(ty).is_none() {
                return Err(op_err(i, "unknown_type", None, format!("unknown node type '{ty}'")));
            }
            let id = match op.obj.get("id") {
                Some(_) => op.str("id")?.to_string(),
                None => (1..).map(|n| format!("{ty}{n}")).find(|c| !p.nodes.contains_key(c)).unwrap(),
            };
            if !project::valid_id(&id) {
                return Err(op_err(i, "invalid_id", Some(&id), format!("node id '{id}' must match ^[a-z][a-z0-9_]{{0,63}}$")));
            }
            if p.nodes.contains_key(&id) {
                return Err(op_err(i, "duplicate_id", Some(&id), format!("node '{id}' already exists")));
            }
            let mut node = Node::new(ty);
            if let Some(v) = op.obj.get("inputs") {
                let m = v.as_object().ok_or_else(|| op.bad("inputs must be an object"))?;
                node.inputs = m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            }
            if let Some(v) = op.obj.get("pos") {
                node.pos = Some(op.pos(v)?);
            }
            if let Some(v) = op.obj.get("label") {
                node.label = op.label(v)?;
            }
            p.nodes.insert(id.clone(), node);
            created.push(id);
        }
        "remove_node" => {
            op.allow(&["id"])?;
            let (id, _) = op.node(p)?;
            p.nodes.shift_remove(&id);
            for n in p.nodes.values_mut() {
                n.inputs.retain(|_, v| ref_target(v) != Some(id.as_str()));
            }
        }
        "rename_node" => {
            op.allow(&["id", "new_id"])?;
            let (id, _) = op.node(p)?;
            let new_id = op.str("new_id")?;
            if !project::valid_id(new_id) {
                return Err(op_err(i, "invalid_id", Some(new_id), format!("node id '{new_id}' must match ^[a-z][a-z0-9_]{{0,63}}$")));
            }
            if p.nodes.contains_key(new_id) {
                return Err(op_err(i, "duplicate_id", Some(new_id), format!("node '{new_id}' already exists")));
            }
            p.nodes = std::mem::take(&mut p.nodes).into_iter().map(|(k, v)| (if k == id { new_id.to_string() } else { k }, v)).collect();
            for n in p.nodes.values_mut() {
                for v in n.inputs.values_mut() {
                    if let Raw::Ref(r) = project::classify("", "", v) {
                        let (target, out) = project::split_ref(r);
                        if target == id {
                            *v = json!({ "ref": out.map_or(new_id.to_string(), |o| format!("{new_id}.{o}")) });
                        }
                    }
                }
            }
        }
        "move_node" => {
            op.allow(&["id", "pos"])?;
            let pos = match op.obj.get("pos") {
                Some(Json::Null) => None,
                v => Some(op.pos(v.unwrap_or(&Json::Null))?),
            };
            op.node(p)?.1.pos = pos;
        }
        "set_input" | "unset_input" => {
            let set = op.str("op")? == "set_input";
            op.allow(if set { &["id", "input", "value"] } else { &["id", "input"] })?;
            let input = op.str("input")?;
            let value = if set { Some(op.obj.get("value").ok_or_else(|| op.bad("field 'value' is required"))?.clone()) } else { None };
            let (id, node) = op.node(p)?;
            if catalog::get(&node.ty).is_some_and(|d| d.input(input).is_none()) {
                let mut e = op_err(i, "unknown_input", Some(&id), format!("{} has no input '{input}'", node.ty));
                e.input = Some(input.into());
                return Err(e);
            }
            match value {
                Some(v) => {
                    node.inputs.insert(input.into(), v);
                }
                None => {
                    node.inputs.shift_remove(input);
                }
            }
        }
        "set_label" => {
            op.allow(&["id", "label"])?;
            let label = op.label(op.obj.get("label").ok_or_else(|| op.bad("field 'label' is required"))?)?;
            op.node(p)?.1.label = label;
        }
        "set_settings" => {
            op.allow(&["width", "height", "frames", "fps"])?;
            for (k, v) in op.obj.iter().filter(|(k, _)| *k != "op") {
                let int = || v.as_i64().filter(|_| !v.is_f64()).ok_or_else(|| op.bad(format!("{k} must be an integer")));
                match k.as_str() {
                    "width" => p.settings.width = int()?,
                    "height" => p.settings.height = int()?,
                    "frames" => p.settings.frames = int()?,
                    _ => match v {
                        Json::Number(n) => p.settings.fps = n.clone(),
                        _ => return Err(op.bad("fps must be a number")),
                    },
                }
            }
        }
        "set_key" | "remove_key" => {
            let set = op.str("op")? == "set_key";
            op.allow(if set { &["id", "input", "time", "value", "ease_in", "ease_out", "ease_in_type", "ease_out_type"] } else { &["id", "input", "time"] })?;
            let input = op.str("input")?.to_string();
            let time = op.obj.get("time").and_then(Json::as_i64).ok_or_else(|| op.bad("field 'time' must be an integer frame"))?;
            let (id, node) = op.node(p)?;
            if catalog::get(&node.ty).is_some_and(|d| d.input(&input).is_none()) {
                let mut e = op_err(i, "unknown_input", Some(&id), format!("{} has no input '{input}'", node.ty));
                e.input = Some(input);
                return Err(e);
            }
            let cur = node.inputs.get(&input).cloned();
            if cur.as_ref().is_some_and(|v| ref_target(v).is_some()) {
                return Err(op.bad(format!("input '{input}' is connected; disconnect it before keying")));
            }
            let mut keys: Vec<Json> = match &cur {
                Some(v) if crate::anim::is_anim(v) => v["anim"].as_array().cloned().unwrap_or_default(),
                _ => vec![],
            };
            let same = |k: &Json| k.get("time").and_then(Json::as_i64) == Some(time);
            if set {
                let mut key = serde_json::Map::new();
                key.insert("time".into(), json!(time));
                key.insert("value".into(), op.obj.get("value").ok_or_else(|| op.bad("field 'value' is required"))?.clone());
                for f in ["ease_in", "ease_out", "ease_in_type", "ease_out_type"] {
                    if let Some(v) = op.obj.get(f) {
                        key.insert(f.into(), v.clone());
                    }
                }
                keys.retain(|k| !same(k));
                keys.push(Json::Object(key));
                keys.sort_by_key(|k| k.get("time").and_then(Json::as_i64).unwrap_or(0));
            } else {
                let Some(pos) = keys.iter().position(same) else { return Err(op.bad(format!("input '{input}' has no key at frame {time}"))) };
                let removed = keys.remove(pos);
                if keys.is_empty() {
                    node.inputs.insert(input, removed["value"].clone());
                    return Ok(());
                }
            }
            let mut anim = match cur {
                Some(Json::Object(o)) if o.contains_key("anim") => o,
                _ => serde_json::Map::new(),
            };
            anim.insert("anim".into(), Json::Array(keys));
            node.inputs.insert(input, Json::Object(anim));
        }
        other => return Err(op.bad(format!("unknown op '{other}'"))),
    }
    Ok(())
}

/// Apply a batch (an array of ops, or one op object) to a copy of `p`.
pub fn apply(before: &Project, ops: &Json) -> Outcome {
    let p = before;
    let list: Vec<&Json> = match ops {
        Json::Array(a) => a.iter().collect(),
        Json::Object(_) => vec![ops],
        _ => return Err(vec![Error::new("bad_op", "ops must be an array of op objects")]),
    };
    let mut p = p.clone();
    let mut created = vec![];
    for (i, v) in list.into_iter().enumerate() {
        let obj = v.as_object().ok_or_else(|| vec![op_err(i, "bad_op", None, "an op must be an object")])?;
        apply_one(&mut p, &Op { i, obj }, &mut created).map_err(|e| vec![e])?;
    }
    let (warnings, errors): (Vec<Error>, Vec<Error>) = project::validate(&p).into_iter().partition(|e| e.code == "missing_input");
    if !errors.is_empty() {
        return Err(errors);
    }
    let undo = restore_ops(&p, before);
    let redo = restore_ops(before, &p);
    Ok(Applied { project: p, created, warnings, undo, redo })
}

/// Load, apply and save under the project lock. `Err` is an I/O error.
pub fn apply_file(path: &Path, ops: &Json) -> std::io::Result<Outcome> {
    project::with_lock(path, || {
        let l = project::load(path)?;
        let Some(p) = l.project else { return Ok(Err(l.errors)) };
        let outcome = apply(&p, ops);
        if let Ok(a) = &outcome {
            project::write_atomic(path, a.project.to_canonical_string().as_bytes())?;
        }
        Ok(outcome)
    })?
}

/// A batch that turns `current` into `target`, touching only what differs.
pub fn restore_ops(current: &Project, target: &Project) -> Vec<Json> {
    let mut ops = vec![];
    let removed: std::collections::HashSet<&str> = current
        .nodes
        .iter()
        .filter(|(id, n)| target.nodes.get(*id).is_none_or(|t| t.ty != n.ty))
        .map(|(id, _)| id.as_str())
        .collect();
    for id in current.nodes.keys().filter(|id| removed.contains(id.as_str())) {
        ops.push(json!({ "op": "remove_node", "id": id }));
    }
    for (id, t) in &target.nodes {
        match current.nodes.get(id).filter(|_| !removed.contains(id.as_str())) {
            None => {
                let mut op = json!({ "op": "add_node", "id": id, "type": t.ty });
                if !t.inputs.is_empty() {
                    op["inputs"] = Json::Object(t.inputs.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                }
                if let Some(l) = &t.label {
                    op["label"] = json!(l);
                }
                if let Some(pos) = &t.pos {
                    op["pos"] = json!(pos);
                }
                ops.push(op);
            }
            Some(c) => {
                if c.label != t.label {
                    ops.push(json!({ "op": "set_label", "id": id, "label": t.label }));
                }
                if c.pos != t.pos {
                    ops.push(json!({ "op": "move_node", "id": id, "pos": t.pos }));
                }
                // remove_node above unsets refs to removed nodes, so those count as unset.
                let effective = |k: &str| c.inputs.get(k).filter(|v| ref_target(v).is_none_or(|r| !removed.contains(r)));
                for (k, v) in &t.inputs {
                    if effective(k) != Some(v) {
                        ops.push(json!({ "op": "set_input", "id": id, "input": k, "value": v }));
                    }
                }
                for k in c.inputs.keys().filter(|k| !t.inputs.contains_key(*k) && effective(k).is_some()) {
                    ops.push(json!({ "op": "unset_input", "id": id, "input": k }));
                }
            }
        }
    }
    let (cs, ts) = (&current.settings, &target.settings);
    let mut set = serde_json::Map::new();
    for (k, a, b) in [("width", cs.width, ts.width), ("height", cs.height, ts.height), ("frames", cs.frames, ts.frames)] {
        if a != b {
            set.insert(k.into(), json!(b));
        }
    }
    if cs.fps != ts.fps {
        set.insert("fps".into(), json!(ts.fps));
    }
    if !set.is_empty() {
        set.insert("op".into(), json!("set_settings"));
        ops.push(Json::Object(set));
    }
    ops
}

pub fn outcome_json(o: &Outcome) -> Json {
    match o {
        Ok(a) => json!({ "ok": true, "created": a.created, "warnings": a.warnings }),
        Err(e) => json!({ "ok": false, "errors": e }),
    }
}
