//! Miyoo Mini / Mini+ under MinUI: framebuffer drawn upside down (the panel is
//! mounted that way), buttons as keyboard codes on event0, backlight through
//! MinUI's GPIO/PWM sequence. Confirmed on a Mini+.

use std::{fs, io};

use pocketkit_core::{input::Button, platform::Platform, render::Canvas};

use crate::{evdev::Input, fb::Framebuffer};

/// Linux key codes sent by the stock Miyoo Mini button board (MinUI's layout).
pub fn map_key(code: u16) -> Option<Button> {
    Some(match code {
        103 => Button::Up,
        108 => Button::Down,
        105 => Button::Left,
        106 => Button::Right,
        57 => Button::A,      // KEY_SPACE
        29 => Button::B,      // KEY_LEFTCTRL
        42 => Button::X,      // KEY_LEFTSHIFT
        56 => Button::Y,      // KEY_LEFTALT
        18 => Button::L1,     // KEY_E
        20 => Button::R1,     // KEY_T
        15 => Button::L2,     // KEY_TAB
        14 => Button::R2,     // KEY_BACKSPACE
        28 => Button::Start,  // KEY_ENTER
        97 => Button::Select, // KEY_RIGHTCTRL
        1 => Button::Menu,    // KEY_ESC
        116 => Button::Power, // KEY_POWER
        _ => return None,
    })
}

pub struct Miyoo {
    fb: Framebuffer,
    input: Input,
}

impl Miyoo {
    pub fn open() -> io::Result<Self> {
        // `FB_ROTATE=0` turns the 180-degree flip off.
        let rotate = std::env::var("FB_ROTATE").map_or(true, |v| v.trim() != "0");
        let fb = Framebuffer::open(rotate)?;
        let mut input = Input::new();
        input.add("/dev/input/event0", map_key)?;
        Ok(Self { fb, input })
    }
}

impl Platform for Miyoo {
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool {
        self.input.poll(pressed);
        true
    }

    fn present(&mut self, canvas: &Canvas) {
        self.fb.present(canvas);
    }

    /// Same sysfs sequence as MinUI's `PLAT_enableBacklight` (miyoomini/platform.c).
    fn set_backlight(&mut self, on: bool) {
        let put = |path: &str, value: &str| {
            let _ = fs::write(path, value);
        };
        if on {
            put("/sys/class/gpio/gpio4/value", "1");
            put("/sys/class/gpio/unexport", "4");
            put("/sys/class/pwm/pwmchip0/export", "0");
            put("/sys/class/pwm/pwmchip0/pwm0/enable", "0");
            put("/sys/class/pwm/pwmchip0/pwm0/enable", "1");
        } else {
            put("/sys/class/gpio/export", "4");
            put("/sys/class/gpio/gpio4/direction", "out");
            put("/sys/class/gpio/gpio4/value", "0");
        }
    }
}
