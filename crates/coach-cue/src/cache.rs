//! RAM/disk WAV cache keyed by the hash of the cue text (spec §8.2).
//!
//! Any cue text not already in the cache is synthesized once and stored; after
//! the first session on a track the hit rate is ~100% and compilation does zero
//! synthesis (spec §8.2). The cache is consulted at plan-compile time on the
//! start/finish straight — never on the trigger path.

use crate::error::Result;
use crate::synth::Synthesizer;
use crate::wav;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// What the compiler needs about a cached clip.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipInfo {
    /// Content hash of the cue text — the `audio_clip_id` in the plan (§12.3).
    pub clip_id: String,
    pub path: PathBuf,
    pub duration_s: f64,
}

/// Disk-backed cache of synthesized cue WAVs.
pub struct WavCache<'a> {
    dir: PathBuf,
    sample_rate: u32,
    synth: &'a dyn Synthesizer,
}

impl<'a> WavCache<'a> {
    pub fn new(dir: impl Into<PathBuf>, sample_rate: u32, synth: &'a dyn Synthesizer) -> Self {
        Self {
            dir: dir.into(),
            sample_rate,
            synth,
        }
    }

    /// Stable content id for a cue text.
    pub fn clip_id(text: &str) -> String {
        let mut h = Sha256::new();
        h.update(text.as_bytes());
        hex::encode(&h.finalize()[..16]) // 128-bit hex is plenty
    }

    fn path_for(&self, clip_id: &str) -> PathBuf {
        self.dir.join(format!("{clip_id}.wav"))
    }

    /// Return the cached clip for `text`, synthesizing + storing it on a miss.
    pub fn get_or_synth(&self, text: &str) -> Result<ClipInfo> {
        let clip_id = Self::clip_id(text);
        let path = self.path_for(&clip_id);
        if !path.exists() {
            std::fs::create_dir_all(&self.dir)?;
            self.synth
                .synthesize_to_wav(text, &path, self.sample_rate)?;
        }
        let duration_s = wav::read_info(&path)?.duration_s();
        Ok(ClipInfo {
            clip_id,
            path,
            duration_s,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::FakeSynthesizer;

    #[test]
    fn synthesizes_once_then_hits_cache() {
        let dir = std::env::temp_dir().join("coach_cache_test_unique");
        let _ = std::fs::remove_dir_all(&dir);
        let synth = FakeSynthesizer::default();
        let cache = WavCache::new(&dir, 22_050, &synth);

        let a = cache.get_or_synth("Turn 5 — brake slightly later").unwrap();
        assert!(a.path.exists());
        assert!(a.duration_s > 0.4);

        // Second call hits the cache → same id and path, no error.
        let b = cache.get_or_synth("Turn 5 — brake slightly later").unwrap();
        assert_eq!(a.clip_id, b.clip_id);
        assert_eq!(a.path, b.path);
    }
}
