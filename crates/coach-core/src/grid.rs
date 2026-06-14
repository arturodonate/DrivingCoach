//! The common distance grid.
//!
//! Every lap and reference is resampled onto a shared, uniform
//! distance-from-start grid so comparisons are clean per-position subtractions
//! (spec §6). The default spacing is 2 m (spec §6, §12).

use serde::{Deserialize, Serialize};

/// Default grid spacing in metres (spec §6).
pub const DEFAULT_STEP_M: f64 = 2.0;

/// A uniform distance grid covering `[0, length_m]` at a fixed step.
///
/// Point `i` sits at distance `i * step_m`. The number of points is
/// `floor(length_m / step_m) + 1` so the grid spans the whole lap, with the
/// wrap from `~length_m` back to `0` handled by the caller at the lap boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DistanceGrid {
    pub length_m: f64,
    pub step_m: f64,
}

impl DistanceGrid {
    /// Build a grid for a track of `length_m` at the default 2 m spacing.
    pub fn new(length_m: f64) -> Self {
        Self {
            length_m,
            step_m: DEFAULT_STEP_M,
        }
    }

    /// Build a grid with an explicit step. `step_m` must be > 0.
    pub fn with_step(length_m: f64, step_m: f64) -> Self {
        assert!(step_m > 0.0, "grid step must be positive");
        assert!(length_m > 0.0, "track length must be positive");
        Self { length_m, step_m }
    }

    /// Number of grid points (inclusive of point 0).
    pub fn len(&self) -> usize {
        (self.length_m / self.step_m).floor() as usize + 1
    }

    pub fn is_empty(&self) -> bool {
        // A valid grid always has at least point 0; never empty in practice.
        self.len() == 0
    }

    /// Distance in metres of grid point `i`.
    pub fn dist_at(&self, i: usize) -> f64 {
        i as f64 * self.step_m
    }

    /// Index of the grid point nearest to `dist_m`, clamped to valid range.
    pub fn index_of(&self, dist_m: f64) -> usize {
        if dist_m <= 0.0 {
            return 0;
        }
        let i = (dist_m / self.step_m).round() as usize;
        i.min(self.len() - 1)
    }

    /// Iterator over the distance value at every grid point.
    pub fn distances(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.len()).map(move |i| self.dist_at(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_count_and_positions() {
        let g = DistanceGrid::with_step(100.0, 2.0);
        assert_eq!(g.len(), 51); // 0,2,..,100
        assert_eq!(g.dist_at(0), 0.0);
        assert_eq!(g.dist_at(50), 100.0);
    }

    #[test]
    fn index_of_rounds_and_clamps() {
        let g = DistanceGrid::with_step(100.0, 2.0);
        assert_eq!(g.index_of(0.0), 0);
        assert_eq!(g.index_of(2.9), 1); // 1.45 steps -> rounds to 1
        assert_eq!(g.index_of(3.1), 2);
        assert_eq!(g.index_of(1_000.0), g.len() - 1); // clamped
    }
}
