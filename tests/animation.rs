mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::eval::Evaluator;
use slopc::ops;
use slopc::project::{self, Project};
use slopc::types::Value;

fn proj(frames: i64, nodes: Json) -> Project {
    let text = json!({ "slopc": 1, "settings": { "width": 2, "height": 2, "frames": frames, "fps": 12 }, "nodes": nodes }).to_string();
    let l = project::load_str(&text);
    assert!(l.blocking().is_empty(), "{:?}", l.errors);
    l.project.unwrap()
}

fn at(p: &Project, target: &str, frame: i64) -> Value {
    Evaluator::new().eval(p, ".".as_ref(), target, frame).unwrap_or_else(|e| panic!("{e:?}"))
}

fn num(v: Value) -> f64 {
    match v {
        Value::Number(x) => x,
        v => panic!("{v:?}"),
    }
}

/// A `number` node whose value is animated.
fn animated_number(frames: i64, anim: Json) -> Project {
    proj(frames, json!({ "v": { "type": "number", "inputs": { "value": anim } } }))
}

fn input_errors(nodes: Json, node: &str, input: &str) -> Vec<String> {
    let text = json!({ "slopc": 1, "nodes": nodes }).to_string();
    project::load_str(&text)
        .errors
        .into_iter()
        .filter(|e| e.node.as_deref() == Some(node) && e.input.as_deref() == Some(input))
        .map(|e| e.code)
        .collect()
}

#[test]
fn an_01_valid_animation() {
    proj(
        24,
        json!({
            "s": { "type": "solid" },
            "t": { "type": "transform", "inputs": { "surface_in": { "ref": "s" }, "rotation": {
                "anim": [{ "time": 0, "value": 0 }, { "time": 12, "value": 90, "ease_in_type": "bezier", "ease_in": [0.5, 1] }],
                "on_end": "hold" } } }
        }),
    );
}

#[test]
fn an_02_type_mismatch() {
    let bad = json!({ "v": { "type": "number", "inputs": { "value": { "anim": [{ "time": 0, "value": "x" }] } } } });
    assert_eq!(input_errors(bad, "v", "value"), vec!["type_mismatch"]);
    let surf = json!({ "t": { "type": "transform", "inputs": { "surface_in": { "anim": [{ "time": 0, "value": 1 }] } } } });
    assert_eq!(input_errors(surf, "t", "surface_in"), vec!["type_mismatch"]);
}

#[test]
fn an_03_out_of_range() {
    for anim in [
        json!({ "anim": [] }),
        json!({ "anim": [{ "time": 3, "value": 1 }, { "time": 3, "value": 2 }] }),
        json!({ "anim": [{ "time": -1, "value": 1 }] }),
    ] {
        let n = json!({ "v": { "type": "number", "inputs": { "value": anim.clone() } } });
        assert_eq!(input_errors(n, "v", "value"), vec!["out_of_range"], "{anim}");
    }
    let n = json!({ "b": { "type": "blend", "inputs": { "opacity": { "anim": [{ "time": 0, "value": 2 }] } } } });
    assert!(input_errors(n, "b", "opacity").contains(&"out_of_range".to_string()));
}

#[test]
fn an_04_invalid_enum() {
    for anim in [
        json!({ "anim": [{ "time": 0, "value": 1 }], "on_end": "bounce" }),
        json!({ "anim": [{ "time": 0, "value": 1, "ease_in_type": "smooth" }] }),
    ] {
        let n = json!({ "v": { "type": "number", "inputs": { "value": anim.clone() } } });
        assert_eq!(input_errors(n, "v", "value"), vec!["invalid_enum"], "{anim}");
    }
}

#[test]
fn an_05_linear() {
    let p = animated_number(20, json!({ "anim": [{ "time": 0, "value": 0 }, { "time": 10, "value": 100 }] }));
    let got: Vec<f64> = [0, 5, 10, 15].iter().map(|&f| num(at(&p, "v", f))).collect();
    assert_eq!(got, vec![0.0, 50.0, 100.0, 100.0]);
}

