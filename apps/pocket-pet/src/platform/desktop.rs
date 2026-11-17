//! Desktop preview window so you can iterate without touching the device.
//! Keys: arrows = D-pad, Z/Space = A, X = B, Enter = Start, Esc = Menu (quit).

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
    pub fn new() -> Self {
        let window = Window::new(
            "Pocket Pet (desktop preview)",
            W,
            H,
            WindowOptions { scale: Scale::X2, ..WindowOptions::default() },
        )
        .expect("failed to open window");
        Self { window }
    }
}

impl Default for Desktop {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for Desktop {
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool {
        for key in self.window.get_keys_pressed(KeyRepeat::No) {
            let button = match key {
                Key::Up => Button::Up,
                Key::Down => Button::Down,
                Key::Left => Button::Left,
                Key::Right => Button::Right,
                Key::Z | Key::Space => Button::A,
                Key::X => Button::B,
                Key::A => Button::L1,
                Key::S => Button::R1,
                Key::Enter => Button::Start,
                Key::Escape => Button::Menu,
                _ => continue,
            };
            pressed.push(button);
        }
        self.window.is_open()
    }

    fn present(&mut self, canvas: &Canvas) {
        let _ = self.window.update_with_buffer(&canvas.px, canvas.w, canvas.h);
    }
}
