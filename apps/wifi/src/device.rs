//! The real Wi-Fi backend for the Miyoo Mini+. Bring-up follows MinUI's ADBD.pak
//! (Realtek 8188fu driver, `axp_test wifion`, wpa_supplicant, udhcpc); everything
//! else goes through wpa_supplicant's control socket.
//!
//! Not yet verified on hardware. Every external command is logged to stderr
//! (log.txt) so failures can be diagnosed.

use std::{
    fs,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{
    backend::{Backend, Target},
    wpa::{self, Ctrl, Network, Saved, Status},
};

const IFACE: &str = "wlan0";
const AXP: &str = "/customer/app/axp_test";
const DHCP_SCRIPT: &str = "/etc/init.d/udhcpc.script";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const IP_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Device {
    conf: PathBuf,
    pak_dir: PathBuf,
    ctrl: Option<Ctrl>,
}

/// Runs a command, logging it and its output. Returns stdout on success.
fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    eprintln!("$ {cmd} {}", args.join(" "));
    match Command::new(cmd).args(args).stdin(Stdio::null()).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() {
                eprintln!("  {}", stderr.trim());
            }
            if out.status.success() {
                Ok(stdout)
            } else {
                Err(format!("{cmd} failed ({})", out.status))
            }
        }
        Err(e) => {
            eprintln!("  {e}");
            Err(format!("{cmd}: {e}"))
        }
    }
}

/// Starts a long-running program in its own process group so it outlives the pak.
fn spawn_detached(cmd: &str, args: &[&str]) -> Result<(), String> {
    eprintln!("$ {cmd} {} &", args.join(" "));
    Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{cmd}: {e}"))
}

fn running(name: &str) -> bool {
    run("pidof", &[name]).is_ok_and(|s| !s.trim().is_empty())
}

fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    done()
}

impl Device {
    pub fn new(conf: PathBuf, pak_dir: PathBuf) -> Self {
        Self { conf, pak_dir, ctrl: None }
    }

    fn iface_exists() -> bool {
        Path::new("/sys/class/net").join(IFACE).exists()
    }

    fn ctrl_dir(&self) -> PathBuf {
        wpa::ctrl_dir(&fs::read_to_string(&self.conf).unwrap_or_default())
    }

    fn ctrl(&mut self) -> Result<&Ctrl, String> {
        if self.ctrl.as_ref().is_some_and(|c| c.request("PING").is_ok_and(|r| r.starts_with("PONG"))) {
            return Ok(self.ctrl.as_ref().unwrap());
        }
        self.ctrl = None;
        let ctrl = Ctrl::open(&self.ctrl_dir(), IFACE).map_err(|e| format!("Can't reach wpa_supplicant: {e}"))?;
        Ok(self.ctrl.insert(ctrl))
    }

    fn request(&mut self, cmd: &str) -> Result<String, String> {
        self.ctrl()?.request(cmd).map_err(|e| format!("wpa_supplicant not responding: {e}"))
    }

    fn ok(&mut self, cmd: &str) -> Result<(), String> {
        self.ctrl()?.ok(cmd)
    }

    /// MinUI doesn't load the Realtek driver by default; ADBD.pak ships a copy.
    fn load_driver(&self) -> Result<(), String> {
        if Path::new("/sys/module/8188fu").exists() {
            return Ok(());
        }
        let candidates = [self.pak_dir.join("8188fu.ko"), self.pak_dir.join("../ADBD.pak/8188fu.ko")];
        let ko = candidates
            .iter()
            .find(|p| p.exists())
            .ok_or("Wi-Fi driver not found. Install ADBD.pak (MinUI Extras) or copy 8188fu.ko into Wifi.pak.")?;
        run("insmod", &[&ko.to_string_lossy()]).map(drop)
    }

    /// Makes sure the config has a control socket and allows SAVE_CONFIG.
    fn prepare_conf(&self) -> Result<(), String> {
        let text = fs::read_to_string(&self.conf).unwrap_or_default();
        if let Some(fixed) = wpa::ensure_conf_header(&text) {
            if !text.is_empty() {
                let backup = self.pak_dir.join("backup/wpa_supplicant.conf.bak");
                if !backup.exists() {
                    let _ = fs::create_dir_all(backup.parent().unwrap());
                    let _ = fs::write(&backup, &text);
                }
            }
            let tmp = self.conf.with_extension("conf.new");
            fs::write(&tmp, fixed).and_then(|_| fs::rename(&tmp, &self.conf)).map_err(|e| {
                format!("Can't write {}: {e}", self.conf.display())
            })?;
            eprintln!("updated {}", self.conf.display());
        }
        Ok(())
    }

    fn restart_dhcp(&self) {
        let _ = run("pkill", &["udhcpc"]);
        let _ = spawn_detached("udhcpc", &["-i", IFACE, "-s", DHCP_SCRIPT]);
    }

