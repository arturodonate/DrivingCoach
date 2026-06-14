//! # coach-cue
//!
//! Cue compilation & TTS (spec §8, §9.1–§9.2, build step §13.5):
//!
//! - [`templates`] — the closed cue-template grammar (spec §8.1).
//! - [`synth`] — [`synth::Synthesizer`] trait with a Piper shell-out impl and a
//!   deterministic fake for tests.
//! - [`cache`] — WAV cache keyed by cue-text hash (spec §8.2).
//! - [`wav`] — minimal mono PCM-16 WAV read/write (for duration + storage).
//! - [`compiler`] — renders a diagnosis into a [`coach_core::CoachingPlan`] with
//!   back-integrated trigger distances and arbiter-resolved overlaps.

pub mod cache;
pub mod compiler;
pub mod error;
pub mod synth;
pub mod templates;
pub mod wav;

pub use cache::{ClipInfo, WavCache};
pub use compiler::{back_integrate_trigger, CueCompiler};
pub use error::{CueError, Result};
pub use synth::{FakeSynthesizer, PiperSynthesizer, Synthesizer};
