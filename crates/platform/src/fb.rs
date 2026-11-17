//! Linux framebuffer output: converts the 640x480 `0x00RRGGBB` canvas to the
//! framebuffer's pixel format and writes it to the page currently on screen.

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::FileExt,
};

use pocketkit_core::render::{Canvas, H, W};

pub struct Framebuffer {
    file: File,
    bytes_per_pixel: usize,
    stride: usize,
    /// Byte offset of the visible page (double-buffered framebuffers pan).
    offset: u64,
    rotate: bool,
    frame: Vec<u8>,
}

fn sysfs(name: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/graphics/fb0/{name}")).ok()
}

impl Framebuffer {
    /// `rotate`: draw upside down (the Miyoo Mini's panel is mounted that way).
    pub fn open(rotate: bool) -> io::Result<Self> {
        let file = OpenOptions::new().write(true).open("/dev/fb0")?;
        let bpp: usize = sysfs("bits_per_pixel").and_then(|s| s.trim().parse().ok()).unwrap_or(32);
        let bytes_per_pixel = bpp / 8;
        let stride: usize = sysfs("stride").and_then(|s| s.trim().parse().ok()).unwrap_or(W * bytes_per_pixel);
        if !(2..=4).contains(&bytes_per_pixel) || W * bytes_per_pixel > stride {
            return Err(io::Error::other(format!("unsupported framebuffer: {bpp} bpp, stride {stride}")));
        }
        // "x,y" of the visible page; write there so we're on screen even if the
        // launcher left the display panned to its second buffer.
        let pan_y: u64 = sysfs("pan")
            .and_then(|p| p.trim().split(',').nth(1).and_then(|y| y.trim().parse().ok()))
            .unwrap_or(0);
        let offset = pan_y * stride as u64;
        eprintln!("framebuffer: {bpp} bpp, stride {stride}, page at y={pan_y}, rotate {rotate}");
        Ok(Self { file, bytes_per_pixel, stride, offset, rotate, frame: vec![0; stride * H] })
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

    pub fn present(&mut self, canvas: &Canvas) {
        let bpp = self.bytes_per_pixel;
        for y in 0..canvas.h {
            let src_y = if self.rotate { canvas.h - 1 - y } else { y };
            let row = y * self.stride;
            for x in 0..canvas.w {
                let src_x = if self.rotate { canvas.w - 1 - x } else { x };
                let bytes = self.encode(canvas.px[src_y * canvas.w + src_x]);
                self.frame[row + x * bpp..row + (x + 1) * bpp].copy_from_slice(&bytes[..bpp]);
            }
        }
        let _ = self.file.write_all_at(&self.frame, self.offset);
    }

    /// The standard fbdev blanking switch (used where there's no backlight API).
    pub fn blank(on: bool) {
        let _ = fs::write("/sys/class/graphics/fb0/blank", if on { "0" } else { "1" });
    }
}
