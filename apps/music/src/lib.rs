//! Music: a music player for the Miyoo Mini+ with a classic late-90s
//! desktop-player look (LED time display, scrolling title, spectrum analyzer,
//! playlist window). All artwork is drawn in code.
//!
//! - `library`: finding music files, play order (pure)
//! - `decoder`: Symphonia (MP3, FLAC, OGG Vorbis, WAV, AAC/M4A)
//! - `dsp`: resampling, FFT, spectrum bands (pure)
//! - `audio`: outputs (MI_AO on the device, pw-play/paplay/aplay on desktop)
//! - `player`: the playback thread
//! - `skin`, `app`: drawing and controls

pub mod audio;
pub mod decoder;
pub mod dsp;
pub mod library;
pub mod app;
pub mod player;
pub mod skin;
