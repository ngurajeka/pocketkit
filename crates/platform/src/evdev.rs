//! Buttons from Linux evdev devices: key codes through a per-device table,
//! D-pad hats (ABS_HAT0X/Y) as Up/Down/Left/Right, and software key repeat.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};

use pocketkit_core::input::Button;

const O_NONBLOCK: i32 = 0o4000;
/// `struct input_event`: 16 bytes on 32-bit ARM, 24 on 64-bit.
const EVENT_SIZE: usize = if cfg!(target_pointer_width = "32") { 16 } else { 24 };
const EV_KEY: u16 = 1;
const EV_ABS: u16 = 3;
const ABS_HAT0X: u16 = 16;
const ABS_HAT0Y: u16 = 17;
const REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_EVERY: Duration = Duration::from_millis(70);

/// Maps a key code to a button; `None` means "some other key" (still activity).
pub type KeyMap = fn(u16) -> Option<Button>;

/// The `/dev/input/eventN` whose name (from /proc/bus/input/devices) is `name`.
pub fn find_by_name(name: &str) -> Option<String> {
    let text = fs::read_to_string("/proc/bus/input/devices").ok()?;
    for block in text.split("\n\n") {
        if block.lines().any(|l| l.strip_prefix("N: Name=").is_some_and(|n| n.trim_matches('"') == name)) {
            let handlers = block.lines().find_map(|l| l.strip_prefix("H: Handlers="))?;
            let ev = handlers.split_whitespace().find(|h| h.starts_with("event"))?;
            return Some(format!("/dev/input/{ev}"));
        }
    }
    None
}

struct Source {
    file: File,
    map: KeyMap,
    /// Current hat directions, to turn axis changes into presses/releases.
    hat: (i32, i32),
}

pub struct Input {
    sources: Vec<Source>,
    held: Option<(Button, Instant)>,
}

/// Turns a hat axis change into (released, pressed) buttons.
pub fn hat_change(old: i32, new: i32, neg: Button, pos: Button) -> (Option<Button>, Option<Button>) {
    let dir = |v: i32| match v.signum() {
        -1 => Some(neg),
        1 => Some(pos),
        _ => None,
    };
    if old.signum() == new.signum() {
        return (None, None);
    }
    (dir(old), dir(new))
}

impl Input {
    pub fn new() -> Self {
        Self { sources: Vec::new(), held: None }
    }

    /// Adds a device; a missing one is reported and skipped.
    pub fn add(&mut self, path: &str, map: KeyMap) -> io::Result<()> {
        let file = OpenOptions::new().read(true).custom_flags(O_NONBLOCK).open(path)?;
        eprintln!("input: {path}");
        self.sources.push(Source { file, map, hat: (0, 0) });
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    fn press(&mut self, pressed: &mut Vec<Button>, b: Button, now: Instant) {
        pressed.push(b);
        self.held = b.repeats().then_some((b, now + REPEAT_DELAY));
    }

    fn release(&mut self, b: Button) {
        if self.held.is_some_and(|(h, _)| h == b) {
            self.held = None;
        }
    }

    pub fn poll(&mut self, pressed: &mut Vec<Button>) {
        let now = Instant::now();
        let mut buf = [0u8; EVENT_SIZE * 32];
        for i in 0..self.sources.len() {
            loop {
                let n = match self.sources[i].file.read(&mut buf) {
                    Ok(n) if n > 0 => n,
                    _ => break, // nothing to read (WouldBlock) or the device went away
                };
                for ev in buf[..n].as_chunks::<EVENT_SIZE>().0 {
                    let o = EVENT_SIZE - 8; // type, code, value are the last 8 bytes
                    let kind = u16::from_le_bytes([ev[o], ev[o + 1]]);
                    let code = u16::from_le_bytes([ev[o + 2], ev[o + 3]]);
                    let value = i32::from_le_bytes([ev[o + 4], ev[o + 5], ev[o + 6], ev[o + 7]]);
                    match kind {
                        EV_KEY => {
                            let b = (self.sources[i].map)(code).unwrap_or(Button::Other);
                            match value {
                                1 => self.press(pressed, b, now),
                                0 => self.release(b),
                                _ => {} // kernel auto-repeat: we repeat ourselves
                            }
                        }
                        EV_ABS if code == ABS_HAT0X || code == ABS_HAT0Y => {
                            let src = &mut self.sources[i];
                            let (old, axis) = if code == ABS_HAT0X { (src.hat.0, 0) } else { (src.hat.1, 1) };
                            if axis == 0 {
                                src.hat.0 = value;
                            } else {
                                src.hat.1 = value;
                            }
                            let (neg, pos) = if axis == 0 { (Button::Left, Button::Right) } else { (Button::Up, Button::Down) };
                            let (released, now_pressed) = hat_change(old, value, neg, pos);
                            if let Some(b) = released {
                                self.release(b);
                            }
                            if let Some(b) = now_pressed {
                                self.press(pressed, b, now);
                            }
                        }
                        _ => {}
                    }
                }
                if n < buf.len() {
                    break;
                }
            }
        }
        if let Some((button, at)) = self.held {
            if now >= at {
                pressed.push(button);
                self.held = Some((button, now + REPEAT_EVERY));
            }
        }
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hat_moves_become_presses_and_releases() {
        assert_eq!(hat_change(0, -1, Button::Up, Button::Down), (None, Some(Button::Up)));
        assert_eq!(hat_change(-1, 0, Button::Up, Button::Down), (Some(Button::Up), None));
        assert_eq!(hat_change(-1, 1, Button::Up, Button::Down), (Some(Button::Up), Some(Button::Down)));
        assert_eq!(hat_change(1, 1, Button::Up, Button::Down), (None, None));
    }
}
