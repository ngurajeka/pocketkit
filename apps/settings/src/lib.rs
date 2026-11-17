//! Settings: MinUI options and our paks' shared settings, on the Miyoo Mini+.
//!
//! - `autostart`: "at startup" options as a marked block in MinUI's auto.sh
//! - `app`: the settings list
//!
//! MinUI's own settings (brightness) and our shared settings file live in
//! `pocketkit_platform::minui_settings` and `pocketkit_core::settings`; the
//! screen-off timer itself is in `pocketkit_core::run_with`, so every app honours it.

pub mod app;
pub mod autostart;
