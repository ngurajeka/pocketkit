//! MinUI's look, for our apps on every platform: its font (BPreplay Bold,
//! loaded from MinUI's own copy on the SD card, so we never redistribute it;
//! other platforms ship an open-licensed font via `POCKETKIT_FONT`), its colours and its
//! measurements (`workspace/all/common/defines.h` and `api.c` in MinUI, at the
//! Mini's 2x scale): black background, 60 px rows, white pill behind the
//! selected item, round button hints in a dark-gray pill at the bottom.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    env, fs,
    path::PathBuf,
};

use fontdue::{Font as FontData, FontSettings, Metrics};

use crate::render::{Canvas, H, W};

pub const SCALE: i32 = 2;
pub const PILL: i32 = 30 * SCALE;
pub const BUTTON: i32 = 20 * SCALE;
pub const BUTTON_MARGIN: i32 = 5 * SCALE;
pub const BUTTON_PADDING: i32 = 12 * SCALE;
pub const PADDING: i32 = 10 * SCALE;

pub const FONT_LARGE: f32 = 16.0 * SCALE as f32;
pub const FONT_MEDIUM: f32 = 14.0 * SCALE as f32;
pub const FONT_SMALL: f32 = 12.0 * SCALE as f32;
pub const FONT_TINY: f32 = 10.0 * SCALE as f32;

pub const WHITE: u32 = 0xFFFFFF;
pub const BLACK: u32 = 0x000000;
pub const LIGHT_GRAY: u32 = 0x7F7F7F;
pub const GRAY: u32 = 0x999999;
pub const DARK_GRAY: u32 = 0x262626;
pub const LIGHT_TEXT: u32 = 0xCCCCCC;
pub const DARK_TEXT: u32 = 0x666666;

/// Rows of content below a header pill and above the button hints.
pub const CONTENT_TOP: i32 = PADDING + PILL + PADDING / 2;
pub const HINTS_TOP: i32 = H as i32 - PADDING - PILL;

pub const FONT_FILE: &str = "BPreplayBold-unhinted.otf";

/// Where the UI font may be: `POCKETKIT_FONT` (muOS launchers point it at the
/// bundled font), `MINUI_FONT`, MinUI's copy on the SD card, or the git-ignored
/// `.minui-res/` from `tools/fetch-minui-res.sh` (desktop preview).
fn font_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for var in ["POCKETKIT_FONT", "MINUI_FONT"] {
        if let Some(p) = env::var_os(var) {
            v.push(PathBuf::from(p));
        }
    }
    if let Some(sd) = env::var_os("SDCARD_PATH") {
        v.push(PathBuf::from(sd).join(".system/res").join(FONT_FILE));
    }
    v.push(PathBuf::from("/mnt/SDCARD/.system/res").join(FONT_FILE));
    for up in [".", "..", "../..", "../../.."] {
        v.push(PathBuf::from(up).join(".minui-res").join(FONT_FILE));
    }
    v
}

type Glyph = (Metrics, Vec<u8>);

/// MinUI's font with a glyph cache. Falls back to the built-in 8x8 bitmap font
/// if the file can't be found, so a pak never fails to draw.
pub struct Font {
    font: Option<FontData>,
    cache: RefCell<HashMap<(char, u32), Glyph>>,
    advances: RefCell<HashMap<(char, u32), f32>>,
}

impl Font {
    pub fn load() -> Self {
        let font = font_candidates().into_iter().find_map(|p| {
            let bytes = fs::read(&p).ok()?;
            let f = FontData::from_bytes(bytes, FontSettings::default()).ok()?;
            eprintln!("ui: font {}", p.display());
            Some(f)
        });
        if font.is_none() {
            eprintln!("ui: MinUI font not found, using the bitmap font");
        }
        Self { font, cache: RefCell::new(HashMap::new()), advances: RefCell::new(HashMap::new()) }
    }

    /// Bitmap fallback only (tests).
    pub fn bitmap() -> Self {
        Self { font: None, cache: RefCell::new(HashMap::new()), advances: RefCell::new(HashMap::new()) }
    }

