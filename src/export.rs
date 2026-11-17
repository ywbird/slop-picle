//! Export frames as PNG images or sequences, GIF and APNG (spec/export.md).

use crate::cli::render_surface;
use crate::eval::Evaluator;
use crate::project::{self, Error, Project};
use crate::sandbox;
use crate::surface::Surface;
use serde::Deserialize;
use std::path::Path;

/// Export options of the CLI, MCP and the server; absent values take the defaults of spec/export.md.
#[derive(Default, Deserialize, schemars::JsonSchema, clap::Args)]
pub struct Options {
    /// Node to export (`id` or `id.output`); default: the single `output` node.
    #[arg(long)]
    pub node: Option<String>,
    /// First and last frame, counting from 1, inclusive (CLI: A:B); default: all frames.
    #[arg(long, value_parser = parse_range)]
    pub range: Option<[i64; 2]>,
    /// Use every n-th frame of the range (default 1).
    #[arg(long, value_parser = clap::value_parser!(i64).range(1..))]
    pub step: Option<i64>,
    /// Added to the number written for %f (default 0).
    #[arg(long, allow_negative_numbers = true)]
    pub begin: Option<i64>,
    /// Integer upscale 1-32 (default 1).
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=32))]
    pub scale: Option<u32>,
    /// Frames per second of a GIF or APNG; default: the project's.
    #[arg(long)]
    pub fps: Option<f64>,
    /// The frame of a single PNG, from 0 (default 0).
    #[arg(long, allow_negative_numbers = true)]
    pub frame: Option<i64>,
}

fn parse_range(s: &str) -> Result<[i64; 2], String> {
    let bad = || format!("'{s}' is not A:B");
    let (a, b) = s.split_once(':').ok_or_else(bad)?;
    Ok([a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?])
}

pub struct Exported {
    /// Written files, as formatted from `out`.
    pub files: Vec<String>,
    /// The frames exported.
    pub frames: Vec<i64>,
}

enum Piece {
    Text(String),
    /// `%f` or `%Nf`: the frame number padded with zeros to N digits.
    Frame(usize),
}

fn template(out: &str) -> Result<Vec<Piece>, Error> {
    let bad = |m: &str| Error::new("bad_path", format!("'{out}': {m}"));
    let (mut pieces, mut text, mut chars) = (vec![], String::new(), out.chars().peekable());
    while let Some(c) = chars.next() {
        if c != '%' {
            text.push(c);
            continue;
        }
        let mut digits = String::new();
        while let Some(d) = chars.next_if(char::is_ascii_digit) {
            digits.push(d);
        }
        match chars.next() {
            Some('%') if digits.is_empty() => text.push('%'),
            Some('f') => {
                let pad = if digits.is_empty() { 0 } else { digits.parse().map_err(|_| bad("frame padding too large"))? };
                pieces.push(Piece::Text(std::mem::take(&mut text)));
                pieces.push(Piece::Frame(pad));
            }
            _ => return Err(bad("unknown % code; use %f, %3f or %%")),
        }
    }
    pieces.push(Piece::Text(text));
    Ok(pieces)
}

fn file_name(t: &[Piece], n: i64) -> String {
    t.iter()
        .map(|p| match p {
            Piece::Text(s) => s.clone(),
            Piece::Frame(w) => format!("{n:0w$}"),
        })
        .collect()
}

/// Render frames of a node and encode them under names formatted from `out`; `write` stores each file.
pub fn export(p: &Project, dir: &Path, out: &str, o: &Options, ev: &mut Evaluator, mut write: impl FnMut(&str, &[u8]) -> Result<(), Error>) -> Result<Exported, Error> {
    let t = template(out)?;
    let seq = t.iter().any(|p| matches!(p, Piece::Frame(_)));
    let lower = out.to_ascii_lowercase();
    let ext = [".png", ".gif", ".apng"].into_iter().find(|e| lower.ends_with(e));
    if ext.is_none() || (seq && ext != Some(".png")) {
        return Err(Error::new("bad_path", format!("'{out}' must end in .png (with %f for a sequence), .gif or .apng")));
    }
    let range = |m: String| Error::new("out_of_range", m);
    let scale = o.scale.unwrap_or(1);
    if !(1..=32).contains(&scale) {
        return Err(range(format!("scale is {scale}, outside 1..32")));
    }
    let mut render = |f: i64| render_surface(p, dir, o.node.as_deref(), f, scale, ev);
    if ext == Some(".png") && !seq {
        let (f, name) = (o.frame.unwrap_or(0), file_name(&t, 0));
        write(&name, &render(f)?.to_png())?;
        return Ok(Exported { files: vec![name], frames: vec![f] });
    }
    let (step, total) = (o.step.unwrap_or(1), p.settings.frames);
    let [a, b] = o.range.unwrap_or([1, total]);
    if step < 1 {
        return Err(range(format!("step is {step}, must be at least 1")));
    }
    if a < 1 || a > b || b > total {
        return Err(range(format!("range {a}:{b} is outside 1:{total}")));
    }
    let frames: Vec<i64> = (a - 1..b).step_by(step as usize).collect();
    if seq {
        let mut files = vec![];
        for &f in &frames {
            let name = file_name(&t, f + 1 + o.begin.unwrap_or(0));
            write(&name, &render(f)?.to_png())?;
            files.push(name);
        }
        return Ok(Exported { files, frames });
    }
    let fps = o.fps.unwrap_or_else(|| p.settings.fps());
    if !(fps > 0.0 && fps.is_finite()) {
        return Err(range(format!("fps is {fps}, must be above 0")));
    }
    let bytes = animation(ext == Some(".gif"), frames.iter().map(|&f| render(f)), frames.len() as u32, fps)?;
    let name = file_name(&t, 0);
    write(&name, &bytes)?;
    Ok(Exported { files: vec![name], frames })
}

