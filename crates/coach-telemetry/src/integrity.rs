//! Telemetry integrity checks (spec §4.3) — built in from day one.
//!
//! [`IntegrityMonitor`] is fed frames in order and returns a per-frame
//! [`FrameAssessment`]. It is stateful but allocation-free, so it is also
//! suitable for the real-time path. Timing (staleness) is driven by
//! `frame.t_us`, so the same logic works for live polling and recorded replay.

use crate::frame::{AcStatus, TelemetryFrame};
use coach_core::config::TelemetryConfig;

/// Per-frame integrity verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameAssessment {
    /// Spline position wrapped ~1.0 → ~0.0 **and** `completedLaps` incremented.
    pub lap_boundary: bool,
    /// Position discontinuity > `teleport_jump_m`, or a mid-lap pit/status
    /// transition. Invalidates the in-progress lap recording (spec §4.3).
    pub teleport: bool,
    /// Large backward jump in spline position — a spin. Feeds crisis mute.
    pub spin: bool,
    /// `packetId` stopped advancing beyond `staleness_ms` while live.
    pub stale: bool,
    /// Small backward jitter that was rejected (no forward progress credited).
    pub backward_jitter: bool,
}

/// Stateful integrity tracker over a single ordered frame stream.
#[derive(Debug, Clone)]
pub struct IntegrityMonitor {
    cfg: TelemetryConfig,
    spline_length_m: f64,
    last_packet_id: Option<u32>,
    last_packet_change_us: u64,
    last_norm_pos: Option<f32>,
    last_completed_laps: Option<i32>,
    last_in_pit: Option<bool>,
    last_status: Option<AcStatus>,
}

impl IntegrityMonitor {
    pub fn new(cfg: TelemetryConfig, spline_length_m: f64) -> Self {
        Self {
            cfg,
            spline_length_m,
            last_packet_id: None,
            last_packet_change_us: 0,
            last_norm_pos: None,
            last_completed_laps: None,
            last_in_pit: None,
            last_status: None,
        }
    }

    /// Assess one frame and advance internal state.
    pub fn assess(&mut self, f: &TelemetryFrame) -> FrameAssessment {
        let mut a = FrameAssessment::default();

        // --- Staleness: packetId must keep advancing while live (§4.3). ---
        match self.last_packet_id {
            Some(prev) if prev == f.packet_id => {
                let stalled_us = f.t_us.saturating_sub(self.last_packet_change_us);
                if f.status == AcStatus::Live && stalled_us > self.cfg.staleness_ms * 1_000 {
                    a.stale = true;
                }
            }
            _ => {
                self.last_packet_change_us = f.t_us;
            }
        }

        // --- Lap boundary: wrap high→low AND completedLaps increments (§4.3). ---
        let laps_incremented =
            matches!(self.last_completed_laps, Some(prev) if f.completed_laps > prev);
        let wrapped = matches!(self.last_norm_pos, Some(prev)
            if (prev as f64) > 0.5 && (f.normalized_car_position as f64) < 0.5);
        if laps_incremented && wrapped {
            a.lap_boundary = true;
        }

        // --- Position deltas (only meaningful within a lap, not across wrap). ---
        if let Some(prev) = self.last_norm_pos {
            if !a.lap_boundary {
                let delta_dist_m =
                    (f.normalized_car_position as f64 - prev as f64) * self.spline_length_m;

                if delta_dist_m.abs() > self.cfg.teleport_jump_m {
                    a.teleport = true; // discontinuity (§4.3)
                } else if delta_dist_m < -self.cfg.spin_backward_m {
                    a.spin = true; // large backward jump (§4.3)
                } else if delta_dist_m < 0.0 && delta_dist_m >= -self.cfg.monotonic_jitter_m {
                    a.backward_jitter = true; // small backward jitter, reject (§4.3)
                } else if delta_dist_m < 0.0 {
                    // Backward beyond jitter but below spin threshold — also a spin.
                    a.spin = true;
                }
            }
        }

        // --- Mid-lap pit / status transition → teleport ("back to pits", §4.3). ---
        if matches!(self.last_in_pit, Some(false)) && f.is_in_pit && !a.lap_boundary {
            a.teleport = true;
        }
        if let Some(prev) = self.last_status {
            if prev == AcStatus::Live && f.status != AcStatus::Live && !a.lap_boundary {
                a.teleport = true;
            }
        }

        // --- Advance state. ---
        if self.last_packet_id != Some(f.packet_id) {
            self.last_packet_change_us = f.t_us;
        }
        self.last_packet_id = Some(f.packet_id);
        self.last_norm_pos = Some(f.normalized_car_position);
        self.last_completed_laps = Some(f.completed_laps);
        self.last_in_pit = Some(f.is_in_pit);
        self.last_status = Some(f.status);

        a
    }
}