    /// Advance width of one character (cached: the browser measures a lot).
    fn advance(&self, f: &FontData, c: char, size: f32) -> f32 {
        let key = (c, size.to_bits());
        if let Some(&a) = self.advances.borrow().get(&key) {
            return a;
        }
        let a = f.metrics(c, size).advance_width;
        self.advances.borrow_mut().insert(key, a);
        a
    }

    pub fn is_minui(&self) -> bool {
        self.font.is_some()
    }

    fn bitmap_scale(size: f32) -> usize {
        ((size / 12.0).round() as usize).max(1)
    }

    fn glyph(&self, f: &FontData, c: char, size: f32) -> Glyph {
        let key = (c, size.to_bits());
        if let Some(g) = self.cache.borrow().get(&key) {
            return g.clone();
        }
        let g = f.rasterize(c, size);
        self.cache.borrow_mut().insert(key, g.clone());
        g
    }

    /// (ascent, descent) in pixels; descent is negative.
    fn vmetrics(&self, size: f32) -> (f32, f32) {
        match self.font.as_ref().and_then(|f| f.horizontal_line_metrics(size)) {
            Some(m) => (m.ascent, m.descent),
            None => (size * 0.8, -size * 0.2),
        }
    }

    /// Height of a line of text (what SDL_ttf reports as the text height).
    pub fn height(&self, size: f32) -> i32 {
        match &self.font {
            Some(_) => {
                let (a, d) = self.vmetrics(size);
                (a - d).ceil() as i32
            }
            None => 8 * Self::bitmap_scale(size) as i32,
        }
    }

    pub fn measure(&self, text: &str, size: f32) -> i32 {
        match &self.font {
            Some(f) => text.chars().map(|c| self.advance(f, c, size)).sum::<f32>().ceil() as i32,
            None => text.chars().count() as i32 * 8 * Self::bitmap_scale(size) as i32,
        }
    }

    /// Draws `text` with its top at `y`; returns the width drawn.
    pub fn draw(&self, c: &mut Canvas, x: i32, y: i32, text: &str, size: f32, color: u32) -> i32 {
        let Some(f) = &self.font else {
            let s = Self::bitmap_scale(size);
            return c.text(x, y, text, s, color) - x;
        };
        let (ascent, _) = self.vmetrics(size);
        let baseline = y as f32 + ascent;
        let mut pen = x as f32;
        for ch in text.chars() {
            let (m, bitmap) = self.glyph(f, ch, size);
            let gx = (pen + m.xmin as f32).round() as i32;
            let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
            for row in 0..m.height {
                for col in 0..m.width {
                    let cov = bitmap[row * m.width + col];
                    if cov > 0 {
                        blend(c, gx + col as i32, gy + row as i32, color, cov as u32);
                    }
                }
            }
            pen += m.advance_width;
        }
        (pen - x as f32).ceil() as i32
    }

    /// Draws `text` vertically centred in the band `(y, h)`.
    pub fn draw_centered_v(&self, c: &mut Canvas, x: i32, (y, h): (i32, i32), text: &str, size: f32, color: u32) -> i32 {
        let ty = y + (h - self.height(size)) / 2;
        self.draw(c, x, ty, text, size, color)
    }

