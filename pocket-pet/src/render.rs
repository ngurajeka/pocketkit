//! Tiny software renderer. Everything is drawn procedurally into a 320x240
//! `0x00RRGGBB` pixel buffer, so there are no image assets to load yet.
//! The device backend scales this 2x to the Mini+'s 640x480 screen.

use crate::game::{Action, Mood, Pet, FEED_SECS, STAT_MAX};

pub const W: usize = 320;
pub const H: usize = 240;
const WI: i32 = W as i32;
const HI: i32 = H as i32;

// ---------------------------------------------------------------- palette
const WALL: u32 = 0xFCE8C8;
const WALL_STRIPE: u32 = 0xF8DDB5;
const FLOOR: u32 = 0xC99A66;
const FLOOR_EDGE: u32 = 0x9C7447;
const RUG: u32 = 0xE07A8F;
const SHADOW: u32 = 0xA9814F;
const WOOD: u32 = 0x8A5A33;
const SKY: u32 = 0xA5D8F3;
const SUN: u32 = 0xFFE066;
const CLOUD: u32 = 0xFFFFFF;

const FUR: u32 = 0xF2A65A;
const FUR_DARK: u32 = 0xE0904A;
const BELLY: u32 = 0xFBD9A8;
const EAR_IN: u32 = 0xE58A8A;
const DARK: u32 = 0x3B2A2A;
const WHITE: u32 = 0xFFFFFF;
const CHEEK: u32 = 0xF59A9A;
const NOSE: u32 = 0xC96A6A;
const MOUTH_IN: u32 = 0x7A2E2E;
const TEAR: u32 = 0x74C0FC;
const DIRT: u32 = 0x8B6B4A;

const PANEL: u32 = 0x3D3350;
const PANEL_EDGE: u32 = 0x2A2238;
const BTN: u32 = 0x5B4E73;
const BTN_SEL: u32 = 0xFFE08A;
const BAR_BG: u32 = 0x2A2238;
const BAR_OK: u32 = 0x6BCB77;
const BAR_MID: u32 = 0xFFD93D;
const BAR_LOW: u32 = 0xFF6B6B;

// ----------------------------------------------------------------- layout
const CX: i32 = 160; // pet centre x
const GROUND: i32 = 150; // where the wall meets the floor
const WIN_X: i32 = 24;
const WIN_Y: i32 = 36;
const MENU_TOP: i32 = 184;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, px: vec![0; w * h] }
    }

    pub fn get(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return 0;
        }
        self.px[y as usize * self.w + x as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, c: u32) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = c;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    pub fn ellipse(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, c: u32) {
        let (rx, ry) = (rx.max(1), ry.max(1));
        for dy in -ry..=ry {
            for dx in -rx..=rx {
                let nx = dx as f32 / rx as f32;
                let ny = dy as f32 / ry as f32;
                if nx * nx + ny * ny <= 1.0 {
                    self.set(cx + dx, cy + dy, c);
                }
            }
        }
    }

    pub fn circle(&mut self, cx: i32, cy: i32, r: i32, c: u32) {
        self.ellipse(cx, cy, r, r, c);
    }

    /// Bresenham line, 1px wide.
    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: u32) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn thick_line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u32) {
        for (ox, oy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            self.line(x0 + ox, y0 + oy, x1 + ox, y1 + oy, c);
        }
    }

    /// Darken rows `y0..y1` (`num` of 256 brightness) with a slight blue tint.
    pub fn shade(&mut self, y0: usize, y1: usize, num: u32) {
        for p in &mut self.px[y0 * self.w..y1 * self.w] {
            let r = ((*p >> 16) & 0xFF) * num / 256;
            let g = ((*p >> 8) & 0xFF) * num / 256;
            let b = (*p & 0xFF) * (num + 40).min(256) / 256;
            *p = (r << 16) | (g << 8) | b;
        }
    }
}

// ------------------------------------------------------------------ icons
// Each icon is drawn around (cx, cy) with "size" s roughly equal to its radius.

fn apple(c: &mut Canvas, cx: i32, cy: i32, s: i32) {
    c.circle(cx, cy + s / 5, s, 0xE5484D);
    c.circle(cx - s / 3, cy - s / 6, (s / 4).max(1), 0xFF9AA0);
    c.rect(cx, cy - s, 2, s / 2 + 1, 0x6B4226);
    c.ellipse(cx + s / 2, cy - s + 1, s / 3 + 1, (s / 5).max(1), 0x4CAF50);
}

