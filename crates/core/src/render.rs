//! Pixel buffer plus an 8x8 bitmap font. The canvas is the Mini+'s native
//! 640x480, so the device backend copies it 1:1.

use font8x8::{UnicodeFonts, BASIC_FONTS, LATIN_FONTS};

pub const W: usize = 640;
pub const H: usize = 480;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    /// `0x00RRGGBB` pixels, row by row.
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![0; w * h] }
    }

    pub fn fill(&mut self, c: u32) {
        self.px.fill(c);
    }

    /// Filled rectangle, clipped to the canvas.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let x0 = x.max(0) as usize;
        let y0 = y.max(0) as usize;
        let x1 = ((x + w).max(0) as usize).min(self.w);
        let y1 = ((y + h).max(0) as usize).min(self.h);
        if x0 >= x1 {
            return;
        }
        for yy in y0..y1 {
            self.px[yy * self.w + x0..yy * self.w + x1].fill(c);
        }
    }

    pub fn glyph(&mut self, x: i32, y: i32, ch: char, scale: usize, c: u32) {
        let s = scale as i32;
        for (row, bits) in glyph_bits(ch).iter().enumerate() {
            for col in 0..8 {
                if bits >> col & 1 == 1 {
                    self.rect(x + col * s, y + row as i32 * s, s, s, c);
                }
            }
        }
    }

    /// Draws `text` starting at (x, y). Returns the x just past the last character.
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: usize, c: u32) -> i32 {
        let mut x = x;
        for ch in text.chars() {
            self.glyph(x, y, ch, scale, c);
            x += 8 * scale as i32;
        }
        x
    }
}

/// Bitmap for `ch` (bit 0 of each row byte is the leftmost pixel). Unknown
/// characters show as `?`.
pub fn glyph_bits(ch: char) -> [u8; 8] {
    BASIC_FONTS
        .get(ch)
        .or_else(|| LATIN_FONTS.get(ch))
        .or_else(|| BASIC_FONTS.get('?'))
        .unwrap_or([0; 8])
}

/// First `n` characters of `s`, ending in ".." if it was cut.
pub fn fit(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let keep = n.saturating_sub(2);
    s.chars().take(keep).collect::<String>() + &"..".chars().take(n).collect::<String>()
}

/// Last `n` characters of `s`, starting with ".." if it was cut.
pub fn fit_end(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    let keep = n.saturating_sub(2);
    "..".chars().take(n).collect::<String>() + &s.chars().skip(count - keep).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_text_inside_canvas_only() {
        let mut c = Canvas::new(32, 16);
        let end = c.text(-4, -4, "Hi?\u{e9}\u{4e2d}", 2, 0xFFFFFF);
        assert_eq!(end, -4 + 5 * 16);
        assert!(c.px.iter().any(|&p| p != 0));
    }

    #[test]
    fn fits_strings() {
        assert_eq!(fit("hello", 10), "hello");
        assert_eq!(fit("hello world", 7), "hello..");
        assert_eq!(fit_end("https://example.com/x", 8), "...com/x");
        assert_eq!(fit_end("abc", 3), "abc");
    }
}
