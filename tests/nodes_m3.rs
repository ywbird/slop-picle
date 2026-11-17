mod common;

use common::*;
use serde_json::{Value as Json, json};
use picle::eval::Evaluator;
use picle::project;
use picle::surface::Surface;
use picle::types::Value;

const CLEAR: [u8; 4] = [0, 0, 0, 0];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

fn generate(w: u32, h: u32, ty: &str, inputs: Json) -> Surface {
    eval(w, h, json!({ "n": { "type": ty, "inputs": inputs } }), "n").unwrap_or_else(|e| panic!("{e:?}"))
}

/// Apply a filter node `ty` to `src`.
fn filter(src: &Surface, ty: &str, inputs: Json) -> Result<Surface, project::Error> {
    let mut inputs = inputs;
    inputs["surface_in"] = json!({ "ref": "src" });
    eval_on(src, json!({ "f": { "type": ty, "inputs": inputs } }), "f")
}

fn ok(src: &Surface, ty: &str, inputs: Json) -> Surface {
    filter(src, ty, inputs).unwrap_or_else(|e| panic!("{e:?}"))
}

fn flat(w: u32, h: u32, c: [u8; 4]) -> Surface {
    surface(w, h, |_, _| c)
}

fn grey(v: u8) -> [u8; 4] {
    [v, v, v, 255]
}

fn value(nodes: Json, target: &str, frames: i64, frame: i64) -> Result<Value, project::Error> {
    let text = json!({ "picle": 1, "settings": { "width": 2, "height": 2, "frames": frames }, "nodes": nodes }).to_string();
    let p = project::load_str(&text).project.unwrap();
    Evaluator::new().eval(&p, ".".as_ref(), target, frame)
}

fn num(nodes: Json) -> f64 {
    match value(nodes, "n", 1, 0).unwrap() {
        Value::Number(v) => v,
        v => panic!("{v:?}"),
    }
}

// ---------- gradient ----------

#[test]
fn n_gradient_01_linear() {
    let s = generate(8, 1, "gradient", json!({}));
    for x in 0..8 {
        let v = ((x as f64 + 0.5) / 8.0 * 255.0).round() as u8;
        assert_close(s.pixel(x, 0), grey(v), 1);
    }
}

#[test]
fn n_gradient_02_constant() {
    let g = json!({ "type": "constant", "keys": [{ "time": 0, "color": "#000000" }, { "time": 0.5, "color": "#ffffff" }] });
    let s = generate(4, 1, "gradient", json!({ "gradient": g }));
    let got: Vec<_> = (0..4).map(|x| s.pixel(x, 0)).collect();
    assert_eq!(got, vec![BLACK, BLACK, WHITE, WHITE]);
}

#[test]
fn n_gradient_03_circular() {
    let s = generate(4, 4, "gradient", json!({ "type": "circular" }));
    assert!(s.pixel(1, 1)[0] < s.pixel(0, 0)[0] && s.pixel(2, 2)[0] < s.pixel(3, 3)[0]);
}

#[test]
fn n_gradient_04_loop() {
    let s = generate(8, 1, "gradient", json!({ "shift": 0.5, "loop": "loop" }));
    assert_close(s.pixel(7, 0), grey((0.4375f64 * 255.0).round() as u8), 1);
}

// ---------- shape ----------

#[test]
fn n_shape_01_rectangle() {
    let s = generate(8, 8, "shape", json!({ "half_size": [0.25, 0.25] }));
    for y in 0..8 {
        for x in 0..8 {
            let inside = (2..=5).contains(&x) && (2..=5).contains(&y);
            assert_eq!(s.pixel(x, y), if inside { WHITE } else { CLEAR }, "({x},{y})");
        }
    }
}

#[test]
fn n_shape_02_ellipse() {
    let s = generate(8, 8, "shape", json!({ "shape": "ellipse" }));
    assert_eq!(s.pixel(0, 0), CLEAR);
    assert_eq!(s.pixel(7, 7), CLEAR);
    for (x, y) in [(3, 3), (4, 3), (3, 4), (4, 4)] {
        assert_eq!(s.pixel(x, y), WHITE);
    }
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(s.pixel(x, y), s.pixel(7 - x, y));
            assert_eq!(s.pixel(x, y), s.pixel(x, 7 - y));
        }
    }
}

#[test]
fn n_shape_03_background() {
    let s = generate(8, 8, "shape", json!({ "half_size": [0.25, 0.25], "background": "solid", "bg_color": "#ff0000" }));
    assert_eq!(s.pixel(0, 0), [255, 0, 0, 255]);
    assert_eq!(s.pixel(4, 4), WHITE);
}

#[test]
fn n_shape_04_rotation() {
    let s = generate(8, 8, "shape", json!({ "half_size": [0.5, 0.25], "shape_rotation": 90 }));
    let filled: Vec<(u32, u32)> = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| s.pixel(x, y) == WHITE).collect();
    let xs: std::collections::BTreeSet<u32> = filled.iter().map(|p| p.0).collect();
    let ys: std::collections::BTreeSet<u32> = filled.iter().map(|p| p.1).collect();
    assert_eq!((xs.len(), ys.len(), filled.len()), (4, 8, 32));
}

