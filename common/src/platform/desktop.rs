//! Desktop preview window so you can iterate without touching the device.
//! Keys: arrows = D-pad, Z/Space = A, X = B, C = X, V = Y, Q = L1, W = R1,
//! Enter = Start, Tab = Select, Esc = Menu (quit).

use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};

use super::Platform;
use crate::{
    input::Button,
    render::{Canvas, H, W},
};

pub struct Desktop {
    window: Window,
}

impl Desktop {
    pub fn new(title: &str) -> Self {
        let window = Window::new(
            &format!("{title} (desktop preview)"),
            W,
            H,
            WindowOptions { scale: Scale::X1, ..WindowOptions::default() },
        )
        .expect("failed to open window");
        Self { window }
    }
}

fn map(key: Key) -> Option<Button> {
    Some(match key {
        Key::Up => Button::Up,
        Key::Down => Button::Down,
        Key::Left => Button::Left,
        Key::Right => Button::Right,
        Key::Z | Key::Space => Button::A,
        Key::X => Button::B,
        Key::C => Button::X,
        Key::V => Button::Y,
        Key::Q => Button::L1,
        Key::W => Button::R1,
        Key::Enter => Button::Start,
        Key::Tab => Button::Select,
        Key::Escape => Button::Menu,
        _ => return None,
    })
}

impl Platform for Desktop {
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool {
        // Held keys repeat only for the ones that repeat on the device too.
        for key in self.window.get_keys_pressed(KeyRepeat::Yes) {
            if let Some(button) = map(key) {
                if button.repeats() || self.window.is_key_pressed(key, KeyRepeat::No) {
                    pressed.push(button);
                }
            }
        }
        self.window.is_open()
    }

    fn present(&mut self, canvas: &Canvas) {
        let _ = self.window.update_with_buffer(&canvas.px, canvas.w, canvas.h);
    }
}
