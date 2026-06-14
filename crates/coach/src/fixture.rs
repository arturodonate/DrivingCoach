//! Synthetic recorded-session generator.
//!
//! Real AC captures don't exist in this environment, so this builds a plausible
//! multi-lap [`Recording`] on a "stadium" track (two straights + two constant-
//! radius corners) that the offline pipeline can run end-to-end and that
//! exercises segmentation, references, diagnosis, and the summary. Each lap is
//! slightly faster than the last so the summary trend reads as *improving*.

use coach_telemetry::frame::{AcStatus, TelemetryFrame};
use coach_telemetry::recording::Recording;
use coach_telemetry::source::StaticInfo;
use std::f64::consts::PI;

const STRAIGHT: f64 = 250.0; // length of each straight (m)
const RADIUS: f64 = 45.0; // corner radius (m)
const DT_S: f64 = 0.01; // 100 Hz physics
const A_LAT: f64 = 14.0; // max lateral accel (m/s²) → corner speed
const V_MAX: f64 = 68.0; // top speed on the straight (m/s)

/// A centerline sample: position, cumulative distance, curvature, target speed.
struct PathPoint {
    x: f64,
    y: f64,
    s: f64,
    kappa: f64,
    v_target: f64,
}

/// Build the stadium centerline at ~1 m spacing.
fn centerline() -> Vec<PathPoint> {
    let mut pts: Vec<(f64, f64, f64)> = Vec::new(); // (x, y, kappa)
    let push = |pts: &mut Vec<(f64, f64, f64)>, x, y, k| pts.push((x, y, k));

    // Bottom straight: x from -STRAIGHT/2 to +STRAIGHT/2 at y = -RADIUS.
    let half = STRAIGHT / 2.0;
    let mut x = -half;
    while x <= half {
        push(&mut pts, x, -RADIUS, 0.0);
        x += 1.0;
    }
    // Right semicircle, center (half, 0), from -90° to +90°.
    let arc_n = (PI * RADIUS).round() as i32;
    for i in 1..arc_n {
        let th = -PI / 2.0 + PI * (i as f64 / arc_n as f64);
        push(
            &mut pts,
            half + RADIUS * th.cos(),
            RADIUS * th.sin(),
            1.0 / RADIUS,
        );
    }
    // Top straight: x from +half to -half at y = +RADIUS.
    let mut x = half;
    while x >= -half {
        push(&mut pts, x, RADIUS, 0.0);
        x -= 1.0;
    }
    // Left semicircle, center (-half, 0), from 90° to 270°.
    for i in 1..arc_n {
        let th = PI / 2.0 + PI * (i as f64 / arc_n as f64);
        push(
            &mut pts,
            -half + RADIUS * th.cos(),
            RADIUS * th.sin(),
            1.0 / RADIUS,
        );
    }

    // Cumulative arc length + corner-limited target speed.
    let mut out = Vec::with_capacity(pts.len());
    let mut s = 0.0;
    for i in 0..pts.len() {
        if i > 0 {
            let dx = pts[i].0 - pts[i - 1].0;
            let dy = pts[i].1 - pts[i - 1].1;
            s += (dx * dx + dy * dy).sqrt();
        }
        let kappa = pts[i].2;
        let v_target = if kappa > 1e-6 {
            (A_LAT / kappa).sqrt().min(V_MAX)
        } else {
            V_MAX
        };
        out.push(PathPoint {
            x: pts[i].0,
            y: pts[i].1,
            s,
            kappa,
            v_target,
        });
    }
    out
}

/// Sample the centerline at arc length `s` (linear interp), returning
/// `(x, y, kappa, v_target)`.
fn sample(path: &[PathPoint], s: f64) -> (f64, f64, f64, f64) {
    let total = path.last().unwrap().s;
    let s = s.rem_euclid(total);
    // Binary search for the bracketing points.
    let mut lo = 0usize;
    let mut hi = path.len() - 1;
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if path[mid].s <= s {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let a = &path[lo];
    let b = &path[hi];
    let t = if (b.s - a.s).abs() < 1e-9 {
        0.0
    } else {
        (s - a.s) / (b.s - a.s)
    };
    (
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.kappa + (b.kappa - a.kappa) * t,
        a.v_target + (b.v_target - a.v_target) * t,
    )
}

/// Generate a recording of `n_laps` laps. Lap 0 acts as the opening out-lap.
pub fn generate(track: &str, car: &str, n_laps: usize) -> Recording {
    let path = centerline();
    let total = path.last().unwrap().s;

    let static_info = StaticInfo {
        track: track.to_string(),
        car_model: car.to_string(),
        track_spline_length_m: total,
    };
    let mut rec = Recording::new(static_info);

    let mut packet: u32 = 0;
    let mut t_us: u64 = 0;
    let mut v = 30.0_f64; // m/s, rolling start

    for lap in 0..n_laps {
        // Each later lap carries a touch more corner speed → improving trend.
        let skill = 1.0 + 0.01 * lap as f64;
        let mut s = 0.0_f64;
        while s < total {
            let (x, y, kappa, v_target) = sample(&path, s);
            let target = (v_target * skill).min(V_MAX);

            // Simple longitudinal model: brake toward a lower target, else accel.
            let (brake, gas) = if target < v - 0.5 {
                v = (v - 18.0 * DT_S).max(target);
                (0.8_f32, 0.0_f32)
            } else if v < target - 0.5 {
                v = (v + 10.0 * DT_S).min(target);
                (0.0, 1.0)
            } else {
                (0.0, 0.6)
            };

            let frame = TelemetryFrame {
                t_us,
                packet_id: packet,
                gas,
                brake,
                gear: 4,
                steer_angle: (kappa * 30.0) as f32,
                speed_kmh: (v * 3.6) as f32,
                velocity: [v as f32, 0.0, 0.0],
                acc_g: [0.0; 3],
                wheel_slip: [0.0; 4],
                normalized_car_position: (s / total) as f32,
                car_coordinates: [x as f32, 0.0, y as f32],
                completed_laps: lap as i32,
                i_current_time_ms: 0,
                i_last_time_ms: 0,
                number_of_tyres_out: 0,
                penalty: false,
                is_in_pit: false,
                status: AcStatus::Live,
                session: 1, // practice
            };
            rec.push(frame);

            s += (v * DT_S).max(0.1);
            packet += 1;
            t_us += (DT_S * 1_000_000.0) as u64;
        }
    }
    rec
}