/// Per-lap distance cross-check (spec §4.3): compare spline-derived distance to
/// speed-integrated distance over the lap's frames. Returns the relative drift
/// `|spline − integrated| / spline_length`. Drift above
/// `distance_drift_frac` indicates a poorly calibrated track spline; the caller
/// logs it and widens per-segment comparison tolerance.
pub fn lap_distance_drift(frames: &[TelemetryFrame], spline_length_m: f64) -> f64 {
    if frames.len() < 2 || spline_length_m <= 0.0 {
        return 0.0;
    }
    // Speed-integrated distance (trapezoidal, speed km/h → m/s).
    let mut integrated_m = 0.0_f64;
    for w in frames.windows(2) {
        let dt_s = (w[1].t_us.saturating_sub(w[0].t_us)) as f64 / 1_000_000.0;
        let v0 = w[0].speed_kmh as f64 / 3.6;
        let v1 = w[1].speed_kmh as f64 / 3.6;
        integrated_m += 0.5 * (v0 + v1) * dt_s;
    }
    // Spline-derived distance: net forward progress over the lap.
    let spline_m = (frames.last().unwrap().normalized_car_position as f64
        - frames[0].normalized_car_position as f64)
        .rem_euclid(1.0)
        * spline_length_m;
    if spline_m <= 0.0 {
        return 0.0;
    }
    (spline_m - integrated_m).abs() / spline_length_m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_frame(t_us: u64, packet: u32, pos: f32, laps: i32) -> TelemetryFrame {
        TelemetryFrame {
            t_us,
            packet_id: packet,
            gas: 0.0,
            brake: 0.0,
            gear: 3,
            steer_angle: 0.0,
            speed_kmh: 200.0,
            velocity: [0.0; 3],
            acc_g: [0.0; 3],
            wheel_slip: [0.0; 4],
            normalized_car_position: pos,
            car_coordinates: [0.0; 3],
            completed_laps: laps,
            i_current_time_ms: 0,
            i_last_time_ms: 0,
            number_of_tyres_out: 0,
            penalty: false,
            is_in_pit: false,
            status: AcStatus::Live,
            session: 1,
        }
    }

    #[test]
    fn detects_lap_boundary_on_wrap_and_increment() {
        let cfg = TelemetryConfig::default();
        let mut m = IntegrityMonitor::new(cfg, 1000.0);
        m.assess(&base_frame(0, 1, 0.98, 0));
        let a = m.assess(&base_frame(10_000, 2, 0.01, 1));
        assert!(a.lap_boundary);
        assert!(!a.teleport);
    }

    #[test]
    fn detects_teleport_on_large_jump() {
        let cfg = TelemetryConfig::default();
        let mut m = IntegrityMonitor::new(cfg, 1000.0); // 1000 m track
        m.assess(&base_frame(0, 1, 0.10, 0));
        // +0.10 of 1000 m = 100 m jump > 50 m teleport threshold.
        let a = m.assess(&base_frame(10_000, 2, 0.20, 0));
        assert!(a.teleport);
    }

    #[test]
    fn small_backward_is_jitter_large_is_spin() {
        let cfg = TelemetryConfig::default(); // jitter 1 m, spin 8 m
        let mut m = IntegrityMonitor::new(cfg, 1000.0);
        m.assess(&base_frame(0, 1, 0.500, 0));
        let jitter = m.assess(&base_frame(10_000, 2, 0.4995, 0)); // -0.5 m
        assert!(jitter.backward_jitter && !jitter.spin);

        let mut m2 = IntegrityMonitor::new(TelemetryConfig::default(), 1000.0);
        m2.assess(&base_frame(0, 1, 0.500, 0));
        let spin = m2.assess(&base_frame(10_000, 2, 0.489, 0)); // -11 m
        assert!(spin.spin && !spin.backward_jitter);
    }

    #[test]
    fn detects_staleness_when_packet_stops() {
        let cfg = TelemetryConfig::default(); // 500 ms
        let mut m = IntegrityMonitor::new(cfg, 1000.0);
        m.assess(&base_frame(0, 7, 0.5, 0));
        // Same packet id 600 ms later while live → stale.
        let a = m.assess(&base_frame(600_000, 7, 0.5, 0));
        assert!(a.stale);
    }

    #[test]
    fn mid_lap_pit_entry_is_teleport() {
        let cfg = TelemetryConfig::default();
        let mut m = IntegrityMonitor::new(cfg, 1000.0);
        m.assess(&base_frame(0, 1, 0.30, 0));
        let mut f = base_frame(10_000, 2, 0.305, 0);
        f.is_in_pit = true;
        let a = m.assess(&f);
        assert!(a.teleport);
    }
}
