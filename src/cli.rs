//! `picle` command line (spec/cli.md).

use crate::catalog;
use crate::eval::Evaluator;
use crate::project::{self, Error, Project};
use crate::types::Value;
use clap::{Parser, Subcommand};
use serde_json::{Value as Json, json};
use std::path::{Path, PathBuf};

pub const EXIT_INVALID: i32 = 1;
pub const EXIT_USAGE: i32 = 2;
pub const EXIT_IO: i32 = 3;
pub const EXIT_GPU: i32 = 4;

#[derive(Parser)]
#[command(name = "picle", version, about = "Node-based procedural pixel-art compositor")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create an empty project.
    New {
        file: PathBuf,
        #[arg(long, default_value_t = 32)]
        width: u32,
        #[arg(long, default_value_t = 32)]
        height: u32,
        #[arg(long)]
        json: bool,
    },
    /// List node types, or describe one.
    Nodes {
        #[arg(value_name = "TYPE")]
        ty: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Print a project.
    Show {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Check a project.
    Validate {
        file: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Render a node to PNG.
    Render {
        file: PathBuf,
        #[arg(short = 'o', long = "output")]
        out: String,
        /// Node to render (`id` or `id.output`); default: the single `output` node.
        #[arg(long)]
        node: Option<String>,
        #[arg(long, default_value_t = 0, allow_negative_numbers = true)]
        frame: i64,
        #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=32))]
        scale: u32,
        #[arg(long)]
        json: bool,
    },
    /// Export PNG images or sequences, GIFs and APNGs.
    Export {
        file: PathBuf,
        /// Output file: .png (%f or %3f for one file per frame), .gif or .apng.
        #[arg(short = 'o', long = "output")]
        out: String,
        #[command(flatten)]
        options: crate::export::Options,
        #[arg(long)]
        json: bool,
    },
    /// Apply a batch of edit ops (a JSON array, or one op object; `-` reads stdin).
    Edit {
        file: PathBuf,
        ops: String,
        #[arg(long)]
        json: bool,
    },
    /// Run the MCP server on stdio.
    Mcp {
        #[arg(long, default_value = ".")]
        root: PathBuf,
    },
    /// Serve the web UI, JSON API and events.
    Serve {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 7420)]
        port: u16,
        /// Access token (default: $PICLE_TOKEN, else random).
        #[arg(long)]
        token: Option<String>,
    },
    /// Print the project file's JSON Schema.
    Schema,
}

pub fn main() -> i32 {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::New { file, width, height, json } => new(&file, width, height, json),
        Cmd::Nodes { ty, json } => nodes(ty.as_deref(), json),
        Cmd::Show { file, json } => show(&file, json),
        Cmd::Validate { file, json } => validate(&file, json),
        Cmd::Render { file, out, node, frame, scale, json } => render(&file, &out, node.as_deref(), frame, scale, json),
        Cmd::Export { file, out, options, json } => export(&file, &out, &options, json),
        Cmd::Edit { file, ops, json } => edit(&file, &ops, json),
        Cmd::Mcp { root } => match crate::mcp::run_stdio(&root) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("picle mcp: {e}");
                EXIT_IO
            }
        },
        Cmd::Serve { root, host, port, token } => match crate::server::run(crate::server::Options { root, host, port, token }) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("picle serve: {e}");
                EXIT_IO
            }
        },
        Cmd::Schema => {
            print!("{}", include_str!("schema.json"));
            0
        }
    }
}

fn print_json(v: &Json) {
    println!("{}", serde_json::to_string(v).unwrap());
}

/// Report errors and return `code`.
pub fn fail(json: bool, code: i32, errors: &[Error]) -> i32 {
    if json {
        print_json(&json!({ "ok": false, "errors": errors }));
    } else {
        for e in errors {
            eprintln!("{e}");
        }
    }
    code
}

fn io_error(json: bool, path: &Path, e: impl std::fmt::Display) -> i32 {
    fail(json, EXIT_IO, &[Error::new("io_error", format!("{}: {e}", path.display()))])
}

pub fn exit_code(e: &Error) -> i32 {
    if e.code == "gpu_unavailable" { EXIT_GPU } else { EXIT_INVALID }
}

fn new(file: &Path, width: u32, height: u32, json: bool) -> i32 {
    if !file.to_string_lossy().ends_with(".picle.json") {
        eprintln!("error: project files must end in .picle.json");
        return EXIT_USAGE;
    }
    let mut p = Project::default();
    p.settings.width = width as i64;
    p.settings.height = height as i64;
    let errors = project::validate(&p);
    if !errors.is_empty() {
        return fail(json, EXIT_INVALID, &errors);
    }
    let r = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .and_then(|_| project::save(file, &p).map_err(|e| std::io::Error::other(e.to_string())));
    if let Err(e) = r {
        return io_error(json, file, e);
    }
    if json {
        print_json(&json!({ "ok": true, "path": file }));
    } else {
        println!("created {}", file.display());
    }
    0
}

