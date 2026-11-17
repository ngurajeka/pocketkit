//! What the UI needs from the Wi-Fi hardware, with a simulated implementation
//! for the desktop preview and tests. The real one is in `device.rs`.

use std::{thread, time::Duration};

use crate::wpa::{Network, Saved, Security, Status};

/// A network to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub ssid: String,
    /// `None` for open networks, or to reuse a saved network's password.
    pub psk: Option<String>,
    /// Set when the network is already saved.
    pub saved_id: Option<u32>,
}

/// All calls may block for a few seconds; they run on the worker thread.
pub trait Backend: Send {
    fn is_on(&mut self) -> bool;
    /// Driver, radio power, wpa_supplicant and DHCP client.
    fn power_on(&mut self) -> Result<(), String>;
    fn power_off(&mut self);
    fn scan(&mut self) -> Result<Vec<Network>, String>;
    fn saved(&mut self) -> Result<Vec<Saved>, String>;
    fn status(&mut self) -> Result<Status, String>;
    /// Connects and saves the network. Returns the IP address if one arrived in time.
    fn connect(&mut self, target: &Target) -> Result<Option<String>, String>;
    fn forget(&mut self, id: u32) -> Result<(), String>;
}

/// Simulated Wi-Fi: a few networks; "password123" is the right password.
pub struct Fake {
    on: bool,
    saved: Vec<Saved>,
    current: Option<String>,
    delay: Duration,
}

pub const FAKE_PASSWORD: &str = "password123";

impl Fake {
    pub fn new(delay: Duration) -> Self {
        Self {
            on: false,
            saved: vec![Saved { id: 0, ssid: "Cafe Free WiFi".into(), current: false }],
            current: None,
            delay,
        }
    }

    fn networks() -> Vec<Network> {
        let n = |ssid: &str, signal, security| Network { ssid: ssid.into(), signal, security };
        vec![
            n("HomeNetwork", -42, Security::Psk),
            n("Cafe Free WiFi", -58, Security::Open),
            n("Office", -66, Security::Enterprise),
            n("Neighbour 5G", -74, Security::Psk),
            n("Old Router", -83, Security::Wep),
            n("Far Away", -90, Security::Psk),
        ]
    }
}

impl Backend for Fake {
    fn is_on(&mut self) -> bool {
        self.on
    }

    fn power_on(&mut self) -> Result<(), String> {
        thread::sleep(self.delay);
        self.on = true;
        Ok(())
    }

    fn power_off(&mut self) {
        self.on = false;
        self.current = None;
    }

    fn scan(&mut self) -> Result<Vec<Network>, String> {
        thread::sleep(self.delay * 2);
        Ok(if self.on { Self::networks() } else { Vec::new() })
    }

    fn saved(&mut self) -> Result<Vec<Saved>, String> {
        let current = self.current.clone();
        Ok(self.saved.iter().map(|s| Saved { current: Some(&s.ssid) == current.as_ref(), ..s.clone() }).collect())
    }

    fn status(&mut self) -> Result<Status, String> {
        Ok(match &self.current {
            Some(ssid) => Status { state: "COMPLETED".into(), ssid: Some(ssid.clone()), ip: Some("192.168.1.42".into()) },
            None => Status { state: if self.on { "DISCONNECTED" } else { "INTERFACE_DISABLED" }.into(), ..Status::default() },
        })
    }

    fn connect(&mut self, t: &Target) -> Result<Option<String>, String> {
        thread::sleep(self.delay * 3);
        let net = Self::networks().into_iter().find(|n| n.ssid == t.ssid).ok_or("Network not found")?;
        let ok = match net.security {
            Security::Open => true,
            Security::Psk => t.psk.as_deref() == Some(FAKE_PASSWORD) || (t.psk.is_none() && t.saved_id.is_some()),
            _ => false,
        };
        if !ok {
            return Err("Couldn't connect. Check the password.".into());
        }
        if t.saved_id.is_none() {
            let id = self.saved.iter().map(|s| s.id + 1).max().unwrap_or(0);
            self.saved.push(Saved { id, ssid: t.ssid.clone(), current: false });
        }
        self.current = Some(t.ssid.clone());
        Ok(Some("192.168.1.42".into()))
    }

    fn forget(&mut self, id: u32) -> Result<(), String> {
        if let Some(i) = self.saved.iter().position(|s| s.id == id) {
            let s = self.saved.remove(i);
            if self.current.as_ref() == Some(&s.ssid) {
                self.current = None;
            }
        }
        Ok(())
    }
}
