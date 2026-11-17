mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::ops;
use slopc::project::{self, Project};
use std::io::Write;
use std::process::{Command, Stdio};

fn base() -> Project {
    project::load_str(&project_json(
        4,
        4,
        json!({
            "bg": { "type": "solid" },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "bg" } } },
            "out2": { "type": "output", "inputs": { "surface_in": { "ref": "bg.surface_out" } } }
        }),
    ))
    .project
    .unwrap()
}

fn ok(p: &Project, ops_: Json) -> ops::Applied {
    ops::apply(p, &ops_).unwrap_or_else(|e| panic!("rejected: {e:?}"))
}

fn rejected(p: &Project, ops_: Json, code: &str) -> Vec<project::Error> {
    let e = ops::apply(p, &ops_).err().unwrap_or_else(|| panic!("accepted: {ops_}"));
    assert!(e.iter().any(|e| e.code == code), "{code} not in {e:?}");
    e
}

#[test]
fn op_01_generated_ids() {
    let a = ok(&Project::default(), json!([{ "op": "add_node", "type": "solid" }, { "op": "add_node", "type": "solid" }]));
    assert_eq!(a.created, vec!["solid1", "solid2"]);
    assert!(a.project.nodes.contains_key("solid1") && a.project.nodes.contains_key("solid2"));
}

#[test]
fn op_02_add_full() {
    let a = ok(
        &Project::default(),
        json!([{ "op": "add_node", "id": "bg", "type": "solid", "inputs": { "color": "#ff0000" }, "pos": [1, 2], "label": "x" }]),
    );
    let n = &a.project.nodes["bg"];
    assert_eq!((n.ty.as_str(), n.label.as_deref()), ("solid", Some("x")));
    assert_eq!(n.inputs["color"], json!("#ff0000"));
    assert_eq!(json!(n.pos), json!([1, 2]));
}

#[test]
fn op_03_duplicate_id() {
    let e = rejected(&base(), json!([{ "op": "add_node", "id": "bg", "type": "solid" }]), "duplicate_id");
    assert_eq!(e[0].op, Some(0));
}

#[test]
fn op_04_unknown_type() {
    rejected(&base(), json!([{ "op": "add_node", "type": "nope" }]), "unknown_type");
}

#[test]
fn op_05_invalid_id() {
    rejected(&base(), json!([{ "op": "add_node", "id": "Bad", "type": "solid" }]), "invalid_id");
}

#[test]
fn op_06_remove_disconnects() {
    let a = ok(&base(), json!([{ "op": "remove_node", "id": "bg" }]));
    assert!(!a.project.nodes.contains_key("bg"));
    assert!(a.project.nodes["out"].inputs.is_empty());
    assert!(a.warnings.iter().any(|w| w.code == "missing_input" && w.node.as_deref() == Some("out")));
}

#[test]
fn op_07_rename_rewrites_refs() {
    let a = ok(&base(), json!([{ "op": "rename_node", "id": "bg", "new_id": "sky" }]));
    assert_eq!(a.project.nodes.keys().collect::<Vec<_>>(), vec!["sky", "out", "out2"]);
    assert_eq!(a.project.nodes["out"].inputs["surface_in"], json!({ "ref": "sky" }));
    assert_eq!(a.project.nodes["out2"].inputs["surface_in"], json!({ "ref": "sky.surface_out" }));
}

#[test]
fn op_08_rename_to_existing() {
    rejected(&base(), json!([{ "op": "rename_node", "id": "bg", "new_id": "out" }]), "duplicate_id");
}

#[test]
fn op_09_move() {
    let a = ok(&base(), json!([{ "op": "move_node", "id": "bg", "pos": [10, 20] }]));
    assert_eq!(json!(a.project.nodes["bg"].pos), json!([10, 20]));
}

#[test]
fn op_10_set_input() {
    let a = ok(&base(), json!([{ "op": "set_input", "id": "bg", "input": "color", "value": "#00ff00" }]));
    assert_eq!(a.project.nodes["bg"].inputs["color"], json!("#00ff00"));
}

#[test]
fn op_11_set_input_bad_type() {
    let e = rejected(&base(), json!([{ "op": "set_input", "id": "bg", "input": "color", "value": 5 }]), "type_mismatch");
    assert!(e.iter().any(|e| e.node.as_deref() == Some("bg") && e.input.as_deref() == Some("color")));
}

#[test]
fn op_12_set_ref() {
    let p = ok(&base(), json!([{ "op": "add_node", "id": "o3", "type": "output" }])).project;
    let a = ok(&p, json!([{ "op": "set_input", "id": "o3", "input": "surface_in", "value": { "ref": "bg" } }]));
    assert_eq!(a.project.nodes["o3"].inputs["surface_in"], json!({ "ref": "bg" }));
}

#[test]
fn op_13_unknown_input() {
    rejected(&base(), json!([{ "op": "set_input", "id": "bg", "input": "nope", "value": 1 }]), "unknown_input");
}

#[test]
fn op_14_unset_input() {
    let p = ok(&base(), json!([{ "op": "set_input", "id": "bg", "input": "color", "value": "#00ff00" }])).project;
    let a = ok(&p, json!([{ "op": "unset_input", "id": "bg", "input": "color" }, { "op": "unset_input", "id": "bg", "input": "color" }]));
    assert!(!a.project.nodes["bg"].inputs.contains_key("color"));
}

