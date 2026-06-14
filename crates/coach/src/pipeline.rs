//! End-to-end offline pipeline: a recorded session → diagnoses + summary.
//!
//! Wires the crates in build-order (spec §13): recorder → resampler → profiler →
//! reference → diagnosis → storage → post-event summary. This is the thin glue
//! that the `analyze` subcommand runs; every step is implemented (and tested) in
//! its own crate.

use anyhow::{anyhow, Context, Result};
use chrono::{Duration, Utc};
use coach_analysis::{
    extract_segment_metrics, resample_lap, Diagnoser, ReferenceModel, TrackProfiler,
};
use coach_core::config::Config;
use coach_core::diagnosis::Diagnosis;
use coach_core::grid::DistanceGrid;
use coach_core::ids::{SegmentId, TrackCar};
use coach_core::segment::TrackGeometry;
use coach_core::session::SessionType;
use coach_storage::{
    ActionPointsRow, LapRow, SectorBestRow, SegmentTraversalRow, SessionRow, SqliteStore, Store,
};
use coach_summary::{build_summary, write_artifacts, SummaryInputs};
use coach_telemetry::recorder::{LapRecorder, RawLap};
use coach_telemetry::{RecordedSource, Recording, TelemetrySource};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What `analyze` produced.
pub struct Outcome {
    pub session_id: String,
    pub clean_laps: usize,
    pub md_path: PathBuf,
    pub json_path: PathBuf,
}

struct Lap {
    raw: RawLap,
    trace: coach_core::trace::LapTrace,
    lap_time_ms: i64,
}

pub fn analyze(
    recording: Recording,
    db_path: &Path,
    report_dir: &Path,
    cfg: &Config,
) -> Result<Outcome> {
    let spline = recording.static_info.track_spline_length_m;
    if spline <= 0.0 {
        return Err(anyhow!("recording has non-positive track spline length"));
    }
    let track = recording.static_info.track.clone();
    let car = recording.static_info.car_model.clone();
    let track_car = TrackCar::new(track.as_str(), car.as_str());
    let grid = DistanceGrid::with_step(spline, cfg.grid_step_m);
    let session_type = recording
        .frames
        .first()
        .map(|f| SessionType::from_ac_flag(f.session))
        .unwrap_or(SessionType::Other);

    // 1) Recorder → raw laps.
    let mut recorder = LapRecorder::new(cfg.telemetry.clone(), spline);
    let mut raw_laps = Vec::new();
    let mut src = RecordedSource::new(recording);
    while let Some(f) = src.poll() {
        if let Some(lap) = recorder.feed(f) {
            raw_laps.push(lap);
        }
    }
    if let Some(lap) = recorder.finish() {
        raw_laps.push(lap);
    }

    // 2) Resample each lap onto the distance grid; derive lap time.
    let laps: Vec<Lap> = raw_laps
        .into_iter()
        .filter_map(|raw| {
            let trace = resample_lap(&raw.frames, spline, grid).ok()?;
            let lap_time_ms = lap_time_ms(&raw);
            Some(Lap {
                raw,
                trace,
                lap_time_ms,
            })
        })
        .collect();

    // 3) Track profile from the first N clean laps (spec §5.1).
    let mut profiler = TrackProfiler::new(track_car.track_id.clone(), cfg.profiler.clone());
    let mut geometry: Option<TrackGeometry> = None;
    for lap in &laps {
        if lap.raw.valid {
            if let Some(g) = profiler.add_clean_lap(lap.trace.clone()) {
                geometry = Some(g);
                break;
            }
        }
    }
    let geometry = geometry.ok_or_else(|| {
        anyhow!(
            "not enough clean laps to build a track profile (need {})",
            cfg.profiler.profile_laps
        )
    })?;

    // 4) Persist + reference + diagnosis, in lap order.
    let store = SqliteStore::open(db_path).context("opening database")?;
    store.upsert_track_geometry(&geometry)?;

    let started_at = Utc::now();
    let session_id = format!("{}_{}_{}", slug(&track), slug(&car), started_at.timestamp());
    store.upsert_session(&SessionRow {
        session_id: session_id.clone().into(),
        track_id: track_car.track_id.clone(),
        car_id: track_car.car_id.clone(),
        session_type,
        started_at,
        ended_at: None,
        summary_md_path: None,
        summary_json_path: None,
    })?;

    let mut reference =
        ReferenceModel::new(track_car.clone(), geometry.clone(), cfg.reference.clone());
    let mut diagnoser = Diagnoser::new(cfg.diagnosis.clone());
    let mut diagnoses: Vec<Diagnosis> = Vec::new();
    let mut clean_laps = 0usize;

    for (i, lap) in laps.iter().enumerate() {
        let lap_id = format!("{session_id}_lap{i}");
        let recorded_at = started_at + Duration::milliseconds(i as i64 * 1000);
        let seg_valid = segment_validity(&geometry, lap, spline);

        store.insert_lap(&LapRow {
            lap_id: lap_id.clone().into(),
            session_id: session_id.clone().into(),
            track_id: track_car.track_id.clone(),
            car_id: track_car.car_id.clone(),
            lap_time_ms: lap.lap_time_ms,
            is_valid: lap.raw.valid,
            is_in_out_lap: lap.raw.in_out_lap,
            is_outlier: false,
            recorded_at,
            trace: Some(lap.trace.clone()),
        })?;

        // Per-segment behavior metrics for every segment (powers the §10 summary).
        for seg in &geometry.segments {
            let m = extract_segment_metrics(seg, &lap.trace, &cfg.reference);
            store.insert_segment_traversal(&SegmentTraversalRow {
                lap_id: lap_id.clone().into(),
                segment_id: seg.segment_id,
                time_ms: m.time_ms,
                is_valid: *seg_valid.get(&seg.segment_id).unwrap_or(&false),
                entry_speed_kmh: m.entry_speed_kmh,
                brake_point_dist_m: m.brake_point_dist_m,
                turn_in_dist_m: m.turn_in_dist_m,
                apex_speed_kmh: Some(m.apex_speed_kmh),
                min_speed_kmh: Some(m.min_speed_kmh),
                throttle_on_dist_m: m.throttle_on_dist_m,
            })?;
        }

        // Fold into the reference (segment-level gating, spec §5.3).
        if !lap.raw.teleported {
            reference.consider_lap(
                &lap_id.clone().into(),
                &lap.trace,
                &seg_valid,
                lap.raw.valid,
                lap.lap_time_ms,
            );
        }

        if lap.raw.valid {
            clean_laps += 1;
            if reference.is_complete() {
                let analysis = diagnoser.diagnose(&lap_id.into(), &lap.trace, &reference);
                diagnoses.push(analysis.diagnosis);
            }
        }
    }

    // Persist the matured reference (sector bests + action points) for next time.
    persist_reference(&store, &track_car, &reference)?;

    // 5) Post-event summary (spec §10).
    let session_row = store
        .get_session(&session_id.clone().into())?
        .ok_or_else(|| anyhow!("session row vanished"))?;
    let lap_rows = store.laps_for_session(&session_id.clone().into(), false)?;
    let traversals = store.segment_traversals_for_session(&session_id.clone().into())?;
    let inputs = SummaryInputs {
        session: &session_row,
        laps: &lap_rows,
        traversals: &traversals,
        geometry: Some(&geometry),
        theoretical_best_ms: reference.theoretical_best_ms(),
        diagnoses: &diagnoses,
    };
    let summary = build_summary(&inputs, &cfg.summary, Utc::now())?;
    let (md_path, json_path) = write_artifacts(&summary, report_dir)?;

    // Record artifact paths + end time on the session.
    store.upsert_session(&SessionRow {
        ended_at: Some(Utc::now()),
        summary_md_path: Some(md_path.to_string_lossy().into_owned()),
        summary_json_path: Some(json_path.to_string_lossy().into_owned()),
        ..session_row
    })?;

    Ok(Outcome {
        session_id,
        clean_laps,
        md_path,
        json_path,
    })
}

