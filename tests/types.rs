mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::catalog;
use slopc::types::{Type, parse_color};

fn input_errors(ty: &str, input: &str, value: Json) -> Vec<String> {
    let mut inputs = serde_json::Map::new();
    inputs.insert(input.into(), value);
    errors(&project_json(4, 4, json!({ "n": { "type": ty, "inputs": inputs } })))
        .into_iter()
        .filter(|e| e.input.as_deref() == Some(input))
        .map(|e| e.code)
        .collect()
}

#[test]
fn ty_01_color_literals() {
    assert_eq!(parse_color("#ff0000"), Some([255, 0, 0, 255]));
    assert_eq!(parse_color("#FF0000"), Some([255, 0, 0, 255]));
    assert_eq!(parse_color("#ff000080"), Some([255, 0, 0, 128]));
}

#[test]
fn ty_02_bad_color_literals() {
    for v in ["#f00", "red", "#ff00000", "#gg0000"] {
        assert_eq!(input_errors("solid", "color", json!(v)), vec!["type_mismatch"], "{v}");
    }
}

#[test]
fn ty_03_int_input() {
    assert!(input_errors("noise", "seed", json!(3.0)).is_empty());
    assert_eq!(input_errors("noise", "seed", json!(3.5)), vec!["type_mismatch"]);
}

#[test]
fn ty_04_int_into_number() {
    assert!(input_errors("blend", "opacity", json!(1)).is_empty());
}

#[test]
fn ty_05_bad_vec2() {
    for v in [json!([1]), json!([1, 2, 3]), json!(["1", 2])] {
        assert_eq!(input_errors("transform", "position", v.clone()), vec!["type_mismatch"], "{v}");
    }
}

#[test]
fn ty_06_bool_given_number() {
    assert_eq!(input_errors("blend", "preserve_alpha", json!(1)), vec!["type_mismatch"]);
}

#[test]
fn ty_07_catalog_rules() {
    let name_ok = |n: &str| {
        let mut c = n.chars();
        c.next().is_some_and(|f| f.is_ascii_lowercase())
            && n.chars().all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    };
    let mut types = std::collections::HashSet::new();
    for def in catalog::all() {
        assert!(types.insert(def.ty), "duplicate type {}", def.ty);
        assert!(name_ok(def.ty), "{}", def.ty);
        assert!(!def.outputs.is_empty(), "{} has no outputs", def.ty);
        let mut names = std::collections::HashSet::new();
        for i in def.inputs {
            assert!(name_ok(i.name) && names.insert(i.name), "{}.{}", def.ty, i.name);
            let j = catalog::input_json(i);
            if i.required {
                assert!(j.get("default").is_none(), "{}.{} required with default", def.ty, i.name);
            } else if i.ty == Type::Dimension {
                assert_eq!(j["default"], Json::Null);
            } else if i.ty == Type::Surface {
                assert!(j.get("default").is_none(), "{}.{} optional surface has no default", def.ty, i.name);
            } else {
                let d = &j["default"];
                assert!(!d.is_null(), "{}.{} lacks default", def.ty, i.name);
                assert!(slopc::types::check_literal(i, d).is_ok(), "{}.{} default {d} invalid", def.ty, i.name);
            }
            if i.ty == Type::Enum {
                assert!(!i.options.is_empty());
            }
            if i.min.is_some() || i.max.is_some() {
                assert!(matches!(i.ty, Type::Number | Type::Int | Type::Vec2), "{}.{}", def.ty, i.name);
            }
        }
        let mut outs = std::collections::HashSet::new();
        for o in def.outputs {
            assert!(name_ok(o.name) && outs.insert(o.name), "{}.{}", def.ty, o.name);
        }
    }
}

#[test]
fn ty_08_nodes_json_matches_catalog() {
    let out = std::process::Command::new(bin()).args(["nodes", "--json"]).output().unwrap();
    assert!(out.status.success());
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), catalog::all().len());
    for (j, def) in arr.iter().zip(catalog::all()) {
        assert_eq!(j, &catalog::def_json(def));
        for key in ["type", "category", "description", "inputs", "outputs"] {
            assert!(j.get(key).is_some(), "{key}");
        }
    }
}

#[test]
fn ty_11_bad_dimension_type() {
    for v in [json!([2.5, 1]), json!([1])] {
        assert_eq!(input_errors("solid", "dimension", v.clone()), vec!["type_mismatch"], "{v}");
    }
}

#[test]
fn ty_12_dimension_out_of_range() {
    for v in [json!([0, 1]), json!([4097, 1])] {
        assert_eq!(input_errors("solid", "dimension", v.clone()), vec!["out_of_range"], "{v}");
    }
}

#[test]
fn ty_13_dimension_too_large() {
    assert_eq!(input_errors("solid", "dimension", json!([4096, 4096])), vec!["too_large"]);
}
