//! Backend for the Miyoo Mini+: writes straight to the Linux framebuffer and
//! reads buttons from evdev, using only the standard library. Works on a Mini+.
//!
//! - framebuffer stride and pixel format are read from sysfs when possible
//! - key codes are the stock Miyoo Mini button layout
//! - the panel is mounted upside down, so `present` rotates every frame 180 degrees
//!
//! If the screen ever stays black or looks garbled, check
//! `/sys/class/graphics/fb0/{virtual_size,bits_per_pixel,stride}` on the device.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::{FileExt, OpenOptionsExt},
};

use super::Platform;
use crate::{
    input::Button,
    render::{Canvas, H, W},
};

const FB_PATH: &str = "/dev/fb0";
const INPUT_PATH: &str = "/dev/input/event0";
/// Visible panel size. The game renders at W x H and is scaled up by SCALE.
const PANEL_W: usize = 640;
const PANEL_H: usize = 480;
const SCALE: usize = 2;
/// `O_NONBLOCK` on Linux.
const O_NONBLOCK: i32 = 0o4000;
/// `struct input_event` is 16 bytes on 32-bit ARM (24 on 64-bit hosts).
const EVENT_SIZE: usize = if cfg!(target_pointer_width = "32") { 16 } else { 24 };
const EV_KEY: u16 = 1;

/// Linux key codes sent by the stock Miyoo Mini button board.
fn map_key(code: u16) -> Option<Button> {
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
        28 => Button::Start,  // KEY_ENTER
        97 => Button::Select, // KEY_RIGHTCTRL
        1 => Button::Menu,    // KEY_ESC
        _ => return None,
    })
}

pub struct Device {
    fb: File,
    input: File,
    bytes_per_pixel: usize,
    stride: usize,
    frame: Vec<u8>,
}

fn sysfs(name: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/graphics/fb0/{name}")).ok()
}

impl Device {
    pub fn open() -> io::Result<Self> {
        let fb = OpenOptions::new().write(true).open(FB_PATH)?;
        let input = OpenOptions::new().read(true).custom_flags(O_NONBLOCK).open(INPUT_PATH)?;

        let bpp: usize = sysfs("bits_per_pixel").and_then(|s| s.trim().parse().ok()).unwrap_or(32);
        let bytes_per_pixel = bpp / 8;
        let stride: usize = sysfs("stride")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(PANEL_W * bytes_per_pixel);

        if !(2..=4).contains(&bytes_per_pixel) || W * SCALE * bytes_per_pixel > stride {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("unsupported framebuffer: {bpp} bpp, stride {stride}"),
            ));
        }
        eprintln!("framebuffer: {bpp} bpp, stride {stride}");

        Ok(Self { fb, input, bytes_per_pixel, stride, frame: vec![0; stride * PANEL_H] })
    }

    fn encode(&self, px: u32) -> [u8; 4] {
        let (r, g, b) = ((px >> 16) & 0xFF, (px >> 8) & 0xFF, px & 0xFF);
        match self.bytes_per_pixel {
            4 => (px | 0xFF00_0000).to_le_bytes(), // B, G, R, A in memory
            3 => [b as u8, g as u8, r as u8, 0],
            _ => {
                let v = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3); // RGB565
                [(v & 0xFF) as u8, (v >> 8) as u8, 0, 0]
            }
        }
    }
}

impl Platform for Device {
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool {
        let mut buf = [0u8; EVENT_SIZE * 32];
        loop {
            match self.input.read(&mut buf) {
                Ok(n) if n > 0 => {
                    for ev in buf[..n].chunks_exact(EVENT_SIZE) {
                        let o = EVENT_SIZE - 8; // type, code, value are the last 8 bytes
                        let kind = u16::from_le_bytes([ev[o], ev[o + 1]]);
                        let code = u16::from_le_bytes([ev[o + 2], ev[o + 3]]);
                        let value = i32::from_le_bytes([ev[o + 4], ev[o + 5], ev[o + 6], ev[o + 7]]);
                        if kind == EV_KEY && value == 1 {
                            if let Some(button) = map_key(code) {
                                pressed.push(button);
                            }
                        }
                    }
                    if n < buf.len() {
                        break;
                    }
                }
                _ => break, // nothing to read (WouldBlock) or the device went away
            }
        }
        true
    }

    fn present(&mut self, canvas: &Canvas) {
        let bpp = self.bytes_per_pixel;
        for y in 0..canvas.h {
            let row = y * SCALE * self.stride;
            // Read the canvas back to front: the panel is upside down.
            let src_row = (canvas.h - 1 - y) * canvas.w;
            for x in 0..canvas.w {
                let bytes = self.encode(canvas.px[src_row + canvas.w - 1 - x]);
                for sx in 0..SCALE {
                    let o = row + (x * SCALE + sx) * bpp;
                    self.frame[o..o + bpp].copy_from_slice(&bytes[..bpp]);
                }
            }
            for sy in 1..SCALE {
                self.frame.copy_within(row..row + self.stride, row + sy * self.stride);
            }
        }
        let _ = self.fb.write_all_at(&self.frame, 0);
    }
}

const _: () = assert!(W * SCALE <= PANEL_W && H * SCALE <= PANEL_H);