    fn ip(&mut self) -> Option<String> {
        if let Some(ip) = self.status().ok().and_then(|s| s.ip) {
            return Some(ip);
        }
        wpa::parse_ifconfig_ip(&run("ifconfig", &[IFACE]).ok()?)
    }
}

impl Backend for Device {
    fn is_on(&mut self) -> bool {
        Self::iface_exists() && running("wpa_supplicant")
    }

    fn power_on(&mut self) -> Result<(), String> {
        self.load_driver()?;
        if !Self::iface_exists() {
            run(AXP, &["wifion"])?;
            if !wait_for(Duration::from_secs(8), Self::iface_exists) {
                return Err("Wi-Fi chip didn't show up (wlan0 missing)".into());
            }
        }
        run("ifconfig", &[IFACE, "up"])?;
        self.prepare_conf()?;
        // An instance started without a control socket (or with an old config) can't be driven.
        if self.ctrl().is_err() {
            if running("wpa_supplicant") {
                let _ = run("pkill", &["wpa_supplicant"]);
                thread::sleep(Duration::from_millis(500));
            }
            let conf = self.conf.to_string_lossy().into_owned();
            run("wpa_supplicant", &["-B", "-D", "nl80211", "-i", IFACE, "-c", &conf])?;
            if !wait_for(Duration::from_secs(5), || self.ctrl().is_ok()) {
                return Err("wpa_supplicant started but its control socket is missing".into());
            }
        }
        if !running("udhcpc") {
            let _ = spawn_detached("udhcpc", &["-i", IFACE, "-s", DHCP_SCRIPT]);
        }
        let _ = run("iw", &["dev", IFACE, "set", "power_save", "off"]);
        Ok(())
    }

    fn power_off(&mut self) {
        self.ctrl = None;
        let _ = run("pkill", &["wpa_supplicant"]);
        let _ = run("pkill", &["udhcpc"]);
        let _ = run("ifconfig", &[IFACE, "down"]);
        let _ = run(AXP, &["wifioff"]);
    }

    fn scan(&mut self) -> Result<Vec<Network>, String> {
        // FAIL-BUSY just means a scan is already running; its results are fine too.
        let _ = self.request("SCAN")?;
        thread::sleep(Duration::from_millis(3500));
        Ok(wpa::parse_scan_results(&self.request("SCAN_RESULTS")?))
    }

    fn saved(&mut self) -> Result<Vec<Saved>, String> {
        Ok(wpa::parse_list_networks(&self.request("LIST_NETWORKS")?))
    }

    fn status(&mut self) -> Result<Status, String> {
        Ok(wpa::parse_status(&self.request("STATUS")?))
    }

    fn connect(&mut self, t: &Target) -> Result<Option<String>, String> {
        let id = match t.saved_id {
            Some(id) => id,
            None => {
                let id = self.request("ADD_NETWORK")?.trim().parse::<u32>().map_err(|_| "ADD_NETWORK failed")?;
                self.ok(&format!("SET_NETWORK {id} ssid {}", wpa::hex_ssid(&t.ssid)))?;
                self.ok(&format!("SET_NETWORK {id} scan_ssid 1"))?;
                if t.psk.is_none() {
                    self.ok(&format!("SET_NETWORK {id} key_mgmt NONE"))?;
                }
                id
            }
        };
        if let Some(psk) = &t.psk {
            if !wpa::valid_psk(psk) {
                return Err("Password must be 8-63 characters".into());
            }
            self.ok(&format!("SET_NETWORK {id} psk \"{psk}\""))?;
            self.ok(&format!("SET_NETWORK {id} key_mgmt WPA-PSK"))?;
        }
        self.ok(&format!("SELECT_NETWORK {id}"))?;

        let ssid = t.ssid.clone();
        let connected = wait_for(CONNECT_TIMEOUT, || {
            self.status().is_ok_and(|s| s.connected() && s.ssid.as_deref() == Some(ssid.as_str()))
        });
        // SELECT_NETWORK disabled the others; let them auto-connect again later.
        let _ = self.ok("ENABLE_NETWORK all");
        if !connected {
            if t.saved_id.is_none() {
                let _ = self.ok(&format!("REMOVE_NETWORK {id}"));
            }
            return Err("Couldn't connect. Check the password.".into());
        }
        if let Err(e) = self.ok("SAVE_CONFIG") {
            eprintln!("{e}");
        }
        self.restart_dhcp();
        let mut ip = None;
        wait_for(IP_TIMEOUT, || {
            ip = self.ip();
            ip.is_some()
        });
        Ok(ip)
    }

    fn forget(&mut self, id: u32) -> Result<(), String> {
        self.ok(&format!("REMOVE_NETWORK {id}"))?;
        self.ok("SAVE_CONFIG")
    }
}
