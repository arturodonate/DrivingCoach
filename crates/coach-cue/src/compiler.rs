//! Cue compiler & arbiter (spec §8, §9.1, §9.2, build step §13.5).
//!
//! Turns a [`Diagnosis`] into a [`CoachingPlan`]: render each ranked finding to a
//! template, synthesize/cache its WAV, back-integrate the reference speed profile
//! to a trigger distance (spec §9.1), and resolve overlaps with the arbiter
//! (one voice, compile-time overlap drop, chicane merge, minimum gap — spec §9.2).

use crate::cache::WavCache;
use crate::error::Result;
use crate::templates::{confirmation, render_finding};
use coach_core::action_points::ActionPoints;
use coach_core::config::Config;
use coach_core::cue::{CoachingPlan, Cue, CueKind};
use coach_core::diagnosis::{Diagnosis, Phase};
use coach_core::ids::SegmentId;
use coach_core::segment::{Segment, TrackGeometry};
use coach_core::trace::LapTrace;
use std::collections::BTreeMap;

/// Compiles plans against a fixed config and WAV cache.
pub struct CueCompiler<'a> {
    cfg: &'a Config,
    cache: &'a WavCache<'a>,
}

/// A cue under construction, before arbitration.
struct PendingCue {
    segment_id: SegmentId,
    action_dist_m: f64,
    text: String,
    kind: CueKind,
    magnitude: Option<coach_core::diagnosis::Magnitude>,
    priority: u32,
}

impl<'a> CueCompiler<'a> {
    pub fn new(cfg: &'a Config, cache: &'a WavCache<'a>) -> Self {
        Self { cfg, cache }
    }

    /// Compile the plan for the next lap from this lap's diagnosis.
    pub fn compile(
        &self,
        diagnosis: &Diagnosis,
        geometry: &TrackGeometry,
        action_points: &BTreeMap<SegmentId, ActionPoints>,
        ref_speed: &LapTrace,
    ) -> Result<CoachingPlan> {
        let mut pending: Vec<PendingCue> = Vec::new();

        // Corrective cues for the ranked (worst 1–2) corners.
        for (rank, seg_id) in diagnosis.ranked_priority.iter().enumerate() {
            let Some(finding) = diagnosis.findings.iter().find(|f| f.segment_id == *seg_id) else {
                continue;
            };
            let Some(seg) = geometry.segments.iter().find(|s| s.segment_id == *seg_id) else {
                continue;
            };
            let r = render_finding(finding);
            let ap = action_points.get(seg_id).copied().unwrap_or_default();
            pending.push(PendingCue {
                segment_id: *seg_id,
                action_dist_m: action_dist(finding.phase, r.kind, &ap, seg),
                text: r.text,
                kind: r.kind,
                magnitude: Some(finding.magnitude),
                priority: (rank as u32) + 1,
            });
        }

        // Chicane merge: if the two coached corners are very close, combine them.
        self.maybe_merge_chicane(&mut pending);

        // Confirmation cues for newly fixed corners (lower priority than advice).
        for (i, seg_id) in diagnosis.fixed.iter().enumerate() {
            let Some(seg) = geometry.segments.iter().find(|s| s.segment_id == *seg_id) else {
                continue;
            };
            let ap = action_points.get(seg_id).copied().unwrap_or_default();
            pending.push(PendingCue {
                segment_id: *seg_id,
                action_dist_m: ap.turn_in_dist_m.unwrap_or(seg.start_dist_m),
                text: confirmation(&geometry.label_for(*seg_id)),
                kind: CueKind::Confirmation,
                magnitude: None,
                priority: 50 + i as u32,
            });
        }

        // Synthesize/cache, back-integrate triggers, build cues.
        let mut cues: Vec<Cue> = Vec::with_capacity(pending.len());
        for p in pending {
            let clip = self.cache.get_or_synth(&p.text)?;
            let lead_s = clip.duration_s + self.cfg.cue.reaction_time_s;
            let trigger_dist_m = back_integrate_trigger(ref_speed, p.action_dist_m, lead_s);
            cues.push(Cue {
                segment_id: p.segment_id,
                action_point_dist_m: p.action_dist_m,
                trigger_dist_m,
                speech_duration_s: clip.duration_s,
                template: p.text,
                magnitude: p.magnitude,
                audio_clip_id: clip.clip_id,
                priority: p.priority,
                kind: p.kind,
            });
        }

        // Arbiter: resolve overlaps and enforce the minimum gap (spec §9.2).
        let cues = self.arbitrate(cues, ref_speed);

        Ok(CoachingPlan {
            track_id: diagnosis.track_id.clone(),
            car_id: diagnosis.car_id.clone(),
            source_lap_id: Some(diagnosis.lap_id.clone()),
            cues,
        })
    }

