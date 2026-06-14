//! Small robust-statistics helpers for the summary (spec §10).
//!
//! The summary uses **median-based** statistics throughout so a single noisy lap
//! cannot swing the result (spec §10.1).

/// Median of a slice (does not mutate the caller's data). Empty → `None`.
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m]
    } else {
        0.5 * (v[m - 1] + v[m])
    })
}

/// Median absolute deviation (scaled to ~σ for normal data, ×1.4826).
pub fn mad(values: &[f64]) -> Option<f64> {
    let med = median(values)?;
    let devs: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    median(&devs).map(|m| m * 1.4826)
}

/// Population standard deviation. Empty/one element → 0.0.
pub fn std_dev(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
    var.sqrt()
}

/// Theil–Sen robust slope: median of pairwise slopes over `(x_i, y_i)`. `None`
/// if fewer than two points. Used for the trend fit (spec §10.1a).
pub fn theil_sen_slope(xs: &[f64], ys: &[f64]) -> Option<f64> {
    let n = xs.len();
    if n < 2 || ys.len() != n {
        return None;
    }
    let mut slopes = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = xs[j] - xs[i];
            if dx.abs() > f64::EPSILON {
                slopes.push((ys[j] - ys[i]) / dx);
            }
        }
    }
    median(&slopes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_and_mad() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), Some(2.5));
        // MAD of a symmetric set.
        let m = mad(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        assert!((m - 1.4826).abs() < 1e-6);
    }

    #[test]
    fn theil_sen_detects_downtrend() {
        let xs: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().map(|x| 100.0 - 2.0 * x).collect();
        let slope = theil_sen_slope(&xs, &ys).unwrap();
        assert!((slope + 2.0).abs() < 1e-9);
    }
}