#[test]
fn an_06_before_first_and_single_key() {
    let p = animated_number(20, json!({ "anim": [{ "time": 5, "value": 1 }, { "time": 8, "value": 2 }] }));
    assert_eq!(num(at(&p, "v", 0)), 1.0);
    let p = animated_number(20, json!({ "anim": [{ "time": 3, "value": 7 }] }));
    for f in [0, 3, 19] {
        assert_eq!(num(at(&p, "v", f)), 7.0);
    }
}

#[test]
fn an_07_cut() {
    let p = animated_number(20, json!({ "anim": [{ "time": 0, "value": 0 }, { "time": 10, "value": 100, "ease_in_type": "cut" }] }));
    assert_eq!(num(at(&p, "v", 5)), 0.0);
    let p = animated_number(20, json!({ "anim": [{ "time": 0, "value": 0, "ease_out_type": "cut" }, { "time": 10, "value": 100 }] }));
    assert_eq!(num(at(&p, "v", 5)), 100.0);
}

#[test]
fn an_08_bezier_ease() {
    let e = |r: f64| slopc::anim::bezier_ease([0.5, 0.0], [0.5, 1.0], r);
    assert!((e(0.5) - 0.5).abs() < 1e-4, "{}", e(0.5));
    assert!(e(0.25) < 0.25);
    assert!((e(0.25) + e(0.75) - 1.0).abs() < 1e-4);
    let samples: Vec<f64> = (0..=10).map(|i| e(i as f64 / 10.0)).collect();
    assert!(samples.windows(2).all(|w| w[1] >= w[0]), "{samples:?}");
}

#[test]
fn an_09_on_end_modes() {
    let keys = json!([{ "time": 2, "value": 0 }, { "time": 6, "value": 40 }]);
    let at_mode = |mode: &str, f: i64| num(at(&animated_number(20, json!({ "anim": keys, "on_end": mode })), "v", f));
    assert_eq!((at_mode("loop", 7), at_mode("loop", 11)), (10.0, 0.0));
    assert_eq!((at_mode("ping", 8), at_mode("ping", 10)), (20.0, 0.0));
    assert!((at_mode("wrap", 13) - 22.5).abs() < 1e-9);
    assert!((at_mode("wrap", 1) - 2.5).abs() < 1e-9);
}

#[test]
fn an_10_types() {
    let p = proj(3, json!({ "n": { "type": "noise", "inputs": { "seed": { "anim": [{ "time": 0, "value": 0 }, { "time": 2, "value": 3 }] } } } }));
    let g = project::graph(&p).unwrap();
    assert_eq!(g.input_at("n", "seed", 1, 3), Some(Value::Int(2)));
    let p = proj(3, json!({ "t": { "type": "transform", "inputs": {
        "position": { "anim": [{ "time": 0, "value": [0, 0] }, { "time": 2, "value": [10, 20] }] } } } }));
    assert_eq!(project::graph(&p).unwrap().input_at("t", "position", 1, 3), Some(Value::Vec2([5.0, 10.0])));
    let p = proj(3, json!({ "s": { "type": "solid", "inputs": {
        "color": { "anim": [{ "time": 0, "value": "#000000" }, { "time": 2, "value": "#ff0000" }] } } } }));
    assert_eq!(project::graph(&p).unwrap().input_at("s", "color", 1, 3), Some(Value::Color([128, 0, 0, 255])));
    let p = proj(3, json!({ "b": { "type": "blend", "inputs": {
        "blend_mode": { "anim": [{ "time": 0, "value": "normal" }, { "time": 2, "value": "add" }] } } } }));
    assert_eq!(project::graph(&p).unwrap().input_at("b", "blend_mode", 1, 3), Some(Value::Enum(0)));
}