    /// If the two ranked corrective cues' action points fall within
    /// `chicane_merge_m`, replace them with one combined "chicane" cue.
    fn maybe_merge_chicane(&self, pending: &mut Vec<PendingCue>) {
        let corrections: Vec<usize> = pending
            .iter()
            .enumerate()
            .filter(|(_, p)| p.kind == CueKind::Correction)
            .map(|(i, _)| i)
            .collect();
        if corrections.len() != 2 {
            return;
        }
        let (a, b) = (corrections[0], corrections[1]);
        if (pending[a].action_dist_m - pending[b].action_dist_m).abs()
            > self.cfg.cue.chicane_merge_m
        {
            return;
        }
        // Keep the higher-priority (lower number) cue's phrase, earliest action.
        let (keep, drop) = if pending[a].priority <= pending[b].priority {
            (a, b)
        } else {
            (b, a)
        };
        let phrase = pending[keep]
            .text
            .split(" — ")
            .nth(1)
            .unwrap_or(&pending[keep].text)
            .to_string();
        let merged = PendingCue {
            segment_id: pending[keep].segment_id,
            action_dist_m: pending[a].action_dist_m.min(pending[b].action_dist_m),
            text: format!("chicane — {phrase}"),
            kind: CueKind::Correction,
            magnitude: pending[keep].magnitude,
            priority: pending[keep].priority.min(pending[drop].priority),
        };
        // Remove both originals (drop the higher index first) and push the merge.
        let (hi, lo) = if a > b { (a, b) } else { (b, a) };
        pending.remove(hi);
        pending.remove(lo);
        pending.push(merged);
    }

    /// One voice, no overlaps: accept cues highest-priority first, dropping any
    /// that would overlap (within the minimum gap) an already-accepted cue.
    fn arbitrate(&self, mut cues: Vec<Cue>, ref_speed: &LapTrace) -> Vec<Cue> {
        cues.sort_by_key(|c| c.priority);
        let mut accepted: Vec<Cue> = Vec::new();
        for c in cues {
            let gap_m = self.cfg.cue.min_gap_s * speed_at(ref_speed, c.trigger_dist_m) / 3.6;
            let conflicts = accepted.iter().any(|a| {
                // Occupied window ≈ [trigger, action]; require a gap between them.
                let (s1, e1) = (c.trigger_dist_m, c.action_point_dist_m);
                let (s2, e2) = (a.trigger_dist_m, a.action_point_dist_m);
                s1 < e2 + gap_m && s2 < e1 + gap_m
            });
            if !conflicts {
                accepted.push(c);
            }
        }
        accepted.sort_by(|a, b| a.trigger_dist_m.partial_cmp(&b.trigger_dist_m).unwrap());
        accepted
    }
}

/// Pick the action point a cue must finish before, by phase / kind.
fn action_dist(phase: Phase, kind: CueKind, ap: &ActionPoints, seg: &Segment) -> f64 {
    let mid = 0.5 * (seg.start_dist_m + seg.end_dist_m);
    if kind == CueKind::Apex {
        return ap.turn_in_dist_m.or(ap.apex_dist_m).unwrap_or(mid);
    }
    match phase {
        Phase::Entry => ap
            .brake_point_dist_m
            .or(ap.turn_in_dist_m)
            .unwrap_or(seg.start_dist_m),
        Phase::Mid => ap.turn_in_dist_m.or(ap.apex_dist_m).unwrap_or(mid),
        Phase::Exit => ap
            .throttle_on_dist_m
            .or(ap.apex_dist_m)
            .unwrap_or(seg.end_dist_m),
    }
}

/// Back-integrate the reference speed profile from `action_dist_m` until
/// `lead_s` (speech + reaction) of time has elapsed; that distance is the
/// trigger point (spec §9.1).
pub fn back_integrate_trigger(ref_speed: &LapTrace, action_dist_m: f64, lead_s: f64) -> f64 {
    let grid = ref_speed.grid;
    let mut i = grid.index_of(action_dist_m);
    let mut acc = 0.0_f64;
    while i > 0 && acc < lead_s {
        let v = (ref_speed.speed_kmh[i] as f64 / 3.6).max(0.5);
        acc += grid.step_m / v;
        i -= 1;
    }
    grid.dist_at(i)
}

fn speed_at(trace: &LapTrace, dist_m: f64) -> f64 {
    let i = trace.grid.index_of(dist_m);
    (trace.speed_kmh[i] as f64).max(1.0)
}
