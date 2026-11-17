//! Settings shared by our paks, kept in one `key=value` text file written by
//! Settings.pak: `$USERDATA_PATH/minui-paks.txt` on the device (override with
//! `PAKS_SETTINGS`), `./minui-paks.txt` on a desktop.
//!
//! Keys: `screen_off` (seconds idle before the screen turns off, 0 = never),
//! `music_dir`, `search_url`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const SCREEN_OFF: &str = "screen_off";
pub const MUSIC_DIR: &str = "music_dir";
pub const SEARCH_URL: &str = "search_url";

/// Default for `screen_off`: a minute, like a phone.
pub const DEFAULT_SCREEN_OFF_SECS: u32 = 60;

pub fn default_path() -> PathBuf {
    if let Some(p) = env::var_os("PAKS_SETTINGS") {
        return PathBuf::from(p);
    }
    match env::var_os("USERDATA_PATH") {
        Some(dir) => Path::new(&dir).join("minui-paks.txt"),
        None => PathBuf::from("minui-paks.txt"),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    entries: Vec<(String, String)>,
}

impl Settings {
    pub fn parse(text: &str) -> Self {
        let entries = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        Self { entries }
    }

    pub fn load(path: &Path) -> Self {
        Self::parse(&fs::read_to_string(path).unwrap_or_default())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
    }

    pub fn set(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value.to_string(),
            None => self.entries.push((key.to_string(), value.to_string())),
        }
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# Written by Settings.pak\n");
        for (k, v) in &self.entries {
            out.push_str(&format!("{k}={v}\n"));
        }
        out
    }

    /// Writes atomically (temp file + rename), so power loss can't corrupt it.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, self.to_text())?;
        fs::rename(&tmp, path)
    }

    pub fn screen_off_secs(&self) -> u32 {
        self.get(SCREEN_OFF).and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_SCREEN_OFF_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sets_and_round_trips() {
        let mut s = Settings::parse("# c\nscreen_off = 30\n\nmusic_dir=/mnt/SDCARD/Music\nbad line\nempty=\n");
        assert_eq!(s.screen_off_secs(), 30);
        assert_eq!(s.get(MUSIC_DIR), Some("/mnt/SDCARD/Music"));
        assert_eq!(s.get("empty"), None);
        s.set(SCREEN_OFF, "0");
        s.set(SEARCH_URL, "https://x/?q=");
        let again = Settings::parse(&s.to_text());
        assert_eq!(again.screen_off_secs(), 0);
        assert_eq!(again.get(SEARCH_URL), Some("https://x/?q="));
        assert_eq!(Settings::default().screen_off_secs(), DEFAULT_SCREEN_OFF_SECS);
    }

    #[test]
    fn saves_atomically() {
        let dir = std::env::temp_dir().join(format!("paks_settings_{}", std::process::id()));
        let path = dir.join("sub/minui-paks.txt");
        let mut s = Settings::default();
        s.set(SCREEN_OFF, "120");
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path).screen_off_secs(), 120);
        assert!(!path.with_extension("tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
