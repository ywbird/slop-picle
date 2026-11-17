//! Render Spritesheet (spec/export.md#render-spritesheet-node). Upstream: `scripts/node_render_sprite_sheet`.

use super::{R, out};
use crate::catalog::{Default as D, InputDef as I};
use crate::eval::Ctx;
use crate::surface::{self, Surface};
use crate::types::{Type as T, Value};

pub const SHEET_IN: &[I] = &[
    I::required("sprites", T::Surface, "Animation to lay out; read at every frame used."),
    I::new("frame_step", T::Int, D::Int(1), "Use every n-th frame.").min(1.0),
    I::new("skip_empty", T::Bool, D::Bool(false), "Leave out fully transparent frames."),
    I::new("packing_type", T::Enum, D::Enum("horizontal"), "One row, one column, or a grid.").options(&["horizontal", "vertical", "grid"]),
    I::new("grid_column", T::Int, D::Int(4), "Columns of the grid.").min(1.0),
    I::new("alignment", T::Enum, D::Enum("first"), "Where a smaller frame sits across the row or column.").options(&["first", "middle", "last"]),
    I::new("spacing", T::Int, D::Int(0), "Pixels between frames (horizontal and vertical packing).").min(0.0),
    I::new("grid_spacing", T::Vec2, D::Vec2([0.0, 0.0]), "Pixels between columns and rows (grid packing).").min(0.0),
    I::new("padding", T::Padding, D::Json("[0,0,0,0]"), "Border: right, top, left, bottom."),
    I::new("custom_range", T::Bool, D::Bool(false), "Use only the frames in range."),
    I::new("range", T::Vec2, D::Vec2([0.0, 0.0]), "First and last frame, counting from 1; 0 means first or last, negative counts from the end."),
];

/// The frames read: from start to end every `frame_step`.
pub fn sheet_frames(v: &[Value], total: i64) -> Vec<i64> {
    let (mut a, mut b) = (0, total - 1);
    if v[9] == Value::Bool(true) {
        let at = |x: f64, zero: i64| match x.round() as i64 {
            0 => zero,
            x if x < 0 => total + x,
            x => x - 1,
        };
        let r = v[10].vec2();
        (a, b) = (at(r[0], 0).max(0), at(r[1], total - 1).min(total - 1));
    }
    (a..=b).step_by(v[1].num().max(1.0) as usize).collect()
}

pub fn render_sprite_sheet(ctx: &mut Ctx, v: &[Value]) -> R {
    let mut cells: Vec<&Surface> = ctx.sources.iter().filter_map(Value::surface).map(|s| &**s).collect();
    if v[2] == Value::Bool(true) {
        cells.retain(|s| s.data.chunks_exact(4).any(|p| p[3] > 0));
    }
    let (Some(cw), Some(ch)) = (cells.iter().map(|s| s.width).max(), cells.iter().map(|s| s.height).max()) else {
        return out(Surface::new(1, 1));
    };
    let Value::Padding([right, top, left, bottom]) = v[8] else { unreachable!() };
    let n = cells.len() as f64;
    // Columns, rows and the gaps between them, in f64 so absurd inputs fail the size check instead of overflowing.
    let (cols, rows, gx, gy) = match v[3] {
        Value::Enum(0) => (n, 1.0, v[6].num(), 0.0),
        Value::Enum(1) => (1.0, n, 0.0, v[6].num()),
        _ => (v[4].num(), (n / v[4].num()).ceil(), v[7].vec2()[0].floor(), v[7].vec2()[1].floor()),
    };
    let (cw, ch) = (cw as f64, ch as f64);
    let w = cols * cw + (cols - 1.0) * gx + (left + right) as f64;
    let h = rows * ch + (rows - 1.0) * gy + (top + bottom) as f64;
    if !(w <= surface::MAX_SIDE as f64 && h <= surface::MAX_SIDE as f64 && surface::size_ok(w as u32, h as u32)) {
        return Err(format!("sheet would be {w}x{h}, outside the size limits").into());
    }
    let mut sheet = Surface::new(w as u32, h as u32);
    for (i, s) in cells.iter().enumerate() {
        let (col, row) = ((i as f64 % cols).floor(), (i as f64 / cols).floor());
        let align = |free: f64| match v[5] {
            Value::Enum(1) => (free / 2.0).floor(),
            Value::Enum(2) => free,
            _ => 0.0,
        };
        let (dx, dy) = match v[3] {
            Value::Enum(0) => (0.0, align(ch - s.height as f64)),
            Value::Enum(1) => (align(cw - s.width as f64), 0.0),
            _ => (0.0, 0.0),
        };
        let x = (left as f64 + col * (cw + gx) + dx) as usize;
        let y = (top as f64 + row * (ch + gy) + dy) as usize;
        let row_bytes = s.width as usize * 4;
        for (r, src) in s.data.chunks_exact(row_bytes).enumerate() {
            let at = ((y + r) * sheet.width as usize + x) * 4;
            sheet.data[at..at + row_bytes].copy_from_slice(src);
        }
    }
    out(sheet)
}
