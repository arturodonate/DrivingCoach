//! Builds the post-event [`Summary`] deterministically from persisted data
//! (spec §10) — no model of any kind.

use crate::error::{Result, SummaryError};
use crate::model::*;
use crate::stats::{mad, median, std_dev, theil_sen_slope};
use chrono::{DateTime, Utc};
use coach_core::config::SummaryConfig;
use coach_core::diagnosis::Diagnosis;
use coach_core::ids::SegmentId;
use coach_core::segment::TrackGeometry;
use coach_storage::models::{LapRow, SegmentTraversalRow, SessionRow};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const TREND_DEADBAND_MS_PER_LAP: f64 = 15.0;

/// Everything the builder needs (all persisted; see spec §10 sources).
pub struct SummaryInputs<'a> {
    pub session: &'a SessionRow,
    /// Laps for the session, ordered by `recorded_at`.
    pub laps: &'a [LapRow],
    pub traversals: &'a [SegmentTraversalRow],
    pub geometry: Option<&'a TrackGeometry>,
    pub theoretical_best_ms: Option<i64>,
    /// Accumulated per-lap diagnoses (for the coaching-response block).
    pub diagnoses: &'a [Diagnosis],
}

pub fn build_summary(
    input: &SummaryInputs<'_>,
    cfg: &SummaryConfig,
    generated_at: DateTime<Utc>,
) -> Result<Summary> {
    let label_for = |seg: SegmentId| -> String {
        input
            .geometry
            .map(|g| g.label_for(seg))
            .unwrap_or_else(|| format!("Segment {seg}"))
    };

    // In/out laps are excluded from all trend/behavior analysis (spec §10.2).
    let in_out_excluded = input.laps.iter().filter(|l| l.is_in_out_lap).count();

    // Candidate laps for analysis: valid and not in/out.
    let candidates: Vec<&LapRow> = input
        .laps
        .iter()
        .filter(|l| l.is_valid && !l.is_in_out_lap)
        .collect();
    if candidates.is_empty() {
        return Err(SummaryError::NoData("no valid non-in/out laps".into()));
    }

    // Outlier rejection on the candidate times: time > median + k·MAD (spec §10.2).
    let times: Vec<f64> = candidates.iter().map(|l| l.lap_time_ms as f64).collect();
    let med = median(&times).unwrap();
    let mad_v = mad(&times).unwrap_or(0.0);
    let outlier_cut = med + cfg.outlier_mad_k * mad_v;
    let is_outlier =
        |l: &LapRow| (mad_v > 0.0 && (l.lap_time_ms as f64) > outlier_cut) || l.is_outlier;

    let clean: Vec<&LapRow> = candidates
        .iter()
        .copied()
        .filter(|l| !is_outlier(l))
        .collect();
    let outlier_excluded = candidates.len() - clean.len();
    if clean.is_empty() {
        return Err(SummaryError::NoData("all laps rejected as outliers".into()));
    }

    // Raw, all-lap listing (nothing hidden, spec §10.2).
    let all_laps: Vec<LapTimeEntry> = input
        .laps
        .iter()
        .enumerate()
        .map(|(i, l)| LapTimeEntry {
            lap_number: i + 1,
            time_ms: l.lap_time_ms,
            valid: l.is_valid,
            in_out_lap: l.is_in_out_lap,
            outlier: is_outlier(l),
        })
        .collect();

    let pace = build_pace(&clean, input.theoretical_best_ms);
    let behavior_changes = build_behavior_changes(&clean, input.traversals, cfg, &label_for);
    let coaching = build_coaching(input.diagnoses, &label_for);
    let consistency = build_consistency(&clean, input.traversals, &label_for);

    Ok(Summary {
        track: input.session.track_id.to_string(),
        car: input.session.car_id.to_string(),
        session_type: input.session.session_type,
        clean_lap_count: clean.len(),
        in_out_excluded,
        outlier_excluded,
        pace,
        behavior_changes,
        coaching,
        consistency,
        all_laps,
        race_caveat: input.session.session_type.has_fuel_tyre_caveat(),
        generated_at: generated_at.to_rfc3339(),
    })
}

