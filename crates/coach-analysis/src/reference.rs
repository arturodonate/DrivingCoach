//! Reference model (spec §5.2, §5.3, build step §13.3).
//!
//! Maintains, per `(track, car)`: the fastest traversal of each micro-sector
//! (segment best), with its entry speed and trace slice; the action-point layer;
//! and the fastest fully-clean full lap (for the entry-speed-guard fallback).
//! The stitched reference is the concatenation of segment bests — "the fastest
//! you have personally driven each piece, in this car."

use crate::metrics::extract_segment_metrics;
use coach_core::action_points::ActionPoints;
use coach_core::config::ReferenceConfig;
use coach_core::diagnosis::MeasuredAgainst;
use coach_core::ids::{LapId, SegmentId, TrackCar};
use coach_core::segment::TrackGeometry;
use coach_core::trace::LapTrace;
use std::collections::BTreeMap;

/// The best-ever traversal of one segment in this car.
#[derive(Debug, Clone)]
pub struct SegmentBest {
    pub segment_id: SegmentId,
    pub time_ms: i64,
    pub entry_speed_kmh: f64,
    /// Full-grid trace with this segment's range populated (zeros elsewhere).
    pub trace: LapTrace,
    pub source_lap_id: LapId,
    /// Observed segment times, for the consistency/variance signal (§5.5).
    pub times_ms: Vec<i64>,
}

impl SegmentBest {
    /// Population standard deviation of segment times, in ms (consistency, §5.5).
    pub fn time_variance_ms(&self) -> f64 {
        let n = self.times_ms.len();
        if n < 2 {
            return 0.0;
        }
        let mean = self.times_ms.iter().map(|&t| t as f64).sum::<f64>() / n as f64;
        let var = self
            .times_ms
            .iter()
            .map(|&t| {
                let d = t as f64 - mean;
                d * d
            })
            .sum::<f64>()
            / n as f64;
        var.sqrt()
    }
}

/// Per-`(track, car)` reference.
pub struct ReferenceModel {
    pub track_car: TrackCar,
    pub geometry: TrackGeometry,
    pub segment_bests: BTreeMap<SegmentId, SegmentBest>,
    pub action_points: BTreeMap<SegmentId, ActionPoints>,
    /// Fastest fully-clean full lap's trace, for the entry-speed guard fallback.
    pub pb_lap_trace: Option<LapTrace>,
    pub pb_lap_time_ms: Option<i64>,
    cfg: ReferenceConfig,
}

impl ReferenceModel {
    pub fn new(track_car: TrackCar, geometry: TrackGeometry, cfg: ReferenceConfig) -> Self {
        Self {
            track_car,
            geometry,
            segment_bests: BTreeMap::new(),
            action_points: BTreeMap::new(),
            pb_lap_trace: None,
            pb_lap_time_ms: None,
            cfg,
        }
    }

    /// True once every segment has a best — the reference is usable.
    pub fn is_complete(&self) -> bool {
        !self.geometry.segments.is_empty()
            && self
                .geometry
                .segments
                .iter()
                .all(|s| self.segment_bests.contains_key(&s.segment_id))
    }

    /// Fold a lap into the reference. `segment_valid[seg]` marks segment-level
    /// clean gating (spec §5.3): valid segments of dirty laps are admitted to
    /// segment bests; `full_lap_clean` gates the full-lap PB.
    pub fn consider_lap(
        &mut self,
        lap_id: &LapId,
        trace: &LapTrace,
        segment_valid: &BTreeMap<SegmentId, bool>,
        full_lap_clean: bool,
        lap_time_ms: i64,
    ) {
        for seg in &self.geometry.segments {
            if !segment_valid.get(&seg.segment_id).copied().unwrap_or(false) {
                continue;
            }
            let m = extract_segment_metrics(seg, trace, &self.cfg);
            let range = trace.index_range(seg.start_dist_m, seg.end_dist_m);
            let entry = &mut self.segment_bests;
            let improves = entry
                .get(&seg.segment_id)
                .map(|b| m.time_ms < b.time_ms)
                .unwrap_or(true);

            match entry.get_mut(&seg.segment_id) {
                Some(best) => {
                    best.times_ms.push(m.time_ms);
                    if improves {
                        best.time_ms = m.time_ms;
                        best.entry_speed_kmh = m.entry_speed_kmh;
                        best.trace = slice_trace(trace, range);
                        best.source_lap_id = lap_id.clone();
                        // Recompute action points whenever the best updates (§5.1).
                        self.action_points.insert(seg.segment_id, m.action_points());
                    }
                }
                None => {
                    entry.insert(
                        seg.segment_id,
                        SegmentBest {
                            segment_id: seg.segment_id,
                            time_ms: m.time_ms,
                            entry_speed_kmh: m.entry_speed_kmh,
                            trace: slice_trace(trace, range),
                            source_lap_id: lap_id.clone(),
                            times_ms: vec![m.time_ms],
                        },
                    );
                    self.action_points.insert(seg.segment_id, m.action_points());
                }
            }
        }

        if full_lap_clean && self.pb_lap_time_ms.map_or(true, |pb| lap_time_ms < pb) {
            self.pb_lap_time_ms = Some(lap_time_ms);
            self.pb_lap_trace = Some(trace.clone());
        }
    }

