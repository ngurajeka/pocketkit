//! The whole app (worker thread + simulated backend), driven by button presses.

use std::time::{Duration, Instant};

use pocketkit_core::input::Button;
use wifi::{
    app::{App, Screen},
    backend::{Fake, FAKE_PASSWORD},
    worker,
};

fn settle(app: &mut App) {
    let start = Instant::now();
    loop {
        app.update(0.01);
        if app.busy.is_none() && app.on.is_some() {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "worker stuck: {:?}", app.busy);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn press(app: &mut App, b: Button) {
    app.handle(b);
    settle(app);
}

#[test]
fn turn_on_scan_connect_forget_turn_off() {
    let (tx, rx) = worker::spawn(Box::new(Fake::new(Duration::ZERO)));
    let mut app = App::new(tx, rx);
    settle(&mut app);
    assert_eq!(app.on, Some(true));
    assert_eq!(app.nets[0].ssid, "HomeNetwork");

    // Wrong password, then the right one.
    press(&mut app, Button::A);
    assert_eq!(app.screen, Screen::Password);
    app.kb.text = "wrongpassword".into();
    press(&mut app, Button::Start);
    assert!(app.flash_text().unwrap().contains("Check the password"));
    assert!(!app.status.connected());

    press(&mut app, Button::A);
    app.kb.text = FAKE_PASSWORD.into();
    press(&mut app, Button::Start);
    assert!(app.flash_text().unwrap().contains("192.168.1.42"), "{:?}", app.flash_text());
    assert!(app.status.connected());
    assert!(app.saved.iter().any(|s| s.ssid == "HomeNetwork"));

    // Saved now: A reconnects without asking; Y forgets.
    press(&mut app, Button::A);
    assert_eq!(app.screen, Screen::List);
    press(&mut app, Button::Y);
    assert!(app.flash_text().unwrap().starts_with("Forgot"));
    assert!(!app.saved.iter().any(|s| s.ssid == "HomeNetwork"));

    press(&mut app, Button::Start);
    assert_eq!(app.on, Some(false));
    assert!(app.nets.is_empty());
    press(&mut app, Button::Start);
    assert_eq!(app.on, Some(true));
    assert!(!app.nets.is_empty());
}
