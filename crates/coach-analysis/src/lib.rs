//! # coach-analysis
//!
//! The offline analysis tier (spec §5–§7, build steps §13.2–§13.4):
//!
//! - [`resample`] — telemetry → uniform distance grid (spec §6).
//! - [`savgol`], [`curvature`], [`segmentation`], [`profiler`] — the
//!   car-independent geometry layer (spec §5.1).
//! - [`metrics`] — per-segment behavior metrics & action points (spec §5.1, §10).
//! - [`reference`] — segment bests, entry-speed guard, stitched reference (§5.2).
//! - [`align`] — position-aligned per-segment deltas (spec §6).
//! - [`diagnosis`] — the stateful diagnosis engine with persistence, hysteresis,
//!   and magnitude buckets (spec §7), emitting the §12.2 diagnosis object.

pub mod align;
pub mod curvature;
pub mod diagnosis;
pub mod error;
pub mod metrics;
pub mod profiler;
pub mod reference;
pub mod resample;
pub mod savgol;
pub mod segmentation;

pub use align::{segment_delta, SegmentDelta};
pub use diagnosis::{Diagnoser, LapAnalysis};
pub use error::{AnalysisError, Result};
pub use metrics::{extract_segment_metrics, SegmentMetrics};
pub use profiler::{build_profile, TrackProfiler};
pub use reference::{ReferenceModel, SegmentBest};
pub use resample::resample_lap;
