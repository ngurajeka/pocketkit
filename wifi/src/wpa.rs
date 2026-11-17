//! wpa_supplicant control-interface client, plus parsers for its replies.
//! Talking to the control socket directly means we don't need `wpa_cli` or `iw`
//! on the device.

use std::{
    io,
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Open,
    /// WPA/WPA2 (or WPA3 transition) with a password.
    Psk,
    Wep,
    /// WPA-Enterprise (802.1X): not supported.
    Enterprise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// dBm, e.g. -45 (strong) .. -90 (weak).
    pub signal: i32,
    pub security: Security,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub id: u32,
    pub ssid: String,
    pub current: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    /// wpa_state, e.g. COMPLETED, SCANNING, DISCONNECTED.
    pub state: String,
    pub ssid: Option<String>,
    pub ip: Option<String>,
}

impl Status {
    pub fn connected(&self) -> bool {
        self.state == "COMPLETED"
    }
}

/// Reverses wpa_supplicant's `printf_encode` (`\\`, `\"`, `\e`, `\n`, `\r`, `\t`, `\xNN`).
pub fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            let (byte, len) = match b[i + 1] {
                b'n' => (b'\n', 2),
                b'r' => (b'\r', 2),
                b't' => (b'\t', 2),
                b'e' => (0x1b, 2),
                b'x' => match s.get(i + 2..i + 4).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => (v, 4),
                    None => (b'x', 2),
                },
                c => (c, 2),
            };
            out.push(byte);
            i += len;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn security(flags: &str) -> Security {
    if flags.contains("EAP") {
        Security::Enterprise
    } else if flags.contains("PSK") || flags.contains("SAE") {
        Security::Psk
    } else if flags.contains("WEP") {
        Security::Wep
    } else {
        Security::Open
    }
}

/// `SCAN_RESULTS`: one entry per network name (strongest access point wins),
/// strongest first. Hidden networks (empty name) are skipped.
pub fn parse_scan_results(text: &str) -> Vec<Network> {
    let mut nets: Vec<Network> = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.splitn(5, '\t').collect();
        if f.len() < 5 {
            continue;
        }
        let ssid = unescape(f[4]);
        if ssid.trim_matches('\0').is_empty() {
            continue;
        }
        let signal = f[2].trim().parse().unwrap_or(-100);
        let net = Network { ssid, signal, security: security(f[3]) };
        match nets.iter_mut().find(|n| n.ssid == net.ssid) {
            Some(n) if n.signal < net.signal => *n = net,
            Some(_) => {}
            None => nets.push(net),
        }
    }
    nets.sort_by(|a, b| b.signal.cmp(&a.signal).then_with(|| a.ssid.cmp(&b.ssid)));
    nets
}

/// `LIST_NETWORKS`: the saved networks.
pub fn parse_list_networks(text: &str) -> Vec<Saved> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            Some(Saved {
                id: f.first()?.trim().parse().ok()?,
                ssid: unescape(f.get(1)?),
                current: f.get(3).is_some_and(|fl| fl.contains("[CURRENT]")),
            })
        })
        .collect()
}

/// `STATUS`: `key=value` lines.
pub fn parse_status(text: &str) -> Status {
    let mut st = Status::default();
    for line in text.lines() {
        match line.split_once('=') {
            Some(("wpa_state", v)) => st.state = v.to_string(),
            Some(("ssid", v)) => st.ssid = Some(unescape(v)),
            Some(("ip_address", v)) if !v.is_empty() => st.ip = Some(v.to_string()),
            _ => {}
        }
    }
    st
}

/// The IPv4 address from `ifconfig` output (busybox `inet addr:` or `inet `).
pub fn parse_ifconfig_ip(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let rest = l.trim_start().strip_prefix("inet ")?;
        let rest = rest.strip_prefix("addr:").unwrap_or(rest).trim_start();
        let ip: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
        (!ip.is_empty()).then_some(ip)
    })
}

/// SSIDs are sent hex-encoded so any byte (quotes, spaces, UTF-8) is safe.
pub fn hex_ssid(ssid: &str) -> String {
    ssid.bytes().map(|b| format!("{b:02x}")).collect()
}

