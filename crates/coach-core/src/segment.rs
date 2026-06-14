//! Track segmentation (the car-independent geometry layer, spec §5.1).

use crate::ids::{SegmentId, TrackId};
use serde::{Deserialize, Serialize};

/// Kind of micro-sector. One segment per corner complex, one per straight; a
/// dedicated braking zone may precede a corner (spec §5.1, §12.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SegmentType {
    Corner,
    Straight,
    Braking,
}

/// One micro-sector spanning `[start_dist_m, end_dist_m)` on the distance grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub segment_id: SegmentId,
    #[serde(rename = "type")]
    pub seg_type: SegmentType,
    pub start_dist_m: f64,
    pub end_dist_m: f64,
}

impl Segment {
    pub fn length_m(&self) -> f64 {
        (self.end_dist_m - self.start_dist_m).max(0.0)
    }

    pub fn contains(&self, dist_m: f64) -> bool {
        dist_m >= self.start_dist_m && dist_m < self.end_dist_m
    }
}

/// The car-independent track profile: ordered segments over the lap (spec §12.1
/// `track_geometry`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackGeometry {
    pub track_id: TrackId,
    pub track_length_m: f64,
    /// Ordered by `start_dist_m`.
    pub segments: Vec<Segment>,
}

impl TrackGeometry {
    /// Human-facing label for a segment, e.g. `Turn 5`. Corners are numbered in
    /// order of appearance; straights/braking zones get descriptive labels.
    pub fn label_for(&self, segment_id: SegmentId) -> String {
        let mut corner_no = 0u32;
        for seg in &self.segments {
            if seg.seg_type == SegmentType::Corner {
                corner_no += 1;
            }
            if seg.segment_id == segment_id {
                return match seg.seg_type {
                    SegmentType::Corner => format!("Turn {corner_no}"),
                    SegmentType::Straight => format!("Straight {}", seg.segment_id),
                    SegmentType::Braking => format!("Braking {}", seg.segment_id),
                };
            }
        }
        format!("Segment {segment_id}")
    }

    /// The segment containing `dist_m`, if any.
    pub fn segment_at(&self, dist_m: f64) -> Option<&Segment> {
        self.segments.iter().find(|s| s.contains(dist_m))
    }
}
