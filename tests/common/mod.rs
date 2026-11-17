#![allow(dead_code)]

use serde_json::{Value as Json, json};
use picle::eval::Evaluator;
use picle::project::{self, Error};
use picle::surface::Surface;
use std::path::{Path, PathBuf};

pub fn project_json(width: u32, height: u32, nodes: Json) -> String {
    json!({ "picle": 1, "settings": { "width": width, "height": height }, "nodes": nodes }).to_string()
}

pub fn errors(text: &str) -> Vec<Error> {
    project::load_str(text).errors
}

pub fn codes(text: &str) -> Vec<String> {
    errors(text).into_iter().map(|e| e.code).collect()
}

pub fn has(errors: &[Error], code: &str, node: Option<&str>, input: Option<&str>) -> bool {
    errors
        .iter()
        .any(|e| e.code == code && e.node.as_deref() == node && (input.is_none() || e.input.as_deref() == input))
}

/// Evaluate `target` in a project built from `nodes`, with files resolved against `dir`.
pub fn eval_in(dir: &Path, width: u32, height: u32, nodes: Json, target: &str) -> Result<Surface, Error> {
    let text = project_json(width, height, nodes);
    let loaded = project::load_str(&text);
    let p = loaded.project.expect("project parses");
    let v = Evaluator::new().eval(&p, dir, target, 0)?;
    Ok(v.into_surface().expect("surface"))
}

pub fn eval(width: u32, height: u32, nodes: Json, target: &str) -> Result<Surface, Error> {
    eval_in(Path::new("."), width, height, nodes, target)
}

pub fn surface(width: u32, height: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Surface {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            data.extend_from_slice(&f(x, y));
        }
    }
    Surface { width, height, data }
}

/// A 4×4 fixture where every pixel is distinct and opaque: (x*60, y*60, 7*x+y, 255).
pub fn fixture(width: u32, height: u32) -> Surface {
    surface(width, height, |x, y| [(x * 60) as u8, (y * 60) as u8, (7 * x + y) as u8, 255])
}

pub fn write_png(dir: &Path, name: &str, s: &Surface) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, s.to_png()).unwrap();
    p
}

/// Evaluate `nodes` where node `src` is an `image` reading `s`.
pub fn eval_on(s: &Surface, mut nodes: Json, target: &str) -> Result<Surface, Error> {
    let dir = tempfile::tempdir().unwrap();
    write_png(dir.path(), "src.png", s);
    nodes["src"] = json!({ "type": "image", "inputs": { "path": "src.png" } });
    eval_in(dir.path(), s.width, s.height, nodes, target)
}

pub fn assert_close(actual: [u8; 4], expected: [u8; 4], tol: u8) {
    for c in 0..4 {
        let d = (actual[c] as i16 - expected[c] as i16).unsigned_abs();
        assert!(d <= tol as u16, "pixel {actual:?} != {expected:?} (tolerance {tol})");
    }
}

pub fn assert_all(s: &Surface, expected: [u8; 4], tol: u8) {
    for y in 0..s.height {
        for x in 0..s.width {
            assert_close(s.pixel(x, y), expected, tol);
        }
    }
}

pub fn golden_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(name)
}

/// Compare with a golden image. `PICLE_UPDATE_GOLDEN=1` rewrites it.
pub fn assert_golden(name: &str, s: &Surface, tol: u8) {
    let path = golden_path(name);
    if std::env::var("PICLE_UPDATE_GOLDEN").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, s.to_png()).unwrap();
        return;
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|_| panic!("missing golden {name}; run with PICLE_UPDATE_GOLDEN=1"));
    let g = Surface::from_png(&bytes).unwrap();
    assert_eq!((g.width, g.height), (s.width, s.height), "golden {name} size");
    for y in 0..s.height {
        for x in 0..s.width {
            assert_close(s.pixel(x, y), g.pixel(x, y), tol);
        }
    }
}

/// PCG hash from spec/nodes.md, for CPU reference values.
pub fn hash(v: u32) -> u32 {
    let state = v.wrapping_mul(747796405).wrapping_add(2891336453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277803737);
    (word >> 22) ^ word
}

pub fn hash2(x: i64, y: i64, seed: i64) -> u32 {
    hash((x as u32) ^ hash((y as u32) ^ hash(seed as u32)))
}

pub fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16777216.0
}

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_picle")
}
