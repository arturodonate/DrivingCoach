//! Text-to-speech behind a trait (spec §8.2, §11).
//!
//! Synthesis is off the hot path and cached after the first session, so the
//! integration is low-stakes (spec §11). The default [`PiperSynthesizer`] shells
//! out to the `piper` binary; an `ort`-based in-process loader could replace it
//! behind this same trait. [`FakeSynthesizer`] produces deterministic silent
//! clips so the compiler and cache are testable without Piper installed.

use crate::error::{CueError, Result};
use crate::wav::write_pcm16_mono;
use std::path::Path;
use std::process::{Command, Stdio};

/// Synthesize a closed-vocabulary cue phrase to a mono WAV file.
pub trait Synthesizer {
    fn synthesize_to_wav(&self, text: &str, out_path: &Path, sample_rate: u32) -> Result<()>;
}

/// Shells out to the Piper CLI: `piper -m <model> -f <out.wav>`, text on stdin.
pub struct PiperSynthesizer {
    pub binary: String,
    pub model_path: String,
}

impl PiperSynthesizer {
    pub fn new(binary: impl Into<String>, model_path: impl Into<String>) -> Self {
        Self {
            binary: binary.into(),
            model_path: model_path.into(),
        }
    }
}

impl Synthesizer for PiperSynthesizer {
    fn synthesize_to_wav(&self, text: &str, out_path: &Path, _sample_rate: u32) -> Result<()> {
        use std::io::Write;
        let mut child = Command::new(&self.binary)
            .args(["-m", &self.model_path, "-f"])
            .arg(out_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CueError::Synthesis(format!("spawning {}: {e}", self.binary)))?;
        child
            .stdin
            .take()
            .ok_or_else(|| CueError::Synthesis("no stdin pipe".into()))?
            .write_all(text.as_bytes())?;
        let out = child
            .wait_with_output()
            .map_err(|e| CueError::Synthesis(e.to_string()))?;
        if !out.status.success() {
            return Err(CueError::Synthesis(format!(
                "piper exited {}: {}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }
}

/// Deterministic stand-in: emits silence whose duration scales with text length
/// (≈ a real cue), so trigger math and caching are exercised offline.
pub struct FakeSynthesizer {
    pub chars_per_second: f64,
}

impl Default for FakeSynthesizer {
    fn default() -> Self {
        Self {
            chars_per_second: 14.0,
        }
    }
}

impl Synthesizer for FakeSynthesizer {
    fn synthesize_to_wav(&self, text: &str, out_path: &Path, sample_rate: u32) -> Result<()> {
        let secs = (text.chars().count() as f64 / self.chars_per_second).max(0.4);
        let n = (secs * sample_rate as f64).round() as usize;
        write_pcm16_mono(out_path, &vec![0i16; n], sample_rate)
    }
}
