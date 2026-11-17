//! Renders the settings list to `snapshots/settings.ppm` (in a temp sandbox):
//! `cargo run --example snapshot`.

use std::fs;

use minui_common::{
    input::Button,
    minui,
    render::{Canvas, H, W},
};
use settings::app::{App, Paths};

fn main() {
    let dir = std::env::temp_dir().join("settings_snapshot");
    let _ = fs::remove_dir_all(&dir);
    let userdata = dir.join("userdata");
    fs::create_dir_all(&userdata).unwrap();
    fs::create_dir_all(dir.join("sd/Tools/miyoomini/ADBD.pak")).unwrap();
    fs::write(dir.join("sd/Tools/miyoomini/ADBD.pak/adbd"), "").unwrap();
    fs::write(dir.join("SharedSettings"), minui::settings_bytes(20, 20, 0)).unwrap();
    fs::write(dir.join("modes"), "U:752x560p-60\n").unwrap();
    let paths = Paths {
        settings: userdata.join("minui-paks.txt"),
        userdata,
        sdcard: "/mnt/SDCARD".into(),
        tools: dir.join("sd/Tools/miyoomini"),
        shared: dir.join("SharedSettings"),
        modes: dir.join("modes"),
    };
    let mut app = App::new(paths);
    app.handle(Button::Down);
    let mut c = Canvas::new(W, H);
    app.render(&mut c);
    fs::create_dir_all("snapshots").unwrap();
    let mut out = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
    for &p in &c.px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    fs::write("snapshots/settings.ppm", out).unwrap();
    let _ = fs::remove_dir_all(&dir);
    println!("wrote snapshots/settings.ppm");
}
