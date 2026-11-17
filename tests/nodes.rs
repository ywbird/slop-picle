mod common;

use common::*;
use serde_json::{Value as Json, json};
use slopc::surface::Surface;

// ---------- solid ----------

#[test]
fn n_solid_01_color() {
    let s = eval(4, 3, json!({ "s": { "type": "solid", "inputs": { "color": "#ff800040" } } }), "s").unwrap();
    assert_eq!((s.width, s.height), (4, 3));
    assert_all(&s, [255, 128, 0, 64], 0);
}

#[test]
fn n_solid_02_default_white() {
    let s = eval(2, 2, json!({ "s": { "type": "solid" } }), "s").unwrap();
    assert_all(&s, [255, 255, 255, 255], 0);
}

#[test]
fn n_solid_03_dimension() {
    let s = eval(4, 3, json!({ "s": { "type": "solid", "inputs": { "dimension": [2, 5] } } }), "s").unwrap();
    assert_eq!((s.width, s.height), (2, 5));
}

// ---------- noise ----------

fn noise(w: u32, h: u32, inputs: Json) -> Surface {
    eval(w, h, json!({ "n": { "type": "noise", "inputs": inputs } }), "n").unwrap()
}

#[test]
fn n_noise_01_greyscale_exact() {
    let s = noise(4, 4, json!({}));
    for y in 0..4 {
        for x in 0..4 {
            let g = (hash2(x as i64, y as i64, 0) >> 24) as u8;
            assert_eq!(s.pixel(x, y), [g, g, g, 255], "({x},{y})");
        }
    }
    assert_golden("noise_grey_s0.png", &s, 0);
}

#[test]
fn n_noise_02_seed_changes_output() {
    assert_ne!(noise(8, 8, json!({ "seed": 0 })), noise(8, 8, json!({ "seed": 1 })));
}

#[test]
fn n_noise_03_distribution() {
    let s = noise(64, 64, json!({}));
    let mut sum = 0u64;
    for y in 0..64 {
        for x in 0..64 {
            let p = s.pixel(x, y);
            assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255);
            sum += p[0] as u64;
        }
    }
    let mean = sum as f64 / 4096.0;
    assert!((112.0..=144.0).contains(&mean), "mean {mean}");
}

#[test]
fn n_noise_04_rgb_ranges() {
    let s = noise(8, 8, json!({ "color_mode": "rgb", "color_r_range": [0, 0], "color_g_range": [1, 1], "color_b_range": [0, 1] }));
    for y in 0..8 {
        for x in 0..8 {
            let h = hash2(x as i64, y as i64, 0);
            let b = (unit(hash(h.wrapping_add(2))) * 255.0).round() as u8;
            assert_close(s.pixel(x, y), [0, 255, b, 255], 1);
        }
    }
}

#[test]
fn n_noise_05_hsv_white() {
    let s = noise(8, 8, json!({ "color_mode": "hsv", "color_r_range": [0, 1], "color_g_range": [0, 0], "color_b_range": [1, 1] }));
    assert_all(&s, [255, 255, 255, 255], 0);
}

#[test]
fn n_noise_06_range_checked() {
    let e = errors(&project_json(4, 4, json!({ "n": { "type": "noise", "inputs": { "color_r_range": [0, 2] } } })));
    assert!(has(&e, "out_of_range", Some("n"), Some("color_r_range")), "{e:?}");
}

#[test]
fn n_noise_07_position_based() {
    let small = noise(4, 4, json!({}));
    let big = noise(4, 4, json!({ "dimension": [8, 8] }));
    assert_eq!((big.width, big.height), (8, 8));
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(big.pixel(x, y), small.pixel(x, y));
        }
    }
}

// ---------- transform ----------

fn transform(src: &Surface, inputs: Json) -> Result<Surface, slopc::project::Error> {
    let mut inputs = inputs;
    inputs["surface_in"] = json!({ "ref": "src" });
    eval_on(src, json!({ "t": { "type": "transform", "inputs": inputs } }), "t")
}

const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[test]
fn n_transform_01_identity() {
    let f = fixture(4, 4);
    assert_eq!(transform(&f, json!({})).unwrap(), f);
}

