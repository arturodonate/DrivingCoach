//! Minimal mono PCM-16 WAV read/write.
//!
//! Cues are mono 22.05 kHz clips (spec §8.2). We only need to (a) write the
//! deterministic test/synth output and (b) read a clip's *duration* to feed the
//! trigger back-integration (spec §9.1). Anything fancier (resampling, float
//! formats) is out of scope — Piper already emits mono 16-bit PCM.

use crate::error::{CueError, Result};
use std::path::Path;

/// Write mono 16-bit PCM samples to a canonical 44-byte-header WAV.
pub fn write_pcm16_mono(path: impl AsRef<Path>, samples: &[i16], sample_rate: u32) -> Result<()> {
    let mut buf = Vec::with_capacity(44 + samples.len() * 2);
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * 2;

    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_len).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, buf)?;
    Ok(())
}

/// WAV header essentials.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WavInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub num_frames: u64,
}

impl WavInfo {
    pub fn duration_s(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.num_frames as f64 / self.sample_rate as f64
        }
    }
}

/// Read enough of a WAV to compute its duration. Scans chunks for `fmt ` + `data`.
pub fn read_info(path: impl AsRef<Path>) -> Result<WavInfo> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(CueError::Wav("not a RIFF/WAVE file".into()));
    }
    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    let mut data_len = 0u32;
    let mut saw_fmt = false;
    let mut saw_data = false;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body = pos + 8;
        match id {
            b"fmt " if body + 16 <= bytes.len() => {
                channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
                sample_rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap());
                bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
                saw_fmt = true;
            }
            b"data" => {
                data_len = size as u32;
                saw_data = true;
            }
            _ => {}
        }
        pos = body + size + (size & 1); // chunks are word-aligned
    }
    if !saw_fmt || !saw_data {
        return Err(CueError::Wav("missing fmt/data chunk".into()));
    }
    let bytes_per_frame = (channels as u32 * (bits as u32 / 8)).max(1);
    Ok(WavInfo {
        sample_rate,
        channels,
        bits_per_sample: bits,
        num_frames: (data_len / bytes_per_frame) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_duration() {
        let dir = std::env::temp_dir().join("coach_wav_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.wav");
        let sr = 22_050;
        let samples = vec![0i16; sr as usize]; // exactly 1 second
        write_pcm16_mono(&path, &samples, sr).unwrap();
        let info = read_info(&path).unwrap();
        assert_eq!(info.sample_rate, sr);
        assert_eq!(info.channels, 1);
        assert!((info.duration_s() - 1.0).abs() < 1e-6);
    }
}
