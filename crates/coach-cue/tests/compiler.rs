//! Cue-compiler tests (spec §9.1 trigger back-integration, §9.2 arbiter).

use coach_core::action_points::ActionPoints;
use coach_core::config::Config;
use coach_core::cue::CueKind;
use coach_core::diagnosis::{
    Diagnosis, ErrorKind, Finding, FindingDetail, Magnitude, MeasuredAgainst, Phase,
};
use coach_core::grid::DistanceGrid;
use coach_core::ids::SegmentId;
use coach_core::segment::{Segment, SegmentType, TrackGeometry};
use coach_core::trace::LapTrace;
use coach_cue::cache::WavCache;
use coach_cue::compiler::{back_integrate_trigger, CueCompiler};
use coach_cue::synth::FakeSynthesizer;
use std::collections::BTreeMap;

fn flat_ref_speed(len: f64, kmh: f32) -> LapTrace {
    let grid = DistanceGrid::with_step(len, 2.0);
    let mut t = LapTrace::zeros(grid);
    for v in t.speed_kmh.iter_mut() {
        *v = kmh;
    }
    t
}

#[test]
fn trigger_lands_at_least_lead_before_action() {
    // 50 m/s (180 km/h) constant. Lead of 1.7 s ⇒ ~85 m before the action point.
    let speed = flat_ref_speed(2000.0, 180.0);
    let action = 1000.0;
    let lead_s = 1.7;
    let trig = back_integrate_trigger(&speed, action, lead_s);
    assert!(trig < action);
    let elapsed = (action - trig) / 50.0; // distance / (m/s)
    assert!(
        elapsed >= lead_s - 0.1,
        "elapsed {elapsed} should cover the lead"
    );
    assert!(
        (action - trig - 85.0).abs() < 4.0,
        "≈85 m back, got {}",
        action - trig
    );
}

fn finding(seg: SegmentId, label: &str, time_lost: i64) -> Finding {
    Finding {
        segment_id: seg,
        segment_label: label.to_string(),
        phase: Phase::Entry,
        time_lost_ms: time_lost,
        error: ErrorKind::BrakeTooEarly,
        magnitude: Magnitude::Slight,
        persistence_count: 2,
        measured_against: MeasuredAgainst::SegmentBest,
        detail: FindingDetail::default(),
    }
}

fn geometry(corners: &[(SegmentId, f64, f64)]) -> TrackGeometry {
    TrackGeometry {
        track_id: "t".into(),
        track_length_m: 2000.0,
        segments: corners
            .iter()
            .map(|&(id, s, e)| Segment {
                segment_id: id,
                seg_type: SegmentType::Corner,
                start_dist_m: s,
                end_dist_m: e,
            })
            .collect(),
    }
}

#[test]
fn arbiter_drops_lower_priority_on_overlap() {
    let dir = std::env::temp_dir().join("coach_cue_arbiter_test");
    let _ = std::fs::remove_dir_all(&dir);
    let synth = FakeSynthesizer::default();
    let cache = WavCache::new(&dir, 22_050, &synth);
    let cfg = Config::default();
    let compiler = CueCompiler::new(&cfg, &cache);

    // Two corners with action points only 10 m apart → their cue windows overlap.
    let geom = geometry(&[(0, 0.0, 100.0), (1, 100.0, 200.0)]);
    let mut ap: BTreeMap<SegmentId, ActionPoints> = BTreeMap::new();
    ap.insert(
        0,
        ActionPoints {
            segment_id: 0,
            brake_point_dist_m: Some(90.0),
            ..Default::default()
        },
    );
    ap.insert(
        1,
        ActionPoints {
            segment_id: 1,
            brake_point_dist_m: Some(100.0),
            ..Default::default()
        },
    );

    let diagnosis = Diagnosis {
        lap_id: "l".into(),
        track_id: "t".into(),
        car_id: "c".into(),
        total_delta_ms: 500,
        findings: vec![finding(0, "Turn 1", 300), finding(1, "Turn 2", 120)],
        fixed: vec![],
        ranked_priority: vec![0, 1],
    };
    let speed = flat_ref_speed(2000.0, 120.0);
    let plan = compiler.compile(&diagnosis, &geom, &ap, &speed).unwrap();

    // Action points within chicane_merge_m (60) → merged into ONE chicane cue.
    assert_eq!(plan.cues.len(), 1);
    assert!(plan.cues[0].template.starts_with("chicane —"));
}

#[test]
fn separate_corners_yield_two_cues() {
    let dir = std::env::temp_dir().join("coach_cue_two_test");
    let _ = std::fs::remove_dir_all(&dir);
    let synth = FakeSynthesizer::default();
    let cache = WavCache::new(&dir, 22_050, &synth);
    let cfg = Config::default();
    let compiler = CueCompiler::new(&cfg, &cache);

    // Far-apart corners → no merge, both fit.
    let geom = geometry(&[(0, 0.0, 200.0), (1, 1500.0, 1700.0)]);
    let mut ap: BTreeMap<SegmentId, ActionPoints> = BTreeMap::new();
    ap.insert(
        0,
        ActionPoints {
            segment_id: 0,
            brake_point_dist_m: Some(150.0),
            ..Default::default()
        },
    );
    ap.insert(
        1,
        ActionPoints {
            segment_id: 1,
            brake_point_dist_m: Some(1650.0),
            ..Default::default()
        },
    );

    let diagnosis = Diagnosis {
        lap_id: "l".into(),
        track_id: "t".into(),
        car_id: "c".into(),
        total_delta_ms: 500,
        findings: vec![finding(0, "Turn 1", 300), finding(1, "Turn 9", 120)],
        fixed: vec![],
        ranked_priority: vec![0, 1],
    };
    let speed = flat_ref_speed(2000.0, 150.0);
    let plan = compiler.compile(&diagnosis, &geom, &ap, &speed).unwrap();

    assert_eq!(plan.cues.len(), 2);
    // Ordered by trigger distance, each finishing before its action point.
    for c in &plan.cues {
        assert!(c.trigger_dist_m < c.action_point_dist_m);
        assert_eq!(c.kind, CueKind::Correction);
    }
    assert!(plan.cues[0].trigger_dist_m <= plan.cues[1].trigger_dist_m);
}
