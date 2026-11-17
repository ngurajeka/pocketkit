//! pocketkit platforms: the device-specific half of every app. Pick one with a
//! Cargo feature:
//! - `desktop`: a preview window on your computer (minifb)
//! - `minui`: Miyoo Mini / Mini+ under MinUI
//! - `muos`: Anbernic RG35XX-SP (H700) under muOS
//!
//! Also MinUI's shared settings (`minui_settings`, used by Settings.pak and
//! Music) and the system volume (`volume`).

#[cfg(feature = "desktop")]
pub mod desktop;
#[cfg(unix)]
pub mod evdev;
#[cfg(unix)]
pub mod fb;
#[cfg(unix)]
pub mod minui;
pub mod minui_settings;
#[cfg(unix)]
pub mod muos;
pub mod volume;

use pocketkit_core::{platform::Platform, App};

/// Which platform this build is for (for logs and small differences).
pub const NAME: &str = if cfg!(feature = "desktop") {
    "desktop"
} else if cfg!(feature = "muos") {
    "muos"
} else if cfg!(feature = "minui") {
    "minui"
} else {
    "none"
};

/// Opens the screen and buttons for this build's platform, or exits with a
/// message in the log if they can't be opened.
pub fn open(title: &str) -> Box<dyn Platform> {
    #[cfg(feature = "desktop")]
    {
        Box::new(desktop::Desktop::new(title))
    }
    #[cfg(all(not(feature = "desktop"), feature = "muos"))]
    {
        let _ = title;
        match muos::Muos::open() {
            Ok(p) => Box::new(p),
            Err(e) => fail(e),
        }
    }
    #[cfg(all(not(feature = "desktop"), not(feature = "muos"), feature = "minui"))]
    {
        let _ = title;
        match minui::Miyoo::open() {
            Ok(p) => Box::new(p),
            Err(e) => fail(e),
        }
    }
    #[cfg(not(any(feature = "desktop", feature = "muos", feature = "minui")))]
    {
        let _ = title;
        compile_error!("pick a platform: --features desktop, minui or muos");
    }
}

#[allow(dead_code)]
fn fail(e: std::io::Error) -> ! {
    eprintln!("could not open the screen/buttons: {e}");
    std::process::exit(1);
}

/// Runs `app` on this build's platform until it quits.
pub fn run(title: &str, app: &mut impl App) {
    pocketkit_core::run_with(open(title), app);
}
