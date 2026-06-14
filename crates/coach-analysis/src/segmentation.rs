//! Hysteresis-threshold curvature segmentation (spec §5.1).
//!
//! Walk the curvature profile with two thresholds: enter a corner only when κ
//! rises above `kappa_hi`, leave it only when κ falls below `kappa_lo`. The gap
//! between them stops gentle kinks (κ between the thresholds) from fragmenting
//! the track into spurious micro-segments. Segments shorter than `min_segment_m`
//! are merged into their predecessor.

use coach_core::config::ProfilerConfig;
use coach_core::grid::DistanceGrid;
use coach_core::ids::TrackId;
use coach_core::segment::{Segment, SegmentType, TrackGeometry};

/// Build the car-independent [`TrackGeometry`] from a curvature profile.
pub fn segment_track(
    track_id: TrackId,
    kappa: &[f64],
    grid: DistanceGrid,
    cfg: &ProfilerConfig,
) -> TrackGeometry {
    let n = kappa.len();
    let mut raw: Vec<(SegmentType, usize, usize)> = Vec::new(); // (type, start_idx, end_idx exclusive)

    if n == 0 {
        return TrackGeometry {
            track_id,
            track_length_m: grid.length_m,
            segments: Vec::new(),
        };
    }

    let mut in_corner = kappa[0] > cfg.kappa_hi;
    let mut seg_start = 0usize;
    for (i, &k) in kappa.iter().enumerate().skip(1) {
        if in_corner && k < cfg.kappa_lo {
            raw.push((SegmentType::Corner, seg_start, i));
            seg_start = i;
            in_corner = false;
        } else if !in_corner && k > cfg.kappa_hi {
            raw.push((SegmentType::Straight, seg_start, i));
            seg_start = i;
            in_corner = true;
        }
    }
    raw.push((
        if in_corner {
            SegmentType::Corner
        } else {
            SegmentType::Straight
        },
        seg_start,
        n,
    ));

    // Merge runts into the predecessor (or successor, for a tiny leading seg).
    let min_pts = (cfg.min_segment_m / grid.step_m).round() as usize;
    let mut merged: Vec<(SegmentType, usize, usize)> = Vec::new();
    for seg in raw {
        let len_pts = seg.2 - seg.1;
        if len_pts < min_pts.max(1) && !merged.is_empty() {
            // Extend the previous segment to swallow this runt.
            merged.last_mut().unwrap().2 = seg.2;
        } else {
            merged.push(seg);
        }
    }
    // If the very first segment is a runt, fold it into the next.
    if merged.len() >= 2 && (merged[0].2 - merged[0].1) < min_pts.max(1) {
        merged[1].1 = merged[0].1;
        merged.remove(0);
    }

    let segments = merged
        .into_iter()
        .enumerate()
        .map(|(id, (ty, s, e))| Segment {
            segment_id: id as u32,
            seg_type: ty,
            start_dist_m: grid.dist_at(s),
            end_dist_m: grid
                .dist_at(e.min(n - 1))
                .max(grid.dist_at(s) + grid.step_m),
        })
        .collect();

    TrackGeometry {
        track_id,
        track_length_m: grid.length_m,
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProfilerConfig {
        ProfilerConfig {
            kappa_hi: 0.012,
            kappa_lo: 0.006,
            min_segment_m: 20.0,
            ..ProfilerConfig::default()
        }
    }

    #[test]
    fn gentle_kink_does_not_fragment() {
        // 1000 m at 2 m → 500 pts. Mostly straight; a gentle kink (κ=0.008, below
        // kappa_hi) at 200–260 m; a real corner (κ=0.025) at 600–760 m.
        let grid = DistanceGrid::with_step(1000.0, 2.0);
        let n = grid.len();
        let mut kappa = vec![0.0; n];
        for k in kappa.iter_mut().take(130).skip(100) {
            *k = 0.008; // gentle kink — between lo and hi
        }
        for k in kappa.iter_mut().take(380).skip(300) {
            *k = 0.025; // genuine corner
        }
        let geom = segment_track("t".into(), &kappa, grid, &cfg());
        let corners = geom
            .segments
            .iter()
            .filter(|s| s.seg_type == SegmentType::Corner)
            .count();
        assert_eq!(corners, 1, "kink must not create a second corner");
    }

    #[test]
    fn detects_a_real_corner() {
        let grid = DistanceGrid::with_step(1000.0, 2.0);
        let n = grid.len();
        let mut kappa = vec![0.0; n];
        for k in kappa.iter_mut().take(300).skip(200) {
            *k = 0.03;
        }
        let geom = segment_track("t".into(), &kappa, grid, &cfg());
        assert!(geom
            .segments
            .iter()
            .any(|s| s.seg_type == SegmentType::Corner));
        // Segments must tile the lap contiguously.
        for w in geom.segments.windows(2) {
            assert!((w[0].end_dist_m - w[1].start_dist_m).abs() < 1e-6);
        }
    }
}
