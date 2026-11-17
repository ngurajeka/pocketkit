use crate::{input::Button, render::Canvas};

/// Everything the game needs from the outside world: buttons in, pixels out.
pub trait Platform {
    /// Append newly pressed buttons to `pressed`. Returns `false` if the user closed the window.
    fn poll(&mut self, pressed: &mut Vec<Button>) -> bool;
    /// Show the finished frame.
    fn present(&mut self, canvas: &Canvas);
    /// Turn the backlight off (screen off, app keeps running) or back on.
    fn set_backlight(&mut self, _on: bool) {}
}

#[cfg(feature = "desktop")]
pub mod desktop;

#[cfg(unix)]
pub mod device;
