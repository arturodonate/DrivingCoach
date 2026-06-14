//! Path curvature κ from a smoothed world path (spec §5.1).
//!
//! κ = |x′y″ − y′x″| / (x′² + y′²)^(3/2), computed with central differences on
//! the (already Savitzky–Golay-smoothed) `x(s)`, `y(s)`. The path is a closed
//! loop, so differences wrap.

/// Curvature at every grid point (units 1/m), given smoothed coordinates on a
/// uniform grid of spacing `step_m`.
pub fn curvature(x: &[f64], y: &[f64], step_m: f64) -> Vec<f64> {
    let n = x.len();
    assert_eq!(n, y.len());
    let mut k = vec![0.0; n];
    if n < 3 {
        return k;
    }
    let h = step_m;
    for i in 0..n {
        let ip = (i + 1) % n;
        let im = (i + n - 1) % n;
        let dx = (x[ip] - x[im]) / (2.0 * h);
        let dy = (y[ip] - y[im]) / (2.0 * h);
        let ddx = (x[ip] - 2.0 * x[i] + x[im]) / (h * h);
        let ddy = (y[ip] - 2.0 * y[i] + y[im]) / (h * h);
        let denom = (dx * dx + dy * dy).powf(1.5);
        k[i] = if denom > 1e-9 {
            (dx * ddy - dy * ddx).abs() / denom
        } else {
            0.0
        };
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_has_constant_curvature() {
        // Radius 100 m circle → κ ≈ 1/100 = 0.01 everywhere.
        let r = 100.0_f64;
        let n = 400;
        let dtheta = std::f64::consts::TAU / n as f64;
        let arc = r * dtheta; // grid spacing along the path
        let x: Vec<f64> = (0..n).map(|i| r * (i as f64 * dtheta).cos()).collect();
        let y: Vec<f64> = (0..n).map(|i| r * (i as f64 * dtheta).sin()).collect();
        let k = curvature(&x, &y, arc);
        for &ki in &k {
            assert!((ki - 0.01).abs() < 1e-3, "kappa={ki}");
        }
    }

    #[test]
    fn straight_line_has_zero_curvature() {
        let x: Vec<f64> = (0..100).map(|i| i as f64 * 2.0).collect();
        let y = vec![0.0; 100];
        let k = curvature(&x, &y, 2.0);
        // Interior points (ignore the two wrap points at the ends).
        for ki in &k[1..99] {
            assert!(ki.abs() < 1e-9);
        }
    }
}
