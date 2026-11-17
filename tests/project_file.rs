mod common;

use common::*;
use serde_json::json;
use slopc::project::{self, GInput, SaveError};
use std::time::{Duration, Instant};

const EXAMPLE: &str = r##"{
  "slopc": 1,
  "settings": { "width": 32, "height": 32, "frames": 1, "fps": 12 },
  "nodes": {
    "bg":  { "type": "solid", "inputs": { "color": "#1a1c2c" } },
    "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } }, "pos": [240, 0] }
  }
}"##;

fn one(text: &str, code: &str) {
    assert_eq!(codes(text), vec![code.to_string()], "{text}");
}

#[test]
fn pf_01_minimal_project_loads() {
    let l = project::load_str(r#"{"slopc":1}"#);
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    let p = l.project.unwrap();
    assert_eq!((p.settings.width, p.settings.height, p.settings.frames), (32, 32, 1));
    assert_eq!(p.settings.fps(), 12.0);
    assert!(p.nodes.is_empty());
}

#[test]
fn pf_02_example_loads_with_ref_to_first_output() {
    let l = project::load_str(EXAMPLE);
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    let p = l.project.unwrap();
    assert_eq!(p.nodes.len(), 2);
    let g = project::graph(&p).unwrap();
    assert_eq!(
        g.input("out", "surface_in"),
        Some(&GInput::Ref { node: "bg".into(), output: "surface_out".into() })
    );
}

#[test]
fn pf_03_not_json() {
    one("not json", "parse_error");
}

#[test]
fn pf_04_version_2() {
    one(r#"{"slopc":2}"#, "unsupported_version");
}

#[test]
fn pf_05_version_missing() {
    one("{}", "unsupported_version");
}

#[test]
fn pf_06_unknown_top_level_key() {
    assert!(codes(r#"{"slopc":1,"extra":true}"#).contains(&"unknown_key".into()));
}

#[test]
fn pf_07_unknown_node_key() {
    let e = errors(&project_json(4, 4, json!({ "bg": { "type": "solid", "note": "x" } })));
    assert!(has(&e, "unknown_key", Some("bg"), None), "{e:?}");
}

#[test]
fn pf_08_invalid_ids() {
    let e = errors(&project_json(4, 4, json!({ "Noise": { "type": "solid" }, "1a": { "type": "solid" } })));
    assert_eq!(e.iter().filter(|e| e.code == "invalid_id").count(), 2, "{e:?}");
}

#[test]
fn pf_09_unknown_type_skips_input_checks() {
    let e = errors(&project_json(4, 4, json!({ "x": { "type": "nope", "inputs": { "foo": 1 } } })));
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(has(&e, "unknown_type", Some("x"), None));
}

#[test]
fn pf_10_unknown_input() {
    let e = errors(&project_json(4, 4, json!({ "bg": { "type": "solid", "inputs": { "colour": "#000000" } } })));
    assert!(has(&e, "unknown_input", Some("bg"), Some("colour")), "{e:?}");
}

#[test]
fn pf_11_missing_input() {
    let e = errors(&project_json(4, 4, json!({ "out": { "type": "output" } })));
    assert!(has(&e, "missing_input", Some("out"), Some("surface_in")), "{e:?}");
}

#[test]
fn pf_12_literal_wrong_type() {
    let e = errors(&project_json(4, 4, json!({ "bg": { "type": "solid", "inputs": { "color": 5 } } })));
    assert!(has(&e, "type_mismatch", Some("bg"), Some("color")), "{e:?}");
}

#[test]
fn pf_13_literal_on_surface() {
    let e = errors(&project_json(4, 4, json!({ "out": { "type": "output", "inputs": { "surface_in": "#ffffff" } } })));
    assert!(has(&e, "type_mismatch", Some("out"), Some("surface_in")), "{e:?}");
}

#[test]
fn pf_14_ref_type_mismatch() {
    let e = errors(&project_json(
        4,
        4,
        json!({
            "bg": { "type": "solid" },
            "t": { "type": "transform", "inputs": { "surface_in": { "ref": "bg" }, "position": { "ref": "bg" } } }
        }),
    ));
    assert!(has(&e, "type_mismatch", Some("t"), Some("position")), "{e:?}");
}

#[test]
fn pf_15_out_of_range() {
    let e = errors(&project_json(4, 4, json!({ "b": { "type": "blend", "inputs": { "opacity": 1.5 } } })));
    assert!(has(&e, "out_of_range", Some("b"), Some("opacity")), "{e:?}");
}

#[test]
fn pf_16_settings_out_of_range() {
    for w in [0, 5000] {
        let t = json!({ "slopc": 1, "settings": { "width": w } }).to_string();
        assert!(codes(&t).contains(&"out_of_range".into()), "width {w}");
    }
}

#[test]
fn pf_17_settings_too_large() {
    let t = json!({ "slopc": 1, "settings": { "width": 4096, "height": 4096 } }).to_string();
    assert!(codes(&t).contains(&"too_large".into()));
}

#[test]
fn pf_18_too_many_nodes() {
    let mut nodes = serde_json::Map::new();
    for i in 0..1001 {
        nodes.insert(format!("n{i}"), json!({ "type": "solid" }));
    }
    assert!(codes(&project_json(4, 4, nodes.into())).contains(&"too_large".into()));
}

#[test]
fn pf_19_invalid_enum() {
    let e = errors(&project_json(4, 4, json!({ "b": { "type": "blend", "inputs": { "blend_mode": "darken" } } })));
    assert!(has(&e, "invalid_enum", Some("b"), Some("blend_mode")), "{e:?}");
}

#[test]
fn pf_20_dangling_ref() {
    let e = errors(&project_json(4, 4, json!({ "out": { "type": "output", "inputs": { "surface_in": { "ref": "nose" } } } })));
    assert!(has(&e, "dangling_ref", Some("out"), Some("surface_in")), "{e:?}");
}

#[test]
fn pf_21_unknown_output() {
    let e = errors(&project_json(
        4,
        4,
        json!({ "bg": { "type": "solid" }, "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg.alpha" } } } }),
    ));
    assert!(has(&e, "unknown_output", Some("out"), Some("surface_in")), "{e:?}");
}

#[test]
fn pf_22_cycle_two_nodes() {
    let e = errors(&project_json(
        4,
        4,
        json!({
            "a": { "type": "output", "inputs": { "surface_in": { "ref": "b" } } },
            "b": { "type": "output", "inputs": { "surface_in": { "ref": "a" } } }
        }),
    ));
    assert!(has(&e, "cycle", Some("a"), None) && has(&e, "cycle", Some("b"), None), "{e:?}");
    assert_eq!(e.iter().filter(|e| e.code == "cycle").count(), 2);
}

#[test]
fn pf_23_self_cycle() {
    let e = errors(&project_json(4, 4, json!({ "a": { "type": "output", "inputs": { "surface_in": { "ref": "a" } } } })));
    assert!(has(&e, "cycle", Some("a"), None), "{e:?}");
}

#[test]
fn pf_24_ref_object_extra_key() {
    let e = errors(&project_json(
        4,
        4,
        json!({ "bg": { "type": "solid" }, "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg", "x": 1 } } } }),
    ));
    assert!(has(&e, "unknown_key", Some("out"), Some("surface_in")), "{e:?}");
}