// ---------- cellular ----------

#[test]
fn n_cellular_01_seed() {
    let a = generate(16, 16, "cellular", json!({ "seed": 0 }));
    assert_eq!(a, generate(16, 16, "cellular", json!({ "seed": 0 })));
    assert_ne!(a, generate(16, 16, "cellular", json!({ "seed": 1 })));
}

#[test]
fn n_cellular_02_periodic() {
    let s = generate(16, 16, "cellular", json!({ "randomness": 0, "position": [0, 0], "scale": 4 }));
    for y in 0..12 {
        for x in 0..12 {
            assert_close(s.pixel(x, y), s.pixel(x + 4, y), 1);
            assert_close(s.pixel(x, y), s.pixel(x, y + 4), 1);
        }
    }
}

#[test]
fn n_cellular_03_inverted() {
    let a = generate(8, 8, "cellular", json!({}));
    let b = generate(8, 8, "cellular", json!({ "inverted": true }));
    for y in 0..8 {
        for x in 0..8 {
            assert_close(b.pixel(x, y), grey(255 - a.pixel(x, y)[0]), 1);
        }
    }
}

#[test]
fn n_cellular_04_no_contrast() {
    assert_all(&generate(8, 8, "cellular", json!({ "contrast": 0 })), grey(128), 1);
}

// ---------- checker ----------

#[test]
fn n_checker_01_default() {
    let s = generate(8, 8, "checker", json!({}));
    for y in 0..8 {
        for x in 0..8 {
            let odd = (x / 4 + y / 4) % 2 == 1;
            assert_eq!(s.pixel(x, y), if odd { WHITE } else { BLACK }, "({x},{y})");
        }
    }
}

#[test]
fn n_checker_02_small_cells() {
    let s = generate(8, 8, "checker", json!({ "size": 0.25 }));
    for y in 0..8 {
        for x in 0..8 {
            let odd = (x / 2 + y / 2) % 2 == 1;
            assert_eq!(s.pixel(x, y), if odd { WHITE } else { BLACK }, "({x},{y})");
        }
    }
}

#[test]
fn n_checker_03_colors() {
    let s = generate(8, 8, "checker", json!({ "color_1": "#ff0000", "color_2": "#0000ff" }));
    for y in 0..8 {
        for x in 0..8 {
            assert!([[255, 0, 0, 255], [0, 0, 255, 255]].contains(&s.pixel(x, y)));
        }
    }
}

// ---------- stripe ----------

#[test]
fn n_stripe_01_columns() {
    let s = generate(8, 8, "stripe", json!({}));
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(s.pixel(x, y), if x % 2 == 0 { WHITE } else { BLACK }, "({x},{y})");
        }
    }
}

#[test]
fn n_stripe_02_rows() {
    let s = generate(8, 8, "stripe", json!({ "angle": 90 }));
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(s.pixel(x, y), if y % 2 == 0 { BLACK } else { WHITE }, "({x},{y})");
        }
    }
}

#[test]
fn n_stripe_03_full_ratio() {
    assert_all(&generate(8, 8, "stripe", json!({ "strip_ratio": 1 })), BLACK, 0);
}

// ---------- scale ----------

#[test]
fn n_scale_01_up() {
    let f = fixture(2, 2);
    let s = ok(&f, "scale", json!({ "scale": 2 }));
    assert_eq!((s.width, s.height), (4, 4));
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(s.pixel(x, y), f.pixel(x / 2, y / 2));
        }
    }
}

#[test]
fn n_scale_02_down() {
    let f = fixture(4, 4);
    let s = ok(&f, "scale", json!({ "scale": 0.5 }));
    assert_eq!((s.width, s.height), (2, 2));
    assert_eq!([s.pixel(0, 0), s.pixel(1, 0), s.pixel(0, 1), s.pixel(1, 1)], [f.pixel(1, 1), f.pixel(3, 1), f.pixel(1, 3), f.pixel(3, 3)]);
}

#[test]
fn n_scale_03_to_size() {
    let f = fixture(6, 2);
    let s = ok(&f, "scale", json!({ "mode": "scale_to_size", "target_dimension": [3, 1] }));
    assert_eq!((s.width, s.height), (3, 1));
    assert_eq!([s.pixel(0, 0), s.pixel(1, 0), s.pixel(2, 0)], [f.pixel(1, 1), f.pixel(3, 1), f.pixel(5, 1)]);
}

#[test]
fn n_scale_04_zero() {
    assert_eq!(filter(&fixture(2, 2), "scale", json!({ "scale": 0 })).unwrap_err().code, "eval_error");
}

// ---------- crop ----------

#[test]
fn n_crop_01_columns() {
    let f = fixture(4, 2);
    let s = ok(&f, "crop", json!({ "crop": [1, 0, 1, 0] }));
    assert_eq!((s.width, s.height), (2, 2));
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(s.pixel(x, y), f.pixel(x + 1, y));
        }
    }
}

