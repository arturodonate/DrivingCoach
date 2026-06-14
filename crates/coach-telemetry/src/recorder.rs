//! Lap recorder: segments a frame stream into laps with integrity flags
//! (spec §13.1).
//!
//! [`LapRecorder`] is fed frames in order; each time a lap boundary is crossed
//! it returns the [`RawLap`] that just completed, with validity/teleport/in-out
//! flags set. Teleported laps are flagged for exclusion (spec §4.3); fully clean
//! laps are eligible for the full-lap PB (spec §5.3 — segment-level gating of
//! dirty laps happens later, in the analysis crate).

use crate::frame::TelemetryFrame;
use crate::integrity::{lap_distance_drift, IntegrityMonitor};
use coach_core::config::TelemetryConfig;

/// One recorded lap with its raw frames and integrity verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct RawLap {
    pub frames: Vec<TelemetryFrame>,
    /// `completedLaps` value at the boundary that closed this lap.
    pub completed_lap_number: i32,
    /// `iLastTime` (ms) reported at the boundary — the just-finished lap's time.
    pub lap_time_ms: i32,
    /// Fully clean: no off-track/penalty sample and no teleport (spec §5.3).
    pub valid: bool,
    /// A teleport/back-to-pits occurred → exclude entirely (spec §4.3).
    pub teleported: bool,
    /// Pit entry/exit happened during the lap, or it is the opening partial lap.
    pub in_out_lap: bool,
    /// Spline-vs-integrated distance drift (spec §4.3 cross-check).
    pub distance_drift: f64,
}

/// Segments a frame stream into [`RawLap`]s.
pub struct LapRecorder {
    monitor: IntegrityMonitor,
    spline_length_m: f64,
    current: Vec<TelemetryFrame>,
    any_off_track: bool,
    any_teleport: bool,
    any_pit: bool,
    is_first_lap: bool,
}

impl LapRecorder {
    pub fn new(cfg: TelemetryConfig, spline_length_m: f64) -> Self {
        Self {
            monitor: IntegrityMonitor::new(cfg, spline_length_m),
            spline_length_m,
            current: Vec::new(),
            any_off_track: false,
            any_teleport: false,
            any_pit: false,
            is_first_lap: true,
        }
    }

    /// Feed one frame. Returns the completed lap when a boundary is crossed.
    pub fn feed(&mut self, frame: TelemetryFrame) -> Option<RawLap> {
        let a = self.monitor.assess(&frame);

        let mut completed = None;
        if a.lap_boundary && !self.current.is_empty() {
            completed = Some(self.close_lap(frame.completed_laps, frame.i_last_time_ms));
        }

        // Accumulate dirtiness for the in-progress lap.
        if frame.is_off_track() || frame.penalty {
            self.any_off_track = true;
        }
        if a.teleport {
            self.any_teleport = true;
        }
        if frame.is_in_pit {
            self.any_pit = true;
        }

        self.current.push(frame);
        completed
    }

    /// Flush a trailing in-progress lap (e.g. at session end). Marked in/out
    /// since it never crossed a closing boundary.
    pub fn finish(&mut self) -> Option<RawLap> {
        if self.current.is_empty() {
            return None;
        }
        let last = self.current.last().unwrap();
        let mut lap = self.close_lap(last.completed_laps, last.i_current_time_ms);
        lap.in_out_lap = true;
        Some(lap)
    }

    fn close_lap(&mut self, completed_lap_number: i32, lap_time_ms: i32) -> RawLap {
        let frames = std::mem::take(&mut self.current);
        let distance_drift = lap_distance_drift(&frames, self.spline_length_m);
        let in_out_lap = self.any_pit || self.is_first_lap;
        let lap = RawLap {
            frames,
            completed_lap_number,
            lap_time_ms,
            valid: !self.any_off_track && !self.any_teleport && !in_out_lap,
            teleported: self.any_teleport,
            in_out_lap,
            distance_drift,
        };
        // Reset per-lap accumulators for the next lap.
        self.any_off_track = false;
        self.any_teleport = false;
        self.any_pit = false;
        self.is_first_lap = false;
        lap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::AcStatus;

    fn frame(t_us: u64, packet: u32, pos: f32, laps: i32) -> TelemetryFrame {
        TelemetryFrame {
            t_us,
            packet_id: packet,
            gas: 1.0,
            brake: 0.0,
            gear: 4,
            steer_angle: 0.0,
            speed_kmh: 180.0,
            velocity: [50.0, 0.0, 0.0],
            acc_g: [0.0; 3],
            wheel_slip: [0.0; 4],
            normalized_car_position: pos,
            car_coordinates: [0.0; 3],
            completed_laps: laps,
            i_current_time_ms: 0,
            i_last_time_ms: 90_000,
            number_of_tyres_out: 0,
            penalty: false,
            is_in_pit: false,
            status: AcStatus::Live,
            session: 1,
        }
    }

    /// Feed one lap of `steps` evenly spaced frames (small, realistic position
    /// deltas) for lap number `lap`, starting at packet `p0`/time `t0_us`.
    /// Returns any laps closed while feeding (the boundary into the *next* lap
    /// must be fed separately).
    fn feed_lap(
        rec: &mut LapRecorder,
        lap: i32,
        steps: u32,
        p0: u32,
        t0_us: u64,
        mutate: impl Fn(u32, &mut TelemetryFrame),
    ) -> Vec<RawLap> {
        let mut out = Vec::new();
        for i in 0..steps {
            let pos = i as f32 / steps as f32; // 0.0 .. ~1.0
            let mut f = frame(t0_us + i as u64 * 10_000, p0 + i, pos, lap);
            mutate(i, &mut f);
            if let Some(l) = rec.feed(f) {
                out.push(l);
            }
        }
        out
    }

    #[test]
    fn segments_two_laps_and_flags_first_as_in_out() {
        let mut rec = LapRecorder::new(TelemetryConfig::default(), 1000.0);
        let mut emitted = Vec::new();
        // Opening (out) lap, then boundary into lap 1, then a clean lap 1.
        emitted.extend(feed_lap(&mut rec, 0, 100, 1, 0, |_, _| {}));
        emitted.extend(feed_lap(&mut rec, 1, 100, 200, 1_000_000, |_, _| {}));
        // Boundary into lap 2 closes the clean lap 1.
        if let Some(l) = rec.feed(frame(2_000_000, 400, 0.0, 2)) {
            emitted.push(l);
        }

        assert_eq!(emitted.len(), 2);
        assert!(
            emitted[0].in_out_lap,
            "first lap is the opening partial/out lap"
        );
        assert!(!emitted[1].in_out_lap);
        assert!(emitted[1].valid, "clean second lap should be valid");
    }

    #[test]
    fn off_track_sample_invalidates_full_lap() {
        let mut rec = LapRecorder::new(TelemetryConfig::default(), 1000.0);
        // Opening (out) lap to clear is_first_lap.
        feed_lap(&mut rec, 0, 100, 1, 0, |_, _| {});
        // Lap 1 with one off-track sample mid-lap.
        feed_lap(&mut rec, 1, 100, 200, 1_000_000, |i, f| {
            if i == 50 {
                f.number_of_tyres_out = 3;
            }
        });
        let lap = rec.feed(frame(2_000_000, 400, 0.0, 2)).expect("lap closed");
        assert!(!lap.valid, "off-track sample must invalidate the full lap");
    }
}