/// A WPA passphrase must be 8-63 printable ASCII characters.
pub fn valid_psk(psk: &str) -> bool {
    (8..=63).contains(&psk.len()) && psk.bytes().all(|b| (0x20..0x7f).contains(&b))
}

/// 0-4 bars for a signal level in dBm.
pub fn signal_bars(dbm: i32) -> usize {
    match dbm {
        d if d >= -55 => 4,
        d if d >= -65 => 3,
        d if d >= -75 => 2,
        d if d >= -85 => 1,
        _ => 0,
    }
}

/// Adds the lines the control socket and SAVE_CONFIG need to a
/// wpa_supplicant.conf. Returns `None` if nothing had to change.
pub fn ensure_conf_header(text: &str) -> Option<String> {
    let has = |key: &str| text.lines().any(|l| l.trim_start().starts_with(key));
    let mut header = String::new();
    if !has("ctrl_interface=") {
        header.push_str("ctrl_interface=/var/run/wpa_supplicant\n");
    }
    if !has("update_config=") {
        header.push_str("update_config=1\n");
    }
    if header.is_empty() {
        return None;
    }
    Some(header + text)
}

/// Directory of the control sockets, from `ctrl_interface=` (plain path or `DIR=... GROUP=...`).
pub fn ctrl_dir(conf: &str) -> PathBuf {
    conf.lines()
        .find_map(|l| l.trim().strip_prefix("ctrl_interface="))
        .map(|v| {
            let v = v.trim();
            let dir = match v.strip_prefix("DIR=") {
                Some(rest) => rest.split_whitespace().next().unwrap_or(""),
                None => v,
            };
            PathBuf::from(dir)
        })
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/var/run/wpa_supplicant"))
}

/// A connection to wpa_supplicant's control socket for one interface.
pub struct Ctrl {
    sock: UnixDatagram,
    local: PathBuf,
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

impl Ctrl {
    pub fn open(dir: &Path, iface: &str) -> io::Result<Self> {
        let local = std::env::temp_dir().join(format!(
            "wifi_pak_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&local);
        let sock = UnixDatagram::bind(&local)?;
        let ctrl = Self { sock, local };
        ctrl.sock.connect(dir.join(iface))?;
        ctrl.sock.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(ctrl)
    }

    pub fn request(&self, cmd: &str) -> io::Result<String> {
        self.sock.send(cmd.as_bytes())?;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = self.sock.recv(&mut buf)?;
            let reply = String::from_utf8_lossy(&buf[..n]).into_owned();
            // Unsolicited event messages start with "<level>"; skip them.
            if !reply.starts_with('<') {
                return Ok(reply);
            }
        }
    }

    /// Sends a command that must answer `OK`.
    pub fn ok(&self, cmd: &str) -> Result<(), String> {
        match self.request(cmd) {
            Ok(r) if r.trim() == "OK" => Ok(()),
            Ok(r) => Err(format!("{} failed: {}", cmd.split(' ').next().unwrap_or(cmd), r.trim())),
            Err(e) => Err(format!("wpa_supplicant not responding: {e}")),
        }
    }
}

impl Drop for Ctrl {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scan_results() {
        let text = "bssid / frequency / signal level / flags / ssid\n\
            aa:aa:aa:aa:aa:01\t2412\t-70\t[WPA2-PSK-CCMP][ESS]\tHome\n\
            aa:aa:aa:aa:aa:02\t2437\t-45\t[WPA2-PSK-CCMP][ESS]\tHome\n\
            aa:aa:aa:aa:aa:03\t2462\t-60\t[ESS]\tCafe \\\"Free\\\"\n\
            aa:aa:aa:aa:aa:04\t2462\t-80\t[WPA2-EAP-CCMP][ESS]\tCorp\n\
            aa:aa:aa:aa:aa:05\t2462\t-50\t[WPA2-PSK-CCMP][ESS]\t\n\
            aa:aa:aa:aa:aa:06\t2462\t-88\t[WEP][ESS]\tOld\\xe2\\x98\\x95\n";
        let nets = parse_scan_results(text);
        let summary: Vec<(&str, i32, Security)> = nets.iter().map(|n| (n.ssid.as_str(), n.signal, n.security)).collect();
        assert_eq!(
            summary,
            vec![
                ("Home", -45, Security::Psk),
                ("Cafe \"Free\"", -60, Security::Open),
                ("Corp", -80, Security::Enterprise),
                ("Old\u{2615}", -88, Security::Wep),
            ]
        );
    }