#[test]
fn n_transform_02_move() {
    let f = fixture(4, 4);
    let s = transform(&f, json!({ "position": [0.75, 0.5] })).unwrap();
    for y in 0..4 {
        assert_eq!(s.pixel(0, y), CLEAR);
        for x in 1..4 {
            assert_eq!(s.pixel(x, y), f.pixel(x - 1, y));
        }
    }
}

#[test]
fn n_transform_03_wrap() {
    let f = fixture(4, 4);
    let s = transform(&f, json!({ "position": [0.75, 0.5], "render_mode": "wrap" })).unwrap();
    for y in 0..4 {
        assert_eq!(s.pixel(0, y), f.pixel(3, y));
    }
}

#[test]
fn n_transform_04_rotate_ccw() {
    let f = fixture(4, 4);
    let s = transform(&f, json!({ "rotation": 90 })).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(s.pixel(x, y), f.pixel(3 - y, x), "({x},{y})");
        }
    }
}

#[test]
fn n_transform_05_full_turns() {
    let f = fixture(4, 4);
    for r in [360, -360] {
        assert_eq!(transform(&f, json!({ "rotation": r })).unwrap(), f, "{r}");
    }
}

#[test]
fn n_transform_06_scale_blocks() {
    let f = surface(4, 4, |x, y| if x < 2 && y < 2 { [x as u8 * 100 + 10, y as u8 * 100 + 10, 5, 255] } else { CLEAR });
    let s = transform(&f, json!({ "scale": [2, 2], "anchor": [0, 0], "position": [0, 0] })).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(s.pixel(x, y), f.pixel(x / 2, y / 2), "({x},{y})");
        }
    }
}

#[test]
fn n_transform_07_mirror() {
    let f = fixture(4, 4);
    let s = transform(&f, json!({ "scale": [-1, 1] })).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(s.pixel(x, y), f.pixel(3 - x, y));
        }
    }
}

#[test]
fn n_transform_08_zero_scale() {
    let e = transform(&fixture(4, 4), json!({ "scale": [0, 1] })).unwrap_err();
    assert_eq!((e.code.as_str(), e.node.as_deref()), ("eval_error", Some("t")));
}

#[test]
fn n_transform_09_rotate_odd_size() {
    let s = transform(&fixture(5, 3), json!({ "rotation": 90 })).unwrap();
    assert_eq!((s.width, s.height), (5, 3));
    assert_golden("transform_rot90_5x3.png", &s, 0);
}

#[test]
fn n_transform_10_alpha() {
    let f = fixture(4, 4);
    let s = transform(&f, json!({ "alpha": 0.5 })).unwrap();
    for y in 0..4 {
        for x in 0..4 {
            let mut e = f.pixel(x, y);
            e[3] = 128;
            assert_close(s.pixel(x, y), e, 1);
            assert_eq!(&s.pixel(x, y)[..3], &f.pixel(x, y)[..3]);
        }
    }
}

// ---------- blend ----------

/// Blend `fg` over `bg` (each a surface read through an image node).
fn blend(bg: &Surface, fg: Option<&Surface>, inputs: Json) -> Surface {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "bg.png", bg);
    let mut inputs = inputs;
    inputs["background"] = json!({ "ref": "bg" });
    let mut nodes = json!({ "bg": { "type": "image", "inputs": { "path": "bg.png" } } });
    if let Some(fg) = fg {
        write_png(dir.path(), "fg.png", fg);
        nodes["fg"] = json!({ "type": "image", "inputs": { "path": "fg.png" } });
        inputs["foreground"] = json!({ "ref": "fg" });
    }
    nodes["b"] = json!({ "type": "blend", "inputs": inputs });
    eval_in(dir.path(), bg.width, bg.height, nodes, "b").unwrap()
}

fn flat(w: u32, h: u32, c: [u8; 4]) -> Surface {
    surface(w, h, |_, _| c)
}

#[test]
fn n_blend_01_normal_opaque() {
    let s = blend(&flat(2, 2, [0, 0, 255, 255]), Some(&flat(2, 2, [255, 0, 0, 255])), json!({}));
    assert_all(&s, [255, 0, 0, 255], 0);
}

