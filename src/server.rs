//! `slopc serve`: web UI, JSON API, events and MCP over HTTP (spec/server.md).

use crate::catalog;
use crate::cli::render_surface;
use crate::eval::Evaluator;
use crate::mcp;
use crate::ops;
use crate::project::{self, Error};
use crate::sandbox;
use axum::body::{Body, Bytes};
use axum::extract::{DefaultBodyLimit, Query, Request, State};
use axum::http::{HeaderMap, Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

pub const BODY_LIMIT: usize = 16 << 20;

#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Assets;

#[derive(Clone)]
pub struct AppState {
    root: PathBuf,
    token: Arc<String>,
    evaluator: Arc<Mutex<Evaluator>>,
    changes: broadcast::Sender<PathBuf>,
}

pub struct Options {
    pub root: PathBuf,
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

/// An API error: the errors and the HTTP status their first code maps to.
pub struct ApiError(Vec<Error>);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        ApiError(vec![e])
    }
}

pub fn status_of(code: &str) -> StatusCode {
    match code {
        "bad_path" | "path_outside_root" | "bad_request" => StatusCode::BAD_REQUEST,
        "not_found" => StatusCode::NOT_FOUND,
        "exists" => StatusCode::CONFLICT,
        "gpu_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "io_error" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.0.first().map_or(StatusCode::UNPROCESSABLE_ENTITY, |e| status_of(&e.code));
        (status, Json(json!({ "ok": false, "errors": self.0 }))).into_response()
    }
}

fn io_err(e: impl std::fmt::Display) -> ApiError {
    Error::new("io_error", e.to_string()).into()
}

fn random_token() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("system random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|c| {
        c.trim().strip_prefix("slopc_token=").map(str::to_string)
    })
}

fn authorised(st: &AppState, headers: &HeaderMap) -> bool {
    let bearer = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    bearer.is_some_and(|t| same(t, &st.token)) || cookie_token(headers).is_some_and(|t| same(&t, &st.token))
}

fn set_cookie(token: &str) -> String {
    format!("slopc_token={token}; HttpOnly; SameSite=Strict; Path=/")
}

async fn require_token(State(st): State<AppState>, req: Request, next: Next) -> Response {
    if !authorised(&st, req.headers()) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "ok": false, "errors": [Error::new("unauthorized", "missing or wrong token")] })))
            .into_response();
    }
    next.run(req).await
}

/// Refuse cross-site state-changing requests: `Origin` must match `Host`.
async fn check_origin(req: Request, next: Next) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD
        && let Some(origin) = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok())
    {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        let origin_host = origin.split("://").nth(1).unwrap_or("");
        if origin_host != host {
            return (StatusCode::FORBIDDEN, Json(json!({ "ok": false, "errors": [Error::new("forbidden", "cross-origin request")] })))
                .into_response();
        }
    }
    next.run(req).await
}

#[derive(Deserialize)]
struct PathQuery {
    path: String,
}

#[derive(Deserialize)]
struct FrameQuery {
    path: String,
    frame: Option<i64>,
}

fn existing(st: &AppState, rel: &str) -> Result<PathBuf, ApiError> {
    let path = sandbox::project(&st.root, rel)?;
    if !path.is_file() {
        return Err(Error::new("not_found", format!("no project '{rel}'")).into());
    }
    Ok(path)
}

async fn catalog_h() -> Json<JsonValue> {
    Json(JsonValue::Array(catalog::all().iter().map(catalog::def_json).collect()))
}

async fn projects_h(State(st): State<AppState>) -> Json<JsonValue> {
    Json(json!({ "projects": sandbox::list_projects(&st.root) }))
}

#[derive(Deserialize)]
struct CreateBody {
    path: String,
    width: Option<u32>,
    height: Option<u32>,
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| Error::new("bad_request", format!("invalid JSON body: {e}")).into())
}

async fn create_h(State(st): State<AppState>, body: Bytes) -> Result<Response, ApiError> {
    let b: CreateBody = parse_body(&body)?;
    mcp::create_project(&st.root, &b.path, b.width.unwrap_or(32), b.height.unwrap_or(32)).map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(json!({ "ok": true, "path": b.path }))).into_response())
}

fn keys(path: &Path, p: &project::Project, frame: i64) -> HashMap<String, String> {
    Evaluator::keys(p, path.parent().unwrap(), frame)
}

