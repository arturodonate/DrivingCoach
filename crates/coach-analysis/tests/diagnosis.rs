//! Diagnosis-engine behavior tests (spec §7): earliest-phase attribution,
//! magnitude buckets, the persistence filter, and direction hysteresis.

use coach_analysis::diagnosis::Diagnoser;
use coach_analysis::reference::ReferenceModel;
use coach_core::config::{DiagnosisConfig, ReferenceConfig};
use coach_core::diagnosis::{ErrorKind, Magnitude, Phase};
use coach_core::grid::DistanceGrid;
use coach_core::ids::SegmentId;
use coach_core::segment::{Segment, SegmentType, TrackGeometry};
use coach_core::trace::LapTrace;
use std::collections::BTreeMap;

const LEN: f64 = 400.0;

/// Build a single-corner lap. `brake_on`/`apex`/`throttle_on` are distances (m);
/// the speed profile is a V bottoming at `apex_speed` at `apex`, peaking at
/// `peak_speed` at the ends.
fn corner_lap(
    brake_on: f64,
    apex: f64,
    throttle_on: f64,
    apex_speed: f32,
    peak_speed: f32,
) -> LapTrace {
    let grid = DistanceGrid::with_step(LEN, 2.0);
    let n = grid.len();
    let mut t = LapTrace::zeros(grid);
    for i in 0..n {
        let d = grid.dist_at(i);
        t.speed_kmh[i] = if d <= apex {
            peak_speed + (apex_speed - peak_speed) * (d / apex) as f32
        } else {
            apex_speed + (peak_speed - apex_speed) * ((d - apex) / (LEN - apex)) as f32
        };
        t.brake[i] = if (brake_on..apex).contains(&d) {
            0.6
        } else {
            0.0
        };
        t.throttle[i] = if d >= throttle_on { 1.0 } else { 0.0 };
        t.steer[i] = if (apex - 50.0..apex + 50.0).contains(&d) {
            0.4
        } else {
            0.0
        };
    }
    t
}

fn one_corner_geometry() -> TrackGeometry {
    TrackGeometry {
        track_id: "t".into(),
        track_length_m: LEN,
        segments: vec![Segment {
            segment_id: 0,
            seg_type: SegmentType::Corner,
            start_dist_m: 0.0,
            end_dist_m: LEN,
        }],
    }
}

/// A reference model whose only segment best is a clean reference lap.
fn reference_with(reference_lap: &LapTrace) -> ReferenceModel {
    let mut model = ReferenceModel::new(
        coach_core::ids::TrackCar::new("t", "c"),
        one_corner_geometry(),
        ReferenceConfig::default(),
    );
    let valid: BTreeMap<SegmentId, bool> = [(0u32, true)].into_iter().collect();
    model.consider_lap(&"ref".into(), reference_lap, &valid, true, 8_000);
    model
}

#[test]
fn earliest_phase_attribution_and_much_bucket() {
    // Reference: brake at 100, apex 200 @120, throttle 240, peak 200.
    let reference = corner_lap(100.0, 200.0, 240.0, 120.0, 200.0);
    let model = reference_with(&reference);

    // Current brakes 12 m early AND carries less apex speed AND is slower overall.
    // Earliest-phase attribution must pick the ENTRY error, not the mid one.
    let current = corner_lap(88.0, 200.0, 240.0, 112.0, 190.0);
    let mut diag = Diagnoser::new(DiagnosisConfig::default());
    let analysis = diag.diagnose(&"lap1".into(), &current, &model);

    let f = analysis
        .diagnosis
        .findings
        .iter()
        .find(|f| f.segment_id == 0)
        .expect("a finding on the corner");
    assert_eq!(f.phase, Phase::Entry, "earliest phase wins");
    assert_eq!(f.error, ErrorKind::BrakeTooEarly);
    assert_eq!(f.magnitude, Magnitude::Much, "12 m delta > 8 m → much");
    assert!(f.time_lost_ms > 0);
    // First coached lap → 1-of-1, so it is eligible to speak.
    assert_eq!(analysis.diagnosis.ranked_priority, vec![0]);
}

#[test]
fn persistence_filter_tightens_after_first_lap() {
    let reference = corner_lap(100.0, 200.0, 240.0, 120.0, 200.0);
    let model = reference_with(&reference);
    let clean = corner_lap(100.0, 200.0, 240.0, 120.0, 200.0);
    let braking_early = corner_lap(88.0, 200.0, 240.0, 112.0, 190.0);

    let mut diag = Diagnoser::new(DiagnosisConfig::default()); // min_of = 2 of 3

    // Lap 1: clean → no finding, valid lap counted.
    let a1 = diag.diagnose(&"l1".into(), &clean, &model);
    assert!(a1.diagnosis.ranked_priority.is_empty());

    // Lap 2: error appears for the first time → counted but NOT yet spoken.
    let a2 = diag.diagnose(&"l2".into(), &braking_early, &model);
    assert!(a2.diagnosis.findings.iter().any(|f| f.segment_id == 0));
    assert!(
        a2.diagnosis.ranked_priority.is_empty(),
        "single occurrence on lap 2 is below the 2-of-3 threshold"
    );

    // Lap 3: error persists → now spoken.
    let a3 = diag.diagnose(&"l3".into(), &braking_early, &model);
    assert_eq!(a3.diagnosis.ranked_priority, vec![0]);
    let f = a3
        .diagnosis
        .findings
        .iter()
        .find(|f| f.segment_id == 0)
        .unwrap();
    assert!(f.persistence_count >= 2);
}

#[test]
fn direction_hysteresis_goes_silent_on_flip() {
    let reference = corner_lap(100.0, 200.0, 240.0, 120.0, 200.0);
    let model = reference_with(&reference);

    // 1-of-1 persistence so we can isolate the hysteresis behavior.
    let cfg = DiagnosisConfig {
        persistence_min_of: 1,
        hysteresis_laps: 3,
        ..DiagnosisConfig::default()
    };
    let mut diag = Diagnoser::new(cfg);

    // Lap 1: brakes early → "brake later" advice spoken.
    let early = corner_lap(88.0, 200.0, 240.0, 112.0, 190.0);
    let a1 = diag.diagnose(&"l1".into(), &early, &model);
    assert_eq!(a1.diagnosis.ranked_priority, vec![0]);

    // Lap 2: now brakes 12 m LATE → opposite advice within the window → silent.
    let late = corner_lap(112.0, 200.0, 240.0, 112.0, 190.0);
    let a2 = diag.diagnose(&"l2".into(), &late, &model);
    assert!(
        a2.diagnosis.findings.iter().any(|f| f.segment_id == 0),
        "the finding still goes to the report"
    );
    assert!(
        a2.diagnosis.ranked_priority.is_empty(),
        "reversed advice within the hysteresis window is silenced"
    );
}
