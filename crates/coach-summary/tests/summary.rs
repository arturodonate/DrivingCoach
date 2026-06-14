//! Summary correctness against the spec §14 acceptance criteria:
//! - overall delta matches a hand-computed first/best/last;
//! - in/out laps and injected outliers are excluded from the trend;
//! - a deliberately-moved brake point is detected with the correct-sign time change;
//! - the race template prints the fuel/tyre caveat and practice does not.

use chrono::{DateTime, Utc};
use coach_core::config::SummaryConfig;
use coach_core::session::SessionType;
use coach_storage::models::{LapRow, SegmentTraversalRow, SessionRow};
use coach_summary::builder::{build_summary, SummaryInputs};
use coach_summary::markdown::render_markdown;
use coach_summary::model::Trend;

fn at() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-06-13T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn session(stype: SessionType) -> SessionRow {
    SessionRow {
        session_id: "s".into(),
        track_id: "brands_hatch".into(),
        car_id: "bmw_m2".into(),
        session_type: stype,
        started_at: at(),
        ended_at: None,
        summary_md_path: None,
        summary_json_path: None,
    }
}

fn lap(id: &str, time_ms: i64, valid: bool, in_out: bool) -> LapRow {
    LapRow {
        lap_id: id.into(),
        session_id: "s".into(),
        track_id: "brands_hatch".into(),
        car_id: "bmw_m2".into(),
        lap_time_ms: time_ms,
        is_valid: valid,
        is_in_out_lap: in_out,
        is_outlier: false,
        recorded_at: at(),
        trace: None,
    }
}

#[test]
fn pace_delta_and_exclusions_match_hand_computation() {
    let laps = vec![
        lap("out", 105_000, false, true), // in/out → excluded
        lap("l1", 100_000, true, false),  // first clean
        lap("l2", 99_000, true, false),
        lap("l3", 98_000, true, false),       // best & last clean
        lap("traffic", 130_000, true, false), // outlier → excluded
    ];
    let input = SummaryInputs {
        session: &session(SessionType::Practice),
        laps: &laps,
        traversals: &[],
        geometry: None,
        theoretical_best_ms: Some(97_500),
        diagnoses: &[],
    };
    let s = build_summary(&input, &SummaryConfig::default(), at()).unwrap();

    assert_eq!(s.in_out_excluded, 1);
    assert_eq!(
        s.outlier_excluded, 1,
        "the 130s lap is rejected as an outlier"
    );
    assert_eq!(s.clean_lap_count, 3);
    assert_eq!(s.pace.first_ms, 100_000);
    assert_eq!(s.pace.best_ms, 98_000);
    assert_eq!(s.pace.last_ms, 98_000);
    assert_eq!(s.pace.best_minus_first_ms, -2_000);
    assert_eq!(s.pace.last_minus_first_ms, -2_000);
    assert_eq!(s.pace.trend, Trend::Improving);
    // Raw all-lap listing keeps everything (nothing hidden).
    assert_eq!(s.all_laps.len(), 5);
    assert!(s.all_laps.iter().any(|e| e.outlier));
}

#[test]
fn detects_brake_point_moved_later_with_time_drop() {
    // Six clean laps; brake point moves from ~100 m to ~110 m, segment time falls.
    let mut laps = Vec::new();
    let mut trav = Vec::new();
    let brake = [100.0, 101.0, 100.0, 109.0, 110.0, 111.0];
    let stime = [5_000, 4_980, 5_010, 4_820, 4_800, 4_790];
    for i in 0..6 {
        let id = format!("l{i}");
        laps.push(lap(&id, 95_000 + (i as i64) * 5, true, false));
        trav.push(SegmentTraversalRow {
            lap_id: id.into(),
            segment_id: 5,
            time_ms: stime[i],
            is_valid: true,
            entry_speed_kmh: 200.0,
            brake_point_dist_m: Some(brake[i]),
            turn_in_dist_m: None,
            apex_speed_kmh: Some(120.0),
            min_speed_kmh: Some(118.0),
            throttle_on_dist_m: None,
        });
    }
    let input = SummaryInputs {
        session: &session(SessionType::Practice),
        laps: &laps,
        traversals: &trav,
        geometry: None,
        theoretical_best_ms: None,
        diagnoses: &[],
    };
    let s = build_summary(&input, &SummaryConfig::default(), at()).unwrap();

    let bc = s
        .behavior_changes
        .iter()
        .find(|c| c.metric == "brake point")
        .expect("brake-point shift detected");
    assert!(bc.metric_delta > 0.0, "brake point moved later (positive)");
    assert!(
        bc.segment_time_delta_ms < 0,
        "segment time fell as it moved later"
    );
    assert_eq!(bc.verdict, "worked, keep it");
    assert!(bc.text.contains("later"));
}

#[test]
fn race_prints_caveat_practice_does_not() {
    let laps = vec![
        lap("l1", 100_000, true, false),
        lap("l2", 99_500, true, false),
        lap("l3", 99_000, true, false),
    ];
    let make = |stype| {
        let sess = session(stype);
        let input = SummaryInputs {
            session: &sess,
            laps: &laps,
            traversals: &[],
            geometry: None,
            theoretical_best_ms: None,
            diagnoses: &[],
        };
        build_summary(&input, &SummaryConfig::default(), at()).unwrap()
    };

    let race = make(SessionType::Race);
    assert!(race.race_caveat);
    assert!(render_markdown(&race).contains("NOTE (race)"));

    let practice = make(SessionType::Practice);
    assert!(!practice.race_caveat);
    assert!(!render_markdown(&practice).contains("NOTE (race)"));
}
