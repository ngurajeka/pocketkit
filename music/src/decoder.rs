//! Opens a music file with Symphonia and hands out decoded samples.

use std::{fs::File, path::Path};

use symphonia::core::{
    audio::SampleBuffer,
    codecs::{DecoderOptions, CODEC_TYPE_NULL},
    errors::Error,
    formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision, StandardTagKey},
    probe::Hint,
    units::{Time, TimeBase},
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    /// Seconds, when the file says.
    pub duration: Option<f64>,
    pub rate: u32,
    pub channels: usize,
    pub kbps: Option<u32>,
}

impl TrackInfo {
    /// "Artist - Title", or just the title, or `None` without tags.
    pub fn display_name(&self) -> Option<String> {
        match (&self.artist, &self.title) {
            (Some(a), Some(t)) => Some(format!("{a} - {t}")),
            (None, Some(t)) => Some(t.clone()),
            _ => None,
        }
    }
}

pub struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    buf: Option<SampleBuffer<f32>>,
    pub info: TrackInfo,
}

fn read_tags(rev: &MetadataRevision, info: &mut TrackInfo) {
    for tag in rev.tags() {
        let value = tag.value.to_string().trim().to_string();
        if value.is_empty() {
            continue;
        }
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) => info.title.get_or_insert(value),
            Some(StandardTagKey::Artist) => info.artist.get_or_insert(value),
            Some(StandardTagKey::AlbumArtist) => {
                if info.artist.is_none() {
                    info.artist = Some(value);
                }
                continue;
            }
            _ => continue,
        };
    }
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("Can't open file: {e}"))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let mut probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
            .map_err(|e| format!("Unsupported file: {e}"))?;

        let track = probed
            .format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or("No audio in this file")?;
        let params = track.codec_params.clone();
        let track_id = track.id;
        let decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .map_err(|e| format!("Unsupported codec: {e}"))?;

        let rate = params.sample_rate.unwrap_or(44100);
        let duration = params.n_frames.map(|n| n as f64 / rate as f64);
        let mut info = TrackInfo {
            rate,
            channels: params.channels.map(|c| c.count()).unwrap_or(2),
            duration,
            kbps: duration.filter(|&d| d > 0.0).map(|d| (size as f64 * 8.0 / d / 1000.0).round() as u32),
            ..TrackInfo::default()
        };
        if let Some(rev) = probed.format.metadata().current() {
            read_tags(rev, &mut info);
        }
        if let Some(mut meta) = probed.metadata.get() {
            if let Some(rev) = meta.skip_to_latest() {
                read_tags(rev, &mut info);
            }
        }
        Ok(Self { format: probed.format, decoder, track_id, time_base: params.time_base, buf: None, info })
    }

    /// The next block of interleaved samples (in the file's own channel layout
    /// and rate), or `None` at the end of the track.
    pub fn next_block(&mut self) -> Option<Result<&[f32], String>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => return None,
                Err(Error::ResetRequired) => return None,
                Err(e) => return Some(Err(format!("Read error: {e}"))),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let needed = decoded.capacity() as u64;
                    if self.buf.as_ref().is_none_or(|b| b.capacity() < needed as usize) {
                        self.buf = Some(SampleBuffer::new(needed, spec));
                    }
                    let buf = self.buf.as_mut().unwrap();
                    buf.copy_interleaved_ref(decoded);
                    self.info.channels = spec.channels.count();
                    return Some(Ok(buf.samples()));
                }
                // A corrupt frame: skip it, like every player does.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Some(Err(format!("Decode error: {e}"))),
            }
        }
    }

    /// Jumps to `secs`; returns where it actually landed.
    pub fn seek(&mut self, secs: f64) -> Result<f64, String> {
        let to = SeekTo::Time { time: Time::from(secs.max(0.0)), track_id: Some(self.track_id) };
        let seeked = self.format.seek(SeekMode::Coarse, to).map_err(|e| format!("Can't seek: {e}"))?;
        self.decoder.reset();
        Ok(match self.time_base {
            Some(tb) => {
                let t = tb.calc_time(seeked.actual_ts);
                t.seconds as f64 + t.frac
            }
            None => secs,
        })
    }
}

/// Writes a 16-bit stereo WAV with a 440 Hz tone (tests, snapshots, demo music).
pub fn write_test_wav(path: &Path, rate: u32, secs: f32) {
    let frames = (rate as f32 * secs) as u32;
    let data_len = frames * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let s = ((i as f32 * 440.0 * 2.0 * std::f32::consts::PI / rate as f32).sin() * 12000.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, out).unwrap();
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_seeks_a_wav() {
        let path = std::env::temp_dir().join(format!("music_dec_{}.wav", std::process::id()));
        write_test_wav(&path, 22050, 2.0);
        let mut d = Decoder::open(&path).unwrap();
        assert_eq!((d.info.rate, d.info.channels), (22050, 2));
        assert!((d.info.duration.unwrap() - 2.0).abs() < 0.01);
        let mut total = 0;
        while let Some(block) = d.next_block() {
            total += block.unwrap().len();
        }
        assert_eq!(total, 22050 * 2 * 2);
        let at = d.seek(1.5).unwrap();
        assert!((at - 1.5).abs() < 0.1, "{at}");
        assert!(d.next_block().is_some());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_non_audio() {
        let path = std::env::temp_dir().join(format!("music_bad_{}.mp3", std::process::id()));
        std::fs::write(&path, b"definitely not audio").unwrap();
        assert!(Decoder::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
