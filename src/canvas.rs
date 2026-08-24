//! A 128x128 RGB888 canvas with just enough drawing primitives, plus the
//! RGB565 little-endian conversion the keyboard expects.

use crate::font;

pub const W: usize = 128;
pub const H: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub struct Canvas {
    pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(bg: Rgb) -> Self {
        Self { pixels: vec![bg; W * H] }
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.pixels[y as usize * W + x as usize] = c;
        }
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> Rgb {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.pixels[y as usize * W + x as usize]
        } else {
            Rgb(0, 0, 0)
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for dy in 0..h {
            for dx in 0..w {
                self.set(x + dx, y + dy, c);
            }
        }
    }

    /// Filled rectangle with rounded corners - used for the progress bars.
    pub fn fill_round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: Rgb) {
        if w <= 0 || h <= 0 {
            return;
        }
        let r = r.min(w / 2).min(h / 2);
        for dy in 0..h {
            for dx in 0..w {
                // Distance test only inside the four corner boxes.
                let cx = if dx < r {
                    r - dx
                } else if dx >= w - r {
                    dx - (w - r - 1)
                } else {
                    0
                };
                let cy = if dy < r {
                    r - dy
                } else if dy >= h - r {
                    dy - (h - r - 1)
                } else {
                    0
                };
                if cx * cx + cy * cy <= r * r {
                    self.set(x + dx, y + dy, c);
                }
            }
        }
    }

    /// Draw `text` with the 5x7 font, upscaled by `scale`. Returns end x.
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: usize, c: Rgb) -> i32 {
        let mut cx = x;
        for ch in text.bytes() {
            let g = font::glyph(ch);
            for (col, mask) in g.iter().enumerate() {
                for row in 0..font::GLYPH_H {
                    if mask & (1 << row) != 0 {
                        self.fill_rect(
                            cx + (col * scale) as i32,
                            y + (row * scale) as i32,
                            scale as i32,
                            scale as i32,
                            c,
                        );
                    }
                }
            }
            cx += ((font::GLYPH_W + 1) * scale) as i32;
        }
        cx - scale as i32
    }

    pub fn text_right(&mut self, right: i32, y: i32, text: &str, scale: usize, c: Rgb) {
        let x = right - font::text_width(text, scale) as i32;
        self.text(x, y, text, scale, c);
    }

    /// Pack to RGB565 little-endian, the wire format for the screen.
    pub fn to_rgb565(&self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), W * H * 2);
        for (i, px) in self.pixels.iter().enumerate() {
            let v = (((px.0 as u16) >> 3) << 11)
                | (((px.1 as u16) >> 2) << 5)
                | ((px.2 as u16) >> 3);
            out[i * 2] = (v & 0xff) as u8;
            out[i * 2 + 1] = (v >> 8) as u8;
        }
    }
}
