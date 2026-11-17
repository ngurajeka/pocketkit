//! Finding music files and the play order (next/previous, shuffle, repeat).

use std::{
    fs,
    path::{Path, PathBuf},
};

pub const EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "m4a", "aac", "mp4"];

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    /// "Artist - Title" once the file has been opened, else the file name.
    pub name: String,
    /// Seconds, once known.
    pub duration: Option<f64>,
}

impl Track {
    pub fn new(path: PathBuf) -> Self {
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Self { path, name, duration: None }
    }
}

fn is_music(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Every music file under `dir` (recursively), sorted by path, skipping hidden
/// files and folders (macOS `._*` junk included).
pub fn scan(dir: &Path) -> Vec<Track> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(_) if is_music(&path) => files.push(path),
                _ => {}
            }
        }
    }
    files.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    files.into_iter().map(Track::new).collect()
}

/// `m:ss` (or `h:mm:ss`).
pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn next(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

/// Small xorshift generator for shuffle (no dependency needed).
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// The track to play after `current` finishes, or `None` to stop.
pub fn after(current: usize, len: usize, shuffle: bool, repeat: Repeat, rng: &mut Rng) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if repeat == Repeat::One {
        return Some(current);
    }
    if shuffle {
        if len == 1 {
            return (repeat == Repeat::All).then_some(0);
        }
        // Any other track.
        let pick = rng.below(len - 1);
        return Some(if pick >= current { pick + 1 } else { pick });
    }
    match current + 1 {
        n if n < len => Some(n),
        _ if repeat == Repeat::All => Some(0),
        _ => None,
    }
}

/// The track for the "next" button (always moves, wrapping around).
pub fn next(current: usize, len: usize, shuffle: bool, rng: &mut Rng) -> usize {
    after(current, len, shuffle, Repeat::All, rng).unwrap_or(0)
}

pub fn previous(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + len - 1) % len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_music_recursively_and_sorted() {
        let dir = std::env::temp_dir().join(format!("music_scan_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("B Album")).unwrap();
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        for f in ["b.MP3", "a.flac", "notes.txt", "._a.mp3", "B Album/01 song.ogg", ".hidden/x.mp3"] {
            fs::write(dir.join(f), "").unwrap();
        }
        let names: Vec<String> = scan(&dir).into_iter().map(|t| t.name).collect();
        // Sorted by full path: "b album/..." comes before "b.mp3".
        assert_eq!(names, vec!["a", "01 song", "b"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn formats_time() {
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(65.9), "1:05");
        assert_eq!(fmt_time(3725.0), "1:02:05");
    }

    #[test]
    fn play_order() {
        let mut rng = Rng::new(42);
        assert_eq!(after(0, 3, false, Repeat::Off, &mut rng), Some(1));
        assert_eq!(after(2, 3, false, Repeat::Off, &mut rng), None);
        assert_eq!(after(2, 3, false, Repeat::All, &mut rng), Some(0));
        assert_eq!(after(1, 3, true, Repeat::One, &mut rng), Some(1));
        for _ in 0..50 {
            let n = after(1, 3, true, Repeat::Off, &mut rng).unwrap();
            assert!(n != 1 && n < 3);
        }
        assert_eq!(after(0, 1, true, Repeat::Off, &mut rng), None);
        assert_eq!(after(0, 0, false, Repeat::All, &mut rng), None);
        assert_eq!(next(2, 3, false, &mut rng), 0);
        assert_eq!(previous(0, 3), 2);
        assert_eq!(Repeat::Off.next().next().next(), Repeat::Off);
    }
}
