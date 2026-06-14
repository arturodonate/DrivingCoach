//! Telemetry source abstraction.
//!
//! A [`TelemetrySource`] yields normalized [`TelemetryFrame`]s in order. The live
//! Windows source ([`crate::live`]) polls AC shared memory; the cross-platform
//! [`crate::recording::RecordedSource`] replays a saved session. The offline
//! pipeline and tests are written against the trait, never a concrete source.

use crate::frame::TelemetryFrame;
use serde::{Deserialize, Serialize};

/// Session-static identity, read once per session from the AC `Static` page
/// (spec §4.1). Carried in recordings so replay needs no live game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticInfo {
    /// AC `track` field → `TrackId`.
    pub track: String,
    /// AC `carModel` field → `CarId`.
    pub car_model: String,
    /// AC `trackSPlineLength` — metres covered by one full spline lap.
    pub track_spline_length_m: f64,
}

/// A pull-based source of telemetry frames.
pub trait TelemetrySource {
    /// Return the next frame, or `None` when the source is exhausted (replay) or
    /// has no new sample this poll (live, after `packetId` dedupe).
    fn poll(&mut self) -> Option<TelemetryFrame>;

    /// Session-static info, available once known.
    fn static_info(&self) -> Option<&StaticInfo>;
}
