mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::eval::Evaluator;
use slopc::ops;
use slopc::project::{self, Project};
use slopc::surface::Surface;
use std::path::Path;
use std::process::{Command, Output};

/// Frame `i`'s colour in the test projects.
fn colour(i: u8) -> [u8; 4] {
    [i * 60 + 10, 20, 200 - i * 40, 255]
}

fn hex(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A 4×4 project of `frames` frames: a 2×2 square in the middle whose colour is cut-keyed per frame, on transparency.
fn write_project(dir: &Path, frames: u8) -> std::path::PathBuf {
    let keys: Vec<Json> = (0..frames).map(|i| json!({ "time": i, "value": hex(colour(i)), "ease_out_type": "cut" })).collect();
    let p = dir.join("p.slopc.json");
    std::fs::write(
        &p,
        json!({ "slopc": 1, "settings": { "width": 4, "height": 4, "frames": frames, "fps": 12 }, "nodes": {
            "src": { "type": "shape", "inputs": { "half_size": [0.25, 0.25], "shape_color": { "anim": keys } } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "src" } } }
        } })
        .to_string(),
    )
    .unwrap();
    p
}

fn export(dir: &Path, args: &[&str]) -> Output {
    let mut a = vec!["export", "p.slopc.json"];
    a.extend_from_slice(args);
    Command::new(bin()).current_dir(dir).args(&a).output().unwrap()
}

fn png(dir: &Path, name: &str) -> Surface {
    Surface::from_png(&std::fs::read(dir.join(name)).unwrap_or_else(|_| panic!("missing {name}"))).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|n| !n.ends_with(".slopc.json") && !n.ends_with(".lock")).collect();
    v.sort();
    v
}

#[test]
fn ex_01_png_sequence() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    let o = export(d.path(), &["-o", "seq_%f.png"]);
    assert!(o.status.success(), "{o:?}");
    assert_eq!(names(d.path()), vec!["seq_1.png", "seq_2.png", "seq_3.png", "seq_4.png"]);
    for i in 0..4u8 {
        let s = png(d.path(), &format!("seq_{}.png", i + 1));
        assert_eq!(s.pixel(1, 1), colour(i));
        assert_eq!(s.pixel(0, 0)[3], 0);
    }
}

#[test]
fn ex_02_padding_begin_and_escape() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "seq_%3f.png", "--begin", "10"]).status.success());
    assert!(export(d.path(), &["-o", "a%%b_%f.png"]).status.success());
    let n = names(d.path());
    for f in ["seq_011.png", "seq_014.png", "a%b_1.png", "a%b_4.png"] {
        assert!(n.contains(&f.to_string()), "{f} in {n:?}");
    }
}

#[test]
fn ex_03_range_and_step() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "r_%f.png", "--range", "2:3"]).status.success());
    assert_eq!(names(d.path()), vec!["r_2.png", "r_3.png"]);
    assert_eq!(png(d.path(), "r_2.png").pixel(1, 1), colour(1));
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "s_%f.png", "--step", "2"]).status.success());
    assert_eq!(names(d.path()), vec!["s_1.png", "s_3.png"]);
    for bad in ["0:3", "3:2", "1:5"] {
        let o = export(d.path(), &["-o", "x_%f.png", "--range", bad, "--json"]);
        assert_eq!(o.status.code(), Some(1), "{bad}");
        assert!(String::from_utf8_lossy(&o.stdout).contains("out_of_range"), "{bad}");
    }
}

#[test]
fn ex_04_gif() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "a.gif", "--fps", "4"]).status.success());
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(std::fs::File::open(d.path().join("a.gif")).unwrap()).unwrap();
    assert_eq!(dec.repeat(), gif::Repeat::Infinite);
    let mut i = 0u8;
    while let Some(f) = dec.read_next_frame().unwrap() {
        assert_eq!((f.delay, f.dispose), (25, gif::DisposalMethod::Background));
        let px = |x: usize, y: usize| -> [u8; 4] { f.buffer[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].try_into().unwrap() };
        assert_eq!(px(1, 1), colour(i));
        assert_eq!(px(0, 0)[3], 0);
        i += 1;
    }
    assert_eq!(i, 4);
}