fn nodes(ty: Option<&str>, json: bool) -> i32 {
    let Some(ty) = ty else {
        if json {
            print_json(&Json::Array(catalog::all().iter().map(catalog::def_json).collect()));
            return 0;
        }
        let mut cats: Vec<&str> = catalog::all().iter().map(|d| d.category).collect();
        cats.dedup();
        for c in cats {
            println!("[{c}]");
            for d in catalog::all().iter().filter(|d| d.category == c) {
                println!("{:<12} {}", d.ty, d.description);
            }
        }
        return 0;
    };
    let Some(def) = catalog::get(ty) else {
        let hint = catalog::suggest(ty);
        let msg = if hint.is_empty() { format!("unknown node type '{ty}'") } else { format!("unknown node type '{ty}'; did you mean {}?", hint.join(", ")) };
        if json {
            print_json(&json!({ "ok": false, "errors": [Error::new("unknown_type", &msg)] }));
        }
        eprintln!("{msg}");
        return EXIT_INVALID;
    };
    if json {
        print_json(&catalog::def_json(def));
        return 0;
    }
    println!("{} ({}): {}", def.ty, def.category, def.description);
    println!("inputs:");
    for i in def.inputs {
        let j = catalog::input_json(i);
        let default = if i.required { "required".into() } else { j.get("default").map_or("optional".into(), |d| format!("default {d}")) };
        let mut extra = String::new();
        if let (Some(a), Some(b)) = (i.min, i.max) {
            extra = format!(" range {a}..{b}");
        } else if let Some(a) = i.min {
            extra = format!(" min {a}");
        }
        if !i.options.is_empty() {
            extra = format!(" options {}", i.options.join("|"));
        }
        println!("  {:<16} {:<9} {default}{extra} - {}", i.name, i.ty.name(), i.description);
    }
    println!("outputs:");
    for o in def.outputs {
        println!("  {:<16} {}", o.name, o.ty.name());
    }
    0
}

/// Load a project file; `Err` is an exit code after reporting.
fn load(file: &Path, json: bool) -> Result<project::Loaded, i32> {
    project::load(file).map_err(|e| io_error(json, file, e))
}

fn value_text(v: &Json) -> String {
    match v.as_object().and_then(|o| o.get("ref")).and_then(Json::as_str) {
        Some(r) => format!("@{r}"),
        None => v.to_string(),
    }
}

/// Node inputs with defaults filled in (refs kept as written).
pub fn filled_inputs(p: &Project, id: &str) -> serde_json::Map<String, Json> {
    let node = &p.nodes[id];
    let mut m: serde_json::Map<String, Json> = node.inputs.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    if let Some(def) = catalog::get(&node.ty) {
        for i in def.inputs {
            if !m.contains_key(i.name)
                && let Some(v) = crate::types::default_value(i, p.settings.size())
            {
                m.insert(i.name.into(), v.to_json(Some(i)));
            }
        }
    }
    m
}

fn show(file: &Path, json: bool) -> i32 {
    let l = match load(file, json) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let code = if l.errors.is_empty() { 0 } else { EXIT_INVALID };
    let Some(p) = &l.project else {
        return fail(json, EXIT_INVALID, &l.errors);
    };
    if json {
        let mut pj = p.to_json();
        for (id, _) in &p.nodes {
            pj["nodes"][id]["inputs"] = Json::Object(filled_inputs(p, id));
        }
        print_json(&json!({ "ok": l.errors.is_empty(), "project": pj, "errors": l.errors }));
        return code;
    }
    let s = &p.settings;
    println!("{}x{}, {} frame(s), {} fps", s.width, s.height, s.frames, s.fps);
    for (id, n) in &p.nodes {
        let inputs: Vec<String> = filled_inputs(p, id).iter().map(|(k, v)| format!("{k}={}", value_text(v))).collect();
        println!("{id}  {}  {}", n.ty, inputs.join(" "));
    }
    for e in &l.errors {
        eprintln!("{e}");
    }
    code
}

fn validate(file: &Path, json: bool) -> i32 {
    let l = match load(file, json) {
        Ok(l) => l,
        Err(c) => return c,
    };
    if !l.errors.is_empty() {
        if json {
            print_json(&json!({ "ok": false, "errors": l.errors }));
        } else {
            for e in &l.errors {
                println!("{e}");
            }
        }
        return EXIT_INVALID;
    }
    if json {
        print_json(&json!({ "ok": true, "errors": [] }));
    } else {
        println!("ok");
    }
    0
}