#[test]
fn an_11_only_animated_nodes_rerun() {
    let p = proj(
        4,
        json!({
            "a": { "type": "solid", "inputs": { "color": { "anim": [{ "time": 0, "value": "#000000" }, { "time": 3, "value": "#ffffff" }] } } },
            "t": { "type": "transform", "inputs": { "surface_in": { "ref": "a" } } },
            "s": { "type": "solid" },
            "b": { "type": "blend", "inputs": { "background": { "ref": "s" }, "foreground": { "ref": "t" } } }
        }),
    );
    let mut ev = Evaluator::new();
    ev.eval(&p, ".".as_ref(), "b", 0).unwrap();
    ev.eval(&p, ".".as_ref(), "b", 1).unwrap();
    let mut runs = ev.last_runs().to_vec();
    runs.sort();
    assert_eq!(runs, vec!["a", "b", "t"]);
}

fn ok(p: &Project, o: Json) -> Project {
    ops::apply(p, &o).unwrap_or_else(|e| panic!("{e:?}")).project
}

#[test]
fn an_12_key_ops() {
    let p = proj(10, json!({ "v": { "type": "number" }, "w": { "type": "number" }, "b": { "type": "blend", "inputs": { "opacity": { "ref": "v" } } } }));
    let p = ok(&p, json!([{ "op": "set_key", "id": "v", "input": "value", "time": 2, "value": 1 }]));
    assert_eq!(p.nodes["v"].inputs["value"], json!({ "anim": [{ "time": 2, "value": 1 }] }));
    let p = ok(&p, json!([{ "op": "set_key", "id": "v", "input": "value", "time": 2, "value": 5 }]));
    assert_eq!(p.nodes["v"].inputs["value"]["anim"], json!([{ "time": 2, "value": 5 }]));
    let p = ok(&p, json!([{ "op": "set_key", "id": "v", "input": "value", "time": 6, "value": 9, "ease_in_type": "bezier" }]));
    assert_eq!(p.nodes["v"].inputs["value"]["anim"].as_array().unwrap().len(), 2);
    let p = ok(&p, json!([{ "op": "remove_key", "id": "v", "input": "value", "time": 2 }]));
    assert_eq!(p.nodes["v"].inputs["value"]["anim"].as_array().unwrap().len(), 1);
    let p = ok(&p, json!([{ "op": "remove_key", "id": "v", "input": "value", "time": 6 }]));
    assert_eq!(p.nodes["v"].inputs["value"], json!(9));
    let e = ops::apply(&p, &json!([{ "op": "set_key", "id": "b", "input": "opacity", "time": 0, "value": 1 }])).err().unwrap();
    assert_eq!(e[0].code, "bad_op");
}

/// A solid whose colour is cut-keyed so frame `i` is grey `i * 20`.
fn frame_coded(frames: i64) -> Json {
    let keys: Vec<Json> = (0..frames).map(|i| json!({ "time": i, "value": format!("#{0:02x}{0:02x}{0:02x}", i * 20), "ease_out_type": "cut" })).collect();
    json!({ "type": "solid", "inputs": { "color": { "anim": keys } } })
}

fn grey_at(p: &Project, target: &str, f: i64) -> Option<u8> {
    let s = at(p, target, f).into_surface().unwrap();
    let px = s.pixel(0, 0);
    if px[3] == 0 { None } else { Some(px[0] / 20) }
}

#[test]
fn an_13_render_frames() {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("a.slopc.json");
    std::fs::write(&path, json!({ "slopc": 1, "settings": { "width": 2, "height": 2, "frames": 5 }, "nodes": { "src": frame_coded(5) } }).to_string()).unwrap();
    for f in 0..5 {
        let o = std::process::Command::new(bin())
            .args(["render", path.to_str().unwrap(), "-o", "-", "--node", "src", "--frame", &f.to_string()])
            .output()
            .unwrap();
        assert!(o.status.success());
        let s = slopc::surface::Surface::from_png(&o.stdout).unwrap();
        assert_eq!(s.pixel(0, 0)[0], (f * 20) as u8);
    }
}

