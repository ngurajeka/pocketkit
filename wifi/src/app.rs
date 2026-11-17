//! Screens: the network list and the password keyboard.

use std::sync::mpsc::{Receiver, Sender};

use minui_common::{
    input::Button,
    keyboard::{KbResult, Keyboard, COLS as KB_COLS, PAGE_NAMES, ROWS as KB_ROWS},
    render::{fit, fit_end, Canvas, H, W},
};

use crate::{
    backend::Target,
    worker::{Cmd, Ev},
    wpa::{self, Network, Saved, Security, Status},
};

const BG: u32 = 0xFAF8F2;
const FG: u32 = 0x24262B;
const DIM: u32 = 0x8A8F98;
const SEL_BG: u32 = 0x1F5FBF;
const SEL_FG: u32 = 0xFFFFFF;
const BAR: u32 = 0x2B2D42;
const BAR_FG: u32 = 0xEDF2F4;
const GOOD: u32 = 0x2E9E5B;
const WARN: u32 = 0xC0392B;
const KEY_BG: u32 = 0xE4E1D8;

const SCALE: usize = 2;
const LINE: usize = 20;
const MARGIN: i32 = 8;
const COLS: usize = (W - 2 * MARGIN as usize) / 16;
/// Rows for the network list (below the two status lines and a gap).
const LIST_TOP: usize = 4;
const LIST_ROWS: usize = H / LINE - LIST_TOP - 1;
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
            Button::L1 | Button::Left => self.sel = self.sel.saturating_sub(LIST_ROWS),
            Button::R1 | Button::Right if n > 0 => self.sel = (self.sel + LIST_ROWS).min(n - 1),
            Button::Start => {
                let cmd = if self.on == Some(true) { Cmd::PowerOff } else { Cmd::PowerOn };
                self.send(cmd);
            }
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
        c.fill(BG);
        match self.screen {
            Screen::List => self.draw_list(c),
            Screen::Password => self.draw_password(c),
        }
        self.draw_status_bar(c);
    }

    fn title(&self, c: &mut Canvas, text: &str) {
        c.rect(0, 0, W as i32, LINE as i32, BAR);
        c.text(MARGIN, 2, &fit(text, COLS - 5), SCALE, BAR_FG);
        let state = match self.on {
            Some(true) => "ON",
            Some(false) => "OFF",
            None => "",
        };
        c.text(W as i32 - MARGIN - 16 * state.len() as i32, 2, state, SCALE, BAR_FG);
    }

    fn row_y(row: usize) -> i32 {
        (row * LINE + 2) as i32
    }

    fn draw_list(&self, c: &mut Canvas) {
        self.title(c, "Wi-Fi");
        let (line1, line2, color) = match self.on {
            Some(false) => ("Wi-Fi is off".to_string(), "Press START to turn it on".to_string(), DIM),
            _ if self.status.connected() => (
                format!("Connected to {}", self.status.ssid.as_deref().unwrap_or("?")),
                match &self.status.ip {
                    Some(ip) => format!("IP {ip}"),
                    None => "Getting an IP address...".into(),
                },
                GOOD,
            ),
            None => ("Starting...".to_string(), String::new(), DIM),
            _ => ("Not connected".to_string(), "Pick a network and press A".to_string(), FG),
        };
        c.text(MARGIN, Self::row_y(1), &fit(&line1, COLS), SCALE, color);
        c.text(MARGIN, Self::row_y(2), &fit(&line2, COLS), SCALE, DIM);
        c.rect(0, (LIST_TOP * LINE) as i32 - 6, W as i32, 2, KEY_BG);

        if self.nets.is_empty() && self.on == Some(true) && self.busy.is_none() {
            c.text(MARGIN, Self::row_y(LIST_TOP), "No networks found. X to scan.", SCALE, DIM);
        }
        let first = self.sel.saturating_sub(LIST_ROWS - 1);
        for (row, (i, net)) in self.nets.iter().enumerate().skip(first).take(LIST_ROWS).enumerate() {
            let y = Self::row_y(LIST_TOP + row);
            let selected = i == self.sel;
            if selected {
                c.rect(0, y - 2, W as i32, LINE as i32, SEL_BG);
            }
            let fg = if selected { SEL_FG } else { FG };
            draw_bars(c, MARGIN, y, wpa::signal_bars(net.signal), if selected { SEL_FG } else { FG }, if selected { 0x6F95D6 } else { KEY_BG });

            let saved = self.saved_for(&net.ssid);
            let current = self.status.connected() && self.status.ssid.as_deref() == Some(net.ssid.as_str());
            let (tag, tag_color) = if current {
                ("connected", if selected { SEL_FG } else { GOOD })
            } else if saved.is_some() {
                ("saved", if selected { SEL_FG } else { DIM })
            } else {
                match net.security {
                    Security::Open => ("open", if selected { SEL_FG } else { DIM }),
                    Security::Psk => ("", fg),
                    Security::Wep => ("WEP", if selected { SEL_FG } else { WARN }),
                    Security::Enterprise => ("802.1X", if selected { SEL_FG } else { WARN }),
                }
            };
            let tag_cols = tag.len();
            let name_cols = COLS - 3 - if tag_cols > 0 { tag_cols + 1 } else { 0 };
            c.text(MARGIN + 40, y, &fit(&net.ssid, name_cols), SCALE, fg);
            if tag_cols > 0 {
                c.text(W as i32 - MARGIN - 16 * tag_cols as i32, y, tag, SCALE, tag_color);
            }
        }
    }

    fn draw_password(&self, c: &mut Canvas) {
        let ssid = self.pending.as_ref().map_or("", |t| t.ssid.as_str());
        self.title(c, &format!("Password: {ssid}"));
        let box_y = LINE as i32 + 12;
        c.rect(MARGIN, box_y, W as i32 - 2 * MARGIN, 28, 0xFFFFFF);
        c.rect(MARGIN, box_y + 27, W as i32 - 2 * MARGIN, 2, SEL_BG);
        let end = c.text(MARGIN + 6, box_y + 6, &fit_end(&self.kb.text, COLS - 2), SCALE, FG);
        if (self.time * 2.0) as i32 % 2 == 0 {
            c.rect(end + 1, box_y + 4, 2, 20, SEL_BG);
        }
        let (kw, kh, gap) = (56, 44, 4);
        let x0 = (W as i32 - (KB_COLS as i32 * (kw + gap) - gap)) / 2;
        let y0 = box_y + 44;
        for row in 0..KB_ROWS {
            for col in 0..KB_COLS {
                let x = x0 + col as i32 * (kw + gap);
                let y = y0 + row as i32 * (kh + gap);
                let selected = row == self.kb.row && col == self.kb.col;
                c.rect(x, y, kw, kh, if selected { SEL_BG } else { KEY_BG });
                c.glyph(x + (kw - 16) / 2, y + (kh - 16) / 2, self.kb.key_at(row, col), SCALE, if selected { SEL_FG } else { FG });
            }
        }
        let hints = [
            format!("A type B delete Y space X {}", PAGE_NAMES[(self.kb.page + 1) % PAGE_NAMES.len()]),
            "L1 clear START connect SELECT back".to_string(),
        ];
        let hy = y0 + KB_ROWS as i32 * (kh + gap) + 8;
        for (i, h) in hints.iter().enumerate() {
            c.text(MARGIN, hy + i as i32 * LINE as i32, &fit(h, COLS), SCALE, DIM);
        }
    }

    fn draw_status_bar(&self, c: &mut Canvas) {
        let y = (H - LINE) as i32;
        c.rect(0, y, W as i32, LINE as i32, BAR);
        let text = if let Some(busy) = &self.busy {
            format!("{busy}{}", ".".repeat((self.time * 3.0) as usize % 4))
        } else if let Some(msg) = self.flash_text() {
            msg.to_string()
        } else {
            match (self.screen, self.on) {
                (Screen::Password, _) => String::new(),
                (_, Some(true)) => "A connect X scan Y forget START off".into(),
                _ => "START turn on  MENU quit".into(),
            }
        };
        c.text(MARGIN, y + 2, &fit(&text, COLS), SCALE, BAR_FG);
    }
}

/// Four rising bars; `n` of them filled.
fn draw_bars(c: &mut Canvas, x: i32, y: i32, n: usize, on: u32, off: u32) {
    for i in 0..4 {
        let h = 4 + 4 * i as i32;
        c.rect(x + i as i32 * 7, y + 16 - h, 5, h, if i < n { on } else { off });
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
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

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
