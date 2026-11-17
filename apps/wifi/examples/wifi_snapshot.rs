//! Renders the screens (with simulated networks) to `snapshots/*.ppm`:
//! `cargo run --example snapshot`.

use std::{fs, time::Duration};

use pocketkit_core::{
    input::Button,
    render::{Canvas, H, W},
};
use wifi::{app::App, backend::Fake, worker};

fn save_ppm(name: &str, c: &Canvas) {
    let mut out = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
    for &p in &c.px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    fs::write(format!("snapshots/{name}.ppm"), out).expect("write snapshot");
}

fn settle(app: &mut App) {
    for _ in 0..500 {
        app.update(0.01);
        if app.busy.is_none() && app.on.is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn main() {
    fs::create_dir_all("snapshots").unwrap();
    let (tx, rx) = worker::spawn(Box::new(Fake::new(Duration::ZERO)));
    let mut app = App::new(tx, rx);
    let mut c = Canvas::new(W, H);
    settle(&mut app);

    app.handle(Button::Down);
    app.handle(Button::A); // connect to the saved open network
    settle(&mut app);
    app.handle(Button::Up);
    app.render(&mut c);
    save_ppm("list", &c);

    app.handle(Button::A);
    for b in [Button::A, Button::Right, Button::A, Button::Down, Button::A] {
        app.handle(b);
    }
    app.render(&mut c);
    save_ppm("password", &c);
    println!("wrote snapshots/{{list,password}}.ppm");
}
