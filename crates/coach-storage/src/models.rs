//! Row types mirroring the spec §12.1 tables.

use chrono::{DateTime, Utc};
use coach_core::action_points::ActionPoints;
use coach_core::ids::{CarId, LapId, SegmentId, SessionId, TrackId};
use coach_core::session::SessionType;
use coach_core::trace::LapTrace;

/// `sessions` row (spec §12.1).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRow {
    pub session_id: SessionId,
    pub track_id: TrackId,
    pub car_id: CarId,
    pub session_type: SessionType,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub summary_md_path: Option<String>,
    pub summary_json_path: Option<String>,
}

/// `laps` row (spec §12.1). `trace` is optional in memory so callers can fetch
/// lap metadata without deserializing the BLOB.
#[derive(Debug, Clone, PartialEq)]
pub struct LapRow {
    pub lap_id: LapId,
    pub session_id: SessionId,
    pub track_id: TrackId,
    pub car_id: CarId,
    pub lap_time_ms: i64,
    pub is_valid: bool,
    pub is_in_out_lap: bool,
    pub is_outlier: bool,
    pub recorded_at: DateTime<Utc>,
    pub trace: Option<LapTrace>,
}

/// `segment_traversals` row (spec §12.1) — per-lap behavior metrics that power
/// the §10 summary as a pure query.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentTraversalRow {
    pub lap_id: LapId,
    pub segment_id: SegmentId,
    pub time_ms: i64,
    pub is_valid: bool,
    pub entry_speed_kmh: f64,
    pub brake_point_dist_m: Option<f64>,
    pub turn_in_dist_m: Option<f64>,
    pub apex_speed_kmh: Option<f64>,
    pub min_speed_kmh: Option<f64>,
    pub throttle_on_dist_m: Option<f64>,
}

/// `sector_bests` row (spec §12.1), maintained incrementally.
#[derive(Debug, Clone, PartialEq)]
pub struct SectorBestRow {
    pub track_id: TrackId,
    pub car_id: CarId,
    pub segment_id: SegmentId,
    pub best_time_ms: i64,
    pub source_lap_id: LapId,
    pub entry_speed_kmh: f64,
    pub trace: LapTrace,
    pub time_variance_ms: f64,
}

/// `action_points` row (spec §12.1) wrapping the core [`ActionPoints`].
#[derive(Debug, Clone, PartialEq)]
pub struct ActionPointsRow {
    pub track_id: TrackId,
    pub car_id: CarId,
    pub points: ActionPoints,
}