/// Values of animated inputs at `frame`: `{ node: { input: value } }`.
fn animated(p: &project::Project, frame: i64) -> JsonValue {
    let Ok(g) = project::graph(p) else { return json!({}) };
    let mut out = serde_json::Map::new();
    for (id, n) in &g.nodes {
        let mut m = serde_json::Map::new();
        for (gi, idef) in n.inputs.iter().zip(n.def.inputs) {
            if let project::GInput::Anim(a) = gi {
                m.insert(idef.name.into(), a.value_at(frame, p.settings.frames, idef.ty).to_json(Some(idef)));
            }
        }
        if !m.is_empty() {
            out.insert(id.clone(), JsonValue::Object(m));
        }
    }
    JsonValue::Object(out)
}

async fn project_h(State(st): State<AppState>, Query(q): Query<FrameQuery>) -> Result<Json<JsonValue>, ApiError> {
    let path = existing(&st, &q.path)?;
    let frame = q.frame.unwrap_or(0);
    let text = std::fs::read_to_string(&path).map_err(io_err)?;
    let l = project::load_str(&text);
    let file: JsonValue = serde_json::from_str(&text).unwrap_or(JsonValue::Null);
    let k = l.project.as_ref().map(|p| keys(&path, p, frame)).unwrap_or_default();
    let a = l.project.as_ref().map(|p| animated(p, frame)).unwrap_or(json!({}));
    Ok(Json(json!({ "project": file, "errors": l.errors, "keys": k, "animated": a })))
}

async fn ops_h(State(st): State<AppState>, Query(q): Query<PathQuery>, body: Bytes) -> Result<Json<JsonValue>, ApiError> {
    let path = existing(&st, &q.path)?;
    let ops_json: JsonValue = parse_body(&body)?;
    match tokio::task::spawn_blocking(move || ops::apply_file(&path, &ops_json)).await.map_err(io_err)? {
        Ok(Ok(a)) => {
            let (undo, redo) = (a.undo.clone(), a.redo.clone());
            let mut out = ops::outcome_json(&Ok(a));
            out["undo"] = JsonValue::Array(undo);
            out["redo"] = JsonValue::Array(redo);
            Ok(Json(out))
        }
        Ok(Err(e)) => Err(ApiError(e)),
        Err(e) => Err(io_err(e)),
    }
}

#[derive(Deserialize)]
struct RenderQuery {
    path: String,
    node: Option<String>,
    frame: Option<i64>,
    scale: Option<u32>,
}

async fn render_h(State(st): State<AppState>, Query(q): Query<RenderQuery>) -> Result<Response, ApiError> {
    let path = existing(&st, &q.path)?;
    let scale = q.scale.unwrap_or(1);
    if !(1..=32).contains(&scale) {
        return Err(Error::new("bad_request", "scale must be 1..32").into());
    }
    let ev = st.evaluator.clone();
    let png = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, ApiError> {
        let l = project::load(&path).map_err(io_err)?;
        let blocking: Vec<Error> = l.blocking().into_iter().cloned().collect();
        let p = l.project.filter(|_| blocking.is_empty()).ok_or_else(|| ApiError(blocking.clone()))?;
        let mut ev = ev.lock().unwrap_or_else(|e| e.into_inner());
        let s = render_surface(&p, path.parent().unwrap(), q.node.as_deref(), q.frame.unwrap_or(0), scale, &mut ev)?;
        Ok(s.to_png())
    })
    .await
    .map_err(io_err)??;
    Ok(([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "no-cache")], png).into_response())
}

#[derive(Deserialize)]
struct LoginBody {
    token: String,
}

async fn login_h(State(st): State<AppState>, body: Bytes) -> Response {
    match parse_body::<LoginBody>(&body) {
        Ok(b) if same(&b.token, &st.token) => (StatusCode::NO_CONTENT, [(header::SET_COOKIE, set_cookie(&st.token))]).into_response(),
        Ok(_) => (StatusCode::UNAUTHORIZED, Json(json!({ "ok": false, "errors": [Error::new("unauthorized", "wrong token")] }))).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn upload_h(State(st): State<AppState>, Query(q): Query<PathQuery>, body: Bytes) -> Result<Response, ApiError> {
    if !q.path.to_ascii_lowercase().ends_with(".png") {
        return Err(Error::new("bad_path", format!("'{}' must end in .png", q.path)).into());
    }
    let path = sandbox::resolve(&st.root, &q.path)?;
    crate::surface::Surface::from_png(&body).map_err(|e| Error::new("bad_request", format!("not a usable PNG: {e}")))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io_err)?;
    }
    project::write_atomic(&path, &body).map_err(io_err)?;
    Ok((StatusCode::CREATED, Json(json!({ "ok": true, "path": q.path }))).into_response())
}

