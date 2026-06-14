//! Diagnosis engine (spec §7, build step §13.4).
//!
//! Per lap, attributes time loss to a specific corner **and** cause, emitting the
//! structured [`Diagnosis`] object (spec §12.2) — never prose. Stability comes
//! from three mechanisms (spec §7.3): a persistence filter, direction
//! hysteresis, and a noise floor with two magnitude buckets. The engine is
//! stateful across laps so it can apply persistence and hysteresis.

use crate::align::{segment_delta, SegmentDelta};
use crate::metrics::{extract_segment_metrics, SegmentMetrics};
use crate::reference::ReferenceModel;
use coach_core::config::DiagnosisConfig;
use coach_core::diagnosis::{Diagnosis, ErrorKind, Finding, FindingDetail, Magnitude, Phase};
use coach_core::ids::{LapId, SegmentId};
use coach_core::segment::SegmentType;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const APEX_SPEED_NOISE_KMH: f64 = 2.0;
const APEX_SPEED_MUCH_KMH: f64 = 6.0;

/// One lap's full analysis: the diagnosis plus the per-segment behavior metrics
/// (which the caller persists to `segment_traversals` for the §10 summary).
pub struct LapAnalysis {
    pub diagnosis: Diagnosis,
    pub metrics: Vec<SegmentMetrics>,
}

/// Stateful per-`(track, car)` diagnoser.
pub struct Diagnoser {
    cfg: DiagnosisConfig,
    /// Per segment: the last `window-1` laps' finding signature (None = clean).
    history: BTreeMap<SegmentId, VecDeque<Option<(Phase, ErrorKind)>>>,
    /// Per segment: last spoken error and the (valid) lap index it was spoken on.
    last_spoken: BTreeMap<SegmentId, (ErrorKind, usize)>,
    /// Segments coached on the previous lap (to detect newly fixed ones).
    previously_coached: BTreeSet<SegmentId>,
    valid_laps_seen: usize,
}

impl Diagnoser {
    pub fn new(cfg: DiagnosisConfig) -> Self {
        Self {
            cfg,
            history: BTreeMap::new(),
            last_spoken: BTreeMap::new(),
            previously_coached: BTreeSet::new(),
            valid_laps_seen: 0,
        }
    }

    /// Diagnose one **valid** lap against the reference.
    pub fn diagnose(
        &mut self,
        lap_id: &LapId,
        current: &coach_core::trace::LapTrace,
        reference: &ReferenceModel,
    ) -> LapAnalysis {
        self.valid_laps_seen += 1;
        let lap_idx = self.valid_laps_seen;

        let mut findings: Vec<Finding> = Vec::new();
        let mut metrics: Vec<SegmentMetrics> = Vec::new();
        let mut total_delta_ms: i64 = 0;
        // Segment → (delta, error) for those that produced a finding this lap.
        let mut eligible: Vec<(SegmentId, i64, ErrorKind)> = Vec::new();
        let mut has_finding: BTreeSet<SegmentId> = BTreeSet::new();

        for seg in &reference.geometry.segments {
            // Per-lap metrics are always recorded (for the summary), even without
            // a usable reference for this segment.
            let cm = extract_segment_metrics(seg, current, &ReferenceConfigShim::cfg());
            metrics.push(cm);

            let entry_speed = cm.entry_speed_kmh;
            let Some((ref_trace, measured_against)) =
                reference.reference_for_segment(seg.segment_id, entry_speed)
            else {
                self.push_history(seg.segment_id, None);
                continue;
            };
            let delta = segment_delta(seg, current, ref_trace, &ReferenceConfigShim::cfg());
            total_delta_ms += delta.time_lost_ms;

            let classified = classify(seg.seg_type, &delta, &self.cfg);
            let Some((error, magnitude, detail)) = classified else {
                self.push_history(seg.segment_id, None);
                continue;
            };
            // Noise floor: only coach losses above the per-segment-type floor.
            if delta.time_lost_ms < self.cfg.noise_floor_ms {
                self.push_history(seg.segment_id, None);
                continue;
            }

            let phase = error.phase();
            let persistence_count = self.persistence_count(seg.segment_id, phase, error);
            self.push_history(seg.segment_id, Some((phase, error)));
            has_finding.insert(seg.segment_id);

            findings.push(Finding {
                segment_id: seg.segment_id,
                segment_label: reference.geometry.label_for(seg.segment_id),
                phase,
                time_lost_ms: delta.time_lost_ms,
                error,
                magnitude,
                persistence_count,
                measured_against,
                detail,
            });

            // Eligible to *speak*: persistence met (1-of-1 on the first coached
            // lap), and not reversed within the hysteresis window.
            let min_of = if lap_idx == 1 {
                1
            } else {
                self.cfg.persistence_min_of as u32
            };
            let reversed = self
                .last_spoken
                .get(&seg.segment_id)
                .map(|(prev, plap)| {
                    is_opposite(*prev, error) && (lap_idx - plap) < self.cfg.hysteresis_laps
                })
                .unwrap_or(false);
            if persistence_count >= min_of && !reversed {
                eligible.push((seg.segment_id, delta.time_lost_ms, error));
            }
        }

        // Rank worst-first; only the worst N corners are spoken (spec §7.2).
        eligible.sort_by_key(|e| std::cmp::Reverse(e.1));
        eligible.truncate(self.cfg.max_coached_corners);
        let ranked_priority: Vec<SegmentId> = eligible.iter().map(|e| e.0).collect();
        for (seg, _, error) in &eligible {
            self.last_spoken.insert(*seg, (*error, lap_idx));
        }

        // Fixed: previously coached corners with no finding above the floor now.
        let fixed: Vec<SegmentId> = self
            .previously_coached
            .iter()
            .filter(|s| !has_finding.contains(s))
            .copied()
            .collect();
        self.previously_coached = ranked_priority.iter().copied().collect();

        LapAnalysis {
            diagnosis: Diagnosis {
                lap_id: lap_id.clone(),
                track_id: reference.track_car.track_id.clone(),
                car_id: reference.track_car.car_id.clone(),
                total_delta_ms,
                findings,
                fixed,
                ranked_priority,
            },
            metrics,
        }
    }