    /// `text`, shortened with "..." to fit `max_w` pixels.
    pub fn truncate(&self, text: &str, size: f32, max_w: i32) -> String {
        if self.measure(text, size) <= max_w {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut n = chars.len();
        while n > 0 {
            n -= 1;
            let s: String = chars[..n].iter().collect::<String>().trim_end().to_string() + "...";
            if self.measure(&s, size) <= max_w {
                return s;
            }
        }
        "...".into()
    }

    /// Keeps the end of `text` (for paths and typed input): "...ng/end".
    pub fn truncate_start(&self, text: &str, size: f32, max_w: i32) -> String {
        if self.measure(text, size) <= max_w {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        for start in 1..chars.len() {
            let s = format!("...{}", chars[start..].iter().collect::<String>());
            if self.measure(&s, size) <= max_w {
                return s;
            }
        }
        "...".into()
    }

    /// Word-wraps `text` to `max_w` pixels.
    pub fn wrap(&self, text: &str, size: f32, max_w: i32) -> Vec<String> {
        let mut lines = vec![String::new()];
        for word in text.split_whitespace() {
            let cur = lines.last_mut().unwrap();
            let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if self.measure(&candidate, size) <= max_w || cur.is_empty() {
                *cur = candidate;
            } else {
                lines.push(word.to_string());
            }
        }
        lines
    }
}

/// Mixes `color` into pixel (x, y) with coverage `a` (0-255).
pub fn blend(c: &mut Canvas, x: i32, y: i32, color: u32, a: u32) {
    if x < 0 || y < 0 || x >= c.w as i32 || y >= c.h as i32 {
        return;
    }
    let i = y as usize * c.w + x as usize;
    if a >= 255 {
        c.px[i] = color;
        return;
    }
    let dst = c.px[i];
    let mix = |s: u32| {
        let (fg, bg) = ((color >> s) & 0xFF, (dst >> s) & 0xFF);
        ((fg * a + bg * (255 - a)) / 255) << s
    };
    c.px[i] = mix(16) | mix(8) | mix(0);
}

/// Anti-aliased rounded rectangle; `r` = h/2 gives MinUI's pill.
pub fn rounded_rect(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, r: i32, color: u32) {
    let r = r.min(w / 2).min(h / 2).max(0);
    c.rect(x + r, y, w - 2 * r, h, color);
    c.rect(x, y + r, r, h - 2 * r, color);
    c.rect(x + w - r, y + r, r, h - 2 * r, color);
    let rf = r as f32;
    for (cx, cy, ox, oy) in [(x + r, y + r, x, y), (x + w - r, y + r, x + w - r, y), (x + r, y + h - r, x, y + h - r), (x + w - r, y + h - r, x + w - r, y + h - r)] {
        for py in oy..oy + r {
            for px in ox..ox + r {
                let dx = px as f32 + 0.5 - cx as f32;
                let dy = py as f32 + 0.5 - cy as f32;
                let cov = (rf - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    blend(c, px, py, color, (cov * 255.0) as u32);
                }
            }
        }
    }
}

pub fn pill(c: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: u32) {
    rounded_rect(c, x, y, w.max(h), h, h / 2, color);
}

/// Which system's look to follow. Layout and sizes are shared; colours and a
/// few shapes differ so apps feel at home on each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// MinUI: black, white pill on the selection, dark-gray hint pills.
    MinUI,
    /// muOS: charcoal, black header and footer bars, mustard highlight.
    Muos,
}

impl Style {
    /// From `POCKETKIT_STYLE` (`minui` / `muos`; muOS launchers set it), MinUI otherwise.
    pub fn from_env() -> Self {
        match env::var("POCKETKIT_STYLE").map(|s| s.to_ascii_lowercase()) {
            Ok(s) if s == "muos" => Style::Muos,
            _ => Style::MinUI,
        }
    }
}

/// The colours a style draws with (apps use it for their own drawing too).
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub text: u32,
    /// Unavailable items.
    pub dim: u32,
    /// Secondary text: values, help, link underlines.
    pub soft: u32,
    pub sel_bg: u32,
    pub sel_text: u32,
    /// Secondary text on the selection.
    pub sel_soft: u32,
    /// Title/hint/toast backgrounds.
    pub chrome: u32,
    pub chrome_text: u32,
    /// Raised surfaces (keys, input field, progress).
    pub surface: u32,
    pub button_bg: u32,
    pub button_text: u32,
    pub accent: u32,
}

pub const MINUI_PALETTE: Palette = Palette {
    bg: BLACK,
    text: WHITE,
    dim: DARK_TEXT,
    soft: LIGHT_TEXT,
    sel_bg: WHITE,
    sel_text: BLACK,
    sel_soft: DARK_TEXT,
    chrome: DARK_GRAY,
    chrome_text: WHITE,
    surface: DARK_GRAY,
    button_bg: WHITE,
    button_text: GRAY,
    accent: GRAY,
};