fn keys_event(path: &Path, frame: i64) -> Event {
    let k = project::load(path).ok().and_then(|l| l.project).map(|p| keys(path, &p, frame)).unwrap_or_default();
    Event::default().event("changed").data(json!({ "keys": k }).to_string())
}

async fn events_h(State(st): State<AppState>, Query(q): Query<FrameQuery>) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let path = existing(&st, &q.path)?;
    let frame = q.frame.unwrap_or(0);
    let rx = st.changes.subscribe();
    let s = stream::unfold((true, rx, path), move |(first, mut rx, path)| async move {
        if !first {
            loop {
                match rx.recv().await {
                    Ok(p) if p == path => break,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Lagged(_)) => break,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
            // Merge bursts (temp file + rename) into one event.
            tokio::time::sleep(Duration::from_millis(50)).await;
            while rx.try_recv().is_ok() {}
        }
        let ev = keys_event(&path, frame);
        Some((Ok(ev), (false, rx, path)))
    });
    Ok(Sse::new(s).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping")))
}

const NO_UI: &str = "<!doctype html><title>slopc</title><p>This slopc binary was built without the web UI. Build it with <code>cd web && bun run build</code>, then rebuild slopc.</p>";

async fn static_h(State(st): State<AppState>, uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return ApiError(vec![Error::new("not_found", "no such API route")]).into_response();
    }
    if uri.path() == "/"
        && let Some(t) = uri.query().and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")))
        && same(t, &st.token)
    {
        return (StatusCode::SEE_OTHER, [(header::LOCATION, "/".to_string()), (header::SET_COOKIE, set_cookie(&st.token))]).into_response();
    }
    let file = uri.path().trim_start_matches('/');
    let asset = Assets::get(file).or_else(|| Assets::get("index.html")).map(|a| (a, ()));
    match asset {
        Some((a, _)) => ([(header::CONTENT_TYPE, a.metadata.mimetype().to_string())], Body::from(a.data.into_owned())).into_response(),
        None => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], NO_UI).into_response(),
    }
}

pub fn router(st: AppState) -> Router {
    let mcp_service = mcp::http_service(st.root.clone(), st.evaluator.clone());
    let api = Router::new()
        .route("/api/catalog", get(catalog_h))
        .route("/api/projects", get(projects_h).post(create_h))
        .route("/api/project", get(project_h))
        .route("/api/ops", post(ops_h))
        .route("/api/render", get(render_h))
        .route("/api/events", get(events_h))
        .route("/api/upload", post(upload_h))
        .nest_service("/mcp", mcp_service)
        .layer(middleware::from_fn_with_state(st.clone(), require_token));
    Router::new()
        .merge(api)
        .route("/api/login", post(login_h))
        .fallback(static_h)
        .layer(middleware::from_fn(check_origin))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .with_state(st)
}

/// Watch the root and broadcast changed file paths.
fn watch(root: &Path, tx: broadcast::Sender<PathBuf>) -> notify::Result<notify::RecommendedWatcher> {
    use notify::Watcher;
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // Only writes count; inotify also reports reads (Access), which made readers reload in a loop.
        if let Ok(ev) = res
            && matches!(ev.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_))
        {
            for p in ev.paths {
                let _ = tx.send(p);
            }
        }
    })?;
    w.watch(root, notify::RecursiveMode::Recursive)?;
    Ok(w)
}

pub fn run(opts: Options) -> Result<(), String> {
    let root = opts.root.canonicalize().map_err(|e| format!("{}: {e}", opts.root.display()))?;
    let token = opts.token.or_else(|| std::env::var("SLOPC_TOKEN").ok().filter(|t| !t.is_empty())).unwrap_or_else(random_token);
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async move {
        let (tx, _) = broadcast::channel(256);
        let _watcher = watch(&root, tx.clone()).map_err(|e| format!("cannot watch {}: {e}", root.display()))?;
        let mut ev = Evaluator::new();
        ev.root = Some(root.clone());
        let st = AppState { root, token: Arc::new(token.clone()), evaluator: Arc::new(Mutex::new(ev)), changes: tx };
        let listener = tokio::net::TcpListener::bind((opts.host.as_str(), opts.port)).await.map_err(|e| format!("cannot listen: {e}"))?;
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        println!("slopc serve: http://{addr}/?token={token}");
        use std::io::Write;
        std::io::stdout().flush().ok();
        axum::serve(listener, router(st)).await.map_err(|e| e.to_string())
    })
}
