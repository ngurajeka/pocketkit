//! "At startup" options, kept as a marked block in MinUI's boot hook
//! `$USERDATA_PATH/auto.sh` (MinUI.pak/launch.sh runs it once at boot, with
//! SDCARD_PATH and PLATFORM exported). Only the lines between `BEGIN` and `END`
//! are ever changed; anything else in the file is left exactly as it was, and the
//! original file is copied to `auto.sh.before-settings-pak` before the first change.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const BEGIN: &str = "# >>> minui-paks: managed by Settings.pak (lines up to the end marker are rewritten)";
pub const END: &str = "# <<< minui-paks";
pub const BACKUP_NAME: &str = "auto.sh.before-settings-pak";
pub const LOG: &str = "/tmp/minui-paks-startup.log";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Startup {
    pub wifi: bool,
    /// Implies `wifi`.
    pub adb: bool,
}

/// Our block's lines, without the markers.
fn block_lines(text: &str) -> Vec<&str> {
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with("# >>> minui-paks") {
            inside = true;
        } else if line.starts_with(END) {
            inside = false;
        } else if inside {
            out.push(line);
        }
    }
    out
}

/// What the current block turns on.
pub fn parse(text: &str) -> Startup {
    let lines = block_lines(text);
    let adb = lines.iter().any(|l| l.contains("adbd"));
    Startup { wifi: adb || lines.iter().any(|l| l.contains("wifi\" --up")), adb }
}

/// The block for `s`, or `None` when nothing should run at startup.
pub fn render_block(s: Startup) -> Option<String> {
    if !s.wifi && !s.adb {
        return None;
    }
    let mut b = vec![
        BEGIN.to_string(),
        "# Runs in the background so it never slows down booting.".into(),
        "(".into(),
        "\tTOOLS=\"$SDCARD_PATH/Tools/$PLATFORM\"".into(),
        "\t\"$TOOLS/Wifi.pak/wifi\" --up".into(),
    ];
    if s.adb {
        b.extend(
            [
                "\t# adbd as ADBD.pak starts it (no password: anyone on your Wi-Fi gets a shell)",
                "\tADBD=\"$TOOLS/ADBD.pak\"",
                "\tif [ -x \"$ADBD/adbd\" ] && ! pidof adbd >/dev/null 2>&1; then",
                "\t\tcd \"$ADBD\" && mount -o bind ./profile /etc/profile && LD_LIBRARY_PATH=\"$LD_LIBRARY_PATH:.\" ./adbd &",
                "\tfi",
            ]
            .map(String::from),
        );
    }
    b.push(format!(") > {LOG} 2>&1 &"));
    b.push(END.to_string());
    Some(b.join("\n") + "\n")
}

/// `existing` with our block replaced (or removed, or appended at the end).
pub fn apply(existing: Option<&str>, s: Startup) -> String {
    let mut out = String::new();
    let mut inside = false;
    match existing {
        Some(text) => {
            for line in text.lines() {
                if line.starts_with("# >>> minui-paks") {
                    inside = true;
                } else if line.starts_with(END) {
                    inside = false;
                } else if !inside {
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        None => out.push_str("#!/bin/sh\n"),
    }
    if let Some(block) = render_block(s) {
        if !out.ends_with("\n\n") && !out.ends_with("#!/bin/sh\n") {
            out.push('\n');
        }
        out.push_str(&block);
    } else {
        while out.ends_with("\n\n") {
            out.pop();
        }
    }
    out
}

pub fn path(userdata: &Path) -> PathBuf {
    userdata.join("auto.sh")
}

pub fn load(userdata: &Path) -> Startup {
    fs::read_to_string(path(userdata)).map(|t| parse(&t)).unwrap_or_default()
}

/// Writes the new setting. Backs up an existing auto.sh once, before the
/// first change, and replaces the file atomically.
pub fn save(userdata: &Path, s: Startup) -> io::Result<()> {
    let file = path(userdata);
    let existing = fs::read_to_string(&file).ok();
    let new = apply(existing.as_deref(), s);
    if existing.as_deref() == Some(new.as_str()) {
        return Ok(());
    }
    if existing.is_none() && render_block(s).is_none() {
        return Ok(()); // nothing to turn off, don't create a file
    }
    fs::create_dir_all(userdata)?;
    let backup = userdata.join(BACKUP_NAME);
    if existing.is_some() && !backup.exists() {
        fs::copy(&file, &backup)?;
    }
    let tmp = userdata.join("auto.sh.tmp");
    fs::write(&tmp, new)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    fs::rename(&tmp, &file)?;
    if let Ok(f) = fs::File::open(&file) {
        let _ = f.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "#!/bin/sh\n# my own stuff\necho hello > /tmp/hi\n";

    #[test]
    fn adds_replaces_and_removes_only_our_block() {
        let on = apply(Some(USER), Startup { wifi: true, adb: false });
        assert!(on.starts_with(USER));
        assert_eq!(parse(&on), Startup { wifi: true, adb: false });
        assert!(on.contains("\"$TOOLS/Wifi.pak/wifi\" --up") && !on.contains("adbd"));

        let both = apply(Some(&on), Startup { wifi: true, adb: true });
        assert_eq!(both.matches(BEGIN).count(), 1);
        assert_eq!(parse(&both), Startup { wifi: true, adb: true });
        assert!(both.starts_with(USER));

        let off = apply(Some(&both), Startup::default());
        assert_eq!(off, USER);
        assert_eq!(parse(&off), Startup::default());
    }

    #[test]
    fn keeps_user_lines_after_the_block() {
        let text = format!("{USER}\n{}echo after\n", render_block(Startup { wifi: true, adb: false }).unwrap());
        let changed = apply(Some(&text), Startup { wifi: true, adb: true });
        assert!(changed.contains("echo after") && changed.contains("echo hello"));
    }

    #[test]
    fn new_file_gets_a_shebang_and_runs_in_the_background() {
        let new = apply(None, Startup { wifi: true, adb: true });
        assert!(new.starts_with("#!/bin/sh\n"));
        let block = render_block(Startup { wifi: false, adb: true }).unwrap();
        assert!(block.contains(&format!(") > {LOG} 2>&1 &")));
        assert!(block.contains("--up"), "adb implies wifi");
        assert_eq!(render_block(Startup::default()), None);
    }

    #[test]
    fn saves_with_a_one_time_backup() {
        let dir = std::env::temp_dir().join(format!("autostart_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Nothing on and no file: nothing written.
        save(&dir, Startup::default()).unwrap();
        assert!(!path(&dir).exists());

        fs::write(path(&dir), USER).unwrap();
        save(&dir, Startup { wifi: true, adb: false }).unwrap();
        assert_eq!(fs::read_to_string(dir.join(BACKUP_NAME)).unwrap(), USER);
        assert_eq!(load(&dir), Startup { wifi: true, adb: false });
        save(&dir, Startup { wifi: true, adb: true }).unwrap();
        // The backup is the original, not the previous version.
        assert_eq!(fs::read_to_string(dir.join(BACKUP_NAME)).unwrap(), USER);
        save(&dir, Startup::default()).unwrap();
        assert_eq!(fs::read_to_string(path(&dir)).unwrap(), USER);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn block_is_valid_shell() {
        // `sh -n` parses without running anything.
        let dir = std::env::temp_dir().join(format!("autostart_sh_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("auto.sh");
        fs::write(&f, apply(Some(USER), Startup { wifi: true, adb: true })).unwrap();
        let ok = std::process::Command::new("sh").arg("-n").arg(&f).status().unwrap().success();
        assert!(ok);
        fs::remove_dir_all(dir).unwrap();
    }
}