fn ball(c: &mut Canvas, cx: i32, cy: i32, s: i32) {
    c.circle(cx, cy, s, 0x4DABF7);
    c.rect(cx - s + 1, cy - 1, 2 * s - 2, 2, WHITE);
    c.rect(cx - 1, cy - s + 1, 2, 2 * s - 2, WHITE);
}

/// Crescent moon. `bg` is the colour behind it (used to carve the crescent).
fn moon(c: &mut Canvas, cx: i32, cy: i32, s: i32, bg: u32) {
    c.circle(cx, cy, s, 0xF0B429);
    c.circle(cx + s / 2, cy - s / 4, s * 4 / 5, bg);
}

fn bubble(c: &mut Canvas, cx: i32, cy: i32, s: i32) {
    c.circle(cx, cy, s, 0x74C0FC);
    c.circle(cx, cy, (s - 2).max(1), 0xE7F5FF);
    c.circle(cx - s / 3, cy - s / 3, (s / 5).max(1), WHITE);
}

fn heart(c: &mut Canvas, cx: i32, cy: i32, s: i32, color: u32) {
    let unit = s as f32 / 1.2;
    let reach = (unit * 1.5) as i32;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let x = dx as f32 / unit;
            let y = -dy as f32 / unit;
            let a = x * x + y * y - 1.0;
            if a * a * a - x * x * y * y * y <= 0.0 {
                c.set(cx + dx, cy + dy, color);
            }
        }
    }
}

fn bolt(c: &mut Canvas, cx: i32, cy: i32, s: i32) {
    let col = 0xFFD43B;
    c.thick_line(cx + s / 3, cy - s, cx - s / 2, cy + s / 8, col);
    c.thick_line(cx - s / 2, cy + s / 8, cx + s / 8, cy + s / 8, col);
    c.thick_line(cx + s / 8, cy + s / 8, cx - s / 3, cy + s, col);
}

fn zed(c: &mut Canvas, x: i32, y: i32, s: i32, color: u32) {
    c.rect(x, y, s, 2, color);
    c.thick_line(x + s, y, x, y + s, color);
    c.rect(x, y + s - 2, s, 2, color);
}

// ------------------------------------------------------------------ scene

pub fn draw_scene(c: &mut Canvas, pet: &Pet, selected: usize, time: f32, shake: f32) {
    let mood = pet.mood();
    draw_room(c);
    draw_pet(c, pet, mood, time, shake);
    draw_effects(c, pet, mood, time);
    if mood == Mood::Sleeping {
        c.shade(0, MENU_TOP as usize, 150);
        draw_night(c, time);
    }
    draw_hud(c, pet);
    draw_menu(c, selected);
}

fn draw_room(c: &mut Canvas) {
    c.rect(0, 0, WI, GROUND, WALL);
    for x in (0..WI).step_by(32) {
        c.rect(x, 0, 16, GROUND, WALL_STRIPE);
    }
    c.rect(0, GROUND, WI, HI - GROUND, FLOOR);
    c.rect(0, GROUND, WI, 4, FLOOR_EDGE);
    c.ellipse(CX, GROUND + 26, 96, 14, RUG);

    // Window
    c.rect(WIN_X - 4, WIN_Y - 4, 68, 68, WOOD);
    c.rect(WIN_X, WIN_Y, 60, 60, SKY);
    c.circle(WIN_X + 42, WIN_Y + 17, 9, SUN);
    c.ellipse(WIN_X + 18, WIN_Y + 38, 14, 6, CLOUD);
    c.ellipse(WIN_X + 28, WIN_Y + 34, 9, 6, CLOUD);
    c.rect(WIN_X + 29, WIN_Y, 2, 60, WOOD);
    c.rect(WIN_X, WIN_Y + 29, 60, 2, WOOD);
}