fn pf_25_text() -> String {
    project_json(
        4,
        4,
        json!({
            "bg": { "type": "solid", "inputs": { "colour": "#000000" } },
            "bg2": { "type": "solid", "inputs": { "color": 5 } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "nose" } } }
        }),
    )
}

#[test]
fn pf_25_all_errors_reported() {
    let c = codes(&pf_25_text());
    for code in ["unknown_input", "type_mismatch", "dangling_ref"] {
        assert!(c.contains(&code.into()), "{code} in {c:?}");
    }
}

#[test]
fn pf_26_ref_to_unknown_type() {
    let e = errors(&project_json(
        4,
        4,
        json!({ "x": { "type": "nope" }, "out": { "type": "output", "inputs": { "surface_in": { "ref": "x" } } } }),
    ));
    assert_eq!(e.iter().map(|e| e.code.as_str()).collect::<Vec<_>>(), vec!["unknown_type"], "{e:?}");
}

#[test]
fn pf_27_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    let p = project::load_str(EXAMPLE).project.unwrap();
    project::save(&path, &p).unwrap();
    let bytes1 = std::fs::read_to_string(&path).unwrap();
    assert_eq!(bytes1, p.to_canonical_string());
    let p2 = project::load(&path).unwrap().project.unwrap();
    assert_eq!(p, p2);
    project::save(&path, &p2).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes1);
    assert!(bytes1.ends_with("}\n") && !bytes1.contains('\r'));
}

