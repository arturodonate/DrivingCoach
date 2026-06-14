//! Central tunable configuration.
//!
//! Every threshold the spec names is surfaced here with its spec-default, so the
//! whole system's behavior is configurable from one serde-deserializable struct
//! (TOML/JSON). Sub-structs group tunables by pipeline stage. Defaults match the
//! spec; section references are inline.

use serde::{Deserialize, Serialize};

/// Top-level configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub grid_step_m: f64,
    pub telemetry: TelemetryConfig,
    pub profiler: ProfilerConfig,
    pub reference: ReferenceConfig,
    pub diagnosis: DiagnosisConfig,
    pub cue: CueConfig,
    pub audio: AudioConfig,
    pub summary: SummaryConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            grid_step_m: crate::grid::DEFAULT_STEP_M,
            telemetry: TelemetryConfig::default(),
            profiler: ProfilerConfig::default(),
            reference: ReferenceConfig::default(),
            diagnosis: DiagnosisConfig::default(),
            cue: CueConfig::default(),
            audio: AudioConfig::default(),
            summary: SummaryConfig::default(),
        }
    }
}

/// Polling & integrity (spec §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    pub physics_hz: f64,
    pub graphics_hz: f64,
    /// Position discontinuity treated as a teleport (spec §4.3).
    pub teleport_jump_m: f64,
    /// `packetId` stall beyond this marks telemetry stale (spec §4.3).
    pub staleness_ms: u64,
    /// Spline-vs-integrated distance drift that widens comparison tolerance
    /// (spec §4.3).
    pub distance_drift_frac: f64,
    /// Backward spline movement up to this (m) is treated as jitter and ignored
    /// (spec §4.3 monotonic position).
    pub monotonic_jitter_m: f64,
    /// Backward spline jump beyond this (m), but below `teleport_jump_m`, is a
    /// spin (feeds crisis mute, spec §4.3 / §9.3).
    pub spin_backward_m: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            physics_hz: 100.0,
            graphics_hz: 60.0,
            teleport_jump_m: 50.0,
            staleness_ms: 500,
            distance_drift_frac: 0.01,
            monotonic_jitter_m: 1.0,
            spin_backward_m: 8.0,
        }
    }
}

/// Track profiling: smoothing & curvature segmentation (spec §5.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProfilerConfig {
    /// Savitzky–Golay window length in metres (spec §5.1, ~15–25 m).
    pub savgol_window_m: f64,
    /// Savitzky–Golay polynomial order.
    pub savgol_poly_order: usize,
    /// Enter-corner curvature threshold (1/m), hysteresis high.
    pub kappa_hi: f64,
    /// Exit-corner curvature threshold (1/m), hysteresis low.
    pub kappa_lo: f64,
    /// Build profile from the median over the first N clean laps (spec §5.1).
    pub profile_laps: usize,
    /// Drop segments shorter than this (m) to avoid fragmentation.
    pub min_segment_m: f64,
}

impl Default for ProfilerConfig {
    fn default() -> Self {
        Self {
            savgol_window_m: 21.0,
            savgol_poly_order: 3,
            kappa_hi: 0.012,
            kappa_lo: 0.006,
            profile_laps: 3,
            min_segment_m: 20.0,
        }
    }
}

/// Reference model (spec §5.2, §5.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReferenceConfig {
    /// Entry-speed guard: relative delta beyond which we fall back to the full
    /// PB-lap trace for a segment (spec §5.2).
    pub entry_speed_guard_frac: f64,
    /// Brake detection: brake fraction considered "on" (spec §5.1).
    pub brake_on_frac: f32,
    /// Sustained-braking window ahead of corner, seconds (spec §5.1).
    pub brake_sustain_s: f64,
    /// Turn-in steering threshold (fraction of per-segment max steer).
    pub turn_in_steer_frac: f32,
    /// Throttle-on threshold, rising, after apex (spec §5.1).
    pub throttle_on_frac: f32,
}

impl Default for ReferenceConfig {
    fn default() -> Self {
        Self {
            entry_speed_guard_frac: 0.07,
            brake_on_frac: 0.10,
            brake_sustain_s: 0.2,
            turn_in_steer_frac: 0.30,
            throttle_on_frac: 0.50,
        }
    }
}