#[test]
fn an_n1_wiggler() {
    let mk = |seed: i64| proj(24, json!({ "w": { "type": "wiggler", "inputs": { "seed": seed, "range": [2, 5] } } }));
    let a: Vec<f64> = (0..24).map(|f| num(at(&mk(1), "w", f))).collect();
    let b: Vec<f64> = (0..24).map(|f| num(at(&mk(1), "w", f))).collect();
    assert_eq!(a, b);
    assert_eq!(a[0], 3.5);
    assert!(a.iter().all(|v| (2.0..=5.0).contains(v)), "{a:?}");
    assert!(a.iter().any(|v| (v - 3.5).abs() > 0.01));
}

#[test]
fn an_n2_functions() {
    let p = proj(20, json!({ "e": { "type": "fn_ease" } }));
    let got: Vec<f64> = [0, 1, 10, 19].iter().map(|&f| num(at(&p, "e", f))).collect();
    for (g, e) in got.iter().zip([0.0, 0.5, 1.0, 0.5]) {
        assert!((g - e).abs() < 1e-9, "{got:?}");
    }
    for (ty, e) in [("cubic_poly", 0.15625), ("quadratic_rat", 0.1), ("cubic_rat", 0.0357143), ("cosine", 0.1464466)] {
        let p = proj(1, json!({ "s": { "type": "fn_smoothstep", "inputs": { "value": 0.25, "type": ty } } }));
        assert!((num(at(&p, "s", 0)) - e).abs() < 1e-6, "{ty}");
    }
    let p = proj(1, json!({ "m": { "type": "fn_math", "inputs": { "value_1": 3, "value_2": 4 } } }));
    assert_eq!(num(at(&p, "m", 0)), 12.0);
}

fn remap(frames: i64, ty: &str, input: &str, inputs: Json) -> Project {
    let mut inputs = inputs;
    inputs[input] = json!({ "ref": "src" });
    proj(frames, json!({ "src": frame_coded(frames), "r": { "type": ty, "inputs": inputs } }))
}

#[test]
fn an_n3_revert() {
    let p = remap(8, "revert", "surface_in", json!({}));
    assert_eq!((grey_at(&p, "r", 0), grey_at(&p, "r", 5)), (Some(7), Some(2)));
}

#[test]
fn an_n4_anim_speed() {
    let p = remap(8, "anim_speed", "surface_in", json!({ "speed": 2 }));
    assert_eq!((grey_at(&p, "r", 3), grey_at(&p, "r", 5)), (Some(6), Some(2)));
    let p = remap(8, "anim_speed", "surface_in", json!({ "speed": 2, "overflow": "empty" }));
    assert_eq!(grey_at(&p, "r", 5), None);
}

#[test]
fn an_n5_rate_remap() {
    let p = remap(12, "rate_remap", "surface", json!({ "framerate": 4 }));
    let got: Vec<_> = [0, 2, 3, 7].iter().map(|&f| grey_at(&p, "r", f)).collect();
    assert_eq!(got, vec![Some(0), Some(0), Some(3), Some(6)]);
}

#[test]
fn an_n6_delay() {
    let p = remap(8, "delay", "surface", json!({ "frames": 2 }));
    assert_eq!((grey_at(&p, "r", 0), grey_at(&p, "r", 5)), (Some(0), Some(3)));
    let p = remap(8, "delay", "surface", json!({ "frames": 2, "overflow": "loop" }));
    assert_eq!(grey_at(&p, "r", 1), Some(7));
    let p = remap(8, "delay", "surface", json!({ "frames": 2, "overflow": "clear" }));
    assert_eq!(grey_at(&p, "r", 1), None);
}

#[test]
fn an_n7_anim_loop() {
    let p = remap(12, "anim_loop", "surface", json!({ "loop_start": 2, "loop_range": 3 }));
    let got: Vec<_> = [0, 1, 4, 5, 9].iter().map(|&f| grey_at(&p, "r", f)).collect();
    assert_eq!(got, vec![Some(0), Some(1), Some(1), Some(2), Some(3)]);
    let p = remap(12, "anim_loop", "surface", json!({ "loop_start": 2, "loop_range": 3, "loop_type": "pingpong" }));
    let got: Vec<_> = [4, 5, 6].iter().map(|&f| grey_at(&p, "r", f)).collect();
    assert_eq!(got, vec![Some(3), Some(2), Some(1)]);
}
