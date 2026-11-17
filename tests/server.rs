mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::surface::Surface;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use ureq::Agent;

pub struct Server {
    child: Child,
    pub url: String,
    pub base: String,
    pub agent: Agent,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

pub fn start(root: &Path, token: Option<&str>, env_token: Option<&str>) -> Server {
    let mut cmd = Command::new(bin());
    cmd.args(["serve", "--port", "0", "--root", root.to_str().unwrap()]).stdout(Stdio::piped()).env_remove("SLOPC_TOKEN");
    if let Some(t) = token {
        cmd.args(["--token", t]);
    }
    if let Some(t) = env_token {
        cmd.env("SLOPC_TOKEN", t);
    }
    let mut child = cmd.spawn().unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let url = line.trim().strip_prefix("slopc serve: ").unwrap_or_else(|| panic!("bad first line: {line}")).to_string();
    let base = url.split("/?").next().unwrap().to_string();
    let agent: Agent = Agent::config_builder().http_status_as_error(false).max_redirects(0).build().into();
    Server { child, url, base, agent }
}

const TOKEN: &str = "test-token";

fn auth() -> String {
    format!("Bearer {TOKEN}")
}

impl Server {
    fn get(&self, path: &str) -> (u16, Vec<u8>) {
        let mut r = self.agent.get(format!("{}{path}", self.base)).header("Authorization", auth()).call().unwrap();
        (r.status().as_u16(), r.body_mut().with_config().limit(64 << 20).read_to_vec().unwrap())
    }
    fn get_json(&self, path: &str) -> (u16, Json) {
        let (s, b) = self.get(path);
        (s, serde_json::from_slice(&b).unwrap_or(Json::Null))
    }
    fn post(&self, path: &str, body: &Json) -> (u16, Json) {
        let mut r = self.agent.post(format!("{}{path}", self.base)).header("Authorization", auth()).send_json(body).unwrap();
        (r.status().as_u16(), serde_json::from_str(&r.body_mut().read_to_string().unwrap()).unwrap_or(Json::Null))
    }
}

fn solid_output() -> Json {
    json!([
        { "op": "add_node", "id": "bg", "type": "solid", "inputs": { "color": "#336699" } },
        { "op": "add_node", "id": "out", "type": "output", "inputs": { "surface_in": { "ref": "bg" } } }
    ])
}

#[test]
fn srv_01_first_line() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let port = s.base.strip_prefix("http://127.0.0.1:").unwrap();
    assert!(port.parse::<u16>().unwrap() > 0);
    assert_eq!(s.url, format!("{}/?token={TOKEN}", s.base));
}

#[test]
fn srv_02_bearer() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let u = format!("{}/api/catalog", s.base);
    assert_eq!(s.agent.get(&u).call().unwrap().status(), 401);
    assert_eq!(s.agent.get(&u).header("Authorization", "Bearer nope").call().unwrap().status(), 401);
    let (st, v) = s.get_json("/api/catalog");
    assert_eq!(st, 200);
    assert_eq!(v.as_array().unwrap().len(), slopc::catalog::all().len());
}

fn cookie_of(r: &ureq::http::Response<ureq::Body>) -> String {
    let c = r.headers().get("set-cookie").expect("set-cookie").to_str().unwrap().to_string();
    assert!(c.contains("HttpOnly") && c.contains("SameSite=Strict"), "{c}");
    c.split(';').next().unwrap().to_string()
}

#[test]
fn srv_03_token_url_sets_cookie() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let r = s.agent.get(&s.url).call().unwrap();
    assert_eq!(r.status(), 303);
    assert_eq!(r.headers().get("location").unwrap(), "/");
    let cookie = cookie_of(&r);
    assert_eq!(cookie, format!("slopc_token={TOKEN}"));
    let r = s.agent.get(format!("{}/api/catalog", s.base)).header("Cookie", &cookie).call().unwrap();
    assert_eq!(r.status(), 200);
}

#[test]
fn srv_04_login() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let u = format!("{}/api/login", s.base);
    let r = s.agent.post(&u).send_json(json!({ "token": TOKEN })).unwrap();
    assert_eq!(r.status(), 204);
    cookie_of(&r);
    assert_eq!(s.agent.post(&u).send_json(json!({ "token": "nope" })).unwrap().status(), 401);
}

#[test]
fn srv_05_origin_check() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let u = format!("{}/api/projects", s.base);
    let post = |origin: Option<&str>, path: &str| {
        let mut req = s.agent.post(&u).header("Authorization", auth());
        if let Some(o) = origin {
            req = req.header("Origin", o);
        }
        req.send_json(json!({ "path": path })).unwrap().status().as_u16()
    };
    assert_eq!(post(Some("http://evil.example"), "a.slopc.json"), 403);
    assert_eq!(post(Some(&s.base), "b.slopc.json"), 201);
    assert_eq!(post(None, "c.slopc.json"), 201);
}

#[test]
fn srv_06_body_limit() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let body = vec![b' '; 17 << 20];
    let r = s.agent.post(format!("{}/api/ops?path=a.slopc.json", s.base)).header("Authorization", auth()).send(&body[..]);
    // The server may reset the connection after answering 413; both count as refusing the body.
    if let Ok(r) = r {
        assert_eq!(r.status(), 413);
    }
}

