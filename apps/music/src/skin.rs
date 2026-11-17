//! Drawing helpers for the classic desktop-player look: bevelled metal panels,
//! striped title bars, seven-segment LED digits, transport icons and sliders.
//! All original artwork, drawn in code.

use pocketkit_core::render::{glyph_bits, Canvas};

pub const FACE: u32 = 0x3A3D4C;
pub const FACE_LIGHT: u32 = 0x6A6F86;
pub const FACE_DARK: u32 = 0x17181F;
pub const FACE_TEXT: u32 = 0xC9CEDD;
pub const FACE_TEXT_DIM: u32 = 0x8C91A6;
pub const TITLE_HI: u32 = 0x3D4E9C;
pub const TITLE_LO: u32 = 0x161C3E;
pub const TITLE_TEXT: u32 = 0xE8ECFF;
pub const LCD_BG: u32 = 0x000000;
pub const LCD_ON: u32 = 0x19E64A;
pub const LCD_OFF: u32 = 0x0B2A12;
pub const LCD_DIM: u32 = 0x0F7A2C;
pub const PEAK: u32 = 0xB8BCC8;
pub const SEL: u32 = 0x1F3C9C;
pub const WHITE: u32 = 0xFFFFFF;

/// A raised (or pressed) panel/button.
pub fn bevel(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, pressed: bool) {
    let (tl, br) = if pressed { (FACE_DARK, FACE_LIGHT) } else { (FACE_LIGHT, FACE_DARK) };
    c.rect(x, y, w, h, if pressed { 0x2E3140 } else { FACE });
    c.rect(x, y, w, 1, tl);
    c.rect(x, y, 1, h, tl);
    c.rect(x, y + h - 1, w, 1, br);
    c.rect(x + w - 1, y, 1, h, br);
}

/// A sunken display area.
pub fn inset(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, fill: u32) {
    c.rect(x, y, w, h, fill);
    c.rect(x - 1, y - 1, w + 2, 1, FACE_DARK);
    c.rect(x - 1, y - 1, 1, h + 2, FACE_DARK);
    c.rect(x - 1, y + h, w + 2, 1, FACE_LIGHT);
    c.rect(x + w, y - 1, 1, h + 2, FACE_LIGHT);
}

/// Window title bar: vertical gradient, ridged stripes either side of the name.
pub fn title_bar(c: &mut Canvas, y: i32, w: i32, text: &str) {
    let h = 16;
    for i in 0..h {
        let t = i as u32 * 255 / (h as u32 - 1);
        c.rect(0, y + i, w, 1, mix(TITLE_HI, TITLE_LO, t));
    }
    let tw = text.chars().count() as i32 * 8;
    let tx = (w - tw) / 2;
    for stripe in 0..3 {
        let sy = y + 4 + stripe * 3;
        for (x0, x1) in [(6, tx - 8), (tx + tw + 8, w - 6)] {
            c.rect(x0, sy, x1 - x0, 1, 0x7B86C8);
            c.rect(x0, sy + 1, x1 - x0, 1, 0x0B0F24);
        }
    }
    c.text(tx, y + 4, text, 1, TITLE_TEXT);
}