/// MustardOS gets mustard.
pub const MUOS_PALETTE: Palette = Palette {
    bg: 0x161616,
    text: 0xE8E8E8,
    dim: 0x5A5A5A,
    soft: 0xA6A6A6,
    sel_bg: 0xF2B705,
    sel_text: 0x161616,
    sel_soft: 0x5C4500,
    chrome: 0x000000,
    chrome_text: 0xFFFFFF,
    surface: 0x2B2B2B,
    button_bg: 0xF2B705,
    button_text: 0x161616,
    accent: 0xF2B705,
};

/// Everything a screen draws with.
pub struct Ui {
    pub font: Font,
    pub style: Style,
    pub pal: Palette,
    /// muOS: the footer bar is drawn once per frame (reset by `clear`), so a
    /// second hint group doesn't paint over the first.
    footer_drawn: Cell<bool>,
}

impl Ui {
    /// The UI font and the style from the environment.
    pub fn new() -> Self {
        Self::with_style(Font::load(), Style::from_env())
    }

    pub fn with_font(font: Font) -> Self {
        Self::with_style(font, Style::MinUI)
    }

    pub fn with_style(font: Font, style: Style) -> Self {
        let pal = match style {
            Style::MinUI => MINUI_PALETTE,
            Style::Muos => MUOS_PALETTE,
        };
        Self { font, style, pal, footer_drawn: Cell::new(false) }
    }

    pub fn clear(&self, c: &mut Canvas) {
        c.fill(self.pal.bg);
        self.footer_drawn.set(false);
    }

    /// The round (one letter) or pill-shaped (word) button glyph; returns its width.
    pub fn button(&self, c: &mut Canvas, x: i32, y: i32, label: &str) -> i32 {
        let (bg, fg) = (self.pal.button_bg, self.pal.button_text);
        if label.chars().count() == 1 {
            rounded_rect(c, x, y, BUTTON, BUTTON, BUTTON / 2, bg);
            let tw = self.font.measure(label, FONT_MEDIUM);
            self.font.draw_centered_v(c, x + (BUTTON - tw) / 2, (y, BUTTON), label, FONT_MEDIUM, fg);
            BUTTON
        } else {
            let tw = self.font.measure(label, FONT_TINY);
            let w = BUTTON / 2 + tw;
            pill(c, x, y, w, BUTTON, bg);
            self.font.draw_centered_v(c, x + BUTTON / 4, (y, BUTTON), label, FONT_TINY, fg);
            w.max(BUTTON)
        }
    }

    fn hint_width(&self, button: &str, hint: &str) -> i32 {
        let b = if button.chars().count() == 1 { BUTTON } else { (BUTTON / 2 + self.font.measure(button, FONT_TINY)).max(BUTTON) };
        b + BUTTON_MARGIN + self.font.measure(hint, FONT_SMALL) + BUTTON_MARGIN
    }

    /// Button hints along the bottom, left or right aligned: MinUI puts each
    /// group in a dark pill; muOS uses one full-width footer bar.
    pub fn hints(&self, c: &mut Canvas, pairs: &[(&str, &str)], align_right: bool) {
        if pairs.is_empty() {
            return;
        }
        let w = pairs.iter().map(|(b, h)| BUTTON_MARGIN + self.hint_width(b, h)).sum::<i32>() + BUTTON_MARGIN;
        let x0 = if align_right { W as i32 - PADDING - w } else { PADDING };
        match self.style {
            Style::MinUI => pill(c, x0, HINTS_TOP, w, PILL, self.pal.chrome),
            Style::Muos => {
                if !self.footer_drawn.replace(true) {
                    let top = HINTS_TOP + PADDING / 2;
                    c.rect(0, top, W as i32, H as i32 - top, self.pal.chrome);
                    c.rect(0, top, W as i32, 2, self.pal.surface);
                }
            }
        }
        let mut x = x0 + BUTTON_MARGIN * 2;
        let by = HINTS_TOP + (PILL - BUTTON) / 2 + if self.style == Style::Muos { PADDING / 4 } else { 0 };
        for (b, h) in pairs {
            let bw = self.button(c, x, by, b);
            x += bw + BUTTON_MARGIN;
            x += self.font.draw_centered_v(c, x, (by, BUTTON), h, FONT_SMALL, self.pal.chrome_text) + BUTTON_MARGIN * 2;
        }
    }