fn build_pace(clean: &[&LapRow], theoretical_best_ms: Option<i64>) -> Pace {
    let times: Vec<i64> = clean.iter().map(|l| l.lap_time_ms).collect();
    let first = times[0];
    let last = *times.last().unwrap();
    let best = *times.iter().min().unwrap();
    let med = median(&times.iter().map(|&t| t as f64).collect::<Vec<_>>()).unwrap() as i64;

    let xs: Vec<f64> = (0..times.len()).map(|i| i as f64).collect();
    let ys: Vec<f64> = times.iter().map(|&t| t as f64).collect();
    let slope = theil_sen_slope(&xs, &ys).unwrap_or(0.0);
    let trend = if slope <= -TREND_DEADBAND_MS_PER_LAP {
        Trend::Improving
    } else if slope >= TREND_DEADBAND_MS_PER_LAP {
        Trend::Regressing
    } else {
        Trend::Plateaued
    };

    Pace {
        first_ms: first,
        best_ms: best,
        last_ms: last,
        median_ms: med,
        best_minus_first_ms: best - first,
        last_minus_first_ms: last - first,
        trend,
        theoretical_best_ms,
    }
}

fn build_behavior_changes(
    clean: &[&LapRow],
    traversals: &[SegmentTraversalRow],
    cfg: &SummaryConfig,
    label_for: &impl Fn(SegmentId) -> String,
) -> Vec<BehaviorChange> {
    let m = clean.len();
    let order: HashMap<&str, usize> = clean
        .iter()
        .enumerate()
        .map(|(i, l)| (l.lap_id.as_str(), i))
        .collect();
    let third = m.div_ceil(3).max(1);

    // Group clean traversals by segment, tagged with their clean-lap order.
    let mut by_seg: BTreeMap<SegmentId, Vec<(usize, &SegmentTraversalRow)>> = BTreeMap::new();
    for t in traversals {
        if let Some(&idx) = order.get(t.lap_id.as_str()) {
            by_seg.entry(t.segment_id).or_default().push((idx, t));
        }
    }

    let mut out = Vec::new();
    for (seg, rows) in by_seg {
        let in_first = |idx: usize| idx < third;
        let in_last = |idx: usize| idx >= m - third;

        let seg_time_first = med_of(&rows, in_first, |t| Some(t.time_ms as f64));
        let seg_time_last = med_of(&rows, in_last, |t| Some(t.time_ms as f64));
        let seg_time_delta_ms = match (seg_time_first, seg_time_last) {
            (Some(a), Some(b)) => (b - a) as i64,
            _ => 0,
        };

        for d in metric_descriptors(cfg) {
            let a = med_of(&rows, in_first, d.extract);
            let b = med_of(&rows, in_last, d.extract);
            let (Some(a), Some(b)) = (a, b) else { continue };
            let delta = b - a;
            if delta.abs() < d.threshold {
                continue;
            }
            out.push(make_change(
                seg,
                label_for(seg),
                d.name,
                d.unit,
                delta,
                seg_time_delta_ms,
                d.is_speed,
                cfg.segment_time_shift_ms,
            ));
        }
    }
    out
}

/// One behavior metric to track in the first-vs-last-third comparison.
struct MetricDescriptor {
    name: &'static str,
    unit: &'static str,
    extract: fn(&SegmentTraversalRow) -> Option<f64>,
    threshold: f64,
    is_speed: bool,
}

fn metric_descriptors(cfg: &SummaryConfig) -> [MetricDescriptor; 4] {
    [
        MetricDescriptor {
            name: "brake point",
            unit: "m",
            extract: |t| t.brake_point_dist_m,
            threshold: cfg.brake_point_shift_m,
            is_speed: false,
        },
        MetricDescriptor {
            name: "turn-in",
            unit: "m",
            extract: |t| t.turn_in_dist_m,
            threshold: cfg.turn_in_shift_m,
            is_speed: false,
        },
        MetricDescriptor {
            name: "apex speed",
            unit: "km/h",
            extract: |t| t.apex_speed_kmh,
            threshold: cfg.apex_speed_shift_kmh,
            is_speed: true,
        },
        MetricDescriptor {
            name: "throttle-on",
            unit: "m",
            extract: |t| t.throttle_on_dist_m,
            threshold: cfg.throttle_on_shift_m,
            is_speed: false,
        },
    ]
}

