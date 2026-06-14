//! Audio output (spec §9.5).
//!
//! A dedicated audio thread owns the output device and plays RAM-resident WAV
//! clips on request. The device backend (WASAPI shared mode via `cpal`) is
//! Windows-only; the [`AudioSink`] trait keeps the engine decoupled from it, and
//! [`LogSink`] records what *would* play so the replay/acceptance harness can
//! verify cue onset timing without a sound card.
//!
//! Pre-rendered audio is handed to the output thread over a lock-free SPSC ring
//! (`rtrb`), never through a lock the tick could block on (spec §11). Wiring the
//! ring + `cpal` stream is the remaining Windows-increment work.

/// Something that can play a cached cue clip by id.
pub trait AudioSink {
    /// Begin playing the clip; returns immediately (non-blocking).
    fn play(&mut self, clip_id: &str);
}

/// Records play requests with timestamps. Used by tests and the replay harness
/// to check cue-onset timing against trigger timestamps (spec §14).
#[derive(Debug, Default)]
pub struct LogSink {
    pub played: Vec<String>,
}

impl AudioSink for LogSink {
    fn play(&mut self, clip_id: &str) {
        self.played.push(clip_id.to_string());
    }
}

/// Windows WASAPI output via `cpal` + a lock-free SPSC ring (spec §9.5, §11).
///
/// Scaffolded: the device is opened on the deploy box; this is where the
/// Windows-increment work lands. The trait above lets the rest of the system be
/// built and tested without it.
#[cfg(windows)]
pub struct WasapiSink {
    // stream: cpal::Stream, ring producer: rtrb::Producer<...>, RAM clip table …
}

#[cfg(windows)]
impl WasapiSink {
    pub fn new() -> crate::error::Result<Self> {
        Err(crate::error::RtError::NotImplemented(
            "WASAPI audio sink lands in the Windows increment",
        ))
    }
}
