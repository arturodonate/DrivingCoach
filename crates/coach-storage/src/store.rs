//! The `Store` trait — the persistence boundary.
//!
//! Keeping persistence behind a trait makes the engine swappable (the spec names
//! DuckDB as an alternative, §11/§12); [`crate::sqlite::SqliteStore`] is the
//! shipped implementation.

use crate::error::Result;
use crate::models::{ActionPointsRow, LapRow, SectorBestRow, SegmentTraversalRow, SessionRow};
use coach_core::ids::{LapId, SessionId, TrackCar, TrackId};
use coach_core::segment::TrackGeometry;

pub trait Store {
    // --- Track geometry (car-independent, once per track) ---
    fn upsert_track_geometry(&self, geom: &TrackGeometry) -> Result<()>;
    fn get_track_geometry(&self, track_id: &TrackId) -> Result<Option<TrackGeometry>>;

    // --- Action points (car-dependent anchors) ---
    fn upsert_action_points(&self, row: &ActionPointsRow) -> Result<()>;
    fn get_action_points(&self, key: &TrackCar) -> Result<Vec<ActionPointsRow>>;

    // --- Sessions ---
    fn upsert_session(&self, row: &SessionRow) -> Result<()>;
    fn get_session(&self, session_id: &SessionId) -> Result<Option<SessionRow>>;

    // --- Laps ---
    fn insert_lap(&self, row: &LapRow) -> Result<()>;
    /// Laps for a session ordered by `recorded_at`. `with_trace` deserializes the
    /// BLOB only when needed.
    fn laps_for_session(&self, session_id: &SessionId, with_trace: bool) -> Result<Vec<LapRow>>;

    // --- Segment traversals (per-lap behavior metrics) ---
    fn insert_segment_traversal(&self, row: &SegmentTraversalRow) -> Result<()>;
    fn segment_traversals_for_session(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<SegmentTraversalRow>>;
    fn segment_traversals_for_lap(&self, lap_id: &LapId) -> Result<Vec<SegmentTraversalRow>>;

    // --- Sector bests (the stitched reference) ---
    fn upsert_sector_best(&self, row: &SectorBestRow) -> Result<()>;
    fn sector_bests(&self, key: &TrackCar) -> Result<Vec<SectorBestRow>>;
}
