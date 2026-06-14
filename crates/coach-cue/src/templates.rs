//! Closed cue-template grammar (spec §8.1).
//!
//! `{segment_label} — {phrase}` plus single-word cues, confirmations, and status
//! lines. The phrase set is fixed (~20 entries); mapping a diagnosis error +
//! magnitude to a phrase here is the only place coaching text is produced.

use coach_core::cue::CueKind;
use coach_core::diagnosis::{ErrorKind, Finding, Magnitude};

/// What template a finding renders to: the phrase, the cue kind, and whether the
/// segment label is prefixed.
pub struct Rendered {
    pub text: String,
    pub kind: CueKind,
}

/// Map an error + magnitude to a closed-vocabulary phrase (spec §8.1).
///
/// `ApexSpeedDeficit` has no dedicated mid-corner phrase in the vocabulary, so it
/// renders to the single-word `apex` reminder (fired at turn-in) — the closest
/// in-vocabulary cue for "commit through the middle".
pub fn phrase_for(error: ErrorKind, mag: Magnitude) -> (&'static str, CueKind) {
    use ErrorKind::*;
    let slight = mag == Magnitude::Slight;
    match error {
        BrakeTooEarly => (
            if slight {
                "brake slightly later"
            } else {
                "brake much later"
            },
            CueKind::Correction,
        ),
        BrakeTooLate => (
            if slight {
                "brake slightly earlier"
            } else {
                "brake much earlier"
            },
            CueKind::Correction,
        ),
        BrakeTooHard => ("brake softer", CueKind::Correction),
        BrakeTooSoft => ("brake harder", CueKind::Correction),
        NoTrailBraking => ("trail the brakes", CueKind::Correction),
        EarlyApex => (
            if slight {
                "turn in slightly later"
            } else {
                "turn in much later"
            },
            CueKind::Correction,
        ),
        LateApex => (
            if slight {
                "turn in slightly earlier"
            } else {
                "turn in much earlier"
            },
            CueKind::Correction,
        ),
        ApexSpeedDeficit => ("apex", CueKind::Apex),
        ThrottleTooLate => ("earlier on throttle", CueKind::Correction),
        Wheelspin => ("easy on throttle", CueKind::Correction),
    }
}

/// Render a finding into its full cue text.
pub fn render_finding(f: &Finding) -> Rendered {
    let (phrase, kind) = phrase_for(f.error, f.magnitude);
    let text = match kind {
        // The "apex" single-word cue carries no segment label (spec §8.1).
        CueKind::Apex => phrase.to_string(),
        _ => format!("{} — {}", f.segment_label, phrase),
    };
    Rendered { text, kind }
}

/// "better — {segment}" confirmation when a coached corner is fixed (spec §7.4).
pub fn confirmation(segment_label: &str) -> String {
    format!("better — {segment_label}")
}

/// Status lines (spec §5.4) and the personal-best jingle line (spec §7.4).
pub const LEARNING_TRACK: &str = "learning track";
pub const COACHING_ACTIVE: &str = "coaching active";
pub const PERSONAL_BEST: &str = "personal best";

/// The full closed vocabulary, for warming the WAV cache ahead of a session.
pub fn full_vocabulary(corner_labels: &[String]) -> Vec<String> {
    let phrases = [
        "brake slightly later",
        "brake much later",
        "brake slightly earlier",
        "brake much earlier",
        "brake harder",
        "brake softer",
        "trail the brakes",
        "turn in slightly later",
        "turn in much later",
        "turn in slightly earlier",
        "turn in much earlier",
        "earlier on throttle",
        "easy on throttle",
        "more throttle",
    ];
    let mut out = vec![
        "apex".to_string(),
        PERSONAL_BEST.to_string(),
        LEARNING_TRACK.to_string(),
        COACHING_ACTIVE.to_string(),
    ];
    for label in corner_labels {
        for p in phrases {
            out.push(format!("{label} — {p}"));
        }
        out.push(confirmation(label));
    }
    out
}