#[test]
fn ex_05_apng() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "a.apng"]).status.success());
    let dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(d.path().join("a.apng")).unwrap()));
    let mut r = dec.read_info().unwrap();
    let ac = r.info().animation_control.unwrap();
    assert_eq!((ac.num_frames, ac.num_plays), (4, 0));
    let mut buf = vec![0; r.output_buffer_size().unwrap()];
    for i in 0..4u8 {
        r.next_frame(&mut buf).unwrap();
        let fc = r.info().frame_control.unwrap();
        assert_eq!((fc.delay_num, fc.delay_den), (1, 12));
        assert_eq!(&buf[(4 + 1) * 4..(4 + 1) * 4 + 4], &colour(i));
        assert_eq!(buf[3], 0);
    }
}

#[test]
fn ex_06_scale() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 2);
    assert!(export(d.path(), &["-o", "s_%f.png", "--scale", "3"]).status.success());
    let s = png(d.path(), "s_1.png");
    assert_eq!((s.width, s.height), (12, 12));
}

#[test]
fn ex_07_single_image() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 4);
    assert!(export(d.path(), &["-o", "a.png", "--frame", "2"]).status.success());
    assert_eq!(names(d.path()), vec!["a.png"]);
    assert_eq!(png(d.path(), "a.png").pixel(1, 1), colour(2));
}

#[test]
fn ex_08_bad_paths() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 2);
    for out in ["a.jpg", "a_%f.gif", "a_%x.png"] {
        assert_eq!(export(d.path(), &["-o", out]).status.code(), Some(2), "{out}");
    }
    assert!(names(d.path()).is_empty());
}

#[test]
fn ex_09_many_colours_gif() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(
        d.path().join("p.slopc.json"),
        json!({ "slopc": 1, "settings": { "width": 32, "height": 32 }, "nodes": {
            "n": { "type": "noise", "inputs": { "color_mode": "rgb" } },
            "out": { "type": "output", "inputs": { "surface_in": { "ref": "n" } } } } })
        .to_string(),
    )
    .unwrap();
    assert!(export(d.path(), &["-o", "n.gif"]).status.success());
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(std::fs::File::open(d.path().join("n.gif")).unwrap()).unwrap();
    let mut n = 0;
    while dec.read_next_frame().unwrap().is_some() {
        n += 1;
    }
    assert_eq!(n, 1);
}

#[test]
fn ex_10_json() {
    let d = tempfile::tempdir().unwrap();
    write_project(d.path(), 3);
    let o = export(d.path(), &["-o", "j_%f.png", "--json"]);
    let v: Json = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v, json!({ "ok": true, "files": ["j_1.png", "j_2.png", "j_3.png"], "frames": [0, 1, 2] }));
}

// ---------- render_sprite_sheet ----------

/// Source `src`: a 3×2 solid whose colour is cut-keyed per frame (`colour(i)`), plus a sheet node with `inputs`.
fn sheet(frames: u8, inputs: Json) -> Project {
    let keys: Vec<Json> = (0..frames).map(|i| json!({ "time": i, "value": hex(colour(i)), "ease_out_type": "cut" })).collect();
    let mut inputs = inputs;
    inputs["sprites"] = json!({ "ref": "src" });
    let text = json!({ "slopc": 1, "settings": { "width": 3, "height": 2, "frames": frames }, "nodes": {
        "src": { "type": "solid", "inputs": { "color": { "anim": keys } } },
        "sh": { "type": "render_sprite_sheet", "inputs": inputs } } })
    .to_string();
    let l = project::load_str(&text);
    assert!(l.errors.is_empty(), "{:?}", l.errors);
    l.project.unwrap()
}

fn render(p: &Project) -> Surface {
    Evaluator::new().eval(p, ".".as_ref(), "sh", 0).unwrap().into_surface().unwrap()
}

#[test]
fn n_sheet_01_horizontal() {
    let s = render(&sheet(4, json!({})));
    assert_eq!((s.width, s.height), (12, 2));
    for i in 0..4u8 {
        assert_eq!(s.pixel(i as u32 * 3 + 1, 1), colour(i));
    }
}

