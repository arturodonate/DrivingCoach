//! Track profiler (spec §5.1, build step §13.2).
//!
//! Builds the car-independent geometry layer from the **median over the first N
//! clean laps** (spec §5.1): median world path → Savitzky–Golay smoothing →
//! curvature → hysteresis segmentation.

use crate::curvature::curvature;
use crate::error::{AnalysisError, Result};
use crate::savgol::smooth;
use crate::segmentation::segment_track;
use coach_core::config::ProfilerConfig;
use coach_core::ids::TrackId;
use coach_core::segment::TrackGeometry;
use coach_core::trace::LapTrace;

/// Build a [`TrackGeometry`] from clean-lap traces (all on the same grid).
pub fn build_profile(
    track_id: TrackId,
    laps: &[LapTrace],
    cfg: &ProfilerConfig,
) -> Result<TrackGeometry> {
    if laps.is_empty() {
        return Err(AnalysisError::InsufficientData(
            "need ≥1 clean lap to profile a track".into(),
        ));
    }
    let grid = laps[0].grid;
    let n = grid.len();
    for l in laps {
        if l.grid != grid || !l.is_consistent() {
            return Err(AnalysisError::Invalid(
                "all profile laps must share one consistent grid".into(),
            ));
        }
    }

    // Per-grid-point median of the world path across laps (robust to one bad lap).
    let mut mx = vec![0.0_f64; n];
    let mut my = vec![0.0_f64; n];
    let mut col = Vec::with_capacity(laps.len());
    for i in 0..n {
        col.clear();
        col.extend(laps.iter().map(|l| l.world_x[i] as f64));
        mx[i] = median(&mut col);
        col.clear();
        col.extend(laps.iter().map(|l| l.world_y[i] as f64));
        my[i] = median(&mut col);
    }

    // Smooth before differentiating (spec §5.1); the path is a closed loop.
    let sx = smooth(
        &mx,
        cfg.savgol_window_m,
        grid.step_m,
        cfg.savgol_poly_order,
        true,
    );
    let sy = smooth(
        &my,
        cfg.savgol_window_m,
        grid.step_m,
        cfg.savgol_poly_order,
        true,
    );
    let kappa = curvature(&sx, &sy, grid.step_m);

    Ok(segment_track(track_id, &kappa, grid, cfg))
}

/// Accumulates clean laps until `profile_laps` are gathered, then yields the
/// geometry once (spec §5.1: median over the first 3 clean laps).
pub struct TrackProfiler {
    track_id: TrackId,
    cfg: ProfilerConfig,
    laps: Vec<LapTrace>,
    built: bool,
}

impl TrackProfiler {
    pub fn new(track_id: TrackId, cfg: ProfilerConfig) -> Self {
        Self {
            track_id,
            cfg,
            laps: Vec::new(),
            built: false,
        }
    }

    /// Add a clean lap. Returns `Some(geometry)` exactly once, when the Nth lap
    /// completes the profile.
    pub fn add_clean_lap(&mut self, trace: LapTrace) -> Option<TrackGeometry> {
        if self.built {
            return None;
        }
        self.laps.push(trace);
        if self.laps.len() >= self.cfg.profile_laps {
            self.built = true;
            build_profile(self.track_id.clone(), &self.laps, &self.cfg).ok()
        } else {
            None
        }
    }

    pub fn is_ready(&self) -> bool {
        self.built
    }
}

/// Median of `v` (mutates by sorting). Empty slice → 0.0.
pub fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    if v.len() % 2 == 1 {
        v[m]
    } else {
        0.5 * (v[m - 1] + v[m])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::grid::DistanceGrid;
    use std::f64::consts::TAU;

    /// A circular track of radius `r`; one constant-curvature corner over the
    /// whole loop. Builds a trace with the world path set; other channels zero.
    fn circular_lap(r: f64, grid: DistanceGrid, jitter: f64) -> LapTrace {
        let n = grid.len();
        let mut t = LapTrace::zeros(grid);
        for i in 0..n {
            let frac = i as f64 / n as f64;
            let theta = frac * TAU;
            t.world_x[i] = (r * theta.cos() + jitter * ((i % 3) as f64 - 1.0)) as f32;
            t.world_y[i] = (r * theta.sin() + jitter * ((i % 2) as f64 - 0.5)) as f32;
        }
        t
    }

    #[test]
    fn median_path_profiles_a_circle_into_a_single_corner() {
        let circumference = TAU * 100.0;
        let grid = DistanceGrid::with_step(circumference, 2.0);
        let laps = vec![
            circular_lap(100.0, grid, 0.0),
            circular_lap(100.0, grid, 0.5),
            circular_lap(100.0, grid, -0.5),
        ];
        let cfg = ProfilerConfig {
            kappa_hi: 0.008,
            kappa_lo: 0.004,
            min_segment_m: 30.0,
            ..ProfilerConfig::default()
        };
        let geom = build_profile("ring".into(), &laps, &cfg).unwrap();
        // A circle is one continuous corner (κ ≈ 0.01 > kappa_hi everywhere).
        assert!(!geom.segments.is_empty());
        assert!(geom
            .segments
            .iter()
            .all(|s| s.seg_type == coach_core::segment::SegmentType::Corner));
    }
}