#[test]
fn n_blend_02_opacity() {
    let s = blend(&flat(2, 2, [0, 0, 0, 255]), Some(&flat(2, 2, [255, 255, 255, 255])), json!({ "opacity": 0.5 }));
    assert_all(&s, [128, 128, 128, 255], 1);
}

#[test]
fn n_blend_03_transparent_foreground() {
    let bg = fixture(4, 4);
    assert_eq!(blend(&bg, Some(&flat(4, 4, CLEAR)), json!({})), bg);
}

#[test]
fn n_blend_04_both_transparent() {
    for mode in ["normal", "replace", "multiply", "add", "screen"] {
        let s = blend(&flat(2, 2, CLEAR), Some(&flat(2, 2, CLEAR)), json!({ "blend_mode": mode }));
        assert_all(&s, CLEAR, 0);
    }
}

#[test]
fn n_blend_05_multiply() {
    let s = blend(&flat(2, 2, [255, 0, 64, 255]), Some(&flat(2, 2, [128, 128, 128, 255])), json!({ "blend_mode": "multiply" }));
    assert_all(&s, [128, 0, 32, 255], 1);
}

#[test]
fn n_blend_06_add() {
    let s = blend(&flat(2, 2, [100, 200, 50, 255]), Some(&flat(2, 2, [200, 100, 0, 255])), json!({ "blend_mode": "add" }));
    assert_all(&s, [255, 255, 50, 255], 0);
}

#[test]
fn n_blend_07_screen() {
    let s = blend(&flat(2, 2, [128, 255, 0, 255]), Some(&flat(2, 2, [128, 0, 255, 255])), json!({ "blend_mode": "screen" }));
    assert_all(&s, [192, 255, 255, 255], 1);
}

#[test]
fn n_blend_08_multiply_over_transparent() {
    let s = blend(&flat(2, 2, CLEAR), Some(&flat(2, 2, [255, 0, 0, 255])), json!({ "blend_mode": "multiply" }));
    assert_all(&s, CLEAR, 0);
}

#[test]
fn n_blend_09_centred_small_foreground() {
    let bg = flat(4, 4, [0, 0, 255, 255]);
    let s = blend(&bg, Some(&flat(2, 2, [255, 0, 0, 255])), json!({}));
    for y in 0..4 {
        for x in 0..4 {
            let inside = (1..=2).contains(&x) && (1..=2).contains(&y);
            assert_eq!(s.pixel(x, y), if inside { [255, 0, 0, 255] } else { [0, 0, 255, 255] }, "({x},{y})");
        }
    }
}

#[test]
fn n_blend_10_large_foreground() {
    let fg = fixture(8, 8);
    let s = blend(&flat(4, 4, [0, 0, 0, 255]), Some(&fg), json!({}));
    assert_eq!((s.width, s.height), (4, 4));
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(s.pixel(x, y), fg.pixel(x + 2, y + 2));
        }
    }
}

#[test]
fn n_blend_11_no_foreground() {
    let bg = fixture(4, 4);
    assert_eq!(blend(&bg, None, json!({})), bg);
}

#[test]
fn n_blend_12_preserve_alpha() {
    let s = blend(&flat(2, 2, CLEAR), Some(&flat(2, 2, [255, 0, 0, 255])), json!({ "preserve_alpha": true }));
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(s.pixel(x, y)[3], 0);
        }
    }
}

#[test]
fn n_blend_13_position_corner() {
    let bg = flat(4, 4, [0, 0, 255, 255]);
    let fg = fixture(2, 2);
    let s = blend(&bg, Some(&fg), json!({ "position": [0, 0] }));
    for y in 0..4 {
        for x in 0..4 {
            let e = if (x, y) == (0, 0) { fg.pixel(1, 1) } else { [0, 0, 255, 255] };
            assert_eq!(s.pixel(x, y), e, "({x},{y})");
        }
    }
}

#[test]
fn n_blend_14_multiply_outside_foreground() {
    let s = blend(&flat(4, 4, [0, 0, 255, 255]), Some(&flat(2, 2, [255, 255, 255, 255])), json!({ "blend_mode": "multiply" }));
    for y in 0..4 {
        for x in 0..4 {
            let inside = (1..=2).contains(&x) && (1..=2).contains(&y);
            if !inside {
                assert_eq!(s.pixel(x, y), CLEAR, "({x},{y})");
            }
        }
    }
}

