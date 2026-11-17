//! Renders the player (with a few generated tone files) to `snapshots/*.ppm`:
//! `cargo run --example snapshot`.

use std::{fs, thread, time::Duration};

use pocketkit_core::{
    input::Button,
    render::{Canvas, H, W},
};
use music::{
    app::{music_dir_with_wavs, App, Config},
    audio::NullSink,
};

fn save_ppm(name: &str, c: &Canvas) {
    let mut out = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
    for &p in &c.px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    fs::write(format!("snapshots/{name}.ppm"), out).expect("write snapshot");
}

fn main() {
    fs::create_dir_all("snapshots").unwrap();
    let dir = std::env::temp_dir().join("music_snapshot");
    let _ = fs::remove_dir_all(&dir);
    let names = [
        "01 Morning Drive.wav",
        "02 Pixel Rain (Extended Club Mix featuring Somebody).wav",
        "03 Night Bus.wav",
        "04 Lo-Fi Loop.wav",
        "05 Arcade Sunset.wav",
    ];
    music_dir_with_wavs(&dir, &names, 20.0);
    let config = Config { music_dir: dir.clone(), state_path: dir.join("state.txt"), volume: pocketkit_platform::volume::VolumeSource::None };
    let mut app = App::new(config, Box::new(NullSink::new(true)));
    let mut c = Canvas::new(W, H);

    app.render(&mut c);
    save_ppm("stopped", &c);

    app.handle(Button::Down);
    app.handle(Button::A);
    app.handle(Button::X);
    for _ in 0..90 {
        app.update(1.0 / 30.0);
        thread::sleep(Duration::from_millis(33));
    }
    app.handle(Button::Down);
    app.handle(Button::Down);
    app.render(&mut c);
    save_ppm("playing", &c);
    // One shot per colour theme.
    for (i, theme) in music::theme::THEMES.iter().enumerate() {
        app.theme = i;
        app.render(&mut c);
        save_ppm(&format!("theme-{}", theme.name.to_lowercase()), &c);
    }
    app.theme = 0;
    app.shutdown();
    let _ = fs::remove_dir_all(&dir);
    println!("wrote snapshots/{{stopped,playing}}.ppm");
}