    #[test]
    fn parses_saved_and_status() {
        let saved = parse_list_networks("network id / ssid / bssid / flags\n0\tHome\tany\t[CURRENT]\n3\tCafe\tany\t[DISABLED]\n");
        assert_eq!(
            saved,
            vec![
                Saved { id: 0, ssid: "Home".into(), current: true },
                Saved { id: 3, ssid: "Cafe".into(), current: false },
            ]
        );
        let st = parse_status("bssid=aa\nfreq=2412\nssid=Home\nid=0\nwpa_state=COMPLETED\nip_address=192.168.1.9\n");
        assert!(st.connected());
        assert_eq!(st.ssid.as_deref(), Some("Home"));
        assert_eq!(st.ip.as_deref(), Some("192.168.1.9"));
        assert!(!parse_status("wpa_state=SCANNING\n").connected());
    }

    #[test]
    fn parses_ifconfig() {
        let busybox = "wlan0     Link encap:Ethernet  HWaddr 00:11\n          inet addr:192.168.1.42  Bcast:192.168.1.255\n          inet6 addr: fe80::1/64";
        assert_eq!(parse_ifconfig_ip(busybox).as_deref(), Some("192.168.1.42"));
        assert_eq!(parse_ifconfig_ip("  inet 10.0.0.7  netmask 255.0.0.0").as_deref(), Some("10.0.0.7"));
        assert_eq!(parse_ifconfig_ip("wlan0 Link encap\n inet6 addr: fe80::1/64"), None);
    }

    #[test]
    fn unescapes_and_encodes() {
        assert_eq!(unescape("a\\\\b\\tc\\x41\\xZZ"), "a\\b\tcAxZZ");
        assert_eq!(unescape("trailing\\"), "trailing\\");
        assert_eq!(hex_ssid("Hi \""), "48692022");
    }

    #[test]
    fn checks_passwords_and_bars() {
        assert!(valid_psk("password1"));
        assert!(!valid_psk("short"));
        assert!(!valid_psk(&"x".repeat(64)));
        assert!(!valid_psk("caf\u{e9}caf\u{e9}"));
        assert_eq!((signal_bars(-40), signal_bars(-70), signal_bars(-95)), (4, 2, 0));
    }

    #[test]
    fn fixes_conf_header() {
        assert_eq!(
            ensure_conf_header("network={\n}\n").unwrap(),
            "ctrl_interface=/var/run/wpa_supplicant\nupdate_config=1\nnetwork={\n}\n"
        );
        assert_eq!(ensure_conf_header("ctrl_interface=/x\nupdate_config=1\n"), None);
        assert_eq!(ctrl_dir("ctrl_interface=DIR=/run/wpa GROUP=0\n"), PathBuf::from("/run/wpa"));
        assert_eq!(ctrl_dir("ctrl_interface=/var/run/x\n"), PathBuf::from("/var/run/x"));
        assert_eq!(ctrl_dir(""), PathBuf::from("/var/run/wpa_supplicant"));
    }

    #[test]
    fn talks_to_a_control_socket() {
        // A tiny stand-in for wpa_supplicant.
        let dir = std::env::temp_dir().join(format!("wifi_ctrl_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let server = UnixDatagram::bind(dir.join("wlan0")).unwrap();
        let t = std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            for _ in 0..2 {
                let (n, from) = server.recv_from(&mut buf).unwrap();
                let reply: &[u8] = if &buf[..n] == b"PING" { b"PONG\n" } else { b"FAIL\n" };
                server.send_to(b"<3>CTRL-EVENT-SCAN-STARTED", from.as_pathname().unwrap()).unwrap();
                server.send_to(reply, from.as_pathname().unwrap()).unwrap();
            }
        });
        let ctrl = Ctrl::open(&dir, "wlan0").unwrap();
        assert_eq!(ctrl.request("PING").unwrap(), "PONG\n");
        assert_eq!(ctrl.ok("SAVE_CONFIG").unwrap_err(), "SAVE_CONFIG failed: FAIL");
        t.join().unwrap();
        drop(ctrl);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
