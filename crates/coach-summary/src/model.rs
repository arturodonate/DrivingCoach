//! Serializable post-event summary model (spec §10).
//!
//! This is the machine-readable `.json` artifact (spec §10) — a later LLM prose
//! debrief or cross-session trend view consumes it without re-parsing traces.

use coach_core::ids::SegmentId;
use coach_core::session::SessionType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Trend {
    Improving,
    Plateaued,
    Regressing,
}

impl Trend {
    pub fn label(self) -> &'static str {
        match self {
            Trend::Improving => "improving",
            Trend::Plateaued => "plateaued",
            Trend::Regressing => "regressing",
        }
    }
}

/// Pace / trajectory block (spec §10.1a).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pace {
    pub first_ms: i64,
    pub best_ms: i64,
    pub last_ms: i64,
    pub median_ms: i64,
    /// best − first (signed; negative = faster).
    pub best_minus_first_ms: i64,
    /// last − first (signed; negative = faster).
    pub last_minus_first_ms: i64,
    pub trend: Trend,
    pub theoretical_best_ms: Option<i64>,
}

/// One behavior change linked to its segment-time movement (spec §10.1b).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorChange {
    pub segment_id: SegmentId,
    pub segment_label: String,
    pub metric: String,
    /// Signed change in the metric, last third − first third.
    pub metric_delta: f64,
    pub metric_unit: String,
    /// Concurrent change in the segment time (ms), last third − first third.
    pub segment_time_delta_ms: i64,
    /// "worked, keep it" / "cost time, revert" / "steady".
    pub verdict: String,
    /// Rendered associative sentence (spec §10.1b language).
    pub text: String,
}

/// Coaching response block (spec §10.1c).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coaching {
    pub cued: Vec<LabeledSegment>,
    pub fixed: Vec<LabeledSegment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabeledSegment {
    pub segment_id: SegmentId,
    pub label: String,
}

/// Consistency block (spec §10.1d).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Consistency {
    pub per_segment: Vec<SegmentVariance>,
    pub most_consistent: Option<SegmentVariance>,
    pub least_consistent: Option<SegmentVariance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentVariance {
    pub segment_id: SegmentId,
    pub label: String,
    pub variance_ms: f64,
}

/// Raw per-lap time, with exclusion flags (nothing hidden, spec §10.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LapTimeEntry {
    pub lap_number: usize,
    pub time_ms: i64,
    pub valid: bool,
    pub in_out_lap: bool,
    pub outlier: bool,
}

/// The full post-event summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub track: String,
    pub car: String,
    pub session_type: SessionType,
    pub clean_lap_count: usize,
    pub in_out_excluded: usize,
    pub outlier_excluded: usize,
    pub pace: Pace,
    pub behavior_changes: Vec<BehaviorChange>,
    pub coaching: Coaching,
    pub consistency: Consistency,
    pub all_laps: Vec<LapTimeEntry>,
    /// Race sessions print the fuel/tyre confounder caveat (spec §10.2).
    pub race_caveat: bool,
    pub generated_at: String,
}
