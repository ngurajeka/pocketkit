//! Shared pieces for the Rust MinUI paks: a 640x480 canvas with an 8x8 font,
//! button input, an on-screen keyboard, two platform backends (desktop preview
//! window, Miyoo Mini+ framebuffer + evdev), MinUI's shared settings (volume,
//! brightness), our paks' own settings file, and the main loop with screen-off.

pub mod input;
pub mod keyboard;
pub mod minui;
pub mod platform;
pub mod render;
pub mod settings;
pub mod ui;

use std::{
    thread,
    time::{Duration, Instant},
};

use input::Button;
use render::{Canvas, H, W};

/// What a pak's main screen object provides to [`run`].
pub trait App {
    fn handle(&mut self, button: Button);
    fn update(&mut self, dt: f32);
    fn render(&self, canvas: &mut Canvas);
    /// Return true to leave (e.g. after B on the top screen). MENU always quits.
    fn quit(&self) -> bool {
        false
    }
}

#[cfg(feature = "desktop")]
fn make_platform(title: &str) -> Box<dyn platform::Platform> {
    Box::new(platform::desktop::Desktop::new(title))
}

#[cfg(not(feature = "desktop"))]
fn make_platform(_title: &str) -> Box<dyn platform::Platform> {
    match platform::device::Device::open() {
        Ok(device) => Box::new(device),
        Err(e) => {
            eprintln!("could not open framebuffer/input: {e}");
            std::process::exit(1);
        }
    }
}

/// How often `run` re-reads the screen-off setting (Settings.pak may change it).
const SETTINGS_POLL: Duration = Duration::from_secs(3);

/// Screen on/off bookkeeping for [`run`]: off after `timeout` without input,
/// or with the power button; the first press while off only wakes it.
#[derive(Debug)]
pub struct Screen {
    pub on: bool,
    idle: Duration,
    timeout: Option<Duration>,
}

impl Screen {
    pub fn new(timeout_secs: u32) -> Self {
        let mut s = Self { on: true, idle: Duration::ZERO, timeout: None };
        s.set_timeout(timeout_secs);
        s
    }

    pub fn set_timeout(&mut self, secs: u32) {
        self.timeout = (secs > 0).then(|| Duration::from_secs(secs as u64));
    }

    /// Feeds a button press. Returns true if the app should see it.
    pub fn press(&mut self, button: Button) -> bool {
        self.idle = Duration::ZERO;
        if !self.on {
            self.on = true;
            return false;
        }
        if button == Button::Power {
            self.on = false;
            return false;
        }
        true
    }

    /// Advances the idle timer; may switch the screen off.
    pub fn tick(&mut self, dt: Duration) {
        self.idle += dt;
        if self.on && self.timeout.is_some_and(|t| self.idle >= t) {
            self.on = false;
        }
    }
}

/// Runs the ~30 fps loop until MENU is pressed (or the preview window closes).
/// The screen turns off after the `screen_off` setting's idle time or with the
/// power button; the app keeps running (music keeps playing) and any button
/// wakes it.
pub fn run(title: &str, app: &mut impl App) {
    let mut platform = make_platform(title);
    let mut canvas = Canvas::new(W, H);
    let black = Canvas::new(W, H);
    let mut pressed = Vec::new();
    let frame_time = Duration::from_millis(33);
    let settings_path = settings::default_path();
    let mut screen = Screen::new(settings::Settings::load(&settings_path).screen_off_secs());
    let mut settings_checked = Instant::now();
    let mut last = Instant::now();

    'main: loop {
        let frame_start = Instant::now();
        pressed.clear();
        if !platform.poll(&mut pressed) {
            break;
        }
        let was_on = screen.on;
        for &button in &pressed {
            if !screen.press(button) {
                continue;
            }
            if button == Button::Menu {
                break 'main;
            }
            app.handle(button);
            if app.quit() {
                break 'main;
            }
        }
        let dt = last.elapsed().min(Duration::from_millis(250));
        last = Instant::now();
        screen.tick(dt);
        if settings_checked.elapsed() >= SETTINGS_POLL {
            settings_checked = Instant::now();
            screen.set_timeout(settings::Settings::load(&settings_path).screen_off_secs());
        }
        if screen.on != was_on {
            if !screen.on {
                platform.present(&black);
            }
            platform.set_backlight(screen.on);
        }

        app.update(dt.as_secs_f32());
        if screen.on {
            app.render(&mut canvas);
            platform.present(&canvas);
        }
        // Screen off: wake up less often, but stay responsive to buttons.
        let budget = if screen.on { frame_time } else { Duration::from_millis(100) };
        if let Some(rest) = budget.checked_sub(frame_start.elapsed()) {
            thread::sleep(rest);
        }
    }
    if !screen.on {
        platform.set_backlight(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_turns_off_when_idle_and_wakes_on_any_button() {
        let mut s = Screen::new(30);
        s.tick(Duration::from_secs(29));
        assert!(s.on);
        assert!(s.press(Button::A)); // resets the idle timer
        s.tick(Duration::from_secs(29));
        assert!(s.on);
        s.tick(Duration::from_secs(2));
        assert!(!s.on);
        assert!(!s.press(Button::A)); // wakes, not passed on
        assert!(s.on);
        assert!(!s.press(Button::Power)); // power: off again
        assert!(!s.on);
        let mut never = Screen::new(0);
        never.tick(Duration::from_secs(100_000));
        assert!(never.on);
    }
}
