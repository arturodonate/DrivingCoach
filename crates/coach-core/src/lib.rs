//! # coach-core
//!
//! Pure domain types shared across the DrivingCoach workspace: identifiers, the
//! distance grid, lap traces, track segmentation, action points, the
//! deterministic diagnosis object (spec §12.2), the compiled cue/plan objects
//! (spec §12.3), session types, and the central [`Config`].
//!
//! This crate has no I/O and depends only on `serde`. Everything else in the
//! system builds on these types.

pub mod action_points;
pub mod config;
pub mod cue;
pub mod diagnosis;
pub mod grid;
pub mod ids;
pub mod segment;
pub mod session;
pub mod trace;

pub use action_points::ActionPoints;
pub use config::Config;
pub use cue::{CoachingPlan, Cue, CueKind};
pub use diagnosis::{
    Diagnosis, ErrorKind, Finding, FindingDetail, Magnitude, MeasuredAgainst, Phase,
};
pub use grid::DistanceGrid;
pub use ids::{CarId, LapId, SegmentId, SessionId, TrackCar, TrackId};
pub use segment::{Segment, SegmentType, TrackGeometry};
pub use session::SessionType;
pub use trace::LapTrace;
