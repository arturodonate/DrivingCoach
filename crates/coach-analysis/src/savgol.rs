//! Savitzky–Golay smoothing (spec §5.1).
//!
//! We smooth `x(s)` and `y(s)` *before* differentiating for curvature, because
//! raw second derivatives of telemetry are numerically explosive (spec §5.1).
//! Coefficients are derived from the least-squares polynomial fit over a sliding
//! window; the convolution preserves polynomials up to the fit order.
//!
//! Track paths are closed loops, so smoothing uses **wrap-around** edges.

/// Solve for the SG smoothing weights (derivative 0) over a window of
/// `2*half + 1` points fitting a polynomial of `poly_order`.
pub fn savgol_coeffs(half: usize, poly_order: usize) -> Vec<f64> {
    let p = poly_order;
    let window = 2 * half + 1;
    // Normal-equation matrix (A^T A)[k][l] = Σ_j j^(k+l), j ∈ [-half, half].
    let dim = p + 1;
    let mut ata = vec![vec![0.0_f64; dim]; dim];
    for (k, row) in ata.iter_mut().enumerate() {
        for (l, cell) in row.iter_mut().enumerate() {
            let mut s = 0.0;
            for j in -(half as i64)..=(half as i64) {
                s += (j as f64).powi((k + l) as i32);
            }
            *cell = s;
        }
    }
    let inv = invert(&ata).expect("SG normal matrix is invertible for sane window/order");

    // weight_j = Σ_k inv[0][k] * j^k
    let mut w = Vec::with_capacity(window);
    for j in -(half as i64)..=(half as i64) {
        let mut wj = 0.0;
        for (k, inv0k) in inv[0].iter().enumerate() {
            wj += inv0k * (j as f64).powi(k as i32);
        }
        w.push(wj);
    }
    w
}

/// Apply a symmetric SG smoothing kernel to `data`. `wrap` selects periodic
/// (closed-loop) vs. clamped edge handling.
pub fn convolve(data: &[f64], coeffs: &[f64], wrap: bool) -> Vec<f64> {
    let n = data.len();
    let half = coeffs.len() / 2;
    let mut out = vec![0.0; n];
    for (i, out_i) in out.iter_mut().enumerate() {
        let mut acc = 0.0;
        for (k, &c) in coeffs.iter().enumerate() {
            let off = k as i64 - half as i64;
            let idx = if wrap {
                ((i as i64 + off).rem_euclid(n as i64)) as usize
            } else {
                (i as i64 + off).clamp(0, n as i64 - 1) as usize
            };
            acc += c * data[idx];
        }
        *out_i = acc;
    }
    out
}

/// Convenience: smooth `data` with a window of `window_m` metres at the given
/// grid `step_m` and polynomial order. Window is rounded to the nearest odd
/// sample count ≥ `poly_order + 1`.
pub fn smooth(data: &[f64], window_m: f64, step_m: f64, poly_order: usize, wrap: bool) -> Vec<f64> {
    let mut win = (window_m / step_m).round() as usize;
    if win < poly_order + 1 {
        win = poly_order + 1;
    }
    if win % 2 == 0 {
        win += 1;
    }
    let half = win / 2;
    let coeffs = savgol_coeffs(half, poly_order);
    convolve(data, &coeffs, wrap)
}

/// Invert a small square matrix via Gauss–Jordan elimination. `None` if singular.
fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut inv = vec![vec![0.0; n]; n];
    for (i, row) in inv.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for col in 0..n {
        // Partial pivot.
        let mut pivot = col;
        for r in (col + 1)..n {
            if a[r][col].abs() > a[pivot][col].abs() {
                pivot = r;
            }
        }
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        inv.swap(col, pivot);

        let d = a[col][col];
        for k in 0..n {
            a[col][k] /= d;
            inv[col][k] /= d;
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = a[r][col];
            for k in 0..n {
                a[r][k] -= f * a[col][k];
                inv[r][k] -= f * inv[col][k];
            }
        }
    }
    Some(inv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coeffs_sum_to_one() {
        // A smoothing kernel must preserve constants: weights sum to 1.
        let w = savgol_coeffs(5, 3);
        let sum: f64 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn preserves_quadratic_in_interior() {
        // SG with poly_order ≥ 2 reproduces a quadratic exactly (interior).
        let f = |x: f64| 3.0 * x * x - 2.0 * x + 7.0;
        let data: Vec<f64> = (0..100).map(|i| f(i as f64)).collect();
        let coeffs = savgol_coeffs(5, 3);
        let out = convolve(&data, &coeffs, false);
        for i in 10..90 {
            assert!((out[i] - data[i]).abs() < 1e-6, "i={i}");
        }
    }

    #[test]
    fn suppresses_noise_on_a_line() {
        // Alternating ±1 noise on a flat line should be largely removed.
        let data: Vec<f64> = (0..200)
            .map(|i| 50.0 + if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        let out = smooth(&data, 11.0, 1.0, 2, true);
        let max_dev = out.iter().map(|v| (v - 50.0).abs()).fold(0.0, f64::max);
        assert!(max_dev < 0.5, "noise not suppressed: {max_dev}");
    }
}
