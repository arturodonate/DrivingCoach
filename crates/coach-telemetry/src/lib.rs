//! # coach-telemetry
//!
//! Telemetry ingestion for Assetto Corsa (spec §4, build step §13.1):
//!
//! - [`frame::TelemetryFrame`] — the normalized, cross-platform sample every
//!   other crate consumes.
//! - [`source::TelemetrySource`] — pull-based source trait, with a live Windows
//!   AC reader ([`live::LiveAcSource`], `cfg(windows)`) and a cross-platform
//!   [`recording::RecordedSource`] for replay.
//! - [`integrity::IntegrityMonitor`] — lap-boundary, teleport, spin, staleness,
//!   and monotonic-position checks (spec §4.3), built in from day one.
//! - [`recorder::LapRecorder`] — segments a frame stream into [`recorder::RawLap`]s.
//!
//! The raw AC `#[repr(C)]` pages live in [`ac_shared`] (Windows-only).

pub mod error;
pub mod frame;
pub mod integrity;
pub mod recorder;
pub mod recording;
pub mod source;

#[cfg(windows)]
pub mod ac_shared;
#[cfg(windows)]
pub mod live;

pub use error::TelemetryError;
pub use frame::{AcStatus, TelemetryFrame};
pub use integrity::{lap_distance_drift, FrameAssessment, IntegrityMonitor};
pub use recorder::{LapRecorder, RawLap};
pub use recording::{RecordedSource, Recording};
pub use source::{StaticInfo, TelemetrySource};