#[test]
fn n_crop_02_row() {
    let f = fixture(4, 4);
    let s = ok(&f, "crop", json!({ "crop": [0, 1, 0, 2] }));
    assert_eq!((s.width, s.height), (4, 1));
    for x in 0..4 {
        assert_eq!(s.pixel(x, 0), f.pixel(x, 1));
    }
}

#[test]
fn n_crop_03_everything() {
    assert_eq!(filter(&fixture(2, 2), "crop", json!({ "crop": [1, 0, 1, 0] })).unwrap_err().code, "eval_error");
}

// ---------- flip ----------

#[test]
fn n_flip_axes() {
    let f = fixture(4, 3);
    for (axis, fx, fy) in [(1, true, false), (2, false, true), (3, true, true), (0, false, false)] {
        let s = ok(&f, "flip", json!({ "axis": axis }));
        for y in 0..3 {
            for x in 0..4 {
                let sx = if fx { 3 - x } else { x };
                let sy = if fy { 2 - y } else { y };
                assert_eq!(s.pixel(x, y), f.pixel(sx, sy), "axis {axis} ({x},{y})");
            }
        }
    }
    // N-FLIP-01: the default is horizontal.
    let s = ok(&f, "flip", json!({}));
    assert_eq!(s.pixel(0, 0), f.pixel(3, 0));
}

// ---------- offset ----------

#[test]
fn n_offset_01_x() {
    let f = fixture(4, 1);
    let s = ok(&f, "offset", json!({ "x_offset": 0.25 }));
    for x in 0..4 {
        assert_eq!(s.pixel(x, 0), f.pixel((x + 3) % 4, 0));
    }
}

#[test]
fn n_offset_02_y_negative() {
    let f = fixture(4, 4);
    let s = ok(&f, "offset", json!({ "y_offset": -0.5 }));
    for y in 0..4 {
        assert_eq!(s.pixel(1, y), f.pixel(1, (y + 2) % 4));
    }
}

// ---------- downscale ----------

#[test]
fn n_downscale_01_blocks() {
    let colors = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [9, 9, 9, 255]];
    let f = surface(4, 4, |x, y| colors[(y / 2 * 2 + x / 2) as usize]);
    let s = ok(&f, "downscale", json!({ "downscale": 2 }));
    assert_eq!((s.width, s.height), (2, 2));
    for (i, c) in colors.iter().enumerate() {
        assert_close(s.pixel(i as u32 % 2, i as u32 / 2), *c, 1);
    }
}

#[test]
fn n_downscale_02_alpha_quirk() {
    let s = ok(&flat(4, 4, [200, 100, 0, 128]), "downscale", json!({ "downscale": 2 }));
    assert_all(&s, [200, 100, 0, 64], 1);
}

#[test]
fn n_downscale_03_max_min() {
    let vals = [10u8, 20, 30, 40];
    let f = surface(2, 2, |x, y| grey(vals[(y * 2 + x) as usize]));
    assert_close(ok(&f, "downscale", json!({ "downscale": 2, "mode": "max" })).pixel(0, 0), grey(40), 0);
    assert_close(ok(&f, "downscale", json!({ "downscale": 2, "mode": "min" })).pixel(0, 0), grey(10), 0);
}

#[test]
fn n_downscale_04_odd_size() {
    let s = ok(&fixture(5, 5), "downscale", json!({ "downscale": 2 }));
    assert_eq!((s.width, s.height), (3, 3));
}

// ---------- blur ----------

#[test]
fn n_blur_01_zero() {
    let f = fixture(4, 4);
    let s = ok(&f, "blur", json!({ "size": 0 }));
    for y in 0..4 {
        for x in 0..4 {
            assert_close(s.pixel(x, y), f.pixel(x, y), 1);
        }
    }
}

#[test]
fn n_blur_02_flat() {
    let s = ok(&flat(8, 8, [30, 60, 90, 255]), "blur", json!({ "size": 3 }));
    for y in 2..6 {
        for x in 2..6 {
            assert_close(s.pixel(x, y), [30, 60, 90, 255], 1);
        }
    }
    let e = s.pixel(0, 4);
    assert_close([e[0], e[1], e[2], 255], [30, 60, 90, 255], 1);
    assert!(e[3] < 255);
}

#[test]
fn n_blur_03_point() {
    let f = surface(9, 9, |x, y| if (x, y) == (4, 4) { WHITE } else { CLEAR });
    let s = ok(&f, "blur", json!({ "size": 3 }));
    let a = |x, y| s.pixel(x, y)[3];
    assert!(a(4, 4) > a(5, 4) && a(5, 4) > a(6, 4) && a(5, 4) > a(5, 5));
    for y in 0..9 {
        for x in 0..9 {
            assert_close([0, 0, 0, a(x, y)], [0, 0, 0, a(8 - x, y)], 1);
            if a(x, y) > 0 {
                assert_close(s.pixel(x, y), [255, 255, 255, a(x, y)], 1);
            }
        }
    }
}

