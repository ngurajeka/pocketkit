//! The system volume, for showing it in apps (Music's slider). Changing it is
//! left to each OS's own buttons/menus.

use std::{path::PathBuf, process::Command};

use crate::minui_settings;

#[derive(Debug, Clone)]
pub enum VolumeSource {
    /// MinUI's shared settings file (0-20), written by its keymon.
    MinUI(PathBuf),
    /// PipeWire's default sink via `wpctl` (muOS).
    PipeWire,
    None,
}

impl VolumeSource {
    /// The right source for this build's platform.
    pub fn for_platform() -> Self {
        if cfg!(feature = "muos") {
            VolumeSource::PipeWire
        } else if cfg!(feature = "minui") {
            VolumeSource::MinUI(minui_settings::shared_path())
        } else {
            VolumeSource::None
        }
    }

    /// (level, max), e.g. (12, 20) or (75, 100).
    pub fn read(&self) -> Option<(u32, u32)> {
        match self {
            VolumeSource::MinUI(path) => minui_settings::read_volume(path).map(|v| (v, minui_settings::VOLUME_MAX)),
            VolumeSource::PipeWire => {
                let out = Command::new("wpctl").args(["get-volume", "@DEFAULT_AUDIO_SINK@"]).output().ok()?;
                parse_wpctl(&String::from_utf8_lossy(&out.stdout))
            }
            VolumeSource::None => None,
        }
    }

    /// How often it's worth re-reading (wpctl starts a process each time).
    pub fn poll_secs(&self) -> f32 {
        match self {
            VolumeSource::PipeWire => 1.0,
            _ => 0.2,
        }
    }
}

/// `Volume: 0.75` (or `Volume: 0.75 [MUTED]`) -> (75, 100); muted counts as 0.
pub fn parse_wpctl(text: &str) -> Option<(u32, u32)> {
    let v: f32 = text.split_whitespace().nth(1)?.parse().ok()?;
    let level = if text.contains("MUTED") { 0 } else { (v * 100.0).round().clamp(0.0, 100.0) as u32 };
    Some((level, 100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl() {
        assert_eq!(parse_wpctl("Volume: 0.75\n"), Some((75, 100)));
        assert_eq!(parse_wpctl("Volume: 0.40 [MUTED]\n"), Some((0, 100)));
        assert_eq!(parse_wpctl(""), None);
    }
}