/// Lap time from AC's reported `iLastTime`, falling back to the recorded frame
/// span when AC didn't report one (e.g. synthetic fixtures).
fn lap_time_ms(raw: &RawLap) -> i64 {
    if raw.lap_time_ms > 0 {
        return raw.lap_time_ms as i64;
    }
    match (raw.frames.first(), raw.frames.last()) {
        (Some(a), Some(b)) => ((b.t_us.saturating_sub(a.t_us)) / 1000) as i64,
        _ => 0,
    }
}

/// Segment-level clean gating (spec §5.3): a segment is valid for a lap if no
/// frame within its distance range was off-track or penalized.
fn segment_validity(geometry: &TrackGeometry, lap: &Lap, spline: f64) -> BTreeMap<SegmentId, bool> {
    let mut valid: BTreeMap<SegmentId, bool> = geometry
        .segments
        .iter()
        .map(|s| (s.segment_id, true))
        .collect();
    if lap.raw.teleported {
        // Teleported laps are excluded entirely (spec §4.3).
        for v in valid.values_mut() {
            *v = false;
        }
        return valid;
    }
    for f in &lap.raw.frames {
        if f.is_off_track() || f.penalty {
            let d = f.normalized_car_position as f64 * spline;
            if let Some(seg) = geometry.segment_at(d) {
                valid.insert(seg.segment_id, false);
            }
        }
    }
    valid
}

fn persist_reference(
    store: &SqliteStore,
    track_car: &TrackCar,
    reference: &ReferenceModel,
) -> Result<()> {
    for best in reference.segment_bests.values() {
        store.upsert_sector_best(&SectorBestRow {
            track_id: track_car.track_id.clone(),
            car_id: track_car.car_id.clone(),
            segment_id: best.segment_id,
            best_time_ms: best.time_ms,
            source_lap_id: best.source_lap_id.clone(),
            entry_speed_kmh: best.entry_speed_kmh,
            trace: best.trace.clone(),
            time_variance_ms: best.time_variance_ms(),
        })?;
    }
    for (seg, points) in &reference.action_points {
        store.upsert_action_points(&ActionPointsRow {
            track_id: track_car.track_id.clone(),
            car_id: track_car.car_id.clone(),
            points: coach_core::action_points::ActionPoints {
                segment_id: *seg,
                ..*points
            },
        })?;
    }
    Ok(())
}

fn slug(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}
