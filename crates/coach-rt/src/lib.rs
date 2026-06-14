//! # coach-rt
//!
//! The real-time tier (spec §9, build step §13.6). The 60 Hz [`engine::CueEngine`]
//! — trigger matching with live speed correction, the runtime arbiter rules, and
//! crisis mute — is pure arithmetic and fully cross-platform/tested. The
//! [`plan_buffer::PlanBuffer`] implements the double-buffered handover with the
//! deadline fallback (spec §9.4).
//!
//! Only the **audio output device** ([`audio::WasapiSink`], WASAPI via `cpal`)
//! is Windows-specific and is scaffolded for the Windows increment; the rest of
//! the system uses the [`audio::AudioSink`] trait (with [`audio::LogSink`] for
//! the offline replay/acceptance harness).

pub mod audio;
pub mod engine;
pub mod error;
pub mod plan_buffer;

pub use audio::{AudioSink, LogSink};
pub use engine::{CrisisFlags, CueEngine, FiredCue, Tick};
pub use error::{Result, RtError};
pub use plan_buffer::PlanBuffer;
