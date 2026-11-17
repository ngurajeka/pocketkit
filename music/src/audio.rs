//! Audio outputs. Everything is 16-bit interleaved stereo at `OUT_RATE`.

use std::{
    io::Write,
    process::{Child, ChildStdin, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const OUT_RATE: u32 = 44100;
pub const CHANNELS: usize = 2;

pub trait Sink: Send {
    /// Plays `pcm`; blocks while the output is full, which paces playback.
    fn write(&mut self, pcm: &[i16]) -> Result<(), String>;
    /// Drops audio queued but not yet heard (stop, seek, track change).
    fn flush(&mut self) {}
    /// Frames written but not heard yet (for the time display).
    fn latency_frames(&self) -> u64 {
        0
    }
}

/// Discards audio. In real-time mode it sleeps as long as the audio would play.
pub struct NullSink {
    realtime: bool,
    clock: Option<(Instant, u64)>,
}

impl NullSink {
    pub fn new(realtime: bool) -> Self {
        Self { realtime, clock: None }
    }
}

impl Sink for NullSink {
    fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
        if self.realtime {
            let (start, frames) = self.clock.get_or_insert((Instant::now(), 0));
            *frames += (pcm.len() / CHANNELS) as u64;
            let due = Duration::from_secs_f64(*frames as f64 / OUT_RATE as f64);
            // Stay ~100 ms ahead, like a real sound card buffer.
            if let Some(wait) = due.checked_sub(start.elapsed() + Duration::from_millis(100)) {
                thread::sleep(wait);
            }
        }
        Ok(())
    }

    fn flush(&mut self) {
        self.clock = None;
    }
}

/// Desktop preview: pipes raw PCM into the first player program found
/// (PipeWire's pw-play, PulseAudio's paplay or ALSA's aplay).
pub struct PipeSink {
    child: Option<(Child, ChildStdin)>,
    failed: bool,
}

const PLAYERS: &[(&str, &[&str])] = &[
    ("pw-play", &["--raw", "--rate", "44100", "--channels", "2", "--format", "s16", "-"]),
    ("paplay", &["--raw", "--rate=44100", "--channels=2", "--format=s16le"]),
    ("aplay", &["-q", "-t", "raw", "-f", "S16_LE", "-r", "44100", "-c", "2", "-"]),
];

impl PipeSink {
    pub fn new() -> Self {
        Self { child: None, failed: false }
    }

    fn start() -> Option<(Child, ChildStdin)> {
        for (cmd, args) in PLAYERS {
            let spawned = Command::new(cmd)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            if let Ok(mut child) = spawned {
                let stdin = child.stdin.take()?;
                eprintln!("audio: playing through {cmd}");
                return Some((child, stdin));
            }
        }
        None
    }
}

impl Default for PipeSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for PipeSink {
    fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
        if self.child.is_none() && !self.failed {
            self.child = Self::start();
            if self.child.is_none() {
                self.failed = true;
                eprintln!("audio: no pw-play/paplay/aplay found, playing silently");
            }
        }
        match &mut self.child {
            Some((_, stdin)) => {
                let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
                if stdin.write_all(&bytes).is_err() {
                    self.flush();
                }
                Ok(())
            }
            // Keep time without sound.
            None => {
                thread::sleep(Duration::from_secs_f64(pcm.len() as f64 / CHANNELS as f64 / OUT_RATE as f64));
                Ok(())
            }
        }
    }

    fn flush(&mut self) {
        // Killing the player is the only way to drop what it has buffered.
        if let Some((mut child, stdin)) = self.child.take() {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for PipeSink {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The Miyoo Mini's audio output, through the vendor MI_AO library (`ao_shim.c`).
#[cfg(feature = "device")]
pub mod miao {
    use super::{Sink, CHANNELS, OUT_RATE};

    extern "C" {
        fn ao_open(rate: u32, channels: u32, frame_samples: u32) -> i32;
        fn ao_write(data: *const i16, bytes: u32) -> i32;
        fn ao_clear();
        fn ao_close();
    }

    /// Frames per hardware buffer; writes are sent in pieces of exactly this size.
    const FRAME: usize = 1024;

    pub struct MiAoSink {
        pending: Vec<i16>,
    }

    impl MiAoSink {
        pub fn open() -> Result<Self, String> {
            let r = unsafe { ao_open(OUT_RATE, CHANNELS as u32, FRAME as u32) };
            if r != 0 {
                return Err(format!("Can't open audio output (MI_AO error {r})"));
            }
            Ok(Self { pending: Vec::with_capacity(FRAME * CHANNELS * 2) })
        }
    }

    impl Sink for MiAoSink {
        fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
            self.pending.extend_from_slice(pcm);
            let chunk = FRAME * CHANNELS;
            let mut sent = 0;
            while self.pending.len() - sent >= chunk {
                let part = &self.pending[sent..sent + chunk];
                let r = unsafe { ao_write(part.as_ptr(), (chunk * 2) as u32) };
                if r != 0 {
                    return Err(format!("Audio output failed (MI_AO error {r})"));
                }
                sent += chunk;
            }
            self.pending.drain(..sent);
            Ok(())
        }

        fn flush(&mut self) {
            self.pending.clear();
            unsafe { ao_clear() };
        }

        fn latency_frames(&self) -> u64 {
            // 6 hardware buffers (u32FrmNum in ao_shim.c), about half full on average.
            (FRAME * 3) as u64
        }
    }

    impl Drop for MiAoSink {
        fn drop(&mut self) {
            unsafe { ao_close() };
        }
    }
}