#[test]
fn n_blur_04_override() {
    let f = surface(5, 5, |x, y| if (x, y) == (2, 2) { WHITE } else { CLEAR });
    let s = ok(&f, "blur", json!({ "size": 2, "override_color": true, "color": "#ff0000" }));
    for y in 0..5 {
        for x in 0..5 {
            let p = s.pixel(x, y);
            if p[3] > 0 {
                assert_eq!(&p[..3], &[255, 0, 0]);
            }
        }
    }
}

// ---------- outline ----------

fn dot5() -> Surface {
    surface(5, 5, |x, y| if (x, y) == (2, 2) { [255, 0, 0, 255] } else { CLEAR })
}

#[test]
fn n_outline_01_square() {
    let s = ok(&dot5(), "outline", json!({ "width": 1, "profile": "square" }));
    for y in 0..5i32 {
        for x in 0..5i32 {
            let d = (x - 2).abs().max((y - 2).abs());
            let e = match d {
                0 => [255, 0, 0, 255],
                1 => WHITE,
                _ => CLEAR,
            };
            assert_eq!(s.pixel(x as u32, y as u32), e, "({x},{y})");
        }
    }
}

#[test]
fn n_outline_02_circle_diamond() {
    for profile in ["circle", "diamond"] {
        let s = ok(&dot5(), "outline", json!({ "width": 1, "profile": profile }));
        let whites = (0..25).filter(|i| s.pixel(i % 5, i / 5) == WHITE).count();
        assert_eq!(whites, 4, "{profile}");
        assert_eq!(s.pixel(2, 1), WHITE);
        assert_eq!(s.pixel(1, 1), CLEAR);
    }
}

#[test]
fn n_outline_03_zero_width() {
    assert_eq!(ok(&dot5(), "outline", json!({})), dot5());
}

#[test]
fn n_outline_04_inside() {
    let f = surface(5, 5, |x, y| if (1..=3).contains(&x) && (1..=3).contains(&y) { [255, 0, 0, 255] } else { CLEAR });
    let s = ok(&f, "outline", json!({ "position": "inside", "width": 1, "profile": "square" }));
    for y in 0..5 {
        for x in 0..5 {
            let e = if (x, y) == (2, 2) {
                [255, 0, 0, 255]
            } else if (1..=3).contains(&x) && (1..=3).contains(&y) {
                WHITE
            } else {
                CLEAR
            };
            assert_eq!(s.pixel(x, y), e, "({x},{y})");
        }
    }
}

// ---------- shadow ----------

#[test]
fn n_shadow_01_opaque() {
    let f = fixture(4, 4);
    let s = ok(&f, "shadow", json!({}));
    for y in 0..4 {
        for x in 0..4 {
            assert_close(s.pixel(x, y), f.pixel(x, y), 1);
        }
    }
}

#[test]
fn n_shadow_02_shift() {
    let f = surface(6, 3, |x, y| if (x, y) == (1, 1) { WHITE } else { CLEAR });
    let s = ok(&f, "shadow", json!({ "grow": 0, "blur": 0, "shift": [2, 0], "strength": 1 }));
    for y in 0..3 {
        for x in 0..6 {
            let e = match (x, y) {
                (1, 1) => WHITE,
                (3, 1) => BLACK,
                _ => CLEAR,
            };
            assert_close(s.pixel(x, y), e, 1);
        }
    }
}

#[test]
fn n_shadow_03_default_spread() {
    let f = surface(12, 12, |x, y| if (5..7).contains(&x) && (5..7).contains(&y) { WHITE } else { CLEAR });
    let s = ok(&f, "shadow", json!({}));
    let p = s.pixel(4, 5);
    assert!(p[3] > 0 && p[0] == 0, "{p:?}");
}

// ---------- posterize ----------

#[test]
fn n_posterize_01_palette() {
    let f = surface(3, 1, |x, _| [grey(100), grey(200), [50, 50, 50, 0]][x as usize]);
    let s = ok(&f, "posterize", json!({}));
    assert_eq!([s.pixel(0, 0), s.pixel(1, 0), s.pixel(2, 0)], [BLACK, WHITE, BLACK]);
}

#[test]
fn n_posterize_02_steps() {
    let f = surface(2, 1, |x, _| [grey(128), grey(255)][x as usize]);
    let s = ok(&f, "posterize", json!({ "use_palette": false }));
    assert_close(s.pixel(0, 0), grey(170), 1);
    assert_close(s.pixel(1, 0), grey(255), 0);
}

#[test]
fn n_posterize_03_keep_alpha() {
    let s = ok(&flat(1, 1, [200, 200, 200, 100]), "posterize", json!({ "posterize_alpha": false }));
    assert_eq!(s.pixel(0, 0), [255, 255, 255, 100]);
}

// ---------- dither ----------

fn dither_grey(inputs: Json) -> Surface {
    ok(&flat(4, 4, grey(128)), "dither", inputs)
}

#[test]
fn n_dither_01_bayer() {
    let s = dither_grey(json!({}));
    assert_eq!(s.pixel(0, 0), BLACK);
    for by in 0..2 {
        for bx in 0..2 {
            let whites = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().filter(|(x, y)| s.pixel(bx * 2 + x, by * 2 + y) == WHITE).count();
            assert_eq!(whites, 2);
        }
    }
}

