//! Renders one frame per mood to `snapshots/*.ppm` so you can eyeball the art
//! without opening a window: `cargo run --example snapshot`.

use std::fs;

use pocket_pet::{
    game::{Action, Pet},
    render::{draw_scene, Canvas, H, W},
};

fn save_ppm(name: &str, c: &Canvas) {
    let mut out = format!("P6\n{} {}\n255\n", c.w, c.h).into_bytes();
    for &p in &c.px {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    fs::write(name, out).expect("write snapshot");
}

fn main() {
    fs::create_dir_all("snapshots").unwrap();

    let mut sleeping = Pet { energy: 20.0, ..Pet::default() };
    sleeping.perform(Action::Sleep);
    let mut eating = Pet { fullness: 40.0, ..Pet::default() };
    eating.perform(Action::Feed);
    let mut playing = Pet::default();
    playing.perform(Action::Play);
    let mut bathing = Pet { cleanliness: 30.0, ..Pet::default() };
    bathing.perform(Action::Clean);

    let states = [
        ("happy", Pet::default()),
        ("hungry", Pet { fullness: 10.0, ..Pet::default() }),
        ("dirty", Pet { cleanliness: 10.0, ..Pet::default() }),
        ("tired", Pet { energy: 10.0, ..Pet::default() }),
        ("sad", Pet { happiness: 10.0, ..Pet::default() }),
        ("sleeping", sleeping),
        ("eating", eating),
        ("playing", playing),
        ("bathing", bathing),
    ];

    let mut canvas = Canvas::new(W, H);
    for (i, (name, pet)) in states.iter().enumerate() {
        draw_scene(&mut canvas, pet, i % 4, 1.3, 0.0);
        save_ppm(&format!("snapshots/{name}.ppm"), &canvas);
    }
    println!("wrote {} snapshots to ./snapshots", states.len());
}
