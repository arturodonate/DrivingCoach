//! Per-segment behavior metrics & action-point extraction (spec §5.1, §10.1).
//!
//! From one lap's [`LapTrace`] and the track segmentation, compute the driving
//! anchors and behavior metrics for each segment. These feed three consumers:
//! the action-point layer (spec §5.1), the per-lap `segment_traversals` metrics
//! that power the post-event summary (spec §10.1b), and the diagnosis engine.

use coach_core::action_points::ActionPoints;
use coach_core::config::ReferenceConfig;
use coach_core::segment::Segment;
use coach_core::trace::LapTrace;

/// Behavior metrics for one segment traversal.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SegmentMetrics {
    pub segment_id: u32,
    /// Time to traverse the segment (ms), from ∫ ds/v.
    pub time_ms: i64,
    pub entry_speed_kmh: f64,
    pub min_speed_kmh: f64,
    /// Speed at the apex (the minimum-speed point).
    pub apex_speed_kmh: f64,
    pub brake_point_dist_m: Option<f64>,
    pub turn_in_dist_m: Option<f64>,
    pub apex_dist_m: Option<f64>,
    pub throttle_on_dist_m: Option<f64>,
}

impl SegmentMetrics {
    pub fn action_points(&self) -> ActionPoints {
        ActionPoints {
            segment_id: self.segment_id,
            brake_point_dist_m: self.brake_point_dist_m,
            turn_in_dist_m: self.turn_in_dist_m,
            apex_dist_m: self.apex_dist_m,
            throttle_on_dist_m: self.throttle_on_dist_m,
        }
    }
}

/// Traversal time of a grid index range, integrating `ds / v` (spec §6).
pub fn segment_time_ms(trace: &LapTrace, range: std::ops::Range<usize>) -> i64 {
    let step = trace.grid.step_m;
    let mut t_s = 0.0_f64;
    for i in range {
        let v_ms = (trace.speed_kmh[i] as f64 / 3.6).max(0.5); // guard standstill
        t_s += step / v_ms;
    }
    (t_s * 1000.0).round() as i64
}

/// Extract [`SegmentMetrics`] for one segment from a lap trace.
pub fn extract_segment_metrics(
    seg: &Segment,
    trace: &LapTrace,
    cfg: &ReferenceConfig,
) -> SegmentMetrics {
    let range = trace.index_range(seg.start_dist_m, seg.end_dist_m);
    let mut m = SegmentMetrics {
        segment_id: seg.segment_id,
        ..Default::default()
    };
    if range.is_empty() {
        return m;
    }
    let (s, e) = (range.start, range.end);
    m.time_ms = segment_time_ms(trace, s..e);
    m.entry_speed_kmh = trace.speed_kmh[s] as f64;

    // Apex = minimum-speed point in the segment.
    let mut min_i = s;
    for i in s..e {
        if trace.speed_kmh[i] < trace.speed_kmh[min_i] {
            min_i = i;
        }
    }
    m.min_speed_kmh = trace.speed_kmh[min_i] as f64;
    m.apex_speed_kmh = m.min_speed_kmh;
    m.apex_dist_m = Some(trace.grid.dist_at(min_i));

    // Brake point = first sustained braking ahead of the corner.
    m.brake_point_dist_m = first_sustained_brake(trace, s..e, cfg);

    // Turn-in = steering crosses a per-segment threshold.
    let max_abs_steer = (s..e).map(|i| trace.steer[i].abs()).fold(0.0_f32, f32::max);
    if max_abs_steer > 1e-3 {
        let thresh = max_abs_steer * cfg.turn_in_steer_frac;
        if let Some(i) = (s..e).find(|&i| trace.steer[i].abs() >= thresh) {
            m.turn_in_dist_m = Some(trace.grid.dist_at(i));
        }
    }

    // Throttle-on = throttle crosses the threshold, rising, after the apex.
    let frac = cfg.throttle_on_frac;
    for i in (min_i + 1)..e {
        if trace.throttle[i] >= frac && trace.throttle[i - 1] < frac {
            m.throttle_on_dist_m = Some(trace.grid.dist_at(i));
            break;
        }
    }

    m
}

/// First grid distance where brake exceeds the threshold and stays above it for
/// roughly `brake_sustain_s` (converted to grid points via local speed).
fn first_sustained_brake(
    trace: &LapTrace,
    range: std::ops::Range<usize>,
    cfg: &ReferenceConfig,
) -> Option<f64> {
    let step = trace.grid.step_m;
    let (s, e) = (range.start, range.end);
    for i in s..e {
        if trace.brake[i] > cfg.brake_on_frac {
            let v_ms = (trace.speed_kmh[i] as f64 / 3.6).max(1.0);
            let need = ((v_ms * cfg.brake_sustain_s) / step).round().max(1.0) as usize;
            let end = (i + need).min(e);
            if (i..end).all(|k| trace.brake[k] > cfg.brake_on_frac) {
                return Some(trace.grid.dist_at(i));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::grid::DistanceGrid;
    use coach_core::segment::{Segment, SegmentType};

    fn corner_trace() -> (Segment, LapTrace) {
        // 400 m segment on a 400 m grid: brake from 100 m, min speed at 200 m,
        // throttle back on from 240 m.
        let grid = DistanceGrid::with_step(400.0, 2.0);
        let n = grid.len();
        let mut t = LapTrace::zeros(grid);
        for i in 0..n {
            let d = grid.dist_at(i);
            t.speed_kmh[i] = if d < 200.0 {
                (200.0 - 0.4 * d) as f32 // 200 → 120
            } else {
                (120.0 + 0.4 * (d - 200.0)) as f32 // 120 → 200
            };
            t.brake[i] = if (100.0..200.0).contains(&d) {
                0.6
            } else {
                0.0
            };
            t.throttle[i] = if d >= 240.0 { 1.0 } else { 0.0 };
            t.steer[i] = if (150.0..260.0).contains(&d) {
                0.4
            } else {
                0.0
            };
        }
        let seg = Segment {
            segment_id: 1,
            seg_type: SegmentType::Corner,
            start_dist_m: 0.0,
            end_dist_m: 400.0,
        };
        (seg, t)
    }

    #[test]
    fn extracts_anchors_in_order() {
        let (seg, trace) = corner_trace();
        let m = extract_segment_metrics(&seg, &trace, &ReferenceConfig::default());
        let bp = m.brake_point_dist_m.unwrap();
        let apex = m.apex_dist_m.unwrap();
        let thr = m.throttle_on_dist_m.unwrap();
        assert!((bp - 100.0).abs() < 4.0, "brake point ~100 m, got {bp}");
        assert!((apex - 200.0).abs() < 4.0, "apex ~200 m, got {apex}");
        assert!(thr >= apex, "throttle-on after apex");
        assert!((m.min_speed_kmh - 120.0).abs() < 1.0);
    }
}