/// One GIF or APNG of `n` frames, encoded as they are rendered.
fn animation(gif: bool, frames: impl Iterator<Item = Result<Surface, Error>>, n: u32, fps: f64) -> Result<Vec<u8>, Error> {
    let mut frames = frames.peekable();
    let (w, h) = match frames.peek() {
        Some(Ok(s)) => (s.width, s.height),
        Some(Err(e)) => return Err(e.clone()),
        None => unreachable!("a range holds at least one frame"),
    };
    let same = |s: Surface| {
        if (s.width, s.height) == (w, h) {
            Ok(s)
        } else {
            Err(Error::new("eval_error", format!("a frame is {}x{} but the first is {w}x{h}; an animation needs one size", s.width, s.height)))
        }
    };
    let fail = |e: &dyn std::fmt::Display| Error::new("eval_error", format!("cannot encode: {e}"));
    let mut bytes = Vec::new();
    if gif {
        if w > u16::MAX as u32 || h > u16::MAX as u32 {
            return Err(Error::new("out_of_range", format!("GIF frames are at most 65535 px per side, these are {w}x{h}")));
        }
        let mut enc = gif::Encoder::new(&mut bytes, w as u16, h as u16, &[]).map_err(|e| fail(&e))?;
        enc.set_repeat(gif::Repeat::Infinite).map_err(|e| fail(&e))?;
        let delay = (100.0 / fps).round().clamp(1.0, u16::MAX as f64) as u16;
        for s in frames {
            let mut s = same(s?)?;
            for px in s.data.chunks_exact_mut(4) {
                px[3] = if px[3] < 128 { 0 } else { 255 };
            }
            // Exact palette up to 256 colours (the transparent one included), NeuQuant above.
            let mut f = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut s.data, 10);
            f.delay = delay;
            f.dispose = gif::DisposalMethod::Background;
            enc.write_frame(&f).map_err(|e| fail(&e))?;
        }
        enc.into_inner().map_err(|e| fail(&e))?;
    } else {
        let mut enc = png::Encoder::new(&mut bytes, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_animated(n, 0).map_err(|e| fail(&e))?;
        let (num, den) = if fps.fract() == 0.0 && fps <= u16::MAX as f64 { (1, fps as u16) } else { ((1000.0 / fps).round().min(u16::MAX as f64) as u16, 1000) };
        enc.set_frame_delay(num, den).map_err(|e| fail(&e))?;
        let mut wr = enc.write_header().map_err(|e| fail(&e))?;
        for s in frames {
            wr.write_image_data(&same(s?)?.data).map_err(|e| fail(&e))?;
        }
        wr.finish().map_err(|e| fail(&e))?;
    }
    Ok(bytes)
}

/// Write a file atomically, creating missing parent directories; errors name the file as `shown`.
pub fn write_file(path: &Path, shown: &str, bytes: &[u8]) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::new("io_error", format!("{shown}: {e}"));
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    project::write_atomic(path, bytes).map_err(io)
}

/// Export from a project file inside `root` (MCP and server): `out` and the returned names are relative to the root,
/// and every file must stay inside it.
pub fn export_in_root(root: &Path, path: &Path, out: &str, o: &Options, ev: &mut Evaluator) -> Result<Exported, Vec<Error>> {
    sandbox::resolve(root, out).map_err(|e| vec![e])?;
    let l = project::load(path).map_err(|e| vec![Error::new("io_error", e.to_string())])?;
    let blocking: Vec<Error> = l.blocking().into_iter().cloned().collect();
    let p = l.project.filter(|_| blocking.is_empty()).ok_or(blocking)?;
    export(&p, path.parent().unwrap(), out, o, ev, |name, bytes| write_file(&sandbox::resolve(root, name)?, name, bytes)).map_err(|e| vec![e])
}
