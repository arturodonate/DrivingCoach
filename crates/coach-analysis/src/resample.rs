//! Distance-grid resampling (spec §6).
//!
//! Converts a lap's time-ordered telemetry frames into a [`LapTrace`] sampled on
//! a uniform [`DistanceGrid`]. Distance is `normalized_car_position ×
//! track_spline_length` (spec §6). Channels are linearly interpolated between
//! the two frames bracketing each grid distance, so comparisons downstream are
//! clean per-position subtractions.

use crate::error::{AnalysisError, Result};
use coach_core::grid::DistanceGrid;
use coach_core::trace::LapTrace;
use coach_telemetry::frame::TelemetryFrame;

/// Resample `frames` (one lap, in order) onto `grid`.
///
/// Frame distance is computed from `normalized_car_position`; tiny
/// non-monotonic jitter is removed by clamping to the running maximum so the
/// interpolation domain is strictly increasing.
pub fn resample_lap(
    frames: &[TelemetryFrame],
    spline_length_m: f64,
    grid: DistanceGrid,
) -> Result<LapTrace> {
    if frames.len() < 2 {
        return Err(AnalysisError::InsufficientData(
            "need ≥2 frames to resample a lap".into(),
        ));
    }

    // Sample distances, made strictly monotonic.
    let mut dist = Vec::with_capacity(frames.len());
    let mut running = f64::NEG_INFINITY;
    for f in frames {
        let mut d = f.normalized_car_position as f64 * spline_length_m;
        if d <= running {
            d = running + 1e-6; // nudge past jitter to keep the domain increasing
        }
        running = d;
        dist.push(d);
    }

    let mut out = LapTrace::zeros(grid);
    let n = grid.len();
    let mut j = 0usize; // sliding lower-bracket index into `dist`

    for i in 0..n {
        let target = grid.dist_at(i);
        // Advance bracket so dist[j] <= target <= dist[j+1] where possible.
        while j + 1 < dist.len() && dist[j + 1] < target {
            j += 1;
        }
        let (a, b) = (j, (j + 1).min(frames.len() - 1));
        let t = if (dist[b] - dist[a]).abs() < f64::EPSILON {
            0.0
        } else {
            ((target - dist[a]) / (dist[b] - dist[a])).clamp(0.0, 1.0)
        };
        let lerp = |x: f32, y: f32| x + (y - x) * t as f32;

        out.speed_kmh[i] = lerp(frames[a].speed_kmh, frames[b].speed_kmh);
        out.throttle[i] = lerp(frames[a].gas, frames[b].gas);
        out.brake[i] = lerp(frames[a].brake, frames[b].brake);
        out.steer[i] = lerp(frames[a].steer_angle, frames[b].steer_angle);
        let (ax, ay) = frames[a].planar_xy();
        let (bx, by) = frames[b].planar_xy();
        out.world_x[i] = lerp(ax, bx);
        out.world_y[i] = lerp(ay, by);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_telemetry::frame::AcStatus;

    fn frame(pos: f32, speed: f32) -> TelemetryFrame {
        TelemetryFrame {
            t_us: 0,
            packet_id: 0,
            gas: 0.5,
            brake: 0.0,
            gear: 3,
            steer_angle: 0.0,
            speed_kmh: speed,
            velocity: [0.0; 3],
            acc_g: [0.0; 3],
            wheel_slip: [0.0; 4],
            normalized_car_position: pos,
            car_coordinates: [pos * 1000.0, 0.0, 0.0],
            completed_laps: 0,
            i_current_time_ms: 0,
            i_last_time_ms: 0,
            number_of_tyres_out: 0,
            penalty: false,
            is_in_pit: false,
            status: AcStatus::Live,
            session: 1,
        }
    }

    #[test]
    fn length_matches_grid_and_interpolates_linearly() {
        // Speed rises linearly 100→200 over a 1000 m lap.
        let frames: Vec<_> = (0..=100)
            .map(|i| frame(i as f32 / 100.0, 100.0 + i as f32))
            .collect();
        let grid = DistanceGrid::with_step(1000.0, 2.0);
        let trace = resample_lap(&frames, 1000.0, grid).unwrap();
        assert_eq!(trace.len(), grid.len());
        assert!(trace.is_consistent());
        // At 500 m (half), speed ≈ 150.
        let mid = grid.index_of(500.0);
        assert!((trace.speed_kmh[mid] - 150.0).abs() < 1.0);
        // World x tracks distance (we set x = pos*1000 = distance).
        assert!((trace.world_x[mid] as f64 - 500.0).abs() < 2.0);
    }
}