/// Diagnosis stability mechanisms (spec §7.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagnosisConfig {
    /// Persistence: a finding is spoken only when it appears in ≥`min_of` of the
    /// last `window` valid laps (spec §7.3).
    pub persistence_window: usize,
    pub persistence_min_of: usize,
    /// Never reverse advice direction within this many laps (spec §7.3).
    pub hysteresis_laps: usize,
    /// Per-segment-type noise floor (ms) below which loss is not coached.
    pub noise_floor_ms: i64,
    /// Brake-point bucket boundary: slight is `[lo, hi]` m, much is `> hi`
    /// (spec §7.3).
    pub brake_bucket_lo_m: f64,
    pub brake_bucket_hi_m: f64,
    /// Only the worst N corners become spoken cues per lap (spec §7.2).
    pub max_coached_corners: usize,
}

impl Default for DiagnosisConfig {
    fn default() -> Self {
        Self {
            persistence_window: 3,
            persistence_min_of: 2,
            hysteresis_laps: 3,
            noise_floor_ms: 50,
            brake_bucket_lo_m: 3.0,
            brake_bucket_hi_m: 8.0,
            max_coached_corners: 2,
        }
    }
}

/// Cue timing & arbitration (spec §9.1, §9.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CueConfig {
    /// Reaction time added to speech duration when back-integrating the trigger
    /// (spec §9.1).
    pub reaction_time_s: f64,
    /// Live trigger correction clamp, fraction (spec §9.1, ±20%).
    pub trigger_correction_clamp: f64,
    /// Minimum gap between cue end and next cue start, seconds (spec §9.2).
    pub min_gap_s: f64,
    /// Two coached corners whose action points fall within this distance (m) are
    /// merged into one combined "chicane" cue (spec §9.2).
    pub chicane_merge_m: f64,
    /// Estimated speaking rate (chars/sec) used to bound speech duration before
    /// the real clip length is known.
    pub chars_per_second: f64,
    /// Directory holding cached cue WAVs (keyed by text hash).
    pub wav_cache_dir: String,
    /// Path to the Piper voice ONNX model.
    pub voice_model_path: String,
    /// Path to the `piper` binary (shell-out synthesizer).
    pub piper_binary: String,
}

impl Default for CueConfig {
    fn default() -> Self {
        Self {
            reaction_time_s: 0.5,
            trigger_correction_clamp: 0.20,
            min_gap_s: 1.5,
            chicane_merge_m: 60.0,
            chars_per_second: 14.0,
            wav_cache_dir: "data/wav-cache".to_string(),
            voice_model_path: "voices/en_US-ryan-medium.onnx".to_string(),
            piper_binary: "piper".to_string(),
        }
    }
}

/// Audio output (spec §9.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub sample_rate_hz: u32,
    /// 0.0–1.0 output gain.
    pub volume: f32,
    /// Output device name; `None` = system default (spec §9.5).
    pub output_device: Option<String>,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 22_050, // matches en_US-ryan-medium (spec §8.2)
            volume: 1.0,
            output_device: None,
        }
    }
}

/// Post-event summary (spec §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SummaryConfig {
    /// Outlier rejection: lap time beyond `median + k·MAD` is flagged
    /// traffic/incident-affected (spec §10.2).
    pub outlier_mad_k: f64,
    /// Behavior-change noise thresholds: a metric must shift beyond this to be
    /// reported (spec §10.1b).
    pub brake_point_shift_m: f64,
    pub turn_in_shift_m: f64,
    pub apex_speed_shift_kmh: f64,
    pub throttle_on_shift_m: f64,
    pub segment_time_shift_ms: f64,
    /// Output directory for `.md` / `.json` artifacts.
    pub report_dir: String,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            outlier_mad_k: 3.0,
            brake_point_shift_m: 3.0,
            turn_in_shift_m: 1.0,
            apex_speed_shift_kmh: 2.0,
            throttle_on_shift_m: 3.0,
            segment_time_shift_ms: 30.0,
            report_dir: "reports".to_string(),
        }
    }
}
