//! Audio outputs. Everything is 16-bit interleaved stereo, at the rate the
//! output reports (`Sink::rate`, normally `OUT_RATE`).

use std::{
    io::Write,
    process::{Child, ChildStdin, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// 48 kHz: the rate the Mini's audio server runs at, so nothing resamples twice.
pub const OUT_RATE: u32 = 48000;
pub const CHANNELS: usize = 2;
/// How far ahead of real time a paced output may run (its "buffer").
const AHEAD: Duration = Duration::from_millis(120);

pub trait Sink: Send {
    /// Plays `pcm`; blocks while the output is full, which paces playback.
    fn write(&mut self, pcm: &[i16]) -> Result<(), String>;
    /// Drops audio queued but not yet heard (stop, seek, track change).
    fn flush(&mut self) {}
    /// Frames written but not heard yet (for the time display).
    fn latency_frames(&self) -> u64 {
        0
    }
    /// Output sample rate.
    fn rate(&self) -> u32 {
        OUT_RATE
    }
}

/// Keeps writes from running more than `AHEAD` in front of the wall clock, for
/// outputs that don't block (or might not).
#[derive(Debug, Default)]
pub struct Pacer {
    clock: Option<(Instant, u64)>,
}

impl Pacer {
    pub fn wrote(&mut self, frames: usize, rate: u32) {
        let (start, total) = self.clock.get_or_insert((Instant::now(), 0));
        *total += frames as u64;
        let due = Duration::from_secs_f64(*total as f64 / rate as f64);
        if let Some(wait) = due.checked_sub(start.elapsed() + AHEAD) {
            thread::sleep(wait);
        }
        // Fell far behind (e.g. a slow SD card read)? Restart the clock instead
        // of rushing to catch up.
        if start.elapsed() > due + Duration::from_millis(500) {
            self.clock = None;
        }
    }

    pub fn reset(&mut self) {
        self.clock = None;
    }
}

/// Discards audio. In real-time mode it takes as long as the audio would play.
pub struct NullSink {
    realtime: bool,
    pacer: Pacer,
}

impl NullSink {
    pub fn new(realtime: bool) -> Self {
        Self { realtime, pacer: Pacer::default() }
    }
}

impl Sink for NullSink {
    fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
        if self.realtime {
            self.pacer.wrote(pcm.len() / CHANNELS, OUT_RATE);
        }
        Ok(())
    }

    fn flush(&mut self) {
        self.pacer.reset();
    }
}

/// Pipes raw PCM into the first player program that works: PipeWire's pw-play,
/// PulseAudio's paplay or ALSA's aplay. Used for the desktop preview and on
/// muOS (whose older pw-play has no `--raw`, but whose ALSA default device
/// goes through PipeWire, so aplay follows the system volume).
pub struct PipeSink {
    child: Option<(Child, ChildStdin)>,
    failed: bool,
    /// The player that worked, so restarts (after stop/seek) don't probe again.
    chosen: Option<usize>,
}

const PLAYERS: &[(&str, &[&str])] = &[
    ("pw-play", &["--raw", "--rate", "48000", "--channels", "2", "--format", "s16", "-"]),
    ("paplay", &["--raw", "--rate=48000", "--channels=2", "--format=s16le"]),
    ("aplay", &["-q", "-t", "raw", "-f", "S16_LE", "-r", "48000", "-c", "2", "-"]),
];

impl PipeSink {
    pub fn new() -> Self {
        Self { child: None, failed: false, chosen: None }
    }

    fn start(&mut self) -> Option<(Child, ChildStdin)> {
        let order: Vec<usize> = match self.chosen {
            Some(i) => vec![i],
            None => (0..PLAYERS.len()).collect(),
        };
        for i in order {
            let (cmd, args) = PLAYERS[i];
            let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            // A player that doesn't accept our options exits right away.
            if self.chosen.is_none() {
                thread::sleep(Duration::from_millis(150));
                if let Ok(Some(status)) = child.try_wait() {
                    eprintln!("audio: {cmd} exited ({status}), trying the next player");
                    continue;
                }
            }
            let stdin = child.stdin.take()?;
            if self.chosen.is_none() {
                eprintln!("audio: playing through {cmd}");
            }
            self.chosen = Some(i);
            return Some((child, stdin));
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
            self.child = self.start();
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

/// The Miyoo Mini's outputs (`ao_shim.c`).
#[cfg(feature = "minui")]
pub mod device {
    use super::{NullSink, Pacer, Sink, AHEAD, CHANNELS, OUT_RATE};

    extern "C" {
        fn dsp_open(rate: u32, channels: u32) -> i32;
        fn dsp_write(data: *const i16, bytes: u32) -> i32;
        fn dsp_reset();
        fn dsp_close();
        fn ao_open(rate: u32, channels: u32, frame_samples: u32) -> i32;
        fn ao_write(data: *const i16, bytes: u32) -> i32;
        fn ao_clear();
        fn ao_close();
    }

    /// The best output available: `/dev/dsp` (MinUI's audio server, through the
    /// libpadsp.so it preloads into every app), else MI_AO directly, else silence.
    pub fn open() -> Box<dyn Sink> {
        let preload = std::env::var("LD_PRELOAD").unwrap_or_default();
        eprintln!("audio: LD_PRELOAD={preload:?}");
        match DspSink::open() {
            Ok(s) => {
                eprintln!("audio: /dev/dsp at {} Hz", s.rate);
                return Box::new(s);
            }
            Err(e) => eprintln!("audio: {e}"),
        }
        match MiAoSink::open() {
            Ok(s) => {
                eprintln!("audio: MI_AO at {OUT_RATE} Hz");
                Box::new(s)
            }
            Err(e) => {
                eprintln!("audio: {e}; playing silently");
                Box::new(NullSink::new(true))
            }
        }
    }

    /// OSS `/dev/dsp`. With MinUI's libpadsp.so preloaded, the calls in
    /// `ao_shim.c` go to the firmware's audioserver, which owns the hardware and
    /// applies the system volume.
    pub struct DspSink {
        rate: u32,
        pacer: Pacer,
        bytes: Vec<u8>,
    }

    impl DspSink {
        pub fn open() -> Result<Self, String> {
            let r = unsafe { dsp_open(OUT_RATE, CHANNELS as u32) };
            if r <= 0 {
                return Err(format!("can't open /dev/dsp (step {})", -r));
            }
            Ok(Self { rate: r as u32, pacer: Pacer::default(), bytes: Vec::new() })
        }
    }

    impl Sink for DspSink {
        fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
            self.bytes.clear();
            self.bytes.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
            if unsafe { dsp_write(self.bytes.as_ptr().cast(), self.bytes.len() as u32) } != 0 {
                return Err("Audio output failed (/dev/dsp write)".into());
            }
            self.pacer.wrote(pcm.len() / CHANNELS, self.rate);
            Ok(())
        }

        fn flush(&mut self) {
            unsafe { dsp_reset() };
            self.pacer.reset();
        }

        fn latency_frames(&self) -> u64 {
            (AHEAD.as_secs_f64() * self.rate as f64) as u64
        }

        fn rate(&self) -> u32 {
            self.rate
        }
    }

    impl Drop for DspSink {
        fn drop(&mut self) {
            unsafe { dsp_close() };
        }
    }

    /// Frames per hardware buffer; writes are sent in pieces of exactly this size.
    const FRAME: usize = 1024;

    /// The vendor MI_AO API directly. Only used when there's no `/dev/dsp`
    /// (no audio server), since it would fight the server for the hardware.
    pub struct MiAoSink {
        pending: Vec<i16>,
    }

    impl MiAoSink {
        pub fn open() -> Result<Self, String> {
            let r = unsafe { ao_open(OUT_RATE, CHANNELS as u32, FRAME as u32) };
            if r != 0 {
                return Err(format!("can't open MI_AO (step {})", -r));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacer_keeps_real_time() {
        let mut p = Pacer::default();
        let start = Instant::now();
        // 0.4 s of audio in 20 ms pieces: should take ~0.4 s - AHEAD.
        for _ in 0..20 {
            p.wrote(960, 48000);
        }
        let took = start.elapsed();
        assert!(took >= Duration::from_millis(250) && took < Duration::from_millis(400), "{took:?}");
    }
}
