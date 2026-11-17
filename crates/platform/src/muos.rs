//! Anbernic RG35XX-SP (Allwinner H700) under muOS. Checked on the device:
//! - `/dev/fb0`: 640x480, 32 bpp, double-buffered (640x960), not rotated
//! - `muOS-Keys` (event1): face/shoulder/system buttons as gamepad codes, the
//!   D-pad as ABS_HAT0X/Y. muOS grabs it while its menu is showing and lets go
//!   when an app is launched through mux_launch.sh.
//! - `axp2202-pek` (event0): the power button
//! - no /sys/class/backlight: the screen is switched off with fbdev blanking

use std::io;

use pocketkit_core::{input::Button, platform::Platform, render::Canvas};

use crate::{
    evdev::{find_by_name, Input},
    fb::Framebuffer,
};

/// `muOS-Keys` codes, recorded by pressing each button on an RG35XX-SP; they
/// match muOS's own SDL mapping for the device.
pub fn map_gamepad(code: u16) -> Option<Button> {
    Some(match code {
        304 => Button::A,
        305 => Button::B,
        307 => Button::X,
        306 => Button::Y,
        308 => Button::L1,
        309 => Button::R1,
        314 => Button::L2,
        315 => Button::R2,
        310 => Button::Select,
        311 => Button::Start,
        312 => Button::Menu,
        _ => return None, // VOL+/- (115/114) and others: activity only
    })
}

pub fn map_power(code: u16) -> Option<Button> {
    (code == 116).then_some(Button::Power)
}

pub struct Muos {
    fb: Framebuffer,
    input: Input,
}

impl Muos {
    pub fn open() -> io::Result<Self> {
        let fb = Framebuffer::open(false)?;
        let mut input = Input::new();
        let pad = find_by_name("muOS-Keys").unwrap_or_else(|| "/dev/input/event1".into());
        input.add(&pad, map_gamepad)?;
        let power = find_by_name("axp2202-pek").unwrap_or_else(|| "/dev/input/event0".into());
        if let Err(e) = input.add(&power, map_power) {
            eprintln!("input: no power button ({e})");
        }
        Ok(Self { fb, input })
    }
}

impl Platform for Muos {
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool {
        self.input.poll(pressed);
        true
    }

    fn present(&mut self, canvas: &Canvas) {
        self.fb.present(canvas);
    }

    fn set_backlight(&mut self, on: bool) {
        Framebuffer::blank(on);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_recorded_codes() {
        assert_eq!(map_gamepad(304), Some(Button::A));
        assert_eq!(map_gamepad(306), Some(Button::Y));
        assert_eq!(map_gamepad(312), Some(Button::Menu));
        assert_eq!(map_gamepad(115), None);
        assert_eq!(map_power(116), Some(Button::Power));
    }
}
