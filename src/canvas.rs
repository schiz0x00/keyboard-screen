//! A 128x128 RGB888 canvas with just enough drawing primitives, plus the
//! RGB565 little-endian conversion the keyboard expects.

use crate::font;

pub const W: usize = 128;
pub const H: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub struct Canvas {
    pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(bg: Rgb) -> Self {
        Self {
            pixels: vec![bg; W * H],
        }
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
            let v =
                (((px.0 as u16) >> 3) << 11) | (((px.1 as u16) >> 2) << 5) | ((px.2 as u16) >> 3);
            out[i * 2] = (v & 0xff) as u8;
            out[i * 2 + 1] = (v >> 8) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_rgb565_little_endian() {
        let mut c = Canvas::new(Rgb(0, 0, 0));
        c.set(0, 0, Rgb(0xff, 0x00, 0x00)); // red   -> 0xf800
        c.set(1, 0, Rgb(0x00, 0xff, 0x00)); // green -> 0x07e0
        c.set(2, 0, Rgb(0x00, 0x00, 0xff)); // blue  -> 0x001f
        c.set(3, 0, Rgb(0xff, 0xff, 0xff)); // white -> 0xffff

        let mut out = vec![0u8; W * H * 2];
        c.to_rgb565(&mut out);
        assert_eq!(&out[0..2], &[0x00, 0xf8]);
        assert_eq!(&out[2..4], &[0xe0, 0x07]);
        assert_eq!(&out[4..6], &[0x1f, 0x00]);
        assert_eq!(&out[6..8], &[0xff, 0xff]);
    }

    #[test]
    fn drawing_outside_the_canvas_is_ignored() {
        let mut c = Canvas::new(Rgb(0, 0, 0));
        c.set(-1, 0, Rgb(255, 255, 255));
        c.set(0, -1, Rgb(255, 255, 255));
        c.set(W as i32, 0, Rgb(255, 255, 255));
        c.fill_rect(-10, -10, 5, 5, Rgb(255, 255, 255));
        let mut out = vec![0u8; W * H * 2];
        c.to_rgb565(&mut out);
        assert!(
            out.iter().all(|b| *b == 0),
            "nothing should have been drawn"
        );
    }

    #[test]
    fn fill_rect_covers_exactly_its_bounds() {
        let mut c = Canvas::new(Rgb(0, 0, 0));
        c.fill_rect(2, 3, 4, 5, Rgb(255, 255, 255));
        assert_eq!(c.get(2, 3), Rgb(255, 255, 255));
        assert_eq!(c.get(5, 7), Rgb(255, 255, 255));
        assert_eq!(c.get(1, 3), Rgb(0, 0, 0));
        assert_eq!(c.get(6, 7), Rgb(0, 0, 0));
        assert_eq!(c.get(2, 8), Rgb(0, 0, 0));
    }
}
