//! Linux framebuffer output: converts the 640x480 `0x00RRGGBB` canvas to the
//! framebuffer's pixel format and writes it to the screen.
//!
//! With `page_flip`, frames are drawn into the hidden page of a double-buffered
//! framebuffer and then shown by panning to it (`FBIOPAN_DISPLAY` with
//! `FB_ACTIVATE_VBL`), so the panel never scans out a half-drawn frame (tearing:
//! old and new highlight/colours visible at once). The page we panned away from
//! keeps being scanned out until the new address latches, which on the H700
//! (checked with the `fbprobe` example) is the refresh *after* the pan ioctl
//! returns; its `FBIO_WAITFORVSYNC` returns at once, so instead we don't draw
//! into that page until one refresh period (from the mode, e.g. `640x480p-59`)
//! plus a margin has passed. If the pan ioctl isn't available it pans through
//! `/sys/class/graphics/fb0/pan`; if panning is refused altogether, frames go
//! straight to the visible page.

use std::{
    fs::{self, File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::FileExt},
    thread,
    time::{Duration, Instant},
};

use pocketkit_core::render::{Canvas, H, W};

pub struct Framebuffer {
    file: File,
    bytes_per_pixel: usize,
    stride: usize,
    /// Page (in screen heights) that was showing when we started.
    start_page: u64,
    /// Pages available for flipping (1 = no flipping).
    pages: u64,
    /// Page drawn into next.
    next: u64,
    pan: Option<File>,
    /// `struct fb_var_screeninfo` (40 x u32), when the pan ioctl is usable.
    var: Option<[u32; 40]>,
    /// One display refresh.
    refresh: Duration,
    /// When the page we last panned away from is certainly off screen.
    free_at: Option<Instant>,
    rotate: bool,
    frame: Vec<u8>,
}

// <linux/fb.h>
const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOPAN_DISPLAY: u32 = 0x4606;
const FB_ACTIVATE_VBL: u32 = 16;
/// Indexes into `fb_var_screeninfo`.
const VAR_YOFFSET: usize = 5;
const VAR_ACTIVATE: usize = 21;

fn sysfs(name: &str) -> Option<String> {
    fs::read_to_string(format!("/sys/class/graphics/fb0/{name}")).ok()
}

impl Framebuffer {
    /// `rotate`: draw upside down (the Miyoo Mini's panel is mounted that way).
    /// `page_flip`: double-buffer through panning when the framebuffer has room.
    pub fn open(rotate: bool, page_flip: bool) -> io::Result<Self> {
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
        let virtual_h: u64 = sysfs("virtual_size")
            .and_then(|v| v.trim().split(',').nth(1).and_then(|h| h.trim().parse().ok()))
            .unwrap_or(H as u64);
        let start_page = pan_y / H as u64;
        let pan = if page_flip && virtual_h >= 2 * H as u64 {
            OpenOptions::new().write(true).open("/sys/class/graphics/fb0/pan").ok()
        } else {
            None
        };
        let pages = if pan.is_some() { 2 } else { 1 };
        let refresh = Duration::from_secs_f64(1.0 / refresh_hz(&sysfs("modes").unwrap_or_default()));
        let mut var = [0u32; 40];
        // SAFETY: FBIOGET_VSCREENINFO fills a `struct fb_var_screeninfo`, which is
        // exactly 40 u32s; `var` outlives the call.
        let var = (pages > 1 && unsafe { libc::ioctl(file.as_raw_fd(), FBIOGET_VSCREENINFO as _, var.as_mut_ptr()) } == 0)
            .then_some(var);
        eprintln!(
            "framebuffer: {bpp} bpp, stride {stride}, virtual height {virtual_h}, page {start_page}, flip {}, pan ioctl {}, rotate {rotate}",
            pages > 1,
            var.is_some()
        );
        Ok(Self {
            file,
            bytes_per_pixel,
            stride,
            start_page,
            pages,
            next: (start_page + 1) % 2,
            pan,
            var,
            refresh,
            free_at: None,
            rotate,
            frame: vec![0; stride * H],
        })
    }

    fn page_offset(&self, page: u64) -> u64 {
        page * (H * self.stride) as u64
    }

    /// Pans to `page` (at the next vertical blank where supported); returns
    /// false if the kernel refused. Records when the previous page is free.
    fn show(&mut self, page: u64) -> bool {
        let fd = self.file.as_raw_fd();
        let mut ok = false;
        if let Some(var) = &mut self.var {
            var[VAR_YOFFSET] = (page * H as u64) as u32;
            var[VAR_ACTIVATE] = FB_ACTIVATE_VBL;
            // SAFETY: FBIOPAN_DISPLAY reads a `struct fb_var_screeninfo` (40 u32s).
            ok = unsafe { libc::ioctl(fd, FBIOPAN_DISPLAY as _, var.as_ptr()) } == 0;
            if !ok {
                eprintln!("framebuffer: pan ioctl refused, using sysfs");
                self.var = None;
            }
        }
        if !ok {
            let Some(pan) = &self.pan else { return false };
            ok = pan.write_all_at(format!("0,{}", page * H as u64).as_bytes(), 0).is_ok();
        }
        if ok {
            // The new address latches at a refresh; until then the old page
            // is still on screen.
            self.free_at = Some(Instant::now() + self.refresh + Duration::from_millis(2));
        }
        ok
    }

    /// Waits until the page we panned away from is no longer on screen.
    fn wait_until_free(&mut self) {
        if let Some(at) = self.free_at.take() {
            if let Some(left) = at.checked_duration_since(Instant::now()) {
                thread::sleep(left);
            }
        }
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
        if self.pages > 1 {
            let page = self.next;
            self.wait_until_free();
            let _ = self.file.write_all_at(&self.frame, self.page_offset(page));
            if self.show(page) {
                self.next = (page + 1) % self.pages;
                return;
            }
            // Panning isn't supported after all: draw on the visible page from now on.
            eprintln!("framebuffer: panning refused, drawing to the visible page");
            self.pages = 1;
            self.pan = None;
        }
        let _ = self.file.write_all_at(&self.frame, self.page_offset(self.start_page));
    }

    /// The standard fbdev blanking switch (used where there's no backlight API).
    pub fn blank(on: bool) {
        let _ = fs::write("/sys/class/graphics/fb0/blank", if on { "0" } else { "1" });
    }
}

/// The refresh rate from sysfs `modes` (e.g. `U:640x480p-59`), 60 if unknown.
pub fn refresh_hz(modes: &str) -> f64 {
    modes
        .lines()
        .next()
        .and_then(|m| m.rsplit('-').next())
        .and_then(|hz| hz.trim().parse::<f64>().ok())
        .filter(|hz| (20.0..=240.0).contains(hz))
        .unwrap_or(60.0)
}

impl Drop for Framebuffer {
    /// Leave the display on the page it started on, as the launcher expects.
    fn drop(&mut self) {
        if self.pages > 1 {
            let start = self.start_page;
            let _ = self.file.write_all_at(&self.frame, self.page_offset(start));
            self.show(start);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_refresh_rate() {
        assert_eq!(refresh_hz("U:640x480p-59\n"), 59.0);
        assert_eq!(refresh_hz("U:640x480p-100\nU:640x480p-273\n"), 100.0);
        assert_eq!(refresh_hz(""), 60.0);
        assert_eq!(refresh_hz("garbage"), 60.0);
    }
}
