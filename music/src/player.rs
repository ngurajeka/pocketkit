//! The playback thread: decodes, converts to the output format, feeds the sink,
//! and shares position and the latest samples (for the analyzer) with the UI.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    thread,
};

use crate::{
    audio::{Sink, CHANNELS, OUT_RATE},
    decoder::{Decoder, TrackInfo},
    dsp::{self, Resampler, FFT_SIZE},
};

#[derive(Debug)]
pub enum Cmd {
    /// Open and start playing (from `start` seconds).
    Load { path: PathBuf, start: f64 },
    Resume,
    Pause,
    Stop,
    /// Absolute position in seconds.
    Seek(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Loaded(TrackInfo),
    /// The track played to the end.
    Ended,
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum State {
    Stopped = 0,
    Playing = 1,
    Paused = 2,
}

/// Read by the UI every frame.
pub struct Shared {
    state: AtomicU8,
    pos_ms: AtomicU64,
    /// 0-100.
    volume: AtomicU32,
    /// The most recent mono samples sent to the output.
    vis: Mutex<Vec<f32>>,
}

impl Shared {
    pub fn state(&self) -> State {
        match self.state.load(Ordering::Relaxed) {
            1 => State::Playing,
            2 => State::Paused,
            _ => State::Stopped,
        }
    }

    pub fn position(&self) -> f64 {
        self.pos_ms.load(Ordering::Relaxed) as f64 / 1000.0
    }

    pub fn volume(&self) -> u32 {
        self.volume.load(Ordering::Relaxed)
    }

    pub fn set_volume(&self, v: u32) {
        self.volume.store(v.min(100), Ordering::Relaxed);
    }

    pub fn samples(&self) -> Vec<f32> {
        self.vis.lock().map(|v| v.clone()).unwrap_or_default()
    }
}

pub struct Player {
    tx: Sender<Cmd>,
    pub events: Receiver<Event>,
    pub shared: Arc<Shared>,
}

impl Player {
    pub fn spawn(sink: Box<dyn Sink>, volume: u32) -> Self {
        let (tx, rx) = mpsc::channel();
        let (ev_tx, events) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: AtomicU8::new(State::Stopped as u8),
            pos_ms: AtomicU64::new(0),
            volume: AtomicU32::new(volume.min(100)),
            vis: Mutex::new(Vec::new()),
        });
        let s = shared.clone();
        thread::spawn(move || Engine::new(sink, s, ev_tx).run(rx));
        Self { tx, events, shared }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

struct Engine {
    sink: Box<dyn Sink>,
    shared: Arc<Shared>,
    events: Sender<Event>,
    decoder: Option<Decoder>,
    resampler: Resampler,
    playing: bool,
    /// Position of the decoder, in seconds.
    pos: f64,
    stereo: Vec<f32>,
    resampled: Vec<f32>,
    pcm: Vec<i16>,
}

impl Engine {
    fn new(sink: Box<dyn Sink>, shared: Arc<Shared>, events: Sender<Event>) -> Self {
        Self {
            sink,
            shared,
            events,
            decoder: None,
            resampler: Resampler::new(OUT_RATE, OUT_RATE),
            playing: false,
            pos: 0.0,
            stereo: Vec::new(),
            resampled: Vec::new(),
            pcm: Vec::new(),
        }
    }

    fn publish(&self) {
        let state = match (&self.decoder, self.playing) {
            (None, _) => State::Stopped,
            (Some(_), true) => State::Playing,
            (Some(_), false) => State::Paused,
        };
        self.shared.state.store(state as u8, Ordering::Relaxed);
        let heard = self.pos - self.sink.latency_frames() as f64 / OUT_RATE as f64;
        self.shared.pos_ms.store((heard.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
        if state == State::Stopped {
            if let Ok(mut v) = self.shared.vis.lock() {
                v.clear();
            }
        }
    }

    fn stop(&mut self) {
        self.decoder = None;
        self.playing = false;
        self.pos = 0.0;
        self.sink.flush();
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Load { path, start } => {
                self.stop();
                match Decoder::open(&path) {
                    Ok(mut d) => {
                        self.resampler = Resampler::new(d.info.rate, OUT_RATE);
                        if start > 0.0 {
                            self.pos = d.seek(start).unwrap_or(0.0);
                        }
                        let _ = self.events.send(Event::Loaded(d.info.clone()));
                        self.decoder = Some(d);
                        self.playing = true;
                    }
                    Err(e) => {
                        let _ = self.events.send(Event::Failed(e));
                    }
                }
            }
            Cmd::Resume => self.playing = self.decoder.is_some(),
            Cmd::Pause => self.playing = false,
            Cmd::Stop => self.stop(),
            Cmd::Seek(secs) => {
                if let Some(d) = &mut self.decoder {
                    let limit = d.info.duration.map_or(secs, |len| secs.min((len - 0.5).max(0.0)));
                    match d.seek(limit) {
                        Ok(at) => {
                            self.pos = at;
                            self.resampler.reset();
                            self.sink.flush();
                        }
                        Err(e) => eprintln!("{e}"),
                    }
                }
            }
        }
        self.publish();
    }

    /// Decodes and plays one block. Returns false when the track is over.
    fn play_block(&mut self) -> bool {
        let Some(d) = &mut self.decoder else { return false };
        let channels = d.info.channels;
        let rate = d.info.rate;
        let block = match d.next_block() {
            None => return false,
            Some(Err(e)) => {
                let _ = self.events.send(Event::Failed(e));
                return false;
            }
            Some(Ok(b)) => b,
        };
        self.pos += (block.len() / channels.max(1)) as f64 / rate as f64;
        self.stereo.clear();
        dsp::to_stereo(block, channels, &mut self.stereo);
        self.resampled.clear();
        self.resampler.process(&self.stereo, &mut self.resampled);
        self.pcm.clear();
        let volume = self.shared.volume() as f32 / 100.0;
        dsp::to_i16(&self.resampled, volume, &mut self.pcm);

        if let Ok(mut vis) = self.shared.vis.lock() {
            vis.extend(self.resampled.as_chunks::<CHANNELS>().0.iter().map(|f| (f[0] + f[1]) * 0.5));
            let excess = vis.len().saturating_sub(FFT_SIZE);
            vis.drain(..excess);
        }
        if let Err(e) = self.sink.write(&self.pcm) {
            let _ = self.events.send(Event::Failed(e));
            return false;
        }
        true
    }

    fn run(mut self, rx: Receiver<Cmd>) {
        loop {
            // Idle: wait for a command. Playing: check without blocking.
            let cmd = if self.playing {
                match rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => return,
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => return,
                }
            };
            if let Some(cmd) = cmd {
                self.handle(cmd);
                continue;
            }
            if !self.play_block() {
                let finished = self.decoder.is_some();
                self.decoder = None;
                self.playing = false;
                self.pos = 0.0;
                if finished {
                    let _ = self.events.send(Event::Ended);
                }
            }
            self.publish();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{audio::NullSink, decoder::write_test_wav as write_wav};

    #[test]
    fn plays_a_file_to_the_end_and_reports() {
        let path = std::env::temp_dir().join(format!("music_player_{}.wav", std::process::id()));
        write_wav(&path, 48000, 1.0);
        let p = Player::spawn(Box::new(NullSink::new(false)), 80);
        p.send(Cmd::Load { path: path.clone(), start: 0.0 });
        let first = p.events.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(first, Event::Loaded(ref i) if i.rate == 48000));
        assert_eq!(p.events.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Ended);
        assert_eq!(p.shared.state(), State::Stopped);

        // Seek near the end (of a longer file), pause, resume.
        write_wav(&path, 48000, 3.0);
        let p = Player::spawn(Box::new(NullSink::new(true)), 80);
        p.send(Cmd::Load { path: path.clone(), start: 0.0 });
        p.events.recv_timeout(Duration::from_secs(5)).unwrap();
        p.send(Cmd::Pause);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(p.shared.state(), State::Paused);
        p.send(Cmd::Seek(2.6));
        std::thread::sleep(Duration::from_millis(50));
        // Seeks stop half a second before the end.
        assert!((p.shared.position() - 2.5).abs() < 0.05, "{}", p.shared.position());
        p.send(Cmd::Resume);
        assert_eq!(p.events.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Ended);
        assert!(!p.shared.samples().is_empty() || p.shared.state() == State::Stopped);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn bad_file_fails() {
        let p = Player::spawn(Box::new(NullSink::new(false)), 80);
        p.send(Cmd::Load { path: "/nonexistent.mp3".into(), start: 0.0 });
        assert!(matches!(p.events.recv_timeout(Duration::from_secs(5)).unwrap(), Event::Failed(_)));
    }
}
