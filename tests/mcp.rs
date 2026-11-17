mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::surface::Surface;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start(root: &Path) -> Client {
        let mut child = Command::new(bin())
            .args(["mcp", "--root", root.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client { child, stdin, stdout, next: 1 };
        let init = c.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } }),
        );
        assert!(init.get("result").is_some(), "{init}");
        c.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        c
    }

    fn send(&mut self, v: Json) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Json) -> Json {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(self.stdout.read_line(&mut line).unwrap() > 0, "server closed");
            let v: Json = serde_json::from_str(&line).unwrap();
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    /// Call a tool; returns (is_error, parsed JSON text, image bytes).
    fn call(&mut self, name: &str, args: Json) -> (bool, Json, Option<Vec<u8>>) {
        let r = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let res = &r["result"];
        assert!(res.is_object(), "{r}");
        let mut text = Json::Null;
        let mut image = None;
        for c in res["content"].as_array().unwrap() {
            match c["type"].as_str().unwrap() {
                "text" => text = serde_json::from_str(c["text"].as_str().unwrap()).unwrap(),
                "image" => image = Some(b64(c["data"].as_str().unwrap())),
                _ => {}
            }
        }
        (res["isError"] == json!(true), text, image)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => panic!("bad base64"),
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = vec![];
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= (val(c) as u32) << (18 - 6 * i);
        }
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    out
}

fn solid_and_output() -> Json {
    json!([
        { "op": "add_node", "id": "bg", "type": "solid", "inputs": { "color": "#ff0000" } },
        { "op": "add_node", "id": "out", "type": "output", "inputs": { "surface_in": { "ref": "bg" } } }
    ])
}

#[test]
fn mcp_01_tools_list() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    let r = c.request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, vec!["create_project", "edit_project", "export", "list_node_types", "list_projects", "read_project", "render"]);
    assert!(tools.iter().all(|t| t["inputSchema"].is_object()));
}

#[test]
fn mcp_02_create_and_list() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    let (err, _, _) = c.call("create_project", json!({ "path": "a.slopc.json" }));
    assert!(!err);
    let (_, v, _) = c.call("list_projects", json!({}));
    assert_eq!(v["projects"], json!(["a.slopc.json"]));
}

#[test]
fn mcp_03_create_twice() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json" }));
    let (err, v, _) = c.call("create_project", json!({ "path": "a.slopc.json" }));
    assert!(err);
    assert_eq!(v["errors"][0]["code"], "exists");
}

#[test]
fn mcp_04_edit_and_render() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json", "width": 4, "height": 2 }));
    let (err, v, _) = c.call("edit_project", json!({ "path": "a.slopc.json", "ops": solid_and_output() }));
    assert!(!err, "{v}");
    let (err, v, img) = c.call("render", json!({ "path": "a.slopc.json" }));
    assert!(!err, "{v}");
    let s = Surface::from_png(&img.unwrap()).unwrap();
    let k = v["scale"].as_u64().unwrap() as u32;
    assert_eq!((s.width, s.height), (4 * k, 2 * k));
    assert_eq!((v["width"].as_u64(), v["height"].as_u64()), (Some(4), Some(2)));
    assert_eq!(v["opaque_bbox"], json!([0, 0, 4, 2]));
    assert_eq!(v["colors"], 1);
    assert_all(&s, [255, 0, 0, 255], 0);
}

#[test]
fn mcp_05_default_scale() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json", "width": 8, "height": 8 }));
    c.call("edit_project", json!({ "path": "a.slopc.json", "ops": solid_and_output() }));
    let (_, v, img) = c.call("render", json!({ "path": "a.slopc.json" }));
    assert_eq!(v["scale"], 32);
    let s = Surface::from_png(&img.unwrap()).unwrap();
    assert_eq!((s.width, s.height), (256, 256));
}

#[test]
fn mcp_06_stats() {
    let d = tempfile::tempdir().unwrap();
    let pix = surface(4, 4, |x, y| if (x, y) == (1, 2) { [9, 9, 9, 255] } else { [0, 0, 0, 0] });
    write_png(d.path(), "p.png", &pix);
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json", "width": 4, "height": 4 }));
    c.call(
        "edit_project",
        json!({ "path": "a.slopc.json", "ops": [
            { "op": "add_node", "id": "img", "type": "image", "inputs": { "path": "p.png" } },
            { "op": "add_node", "id": "out", "type": "output", "inputs": { "surface_in": { "ref": "img" } } }
        ] }),
    );
    let (err, v, _) = c.call("render", json!({ "path": "a.slopc.json" }));
    assert!(!err, "{v}");
    assert_eq!(v["opaque_bbox"], json!([1, 2, 1, 1]));
    assert_eq!(v["colors"], 2);
}

