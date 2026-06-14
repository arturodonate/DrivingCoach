//! Session-level types (spec §10, §12.1 `sessions`).

use serde::{Deserialize, Serialize};

/// AC session type, read from the Graphics `session` flag (spec §10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionType {
    Practice,
    Qualify,
    Race,
    /// Anything else AC reports (hotlap, time attack, etc.) — treated like
    /// practice for trend purposes but labeled honestly.
    Other,
}

impl SessionType {
    /// Map AC's integer `session` flag. AC encodes: 0=unknown,1=practice,
    /// 2=qualify,3=race,4=hotlap,5=time attack,6=drift,7=drag (CSP may extend;
    /// **confirm on the deploy box**, spec §4.2).
    pub fn from_ac_flag(flag: i32) -> Self {
        match flag {
            1 => SessionType::Practice,
            2 => SessionType::Qualify,
            3 => SessionType::Race,
            _ => SessionType::Other,
        }
    }

    /// Race sessions carry the fuel/tyre confounder caveat in the summary
    /// (spec §10.2).
    pub fn has_fuel_tyre_caveat(self) -> bool {
        matches!(self, SessionType::Race)
    }

    pub fn label(self) -> &'static str {
        match self {
            SessionType::Practice => "Practice",
            SessionType::Qualify => "Qualify",
            SessionType::Race => "Race",
            SessionType::Other => "Session",
        }
    }
}