#[test]
fn n_dither_02_exact() {
    let f = surface(2, 2, |x, _| if x == 0 { BLACK } else { WHITE });
    assert_eq!(ok(&f, "dither", json!({})), f);
}

#[test]
fn n_dither_03_invert() {
    let a = dither_grey(json!({}));
    let b = dither_grey(json!({ "invert": true }));
    for y in 0..4 {
        for x in 0..4 {
            assert_ne!(a.pixel(x, y), b.pixel(x, y));
        }
    }
}

#[test]
fn n_dither_04_greyscale() {
    let s = ok(&flat(4, 4, grey(64)), "dither", json!({ "color_type": "greyscale" }));
    assert_all(&s, grey(64), 1);
}

// ---------- threshold ----------

fn two_greys() -> Surface {
    surface(2, 1, |x, _| [[100, 100, 100, 100], [200, 200, 200, 200]][x as usize])
}

#[test]
fn n_threshold_01_identity() {
    assert_eq!(ok(&two_greys(), "threshold", json!({})), two_greys());
}

#[test]
fn n_threshold_02_brightness() {
    let s = ok(&two_greys(), "threshold", json!({ "brightness": true }));
    assert_eq!([s.pixel(0, 0), s.pixel(1, 0)], [[0, 0, 0, 100], [255, 255, 255, 200]]);
}

#[test]
fn n_threshold_03_alpha() {
    let s = ok(&two_greys(), "threshold", json!({ "alpha": true }));
    assert_eq!([s.pixel(0, 0)[3], s.pixel(1, 0)[3]], [0, 255]);
}

#[test]
fn n_threshold_04_invert() {
    let s = ok(&two_greys(), "threshold", json!({ "brightness": true, "brightness_invert": true }));
    assert_eq!([s.pixel(0, 0)[0], s.pixel(1, 0)[0]], [255, 0]);
}

// ---------- displace ----------

fn displace(map: Option<[u8; 4]>, inputs: Json) -> (Surface, Surface) {
    let f = fixture(4, 1);
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "src.png", &f);
    let mut inputs = inputs;
    inputs["surface_in"] = json!({ "ref": "src" });
    let mut nodes = json!({ "src": { "type": "image", "inputs": { "path": "src.png" } } });
    if let Some(c) = map {
        write_png(dir.path(), "map.png", &flat(4, 1, c));
        nodes["map"] = json!({ "type": "image", "inputs": { "path": "map.png" } });
        inputs["displace_map"] = json!({ "ref": "map" });
    }
    nodes["d"] = json!({ "type": "displace", "inputs": inputs });
    (f, eval_in(dir.path(), 4, 1, nodes, "d").unwrap())
}

#[test]
fn n_displace_01_shift() {
    let (f, s) = displace(Some(WHITE), json!({ "position": [0.5, 0] }));
    for x in 0..3 {
        assert_eq!(s.pixel(x, 0), f.pixel(x + 1, 0));
    }
    assert_eq!(s.pixel(3, 0), CLEAR);
}

#[test]
fn n_displace_02_mid() {
    let (f, s) = displace(Some(grey(128)), json!({ "mid_value": 128.0 / 255.0 }));
    assert_eq!(s, f);
}

#[test]
fn n_displace_03_no_map() {
    let (f, s) = displace(None, json!({}));
    assert_eq!(s, f);
}

// ---------- color_adjust ----------

#[test]
fn n_color_adjust_01_identity() {
    let f = fixture(4, 4);
    let s = ok(&f, "color_adjust", json!({}));
    for y in 0..4 {
        for x in 0..4 {
            assert_close(s.pixel(x, y), f.pixel(x, y), 1);
        }
    }
}

#[test]
fn n_color_adjust_02_brightness() {
    assert_all(&ok(&flat(2, 2, grey(100)), "color_adjust", json!({ "brightness": 0.2 })), grey(151), 1);
}

#[test]
fn n_color_adjust_03_hue() {
    assert_all(&ok(&flat(2, 2, [255, 0, 0, 255]), "color_adjust", json!({ "hue": 0.5 })), [0, 255, 255, 255], 1);
}

#[test]
fn n_color_adjust_04_alpha() {
    assert_all(&ok(&flat(2, 2, grey(90)), "color_adjust", json!({ "alpha": 0.5 })), [90, 90, 90, 128], 1);
}

#[test]
fn n_color_adjust_05_blend() {
    let s = ok(&flat(2, 2, [10, 20, 30, 77]), "color_adjust", json!({ "blend": "#00ff00", "blend_amount": 1 }));
    assert_all(&s, [0, 255, 0, 77], 1);
}

// ---------- invert ----------

#[test]
fn n_invert_01_02() {
    let f = flat(1, 1, [10, 20, 30, 200]);
    assert_close(ok(&f, "invert", json!({})).pixel(0, 0), [245, 235, 225, 200], 0);
    assert_close(ok(&f, "invert", json!({ "include_alpha": true })).pixel(0, 0), [245, 235, 225, 55], 0);
}

