//! MCP server: tools over stdio and Streamable HTTP (spec/mcp.md).

use crate::catalog;
use crate::cli::render_surface;
use crate::eval::Evaluator;
use crate::ops;
use crate::project::{self, Error, Project};
use crate::sandbox;
use crate::surface::Surface;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{CallToolResult, ContentBlock as Content, ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, ServiceExt, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value as Json, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const INSTRUCTIONS: &str = "slopc builds pixel art from a node graph stored in *.slopc.json files.
Workflow: list_node_types (then list_node_types {type} for inputs) -> create_project -> edit_project with ops -> render to look at the result.
Ops: add_node {type, id?, inputs?}, set_input {id, input, value}, remove_node, rename_node, move_node, unset_input, set_label, set_settings.
Connect nodes with an input value {\"ref\": \"node\"} or {\"ref\": \"node.output\"}. An `output` node marks what render shows.
`missing_input` warnings are fine while building; other errors reject the whole batch.";

#[derive(Clone)]
pub struct Slopc {
    root: PathBuf,
    evaluator: Arc<Mutex<Evaluator>>,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct PathArg {
    /// Project path relative to the root, ending in .slopc.json.
    pub path: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct CreateArgs {
    /// Project path relative to the root, ending in .slopc.json.
    pub path: String,
    /// Canvas width in pixels (default 32).
    pub width: Option<u32>,
    /// Canvas height in pixels (default 32).
    pub height: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct EditArgs {
    /// Project path relative to the root.
    pub path: String,
    /// Ops applied in order as one batch, e.g. [{"op":"add_node","type":"solid","id":"bg"}].
    pub ops: Vec<serde_json::Map<String, Json>>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct TypeArg {
    /// A node type to describe in full; omit to list all types.
    #[serde(rename = "type")]
    pub ty: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
pub struct RenderArgs {
    /// Project path relative to the root.
    pub path: String,
    /// Node to render (`id` or `id.output`); default: the single `output` node.
    pub node: Option<String>,
    /// Frame number (default 0).
    pub frame: Option<i64>,
    /// Integer upscale 1-32; default fits the longer side in 512 px.
    pub scale: Option<u32>,
}

fn ok(v: Json) -> CallToolResult {
    CallToolResult::success(vec![Content::text(v.to_string())])
}

fn fail(errors: Vec<Error>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(json!({ "ok": false, "errors": errors }).to_string())])
}

fn io(path: &str, e: impl std::fmt::Display) -> CallToolResult {
    fail(vec![Error::new("io_error", format!("{path}: {e}"))])
}

pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    s
}

/// Default MCP render scale: largest k ≤ 32 with the longer side ≤ 512.
pub fn auto_scale(s: &Surface) -> u32 {
    (512 / s.width.max(s.height)).clamp(1, 32)
}

/// Opaque bounding box `[x, y, w, h]` (alpha > 0) and distinct colour count.
pub fn stats(s: &Surface) -> (Option<[u32; 4]>, usize) {
    let mut colors = std::collections::HashSet::new();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..s.height {
        for x in 0..s.width {
            let p = s.pixel(x, y);
            colors.insert(p);
            if p[3] > 0 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    let bbox = (x0 != u32::MAX).then(|| [x0, y0, x1 - x0 + 1, y1 - y0 + 1]);
    (bbox, colors.len())
}

/// Create an empty project inside the root (shared by MCP and the server).
pub fn create_project(root: &Path, rel: &str, width: u32, height: u32) -> Result<(), Vec<Error>> {
    let path = sandbox::project(root, rel).map_err(|e| vec![e])?;
    if path.exists() {
        return Err(vec![Error::new("exists", format!("'{rel}' already exists"))]);
    }
    let mut p = Project::default();
    p.settings.width = width as i64;
    p.settings.height = height as i64;
    let errors = project::validate(&p);
    if !errors.is_empty() {
        return Err(errors);
    }
    let io = |e: &dyn std::fmt::Display| vec![Error::new("io_error", format!("{rel}: {e}"))];
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| io(&e))?;
    }
    project::save(&path, &p).map_err(|e| io(&e))
}

impl Slopc {
    pub fn new(root: &Path) -> std::io::Result<Self> {
        let root = root.canonicalize()?;
        let mut ev = Evaluator::new();
        ev.root = Some(root.clone());
        Ok(Slopc { root, evaluator: Arc::new(Mutex::new(ev)), tool_router: Self::tool_router() })
    }

    pub fn with_evaluator(root: PathBuf, evaluator: Arc<Mutex<Evaluator>>) -> Self {
        Slopc { root, evaluator, tool_router: Self::tool_router() }
    }
}

#[tool_router]
impl Slopc {
    #[tool(description = "List *.slopc.json projects under the root.")]
    fn list_projects(&self) -> CallToolResult {
        ok(json!({ "projects": sandbox::list_projects(&self.root) }))
    }

    #[tool(description = "Create an empty project file (no nodes).")]
    fn create_project(&self, Parameters(a): Parameters<CreateArgs>) -> CallToolResult {
        match create_project(&self.root, &a.path, a.width.unwrap_or(32), a.height.unwrap_or(32)) {
            Ok(()) => ok(json!({ "ok": true, "path": a.path })),
            Err(e) => fail(e),
        }
    }

    #[tool(description = "Read a project file and its validation errors.")]
    fn read_project(&self, Parameters(a): Parameters<PathArg>) -> CallToolResult {
        let path = match sandbox::project(&self.root, &a.path) {
            Ok(p) => p,
            Err(e) => return fail(vec![e]),
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => return io(&a.path, e),
        };
        let l = project::load_str(&text);
        let file: Json = serde_json::from_str(&text).unwrap_or(Json::Null);
        ok(json!({ "project": file, "errors": l.errors }))
    }

    #[tool(description = "Apply a batch of edit ops to a project. All-or-nothing; returns created ids and warnings.")]
    fn edit_project(&self, Parameters(a): Parameters<EditArgs>) -> CallToolResult {
        let path = match sandbox::project(&self.root, &a.path) {
            Ok(p) => p,
            Err(e) => return fail(vec![e]),
        };
        let ops = Json::Array(a.ops.into_iter().map(Json::Object).collect());
        match ops::apply_file(&path, &ops) {
            Ok(Ok(applied)) => ok(ops::outcome_json(&Ok(applied))),
            Ok(Err(e)) => fail(e),
            Err(e) => io(&a.path, e),
        }
    }

    #[tool(description = "List node types, or describe one type's inputs and outputs.")]
    fn list_node_types(&self, Parameters(a): Parameters<TypeArg>) -> CallToolResult {
        match a.ty {
            None => ok(Json::Array(
                catalog::all().iter().map(|d| json!({ "type": d.ty, "category": d.category, "description": d.description })).collect(),
            )),
            Some(t) => match catalog::get(&t) {
                Some(d) => ok(catalog::def_json(d)),
                None => {
                    let hint = catalog::suggest(&t);
                    fail(vec![Error::new("unknown_type", format!("unknown node type '{t}'; did you mean {}?", hint.join(", ")))])
                }
            },
        }
    }

    #[tool(description = "Render a node to PNG (upscaled so pixels are visible) with size, opaque bounding box and colour count.")]
    async fn render(&self, Parameters(a): Parameters<RenderArgs>) -> CallToolResult {
        let path = match sandbox::project(&self.root, &a.path) {
            Ok(p) => p,
            Err(e) => return fail(vec![e]),
        };
        let ev = self.evaluator.clone();
        #[allow(clippy::result_large_err)]
        let job = tokio::task::spawn_blocking(move || -> Result<CallToolResult, CallToolResult> {
            let l = project::load(&path).map_err(|e| io(&a.path, e))?;
            let blocking: Vec<Error> = l.blocking().into_iter().cloned().collect();
            let p = l.project.filter(|_| blocking.is_empty()).ok_or_else(|| fail(blocking.clone()))?;
            let dir = path.parent().unwrap();
            let mut ev = ev.lock().unwrap();
            let s = render_surface(&p, dir, a.node.as_deref(), a.frame.unwrap_or(0), 1, &mut ev).map_err(|e| fail(vec![e]))?;
            let scale = a.scale.unwrap_or_else(|| auto_scale(&s)).clamp(1, 32);
            let (bbox, colors) = stats(&s);
            let png = s.upscale(scale).to_png();
            let info = json!({ "width": s.width, "height": s.height, "scale": scale, "opaque_bbox": bbox, "colors": colors });
            Ok(CallToolResult::success(vec![Content::image(base64(&png), "image/png"), Content::text(info.to_string())]))
        });
        match job.await {
            Ok(Ok(r) | Err(r)) => r,
            Err(e) => fail(vec![Error::new("eval_error", e.to_string())]),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Slopc {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(INSTRUCTIONS)
    }
}

/// The tools over Streamable HTTP for `slopc serve` (auth is the server's job).
pub fn http_service(
    root: PathBuf,
    evaluator: Arc<Mutex<Evaluator>>,
) -> rmcp::transport::streamable_http_server::StreamableHttpService<Slopc, rmcp::transport::streamable_http_server::session::local::LocalSessionManager> {
    use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
    // Token auth guards against DNS rebinding, so LAN and remote Host headers are allowed.
    let config = StreamableHttpServerConfig::default().disable_allowed_hosts();
    StreamableHttpService::new(move || Ok(Slopc::with_evaluator(root.clone(), evaluator.clone())), Default::default(), config)
}

/// `slopc mcp`: serve on stdio until the client disconnects.
pub fn run_stdio(root: &Path) -> Result<(), String> {
    let server = Slopc::new(root).map_err(|e| format!("{}: {e}", root.display()))?;
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let service = server.serve(rmcp::transport::stdio()).await.map_err(|e| e.to_string())?;
        service.waiting().await.map_err(|e| e.to_string())?;
        Ok(())
    })
}