    fn persistence_count(&self, seg: SegmentId, phase: Phase, error: ErrorKind) -> u32 {
        let prev = self
            .history
            .get(&seg)
            .map(|dq| dq.iter().filter(|f| **f == Some((phase, error))).count())
            .unwrap_or(0);
        (prev + 1) as u32
    }

    fn push_history(&mut self, seg: SegmentId, entry: Option<(Phase, ErrorKind)>) {
        let dq = self.history.entry(seg).or_default();
        dq.push_back(entry);
        // Keep only the previous `window - 1` laps (current is counted live).
        while dq.len() >= self.cfg.persistence_window {
            dq.pop_front();
        }
    }
}

/// Shim so metric extraction inside the diagnoser uses a default `ReferenceConfig`
/// (anchor thresholds are reference-layer concerns, not diagnosis tunables).
struct ReferenceConfigShim;
impl ReferenceConfigShim {
    fn cfg() -> coach_core::config::ReferenceConfig {
        coach_core::config::ReferenceConfig::default()
    }
}

/// Classify a segment's dominant error using **earliest-phase attribution**
/// (spec §7.2): check Entry, then Mid, then Exit, and return the first phase
/// that shows an above-threshold error.
fn classify(
    seg_type: SegmentType,
    d: &SegmentDelta,
    cfg: &DiagnosisConfig,
) -> Option<(ErrorKind, Magnitude, FindingDetail)> {
    let lo = cfg.brake_bucket_lo_m;
    let hi = cfg.brake_bucket_hi_m;
    let dist_mag = |delta_abs: f64| {
        if delta_abs > hi {
            Magnitude::Much
        } else {
            Magnitude::Slight
        }
    };

    // Straights don't get corner coaching.
    if seg_type == SegmentType::Straight {
        return None;
    }

    // --- Entry ---
    if let Some(bp) = d.brake_point_delta_m {
        if bp.abs() >= lo {
            let error = if bp < 0.0 {
                ErrorKind::BrakeTooEarly // brakes sooner than reference → brake later
            } else {
                ErrorKind::BrakeTooLate
            };
            let detail = FindingDetail {
                brake_point_delta_m: Some(bp),
                ..Default::default()
            };
            return Some((error, dist_mag(bp.abs()), detail));
        }
    }

    // --- Mid ---
    if let Some(asd) = d.apex_speed_delta_kmh {
        if asd <= -APEX_SPEED_NOISE_KMH {
            let mag = if asd.abs() > APEX_SPEED_MUCH_KMH {
                Magnitude::Much
            } else {
                Magnitude::Slight
            };
            let detail = FindingDetail {
                apex_speed_delta_kmh: Some(asd),
                line_offset_m: Some(d.line_offset_m),
                ..Default::default()
            };
            return Some((ErrorKind::ApexSpeedDeficit, mag, detail));
        }
    }
    if let Some(ad) = d.apex_dist_delta_m {
        if ad.abs() >= lo {
            let error = if ad < 0.0 {
                ErrorKind::EarlyApex
            } else {
                ErrorKind::LateApex
            };
            let detail = FindingDetail {
                line_offset_m: Some(d.line_offset_m),
                ..Default::default()
            };
            return Some((error, dist_mag(ad.abs()), detail));
        }
    }

    // --- Exit ---
    if let Some(td) = d.throttle_on_delta_m {
        if td >= lo {
            let detail = FindingDetail {
                throttle_on_delta_m: Some(td),
                ..Default::default()
            };
            return Some((ErrorKind::ThrottleTooLate, dist_mag(td.abs()), detail));
        }
    }

    None
}

/// Opposite advice directions (spec §7.3 hysteresis): reversing within the
/// window goes silent instead.
fn is_opposite(a: ErrorKind, b: ErrorKind) -> bool {
    use ErrorKind::*;
    matches!(
        (a, b),
        (BrakeTooEarly, BrakeTooLate)
            | (BrakeTooLate, BrakeTooEarly)
            | (BrakeTooHard, BrakeTooSoft)
            | (BrakeTooSoft, BrakeTooHard)
            | (EarlyApex, LateApex)
            | (LateApex, EarlyApex)
    )
}
