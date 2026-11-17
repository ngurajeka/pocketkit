//! The settings list: Up/Down to pick a row, Left/Right (or A) to change it.
//! Every change is saved immediately.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use pocketkit_platform::minui_settings as minui;
use pocketkit_core::{
    input::Button,
    keyboard::{KbResult, Keyboard},
    render::{Canvas, W},

    settings::{self, Settings},
    ui::{self, Ui},
};

use crate::autostart::{self, Startup};

/// Rows shown at once (the rest scroll); room is left for the help text.
const VISIBLE_ROWS: usize = 4;
const FLASH_SECS: f32 = 2.5;

pub const SCREEN_OFF_CHOICES: [(u32, &str); 6] =
    [(0, "Never"), (30, "30 s"), (60, "1 min"), (120, "2 min"), (300, "5 min"), (600, "10 min")];
pub const SEARCH_CHOICES: [(&str, &str); 3] = [
    ("Brave", "https://search.brave.com/search?q="),
    ("DuckDuckGo Lite", "https://lite.duckduckgo.com/lite/?q="),
    ("Wiby", "https://wiby.me/?q="),
];

/// Where everything lives. On the device these come from MinUI's environment.
#[derive(Debug, Clone)]
pub struct Paths {
    pub userdata: PathBuf,
    pub sdcard: PathBuf,
    pub tools: PathBuf,
    pub shared: PathBuf,
    pub settings: PathBuf,
    pub modes: PathBuf,
}

impl Paths {
    /// On a desktop everything goes into ./sandbox, so nothing real is touched.
    pub fn from_env() -> Self {
        let on_device = env::var_os("USERDATA_PATH").is_some();
        let sandbox = PathBuf::from("sandbox");
        let userdata = env::var_os("USERDATA_PATH").map_or_else(|| sandbox.join("userdata"), PathBuf::from);
        let sdcard = env::var_os("SDCARD_PATH").map_or_else(|| sandbox.join("sdcard"), PathBuf::from);
        let platform = env::var("PLATFORM").unwrap_or_else(|_| "miyoomini".into());
        Self {
            tools: sdcard.join("Tools").join(platform),
            shared: if on_device { minui::shared_path() } else { sandbox.join("SharedSettings") },
            settings: if on_device { settings::default_path() } else { userdata.join("minui-paks.txt") },
            modes: PathBuf::from("/sys/class/graphics/fb0/modes"),
            userdata,
            sdcard,
        }
    }

