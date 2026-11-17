//! Shared pieces for the Rust MinUI paks (web-browser, wifi): a 640x480 canvas
//! with an 8x8 font, button input, an on-screen keyboard, and two platform
//! backends (desktop preview window, Miyoo Mini+ framebuffer + evdev).

pub mod input;
pub mod keyboard;
pub mod platform;
pub mod render;

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

/// Runs the ~30 fps loop until MENU is pressed (or the preview window closes).
pub fn run(title: &str, app: &mut impl App) {
    let mut platform = make_platform(title);
    let mut canvas = Canvas::new(W, H);
    let mut pressed = Vec::new();
    let frame_time = Duration::from_millis(33);
    let mut last = Instant::now();

    'main: loop {
        let frame_start = Instant::now();
        pressed.clear();
        if !platform.poll(&mut pressed) {
            break;
        }
        for &button in &pressed {
            if button == Button::Menu {
                break 'main;
            }
            app.handle(button);
        }
        let dt = last.elapsed().as_secs_f32().min(0.25);
        last = Instant::now();
        app.update(dt);
        app.render(&mut canvas);
        platform.present(&canvas);
        if let Some(rest) = frame_time.checked_sub(frame_start.elapsed()) {
            thread::sleep(rest);
        }
    }
}
