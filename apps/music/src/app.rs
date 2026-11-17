//! The player screen: main window (LED time, analyzer, title, controls) on top,
//! playlist window below. Handles buttons, track order and saved settings.

use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use pocketkit_platform::volume::VolumeSource;
use pocketkit_core::{
    input::Button,
    settings::{self, Settings},
    render::{fit, Canvas, W},
};

use crate::{
    audio::Sink,
    decoder::TrackInfo,
    dsp,
    library::{self, fmt_time, Repeat, Rng, Track},
    player::{Cmd, Event, Player, State},
    skin::{self, Icon},
};

const BANDS: usize = 20;
const SEEK_STEP: f64 = 5.0;
const FLASH_SECS: f32 = 2.5;

// Layout (640x480).
const MAIN_TOP: i32 = 0;
const PL_TOP: i32 = 196;
const LIST_X: i32 = 8;
const LIST_Y: i32 = PL_TOP + 22;
const LIST_W: i32 = W as i32 - 16;
const ROW_H: i32 = 18;
const PL_ROWS: usize = 13;
const MARQUEE: (i32, i32, i32, i32) = (244, 24, 386, 24);

pub struct Config {
    pub music_dir: PathBuf,
    pub state_path: PathBuf,
    /// Where the system volume is read from (MinUI's settings, PipeWire, ...).
    pub volume: VolumeSource,
}