fn draw_pet(c: &mut Canvas, pet: &Pet, mood: Mood, time: f32, shake: f32) {
    let sleeping = mood == Mood::Sleeping;
    let hop = match mood {
        Mood::Playing => -((time * 9.0).sin().abs() * 16.0),
        Mood::Sleeping => (time * 1.6).sin() * 1.5,
        _ => (time * 4.0).sin() * 2.5,
    };
    let sway = (time * 55.0).sin() * 7.0 * (shake / 0.4);
    let cx = CX + sway as i32;
    let cy = 116 + hop as i32;

    // Shadow stays on the floor and shrinks while the pet hops.
    c.ellipse(CX, 160, (44.0 + hop * 0.6) as i32, 7, SHADOW);

    // Ears, feet, body, belly
    for s in [-1, 1] {
        c.circle(cx + s * 30, cy - 34, 15, FUR);
        c.circle(cx + s * 30, cy - 33, 8, EAR_IN);
        c.ellipse(cx + s * 24, cy + 38, 15, 8, FUR_DARK);
    }
    c.ellipse(cx, cy, 48, 42, FUR);
    c.ellipse(cx, cy + 12, 30, 26, BELLY);

    if pet.cleanliness < 45.0 {
        for (dx, dy, r) in [(-28, -12, 5), (30, 6, 6), (-10, 24, 4), (22, -26, 4)] {
            c.circle(cx + dx, cy + dy, r, DIRT);
        }
    }

    // Eyes
    let blink = (time % 4.0) < 0.12;
    let eye_y = cy - 10;
    for s in [-1, 1] {
        let ex = cx + s * 18;
        if sleeping || blink || mood == Mood::Bathing {
            c.rect(ex - 7, eye_y, 14, 3, DARK);
            continue;
        }
        let ey = if mood == Mood::Sad { eye_y + 2 } else { eye_y };
        c.circle(ex, ey, 7, DARK);
        c.circle(ex + 2, ey - 3, 2, WHITE);
        if mood == Mood::Tired {
            c.rect(ex - 8, ey - 8, 17, 9, FUR); // heavy eyelids
            c.rect(ex - 8, ey + 1, 17, 2, DARK);
        }
        if mood == Mood::Sad {
            c.ellipse(ex + s * 3, ey + 12, 2, 4, TEAR);
        }
    }

    // Cheeks and nose
    for s in [-1, 1] {
        c.circle(cx + s * 32, cy + 6, 7, CHEEK);
    }
    c.ellipse(cx, cy + 1, 5, 3, NOSE);

    // Mouth
    let my = cy + 10;
    match mood {
        Mood::Happy | Mood::Playing | Mood::Bathing => {
            for dx in -9..=9 {
                c.rect(cx + dx, my + 7 - dx * dx / 12, 2, 2, DARK);
            }
        }
        Mood::Sad | Mood::Dirty => {
            for dx in -8..=8 {
                c.rect(cx + dx, my + dx * dx / 11, 2, 2, DARK);
            }
        }
        Mood::Eating => {
            let open = 3 + ((time * 14.0).sin().abs() * 5.0) as i32;
            c.ellipse(cx, my + 4, 8, open, MOUTH_IN);
        }
        Mood::Hungry => c.ellipse(cx, my + 4, 5, 6, MOUTH_IN),
        Mood::Tired => c.ellipse(cx, my + 5, 6, 7, MOUTH_IN),
        Mood::Sleeping => c.rect(cx - 4, my + 3, 8, 2, DARK),
        Mood::Content => c.rect(cx - 6, my + 4, 12, 2, DARK),
    }
}

fn draw_effects(c: &mut Canvas, pet: &Pet, mood: Mood, time: f32) {
    match mood {
        Mood::Eating => {
            let left = pet.busy.map(|(_, t)| t).unwrap_or(0.0);
            let s = 5 + (left / FEED_SECS * 9.0) as i32; // apple shrinks as it's eaten
            apple(c, CX + 68, 124, s);
        }
        Mood::Playing => {
            let by = GROUND - 9 - ((time * 9.0).sin().abs() * 46.0) as i32;
            ball(c, CX + 84, by, 9);
        }
        Mood::Bathing => {
            for i in 0..10i32 {
                let t = (time * 0.8 + i as f32 * 0.137) % 1.0;
                let x = CX - 60 + (i * 47) % 120;
                let y = 150 - (t * 110.0) as i32;
                bubble(c, x, y, 4 + (i % 3) * 2);
            }
        }
        Mood::Hungry | Mood::Dirty | Mood::Tired | Mood::Sad => {
            let bx = CX + 66;
            let by = 62 + ((time * 3.0).sin() * 2.0) as i32;
            c.circle(bx, by, 24, 0xD9D9E3);
            c.circle(bx, by, 22, WHITE);
            for (dx, dy, r) in [(-22, 24, 5), (-30, 34, 3)] {
                c.circle(bx + dx, by + dy, r + 1, 0xD9D9E3);
                c.circle(bx + dx, by + dy, r, WHITE);
            }
            match mood {
                Mood::Hungry => apple(c, bx, by, 11),
                Mood::Dirty => bubble(c, bx, by, 11),
                Mood::Tired => moon(c, bx, by, 11, WHITE),
                _ => heart(c, bx, by, 10, 0x9AA0B5),
            }
        }
        _ => {}
    }
}