#[test]
fn mcp_07_read_project() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json" }));
    c.call("edit_project", json!({ "path": "a.slopc.json", "ops": [{ "op": "add_node", "id": "o", "type": "output" }] }));
    let (err, v, _) = c.call("read_project", json!({ "path": "a.slopc.json" }));
    assert!(!err);
    assert_eq!(v["project"]["nodes"]["o"]["type"], "output");
    assert_eq!(v["errors"][0]["code"], "missing_input");
}

#[test]
fn mcp_08_rejected_edit() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json" }));
    let before = std::fs::read(d.path().join("a.slopc.json")).unwrap();
    let (err, v, _) = c.call("edit_project", json!({ "path": "a.slopc.json", "ops": [{ "op": "remove_node", "id": "x" }] }));
    assert!(err);
    assert_eq!(v["ok"], false);
    assert_eq!(std::fs::read(d.path().join("a.slopc.json")).unwrap(), before);
}

#[test]
fn mcp_09_outside_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    let outside = parent.path().join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    let mut c = Client::start(&root);
    for p in ["../x.slopc.json", "/tmp/x.slopc.json", "link/x.slopc.json"] {
        let (err, v, _) = c.call("create_project", json!({ "path": p }));
        assert!(err, "{p}");
        assert_eq!(v["errors"][0]["code"], "path_outside_root", "{p}: {v}");
    }
    assert!(!outside.join("x.slopc.json").exists());
}

#[test]
fn mcp_10_bad_path() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    let (err, v, _) = c.call("create_project", json!({ "path": "a.json" }));
    assert!(err);
    assert_eq!(v["errors"][0]["code"], "bad_path");
}

#[test]
fn mcp_11_image_outside_root() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("root");
    std::fs::create_dir(&root).unwrap();
    write_png(parent.path(), "secret.png", &surface(1, 1, |_, _| [1, 1, 1, 255]));
    let mut c = Client::start(&root);
    c.call("create_project", json!({ "path": "a.slopc.json", "width": 1, "height": 1 }));
    c.call(
        "edit_project",
        json!({ "path": "a.slopc.json", "ops": [
            { "op": "add_node", "id": "img", "type": "image", "inputs": { "path": "../secret.png" } },
            { "op": "add_node", "id": "out", "type": "output", "inputs": { "surface_in": { "ref": "img" } } }
        ] }),
    );
    let (err, v, _) = c.call("render", json!({ "path": "a.slopc.json" }));
    assert!(err);
    assert_eq!(v["errors"][0]["code"], "eval_error");
}

#[test]
fn mcp_12_node_types() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    let (_, v, _) = c.call("list_node_types", json!({}));
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), slopc::catalog::all().len());
    assert!(arr[0].get("inputs").is_none() && arr[0].get("description").is_some());
    let (_, v, _) = c.call("list_node_types", json!({ "type": "blend" }));
    assert_eq!(v, slopc::catalog::def_json(slopc::catalog::get("blend").unwrap()));
}

#[test]
fn mcp_13_round_trip() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "art/s.slopc.json", "width": 16, "height": 16 }));
    let (err, v, _) = c.call(
        "edit_project",
        json!({ "path": "art/s.slopc.json", "ops": [
            { "op": "add_node", "type": "noise", "inputs": { "color_mode": "hsv" } },
            { "op": "add_node", "type": "transform", "inputs": { "surface_in": { "ref": "noise1" }, "rotation": 90 } },
            { "op": "add_node", "type": "output", "inputs": { "surface_in": { "ref": "transform1" } } }
        ] }),
    );
    assert!(!err, "{v}");
    let (err, v, img) = c.call("render", json!({ "path": "art/s.slopc.json" }));
    assert!(!err, "{v}");
    assert!(!img.unwrap().is_empty());
    let (_, v, _) = c.call("list_projects", json!({}));
    assert_eq!(v["projects"], json!(["art/s.slopc.json"]));
}

#[test]
fn mcp_14_export() {
    let d = tempfile::tempdir().unwrap();
    let mut c = Client::start(d.path());
    c.call("create_project", json!({ "path": "a.slopc.json", "width": 4, "height": 4 }));
    c.call("edit_project", json!({ "path": "a.slopc.json", "ops": [
        { "op": "set_settings", "frames": 3 },
        { "op": "add_node", "id": "s", "type": "solid", "inputs": { "color": { "anim": [{ "time": 0, "value": "#000000" }, { "time": 2, "value": "#ffffff" }] } } },
        { "op": "add_node", "id": "out", "type": "output", "inputs": { "surface_in": { "ref": "s" } } }
    ] }));
    let (err, v, _) = c.call("export", json!({ "path": "a.slopc.json", "out": "exports/a.gif" }));
    assert!(!err, "{v}");
    assert_eq!(v["files"], json!(["exports/a.gif"]));
    assert_eq!(v["frames"], json!([0, 1, 2]));
    assert!(d.path().join("exports/a.gif").is_file());
    let (err, v, _) = c.call("export", json!({ "path": "a.slopc.json", "out": "../a.gif" }));
    assert!(err);
    assert_eq!(v["errors"][0]["code"], "path_outside_root");
}