#[test]
fn pf_28_explicit_default_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    let p = project::load_str(&project_json(4, 4, json!({ "bg": { "type": "solid", "inputs": { "color": "#ffffff" } } })))
        .project
        .unwrap();
    project::save(&path, &p).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("\"color\": \"#ffffff\""));
}

#[test]
fn pf_29_invalid_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    std::fs::write(&path, "original").unwrap();
    let p = project::load_str(&pf_25_text()).project.unwrap();
    match project::save(&path, &p) {
        Err(SaveError::Invalid(e)) => assert!(!e.is_empty()),
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}

#[test]
fn pf_30_save_waits_for_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    let lock = std::fs::File::create(project::lock_path(&path)).unwrap();
    lock.lock().unwrap();
    let start = Instant::now();
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        lock.unlock().unwrap();
    });
    let p = project::load_str(EXAMPLE).project.unwrap();
    project::save(&path, &p).unwrap();
    assert!(start.elapsed() >= Duration::from_millis(250));
    release.join().unwrap();
    assert!(path.exists());
}

#[test]
fn pf_31_failed_rename_keeps_original() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    std::fs::write(&path, "original").unwrap();
    let r = project::write_atomic_with(&path, b"new", |_, _| Err(std::io::Error::other("simulated")));
    assert!(r.is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
}

#[test]
fn pf_32_label_kept_and_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    let with = project_json(4, 4, json!({ "bg": { "type": "solid", "label": "sky gradient", "inputs": { "color": "#ff0000" } } }));
    let p = project::load_str(&with).project.unwrap();
    assert!(project::validate(&p).is_empty());
    project::save(&path, &p).unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("sky gradient"));
    let a = eval(4, 4, json!({ "bg": { "type": "solid", "label": "x", "inputs": { "color": "#ff0000" } } }), "bg").unwrap();
    let b = eval(4, 4, json!({ "bg": { "type": "solid", "inputs": { "color": "#ff0000" } } }), "bg").unwrap();
    assert_eq!(a, b);
}

#[test]
fn pf_33_bad_label() {
    let e = errors(&project_json(4, 4, json!({ "bg": { "type": "solid", "label": 5 } })));
    assert!(has(&e, "type_mismatch", Some("bg"), None), "{e:?}");
    let e = errors(&project_json(4, 4, json!({ "bg": { "type": "solid", "label": "x".repeat(501) } })));
    assert!(has(&e, "out_of_range", Some("bg"), None), "{e:?}");
}

#[test]
fn pf_34_missing_input_can_be_saved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.slopc.json");
    let p = project::load_str(&project_json(4, 4, json!({ "out": { "type": "output" } }))).project.unwrap();
    project::save(&path, &p).unwrap();
    let l = project::load(&path).unwrap();
    assert_eq!(l.errors.iter().map(|e| e.code.as_str()).collect::<Vec<_>>(), vec!["missing_input"]);
}