    fn flag(&self, name: &str) -> PathBuf {
        self.userdata.join(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    ScreenOff,
    Brightness,
    WifiAtStartup,
    AdbAtStartup,
    Rtc,
    Hd560,
    MusicDir,
    Search,
}

impl Row {
    fn label(self) -> &'static str {
        match self {
            Row::ScreenOff => "Screen off after",
            Row::Brightness => "Brightness",
            Row::WifiAtStartup => "Wi-Fi at startup",
            Row::AdbAtStartup => "ADB at startup",
            Row::Rtc => "Hardware clock",
            Row::Hd560 => "560p screen mode",
            Row::MusicDir => "Music folder",
            Row::Search => "Browser search",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Row::ScreenOff => "Our paks only (Music, Browser, Wi-Fi...). MinUI itself always sleeps after 30 s. POWER turns the screen off now.",
            Row::Brightness => "MinUI's brightness, same as START + L/R.",
            Row::WifiAtStartup => "Connects to your saved networks when the Mini boots, in the background.",
            Row::AdbAtStartup => "Starts ADB with Wi-Fi so deploy.sh works right after boot. No password: only use on a trusted network.",
            Row::Rtc => "Use the Mini+'s battery-backed clock instead of the last saved time. Applies at the next boot.",
            Row::Hd560 => "Use the 752x560 mode of newer screens. Applies when MinUI restarts.",
            Row::MusicDir => "Where Music looks for songs. Press A to type a folder.",
            Row::Search => "Search engine for typed searches in Web Browser.",
        }
    }
}

pub struct App {
    pub paths: Paths,
    pub rows: Vec<Row>,
    pub sel: usize,
    pub settings: Settings,
    pub startup: Startup,
    pub brightness: Option<u32>,
    pub rtc: bool,
    pub hd560: bool,
    pub editing: Option<Keyboard>,
    /// First visible row.
    top: usize,
    quitting: bool,
    ui: Ui,
    flash: Option<(String, f32)>,
    time: f32,
}

fn has_mode(modes: &Path, mode: &str) -> bool {
    fs::read_to_string(modes).is_ok_and(|m| m.contains(mode))
}

impl App {
    pub fn new(paths: Paths) -> Self {
        let mut rows = vec![Row::ScreenOff, Row::Brightness, Row::WifiAtStartup, Row::AdbAtStartup, Row::Rtc];
        if has_mode(&paths.modes, "752x560p") {
            rows.push(Row::Hd560);
        }
        rows.extend([Row::MusicDir, Row::Search]);
        Self {
            settings: Settings::load(&paths.settings),
            startup: autostart::load(&paths.userdata),
            brightness: minui::read_brightness(&paths.shared),
            rtc: paths.flag("enable-rtc").exists(),
            hd560: paths.flag("enable-560p").exists(),
            paths,
            rows,
            sel: 0,
            editing: None,
            top: 0,
            quitting: false,
            ui: Ui::new(),
            flash: None,
            time: 0.0,
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), FLASH_SECS));
    }

    pub fn flash_text(&self) -> Option<&str> {
        self.flash.as_ref().map(|(m, _)| m.as_str())
    }

    fn report(&mut self, result: std::io::Result<()>, ok: &str) {
        match result {
            Ok(()) => self.flash(ok),
            Err(e) => self.flash(format!("Couldn't save: {e}")),
        }
    }

    fn save_settings(&mut self, ok: &str) {
        let r = self.settings.save(&self.paths.settings);
        self.report(r, ok);
    }

    fn adbd_installed(&self) -> bool {
        self.paths.tools.join("ADBD.pak/adbd").exists()
    }

    pub fn screen_off(&self) -> u32 {
        self.settings.screen_off_secs()
    }

    pub fn music_dir(&self) -> String {
        self.settings
            .get(settings::MUSIC_DIR)
            .map(str::to_string)
            .unwrap_or_else(|| self.paths.sdcard.join("Music").display().to_string())
    }

    fn search_index(&self) -> usize {
        let url = self.settings.get(settings::SEARCH_URL).unwrap_or(SEARCH_CHOICES[0].1);
        SEARCH_CHOICES.iter().position(|(_, u)| *u == url).unwrap_or(0)
    }

    fn set_flag(&mut self, name: &str, on: bool) -> std::io::Result<()> {
        let f = self.paths.flag(name);
        if on {
            fs::create_dir_all(&self.paths.userdata)?;
            fs::write(f, "")
        } else {
            match fs::remove_file(f) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            }
        }
    }

    fn set_startup(&mut self, s: Startup) {
        let r = autostart::save(&self.paths.userdata, s);
        if r.is_ok() {
            self.startup = s;
        }
        self.report(r, "Saved to auto.sh (backed up first)");
    }

    /// Left (-1) / Right (+1) / A (0, toggle or edit) on the selected row.
    fn change(&mut self, dir: i32) {
        let Some(&row) = self.rows.get(self.sel) else { return };
        let step = |i: usize, n: usize| -> usize {
            match dir {
                -1 => (i + n - 1) % n,
                _ => (i + 1) % n,
            }
        };
        match row {
            Row::ScreenOff => {
                let n = SCREEN_OFF_CHOICES.len();
                let i = SCREEN_OFF_CHOICES.iter().position(|(s, _)| *s == self.screen_off()).unwrap_or(2);
                let (secs, label) = SCREEN_OFF_CHOICES[step(i, n)];
                self.settings.set(settings::SCREEN_OFF, &secs.to_string());
                self.save_settings(&format!("Screen off after: {label}"));
            }
            Row::Brightness => {
                let Some(b) = self.brightness else {
                    self.flash("MinUI isn't running");
                    return;
                };
                let new = match dir {
                    -1 => b.saturating_sub(1),
                    _ => (b + 1).min(minui::BRIGHTNESS_MAX),
                };
                let saved = self.paths.userdata.join("msettings.bin");
                let r = minui::set_brightness(&self.paths.shared, Some(&saved), new);
                if r.is_ok() {
                    self.brightness = Some(new);
                }
                if let Err(e) = r {
                    self.flash(format!("Couldn't set brightness: {e}"));
                }
            }
            Row::WifiAtStartup => {
                let wifi = !self.startup.wifi;
                if wifi && !self.paths.tools.join("Wifi.pak/wifi").exists() {
                    self.flash("Wifi.pak isn't in Tools");
                    return;
                }
                // ADB needs Wi-Fi.
                self.set_startup(Startup { wifi, adb: self.startup.adb && wifi });
            }
            Row::AdbAtStartup => {
                let adb = !self.startup.adb;
                if adb && !self.adbd_installed() {
                    self.flash("Install ADBD.pak (MinUI Extras) first");
                    return;
                }
                self.set_startup(Startup { wifi: self.startup.wifi || adb, adb });
            }
            Row::Rtc => {
                let r = self.set_flag("enable-rtc", !self.rtc);
                if r.is_ok() {
                    self.rtc = !self.rtc;
                }
                self.report(r, "Saved, applies at the next boot");
            }
            Row::Hd560 => {
                let r = self.set_flag("enable-560p", !self.hd560);
                if r.is_ok() {
                    self.hd560 = !self.hd560;
                }
                self.report(r, "Saved, applies when MinUI restarts");
            }
            Row::MusicDir => {
                if dir == 0 {
                    self.editing = Some(Keyboard::new(&self.music_dir()));
                }
            }
            Row::Search => {
                let (name, url) = SEARCH_CHOICES[step(self.search_index(), SEARCH_CHOICES.len())];
                self.settings.set(settings::SEARCH_URL, url);
                self.save_settings(&format!("Search: {name}"));
            }
        }
    }

    pub fn handle(&mut self, button: Button) {
        if let Some(kb) = &mut self.editing {
            match kb.handle(button) {
                KbResult::Pending => {}
                KbResult::Cancel => self.editing = None,
                KbResult::Submit(text) => {
                    self.editing = None;
                    let path = text.trim().trim_end_matches('/');
                    if !path.starts_with('/') {
                        self.flash("Use a full path, like /mnt/SDCARD/Music");
                        return;
                    }
                    self.settings.set(settings::MUSIC_DIR, path);
                    let msg = if Path::new(path).is_dir() { "Music folder saved" } else { "Saved (Music will create it)" };
                    self.save_settings(msg);
                }
            }
            return;
        }
        let n = self.rows.len();
        match button {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            Button::Left => self.change(-1),
            Button::Right => self.change(1),
            Button::A => self.change(0),
            Button::B => self.quitting = true,
            _ => {}
        }
        // Keep the selection on screen.
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + VISIBLE_ROWS {
            self.top = self.sel + 1 - VISIBLE_ROWS;
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some((_, t)) = &mut self.flash {
            *t -= dt;
            if *t <= 0.0 {
                self.flash = None;
            }
        }
        // Brightness may also change with START + L/R while we're open.
        if (self.time * 4.0) as i32 % 4 == 0 {
            if let Some(b) = minui::read_brightness(&self.paths.shared) {
                self.brightness = Some(b);
            }
        }
    }

    pub fn value(&self, row: Row) -> String {
        let on_off = |b: bool| if b { "On" } else { "Off" }.to_string();
        match row {
            Row::ScreenOff => SCREEN_OFF_CHOICES
                .iter()
                .find(|(s, _)| *s == self.screen_off())
                .map_or_else(|| format!("{} s", self.screen_off()), |(_, l)| l.to_string()),
            Row::Brightness => self.brightness.map_or("--".into(), |b| format!("{b}/{}", minui::BRIGHTNESS_MAX)),
            Row::WifiAtStartup => on_off(self.startup.wifi),
            Row::AdbAtStartup if !self.adbd_installed() && !self.startup.adb => "No ADBD.pak".into(),
            Row::AdbAtStartup => on_off(self.startup.adb),
            Row::Rtc => on_off(self.rtc),
            Row::Hd560 => on_off(self.hd560),
            Row::MusicDir => self.music_dir(),
            Row::Search => SEARCH_CHOICES[self.search_index()].0.to_string(),
        }
    }

    // ------------------------------------------------------------- drawing

    pub fn render(&self, c: &mut Canvas) {
        if let Some(kb) = &self.editing {
            self.ui.keyboard(c, kb, "Music folder", "SAVE", self.time);
            if let Some(m) = self.flash_text() {
                self.ui.toast(c, m);
            }
            return;
        }
        self.ui.clear(c);
        self.ui.title(c, "Settings");
        let first = self.top;
        for (i, &row) in self.rows.iter().enumerate().skip(first).take(VISIBLE_ROWS) {
            let selected = i == self.sel;
            let value = self.value(row);
            let value = if row == Row::MusicDir {
                self.ui.font.truncate_start(&value, ui::FONT_SMALL, 260)
            } else if selected {
                format!("<  {value}  >")
            } else {
                value
            };
            self.ui.row(c, i - first, row.label(), selected, Some(&value), false);
        }
        self.ui.scroll_marks(c, first, VISIBLE_ROWS, self.rows.len());
        // Help for the selected row, under the list.
        if let Some(row) = self.rows.get(self.sel) {
            let y0 = Ui::row_y(VISIBLE_ROWS) + ui::PADDING / 2;
            let lh = self.ui.font.height(ui::FONT_TINY) + 2;
            let max = W as i32 - 2 * ui::PADDING - 2 * ui::BUTTON_PADDING;
            for (k, line) in self.ui.font.wrap(row.help(), ui::FONT_TINY, max).iter().take(2).enumerate() {
                self.ui.font.draw(c, ui::PADDING + ui::BUTTON_PADDING, y0 + k as i32 * lh, line, ui::FONT_TINY, self.ui.pal.soft);
            }
        }
        if let Some(m) = self.flash_text() {
            self.ui.toast(c, m);
        }
        let action = match self.rows.get(self.sel) {
            Some(Row::MusicDir) => "EDIT",
            Some(Row::ScreenOff | Row::Brightness | Row::Search) => "NEXT",
            _ => "TOGGLE",
        };
        self.ui.hints(c, &[("B", "BACK")], false);
        self.ui.hints(c, &[("A", action)], true);
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
    fn quit(&self) -> bool {
        self.quitting
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketkit_core::render::H;

    fn sandbox(name: &str, with_560p: bool) -> (App, PathBuf) {
        let dir = env::temp_dir().join(format!("settings_app_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let userdata = dir.join("userdata");
        let tools = dir.join("sd/Tools/miyoomini");
        fs::create_dir_all(&userdata).unwrap();
        fs::create_dir_all(tools.join("Wifi.pak")).unwrap();
        fs::create_dir_all(tools.join("ADBD.pak")).unwrap();
        fs::write(tools.join("Wifi.pak/wifi"), "").unwrap();
        fs::write(tools.join("ADBD.pak/adbd"), "").unwrap();
        fs::write(dir.join("SharedSettings"), minui::settings_bytes(20, 20, 0)).unwrap();
        fs::write(userdata.join("msettings.bin"), minui::settings_bytes(20, 20, 0)).unwrap();
        fs::write(dir.join("modes"), if with_560p { "U:640x480p-60\nU:752x560p-60\n" } else { "U:640x480p-60\n" }).unwrap();
        fs::write(userdata.join("auto.sh"), "#!/bin/sh\necho mine\n").unwrap();
        let paths = Paths {
            settings: userdata.join("minui-paks.txt"),
            userdata,
            sdcard: dir.join("sd"),
            tools,
            shared: dir.join("SharedSettings"),
            modes: dir.join("modes"),
        };
        (App::new(paths), dir)
    }

    fn select(app: &mut App, row: Row) {
        app.sel = app.rows.iter().position(|r| *r == row).unwrap();
    }

    #[test]
    fn screen_off_and_search_are_saved_for_the_other_paks() {
        let (mut app, dir) = sandbox("shared", false);
        assert_eq!(app.value(Row::ScreenOff), "1 min");
        select(&mut app, Row::ScreenOff);
        app.handle(Button::Right);
        app.handle(Button::Right);
        let saved = Settings::load(&app.paths.settings);
        assert_eq!(saved.screen_off_secs(), 300);
        for _ in 0..4 {
            app.handle(Button::Left);
        }
        assert_eq!(app.value(Row::ScreenOff), "Never");
        app.handle(Button::Left);
        assert_eq!(app.value(Row::ScreenOff), "10 min"); // wraps around

        select(&mut app, Row::Search);
        app.handle(Button::Right);
        assert_eq!(Settings::load(&app.paths.settings).get(settings::SEARCH_URL), Some(SEARCH_CHOICES[1].1));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn startup_rows_edit_auto_sh_and_adb_implies_wifi() {
        let (mut app, dir) = sandbox("startup", false);
        select(&mut app, Row::AdbAtStartup);
        app.handle(Button::A);
        assert_eq!(app.startup, Startup { wifi: true, adb: true });
        let text = fs::read_to_string(app.paths.userdata.join("auto.sh")).unwrap();
        assert!(text.starts_with("#!/bin/sh\necho mine\n") && text.contains("adbd"));
        assert!(app.paths.userdata.join(autostart::BACKUP_NAME).exists());
        // Wi-Fi off turns ADB off too.
        select(&mut app, Row::WifiAtStartup);
        app.handle(Button::A);
        assert_eq!(app.startup, Startup::default());
        assert_eq!(fs::read_to_string(app.paths.userdata.join("auto.sh")).unwrap(), "#!/bin/sh\necho mine\n");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn brightness_flags_and_560p_row() {
        let (mut app, dir) = sandbox("minui", true);
        assert!(app.rows.contains(&Row::Hd560));
        select(&mut app, Row::Brightness);
        assert_eq!(app.value(Row::Brightness), "3/10");
        app.handle(Button::Right);
        assert_eq!(minui::read_brightness(&app.paths.shared), Some(4));
        assert_eq!(minui::read_brightness(&app.paths.userdata.join("msettings.bin")), Some(4));
        select(&mut app, Row::Rtc);
        app.handle(Button::A);
        assert!(app.paths.userdata.join("enable-rtc").exists());
        app.handle(Button::A);
        assert!(!app.paths.userdata.join("enable-rtc").exists());
        let (plain, dir2) = sandbox("no560", false);
        assert!(!plain.rows.contains(&Row::Hd560));
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(dir2).unwrap();
    }

    #[test]
    fn music_folder_is_typed_in() {
        let (mut app, dir) = sandbox("music", false);
        select(&mut app, Row::MusicDir);
        assert!(app.value(Row::MusicDir).ends_with("sd/Music"));
        app.handle(Button::A);
        app.editing.as_mut().unwrap().text = "/mnt/SDCARD/Songs/".into();
        app.handle(Button::Start);
        assert!(app.editing.is_none());
        assert_eq!(Settings::load(&app.paths.settings).get(settings::MUSIC_DIR), Some("/mnt/SDCARD/Songs"));
        app.handle(Button::A);
        app.editing.as_mut().unwrap().text = "relative".into();
        app.handle(Button::Start);
        assert!(app.flash_text().unwrap().contains("full path"));
        let mut c = Canvas::new(W, H);
        app.render(&mut c);
        fs::remove_dir_all(dir).unwrap();
    }
}