/// Night-time extras, drawn after the room has been darkened.
fn draw_night(c: &mut Canvas, time: f32) {
    let sky = c.get(WIN_X + 10, WIN_Y + 10);
    moon(c, WIN_X + 34, WIN_Y + 22, 12, sky);
    for i in 0..3 {
        let phase = (time * 0.6 + i as f32 * 0.33) % 1.0;
        let x = CX + 36 + (phase * 34.0) as i32 + i * 6;
        let y = 70 - (phase * 40.0) as i32;
        zed(c, x, y, 5 + (phase * 10.0) as i32, WHITE);
    }
}

fn draw_hud(c: &mut Canvas, pet: &Pet) {
    c.rect(0, 0, WI, 26, PANEL);
    c.rect(0, 26, WI, 2, PANEL_EDGE);
    let stats = [pet.fullness, pet.happiness, pet.energy, pet.cleanliness];
    for (i, v) in stats.iter().enumerate() {
        let x = 6 + i as i32 * 78;
        match i {
            0 => apple(c, x + 8, 14, 6),
            1 => heart(c, x + 8, 13, 6, 0xFF6B81),
            2 => bolt(c, x + 8, 13, 7),
            _ => bubble(c, x + 8, 13, 6),
        }
        let color = if *v < 25.0 {
            BAR_LOW
        } else if *v < 50.0 {
            BAR_MID
        } else {
            BAR_OK
        };
        c.rect(x + 20, 8, 54, 12, BAR_BG);
        c.rect(x + 22, 10, (*v / STAT_MAX * 50.0) as i32, 8, color);
    }
}

fn draw_menu(c: &mut Canvas, selected: usize) {
    c.rect(0, MENU_TOP, WI, HI - MENU_TOP, PANEL);
    c.rect(0, MENU_TOP, WI, 2, PANEL_EDGE);
    for (i, action) in Action::ALL.iter().enumerate() {
        let x = 14 + i as i32 * 76;
        let sel = i == selected;
        let y = if sel { 190 } else { 196 };
        let fill = if sel { BTN_SEL } else { BTN };
        if sel {
            c.rect(x - 3, y - 3, 70, 44, WHITE);
        }
        c.rect(x, y, 64, 38, fill);
        let (ix, iy) = (x + 32, y + 19);
        match action {
            Action::Feed => apple(c, ix, iy, 10),
            Action::Play => ball(c, ix, iy, 10),
            Action::Sleep => moon(c, ix, iy, 10, fill),
            Action::Clean => bubble(c, ix, iy, 10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_is_clipped_not_panicking() {
        let mut c = Canvas::new(W, H);
        c.circle(-50, -50, 30, WHITE);
        c.rect(300, 230, 100, 100, WHITE);
        c.line(-10, -10, 400, 300, WHITE);
        assert_eq!(c.get(-1, 0), 0);
    }

    #[test]
    fn every_mood_renders() {
        let mut c = Canvas::new(W, H);
        let pets = [
            Pet::default(),
            Pet { fullness: 5.0, ..Pet::default() },
            Pet { cleanliness: 5.0, ..Pet::default() },
            Pet { energy: 5.0, ..Pet::default() },
            Pet { happiness: 5.0, ..Pet::default() },
            Pet { sleeping: true, ..Pet::default() },
            Pet { busy: Some((Action::Feed, 1.0)), ..Pet::default() },
            Pet { busy: Some((Action::Play, 1.0)), ..Pet::default() },
            Pet { busy: Some((Action::Clean, 1.0)), ..Pet::default() },
        ];
        for (i, pet) in pets.iter().enumerate() {
            draw_scene(&mut c, pet, i % 4, i as f32 * 0.37, 0.2);
        }
    }
}