    /// The screen title: a dark pill (MinUI) or a header bar with an accent
    /// underline (muOS).
    pub fn title(&self, c: &mut Canvas, text: &str) {
        let max = W as i32 - 2 * PADDING - 2 * BUTTON_PADDING;
        match self.style {
            Style::MinUI => {
                let t = self.font.truncate(text, FONT_SMALL, max);
                let w = self.font.measure(&t, FONT_SMALL) + 2 * BUTTON_PADDING;
                pill(c, PADDING, PADDING, w, PILL, self.pal.chrome);
                self.font.draw_centered_v(c, PADDING + BUTTON_PADDING, (PADDING, PILL), &t, FONT_SMALL, self.pal.chrome_text);
            }
            Style::Muos => {
                let h = PADDING + PILL - PADDING / 2;
                c.rect(0, 0, W as i32, h, self.pal.chrome);
                c.rect(0, h - 3, W as i32, 3, self.pal.accent);
                let t = self.font.truncate(text, FONT_MEDIUM, max);
                self.font.draw_centered_v(c, PADDING + BUTTON_PADDING / 2, (0, h - 3), &t, FONT_MEDIUM, self.pal.chrome_text);
            }
        }
    }

    /// Top of content row `i` (rows are PILL high, below the title).
    pub fn row_y(i: usize) -> i32 {
        CONTENT_TOP + i as i32 * PILL
    }

    /// How many content rows fit between the title and the hints (minus `reserve` px).
    pub fn rows_fitting(reserve: i32) -> usize {
        ((HINTS_TOP - CONTENT_TOP - reserve) / PILL).max(1) as usize
    }

    /// A menu row: plain text, or highlighted when selected. `right` is an
    /// optional value drawn at the right edge.
    pub fn row(&self, c: &mut Canvas, i: usize, text: &str, selected: bool, right: Option<&str>, dim: bool) {
        self.row_ex(c, i, text, (selected, dim), right, 0);
    }

    /// Like `row`, but keeps `reserve` px free at the right edge for the
    /// caller's own drawing (e.g. signal bars); returns that area's x.
    pub fn row_ex(&self, c: &mut Canvas, i: usize, text: &str, (selected, dim): (bool, bool), right: Option<&str>, reserve: i32) -> i32 {
        let y = Self::row_y(i);
        let full = W as i32 - 2 * PADDING;
        let right_w = right.map_or(0, |r| self.font.measure(r, FONT_SMALL) + BUTTON_PADDING) + reserve;
        let label = self.font.truncate(text, FONT_LARGE, full - 2 * BUTTON_PADDING - right_w);
        match self.style {
            Style::MinUI => {
                if selected {
                    // With a value: the pill spans the row, like MinUI's option lists.
                    let w = if right.is_some() || reserve > 0 { full } else { self.font.measure(&label, FONT_LARGE) + 2 * BUTTON_PADDING };
                    pill(c, PADDING, y, w, PILL, self.pal.sel_bg);
                }
            }
            Style::Muos => {
                if selected {
                    rounded_rect(c, PADDING / 2, y + 4, W as i32 - PADDING, PILL - 8, 10, self.pal.sel_bg);
                } else {
                    c.rect(PADDING + BUTTON_PADDING / 2, y + PILL - 1, full - BUTTON_PADDING, 1, self.pal.surface);
                }
            }
        }
        let color = match (selected, dim) {
            (true, _) => self.pal.sel_text,
            (false, true) => self.pal.dim,
            (false, false) => self.pal.text,
        };
        self.font.draw_centered_v(c, PADDING + BUTTON_PADDING, (y, PILL), &label, FONT_LARGE, color);
        let reserve_x = PADDING + full - BUTTON_PADDING - reserve;
        if let Some(r) = right {
            let rw = self.font.measure(r, FONT_SMALL);
            let rc = if selected { self.pal.sel_soft } else { self.pal.soft };
            let gap = if reserve > 0 { BUTTON_MARGIN * 2 } else { 0 };
            self.font.draw_centered_v(c, reserve_x - gap - rw, (y, PILL), r, FONT_SMALL, rc);
        }
        reserve_x
    }

