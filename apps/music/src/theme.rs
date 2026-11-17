//! Colour themes for the player skin. L2 / R2 cycles them; the choice is saved
//! with the rest of the player state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// Metal panels: surface, bevel highlight, bevel shadow, pressed button.
    pub face: u32,
    pub face_light: u32,
    pub face_dark: u32,
    pub pressed: u32,
    /// Slider grooves.
    pub groove: u32,
    pub text: u32,
    pub text_dim: u32,
    /// Title bars: gradient top/bottom, text, the ridged stripes.
    pub title_hi: u32,
    pub title_lo: u32,
    pub title_text: u32,
    pub ridge_hi: u32,
    pub ridge_lo: u32,
    /// LED displays and the playlist.
    pub lcd_bg: u32,
    pub lcd_on: u32,
    pub lcd_off: u32,
    pub lcd_dim: u32,
    /// Spectrum analyzer and volume gradient, bottom to top.
    pub bars: [u32; 4],
    pub peak: u32,
    /// Playlist selection background and the playing track's text.
    pub sel: u32,
    pub current: u32,
}

pub const THEMES: [Theme; 5] = [
    Theme {
        name: "Classic",
        face: 0x3A3D4C,
        face_light: 0x6A6F86,
        face_dark: 0x17181F,
        pressed: 0x2E3140,
        groove: 0x101118,
        text: 0xC9CEDD,
        text_dim: 0x8C91A6,
        title_hi: 0x3D4E9C,
        title_lo: 0x161C3E,
        title_text: 0xE8ECFF,
        ridge_hi: 0x7B86C8,
        ridge_lo: 0x0B0F24,
        lcd_bg: 0x000000,
        lcd_on: 0x19E64A,
        lcd_off: 0x0B2A12,
        lcd_dim: 0x0F7A2C,
        bars: [0x14B83A, 0x8BE02A, 0xE6D21E, 0xE0402A],
        peak: 0xB8BCC8,
        sel: 0x1F3C9C,
        current: 0xFFFFFF,
    },
    Theme {
        name: "Sakura",
        face: 0xF6C9D8,
        face_light: 0xFFEAF2,
        face_dark: 0xC48AA0,
        pressed: 0xEDB6C9,
        groove: 0x8E4A64,
        text: 0x5A1E3A,
        text_dim: 0x9C5C78,
        title_hi: 0xF58BB4,
        title_lo: 0x92245A,
        title_text: 0xFFFFFF,
        ridge_hi: 0xFFC2DA,
        ridge_lo: 0x9E3A68,
        lcd_bg: 0x3A0F2A,
        lcd_on: 0xFF8FC7,
        lcd_off: 0x5A1F44,
        lcd_dim: 0xC25A93,
        bars: [0xFF9AD5, 0xF77FBE, 0xC77DFF, 0x9B5CFF],
        peak: 0xFFFFFF,
        sel: 0x5E1E42,
        current: 0xFFFFFF,
    },
    Theme {
        name: "Midnight",
        face: 0x1C1C22,
        face_light: 0x34343F,
        face_dark: 0x08080B,
        pressed: 0x141418,
        groove: 0x050507,
        text: 0xB8BCC8,
        text_dim: 0x6E7280,
        title_hi: 0x2A2A33,
        title_lo: 0x101014,
        title_text: 0xD0D4E0,
        ridge_hi: 0x44444F,
        ridge_lo: 0x000000,
        lcd_bg: 0x05070A,
        lcd_on: 0x4FC3F7,
        lcd_off: 0x0D2230,
        lcd_dim: 0x237A9E,
        bars: [0x1E88E5, 0x29B6F6, 0x80DEEA, 0xE1F5FE],
        peak: 0xFFFFFF,
        sel: 0x263238,
        current: 0xFFFFFF,
    },
    Theme {
        name: "Amber",
        face: 0x4A3524,
        face_light: 0x7A5A3E,
        face_dark: 0x22170F,
        pressed: 0x3D2B1D,
        groove: 0x140D08,
        text: 0xF0D9B5,
        text_dim: 0xB89A72,
        title_hi: 0x8A5A2B,
        title_lo: 0x4A2E12,
        title_text: 0xFFE8C2,
        ridge_hi: 0xC08A4E,
        ridge_lo: 0x2A1808,
        lcd_bg: 0x120A02,
        lcd_on: 0xFFB000,
        lcd_off: 0x332200,
        lcd_dim: 0x9C6A00,
        bars: [0xFF8F00, 0xFFB300, 0xFFD54F, 0xFF5722],
        peak: 0xFFE0B2,
        sel: 0x4A3018,
        current: 0xFFFFFF,
    },
    Theme {
        name: "Ocean",
        face: 0x1F4E5F,
        face_light: 0x3E7F94,
        face_dark: 0x0B2530,
        pressed: 0x18414F,
        groove: 0x061820,
        text: 0xD6F1F5,
        text_dim: 0x8CC1CC,
        title_hi: 0x2A9D8F,
        title_lo: 0x0E4A52,
        title_text: 0xFFFFFF,
        ridge_hi: 0x7FD1C7,
        ridge_lo: 0x08343A,
        lcd_bg: 0x021A1F,
        lcd_on: 0x5EEAD4,
        lcd_off: 0x0B3A3A,
        lcd_dim: 0x2A9D8F,
        bars: [0x26A69A, 0x4DD0E1, 0x80DEEA, 0xF1FFFD],
        peak: 0xFFFFFF,
        sel: 0x0A4650,
        current: 0xFFFFFF,
    },
];

/// Index of the theme called `name` (case-insensitive), Classic otherwise.
pub fn by_name(name: &str) -> usize {
    THEMES.iter().position(|t| t.name.eq_ignore_ascii_case(name.trim())).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance difference, roughly: enough for text to read.
    fn contrast(a: u32, b: u32) -> f32 {
        let lum = |c: u32| {
            let ch = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
            0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn five_named_themes_with_readable_text() {
        assert_eq!(THEMES.len(), 5);
        for t in &THEMES {
            assert!(contrast(t.lcd_on, t.lcd_bg) >= 4.5, "{} lcd", t.name);
            assert!(contrast(t.lcd_on, t.sel) >= 2.5, "{} selected row", t.name);
            assert!(contrast(t.current, t.sel) >= 3.0, "{} current on selection", t.name);
            // Small panel labels; Classic (the original look) sits at 2.9.
            assert!(contrast(t.text, t.face) >= 2.8, "{} panel text", t.name);
            assert!(contrast(t.title_text, t.title_lo) >= 3.0, "{} title", t.name);
        }
        assert_eq!(by_name("sakura"), 1);
        assert_eq!(by_name("nope"), 0);
    }
}
