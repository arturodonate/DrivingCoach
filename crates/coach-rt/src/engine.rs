//! The 60 Hz cue engine (spec §9) — pure arithmetic, no allocation on the tick.
//!
//! This is the real-time core: trigger matching with the clamped live speed
//! correction (spec §9.1), the cue arbiter's runtime rules (one voice, drop-if-
//! late, minimum gap — spec §9.2), and crisis mute (spec §9.3). It is fully
//! cross-platform and unit-tested; only the audio *output* device (WASAPI) is
//! Windows-specific (see [`crate::audio`]).

use coach_core::config::CueConfig;
use coach_core::cue::{CoachingPlan, Cue};
use std::sync::Arc;

/// Conditions that silence all cues (spec §9.3). The caller derives these from
/// telemetry + the integrity monitor each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CrisisFlags {
    /// ≥2 tyres out.
    pub tyres_out: bool,
    /// Large backward position jump (spin).
    pub spin: bool,
    /// Sustained wheel slip beyond threshold.
    pub wheel_slip: bool,
    /// Speed < 30 km/h mid-track.
    pub slow_mid_track: bool,
    /// Telemetry stale.
    pub stale: bool,
}

impl CrisisFlags {
    pub fn any(self) -> bool {
        self.tyres_out || self.spin || self.wheel_slip || self.slow_mid_track || self.stale
    }
}

/// What the audio thread should play.
#[derive(Debug, Clone, PartialEq)]
pub struct FiredCue {
    pub segment_id: u32,
    pub audio_clip_id: String,
    pub template: String,
}

/// Per-tick inputs.
#[derive(Debug, Clone, Copy)]
pub struct Tick {
    /// Monotonic time, seconds.
    pub now_s: f64,
    /// Car distance-from-start, metres.
    pub position_m: f64,
    /// Current speed, m/s.
    pub v_now_ms: f64,
    /// Reference speed at this position, m/s (for the live trigger correction).
    pub v_ref_ms: f64,
}

/// Re-arm window after a crisis clears (spec §9.3).
const CRISIS_REARM_S: f64 = 3.0;

/// The cue engine. One per coaching session; the plan is swapped at the lap
/// boundary (spec §9.4).
pub struct CueEngine {
    cfg: CueConfig,
    plan: Arc<CoachingPlan>,
    next_idx: usize,
    /// Time the current cue finishes speaking (one-voice rule).
    busy_until_s: f64,
    /// Time the last cue finished (minimum-gap rule).
    last_cue_end_s: f64,
    /// Muted until this time (crisis re-arm).
    muted_until_s: f64,
}

impl CueEngine {
    pub fn new(cfg: CueConfig, plan: Arc<CoachingPlan>) -> Self {
        Self {
            cfg,
            plan,
            next_idx: 0,
            busy_until_s: f64::NEG_INFINITY,
            last_cue_end_s: f64::NEG_INFINITY,
            muted_until_s: f64::NEG_INFINITY,
        }
    }

    /// Swap in a new plan at the lap boundary and reset the cue cursor
    /// (spec §9.4). The caller applies the deadline fallback (keep the old plan
    /// if the new one is not ready).
    pub fn load_plan(&mut self, plan: Arc<CoachingPlan>) {
        self.plan = plan;
        self.next_idx = 0;
    }

    pub fn plan(&self) -> &CoachingPlan {
        &self.plan
    }

    /// Advance one tick. Returns a cue to play, if one fires now.
    pub fn tick(&mut self, t: Tick, crisis: CrisisFlags) -> Option<FiredCue> {
        // Crisis mute, with a 3 s re-arm after it clears (spec §9.3).
        if crisis.any() {
            self.muted_until_s = t.now_s + CRISIS_REARM_S;
            return None;
        }
        if t.now_s < self.muted_until_s {
            return None;
        }
        // One voice: never start a cue while one is playing (spec §9.2).
        if t.now_s < self.busy_until_s {
            return None;
        }

        while self.next_idx < self.plan.cues.len() {
            let cue = &self.plan.cues[self.next_idx];
            let corrected_trigger = self.corrected_trigger(cue, t.v_now_ms, t.v_ref_ms);

            // Drop-if-late: the action point is already behind us (spec §9.2).
            if t.position_m >= cue.action_point_dist_m {
                self.next_idx += 1;
                continue;
            }
            // Not yet at the (corrected) trigger — cues are ordered, so stop.
            if t.position_m < corrected_trigger {
                break;
            }
            // Minimum gap since the last cue ended (spec §9.2).
            if t.now_s - self.last_cue_end_s < self.cfg.min_gap_s {
                break;
            }

            // Fire it.
            self.busy_until_s = t.now_s + cue.speech_duration_s;
            self.last_cue_end_s = self.busy_until_s;
            self.next_idx += 1;
            return Some(FiredCue {
                segment_id: cue.segment_id,
                audio_clip_id: cue.audio_clip_id.clone(),
                template: cue.template.clone(),
            });
        }
        None
    }