#[test]
fn srv_07_full_flow() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let (st, _) = s.post("/api/projects", &json!({ "path": "art/a.slopc.json", "width": 4, "height": 3 }));
    assert_eq!(st, 201);
    assert_eq!(s.get_json("/api/projects").1["projects"], json!(["art/a.slopc.json"]));
    let (st, v) = s.post("/api/ops?path=art/a.slopc.json", &solid_output());
    assert_eq!(st, 200, "{v}");
    let (st, v) = s.get_json("/api/project?path=art/a.slopc.json");
    assert_eq!(st, 200);
    assert_eq!(v["project"]["nodes"]["bg"]["type"], "solid");
    assert!(v["keys"]["bg"].is_string() && v["keys"]["out"].is_string(), "{v}");
    let (st, png) = s.get("/api/render?path=art/a.slopc.json");
    assert_eq!(st, 200);
    let o = Command::new(bin()).args(["render", d.path().join("art/a.slopc.json").to_str().unwrap(), "-o", "-"]).output().unwrap();
    assert_eq!(Surface::from_png(&png).unwrap(), Surface::from_png(&o.stdout).unwrap());
    let (_, png2) = s.get("/api/render?path=art/a.slopc.json&node=bg&scale=2");
    assert_eq!(Surface::from_png(&png2).unwrap().width, 8);
}

#[test]
fn srv_08_rejected_ops() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    s.post("/api/projects", &json!({ "path": "a.slopc.json" }));
    let before = std::fs::read(d.path().join("a.slopc.json")).unwrap();
    let (st, v) = s.post("/api/ops?path=a.slopc.json", &json!([{ "op": "remove_node", "id": "zz" }]));
    assert_eq!(st, 422);
    assert_eq!(v["errors"][0]["code"], "unknown_node");
    assert_eq!(std::fs::read(d.path().join("a.slopc.json")).unwrap(), before);
}

#[test]
fn srv_09_outside_root() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    let (st, v) = s.get_json("/api/project?path=../x.slopc.json");
    assert_eq!((st, v["errors"][0]["code"].as_str()), (400, Some("path_outside_root")));
}

#[test]
fn srv_10_missing_project() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    assert_eq!(s.get_json("/api/project?path=nope.slopc.json").1["errors"][0]["code"], "not_found");
    assert_eq!(s.get_json("/api/project?path=nope.slopc.json").0, 404);
    assert_eq!(s.get("/api/render?path=nope.slopc.json").0, 404);
}

#[test]
fn srv_11_no_target() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    s.post("/api/projects", &json!({ "path": "a.slopc.json" }));
    let (st, b) = s.get("/api/render?path=a.slopc.json");
    let v: Json = serde_json::from_slice(&b).unwrap();
    assert_eq!((st, v["errors"][0]["code"].as_str()), (422, Some("no_target")));
}

#[test]
fn srv_12_events_follow_external_edits() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    s.post("/api/projects", &json!({ "path": "a.slopc.json" }));
    s.post(
        "/api/ops?path=a.slopc.json",
        &json!([
            { "op": "add_node", "id": "bg", "type": "solid" },
            { "op": "add_node", "id": "t", "type": "transform", "inputs": { "surface_in": { "ref": "bg" } } }
        ]),
    );
    let r = s.agent.get(format!("{}/api/events?path=a.slopc.json", s.base)).header("Authorization", auth()).call().unwrap();
    assert_eq!(r.status(), 200);
    let (tx, rx) = mpsc::channel::<Json>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(r.into_body().into_reader());
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if let Some(data) = line.trim().strip_prefix("data: ")
                && tx.send(serde_json::from_str(data).unwrap()).is_err()
            {
                break;
            }
            line.clear();
        }
    });
    let first = rx.recv_timeout(Duration::from_secs(5)).expect("first event");
    let start_t = Instant::now();
    let o = Command::new(bin())
        .current_dir(d.path())
        .args(["edit", "a.slopc.json", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            c.stdin.take().unwrap().write_all(br#"[{"op":"set_input","id":"t","input":"rotation","value":90}]"#)?;
            c.wait()
        })
        .unwrap();
    assert!(o.success());
    let mut second;
    loop {
        second = rx.recv_timeout(Duration::from_secs(2)).expect("changed event within 2 s");
        if second["keys"]["t"] != first["keys"]["t"] {
            break;
        }
        assert!(start_t.elapsed() < Duration::from_secs(2));
    }
    assert_eq!(second["keys"]["bg"], first["keys"]["bg"]);
}

#[test]
fn srv_13_static() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), Some(TOKEN), None);
    for p in ["/", "/some/page"] {
        let r = s.agent.get(format!("{}{p}", s.base)).call().unwrap();
        assert_eq!(r.status(), 200, "{p}");
        assert!(r.headers().get("content-type").unwrap().to_str().unwrap().starts_with("text/html"));
    }
}

#[test]
fn srv_14_env_token() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), None, Some("env-token"));
    assert!(s.url.ends_with("?token=env-token"));
    let r = s.agent.get(format!("{}/api/catalog", s.base)).header("Authorization", "Bearer env-token").call().unwrap();
    assert_eq!(r.status(), 200);
}

#[test]
fn srv_15_generated_token() {
    let d = tempfile::tempdir().unwrap();
    let s = start(d.path(), None, None);
    let t = s.url.rsplit("token=").next().unwrap();
    assert!(t.len() == 32 && t.chars().all(|c| c.is_ascii_hexdigit()), "{t}");
}
