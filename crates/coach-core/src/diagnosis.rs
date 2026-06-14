//! The deterministic diagnosis object (spec §7, §12.2).
//!
//! The diagnosis engine emits *only* this structured object — never prose. It is
//! consumed by both the cue compiler and the post-event report writer.

use crate::ids::{CarId, LapId, SegmentId, TrackId};
use serde::{Deserialize, Serialize};

/// Corner phase an error is attributed to (spec §7.1). Attribution always picks
/// the earliest phase in the causal chain (spec §7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Entry,
    Mid,
    Exit,
}

/// The error taxonomy (spec §7.1). Serialized in snake_case to match the §12.2
/// example (`"brake_too_early"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    // Entry
    BrakeTooEarly,
    BrakeTooLate,
    BrakeTooHard,
    BrakeTooSoft,
    NoTrailBraking,
    // Mid
    ApexSpeedDeficit,
    EarlyApex,
    LateApex,
    // Exit
    ThrottleTooLate,
    Wheelspin,
}

impl ErrorKind {
    pub fn phase(self) -> Phase {
        use ErrorKind::*;
        match self {
            BrakeTooEarly | BrakeTooLate | BrakeTooHard | BrakeTooSoft | NoTrailBraking => {
                Phase::Entry
            }
            ApexSpeedDeficit | EarlyApex | LateApex => Phase::Mid,
            ThrottleTooLate | Wheelspin => Phase::Exit,
        }
    }
}

/// Magnitude bucket (spec §7.3). Two buckets baked into the template: a small,
/// car-length adjustment vs. a large, marker-board one.
///
/// (The §12.2 *example* writes `"strong"`; the normative rule in §7.3 and the
/// cue object §12.3 use `slight` / `much`, which is what we serialize.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Magnitude {
    /// e.g. brake-point delta 3–8 m → "slightly".
    Slight,
    /// e.g. brake-point delta > 8 m → "much".
    Much,
}

/// What the lap was compared against for this finding (spec §5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasuredAgainst {
    /// The stitched per-segment best.
    SegmentBest,
    /// Fallback to the full personal-best lap's trace (entry-speed guard, §5.2).
    PersonalBestLap,
}

/// Quantitative detail backing a finding (spec §12.2 `detail`). Fields are
/// optional so each error kind populates only what is relevant.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct FindingDetail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brake_point_delta_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_in_delta_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apex_speed_delta_kmh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throttle_on_delta_m: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_offset_m: Option<f64>,
}

/// One attributed time loss (spec §12.2 `findings[]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub segment_id: SegmentId,
    pub segment_label: String,
    pub phase: Phase,
    pub time_lost_ms: i64,
    pub error: ErrorKind,
    pub magnitude: Magnitude,
    /// How many of the last 3 valid laps showed this (segment, phase, error)
    /// (spec §7.3 persistence filter).
    pub persistence_count: u32,
    pub measured_against: MeasuredAgainst,
    pub detail: FindingDetail,
}

/// The full diagnosis for one lap (spec §12.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnosis {
    pub lap_id: LapId,
    pub track_id: TrackId,
    pub car_id: CarId,
    /// Total lap time lost vs. the reference, in ms.
    pub total_delta_ms: i64,
    pub findings: Vec<Finding>,
    /// Segments that moved into the "fixed" set this lap (spec §7.4).
    pub fixed: Vec<SegmentId>,
    /// Segments ranked worst-first by time lost (spec §7.2).
    pub ranked_priority: Vec<SegmentId>,
}
