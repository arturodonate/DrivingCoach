//! Resampled lap telemetry on the distance grid.
//!
//! A [`LapTrace`] is a struct-of-arrays: each channel is a `Vec<f32>` indexed by
//! grid point. All channels share the grid length, so position `i` means
//! distance `grid.dist_at(i)` across every channel. This layout keeps per-channel
//! resampling and per-segment slicing cache-friendly and allocation-free to read.

use crate::grid::DistanceGrid;
use serde::{Deserialize, Serialize};

/// One lap's telemetry resampled onto a [`DistanceGrid`].
///
/// Channels carry the signals the diagnosis and cue engines need (spec §4.2,
/// §6, §7): longitudinal state, driver inputs, and world position for line
/// offset / curvature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LapTrace {
    pub grid: DistanceGrid,
    /// Speed in km/h.
    pub speed_kmh: Vec<f32>,
    /// Throttle 0.0–1.0 (`gas`).
    pub throttle: Vec<f32>,
    /// Brake 0.0–1.0.
    pub brake: Vec<f32>,
    /// Steering angle (radians or AC units; used only relatively).
    pub steer: Vec<f32>,
    /// World X coordinate (metres) — for curvature & lateral line offset.
    pub world_x: Vec<f32>,
    /// World Y coordinate (metres). AC's planar coordinate; see telemetry crate.
    pub world_y: Vec<f32>,
}

impl LapTrace {
    /// Allocate an all-zero trace sized to `grid`.
    pub fn zeros(grid: DistanceGrid) -> Self {
        let n = grid.len();
        Self {
            grid,
            speed_kmh: vec![0.0; n],
            throttle: vec![0.0; n],
            brake: vec![0.0; n],
            steer: vec![0.0; n],
            world_x: vec![0.0; n],
            world_y: vec![0.0; n],
        }
    }

    /// Number of grid points.
    pub fn len(&self) -> usize {
        self.speed_kmh.len()
    }

    pub fn is_empty(&self) -> bool {
        self.speed_kmh.is_empty()
    }

    /// True if every channel matches the grid length. Cheap invariant check.
    pub fn is_consistent(&self) -> bool {
        let n = self.grid.len();
        self.speed_kmh.len() == n
            && self.throttle.len() == n
            && self.brake.len() == n
            && self.steer.len() == n
            && self.world_x.len() == n
            && self.world_y.len() == n
    }

    /// Half-open grid-index range `[start, end)` covering `[start_m, end_m)`.
    pub fn index_range(&self, start_m: f64, end_m: f64) -> std::ops::Range<usize> {
        let start = self.grid.index_of(start_m);
        let end = (self.grid.index_of(end_m) + 1).min(self.len());
        start..end.max(start)
    }
}
