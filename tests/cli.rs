mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::surface::Surface;
use std::path::Path;
use std::process::{Command, Output};

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(bin()).current_dir(dir).args(args).output().unwrap()
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn hello_text() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hello.slopc.json")).unwrap()
}

#[test]
fn cli_01_new() {
    let d = tempfile::tempdir().unwrap();
    let o = run(d.path(), &["new", "a.slopc.json", "--width", "16", "--height", "8"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let p = slopc::project::load(&d.path().join("a.slopc.json")).unwrap().project.unwrap();
    assert_eq!((p.settings.width, p.settings.height, p.nodes.len()), (16, 8, 0));
}

#[test]
fn cli_02_new_existing() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", "keep");
    assert_eq!(run(d.path(), &["new", "a.slopc.json"]).status.code(), Some(3));
    assert_eq!(std::fs::read_to_string(d.path().join("a.slopc.json")).unwrap(), "keep");
}

#[test]
fn cli_03_new_bad_extension() {
    let d = tempfile::tempdir().unwrap();
    assert_eq!(run(d.path(), &["new", "a.json"]).status.code(), Some(2));
}

#[test]
fn cli_04_nodes_lists_types() {
    let o = run(Path::new("."), &["nodes"]);
    assert!(o.status.success());
    let s = stdout(&o);
    for t in ["solid", "noise", "transform", "blend", "image", "output"] {
        assert!(s.lines().any(|l| l.split_whitespace().next() == Some(t)), "{t} in\n{s}");
    }
}

#[test]
fn cli_05_nodes_one_json() {
    let o = run(Path::new("."), &["nodes", "blend", "--json"]);
    assert!(o.status.success());
    let v: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v, slopc::catalog::def_json(slopc::catalog::get("blend").unwrap()));
}

#[test]
fn cli_06_nodes_suggests() {
    let o = run(Path::new("."), &["nodes", "blnd"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("blend"));
}

#[test]
fn cli_07_validate_ok() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    let o = run(d.path(), &["validate", "a.slopc.json"]);
    assert_eq!((o.status.code(), stdout(&o).trim()), (Some(0), "ok"));
}

#[test]
fn cli_08_validate_json_errors() {
    let d = tempfile::tempdir().unwrap();
    let text = project_json(
        4,
        4,
        json!({
            "bg": { "type": "solid", "inputs": { "colour": "#000000" } },
            "bg2": { "type": "solid", "inputs": { "color": 5 } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "nose" } } }
        }),
    );
    write(d.path(), "a.slopc.json", &text);
    let o = run(d.path(), &["validate", "a.slopc.json", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    let v: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["errors"].as_array().unwrap().len(), 3);
}

#[test]
fn cli_09_validate_missing_file() {
    assert_eq!(run(Path::new("."), &["validate", "/nonexistent/a.slopc.json"]).status.code(), Some(3));
}

#[test]
fn cli_10_show() {
    let d = tempfile::tempdir().unwrap();
    write(
        d.path(),
        "a.slopc.json",
        &project_json(4, 4, json!({ "bg": { "type": "solid" }, "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } } } })),
    );
    let o = run(d.path(), &["show", "a.slopc.json"]);
    assert!(o.status.success());
    let s = stdout(&o);
    assert!(s.lines().any(|l| l.starts_with("bg") && l.contains("solid")), "{s}");
    assert!(s.lines().any(|l| l.starts_with("out") && l.contains("surface_in=@bg")), "{s}");
}

#[test]
fn cli_11_render_hello_golden() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "hello.slopc.json", &hello_text());
    let o = run(d.path(), &["render", "hello.slopc.json", "-o", "hello.png"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let s = Surface::from_png(&std::fs::read(d.path().join("hello.png")).unwrap()).unwrap();
    assert_golden("hello.png", &s, 1);
}

fn two_outputs() -> String {
    project_json(
        4,
        4,
        json!({
            "bg": { "type": "solid" },
            "o1": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } } },
            "o2": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } } }
        }),
    )
}

#[test]
fn cli_12_ambiguous_target() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &two_outputs());
    let o = run(d.path(), &["render", "a.slopc.json", "-o", "x.png", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).contains("ambiguous_target"));
}

