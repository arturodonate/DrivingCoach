//! The car-dependent action-point layer (spec §5.1).
//!
//! Per `(track, car, segment)`: the key driving anchors extracted from that
//! car's reference traces. Recomputed whenever a segment best updates.

use crate::ids::SegmentId;
use serde::{Deserialize, Serialize};

/// Driving anchors for one segment (spec §5.1, §12.1 `action_points`).
///
/// Distances are metres-from-start on the lap distance grid. Any anchor may be
/// absent if it could not be extracted (e.g. no braking in a flat-out kink).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ActionPoints {
    pub segment_id: SegmentId,
    /// First sustained braking ahead of the corner.
    pub brake_point_dist_m: Option<f64>,
    /// Steering crosses the per-segment turn-in threshold.
    pub turn_in_dist_m: Option<f64>,
    /// Minimum-speed point (fallback: max curvature).
    pub apex_dist_m: Option<f64>,
    /// Throttle crosses 50% rising, after the apex.
    pub throttle_on_dist_m: Option<f64>,
}
