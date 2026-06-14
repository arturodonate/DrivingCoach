//! Alignment & delta engine (spec §6).
//!
//! Both laps are already on the common 2 m distance grid, so a "comparison" is a
//! per-position subtraction. For one segment we compute the running time delta
//! (∫ ds·(1/v_cur − 1/v_ref)) plus the brake-point, turn-in, apex-speed,
//! throttle-on, and lateral-line deltas the diagnosis engine classifies on.

use crate::metrics::{extract_segment_metrics, SegmentMetrics};
use coach_core::config::ReferenceConfig;
use coach_core::segment::Segment;
use coach_core::trace::LapTrace;

/// Signed per-segment deltas, current − reference (positive time = current slower).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SegmentDelta {
    pub segment_id: u32,
    pub time_lost_ms: i64,
    /// current − reference; +ve = current brakes later (further along).
    pub brake_point_delta_m: Option<f64>,
    pub turn_in_delta_m: Option<f64>,
    /// current − reference; −ve = current is slower at the apex.
    pub apex_speed_delta_kmh: Option<f64>,
    pub throttle_on_delta_m: Option<f64>,
    /// Mean lateral offset between the two paths over the segment (m).
    pub line_offset_m: f64,
    /// +ve = current apex is further along than reference (later apex).
    pub apex_dist_delta_m: Option<f64>,
    pub current: SegmentMetrics,
    pub reference: SegmentMetrics,
}

/// Time delta over a grid range: ∫ ds·(1/v_cur − 1/v_ref), in ms (spec §6).
pub fn time_delta_ms(cur: &LapTrace, reff: &LapTrace, range: std::ops::Range<usize>) -> i64 {
    let step = cur.grid.step_m;
    let mut dt = 0.0_f64;
    for i in range {
        let vc = (cur.speed_kmh[i] as f64 / 3.6).max(0.5);
        let vr = (reff.speed_kmh[i] as f64 / 3.6).max(0.5);
        dt += step * (1.0 / vc - 1.0 / vr);
    }
    (dt * 1000.0).round() as i64
}

fn opt_delta(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x - y),
        _ => None,
    }
}

/// Compute all deltas for one segment between the current lap and a reference.
pub fn segment_delta(
    seg: &Segment,
    current: &LapTrace,
    reference: &LapTrace,
    cfg: &ReferenceConfig,
) -> SegmentDelta {
    let cm = extract_segment_metrics(seg, current, cfg);
    let rm = extract_segment_metrics(seg, reference, cfg);
    let range = current.index_range(seg.start_dist_m, seg.end_dist_m);

    // Mean lateral offset between the paths over the segment.
    let mut off = 0.0_f64;
    let mut count = 0usize;
    for i in range.clone() {
        let dx = current.world_x[i] as f64 - reference.world_x[i] as f64;
        let dy = current.world_y[i] as f64 - reference.world_y[i] as f64;
        off += (dx * dx + dy * dy).sqrt();
        count += 1;
    }
    let line_offset_m = if count > 0 { off / count as f64 } else { 0.0 };

    SegmentDelta {
        segment_id: seg.segment_id,
        time_lost_ms: time_delta_ms(current, reference, range),
        brake_point_delta_m: opt_delta(cm.brake_point_dist_m, rm.brake_point_dist_m),
        turn_in_delta_m: opt_delta(cm.turn_in_dist_m, rm.turn_in_dist_m),
        apex_speed_delta_kmh: Some(cm.apex_speed_kmh - rm.apex_speed_kmh),
        throttle_on_delta_m: opt_delta(cm.throttle_on_dist_m, rm.throttle_on_dist_m),
        line_offset_m,
        apex_dist_delta_m: opt_delta(cm.apex_dist_m, rm.apex_dist_m),
        current: cm,
        reference: rm,
    }
}