#[test]
fn cli_13_render_node() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &project_json(4, 4, json!({ "bg": { "type": "solid", "inputs": { "color": "#00ff00" } } })));
    let o = run(d.path(), &["render", "a.slopc.json", "-o", "x.png", "--node", "bg"]);
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    let s = Surface::from_png(&std::fs::read(d.path().join("x.png")).unwrap()).unwrap();
    assert_all(&s, [0, 255, 0, 255], 0);
}

#[test]
fn cli_14_scale() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &project_json(4, 4, json!({ "n": { "type": "noise" } })));
    assert!(run(d.path(), &["render", "a.slopc.json", "-o", "x.png", "--node", "n", "--scale", "3"]).status.success());
    let s = Surface::from_png(&std::fs::read(d.path().join("x.png")).unwrap()).unwrap();
    assert_eq!((s.width, s.height), (12, 12));
    for y in 0..12 {
        for x in 0..12 {
            assert_eq!(s.pixel(x, y), s.pixel(x / 3 * 3, y / 3 * 3));
        }
    }
}

#[test]
fn cli_15_bad_scale() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    for k in ["0", "33"] {
        assert_eq!(run(d.path(), &["render", "a.slopc.json", "-o", "x.png", "--scale", k]).status.code(), Some(2), "{k}");
    }
}

#[test]
fn cli_16_stdout_png() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    let o = run(d.path(), &["render", "a.slopc.json", "-o", "-"]);
    assert!(o.status.success());
    assert!(Surface::from_png(&o.stdout).is_ok());
}

#[test]
fn cli_17_gpu_unavailable() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    let o = Command::new(bin())
        .current_dir(d.path())
        .env("SLOPC_ADAPTER", "no-such-gpu")
        .args(["render", "a.slopc.json", "-o", "x.png"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(4));
}

#[test]
fn cli_18_render_invalid() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", r#"{"slopc":1,"nodes":{"o":{"type":"output","inputs":{"surface_in":{"ref":"x"}}}}}"#);
    assert_eq!(run(d.path(), &["render", "a.slopc.json", "-o", "x.png"]).status.code(), Some(1));
    assert!(!d.path().join("x.png").exists());
}

#[test]
fn cli_19_json_everywhere() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    write(d.path(), "bad.slopc.json", "not json");
    let cases: Vec<Vec<&str>> = vec![
        vec!["new", "n.slopc.json", "--json"],
        vec!["new", "a.slopc.json", "--json"],
        vec!["nodes", "--json"],
        vec!["nodes", "blnd", "--json"],
        vec!["show", "a.slopc.json", "--json"],
        vec!["show", "bad.slopc.json", "--json"],
        vec!["validate", "a.slopc.json", "--json"],
        vec!["validate", "bad.slopc.json", "--json"],
        vec!["validate", "missing.slopc.json", "--json"],
        vec!["render", "a.slopc.json", "-o", "x.png", "--json"],
        vec!["render", "bad.slopc.json", "-o", "x.png", "--json"],
    ];
    for args in cases {
        let o = run(d.path(), &args);
        let text = stdout(&o);
        let v: Result<Json, _> = serde_json::from_str(&text);
        assert!(v.is_ok() && text.ends_with('\n') && text.trim().lines().count() >= 1, "{args:?}: {text}");
    }
}

#[test]
fn cli_20_schema() {
    let o = run(Path::new("."), &["schema"]);
    assert!(o.status.success());
    let schema: Json = serde_json::from_slice(&o.stdout).unwrap();
    let v = jsonschema::validator_for(&schema).unwrap();
    let fixtures = [
        hello_text(),
        r#"{"slopc":1}"#.to_string(),
        project_json(4, 4, json!({ "bg": { "type": "solid", "label": "x", "pos": [1, 2] } })),
    ];
    for f in &fixtures {
        assert!(v.is_valid(&serde_json::from_str::<Json>(f).unwrap()), "{f}");
    }
    assert!(!v.is_valid(&json!({ "slopc": 1, "extra": true })));
}

#[test]
fn cli_21_frame_out_of_range() {
    let d = tempfile::tempdir().unwrap();
    write(d.path(), "a.slopc.json", &hello_text());
    let o = run(d.path(), &["render", "a.slopc.json", "-o", "x.png", "--frame", "1", "--json"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stdout(&o).contains("frame_out_of_range"));
}
