// Shared mask, mix, channel and UV map modifiers (spec/nodes.md#shared-modifiers).
mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::surface::Surface;

const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn columns(w: u32, h: u32, white_below: u32) -> Surface {
    surface(w, h, |x, _| if x < white_below { WHITE } else { BLACK })
}

/// Evaluate node `n` of type `ty` over image `src`, with an image `mask` when given.
fn run(ty: &str, src: &Surface, mask: Option<&Surface>, inputs: Json) -> Surface {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "src.png", src);
    let mut inputs = inputs;
    inputs["surface_in"] = json!({ "ref": "src" });
    let mut nodes = json!({ "src": { "type": "image", "inputs": { "path": "src.png" } } });
    if let Some(m) = mask {
        write_png(dir.path(), "mask.png", m);
        nodes["m"] = json!({ "type": "image", "inputs": { "path": "mask.png" } });
        inputs["mask"] = json!({ "ref": "m" });
    }
    nodes["n"] = json!({ "type": ty, "inputs": inputs });
    eval_in(dir.path(), src.width, src.height, nodes, "n").unwrap()
}

#[test]
fn n_mod_01_to_03_mask_mix_invert() {
    let src = surface(2, 2, |_, _| [10, 20, 30, 255]);
    let s = run("invert", &src, Some(&columns(2, 2, 1)), json!({}));
    assert_eq!((s.pixel(0, 1), s.pixel(1, 1)), ([245, 235, 225, 255], [10, 20, 30, 255]));
    assert_all(&run("invert", &surface(2, 2, |_, _| BLACK), None, json!({ "mix": 0.5 })), [128, 128, 128, 255], 1);
    let s = run("invert", &src, Some(&columns(2, 2, 1)), json!({ "invert_mask": true }));
    assert_eq!((s.pixel(0, 1), s.pixel(1, 1)), ([10, 20, 30, 255], [245, 235, 225, 255]));
}

#[test]
fn n_mod_04_mask_feather() {
    let s = run("invert", &surface(6, 7, |_, _| BLACK), Some(&columns(6, 7, 3)), json!({ "mask_feather": 4 }));
    let row: Vec<u8> = (0..6).map(|x| s.pixel(x, 3)[0]).collect();
    for (got, want) in row.iter().zip([160u8, 193, 160, 89, 31, 6]) {
        assert!(got.abs_diff(want) <= 2, "row 3 {row:?}");
    }
}

#[test]
fn n_mod_05_channel() {
    assert_all(&run("invert", &surface(2, 2, |_, _| [10, 20, 30, 255]), None, json!({ "channel": 1 })), [245, 20, 30, 255], 0);
}

#[test]
fn n_mod_06_colour_where_original_is_clear() {
    let src = surface(3, 3, |x, y| if (x, y) == (1, 1) { WHITE } else { [0, 0, 0, 0] });
    let s = run("outline", &src, None, json!({ "width": 1, "color": "#ff0000", "mix": 0.5 }));
    assert_close(s.pixel(2, 1), [255, 0, 0, 128], 1);
}

#[test]
fn n_mod_07_generator_mask() {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "mask.png", &columns(2, 2, 1));
    let nodes = json!({ "m": { "type": "image", "inputs": { "path": "mask.png" } }, "c": { "type": "checker", "inputs": { "mask": { "ref": "m" } } } });
    let with = eval_in(dir.path(), 2, 2, nodes, "c").unwrap();
    let without = eval(2, 2, json!({ "c": { "type": "checker" } }), "c").unwrap();
    assert_eq!((with.pixel(0, 0), with.pixel(1, 0)[3]), (without.pixel(0, 0), 0));
}

#[test]
fn n_mod_08_uv_map() {
    let dir = tempfile::tempdir().unwrap();
    let map = surface(4, 1, |x, _| [(((3.5 - x as f64) / 4.0) * 255.0).round() as u8, 128, 0, 128]);
    write_png(dir.path(), "uv.png", &map);
    let grad = json!({ "type": "gradient", "inputs": {} });
    let plain = eval(4, 1, json!({ "g": grad }), "g").unwrap();
    let mut mapped = grad.clone();
    mapped["inputs"]["uv_map"] = json!({ "ref": "uv" });
    let s = eval_in(dir.path(), 4, 1, json!({ "uv": { "type": "image", "inputs": { "path": "uv.png" } }, "g": mapped }), "g").unwrap();
    for x in 0..4 {
        let (got, want) = (s.pixel(x, 0), plain.pixel(3 - x, 0));
        assert_close(got, [want[0], want[1], want[2], 128], 1);
    }
}

#[test]
fn n_mod_09_catalog() {
    let names = |ty: &str| -> Vec<&str> { slopc::catalog::get(ty).unwrap().inputs.iter().map(|i| i.name).collect() };
    for ty in ["flip", "blur", "dither", "threshold", "displace", "invert", "color_replace", "colorize", "greyscale"] {
        let n = names(ty);
        assert!(["mask", "mix", "invert_mask", "mask_feather", "channel"].iter().all(|i| n.contains(i)), "{ty}: {n:?}");
    }
    for ty in ["offset", "outline", "shadow", "posterize"] {
        let n = names(ty);
        assert!(["mask", "mix", "invert_mask", "mask_feather"].iter().all(|i| n.contains(i)) && !n.contains(&"channel"), "{ty}: {n:?}");
    }
    for ty in ["noise", "gradient", "shape", "cellular", "checker", "stripe"] {
        let n = names(ty);
        assert!(["uv_map", "uv_mix", "mask"].iter().all(|i| n.contains(i)), "{ty}: {n:?}");
    }
}