#[test]
fn n_sheet_02_vertical() {
    let s = render(&sheet(4, json!({ "packing_type": "vertical", "spacing": 1 })));
    assert_eq!((s.width, s.height), (3, 11));
    assert_eq!(s.pixel(0, 3), colour(1));
    assert_eq!(s.pixel(0, 2)[3], 0);
}

#[test]
fn n_sheet_03_grid() {
    let s = render(&sheet(5, json!({ "packing_type": "grid", "grid_column": 3, "grid_spacing": [1, 2] })));
    assert_eq!((s.width, s.height), (11, 6));
    assert_eq!(s.pixel(4, 4), colour(4));
    assert_eq!(s.pixel(8, 4)[3], 0);
}

#[test]
fn n_sheet_04_padding() {
    let s = render(&sheet(4, json!({ "padding": [1, 2, 3, 4] })));
    assert_eq!((s.width, s.height), (16, 8));
    assert_eq!(s.pixel(3, 2), colour(0));
    assert_eq!(s.pixel(2, 2)[3], 0);
}

#[test]
fn n_sheet_05_step_and_range() {
    let s = render(&sheet(6, json!({ "frame_step": 2 })));
    assert_eq!(s.width, 9);
    assert_eq!([s.pixel(0, 0), s.pixel(3, 0), s.pixel(6, 0)], [colour(0), colour(2), colour(4)]);
    let s = render(&sheet(6, json!({ "custom_range": true, "range": [2, -2] })));
    assert_eq!(s.width, 12);
    assert_eq!([s.pixel(0, 0), s.pixel(9, 0)], [colour(1), colour(4)]);
}

#[test]
fn n_sheet_06_skip_empty() {
    let mut p = sheet(4, json!({ "skip_empty": true }));
    // Frames 1 and 2 transparent.
    let keys: Vec<Json> = (0..4u8).map(|i| json!({ "time": i, "value": if i == 1 || i == 2 { "#00000000".to_string() } else { hex(colour(i)) }, "ease_out_type": "cut" })).collect();
    p = ops::apply(&p, &json!([{ "op": "set_input", "id": "src", "input": "color", "value": { "anim": keys } }])).unwrap().project;
    let s = render(&p);
    assert_eq!(s.width, 6);
    assert_eq!([s.pixel(0, 0), s.pixel(3, 0)], [colour(0), colour(3)]);
}

#[test]
fn n_sheet_07_alignment() {
    let text = json!({ "slopc": 1, "settings": { "width": 4, "height": 4, "frames": 2 }, "nodes": {
        "src": { "type": "solid", "inputs": { "color": "#ff0000", "dimension": { "anim": [
            { "time": 0, "value": [2, 2], "ease_out_type": "cut" }, { "time": 1, "value": [4, 4] }] } } },
        "sh": { "type": "render_sprite_sheet", "inputs": { "sprites": { "ref": "src" }, "alignment": "middle" } } } })
    .to_string();
    let p = project::load_str(&text).project.unwrap();
    let s = render(&p);
    assert_eq!((s.width, s.height), (8, 4));
    assert_eq!(s.pixel(0, 0)[3], 0);
    assert_eq!(s.pixel(0, 1), [255, 0, 0, 255]);
    assert_eq!(s.pixel(0, 3)[3], 0);
}

#[test]
fn n_sheet_08_key_follows_frames() {
    let p = sheet(4, json!({}));
    let mut ev = Evaluator::new();
    let a = ev.eval(&p, ".".as_ref(), "sh", 0).unwrap().into_surface().unwrap();
    assert_eq!(a.pixel(10, 0), colour(3));
    let p2 = ops::apply(&p, &json!([{ "op": "set_key", "id": "src", "input": "color", "time": 3, "value": "#ffffff", "ease_out_type": "cut" }])).unwrap().project;
    let b = ev.eval(&p2, ".".as_ref(), "sh", 0).unwrap().into_surface().unwrap();
    assert_eq!(b.pixel(10, 0), [255, 255, 255, 255]);
}
