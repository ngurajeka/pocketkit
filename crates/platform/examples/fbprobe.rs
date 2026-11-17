//! Framebuffer diagnostics for a device (run it over SSH/adb, not inside an
//! app): what the driver reports, whether panning works, and whether
//! FBIO_WAITFORVSYNC really waits for the vertical blank (it should block for
//! about one refresh, ~16.7 ms at 60 Hz). It only re-selects the page that's
//! already showing, so the screen doesn't change.
//!
//!   cargo build --release -p pocketkit_platform --example fbprobe --target aarch64-unknown-linux-musl

#[cfg(unix)]
fn main() {
    use std::{fs, os::fd::AsRawFd, time::Instant};

    const FBIOGET_VSCREENINFO: u32 = 0x4600;
    const FBIOPAN_DISPLAY: u32 = 0x4606;
    const FBIO_WAITFORVSYNC: u32 = 0x4004_4620;
    const FB_ACTIVATE_VBL: u32 = 16;

    let sysfs = |n: &str| fs::read_to_string(format!("/sys/class/graphics/fb0/{n}")).unwrap_or_default().trim().to_string();
    for n in ["name", "virtual_size", "pan", "bits_per_pixel", "stride", "modes"] {
        println!("sysfs {n}: {}", sysfs(n));
    }

    let file = fs::OpenOptions::new().read(true).write(true).open("/dev/fb0").expect("open /dev/fb0");
    let fd = file.as_raw_fd();
    let mut var = [0u32; 40];
    // SAFETY: fb_var_screeninfo is 40 u32s.
    let r = unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, var.as_mut_ptr()) };
    println!("FBIOGET_VSCREENINFO -> {r}: xres {} yres {} xres_virtual {} yres_virtual {} xoffset {} yoffset {} bpp {} activate {}",
        var[0], var[1], var[2], var[3], var[4], var[5], var[6], var[21]);

    let zero: u32 = 0;
    let mut times = Vec::new();
    for _ in 0..10 {
        let t = Instant::now();
        // SAFETY: one u32 argument.
        let r = unsafe { libc::ioctl(fd, FBIO_WAITFORVSYNC as _, &zero) };
        times.push((r, t.elapsed().as_secs_f64() * 1000.0));
    }
    println!("FBIO_WAITFORVSYNC x10 (result, ms): {times:.2?}");
    if times.iter().all(|(r, _)| *r != 0) {
        println!("  -> not supported: {}", std::io::Error::last_os_error());
    } else if times.iter().all(|(_, ms)| *ms < 2.0) {
        println!("  -> returns immediately: it doesn't really wait for vblank");
    }

    // Pan to the page that's already showing (no visible change), with and
    // without FB_ACTIVATE_VBL, and time it.
    for activate in [0, FB_ACTIVATE_VBL] {
        var[21] = activate;
        let t = Instant::now();
        // SAFETY: fb_var_screeninfo is 40 u32s.
        let r = unsafe { libc::ioctl(fd, FBIOPAN_DISPLAY as _, var.as_ptr()) };
        let err = if r != 0 { format!(" ({})", std::io::Error::last_os_error()) } else { String::new() };
        println!("FBIOPAN_DISPLAY activate={activate} -> {r}{err} in {:.2} ms; pan now {}", t.elapsed().as_secs_f64() * 1000.0, sysfs("pan"));
    }

    // Who else has the framebuffer open (something else panning would make
    // stale frames flash)?
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        let pid = entry.file_name().to_string_lossy().to_string();
        if !pid.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let fds = fs::read_dir(format!("/proc/{pid}/fd")).into_iter().flatten().flatten();
        if fds.filter_map(|f| fs::read_link(f.path()).ok()).any(|l| l.to_string_lossy() == "/dev/fb0") {
            let comm = fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default();
            println!("/dev/fb0 open in pid {pid}: {}", comm.trim());
        }
    }
}

#[cfg(not(unix))]
fn main() {}