#[test]
fn n_blend_15_replace() {
    let s = blend(
        &flat(2, 2, [0, 0, 0, 255]),
        Some(&flat(2, 2, [255, 255, 255, 255])),
        json!({ "blend_mode": "replace", "opacity": 0.25 }),
    );
    assert_all(&s, [64, 64, 64, 255], 1);
}

// ---------- image ----------

fn write_raw_png(path: &std::path::Path, w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, data: &[u8], palette: Option<(&[u8], &[u8])>) {
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(color);
    enc.set_depth(depth);
    if let Some((pal, trns)) = palette {
        enc.set_palette(pal.to_vec());
        enc.set_trns(trns.to_vec());
    }
    enc.write_header().unwrap().write_image_data(data).unwrap();
}

fn read_image(dir: &std::path::Path, name: &str) -> Result<Surface, slopc::project::Error> {
    eval_in(dir, 4, 4, json!({ "img": { "type": "image", "inputs": { "path": name } } }), "img")
}

#[test]
fn n_image_01_rgba() {
    let dir = tempfile::tempdir().unwrap();
    let f = surface(3, 2, |x, y| [x as u8 * 80, y as u8 * 100, 7, if x == 0 { 0 } else { 200 }]);
    write_png(dir.path(), "a.png", &f);
    assert_eq!(read_image(dir.path(), "a.png").unwrap(), f);
}

#[test]
fn n_image_02_conversions() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    write_raw_png(&d.join("pal.png"), 2, 1, png::ColorType::Indexed, png::BitDepth::Eight, &[0, 1], Some((&[10, 20, 30, 40, 50, 60], &[0])));
    assert_eq!(read_image(d, "pal.png").unwrap().data, vec![10, 20, 30, 0, 40, 50, 60, 255]);
    write_raw_png(&d.join("grey.png"), 2, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[0, 200], None);
    assert_eq!(read_image(d, "grey.png").unwrap().data, vec![0, 0, 0, 255, 200, 200, 200, 255]);
    write_raw_png(&d.join("rgb16.png"), 1, 1, png::ColorType::Rgb, png::BitDepth::Sixteen, &[0x12, 0x34, 0xab, 0xcd, 0xff, 0x00], None);
    assert_eq!(read_image(d, "rgb16.png").unwrap().data, vec![0x12, 0xab, 0xff, 255]);
}

#[test]
fn n_image_03_relative_to_project_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    write_png(&dir.path().join("assets"), "a.png", &flat(1, 1, [1, 2, 3, 255]));
    let path = dir.path().join("p.slopc.json");
    std::fs::write(&path, project_json(1, 1, json!({ "img": { "type": "image", "inputs": { "path": "assets/a.png" } } }))).unwrap();
    let out = dir.path().join("o.png");
    let st = std::process::Command::new(bin())
        .current_dir(std::env::temp_dir())
        .args(["render", path.to_str().unwrap(), "-o", out.to_str().unwrap(), "--node", "img"])
        .status()
        .unwrap();
    assert!(st.success());
    assert_eq!(Surface::from_png(&std::fs::read(out).unwrap()).unwrap().pixel(0, 0), [1, 2, 3, 255]);
}

#[test]
fn n_image_04_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let e = read_image(dir.path(), "nope.png").unwrap_err();
    assert_eq!(e.code, "eval_error");
    assert!(e.message.contains("nope.png"), "{}", e.message);
}

#[test]
fn n_image_05_not_png() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("j.png"), [0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0]).unwrap();
    assert_eq!(read_image(dir.path(), "j.png").unwrap_err().code, "eval_error");
}

#[test]
fn n_image_06_too_large() {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "w.png", &flat(5000, 1, [0, 0, 0, 255]));
    assert_eq!(read_image(dir.path(), "w.png").unwrap_err().code, "eval_error");
}

// ---------- output ----------

#[test]
fn n_output_01_passthrough() {
    let nodes = json!({
        "s": { "type": "solid", "inputs": { "color": "#abcdef" } },
        "o": { "type": "output", "inputs": { "surface_in": { "ref": "s" } } }
    });
    assert_eq!(eval(3, 3, nodes.clone(), "o").unwrap(), eval(3, 3, nodes, "s").unwrap());
}