    /// The box behind toasts and progress: a pill (MinUI) or a card with an
    /// accent edge (muOS).
    fn card(&self, c: &mut Canvas, x: i32, y: i32, w: i32) {
        match self.style {
            Style::MinUI => pill(c, x, y, w, PILL, self.pal.chrome),
            Style::Muos => {
                rounded_rect(c, x, y, w, PILL, 10, self.pal.surface);
                rounded_rect(c, x, y, 8, PILL, 4, self.pal.accent);
            }
        }
    }

    /// A progress bar above the hints: `frac` fills it from the left.
    pub fn progress(&self, c: &mut Canvas, text: &str, frac: Option<f32>) {
        let w = W as i32 - 2 * PADDING;
        let y = HINTS_TOP - PILL - PADDING / 2;
        self.card(c, PADDING, y, w);
        if let Some(f) = frac {
            let fill = ((w as f32) * f.clamp(0.0, 1.0)) as i32;
            let fill_color = match self.style {
                Style::MinUI => 0x4A4A4A,
                Style::Muos => 0x5C4500,
            };
            if fill > PILL / 2 {
                match self.style {
                    Style::MinUI => pill(c, PADDING, y, fill, PILL, fill_color),
                    Style::Muos => rounded_rect(c, PADDING, y, fill, PILL, 10, fill_color),
                }
            }
        }
        let t = self.font.truncate(text, FONT_SMALL, w - 2 * BUTTON_PADDING);
        self.font.draw_centered_v(c, PADDING + BUTTON_PADDING, (y, PILL), &t, FONT_SMALL, self.pal.chrome_text);
    }

    /// Small scroll arrows at the right edge when a list doesn't fit.
    pub fn scroll_marks(&self, c: &mut Canvas, first: usize, shown: usize, total: usize) {
        let x = W as i32 - PADDING / 2 - 6;
        let color = self.pal.accent;
        let tri = |c: &mut Canvas, y: i32, up: bool| {
            for i in 0..6 {
                let w = if up { i * 2 + 1 } else { 11 - i * 2 };
                c.rect(x + 6 - w / 2, y + i, w, 1, color);
            }
        };
        if first > 0 {
            tri(c, CONTENT_TOP - 8, true);
        }
        if first + shown < total {
            tri(c, CONTENT_TOP + shown as i32 * PILL + 2, false);
        }
    }

    /// Centred multi-line message (empty lists, errors).
    pub fn message(&self, c: &mut Canvas, text: &str) {
        let max = W as i32 - 4 * PADDING;
        let lines: Vec<String> = text.lines().flat_map(|l| self.font.wrap(l, FONT_LARGE, max)).collect();
        let lh = self.font.height(FONT_LARGE) + 4;
        let mut y = (H as i32 - lh * lines.len() as i32) / 2;
        for l in &lines {
            let w = self.font.measure(l, FONT_LARGE);
            self.font.draw(c, (W as i32 - w) / 2, y, l, FONT_LARGE, self.pal.text);
            y += lh;
        }
    }

    /// A short message above the hints.
    pub fn toast(&self, c: &mut Canvas, text: &str) {
        let max = W as i32 - 2 * PADDING - 2 * BUTTON_PADDING;
        let t = self.font.truncate(text, FONT_SMALL, max);
        let w = self.font.measure(&t, FONT_SMALL) + 2 * BUTTON_PADDING;
        let x = (W as i32 - w) / 2;
        let y = HINTS_TOP - PILL - PADDING / 2;
        self.card(c, x, y, w);
        self.font.draw_centered_v(c, x + BUTTON_PADDING, (y, PILL), &t, FONT_SMALL, self.pal.chrome_text);
    }