/// The default render target: the single `output` node.
pub fn default_target(p: &Project) -> Result<String, Error> {
    let outs: Vec<&String> = p.nodes.iter().filter(|(_, n)| n.ty == "output").map(|(id, _)| id).collect();
    match outs.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(Error::new("no_target", "no `output` node; pass --node")),
        _ => Err(Error::new("ambiguous_target", format!("several `output` nodes ({}); pass --node", outs.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")))),
    }
}

/// Render a project node to a surface (shared by CLI, MCP and server).
pub fn render_surface(p: &Project, dir: &Path, node: Option<&str>, frame: i64, scale: u32, ev: &mut Evaluator) -> Result<crate::surface::Surface, Error> {
    let target = match node {
        Some(n) => n.to_string(),
        None => default_target(p)?,
    };
    match ev.eval(p, dir, &target, frame)? {
        Value::Surface(s) => Ok(s.upscale(scale)),
        v => Err(Error::new("type_mismatch", format!("'{target}' is {}, not a surface", v.ty().map_or("empty", |t| t.name())))),
    }
}

fn render(file: &Path, out: &str, node: Option<&str>, frame: i64, scale: u32, json: bool) -> i32 {
    let l = match load(file, json) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let blocking: Vec<Error> = l.blocking().into_iter().cloned().collect();
    let Some(p) = l.project.as_ref().filter(|_| blocking.is_empty()) else {
        return fail(json, EXIT_INVALID, &blocking);
    };
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let s = match render_surface(p, dir, node, frame, scale, &mut Evaluator::new()) {
        Ok(s) => s,
        Err(e) => return fail(json, exit_code(&e), &[e]),
    };
    let png = s.to_png();
    if out == "-" {
        use std::io::Write;
        std::io::stdout().write_all(&png).unwrap();
        return 0;
    }
    if let Err(e) = project::write_atomic(Path::new(out), &png) {
        return io_error(json, Path::new(out), e);
    }
    if json {
        print_json(&json!({ "ok": true, "path": out, "width": s.width, "height": s.height }));
    } else {
        println!("wrote {out} ({}x{})", s.width, s.height);
    }
    0
}

fn export(file: &Path, out: &str, o: &crate::export::Options, json: bool) -> i32 {
    let l = match load(file, json) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let blocking: Vec<Error> = l.blocking().into_iter().cloned().collect();
    let Some(p) = l.project.as_ref().filter(|_| blocking.is_empty()) else {
        return fail(json, EXIT_INVALID, &blocking);
    };
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let write = |name: &str, bytes: &[u8]| crate::export::write_file(Path::new(name), name, bytes);
    match crate::export::export(p, dir, out, o, &mut Evaluator::new(), write) {
        Ok(x) => {
            if json {
                print_json(&json!({ "ok": true, "files": x.files, "frames": x.frames }));
            } else {
                for f in &x.files {
                    println!("wrote {f}");
                }
            }
            0
        }
        Err(e) => {
            let code = match e.code.as_str() {
                "bad_path" => EXIT_USAGE,
                "io_error" => EXIT_IO,
                _ => exit_code(&e),
            };
            fail(json, code, &[e])
        }
    }
}

fn edit(file: &Path, ops_arg: &str, json: bool) -> i32 {
    let text = if ops_arg == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::Read::read_to_string(&mut std::io::stdin(), &mut s) {
            return io_error(json, Path::new("-"), e);
        }
        s
    } else {
        match std::fs::read_to_string(ops_arg) {
            Ok(s) => s,
            Err(e) => return io_error(json, Path::new(ops_arg), e),
        }
    };
    let ops: Json = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return fail(json, EXIT_INVALID, &[Error::new("bad_op", format!("ops are not valid JSON: {e}"))]),
    };
    if !file.exists() {
        return io_error(json, file, "no such file");
    }
    let outcome = match crate::ops::apply_file(file, &ops) {
        Ok(o) => o,
        Err(e) => return io_error(json, file, e),
    };
    if json {
        print_json(&crate::ops::outcome_json(&outcome));
    }
    match outcome {
        Ok(a) => {
            if !json {
                println!("ok");
                for id in &a.created {
                    println!("created {id}");
                }
                for w in &a.warnings {
                    println!("warning: {w}");
                }
            }
            0
        }
        Err(e) => {
            if !json {
                for e in &e {
                    eprintln!("{}{e}", e.op.map_or(String::new(), |i| format!("op {i}: ")));
                }
            }
            EXIT_INVALID
        }
    }
}
