//! Screens: the network list and the password keyboard.

use std::sync::mpsc::{Receiver, Sender};

use minui_common::{
    input::Button,
    keyboard::{KbResult, Keyboard},
    render::Canvas,
    ui::{self, Ui},
};

use crate::{
    backend::Target,
    worker::{Cmd, Ev},
    wpa::{self, Network, Saved, Security, Status},
};

/// Networks shown at once (MinUI fits five rows under a title).
const VISIBLE_ROWS: usize = 5;
const FLASH_SECS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    List,
    Password,
}

pub struct App {
    tx: Sender<Cmd>,
    rx: Receiver<Ev>,
    pub screen: Screen,
    pub kb: Keyboard,
    /// The network the password is being typed for.
    pending: Option<Target>,
    pub nets: Vec<Network>,
    pub saved: Vec<Saved>,
    pub status: Status,
    /// `None` until the worker reports.
    pub on: Option<bool>,
    pub sel: usize,
    pub busy: Option<String>,
    flash: Option<(String, f32)>,
    time: f32,
    quitting: bool,
    ui: Ui,
}

impl App {
    pub fn new(tx: Sender<Cmd>, rx: Receiver<Ev>) -> Self {
        Self {
            tx,
            rx,
            screen: Screen::List,
            kb: Keyboard::new(""),
            pending: None,
            nets: Vec::new(),
            saved: Vec::new(),
            status: Status::default(),
            on: None,
            sel: 0,
            busy: None,
            flash: None,
            time: 0.0,
            quitting: false,
            ui: Ui::new(),
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some((msg.into(), FLASH_SECS));
    }

    pub fn flash_text(&self) -> Option<&str> {
        self.flash.as_ref().map(|(m, _)| m.as_str())
    }

    fn send(&mut self, cmd: Cmd) {
        if self.busy.is_some() {
            self.flash("Busy, please wait");
            return;
        }
        // Show busy immediately; the worker confirms with its own Busy event.
        self.busy = Some("Working".into());
        let _ = self.tx.send(cmd);
    }

    fn saved_for(&self, ssid: &str) -> Option<&Saved> {
        self.saved.iter().find(|s| s.ssid == ssid)
    }

    fn selected(&self) -> Option<&Network> {
        self.nets.get(self.sel)
    }

    pub fn handle(&mut self, button: Button) {
        match self.screen {
            Screen::List => self.handle_list(button),
            Screen::Password => match self.kb.handle(button) {
                KbResult::Pending => {}
                KbResult::Cancel => {
                    self.pending = None;
                    self.screen = Screen::List;
                }
                KbResult::Submit(psk) => {
                    if !wpa::valid_psk(&psk) {
                        self.flash("Password must be 8-63 characters");
                    } else if let Some(mut t) = self.pending.take() {
                        t.psk = Some(psk);
                        self.screen = Screen::List;
                        self.send(Cmd::Connect(t));
                    }
                }
            },
        }
    }

    fn ask_password(&mut self, ssid: String, saved_id: Option<u32>) {
        self.pending = Some(Target { ssid, psk: None, saved_id });
        self.kb = Keyboard::new("");
        self.screen = Screen::Password;
    }

    fn handle_list(&mut self, button: Button) {
        let n = self.nets.len();
        match button {
            Button::Up if n > 0 => self.sel = (self.sel + n - 1) % n,
            Button::Down if n > 0 => self.sel = (self.sel + 1) % n,
            Button::L1 | Button::Left => self.sel = self.sel.saturating_sub(VISIBLE_ROWS),
            Button::R1 | Button::Right if n > 0 => self.sel = (self.sel + VISIBLE_ROWS).min(n - 1),
            Button::Start => {
                let cmd = if self.on == Some(true) { Cmd::PowerOff } else { Cmd::PowerOn };
                self.send(cmd);
            }
            Button::B => self.quitting = true,
            _ if self.on != Some(true) => {
                if matches!(button, Button::A | Button::X | Button::Y | Button::Select) {
                    self.flash("Wi-Fi is off. Press START to turn it on.");
                }
            }
            Button::X => self.send(Cmd::Scan),
            Button::A => {
                let Some(net) = self.selected().cloned() else { return };
                let saved_id = self.saved_for(&net.ssid).map(|s| s.id);
                match net.security {
                    Security::Enterprise => self.flash("WPA-Enterprise networks aren't supported"),
                    Security::Wep => self.flash("WEP networks aren't supported"),
                    Security::Psk if saved_id.is_none() => self.ask_password(net.ssid, None),
                    Security::Psk | Security::Open => {
                        self.send(Cmd::Connect(Target { ssid: net.ssid, psk: None, saved_id }))
                    }
                }
            }
            // Re-enter the password of a saved network.
            Button::Select => {
                let Some(net) = self.selected().cloned() else { return };
                if net.security == Security::Psk {
                    let saved_id = self.saved_for(&net.ssid).map(|s| s.id);
                    self.ask_password(net.ssid, saved_id);
                }
            }
            Button::Y => {
                let Some(net) = self.selected().cloned() else { return };
                match self.saved_for(&net.ssid).map(|s| s.id) {
                    Some(id) => self.send(Cmd::Forget { id, ssid: net.ssid }),
                    None => self.flash("This network isn't saved"),
                }
            }
            _ => {}
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        if let Some((_, t)) = &mut self.flash {
            *t -= dt;
            if *t <= 0.0 {
                self.flash = None;
            }
        }
        while let Ok(ev) = self.rx.try_recv() {
            self.apply(ev);
        }
    }

    pub fn apply(&mut self, ev: Ev) {
        match ev {
            Ev::Busy(msg) => self.busy = Some(msg),
            Ev::Idle => self.busy = None,
            Ev::Power(on) => self.on = Some(on),
            Ev::Status(st) => self.status = st,
            Ev::Saved(saved) => self.saved = saved,
            Ev::Message(msg) => self.flash(msg),
            Ev::Networks(nets) => {
                // Keep the highlight on the same network if it's still there.
                let keep = self.selected().map(|n| n.ssid.clone());
                self.nets = nets;
                self.sel = keep
                    .and_then(|s| self.nets.iter().position(|n| n.ssid == s))
                    .unwrap_or(0)
                    .min(self.nets.len().saturating_sub(1));
            }
        }
    }

    // ---------------------------------------------------------------- drawing

    pub fn render(&self, c: &mut Canvas) {
        match self.screen {
            Screen::List => self.draw_list(c),
            Screen::Password => {
                let ssid = self.pending.as_ref().map_or("", |t| t.ssid.as_str());
                self.ui.keyboard(c, &self.kb, &format!("Password for {ssid}"), "CONNECT", self.time);
            }
        }
        // Busy (with animated dots) wins over a message.
        if let Some(busy) = &self.busy {
            let dots = ".".repeat((self.time * 3.0) as usize % 4);
            self.ui.toast(c, &format!("{busy}{dots}"));
        } else if let Some(msg) = self.flash_text() {
            self.ui.toast(c, msg);
        }
    }

    fn title_text(&self) -> String {
        match self.on {
            Some(false) => "Wi-Fi is off".into(),
            None => "Wi-Fi".into(),
            _ if self.status.connected() => {
                let ssid = self.status.ssid.as_deref().unwrap_or("?");
                match &self.status.ip {
                    Some(ip) => format!("{ssid}  {ip}"),
                    None => format!("{ssid}  getting address..."),
                }
            }
            _ => "Wi-Fi: not connected".into(),
        }
    }

    fn draw_list(&self, c: &mut Canvas) {
        self.ui.clear(c);
        self.ui.title(c, &self.title_text());
        match self.on {
            Some(false) => self.ui.message(c, "Wi-Fi is off\nPress START to turn it on"),
            Some(true) if self.nets.is_empty() && self.busy.is_none() => {
                self.ui.message(c, "No networks found\nPress X to scan")
            }
            _ => {}
        }
        let first = self.sel.saturating_sub(VISIBLE_ROWS - 1);
        for (row, (i, net)) in self.nets.iter().enumerate().skip(first).take(VISIBLE_ROWS).enumerate() {
            let selected = i == self.sel;
            let current = self.status.connected() && self.status.ssid.as_deref() == Some(net.ssid.as_str());
            let tag = if current {
                "connected"
            } else if self.saved_for(&net.ssid).is_some() {
                "saved"
            } else {
                match net.security {
                    Security::Open => "open",
                    Security::Psk => "",
                    Security::Wep => "WEP",
                    Security::Enterprise => "802.1X",
                }
            };
            let unsupported = matches!(net.security, Security::Wep | Security::Enterprise);
            let bx = self.ui.row_ex(c, row, &net.ssid, (selected, unsupported), (!tag.is_empty()).then_some(tag), BARS_W);
            let on = if selected { ui::BLACK } else { ui::WHITE };
            let off = if selected { 0xC8C8C8 } else { ui::DARK_GRAY };
            draw_bars(c, bx, Ui::row_y(row) + (ui::PILL - 22) / 2, wpa::signal_bars(net.signal), on, off);
        }
        self.ui.scroll_marks(c, first, VISIBLE_ROWS, self.nets.len());
        let power = if self.on == Some(true) { "OFF" } else { "ON" };
        if self.on == Some(true) {
            self.ui.hints(c, &[("START", power), ("X", "SCAN")], false);
            let forget = self.selected().is_some_and(|n| self.saved_for(&n.ssid).is_some());
            if forget {
                self.ui.hints(c, &[("Y", "FORGET"), ("A", "CONNECT")], true);
            } else {
                self.ui.hints(c, &[("A", "CONNECT")], true);
            }
        } else {
            self.ui.hints(c, &[("START", power)], false);
            self.ui.hints(c, &[("B", "QUIT")], true);
        }
    }
}

const BARS_W: i32 = 30;

/// Four rising bars (30 x 22 px); `n` of them filled.
fn draw_bars(c: &mut Canvas, x: i32, y: i32, n: usize, on: u32, off: u32) {
    for i in 0..4 {
        let h = 7 + 5 * i as i32;
        ui::rounded_rect(c, x + i as i32 * 8, y + 22 - h, 6, h, 2, if i < n { on } else { off });
    }
}

impl minui_common::App for App {
    fn handle(&mut self, button: Button) {
        App::handle(self, button);
    }
    fn update(&mut self, dt: f32) {
        App::update(self, dt);
    }
    fn render(&self, canvas: &mut Canvas) {
        App::render(self, canvas);
    }
    fn quit(&self) -> bool {
        self.quitting
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use minui_common::render::{H, W};

    fn net(ssid: &str, security: Security) -> Network {
        Network { ssid: ssid.into(), signal: -50, security }
    }

    /// An app wired to channels the test controls, with Wi-Fi on and two networks.
    fn app() -> (App, mpsc::Receiver<Cmd>) {
        let (tx, cmds) = mpsc::channel();
        let (_evt, rx) = mpsc::channel();
        let mut app = App::new(tx, rx);
        app.apply(Ev::Power(true));
        app.apply(Ev::Networks(vec![net("Home", Security::Psk), net("Cafe", Security::Open), net("Corp", Security::Enterprise)]));
        app.apply(Ev::Saved(vec![Saved { id: 7, ssid: "Cafe".into(), current: false }]));
        (app, cmds)
    }

    #[test]
    fn new_password_network_asks_for_password() {
        let (mut app, cmds) = app();
        app.handle(Button::A);
        assert_eq!(app.screen, Screen::Password);
        app.kb.text = "short".into();
        app.handle(Button::Start);
        assert_eq!(app.screen, Screen::Password);
        assert!(app.flash_text().unwrap().contains("8-63"));
        app.kb.text = "longenough".into();
        app.handle(Button::Start);
        assert_eq!(app.screen, Screen::List);
        assert_eq!(
            cmds.try_recv().unwrap(),
            Cmd::Connect(Target { ssid: "Home".into(), psk: Some("longenough".into()), saved_id: None })
        );
    }

    #[test]
    fn saved_or_open_network_connects_directly() {
        let (mut app, cmds) = app();
        app.handle(Button::Down);
        app.handle(Button::A);
        assert_eq!(cmds.try_recv().unwrap(), Cmd::Connect(Target { ssid: "Cafe".into(), psk: None, saved_id: Some(7) }));
        // Busy until the worker says it's idle.
        app.handle(Button::A);
        assert!(cmds.try_recv().is_err());
        app.apply(Ev::Idle);
        app.handle(Button::Y);
        assert_eq!(cmds.try_recv().unwrap(), Cmd::Forget { id: 7, ssid: "Cafe".into() });
    }

    #[test]
    fn unsupported_networks_and_power_off() {
        let (mut app, cmds) = app();
        app.handle(Button::Up); // wraps to "Corp"
        app.handle(Button::A);
        assert!(app.flash_text().unwrap().contains("Enterprise"));
        app.handle(Button::Start);
        assert_eq!(cmds.try_recv().unwrap(), Cmd::PowerOff);
        app.apply(Ev::Idle);
        app.apply(Ev::Power(false));
        app.handle(Button::X);
        assert!(cmds.try_recv().is_err());
        assert!(app.flash_text().unwrap().contains("START"));
    }

    #[test]
    fn keeps_selection_across_rescans() {
        let (mut app, _cmds) = app();
        app.handle(Button::Down); // Cafe
        app.apply(Ev::Networks(vec![net("New", Security::Open), net("Cafe", Security::Open)]));
        assert_eq!(app.selected().unwrap().ssid, "Cafe");
        app.apply(Ev::Networks(vec![]));
        assert_eq!(app.sel, 0);
    }

    #[test]
    fn renders_both_screens() {
        let (mut app, _cmds) = app();
        let mut c = Canvas::new(W, H);
        app.apply(Ev::Status(Status { state: "COMPLETED".into(), ssid: Some("Cafe".into()), ip: None }));
        app.render(&mut c);
        app.handle(Button::A);
        app.render(&mut c);
    }
}
