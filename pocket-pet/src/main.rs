use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use pocket_pet::{
    app::App,
    input::Button,
    platform::Platform,
    render::{Canvas, H, W},
};

#[cfg(feature = "desktop")]
fn make_platform() -> Box<dyn Platform> {
    Box::new(pocket_pet::platform::desktop::Desktop::new())
}

#[cfg(not(feature = "desktop"))]
fn make_platform() -> Box<dyn Platform> {
    match pocket_pet::platform::device::Device::open() {
        Ok(device) => Box::new(device),
        Err(e) => {
            eprintln!("could not open framebuffer/input: {e}");
            std::process::exit(1);
        }
    }
}

fn main() {
    let save_path = std::env::var_os("PET_SAVE_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("pet_save.txt"));
    // Dev helper: PET_SPEED=60 makes the pet's needs run 60x faster.
    let speed: f32 = std::env::var("PET_SPEED").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let mut platform = make_platform();
    let mut app = App::load(save_path);
    let mut canvas = Canvas::new(W, H);
    let mut pressed = Vec::new();

    let frame_time = Duration::from_millis(33); // ~30 fps is plenty for a pet
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
        app.update(dt * speed);
        app.render(&mut canvas);
        platform.present(&canvas);

        if let Some(rest) = frame_time.checked_sub(frame_start.elapsed()) {
            thread::sleep(rest);
        }
    }

    app.save();
}
