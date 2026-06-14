//! Normalized, cross-platform telemetry frame.
//!
//! The live Windows source reads the raw AC `#[repr(C)]` pages (see
//! [`crate::ac_shared`]) and converts them into this platform-independent
//! struct; the recorder persists these, and the replay source yields them. The
//! whole offline pipeline and all tests operate on [`TelemetryFrame`], never on
//! the raw pages — so they build and run anywhere.
//!
//! Fields mirror the spec §4.2 table (only what the system uses).

use serde::{Deserialize, Serialize};

/// AC `status` flag (Graphics page). Values per the AC SDK; **confirm on the
/// deploy box** (spec §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcStatus {
    Off,
    Replay,
    Live,
    Pause,
}

impl AcStatus {
    pub fn from_flag(flag: i32) -> Self {
        match flag {
            1 => AcStatus::Replay,
            2 => AcStatus::Live,
            3 => AcStatus::Pause,
            _ => AcStatus::Off,
        }
    }
}

/// One merged telemetry sample: the Physics fields at this instant plus the
/// most-recent Graphics fields, on a common timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Monotonic capture time in microseconds. Drives staleness + replay pacing.
    pub t_us: u64,

    // ---- Physics page (spec §4.2) ----
    /// Physics packet id; dedupe + staleness watchdog.
    pub packet_id: u32,
    /// Throttle 0.0–1.0 (`gas`).
    pub gas: f32,
    /// Brake 0.0–1.0.
    pub brake: f32,
    pub gear: i32,
    pub steer_angle: f32,
    pub speed_kmh: f32,
    pub velocity: [f32; 3],
    pub acc_g: [f32; 3],
    pub wheel_slip: [f32; 4],

    // ---- Graphics page (spec §4.2) ----
    /// Spline position in `[0, 1)`; × `track_spline_length` → distance.
    pub normalized_car_position: f32,
    pub car_coordinates: [f32; 3],
    pub completed_laps: i32,
    /// Current lap time, milliseconds.
    pub i_current_time_ms: i32,
    /// Last completed lap time, milliseconds.
    pub i_last_time_ms: i32,
    pub number_of_tyres_out: i32,
    /// Penalty active (any). AC exposes a penalty enum/time; we only need "any".
    pub penalty: bool,
    pub is_in_pit: bool,
    pub status: AcStatus,
    /// Raw AC `session` flag; map via [`coach_core::SessionType::from_ac_flag`].
    pub session: i32,
}

impl TelemetryFrame {
    /// Off-track this sample if ≥1 tyre is out (per-sample clean gating input,
    /// spec §4.2 / §5.3). The segment gate uses ≥1 tyre-out as "dirty"; the
    /// crisis mute uses ≥2 (spec §9.3).
    pub fn is_off_track(&self) -> bool {
        self.number_of_tyres_out >= 1
    }

    /// Lateral world position used for line-offset / curvature. AC's car
    /// coordinates are `[x, y, z]` with `y` vertical, so the planar path is
    /// `(x, z)`.
    pub fn planar_xy(&self) -> (f32, f32) {
        (self.car_coordinates[0], self.car_coordinates[2])
    }
}
