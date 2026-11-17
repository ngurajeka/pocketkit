//! Settings: MinUI options and our paks' shared settings, on the Miyoo Mini+.
//!
//! - `autostart`: "at startup" options as a marked block in MinUI's auto.sh
//! - `app`: the settings list
//!
//! MinUI's own settings (brightness) and our shared settings file live in
//! `minui_common::{minui, settings}`; the screen-off timer itself is in
//! `minui_common::run`, so every pak using it honours the setting.

pub mod app;
pub mod autostart;