// ---------- color_replace ----------

#[test]
fn n_color_replace_01_red_to_blue() {
    let f = surface(2, 1, |x, _| [[255, 0, 0, 200], [0, 255, 0, 255]][x as usize]);
    let s = ok(&f, "color_replace", json!({ "from": ["#ff0000"], "to": ["#0000ff"], "multiply_alpha": false }));
    assert_eq!([s.pixel(0, 0), s.pixel(1, 0)], [[0, 0, 255, 200], [0, 255, 0, 255]]);
}

#[test]
fn n_color_replace_02_defaults() {
    let f = surface(2, 1, |x, _| [BLACK, WHITE][x as usize]);
    assert_eq!(ok(&f, "color_replace", json!({})), f);
}

#[test]
fn n_color_replace_03_order_mapping() {
    let s = ok(&flat(1, 1, WHITE), "color_replace", json!({ "to": ["#ff0000", "#00ff00", "#0000ff"] }));
    assert_eq!(s.pixel(0, 0), [0, 0, 255, 255]);
}

// ---------- colorize ----------

fn black_to(color: &str) -> Json {
    json!({ "type": "rgb", "keys": [{ "time": 0, "color": "#000000" }, { "time": 1, "color": color }] })
}

#[test]
fn n_colorize_01_map() {
    let s = ok(&flat(2, 2, grey(128)), "colorize", json!({ "gradient": black_to("#ff0000") }));
    assert_all(&s, [128, 0, 0, 255], 1);
}

#[test]
fn n_colorize_02_transparent() {
    let s = ok(&flat(1, 1, [255, 255, 255, 0]), "colorize", json!({ "gradient": black_to("#ff0000") }));
    assert_eq!(s.pixel(0, 0), CLEAR);
}

#[test]
fn n_colorize_03_gradient_alpha() {
    let g = json!({ "type": "rgb", "keys": [{ "time": 0, "color": "#00000080" }, { "time": 1, "color": "#ffffff80" }] });
    let s = ok(&flat(1, 1, grey(255)), "colorize", json!({ "gradient": g, "keep_alpha": false }));
    assert_close(s.pixel(0, 0), [255, 255, 255, 128], 1);
}

// ---------- greyscale ----------

#[test]
fn n_greyscale_01_02() {
    assert_close(ok(&flat(1, 1, [255, 0, 0, 255]), "greyscale", json!({})).pixel(0, 0), grey(54), 1);
    assert_close(ok(&flat(1, 1, [255, 255, 255, 128]), "greyscale", json!({})).pixel(0, 0), [128, 128, 128, 128], 1);
}

// ---------- value nodes ----------

#[test]
fn n_value_01_number() {
    assert_eq!(num(json!({ "n": { "type": "number", "inputs": { "value": 2.5 } } })), 2.5);
}

#[test]
fn n_value_02_color_into_solid() {
    let s = eval(
        2,
        2,
        json!({ "c": { "type": "color", "inputs": { "color": "#123456" } }, "s": { "type": "solid", "inputs": { "color": { "ref": "c" } } } }),
        "s",
    )
    .unwrap();
    assert_all(&s, [0x12, 0x34, 0x56, 255], 0);
}

#[test]
fn n_value_03_palette_into_posterize() {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "src.png", &flat(1, 1, [200, 30, 10, 255]));
    let s = eval_in(
        dir.path(),
        1,
        1,
        json!({
            "src": { "type": "image", "inputs": { "path": "src.png" } },
            "pal": { "type": "palette", "inputs": { "palette": ["#ff0000", "#00ff00"] } },
            "p": { "type": "posterize", "inputs": { "surface_in": { "ref": "src" }, "palette": { "ref": "pal" } } }
        }),
        "p",
    )
    .unwrap();
    assert_eq!(s.pixel(0, 0), [255, 0, 0, 255]);
}

fn math(op: &str, a: f64, b: f64) -> Result<Value, project::Error> {
    value(json!({ "n": { "type": "math", "inputs": { "type": op, "a": a, "b": b } } }), "n", 1, 0)
}

#[test]
fn n_value_04_math() {
    for (op, a, b, e) in [
        ("add", 2.0, 3.0, 5.0),
        ("divide", 1.0, 0.0, 0.0),
        ("sin", 90.0, 1.0, 1.0),
        ("power", 2.0, 10.0, 1024.0),
        ("modulo", -7.0, 3.0, -1.0),
        ("snap", 7.0, 5.0, 5.0),
        ("log", 8.0, 2.0, 3.0),
    ] {
        match math(op, a, b).unwrap() {
            Value::Number(v) => assert!((v - e).abs() < 1e-9, "{op}: {v} != {e}"),
            v => panic!("{v:?}"),
        }
    }
}

#[test]
fn n_value_05_math_nan() {
    assert_eq!(math("power", -1.0, 0.5).unwrap_err().code, "eval_error");
}

