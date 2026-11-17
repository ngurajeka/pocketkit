//! Web Browser: a tiny text-mode browser and file downloader for the Miyoo Mini+.
//!
//! Like pocket-pet, almost everything is testable on a desktop:
//! - `html`: HTML -> text + links, and word-wrapping (pure)
//! - `net`: HTTPS fetching/downloading on a background thread, URL and filename helpers
//! - `app`: screens, navigation, history, drawing
//!
//! Screen, buttons, on-screen keyboard and platform backends come from `../common`.

pub mod app;
pub mod html;
pub mod net;

pub use pocketkit_core::{input, keyboard, platform, render};