    /// Theoretical-best lap time = sum of segment bests (spec §10.1a).
    pub fn theoretical_best_ms(&self) -> Option<i64> {
        if self.segment_bests.is_empty() {
            return None;
        }
        Some(self.segment_bests.values().map(|b| b.time_ms).sum())
    }

    /// Assemble the stitched reference trace from segment bests (spec §5.2).
    pub fn stitched_reference(&self) -> Option<LapTrace> {
        let grid = self.geometry_grid()?;
        let mut out = LapTrace::zeros(grid);
        for seg in &self.geometry.segments {
            let best = self.segment_bests.get(&seg.segment_id)?;
            let range = out.index_range(seg.start_dist_m, seg.end_dist_m);
            copy_range(&best.trace, &mut out, range);
        }
        Some(out)
    }

    /// Pick the comparison reference for a segment, applying the entry-speed
    /// guard (spec §5.2): if the current entry speed differs from the segment
    /// best's by more than the configured fraction, fall back to the full PB
    /// lap's trace for that segment.
    pub fn reference_for_segment(
        &self,
        segment_id: SegmentId,
        current_entry_speed_kmh: f64,
    ) -> Option<(&LapTrace, MeasuredAgainst)> {
        let best = self.segment_bests.get(&segment_id)?;
        let rel = if best.entry_speed_kmh.abs() > 1e-3 {
            (current_entry_speed_kmh - best.entry_speed_kmh).abs() / best.entry_speed_kmh
        } else {
            0.0
        };
        if rel > self.cfg.entry_speed_guard_frac {
            if let Some(pb) = &self.pb_lap_trace {
                return Some((pb, MeasuredAgainst::PersonalBestLap));
            }
        }
        Some((&best.trace, MeasuredAgainst::SegmentBest))
    }

    fn geometry_grid(&self) -> Option<coach_core::grid::DistanceGrid> {
        self.segment_bests.values().next().map(|b| b.trace.grid)
    }
}

/// Produce a full-grid trace with only `range` copied from `src` (zeros else).
fn slice_trace(src: &LapTrace, range: std::ops::Range<usize>) -> LapTrace {
    let mut out = LapTrace::zeros(src.grid);
    copy_range(src, &mut out, range);
    out
}

fn copy_range(src: &LapTrace, dst: &mut LapTrace, range: std::ops::Range<usize>) {
    for i in range {
        dst.speed_kmh[i] = src.speed_kmh[i];
        dst.throttle[i] = src.throttle[i];
        dst.brake[i] = src.brake[i];
        dst.steer[i] = src.steer[i];
        dst.world_x[i] = src.world_x[i];
        dst.world_y[i] = src.world_y[i];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::grid::DistanceGrid;
    use coach_core::segment::{Segment, SegmentType};

    fn one_segment_geometry() -> TrackGeometry {
        TrackGeometry {
            track_id: "t".into(),
            track_length_m: 400.0,
            segments: vec![Segment {
                segment_id: 0,
                seg_type: SegmentType::Corner,
                start_dist_m: 0.0,
                end_dist_m: 400.0,
            }],
        }
    }

    fn flat_trace(speed: f32, entry_marker: f32) -> LapTrace {
        let grid = DistanceGrid::with_step(400.0, 2.0);
        let mut t = LapTrace::zeros(grid);
        for i in 0..grid.len() {
            t.speed_kmh[i] = speed;
        }
        t.speed_kmh[0] = entry_marker; // entry speed at segment start
        t
    }

    #[test]
    fn entry_speed_guard_falls_back_to_pb_lap() {
        let mut model = ReferenceModel::new(
            TrackCar::new("t", "c"),
            one_segment_geometry(),
            ReferenceConfig::default(), // 7% guard
        );
        let valid: BTreeMap<SegmentId, bool> = [(0u32, true)].into_iter().collect();

        // Best segment at entry speed 200; also a clean full-lap PB.
        let best = flat_trace(150.0, 200.0);
        model.consider_lap(&"lap_best".into(), &best, &valid, true, 10_000);

        // Same-ish entry speed → use segment best.
        let (_, ma) = model.reference_for_segment(0, 205.0).unwrap();
        assert_eq!(ma, MeasuredAgainst::SegmentBest);

        // Entry speed 150 vs best 200 → 25% off → fall back to PB lap.
        let (_, ma2) = model.reference_for_segment(0, 150.0).unwrap();
        assert_eq!(ma2, MeasuredAgainst::PersonalBestLap);
    }

    #[test]
    fn faster_traversal_updates_segment_best() {
        let mut model = ReferenceModel::new(
            TrackCar::new("t", "c"),
            one_segment_geometry(),
            ReferenceConfig::default(),
        );
        let valid: BTreeMap<SegmentId, bool> = [(0u32, true)].into_iter().collect();
        // Slow lap then fast lap (higher speed → lower ∫ds/v time).
        model.consider_lap(
            &"slow".into(),
            &flat_trace(100.0, 100.0),
            &valid,
            true,
            20_000,
        );
        let slow_best = model.segment_bests[&0].time_ms;
        model.consider_lap(
            &"fast".into(),
            &flat_trace(200.0, 200.0),
            &valid,
            true,
            10_000,
        );
        let fast_best = model.segment_bests[&0].time_ms;
        assert!(fast_best < slow_best);
        assert_eq!(model.segment_bests[&0].source_lap_id.as_str(), "fast");
    }
}
