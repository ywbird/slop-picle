/// An RGBA8 image with straight alpha, rows top to bottom.
#[derive(Clone, PartialEq, Eq)]
pub struct Surface {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl std::fmt::Debug for Surface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Surface({}x{})", self.width, self.height)
    }
}

pub const MAX_SIDE: u32 = 4096;
pub const MAX_AREA: u64 = 2048 * 2048;

pub fn size_ok(w: u32, h: u32) -> bool {
    (1..=MAX_SIDE).contains(&w) && (1..=MAX_SIDE).contains(&h) && (w as u64) * (h as u64) <= MAX_AREA
}

impl Surface {
    pub fn new(width: u32, height: u32) -> Self {
        Surface { width, height, data: vec![0; (width * height * 4) as usize] }
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.data[i..i + 4].try_into().unwrap()
    }

    /// Decode a PNG of any colour type and bit depth to RGBA8 (16-bit keeps the high byte).
    pub fn from_png(bytes: &[u8]) -> Result<Surface, String> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = dec.read_info().map_err(|e| format!("not a PNG: {e}"))?;
        let (w, h) = (reader.info().width, reader.info().height);
        if !size_ok(w, h) {
            return Err(format!("image is {w}x{h}, over the size limit"));
        }
        let mut buf = vec![0; reader.output_buffer_size().ok_or("image too large")?];
        let info = reader.next_frame(&mut buf).map_err(|e| format!("cannot decode PNG: {e}"))?;
        let buf = &buf[..info.buffer_size()];
        let data: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => buf.to_vec(),
            png::ColorType::Rgb => buf.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => buf.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
            png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err("unexpanded palette PNG".into()),
        };
        Ok(Surface { width: w, height: h, data })
    }

    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&self.data).unwrap();
        out
    }

    /// Nearest-neighbour upscale by an integer factor.
    pub fn upscale(&self, k: u32) -> Surface {
        if k == 1 {
            return self.clone();
        }
        let mut out = Surface::new(self.width * k, self.height * k);
        for y in 0..out.height {
            for x in 0..out.width {
                let i = ((y * out.width + x) * 4) as usize;
                out.data[i..i + 4].copy_from_slice(&self.pixel(x / k, y / k));
            }
        }
        out
    }
}
