//! Pocket Pet: a tiny virtual pet for the Miyoo Mini+.
//!
//! The game is split so that almost everything is testable on a desktop:
//! - `game`: pure pet logic (stats, actions, save format)
//! - `render`: software renderer drawing into a 320x240 pixel buffer
//! - `app`: glue between input, game logic, rendering and saving
//! - `platform`: desktop window (for development) and device backend (framebuffer + evdev)

pub mod app;
pub mod game;
pub mod input;
pub mod platform;
pub mod render;