    /// Live trigger correction: scale the precomputed lead by `v_now / v_ref`,
    /// clamped to ±`trigger_correction_clamp` (spec §9.1).
    fn corrected_trigger(&self, cue: &Cue, v_now_ms: f64, v_ref_ms: f64) -> f64 {
        let lead = cue.action_point_dist_m - cue.trigger_dist_m;
        let ratio = if v_ref_ms > 0.1 {
            (v_now_ms / v_ref_ms).clamp(
                1.0 - self.cfg.trigger_correction_clamp,
                1.0 + self.cfg.trigger_correction_clamp,
            )
        } else {
            1.0
        };
        cue.action_point_dist_m - lead * ratio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coach_core::cue::{CoachingPlan, CueKind};

    fn plan_with(cues: Vec<Cue>) -> Arc<CoachingPlan> {
        Arc::new(CoachingPlan {
            track_id: "t".into(),
            car_id: "c".into(),
            source_lap_id: None,
            cues,
        })
    }

    fn cue(seg: u32, trigger: f64, action: f64, dur: f64) -> Cue {
        Cue {
            segment_id: seg,
            action_point_dist_m: action,
            trigger_dist_m: trigger,
            speech_duration_s: dur,
            template: format!("Turn {seg} — brake later"),
            magnitude: None,
            audio_clip_id: format!("clip{seg}"),
            priority: 1,
            kind: CueKind::Correction,
        }
    }

    fn tick(now_s: f64, pos: f64) -> Tick {
        Tick {
            now_s,
            position_m: pos,
            v_now_ms: 50.0,
            v_ref_ms: 50.0,
        }
    }

    #[test]
    fn fires_once_when_crossing_trigger() {
        let mut e = CueEngine::new(
            CueConfig::default(),
            plan_with(vec![cue(5, 900.0, 1000.0, 1.0)]),
        );
        assert!(e.tick(tick(0.0, 800.0), CrisisFlags::default()).is_none());
        let fired = e
            .tick(tick(1.0, 905.0), CrisisFlags::default())
            .expect("fires");
        assert_eq!(fired.segment_id, 5);
        // Does not re-fire on the next tick.
        assert!(e.tick(tick(1.1, 920.0), CrisisFlags::default()).is_none());
    }

    #[test]
    fn drop_if_late_when_past_action_point() {
        let mut e = CueEngine::new(
            CueConfig::default(),
            plan_with(vec![cue(5, 900.0, 1000.0, 1.0)]),
        );
        // First observed position is already beyond the action point.
        assert!(e.tick(tick(0.0, 1010.0), CrisisFlags::default()).is_none());
        // And it never fires later either.
        assert!(e.tick(tick(0.5, 1050.0), CrisisFlags::default()).is_none());
    }

    #[test]
    fn crisis_mutes_then_rearms() {
        let mut e = CueEngine::new(
            CueConfig::default(),
            plan_with(vec![cue(5, 900.0, 1000.0, 1.0)]),
        );
        let crisis = CrisisFlags {
            spin: true,
            ..Default::default()
        };
        // At the trigger but in crisis → silent.
        assert!(e
            .tick(
                Tick {
                    now_s: 1.0,
                    position_m: 905.0,
                    v_now_ms: 50.0,
                    v_ref_ms: 50.0
                },
                crisis
            )
            .is_none());
        // 1 s later, crisis cleared but still inside the 3 s re-arm → silent.
        assert!(e.tick(tick(2.0, 950.0), CrisisFlags::default()).is_none());
        // Past re-arm AND still before the action point → fires.
        let fired = e.tick(tick(4.5, 980.0), CrisisFlags::default());
        assert!(fired.is_some());
    }

    #[test]
    fn live_correction_shifts_trigger_earlier_when_faster() {
        let e = CueEngine::new(
            CueConfig::default(),
            plan_with(vec![cue(5, 900.0, 1000.0, 1.0)]),
        );
        // Arriving 20% faster than reference → lead grows by the clamp (20%):
        // corrected trigger = 1000 - 100*1.2 = 880.
        let cue = &e.plan().cues[0];
        let ct = e.corrected_trigger(cue, 60.0, 50.0);
        assert!((ct - 880.0).abs() < 1e-6);
    }
}
