//! Compiled cue and coaching-plan objects (spec §8, §12.3).
//!
//! A [`CoachingPlan`] is the artifact the cue compiler produces and the
//! real-time engine consumes. Everything except the live trigger correction
//! (spec §9.1) is fixed at compile time.

use crate::diagnosis::Magnitude;
use crate::ids::{CarId, LapId, SegmentId, TrackId};
use serde::{Deserialize, Serialize};

/// What a cue is for. Drives arbiter priority and report attribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    /// A corrective instruction ("brake slightly later").
    Correction,
    /// The single-word "apex" cue, fired at turn-in (spec §8.1).
    Apex,
    /// "better — {segment}" when a coached corner is fixed (spec §7.4).
    Confirmation,
    /// Personal-best jingle at the line (spec §7.4).
    PersonalBest,
    /// "learning track" / "coaching active" status lines (spec §5.4).
    Status,
}

/// One compiled cue (spec §12.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub segment_id: SegmentId,
    /// The action point the cue must finish before (metres-from-start).
    pub action_point_dist_m: f64,
    /// Back-integrated trigger distance (spec §9.1); corrected live at runtime.
    pub trigger_dist_m: f64,
    pub speech_duration_s: f64,
    /// Rendered template text, e.g. "Turn 5 — brake slightly later".
    pub template: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub magnitude: Option<Magnitude>,
    /// Content-hash id of the cached WAV clip (spec §8.2).
    pub audio_clip_id: String,
    /// 1 = highest. Lower number wins arbiter overlap resolution (spec §9.2).
    pub priority: u32,
    pub kind: CueKind,
}

/// A full lap's coaching plan, double-buffered and swapped at the lap boundary
/// (spec §9.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoachingPlan {
    pub track_id: TrackId,
    pub car_id: CarId,
    /// The lap this plan was compiled *from* (it coaches lap N+1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_lap_id: Option<LapId>,
    /// Cues ordered by `trigger_dist_m`.
    pub cues: Vec<Cue>,
}

impl CoachingPlan {
    pub fn empty(track_id: TrackId, car_id: CarId) -> Self {
        Self {
            track_id,
            car_id,
            source_lap_id: None,
            cues: Vec::new(),
        }
    }
}