#[allow(clippy::too_many_arguments)]
fn make_change(
    seg: SegmentId,
    label: String,
    metric: &str,
    unit: &str,
    delta: f64,
    seg_time_delta_ms: i64,
    is_speed: bool,
    time_thresh_ms: f64,
) -> BehaviorChange {
    // Direction word for the metric movement.
    let dir = if is_speed {
        if delta > 0.0 {
            "more"
        } else {
            "less"
        }
    } else if delta > 0.0 {
        "later"
    } else {
        "earlier"
    };
    // Verdict from the concurrent segment-time movement (associative, §10.1b).
    let verdict = if (seg_time_delta_ms as f64) <= -time_thresh_ms {
        "worked, keep it"
    } else if (seg_time_delta_ms as f64) >= time_thresh_ms {
        "cost time, revert"
    } else {
        "steady"
    };
    let time_word = if seg_time_delta_ms < 0 {
        "fell"
    } else if seg_time_delta_ms > 0 {
        "rose"
    } else {
        "held"
    };
    let metric_phrase = if is_speed {
        format!("you carried ~{:.0} km/h {dir} apex speed", delta.abs())
    } else {
        format!("you moved your {metric} ~{:.0}{unit} {dir}", delta.abs())
    };
    let text = format!(
        "{label}: {metric_phrase} over the session. {label} segment time {time_word} \
         {:.2}s alongside it — {verdict}.",
        (seg_time_delta_ms.abs() as f64) / 1000.0
    );
    BehaviorChange {
        segment_id: seg,
        segment_label: label,
        metric: metric.to_string(),
        metric_delta: delta,
        metric_unit: unit.to_string(),
        segment_time_delta_ms: seg_time_delta_ms,
        verdict: verdict.to_string(),
        text,
    }
}

/// Median of an extracted metric over the rows whose order index passes `take`.
fn med_of(
    rows: &[(usize, &SegmentTraversalRow)],
    take: impl Fn(usize) -> bool,
    extract: impl Fn(&SegmentTraversalRow) -> Option<f64>,
) -> Option<f64> {
    let vals: Vec<f64> = rows
        .iter()
        .filter(|(i, _)| take(*i))
        .filter_map(|(_, t)| extract(t))
        .collect();
    median(&vals)
}

fn build_coaching(diagnoses: &[Diagnosis], label_for: &impl Fn(SegmentId) -> String) -> Coaching {
    let mut cued: BTreeSet<SegmentId> = BTreeSet::new();
    let mut fixed: BTreeSet<SegmentId> = BTreeSet::new();
    for d in diagnoses {
        cued.extend(d.ranked_priority.iter().copied());
        fixed.extend(d.fixed.iter().copied());
    }
    let to_vec = |set: BTreeSet<SegmentId>| {
        set.into_iter()
            .map(|s| LabeledSegment {
                segment_id: s,
                label: label_for(s),
            })
            .collect()
    };
    Coaching {
        cued: to_vec(cued),
        fixed: to_vec(fixed),
    }
}

fn build_consistency(
    clean: &[&LapRow],
    traversals: &[SegmentTraversalRow],
    label_for: &impl Fn(SegmentId) -> String,
) -> Consistency {
    let clean_ids: BTreeSet<&str> = clean.iter().map(|l| l.lap_id.as_str()).collect();
    let mut by_seg: BTreeMap<SegmentId, Vec<f64>> = BTreeMap::new();
    for t in traversals {
        if clean_ids.contains(t.lap_id.as_str()) {
            by_seg
                .entry(t.segment_id)
                .or_default()
                .push(t.time_ms as f64);
        }
    }
    let per_segment: Vec<SegmentVariance> = by_seg
        .into_iter()
        .map(|(seg, times)| SegmentVariance {
            segment_id: seg,
            label: label_for(seg),
            variance_ms: std_dev(&times),
        })
        .collect();
    let most_consistent = per_segment
        .iter()
        .min_by(|a, b| a.variance_ms.partial_cmp(&b.variance_ms).unwrap())
        .cloned();
    let least_consistent = per_segment
        .iter()
        .max_by(|a, b| a.variance_ms.partial_cmp(&b.variance_ms).unwrap())
        .cloned();
    Consistency {
        per_segment,
        most_consistent,
        least_consistent,
    }
}