#[test]
fn n_value_06_counter() {
    let v = value(json!({ "n": { "type": "counter" } }), "n", 10, 3).unwrap();
    assert_eq!(v, Value::Number(4.0));
    let v = value(json!({ "n": { "type": "counter", "inputs": { "mode": "animation_progress", "speed": 2 } } }), "n", 10, 9).unwrap();
    assert_eq!(v, Value::Number(2.0));
}

#[test]
fn n_value_07_counter_frame_in_key() {
    let text = json!({ "picle": 1, "settings": { "frames": 4 }, "nodes": { "n": { "type": "counter" } } }).to_string();
    let p = project::load_str(&text).project.unwrap();
    let mut ev = Evaluator::new();
    assert_eq!(ev.eval(&p, ".".as_ref(), "n", 0).unwrap(), Value::Number(1.0));
    assert_eq!(ev.eval(&p, ".".as_ref(), "n", 1).unwrap(), Value::Number(2.0));
    assert_eq!(ev.last_runs(), ["n"]);
}

// ---------- types and evaluation added in M3 ----------

#[test]
fn ty_09_int_fits_number() {
    use picle::types::{Type, fits};
    assert!(fits(Type::Int, Type::Number));
    assert!(!fits(Type::Number, Type::Int));
}

#[test]
fn ty_10_number_into_int() {
    let e = errors(&project_json(
        4,
        4,
        json!({ "v": { "type": "number" }, "n": { "type": "noise", "inputs": { "seed": { "ref": "v" } } } }),
    ));
    assert!(has(&e, "type_mismatch", Some("n"), Some("seed")), "{e:?}");
}

fn lit_errors(ty: &str, input: &str, v: Json) -> Vec<String> {
    let mut inputs = serde_json::Map::new();
    inputs.insert(input.into(), v);
    errors(&project_json(4, 4, json!({ "n": { "type": ty, "inputs": inputs } })))
        .into_iter()
        .filter(|e| e.input.as_deref() == Some(input))
        .map(|e| e.code)
        .collect()
}

#[test]
fn ty_14_palette() {
    assert!(lit_errors("palette", "palette", json!(["#000000", "#ffffff"])).is_empty());
    assert_eq!(lit_errors("palette", "palette", json!([])), vec!["out_of_range"]);
    assert_eq!(lit_errors("palette", "palette", json!(["red"])), vec!["type_mismatch"]);
    assert_eq!(lit_errors("palette", "palette", json!(vec!["#000000"; 257])), vec!["out_of_range"]);
}

#[test]
fn ty_15_16_gradient() {
    assert!(lit_errors("gradient", "gradient", json!({ "type": "rgb", "keys": [{ "time": 0, "color": "#000000" }] })).is_empty());
    assert_eq!(lit_errors("gradient", "gradient", json!({ "type": "lab", "keys": [{ "time": 0, "color": "#000000" }] })), vec!["invalid_enum"]);
    assert_eq!(lit_errors("gradient", "gradient", json!({ "type": "rgb", "keys": [{ "time": 2, "color": "#000000" }] })), vec!["out_of_range"]);
    assert_eq!(lit_errors("gradient", "gradient", json!({ "type": "rgb", "keys": [] })), vec!["out_of_range"]);
    assert_eq!(lit_errors("gradient", "gradient", json!({ "type": "rgb", "keys": [{ "time": 0 }] })), vec!["type_mismatch"]);
}

#[test]
fn ty_17_value_refs() {
    let e = errors(&project_json(
        4,
        4,
        json!({
            "c": { "type": "color" }, "v": { "type": "number" },
            "s": { "type": "solid", "inputs": { "color": { "ref": "c" } } },
            "b": { "type": "blend", "inputs": { "background": { "ref": "s" }, "opacity": { "ref": "v" } } }
        }),
    ));
    assert!(e.is_empty(), "{e:?}");
}

#[test]
fn ty_18_padding() {
    assert!(lit_errors("crop", "crop", json!([1, 0, 1, 0])).is_empty());
    assert_eq!(lit_errors("crop", "crop", json!([1, 2])), vec!["type_mismatch"]);
    assert_eq!(lit_errors("crop", "crop", json!([-1, 0, 0, 0])), vec!["out_of_range"]);
}

#[test]
fn ev_10_ref_clamped() {
    let nodes = |opacity: Json| {
        json!({
            "bg": { "type": "solid", "inputs": { "color": "#000000" } },
            "fg": { "type": "solid", "inputs": { "color": "#ffffff" } },
            "v": { "type": "number", "inputs": { "value": 5 } },
            "b": { "type": "blend", "inputs": { "background": { "ref": "bg" }, "foreground": { "ref": "fg" }, "opacity": opacity } }
        })
    };
    assert_eq!(eval(2, 2, nodes(json!({ "ref": "v" })), "b").unwrap(), eval(2, 2, nodes(json!(1)), "b").unwrap());
}