#[test]
fn op_15_label() {
    let p = ok(&base(), json!([{ "op": "set_label", "id": "bg", "label": "sky" }])).project;
    assert_eq!(p.nodes["bg"].label.as_deref(), Some("sky"));
    let p = ok(&p, json!([{ "op": "set_label", "id": "bg", "label": null }])).project;
    assert_eq!(p.nodes["bg"].label, None);
}

#[test]
fn op_16_settings() {
    let a = ok(&base(), json!([{ "op": "set_settings", "width": 64 }]));
    assert_eq!((a.project.settings.width, a.project.settings.height), (64, 4));
}

#[test]
fn op_17_settings_out_of_range() {
    rejected(&base(), json!([{ "op": "set_settings", "width": 0 }]), "out_of_range");
}

#[test]
fn op_18_bad_ops() {
    for op in [json!({ "op": "explode" }), json!({ "op": "move_node" }), json!({ "op": "move_node", "id": "bg", "pos": [1, 2], "x": 1 })] {
        rejected(&base(), json!([op]), "bad_op");
    }
}

#[test]
fn op_19_batch_atomic() {
    let p = base();
    let e = rejected(
        &p,
        json!([{ "op": "add_node", "id": "n1", "type": "solid" }, { "op": "add_node", "id": "n1", "type": "solid" }]),
        "duplicate_id",
    );
    assert_eq!(e[0].op, Some(1));
    assert_eq!(p, base());
}

#[test]
fn op_20_cycle() {
    rejected(
        &base(),
        json!([
            { "op": "add_node", "id": "a", "type": "output", "inputs": { "surface_in": { "ref": "b" } } },
            { "op": "add_node", "id": "b", "type": "output", "inputs": { "surface_in": { "ref": "a" } } }
        ]),
        "cycle",
    );
}

#[test]
fn op_21_unknown_node() {
    for op in [
        json!({ "op": "remove_node", "id": "zz" }),
        json!({ "op": "move_node", "id": "zz", "pos": [0, 0] }),
        json!({ "op": "set_input", "id": "zz", "input": "color", "value": "#000000" }),
    ] {
        rejected(&base(), json!([op]), "unknown_node");
    }
}

#[test]
fn op_22_missing_input_warning() {
    let a = ok(&base(), json!([{ "op": "add_node", "id": "b", "type": "blend" }]));
    assert!(a.warnings.iter().any(|w| w.code == "missing_input" && w.input.as_deref() == Some("background")));
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf) {
    let d = tempfile::tempdir().unwrap();
    let path = d.path().join("a.slopc.json");
    project::save(&path, &base()).unwrap();
    (d, path)
}

#[test]
fn op_23_cli_edit() {
    let (d, path) = setup();
    std::fs::write(d.path().join("ops.json"), r##"[{"op":"set_input","id":"bg","input":"color","value":"#123456"}]"##).unwrap();
    let o = Command::new(bin()).current_dir(d.path()).args(["edit", "a.slopc.json", "ops.json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert_eq!(project::load(&path).unwrap().project.unwrap().nodes["bg"].inputs["color"], json!("#123456"));
}

#[test]
fn op_24_cli_edit_rejected() {
    let (d, path) = setup();
    let before = std::fs::read(&path).unwrap();
    std::fs::write(d.path().join("ops.json"), r#"[{"op":"remove_node","id":"zz"}]"#).unwrap();
    let o = Command::new(bin()).current_dir(d.path()).args(["edit", "a.slopc.json", "ops.json", "--json"]).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    let v: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert!(!v["errors"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

fn edit_stdin(dir: &std::path::Path, input: &str) -> std::process::Output {
    let mut child = Command::new(bin())
        .current_dir(dir)
        .args(["edit", "a.slopc.json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn op_25_cli_stdin() {
    let (d, path) = setup();
    let o = edit_stdin(d.path(), r#"[{"op":"add_node","id":"n","type":"noise"}]"#);
    assert!(o.status.success(), "{o:?}");
    assert!(project::load(&path).unwrap().project.unwrap().nodes.contains_key("n"));
}

#[test]
fn op_26_single_op_object() {
    let (d, path) = setup();
    let o = edit_stdin(d.path(), r#"{"op":"add_node","id":"n","type":"noise"}"#);
    assert!(o.status.success(), "{o:?}");
    assert!(project::load(&path).unwrap().project.unwrap().nodes.contains_key("n"));
}

#[test]
fn op_27_concurrent_edits() {
    let (d, path) = setup();
    let children: Vec<_> = (0..8)
        .map(|i| {
            let mut c = Command::new(bin())
                .current_dir(d.path())
                .args(["edit", "a.slopc.json", "-"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .spawn()
                .unwrap();
            c.stdin.take().unwrap().write_all(format!(r#"{{"op":"add_node","id":"c{i}","type":"solid"}}"#).as_bytes()).unwrap();
            c
        })
        .collect();
    for mut c in children {
        assert!(c.wait().unwrap().success());
    }
    let p = project::load(&path).unwrap().project.unwrap();
    for i in 0..8 {
        assert!(p.nodes.contains_key(&format!("c{i}")), "c{i} missing");
    }
}