impl Config {
    /// The music folder is `MUSIC_DIR` if set, else the `music_dir` setting
    /// (Settings.pak), else `$SDCARD_PATH/Music` on the device, else ~/Music (or
    /// ./music) on a desktop. `MUSIC_STATE` is set by launch.sh (default
    /// ./state.txt); `MINUI_SETTINGS` overrides where the system volume is read.
    pub fn from_env() -> Self {
        let configured = Settings::load(&settings::default_path()).get(settings::MUSIC_DIR).map(PathBuf::from);
        let music_dir = env::var_os("MUSIC_DIR")
            .map(PathBuf::from)
            .or(configured)
            .or_else(|| env::var_os("SDCARD_PATH").map(|sd| PathBuf::from(sd).join("Music")))
            .unwrap_or_else(|| {
                let home = env::var_os("HOME").map(|h| PathBuf::from(h).join("Music"));
                home.filter(|p| p.is_dir()).unwrap_or_else(|| PathBuf::from("music"))
            });
        let _ = fs::create_dir_all(&music_dir);
        let state_path = env::var_os("MUSIC_STATE").map_or_else(|| PathBuf::from("state.txt"), PathBuf::from);
        let volume = env::var_os("MINUI_SETTINGS")
            .map_or_else(VolumeSource::for_platform, |p| VolumeSource::MinUI(PathBuf::from(p)));
        Self { music_dir, state_path, volume }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctl {
    Prev,
    Play,
    Pause,
    Stop,
    Next,
}

pub struct App {
    pub tracks: Vec<Track>,
    pub sel: usize,
    list_top: usize,
    /// Index of the loaded track.
    pub current: Option<usize>,
    pub info: Option<TrackInfo>,
    pub shuffle: bool,
    pub repeat: Repeat,
    rng: Rng,
    player: Player,
    music_dir: PathBuf,
    state_path: PathBuf,
    volume: VolumeSource,
    /// The system volume as (level, max), when the platform reports one.
    pub system_volume: Option<(u32, u32)>,
    volume_poll: f32,
    bars: [f32; BANDS],
    peaks: [f32; BANDS],
    peak_hold: [f32; BANDS],
    time: f32,
    flash: Option<(String, f32)>,
    pressed: Option<(Ctl, f32)>,
    marquee_px: f32,
    /// Consecutive files that failed to play (to stop skipping eventually).
    failures: usize,
}

impl App {
    pub fn new(config: Config, sink: Box<dyn Sink>) -> Self {
        let tracks = library::scan(&config.music_dir);
        let saved = fs::read_to_string(&config.state_path).unwrap_or_default();
        let get = |key: &str| saved.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(str::trim);
        let last = get("track").map(|t| config.music_dir.join(t));
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let mut app = Self {
            sel: last.and_then(|p| tracks.iter().position(|t| t.path == p)).unwrap_or(0),
            tracks,
            list_top: 0,
            current: None,
            info: None,
            shuffle: get("shuffle") == Some("1"),
            repeat: match get("repeat") {
                Some("all") => Repeat::All,
                Some("one") => Repeat::One,
                _ => Repeat::Off,
            },
            rng: Rng::new(seed),
            // Full scale: loudness is the system volume (VOL+/-), like everywhere in MinUI.
            player: Player::spawn(sink, 100),
            system_volume: config.volume.read(),
            volume_poll: 0.0,
            music_dir: config.music_dir,
            state_path: config.state_path,
            volume: config.volume,
            bars: [0.0; BANDS],
            peaks: [0.0; BANDS],
            peak_hold: [0.0; BANDS],
            time: 0.0,
            flash: None,
            pressed: None,
            marquee_px: 0.0,
            failures: 0,
        };
        app.scroll_to_sel();
        app
    }

    pub fn state(&self) -> State {
        self.player.shared.state()
    }

    pub fn position(&self) -> f64 {
        self.player.shared.position()
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), FLASH_SECS));
    }

    pub fn flash_text(&self) -> Option<&str> {
        self.flash.as_ref().map(|(m, _)| m.as_str())
    }

    pub fn save_state(&self) {
        let track = self
            .current
            .or(Some(self.sel))
            .and_then(|i| self.tracks.get(i))
            .and_then(|t| t.path.strip_prefix(&self.music_dir).ok())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let repeat = match self.repeat {
            Repeat::Off => "off",
            Repeat::All => "all",
            Repeat::One => "one",
        };
        let text = format!(
            "shuffle={}\nrepeat={repeat}\ntrack={track}\n",
            self.shuffle as u8
        );
        if let Some(dir) = self.state_path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&self.state_path, text);
    }

    /// Stops playback (lets the device release the audio output cleanly).
    pub fn shutdown(&mut self) {
        self.player.send(Cmd::Stop);
        self.save_state();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    fn press(&mut self, ctl: Ctl) {
        self.pressed = Some((ctl, 0.18));
    }

    pub fn play(&mut self, index: usize) {
        let Some(track) = self.tracks.get(index) else { return };
        self.player.send(Cmd::Load { path: track.path.clone(), start: 0.0 });
        self.current = Some(index);
        self.sel = index;
        self.info = None;
        self.marquee_px = 0.0;
        self.scroll_to_sel();
    }

    fn scroll_to_sel(&mut self) {
        if self.sel < self.list_top {
            self.list_top = self.sel;
        } else if self.sel >= self.list_top + PL_ROWS {
            self.list_top = self.sel + 1 - PL_ROWS;
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.tracks.len();
        if n == 0 {
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(n as isize) as usize;
        self.scroll_to_sel();
    }

    fn seek_by(&mut self, delta: f64) {
        if self.state() != State::Stopped {
            let to = (self.position() + delta).max(0.0);
            self.player.send(Cmd::Seek(to));
        }
    }

    pub fn handle(&mut self, button: Button) {
        let n = self.tracks.len();
        match button {
            Button::Up => self.move_sel(-1),
            Button::Down => self.move_sel(1),
            Button::A if n > 0 => {
                self.press(Ctl::Play);
                self.play(self.sel);
            }
            Button::Start => match self.state() {
                State::Playing => {
                    self.press(Ctl::Pause);
                    self.player.send(Cmd::Pause);
                }
                State::Paused => {
                    self.press(Ctl::Play);
                    self.player.send(Cmd::Resume);
                }
                State::Stopped if n > 0 => {
                    self.press(Ctl::Play);
                    self.play(self.current.unwrap_or(self.sel));
                }
                State::Stopped => {}
            },
            Button::B => {
                self.press(Ctl::Stop);
                self.player.send(Cmd::Stop);
            }
            Button::L1 if n > 0 => {
                self.press(Ctl::Prev);
                if self.state() != State::Stopped && self.position() > 3.0 {
                    self.player.send(Cmd::Seek(0.0));
                } else {
                    self.play(library::previous(self.current.unwrap_or(self.sel), n));
                }
            }
            Button::R1 if n > 0 => {
                self.press(Ctl::Next);
                let next = library::next(self.current.unwrap_or(self.sel), n, self.shuffle, &mut self.rng);
                self.play(next);
            }
            Button::Left => self.seek_by(-SEEK_STEP),
            Button::Right => self.seek_by(SEEK_STEP),
            Button::X => {
                self.shuffle = !self.shuffle;
                self.flash(if self.shuffle { "Shuffle on" } else { "Shuffle off" });
            }
            Button::Y => {
                self.repeat = self.repeat.next();
                self.flash(match self.repeat {
                    Repeat::Off => "Repeat off",
                    Repeat::All => "Repeat all",
                    Repeat::One => "Repeat this track",
                });
            }
            Button::Select => {
                if let Some(c) = self.current {
                    self.sel = c;
                    self.scroll_to_sel();
                }
            }
            _ => {}
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.marquee_px += dt * 40.0;
        for timer in [self.flash.as_mut().map(|f| &mut f.1), self.pressed.as_mut().map(|p| &mut p.1)].into_iter().flatten() {
            *timer -= dt;
        }
        if self.flash.as_ref().is_some_and(|f| f.1 <= 0.0) {
            self.flash = None;
        }
        if self.pressed.as_ref().is_some_and(|p| p.1 <= 0.0) {
            self.pressed = None;
        }
        self.volume_poll -= dt;
        if self.volume_poll <= 0.0 {
            self.volume_poll = self.volume.poll_secs();
            self.system_volume = self.volume.read();
        }

        while let Ok(ev) = self.player.events.try_recv() {
            match ev {
                Event::Loaded(info) => {
                    self.failures = 0;
                    if let Some(t) = self.current.and_then(|i| self.tracks.get_mut(i)) {
                        if let Some(name) = info.display_name() {
                            t.name = name;
                        }
                        t.duration = info.duration.or(t.duration);
                    }
                    self.info = Some(info);
                }
                Event::Ended => self.advance(),
                Event::Failed(msg) => {
                    eprintln!("playback failed: {msg}");
                    self.flash(msg);
                    self.failures += 1;
                    if self.failures < self.tracks.len() {
                        self.advance();
                    }
                }
            }
        }

        // Analyzer: bars jump up, fall smoothly; peak caps hold, then drop.
        let levels = match self.state() {
            State::Playing => dsp::spectrum(&self.player.shared.samples(), self.player.shared.rate(), BANDS),
            _ => vec![0.0; BANDS],
        };
        for i in 0..BANDS {
            self.bars[i] = levels[i].max(self.bars[i] - dt * 2.2);
            if self.bars[i] >= self.peaks[i] {
                self.peaks[i] = self.bars[i];
                self.peak_hold[i] = 0.5;
            } else if self.peak_hold[i] > 0.0 {
                self.peak_hold[i] -= dt;
            } else {
                self.peaks[i] = (self.peaks[i] - dt * 0.7).max(0.0);
            }
        }
    }

    /// The current track finished (or failed): move on according to shuffle/repeat.
    fn advance(&mut self) {
        let Some(cur) = self.current else { return };
        match library::after(cur, self.tracks.len(), self.shuffle, self.repeat, &mut self.rng) {
            Some(next) => self.play(next),
            None => self.info = None,
        }
    }

    // ------------------------------------------------------------- drawing

    pub fn render(&self, c: &mut Canvas) {
        c.fill(skin::FACE);
        self.draw_main(c);
        self.draw_playlist(c);
    }

    fn duration(&self) -> Option<f64> {
        self.info.as_ref().and_then(|i| i.duration)
    }

    fn draw_main(&self, c: &mut Canvas) {
        let w = W as i32;
        skin::title_bar(c, MAIN_TOP, w, "MUSIC");
        skin::bevel(c, 0, MAIN_TOP + 16, w, PL_TOP - 16, false);

        // LCD: state icon, LED time, format info, analyzer.
        let (lx, ly, lw, lh) = (10, 24, 224, 86);
        skin::inset(c, lx, ly, lw, lh, skin::LCD_BG);
        let state = self.state();
        let state_icon = match state {
            State::Playing => Icon::Play,
            State::Paused => Icon::Pause,
            State::Stopped => Icon::Stop,
        };
        let icon_on = state != State::Paused || (self.time * 2.0) as i32 % 2 == 0;
        skin::icon(c, state_icon, lx + 4, ly + 6, 22, 22, if icon_on { skin::LCD_ON } else { skin::LCD_OFF });
        let shown = (state != State::Stopped).then(|| self.position());
        skin::time_display(c, lx + 32, ly + 6, shown, state != State::Paused || (self.time * 2.0) as i32 % 2 == 0);

        let (kbps, khz, stereo) = match &self.info {
            Some(i) => (
                i.kbps.map_or("---".into(), |k| k.min(9999).to_string()),
                format!("{}", (i.rate as f32 / 1000.0).round()),
                Some(i.channels >= 2),
            ),
            None => ("---".into(), "--".into(), None),
        };
        let ix = lx + 146;
        c.text(ix, ly + 8, &format!("{kbps:>4}"), 1, skin::LCD_ON);
        c.text(ix + 36, ly + 8, "kbps", 1, skin::LCD_DIM);
        c.text(ix, ly + 22, &format!("{khz:>4}"), 1, skin::LCD_ON);
        c.text(ix + 36, ly + 22, "kHz", 1, skin::LCD_DIM);
        c.text(ix, ly + 36, "mono", 1, if stereo == Some(false) { skin::LCD_ON } else { skin::LCD_OFF });
        c.text(ix + 36, ly + 36, "st", 1, if stereo == Some(true) { skin::LCD_ON } else { skin::LCD_OFF });
        self.draw_analyzer(c, lx + 8, ly + 48, 32);

        // Scrolling title.
        let (mx, my, mw, mh) = MARQUEE;
        skin::inset(c, mx, my, mw, mh, skin::LCD_BG);
        let title = match self.current.and_then(|i| self.tracks.get(i).map(|t| (i, t))) {
            Some((i, t)) => match t.duration.or(self.duration()) {
                Some(d) => format!("{}. {} ({})", i + 1, t.name, fmt_time(d)),
                None => format!("{}. {}", i + 1, t.name),
            },
            None if self.tracks.is_empty() => "No music yet - copy some to the SD card".to_string(),
            None => "Press A to play".to_string(),
        };
        let text = if let Some(msg) = self.flash_text() { msg.to_string() } else { title };
        let text_w = text.chars().count() as i32 * 16;
        if text_w <= mw - 8 || self.flash.is_some() {
            skin::text_clipped(c, mx + 4, my + 4, &text, (2, skin::LCD_ON), (mx + 2, mx + mw - 2));
        } else {
            let gap = "  ***  ";
            let full = format!("{text}{gap}");
            let period = full.chars().count() as i32 * 16;
            let off = self.marquee_px as i32 % period;
            for k in 0..2 {
                skin::text_clipped(c, mx + 4 - off + k * period, my + 4, &full, (2, skin::LCD_ON), (mx + 2, mx + mw - 2));
            }
        }

        // Volume, shuffle, repeat.
        c.text(244, 62, "VOL", 1, skin::FACE_TEXT);
        let vol_frac = self.system_volume.map(|(v, max)| v as f32 / max.max(1) as f32);
        skin::slider(c, 274, 56, 164, 18, vol_frac, true);
        self.toggle(c, 450, 54, 84, "SHUFFLE", self.shuffle);
        let rep = match self.repeat {
            Repeat::One => "REPEAT 1",
            _ => "REPEAT",
        };
        self.toggle(c, 542, 54, 88, rep, self.repeat != Repeat::Off);

        // Track counter and total time.
        let count = match self.current {
            Some(i) => format!("Track {} of {}", i + 1, self.tracks.len()),
            None => format!("{} tracks", self.tracks.len()),
        };
        c.text(244, 90, &count, 1, skin::FACE_TEXT_DIM);
        let vol = match self.system_volume {
            Some((v, 100)) => format!("Volume {v}%"),
            Some((v, max)) => format!("Volume {v}/{max}"),
            None => "Volume --".to_string(),
        };
        c.text(630 - 8 * vol.len() as i32, 90, &vol, 1, skin::FACE_TEXT_DIM);

        // Seek bar.
        let frac = match (self.duration(), state) {
            (Some(d), State::Playing | State::Paused) if d > 0.0 => Some((self.position() / d) as f32),
            (_, State::Stopped) => None,
            _ => Some(0.0),
        };
        skin::slider(c, 10, 116, 620, 18, frac, false);

        // Transport buttons.
        let ctls = [(Ctl::Prev, Icon::Prev), (Ctl::Play, Icon::Play), (Ctl::Pause, Icon::Pause), (Ctl::Stop, Icon::Stop), (Ctl::Next, Icon::Next)];
        for (i, (ctl, ic)) in ctls.into_iter().enumerate() {
            let x = 10 + i as i32 * 58;
            let down = self.pressed.is_some_and(|(p, _)| p == ctl);
            skin::bevel(c, x, 144, 52, 36, down);
            let off = if down { 1 } else { 0 };
            skin::icon(c, ic, x + off, 144 + off, 52, 36, skin::FACE_TEXT);
        }
        let hints = [
            "A play   START pause   B stop",
            "L1/R1 track  </> seek  VOL+/- vol",
        ];
        for (i, h) in hints.iter().enumerate() {
            c.text(310, 150 + i as i32 * 14, &fit(h, 40), 1, skin::FACE_TEXT_DIM);
        }
    }

    fn toggle(&self, c: &mut Canvas, x: i32, y: i32, w: i32, label: &str, on: bool) {
        skin::bevel(c, x, y, w, 20, on);
        c.rect(x + 6, y + 7, 6, 6, if on { skin::LCD_ON } else { skin::LCD_OFF });
        c.text(x + 16, y + 6, label, 1, skin::FACE_TEXT);
    }

    fn draw_analyzer(&self, c: &mut Canvas, x: i32, y: i32, h: i32) {
        let (bw, gap) = (8, 2);
        for i in 0..BANDS {
            let bx = x + i as i32 * (bw + gap);
            let bar_h = (self.bars[i] * h as f32) as i32;
            for row in 0..bar_h {
                let t = (row * 255 / h) as u32;
                c.rect(bx, y + h - 1 - row, bw, 1, skin::bar_color(t));
            }
            let peak_y = y + h - 1 - (self.peaks[i] * (h - 1) as f32) as i32;
            if self.peaks[i] > 0.02 {
                c.rect(bx, peak_y, bw, 1, skin::PEAK);
            }
        }
    }

    fn draw_playlist(&self, c: &mut Canvas) {
        let w = W as i32;
        skin::title_bar(c, PL_TOP, w, "PLAYLIST");
        skin::bevel(c, 0, PL_TOP + 16, w, 480 - PL_TOP - 16, false);
        let list_h = PL_ROWS as i32 * ROW_H + 4;
        skin::inset(c, LIST_X, LIST_Y, LIST_W, list_h, skin::LCD_BG);
        let cols = (LIST_W as usize - 8) / 16;

        if self.tracks.is_empty() {
            let lines = [
                "No music found in".to_string(),
                self.music_dir.display().to_string(),
                String::new(),
                "Copy MP3, FLAC, OGG, WAV or M4A".to_string(),
                "files (folders are fine) there.".to_string(),
            ];
            for (i, l) in lines.iter().enumerate() {
                c.text(LIST_X + 4, LIST_Y + 4 + i as i32 * ROW_H, &fit(l, cols), 2, skin::LCD_ON);
            }
        }
        for (row, (i, t)) in self.tracks.iter().enumerate().skip(self.list_top).take(PL_ROWS).enumerate() {
            let y = LIST_Y + 2 + row as i32 * ROW_H;
            if i == self.sel {
                c.rect(LIST_X, y, LIST_W, ROW_H, skin::SEL);
            }
            let color = if Some(i) == self.current { skin::WHITE } else { skin::LCD_ON };
            let dur = t.duration.map(fmt_time).unwrap_or_default();
            let dur_cols = dur.chars().count();
            let name = format!("{}. {}", i + 1, t.name);
            let room = cols - if dur_cols > 0 { dur_cols + 1 } else { 0 };
            c.text(LIST_X + 4, y + 1, &fit(&name, room), 2, color);
            if dur_cols > 0 {
                c.text(LIST_X + LIST_W - 4 - 16 * dur_cols as i32, y + 1, &dur, 2, color);
            }
        }

        // Footer: totals and the rest of the controls.
        let known: f64 = self.tracks.iter().filter_map(|t| t.duration).sum();
        let total = if known > 0.0 { format!("{} tracks, {}+", self.tracks.len(), fmt_time(known)) } else { format!("{} tracks", self.tracks.len()) };
        let fy = LIST_Y + list_h + 8;
        c.text(LIST_X, fy, &total, 1, skin::FACE_TEXT_DIM);
        let hint = "X shuffle  Y repeat  SELECT now playing";
        c.text(w - 8 - 8 * hint.len() as i32, fy, hint, 1, skin::FACE_TEXT_DIM);
    }
}

impl pocketkit_core::App for App {
    fn handle(&mut self, button: Button) {
        App::handle(self, button);
    }
    fn update(&mut self, dt: f32) {
        App::update(self, dt);
    }
    fn render(&self, canvas: &mut Canvas) {
        App::render(self, canvas);
    }
}

/// Used by tests and the snapshot example.
pub fn music_dir_with_wavs(dir: &Path, names: &[&str], secs: f32) {
    let _ = fs::create_dir_all(dir);
    for name in names {
        crate::decoder::write_test_wav(&dir.join(name), 44100, secs);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::audio::NullSink;

    fn app(name: &str, files: &[&str], secs: f32) -> (App, PathBuf) {
        let dir = env::temp_dir().join(format!("music_app_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        music_dir_with_wavs(&dir.join("lib"), files, secs);
        (App::new(config(&dir, "state.txt"), Box::new(NullSink::new(false))), dir)
    }

    fn config(dir: &Path, state: &str) -> Config {
        Config { music_dir: dir.join("lib"), state_path: dir.join(state), volume: VolumeSource::MinUI(dir.join("SharedSettings")) }
    }

    fn wait(app: &mut App, done: impl Fn(&App) -> bool) {
        let start = Instant::now();
        while !done(app) {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            app.update(0.01);
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn plays_through_the_list_then_stops() {
        let (mut app, dir) = app("order", &["a.wav", "b.wav", "c.wav"], 0.2);
        assert_eq!(app.tracks.len(), 3);
        app.handle(Button::A);
        // Done once the last track has loaded (it learns its duration) and ended again.
        wait(&mut app, |a| a.current == Some(2) && a.tracks[2].duration.is_some() && a.info.is_none());
        assert_eq!(app.state(), State::Stopped);
        // Durations were learned while playing.
        assert!(app.tracks.iter().all(|t| t.duration.is_some()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repeat_one_and_settings_are_saved() {
        let (mut app, dir) = app("repeat", &["a.wav", "b.wav"], 0.2);
        app.handle(Button::Y);
        app.handle(Button::Y);
        assert_eq!(app.repeat, Repeat::One);
        app.handle(Button::X);
        app.handle(Button::Down);
        app.handle(Button::A);
        // Plays b, then b again (repeat one), never a.
        wait(&mut app, |a| a.info.is_some());
        app.save_state();
        let saved = fs::read_to_string(dir.join("state.txt")).unwrap();
        assert!(saved.contains("shuffle=1") && saved.contains("repeat=one") && saved.contains("track=b.wav"), "{saved}");
        let again = App::new(config(&dir, "state.txt"), Box::new(NullSink::new(false)));
        assert_eq!((again.sel, again.shuffle, again.repeat), (1, true, Repeat::One));
        app.handle(Button::B);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn follows_the_system_volume() {
        let (mut app, dir) = app("volume", &["a.wav"], 0.1);
        assert_eq!(app.system_volume, None); // not running under MinUI
        fs::write(dir.join("SharedSettings"), pocketkit_platform::minui_settings::settings_bytes(12, 4, 0)).unwrap();
        app.update(0.25);
        assert_eq!(app.system_volume, Some((12, 20)));
        // Headphones plugged in: their own level.
        fs::write(dir.join("SharedSettings"), pocketkit_platform::minui_settings::settings_bytes(12, 4, 1)).unwrap();
        app.update(0.25);
        assert_eq!(app.system_volume, Some((4, 20)));
        // L2/R2 no longer change anything.
        app.handle(Button::L2);
        app.update(0.25);
        assert_eq!(app.system_volume, Some((4, 20)));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_library_and_bad_files_dont_break() {
        let (mut app, dir) = app("empty", &[], 0.1);
        for b in [Button::A, Button::Start, Button::L1, Button::R1, Button::Up, Button::Left] {
            app.handle(b);
        }
        let mut c = Canvas::new(W, 480);
        app.update(0.1);
        app.render(&mut c);
        fs::write(dir.join("lib/broken.mp3"), "nope").unwrap();
        let mut app = App::new(config(&dir, "s.txt"), Box::new(NullSink::new(false)));
        app.handle(Button::A);
        wait(&mut app, |a| a.flash_text().is_some());
        app.render(&mut c);
        fs::remove_dir_all(dir).unwrap();
    }
}