    /// The on-screen keyboard: title, input field with a blinking caret,
    /// rounded keys (the selected one highlighted), hints at the bottom.
    pub fn keyboard(&self, c: &mut Canvas, kb: &crate::keyboard::Keyboard, title: &str, ok_hint: &str, time: f32) {
        use crate::keyboard::{COLS, PAGE_NAMES, ROWS};
        self.clear(c);
        self.title(c, title);
        let (fx, fw) = (PADDING, W as i32 - 2 * PADDING);
        let fy = CONTENT_TOP;
        match self.style {
            Style::MinUI => pill(c, fx, fy, fw, PILL, self.pal.surface),
            Style::Muos => {
                rounded_rect(c, fx, fy, fw, PILL, 10, self.pal.surface);
                c.rect(fx + 10, fy + PILL - 3, fw - 20, 3, self.pal.accent);
            }
        }
        let shown = self.font.truncate_start(&kb.text, FONT_LARGE, fw - 2 * BUTTON_PADDING - 8);
        let end = fx + BUTTON_PADDING + self.font.draw_centered_v(c, fx + BUTTON_PADDING, (fy, PILL), &shown, FONT_LARGE, self.pal.text);
        if (time * 2.0) as i32 % 2 == 0 {
            let caret = match self.style {
                Style::MinUI => self.pal.text,
                Style::Muos => self.pal.accent,
            };
            c.rect(end + 3, fy + 14, 3, PILL - 28, caret);
        }
        let (kw, kh, gap) = (54, 50, 6);
        let x0 = (W as i32 - (COLS as i32 * (kw + gap) - gap)) / 2;
        let y0 = fy + PILL + PADDING;
        let radius = match self.style {
            Style::MinUI => 12,
            Style::Muos => 8,
        };
        for row in 0..ROWS {
            for col in 0..COLS {
                let x = x0 + col as i32 * (kw + gap);
                let y = y0 + row as i32 * (kh + gap);
                let selected = row == kb.row && col == kb.col;
                rounded_rect(c, x, y, kw, kh, radius, if selected { self.pal.sel_bg } else { self.pal.surface });
                let label = kb.key_at(row, col).to_string();
                let tw = self.font.measure(&label, FONT_MEDIUM);
                let fg = if selected { self.pal.sel_text } else { self.pal.text };
                self.font.draw_centered_v(c, x + (kw - tw) / 2, (y, kh), &label, FONT_MEDIUM, fg);
            }
        }
        let next = PAGE_NAMES[(kb.page + 1) % PAGE_NAMES.len()];
        self.hints(c, &[("B", "DEL"), ("Y", "SPACE"), ("X", next)], false);
        self.hints(c, &[("START", ok_hint)], true);
    }
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_fallback_measures_and_truncates() {
        let f = Font::bitmap();
        assert!(!f.is_minui());
        assert_eq!(f.measure("abc", FONT_LARGE), 3 * 8 * 3);
        let t = f.truncate("hello world", FONT_LARGE, 6 * 24);
        assert!(t.ends_with("...") && f.measure(&t, FONT_LARGE) <= 6 * 24, "{t}");
        assert_eq!(f.truncate_start("/mnt/SDCARD/Music", FONT_LARGE, 8 * 24), "...Music");
        assert_eq!(f.wrap("one two three", FONT_LARGE, 7 * 24), vec!["one two", "three"]);
    }

    #[test]
    fn draws_every_widget_without_panicking() {
        let ui = Ui::new(); // MinUI font if available, else bitmap
        let mut c = Canvas::new(W, H);
        ui.clear(&mut c);
        ui.title(&mut c, "Settings");
        ui.row(&mut c, 0, "Screen off after", true, Some("2 min"), false);
        ui.row(&mut c, 1, "A very long label that will certainly need truncating", false, None, true);
        ui.hints(&mut c, &[("A", "SELECT"), ("B", "BACK")], true);
        ui.hints(&mut c, &[("MENU", "QUIT")], false);
        ui.scroll_marks(&mut c, 1, 4, 8);
        ui.toast(&mut c, "Saved");
        ui.message(&mut c, "No networks found\nPress X to scan");
        assert!(c.px.contains(&WHITE));
    }

    #[test]
    fn pill_is_antialiased() {
        let mut c = Canvas::new(100, 40);
        pill(&mut c, 0, 0, 100, 40, WHITE);
        assert_eq!(c.px[20 * 100 + 50], WHITE); // middle
        assert_eq!(c.px[0], BLACK); // corner outside the curve
        let edge = c.px[12 * 100 + 1]; // ~20 px from the left cap's centre
        assert!(edge != BLACK && edge != WHITE, "{edge:x}"); // partial coverage on the curve
    }
}
