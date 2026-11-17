//! Wi-Fi: pick a network on screen and connect to it, on the Miyoo Mini+.
//!
//! - `wpa`: wpa_supplicant control-socket client and reply parsers (pure, tested)
//! - `backend`: what the UI needs from the hardware, plus a simulated `Fake`
//! - `device`: the real backend (driver, radio power, wpa_supplicant, DHCP)
//! - `worker`: runs the slow backend calls on a background thread
//! - `app`: the network list and password screens
//!
//! Screen, buttons and the on-screen keyboard come from `../common`.

pub mod app;
pub mod backend;
pub mod device;
pub mod worker;
pub mod wpa;
