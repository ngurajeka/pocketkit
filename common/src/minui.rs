//! MinUI's own settings, shared between its processes. MinUI's keymon owns the
//! volume (VOL+/-, SELECT+L/R) and brightness (START+L/R) and keeps them in POSIX
//! shared memory `/SharedSettings`, saved to `$USERDATA_PATH/msettings.bin`:
//!
//! ```c
//! typedef struct Settings { int version, brightness, headphones, speaker, jack, unused[3]; } Settings;
//! ```
//!
//! (workspace/miyoomini/libmsettings/msettings.c in MinUI). The Mini has no
//! /dev/shm, so its glibc keeps shared memory on the first tmpfs mount: the file
//! is `/tmp/SharedSettings` (checked on a Mini+). Volume is 0-20 (`headphones`
//! when something is plugged in, else `speaker`); brightness is 0-10.

use std::{
    fs::{self, OpenOptions},
    io,
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
};

pub const SHARED_PATHS: [&str; 2] = ["/dev/shm/SharedSettings", "/tmp/SharedSettings"];
pub const VOLUME_MAX: u32 = 20;
pub const BRIGHTNESS_MAX: u32 = 10;
const BRIGHTNESS: usize = 1;
const HEADPHONES: usize = 2;
const SPEAKER: usize = 3;
const JACK: usize = 4;
const PWM_DUTY: &str = "/sys/class/pwm/pwmchip0/pwm0/duty_cycle";

fn int(bytes: &[u8], i: usize) -> Option<i32> {
    bytes.get(i * 4..i * 4 + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The active volume (0-20) from the raw settings bytes.
pub fn parse_volume(bytes: &[u8]) -> Option<u32> {
    let (headphones, speaker, jack) = (int(bytes, HEADPHONES)?, int(bytes, SPEAKER)?, int(bytes, JACK)?);
    let v = if jack != 0 { headphones } else { speaker };
    (0..=VOLUME_MAX as i32).contains(&v).then_some(v as u32)
}

pub fn parse_brightness(bytes: &[u8]) -> Option<u32> {
    let v = int(bytes, BRIGHTNESS)?;
    (0..=BRIGHTNESS_MAX as i32).contains(&v).then_some(v as u32)
}

/// `None` when MinUI isn't running (desktop preview) or the data looks wrong.
pub fn read_volume(path: &Path) -> Option<u32> {
    parse_volume(&fs::read(path).ok()?)
}

pub fn read_brightness(path: &Path) -> Option<u32> {
    parse_brightness(&fs::read(path).ok()?)
}

/// The first of `SHARED_PATHS` that exists (the first one if none do yet).
pub fn shared_path() -> PathBuf {
    SHARED_PATHS.iter().map(Path::new).find(|p| p.exists()).unwrap_or(Path::new(SHARED_PATHS[0])).to_path_buf()
}

/// Sets brightness the way keymon does: the backlight PWM duty (0 means 6,
/// otherwise level x 10), the shared copy and the saved `msettings.bin`.
pub fn set_brightness(shared: &Path, saved: Option<&Path>, level: u32) -> io::Result<()> {
    let level = level.min(BRIGHTNESS_MAX);
    let duty = if level == 0 { 6 } else { level * 10 };
    let _ = fs::write(PWM_DUTY, duty.to_string()); // absent on a desktop
    let bytes = (level as i32).to_le_bytes();
    OpenOptions::new().write(true).open(shared)?.write_all_at(&bytes, (BRIGHTNESS * 4) as u64)?;
    if let Some(saved) = saved {
        if let Ok(f) = OpenOptions::new().write(true).open(saved) {
            f.write_all_at(&bytes, (BRIGHTNESS * 4) as u64)?;
            f.sync_all()?;
        }
    }
    Ok(())
}

/// Raw settings bytes, for tests (here and in the paks).
#[doc(hidden)]
pub fn settings_bytes(speaker: i32, headphones: i32, jack: i32) -> Vec<u8> {
    [2, 3, headphones, speaker, jack, 0, 0, 0].iter().flat_map(|v: &i32| v.to_le_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_speaker_or_headphones() {
        assert_eq!(parse_volume(&settings_bytes(12, 5, 0)), Some(12));
        assert_eq!(parse_volume(&settings_bytes(12, 5, 1)), Some(5));
        assert_eq!(parse_volume(&settings_bytes(99, 5, 0)), None);
        assert_eq!(parse_volume(&[0; 8]), None);
        assert_eq!(read_volume(Path::new("/nonexistent/SharedSettings")), None);
        assert_eq!(parse_brightness(&settings_bytes(1, 1, 0)), Some(3));
    }

    #[test]
    fn writes_brightness_into_both_copies() {
        let dir = std::env::temp_dir().join(format!("minui_bright_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (shared, saved) = (dir.join("SharedSettings"), dir.join("msettings.bin"));
        fs::write(&shared, settings_bytes(20, 20, 0)).unwrap();
        fs::write(&saved, settings_bytes(20, 20, 0)).unwrap();
        set_brightness(&shared, Some(&saved), 7).unwrap();
        assert_eq!(read_brightness(&shared), Some(7));
        assert_eq!(read_brightness(&saved), Some(7));
        assert_eq!(read_volume(&shared), Some(20)); // untouched
        set_brightness(&shared, None, 99).unwrap();
        assert_eq!(read_brightness(&shared), Some(BRIGHTNESS_MAX));
        fs::remove_dir_all(dir).unwrap();
    }
}