/// Blends two colours; `t` 0 = a, 255 = b.
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let ch = |s: u32| {
        let (ca, cb) = ((a >> s) & 0xFF, (b >> s) & 0xFF);
        ((ca * (255 - t) + cb * t) / 255) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// Text clipped to the columns `clip.0..clip.1` (for the scrolling title).
pub fn text_clipped(c: &mut Canvas, x: i32, y: i32, s: &str, (scale, color): (usize, u32), (x0, x1): (i32, i32)) {
    let sc = scale as i32;
    let mut gx = x;
    for ch in s.chars() {
        if gx + 8 * sc > x0 && gx < x1 {
            for (row, bits) in glyph_bits(ch).iter().enumerate() {
                for col in 0..8 {
                    let px = gx + col * sc;
                    if bits >> col & 1 == 1 && px >= x0 && px + sc <= x1 {
                        c.rect(px, y + row as i32 * sc, sc, sc, color);
                    }
                }
            }
        }
        gx += 8 * sc;
        if gx >= x1 {
            break;
        }
    }
}

pub const DIGIT_W: i32 = 18;
pub const DIGIT_H: i32 = 32;

/// Segments a-g (top, top-right, bottom-right, bottom, bottom-left, top-left, middle).
const DIGITS: [u8; 10] = [0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F];

/// One seven-segment LED digit; unlit segments are drawn dimly, like real LCDs.
pub fn seg_digit(c: &mut Canvas, x: i32, y: i32, digit: Option<u8>) {
    let (w, h, t) = (DIGIT_W, DIGIT_H, 4);
    let half = h / 2;
    let segs = [
        (x + t, y, w - 2 * t, t),
        (x + w - t, y + t, t, half - t),
        (x + w - t, y + half, t, half - t),
        (x + t, y + h - t, w - 2 * t, t),
        (x, y + half, t, half - t),
        (x, y + t, t, half - t),
        (x + t, y + half - t / 2, w - 2 * t, t),
    ];
    let lit = digit.map_or(0, |d| DIGITS[d as usize % 10]);
    for (i, (sx, sy, sw, sh)) in segs.into_iter().enumerate() {
        c.rect(sx, sy, sw, sh, if lit >> i & 1 == 1 { LCD_ON } else { LCD_OFF });
    }
}

/// `MM:SS` in LED digits; `None` shows all segments unlit. Returns the right edge.
pub fn time_display(c: &mut Canvas, x: i32, y: i32, secs: Option<f64>, colon: bool) -> i32 {
    let digits = secs.map(|s| {
        let s = (s.max(0.0) as u64).min(99 * 60 + 59);
        [(s / 600) as u8, (s / 60 % 10) as u8, (s % 60 / 10) as u8, (s % 10) as u8]
    });
    let d = |i: usize| digits.map(|ds| ds[i]);
    let mut cx = x;
    for i in 0..2 {
        seg_digit(c, cx, y, d(i));
        cx += DIGIT_W + 4;
    }
    let dot = if colon && digits.is_some() { LCD_ON } else { LCD_OFF };
    c.rect(cx + 1, y + 8, 4, 4, dot);
    c.rect(cx + 1, y + 20, 4, 4, dot);
    cx += 10;
    for i in 2..4 {
        seg_digit(c, cx, y, d(i));
        cx += DIGIT_W + 4;
    }
    cx
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Prev,
    Play,
    Pause,
    Stop,
    Next,
}

fn triangle_right(c: &mut Canvas, x: i32, y: i32, h: i32, color: u32) {
    for i in 0..h {
        let half = h / 2;
        let len = (half - (i - half).abs()).max(0) + 1;
        c.rect(x, y + i, len, 1, color);
    }
}

fn triangle_left(c: &mut Canvas, x: i32, y: i32, h: i32, color: u32) {
    for i in 0..h {
        let half = h / 2;
        let len = (half - (i - half).abs()).max(0) + 1;
        c.rect(x + half + 1 - len, y + i, len, 1, color);
    }
}

/// A transport icon centred in the box (x, y, w, h).
pub fn icon(c: &mut Canvas, kind: Icon, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let s = 14;
    let (ox, oy) = (x + (w - s) / 2, y + (h - s) / 2);
    match kind {
        Icon::Play => triangle_right(c, ox + 3, oy, s + 1, color),
        Icon::Pause => {
            c.rect(ox + 2, oy + 1, 4, s - 1, color);
            c.rect(ox + 9, oy + 1, 4, s - 1, color);
        }
        Icon::Stop => c.rect(ox + 1, oy + 1, s - 2, s - 2, color),
        Icon::Prev => {
            c.rect(ox - 1, oy + 1, 3, s - 1, color);
            triangle_left(c, ox + 2, oy, s + 1, color);
            triangle_left(c, ox + 8, oy, s + 1, color);
        }
        Icon::Next => {
            triangle_right(c, ox - 1, oy, s + 1, color);
            triangle_right(c, ox + 5, oy, s + 1, color);
            c.rect(ox + 13, oy + 1, 3, s - 1, color);
        }
    }
}

/// A groove with a filled part and a bevelled thumb at `frac` (0.0-1.0).
pub fn slider(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, frac: Option<f32>, gradient: bool) {
    inset(c, x, y + h / 2 - 3, w, 6, 0x101118);
    let thumb_w = 24;
    let Some(f) = frac else { return };
    let pos = ((w - thumb_w) as f32 * f.clamp(0.0, 1.0)) as i32;
    if gradient {
        for i in 0..pos + thumb_w / 2 {
            let t = (i * 255 / w.max(1)) as u32;
            let col = if t < 128 { mix(0x1FBF3A, 0xE8D21F, t * 2) } else { mix(0xE8D21F, 0xE0402A, (t - 128) * 2) };
            c.rect(x + i, y + h / 2 - 2, 1, 4, col);
        }
    } else {
        c.rect(x, y + h / 2 - 2, pos + thumb_w / 2, 4, LCD_DIM);
    }
    bevel(c, x + pos, y, thumb_w, h, false);
    c.rect(x + pos + thumb_w / 2 - 1, y + 3, 2, h - 6, FACE_LIGHT);
}

/// Bar colour for a pixel row at relative height `t` (0 = bottom, 255 = top).
pub fn bar_color(t: u32) -> u32 {
    if t < 150 {
        mix(0x14B83A, 0x8BE02A, t * 255 / 150)
    } else {
        mix(0xE6D21E, 0xE0402A, (t - 150) * 255 / 105)
    }
}