#[test]
fn ty_19_gradient_blends() {
    let grad = |ty: &str, to: &str| {
        let nodes = json!({
            "s": { "type": "solid", "inputs": { "color": "#808080" } },
            "c": { "type": "colorize", "inputs": { "surface_in": { "ref": "s" }, "gradient": { "type": ty, "keys": [{ "time": 0, "color": "#ff0000" }, { "time": 1, "color": to }] } } }
        });
        eval(2, 2, nodes, "c").unwrap().pixel(0, 0)
    };
    for (ty, to, want) in [
        ("rgb", "#0000ff", [127, 0, 128, 255]),
        ("constant", "#0000ff", [255, 0, 0, 255]),
        ("hsv", "#0000ff", [254, 0, 255, 255]),
        ("hsv_inverted", "#0000ff", [0, 255, 2, 255]),
        ("oklab", "#0000ff", [139, 84, 161, 255]),
        ("cmyk", "#000000", [127, 64, 64, 255]),
    ] {
        assert_close(grad(ty, to), want, 2);
    }
}

// ---------- area 3: remaining inputs (batch 1) ----------

#[test]
fn n_offset_03_angle() {
    let f = fixture(4, 4);
    let s = ok(&f, "offset", json!({ "x_offset": 0.25, "angle": 90 }));
    for y in 0..4 {
        assert_eq!(s.pixel(1, y), f.pixel(1, (y + 3) % 4), "y {y}");
    }
}

#[test]
fn n_downscale_05_multiply_alpha() {
    let src = surface(2, 1, |x, _| if x == 0 { [200, 0, 0, 255] } else { [0, 0, 200, 128] });
    let s = ok(&src, "downscale", json!({ "downscale": 2, "multiply_alpha": true }));
    assert_close(s.pixel(0, 0), [133, 0, 34, 160], 1);
}

#[test]
fn n_scale_05_fit_mode() {
    let f = fixture(4, 2);
    let size = |fit: &str| {
        let s = ok(&f, "scale", json!({ "mode": "scale_to_size", "target_dimension": [8, 8], "fit_mode": fit }));
        (s.width, s.height)
    };
    assert_eq!((size("keep_aspect_minimum"), size("keep_aspect_maximum")), ((8, 4), (16, 8)));
}

#[test]
fn n_crop_04_05_aspect() {
    let f = fixture(4, 4);
    let s = ok(&f, "crop", json!({ "aspect_ratio": "1:1", "width": 2 }));
    assert_eq!((s.width, s.height), (2, 2));
    assert_eq!((s.pixel(0, 0), s.pixel(1, 1)), (f.pixel(1, 1), f.pixel(2, 2)));
    let f = fixture(4, 2);
    let s = ok(&f, "crop", json!({ "aspect_ratio": "1:1", "fit_mode": "width" }));
    assert_eq!((s.width, s.height), (4, 4));
    assert_eq!((s.pixel(0, 0)[3], s.pixel(2, 1), s.pixel(3, 2), s.pixel(1, 3)[3]), (0, f.pixel(2, 0), f.pixel(3, 1), 0));
}

#[test]
fn n_value_08_palette_trim() {
    let pal = json!(["#000000", "#ff0000", "#00ff00", "#ffffff"]);
    let v = value(json!({ "n": { "type": "palette", "inputs": { "palette": pal, "trim_range": [0.25, 0.75] } } }), "n", 1, 0).unwrap();
    assert_eq!(v, Value::Palette(vec![[255, 0, 0, 255], [0, 255, 0, 255]]));
    assert!(value(json!({ "n": { "type": "palette", "inputs": { "palette": pal, "trim_range": [0.5, 0.5] } } }), "n", 1, 0).is_err());
}

#[test]
fn n_value_09_counter_async_reset() {
    let reset = json!({ "anim": [{ "time": 0, "value": false }, { "time": 3, "value": true }, { "time": 4, "value": false }] });
    let at = |f: i64| match value(json!({ "n": { "type": "counter", "inputs": { "async": true, "reset": reset } } }), "n", 10, f).unwrap() {
        Value::Number(v) => v,
        v => panic!("{v:?}"),
    };
    assert_eq!((at(2), at(3), at(5)), (3.0, 1.0, 3.0));
}

#[test]
fn n_value_10_math_more() {
    let m = |inputs: Json| num(json!({ "n": { "type": "math", "inputs": inputs } }));
    assert_eq!(m(json!({ "type": "lerp", "a": 2, "b": 6, "amount": 0.25 })), 3.0);
    assert_eq!(m(json!({ "type": "clamp", "a": 5, "b": 0, "amount": 3 })), 3.0);
    assert_eq!(m(json!({ "type": "map", "a": 5, "from": [0, 10] })), 0.5);
    assert_eq!(m(json!({ "type": "map", "a": 15, "from": [0, 10], "clamp_output": true })), 1.0);
    assert!((m(json!({ "type": "sin", "a": 90, "b": 2 })) - 2.0).abs() < 1e-9);
    assert_eq!(m(json!({ "type": "root", "a": 8, "b": 0 })), 0.0);
}

#[test]
fn n_value_11_number_integer() {
    assert_eq!(num(json!({ "n": { "type": "number", "inputs": { "value": 2.6, "integer": true } } })), 3.0);
}
