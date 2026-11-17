mod common;

use common::*;
use serde_json::json;
use slopc::eval::Evaluator;
use slopc::project;
use std::process::Command;

fn load(text: &str) -> project::Project {
    project::load_str(text).project.unwrap()
}

#[test]
fn ev_01_target_node() {
    let s = eval(3, 2, json!({ "bg": { "type": "solid", "inputs": { "color": "#102030" } } }), "bg").unwrap();
    assert_eq!((s.width, s.height), (3, 2));
    assert_all(&s, [0x10, 0x20, 0x30, 255], 0);
}

#[test]
fn ev_02_unknown_output() {
    let e = eval(2, 2, json!({ "bg": { "type": "solid" } }), "bg.nope").unwrap_err();
    assert_eq!(e.code, "unknown_output");
}

#[test]
fn ev_03_unknown_node() {
    let e = eval(2, 2, json!({ "bg": { "type": "solid" } }), "nope").unwrap_err();
    assert_eq!(e.code, "dangling_ref");
}

#[test]
fn ev_04_frame_out_of_range() {
    let p = load(&project_json(2, 2, json!({ "bg": { "type": "solid" } })));
    for f in [1, -1] {
        let e = Evaluator::new().eval(&p, ".".as_ref(), "bg", f).unwrap_err();
        assert_eq!(e.code, "frame_out_of_range", "frame {f}");
    }
}

#[test]
fn ev_05_unrelated_failing_node_ignored() {
    let s = eval(
        2,
        2,
        json!({ "bg": { "type": "solid" }, "img": { "type": "image", "inputs": { "path": "missing.png" } } }),
        "bg",
    );
    assert!(s.is_ok());
}

#[test]
fn ev_06_failing_dependency() {
    let e = eval(
        2,
        2,
        json!({
            "img": { "type": "image", "inputs": { "path": "missing.png" } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "img" } } }
        }),
        "out",
    )
    .unwrap_err();
    assert_eq!((e.code.as_str(), e.node.as_deref()), ("eval_error", Some("img")));
}

fn chain(b_x: f64) -> project::Project {
    load(&project_json(
        4,
        4,
        json!({
            "a": { "type": "solid", "inputs": { "color": "#ff0000" } },
            "b": { "type": "transform", "inputs": { "surface_in": { "ref": "a" }, "position": [b_x, 0.5] } },
            "c": { "type": "transform", "inputs": { "surface_in": { "ref": "b" } } }
        }),
    ))
}

#[test]
fn ev_07_cache_hit() {
    let p = chain(0.5);
    let mut ev = Evaluator::new();
    ev.eval(&p, ".".as_ref(), "c", 0).unwrap();
    assert_eq!(ev.last_runs().len(), 3);
    ev.eval(&p, ".".as_ref(), "c", 0).unwrap();
    assert!(ev.last_runs().is_empty(), "{:?}", ev.last_runs());
}

#[test]
fn ev_08_only_changed_and_dependents_rerun() {
    let mut ev = Evaluator::new();
    ev.eval(&chain(0.5), ".".as_ref(), "c", 0).unwrap();
    ev.eval(&chain(0.75), ".".as_ref(), "c", 0).unwrap();
    let mut runs = ev.last_runs().to_vec();
    runs.sort();
    assert_eq!(runs, vec!["b", "c"]);
}

#[test]
fn ev_09_file_change_reruns() {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "a.png", &surface(2, 2, |_, _| [1, 2, 3, 255]));
    let p = load(&project_json(
        2,
        2,
        json!({
            "img": { "type": "image", "inputs": { "path": "a.png" } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "img" } } }
        }),
    ));
    let mut ev = Evaluator::new();
    ev.eval(&p, dir.path(), "out", 0).unwrap();
    write_png(dir.path(), "a.png", &surface(2, 2, |_, _| [9, 9, 9, 255]));
    let v = ev.eval(&p, dir.path(), "out", 0).unwrap();
    let mut runs = ev.last_runs().to_vec();
    runs.sort();
    assert_eq!(runs, vec!["img", "out"]);
    assert_eq!(v.into_surface().unwrap().pixel(0, 0), [9, 9, 9, 255]);
}

#[test]
fn ev_11_unconnected_node_elsewhere_ok() {
    let s = eval(2, 2, json!({ "bg": { "type": "solid" }, "b": { "type": "blend" } }), "bg");
    assert!(s.is_ok(), "{:?}", s.err());
}

#[test]
fn ev_12_missing_input_in_dependencies() {
    let e = eval(
        2,
        2,
        json!({ "b": { "type": "blend" }, "out": { "type": "output", "inputs": { "surface_in": { "ref": "b" } } } }),
        "out",
    )
    .unwrap_err();
    assert_eq!((e.code.as_str(), e.node.as_deref()), ("missing_input", Some("b")));
}

fn write_project(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("p.slopc.json");
    std::fs::write(
        &path,
        project_json(
            8,
            8,
            json!({
                "n": { "type": "noise", "inputs": { "seed": 3, "color_mode": "rgb" } },
                "out": { "type": "output", "inputs": { "surface_in": { "ref": "n" } } }
            }),
        ),
    )
    .unwrap();
    path
}

#[test]
fn gpu_01_no_adapter() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_project(dir.path());
    let out = Command::new(bin())
        .env("SLOPC_ADAPTER", "no-such-gpu")
        .args(["render", path.to_str().unwrap(), "-o", dir.path().join("o.png").to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&out.stdout).contains("gpu_unavailable"));
}

#[test]
fn gpu_02_validate_never_opens_gpu() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_project(dir.path());
    let out = Command::new(bin()).env("SLOPC_ADAPTER", "no-such-gpu").args(["validate", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn gpu_03_upload_readback() {
    let s = surface(3, 2, |x, y| [x as u8 * 50 + 1, y as u8 * 90 + 2, 200, if x == 1 { 0 } else { 255 }]);
    let back = slopc::gpu::Gpu::get().unwrap().passthrough(&s);
    assert_eq!(back, s);
}

#[test]
fn gpu_04_deterministic_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_project(dir.path());
    let render = |name: &str| {
        let o = dir.path().join(name);
        let st = Command::new(bin()).args(["render", path.to_str().unwrap(), "-o", o.to_str().unwrap()]).status().unwrap();
        assert!(st.success());
        std::fs::read(o).unwrap()
    };
    assert_eq!(render("a.png"), render("b.png"));
}

#[test]
fn ev_13_frame_count_in_key() {
    let mk = |frames: i64| {
        load(&json!({ "slopc": 1, "settings": { "frames": frames }, "nodes": { "c": { "type": "counter", "inputs": { "mode": "animation_progress" } } } }).to_string())
    };
    let mut ev = Evaluator::new();
    assert_eq!(ev.eval(&mk(10), ".".as_ref(), "c", 9).unwrap(), slopc::types::Value::Number(1.0));
    assert_eq!(ev.eval(&mk(19), ".".as_ref(), "c", 9).unwrap(), slopc::types::Value::Number(0.5));
}
