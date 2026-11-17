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
///
/// A player that exits or stops accepting audio is dropped for good and the
/// next one is tried; while none works, playback is held to real time (silent)
/// instead of racing through the track.
pub struct PipeSink {
    child: Option<(Child, ChildStdin, usize)>,
    /// (program, arguments), in the order to try them.
    players: Vec<(String, Vec<String>)>,
    /// Players that failed; never tried again.
    bad: Vec<bool>,
    /// The player that worked, so restarts (after stop/seek) go straight to it.
    chosen: Option<usize>,
    gave_up: bool,
    pacer: Pacer,
}

pub const PLAYERS: &[(&str, &[&str])] = &[
    // Small buffers, so pause/seek/stop take effect quickly (aplay's default is ~0.5 s).
    ("pw-play", &["--raw", "--rate", "48000", "--channels", "2", "--format", "s16", "--latency", "60ms", "-"]),
    ("paplay", &["--raw", "--rate=48000", "--channels=2", "--format=s16le", "--latency-msec=60"]),
    ("aplay", &["-q", "-t", "raw", "-f", "S16_LE", "-r", "48000", "-c", "2", "-B", "80000", "-"]),
];

/// How long a freshly started player gets to reject our options. Generous,
/// because a cold start from an SD card can take a while.
const PROBE: Duration = Duration::from_millis(400);

impl PipeSink {
    pub fn new() -> Self {
        // muOS: aplay is the one that works (its pw-play is too old for --raw).
        let order = if cfg!(feature = "muos") { [2, 0, 1] } else { [0, 1, 2] };
        let players = order
            .iter()
            .map(|&i| (PLAYERS[i].0.to_string(), PLAYERS[i].1.iter().map(|a| a.to_string()).collect()))
            .collect();
        Self::with_players(players)
    }

    /// Custom player programs (tests).
    pub fn with_players(players: Vec<(String, Vec<String>)>) -> Self {
        let n = players.len();
        Self { child: None, players, bad: vec![false; n], chosen: None, gave_up: false, pacer: Pacer::default() }
    }

    pub fn chosen_player(&self) -> Option<&str> {
        self.chosen.map(|i| self.players[i].0.as_str())
    }

    fn start(&mut self) -> Option<(Child, ChildStdin, usize)> {
        let candidates: Vec<usize> = match self.chosen {
            Some(i) => vec![i],
            None => (0..self.players.len()).filter(|&i| !self.bad[i]).collect(),
        };
        for i in candidates {
            let (cmd, args) = &self.players[i];
            let Ok(mut child) = Command::new(cmd)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                self.bad[i] = true;
                continue;
            };
            // A player that doesn't accept our options usually exits right away.
            if self.chosen.is_none() {
                thread::sleep(PROBE);
                if let Ok(Some(status)) = child.try_wait() {
                    eprintln!("audio: {cmd} exited ({status}), trying the next player");
                    self.bad[i] = true;
                    continue;
                }
                eprintln!("audio: trying {cmd}");
            }
            let stdin = child.stdin.take()?;
            return Some((child, stdin, i));
        }
        None
    }

    fn stop_child(&mut self) {
        if let Some((mut child, stdin, _)) = self.child.take() {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Default for PipeSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for PipeSink {
    fn write(&mut self, pcm: &[i16]) -> Result<(), String> {
        if self.child.is_none() && !self.gave_up {
            self.child = self.start();
            if self.child.is_none() && self.chosen.is_none() {
                self.gave_up = true;
                eprintln!("audio: no working pw-play/paplay/aplay, playing silently");
            }
        }
        if let Some((_, stdin, i)) = &mut self.child {
            let i = *i;
            let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
            if stdin.write_all(&bytes).is_ok() {
                if self.chosen.is_none() {
                    eprintln!("audio: playing through {}", self.players[i].0);
                    self.chosen = Some(i);
                }
                // Don't let the pipe fill up (it holds ~0.3 s): stay just ahead.
                self.pacer.wrote(pcm.len() / CHANNELS, OUT_RATE);
                return Ok(());
            }
            // The player died: never use it again; the next write tries another.
            eprintln!("audio: {} stopped accepting audio, trying the next player", self.players[i].0);
            self.bad[i] = true;
            if self.chosen == Some(i) {
                self.chosen = None;
            }
            self.stop_child();
            self.gave_up = false;
        }
        // No player (yet): keep real time, silently.
        self.pacer.wrote(pcm.len() / CHANNELS, OUT_RATE);
        Ok(())
    }

    fn flush(&mut self) {
        // Killing the player is the only way to drop what it has buffered.
        self.stop_child();
        self.pacer.reset();
    }

    fn latency_frames(&self) -> u64 {
        (AHEAD.as_secs_f64() * OUT_RATE as f64) as u64
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

    fn player(cmd: &str, args: &[&str]) -> (String, Vec<String>) {
        (cmd.to_string(), args.iter().map(|a| a.to_string()).collect())
    }

    #[test]
    fn skips_players_that_fail_early_or_late() {
        // 1: rejects its options at once; 2: looks fine, then dies (a slow
        // cold start); 3: works. 2 s of audio in 20 ms writes.
        let mut sink = PipeSink::with_players(vec![
            player("false", &[]),
            player("sh", &["-c", "sleep 0.6; exit 1"]),
            player("sh", &["-c", "cat > /dev/null"]),
        ]);
        let chunk = vec![0i16; 960 * CHANNELS];
        for _ in 0..100 {
            sink.write(&chunk).unwrap();
        }
        assert_eq!(sink.chosen_player(), Some("sh"));
        assert_eq!(sink.bad, vec![true, true, false]);
    }

    #[test]
    fn without_a_player_playback_keeps_real_time() {
        let mut sink = PipeSink::with_players(vec![player("false", &[])]);
        let start = Instant::now();
        let chunk = vec![0i16; 960 * CHANNELS]; // 20 ms
        for _ in 0..25 {
            sink.write(&chunk).unwrap();
        }
        // 0.5 s of audio must not finish in a blink (it was racing before).
        assert!(start.elapsed() >= Duration::from_millis(350), "{:?}", start.elapsed());
    }

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
