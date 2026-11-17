//! Runs the (slow, blocking) backend on its own thread so the screen stays responsive.

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use crate::{
    backend::{Backend, Target},
    wpa::{Network, Saved, Status},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    PowerOn,
    PowerOff,
    Scan,
    Connect(Target),
    Forget { id: u32, ssid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    /// Something slow started (shown in the status bar until `Idle`).
    Busy(String),
    Idle,
    Power(bool),
    Status(Status),
    Networks(Vec<Network>),
    Saved(Vec<Saved>),
    Message(String),
}

/// Starts the worker; it turns Wi-Fi on and scans right away.
pub fn spawn(backend: Box<dyn Backend>) -> (Sender<Cmd>, Receiver<Ev>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    let _ = cmd_tx.send(Cmd::PowerOn);
    thread::spawn(move || run(backend, cmd_rx, ev_tx));
    (cmd_tx, ev_rx)
}

fn run(mut b: Box<dyn Backend>, rx: Receiver<Cmd>, tx: Sender<Ev>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(cmd) => {
                handle(b.as_mut(), cmd, &tx);
                let _ = tx.send(Ev::Idle);
            }
            // Keep the "Connected / IP" line fresh.
            Err(RecvTimeoutError::Timeout) => {
                if b.is_on() {
                    if let Ok(st) = b.status() {
                        let _ = tx.send(Ev::Status(st));
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn refresh(b: &mut dyn Backend, tx: &Sender<Ev>) {
    if let Ok(saved) = b.saved() {
        let _ = tx.send(Ev::Saved(saved));
    }
    if let Ok(st) = b.status() {
        let _ = tx.send(Ev::Status(st));
    }
}

fn scan(b: &mut dyn Backend, tx: &Sender<Ev>) {
    let _ = tx.send(Ev::Busy("Scanning".into()));
    match b.scan() {
        Ok(nets) => {
            let _ = tx.send(Ev::Networks(nets));
        }
        Err(e) => {
            let _ = tx.send(Ev::Message(e));
        }
    }
}

fn handle(b: &mut dyn Backend, cmd: Cmd, tx: &Sender<Ev>) {
    eprintln!("> {cmd:?}");
    match cmd {
        Cmd::PowerOn => {
            let _ = tx.send(Ev::Busy("Turning Wi-Fi on".into()));
            match b.power_on() {
                Ok(()) => {
                    let _ = tx.send(Ev::Power(true));
                    refresh(b, tx);
                    scan(b, tx);
                }
                Err(e) => {
                    eprintln!("power on failed: {e}");
                    let _ = tx.send(Ev::Power(b.is_on()));
                    let _ = tx.send(Ev::Message(e));
                }
            }
        }
        Cmd::PowerOff => {
            let _ = tx.send(Ev::Busy("Turning Wi-Fi off".into()));
            b.power_off();
            let _ = tx.send(Ev::Power(false));
            let _ = tx.send(Ev::Networks(Vec::new()));
            let _ = tx.send(Ev::Status(Status::default()));
        }
        Cmd::Scan => scan(b, tx),
        Cmd::Connect(target) => {
            let _ = tx.send(Ev::Busy(format!("Connecting to {}", target.ssid)));
            let msg = match b.connect(&target) {
                Ok(Some(ip)) => format!("Connected! IP {ip}"),
                Ok(None) => "Connected, waiting for an IP address".into(),
                Err(e) => e,
            };
            eprintln!("{msg}");
            let _ = tx.send(Ev::Message(msg));
            refresh(b, tx);
        }
        Cmd::Forget { id, ssid } => {
            let msg = match b.forget(id) {
                Ok(()) => format!("Forgot {ssid}"),
                Err(e) => e,
            };
            let _ = tx.send(Ev::Message(msg));
            refresh(b, tx);
        }
    }
}
