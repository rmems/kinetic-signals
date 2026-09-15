// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hurst exponent estimation via rescaled-range (R/S) analysis.
//!
//! The Hurst exponent \( H \in [0, 1] \) characterises long-range dependence
//! in a time series:
//!
//! | \( H \) | Interpretation |
//! |---------|----------------|
//! | \( > 0.5 \) | Persistent (trending / long memory) |
//! | \( \approx 0.5 \) | Uncorrelated (random walk) |
//! | \( < 0.5 \) | Antipersistent (mean-reverting) |
//!
//! [`compute_hurst`] uses logarithmically spaced window sizes and linear
//! regression on the log-log R/S plot. Series shorter than 32 samples return
//! \( H = 0.5 \) with neither persistence flag set.
//!
//! Supports any scalar type implementing the crate's private `Real` trait
//! (`f32` / `f64`).

use crate::numeric::all_finite;
use crate::real::Real;

/// Result of a Hurst exponent estimate.
#[derive(Debug, Clone)]
pub struct HurstResult<T = f64> {
    /// Estimated Hurst exponent, clamped to \([0, 1]\).
    pub h: T,
    /// `true` when \( H > 0.52 \) (small buffer above random-walk).
    pub is_persistent: bool,
    /// `true` when \( H < 0.48 \) (small buffer below random-walk).
    pub is_antipersistent: bool,
}

fn c<T: Real>(value: f64) -> T {
    T::from_f64(value)
}

fn underdetermined<T: Real>() -> HurstResult<T> {
    HurstResult {
        h: c(0.5),
        is_persistent: false,
        is_antipersistent: false,
    }
}

fn welford_mean<T: Real>(chunk: &[T]) -> T {
    let mut mean = T::zero();
    for (i, &x) in chunk.iter().enumerate() {
        mean = mean + (x - mean) / T::from_usize(i + 1);
    }
    mean
}

fn chunk_rs<T: Real>(chunk: &[T]) -> Option<T> {
    let tau = chunk.len();
    let mean = welford_mean(chunk);
    let mut cumdev = T::zero();
    let mut max_dev = T::zero();
    let mut min_dev = T::zero();
    let mut sq_diff_sum = T::zero();
    for &x in chunk {
        let diff = x - mean;
        cumdev = cumdev + diff;
        max_dev = max_dev.max(cumdev);
        min_dev = min_dev.min(cumdev);
        sq_diff_sum = sq_diff_sum + diff * diff;
    }
    let std_dev = (sq_diff_sum / T::from_usize(tau)).sqrt();
    if std_dev > c(1e-12) {
        Some((max_dev - min_dev) / std_dev)
    } else {
        None
    }
}

fn log_rs_point<T: Real>(data: &[T], tau: usize) -> Option<(T, T)> {
    let n = data.len();
    let mut rs_sums = T::zero();
    let mut count = 0;
    for i in (0..=(n - tau)).step_by(tau) {
        if let Some(rs) = chunk_rs(&data[i..i + tau]) {
            rs_sums = rs_sums + rs;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let rs_avg = rs_sums / T::from_usize(count);
    if rs_avg > T::zero() {
        Some((T::from_usize(tau).ln(), rs_avg.ln()))
    } else {
        None
    }
}

fn tau_schedule(n: usize) -> Vec<usize> {
    let mut tau_values = Vec::new();
    let mut current_tau = 8usize;
    while current_tau <= n / 2 {
        tau_values.push(current_tau);
        current_tau = (current_tau as f64 * 1.4).ceil() as usize;
        if tau_values.len() >= 30 {
            break;
        }
    }
    tau_values
}

fn slope_from_loglog<T: Real>(log_n: &[T], log_rs: &[T]) -> T {
    if log_n.len() < 2 {
        return c(0.5);
    }
    let n_mean =
        log_n.iter().copied().fold(T::zero(), |acc, x| acc + x) / T::from_usize(log_n.len());
    let rs_mean =
        log_rs.iter().copied().fold(T::zero(), |acc, x| acc + x) / T::from_usize(log_rs.len());
    let num = log_n
        .iter()
        .zip(log_rs.iter())
        .fold(T::zero(), |acc, (&x, &y)| {
            acc + (x - n_mean) * (y - rs_mean)
        });
    let den = log_n
        .iter()
        .fold(T::zero(), |acc, &x| acc + (x - n_mean).powi(2));
    if den.abs() < c(1e-12) {
        c(0.5)
    } else {
        num / den
    }
}

/// Estimate the Hurst exponent of `data` using R/S analysis.
///
/// Returns \( H = 0.5 \) (no persistence flags) when fewer than 32 samples
/// are provided, any sample is non-finite, every window is degenerate
/// (constant / near-constant, so R/S is undefined), or the log-log
/// regression is underdetermined.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::compute_hurst;
///
/// let trending: Vec<f64> = (0..100).map(|i| i as f64).collect();
/// let result = compute_hurst(&trending);
/// assert!(result.h >= 0.0 && result.h <= 1.0);
/// ```
pub fn compute_hurst<T>(data: &[T]) -> HurstResult<T>
where
    T: Real,
{
    if data.len() < 32 || !all_finite(data) {
        return underdetermined();
    }

    let mut log_rs = Vec::new();
    let mut log_n = Vec::new();
    for &tau in &tau_schedule(data.len()) {
        if let Some((ln_n, ln_rs)) = log_rs_point(data, tau) {
            log_n.push(ln_n);
            log_rs.push(ln_rs);
        }
    }

    let h = slope_from_loglog(&log_n, &log_rs)
        .max(T::zero())
        .min(T::one());
    HurstResult {
        h,
        is_persistent: h > c(0.52),
        is_antipersistent: h < c(0.48),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hurst_basic() {
        let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let result = compute_hurst(&data);
        assert!(result.h >= 0.0 && result.h <= 1.0);
    }

    #[test]
    fn test_hurst_f32_support() {
        let data: Vec<f32> = (0..64).map(|i| i as f32 * 0.1).collect();
        let result = compute_hurst(&data);
        assert!(result.h >= 0.0_f32 && result.h <= 1.0_f32);
    }

    #[test]
    fn test_hurst_constant_is_half() {
        let data = vec![3.0_f64; 64];
        let result = compute_hurst(&data);
        assert_eq!(result.h, 0.5);
        assert!(!result.is_persistent);
        assert!(!result.is_antipersistent);
    }

    #[test]
    fn test_hurst_nonfinite_is_half() {
        let mut nan_data = vec![1.0_f64; 64];
        nan_data[10] = f64::NAN;
        let nan = compute_hurst(&nan_data);
        assert_eq!(nan.h, 0.5);
        assert!(!nan.is_persistent && !nan.is_antipersistent);

        let mut inf_data = vec![1.0_f64; 64];
        inf_data[10] = f64::INFINITY;
        let inf = compute_hurst(&inf_data);
        assert_eq!(inf.h, 0.5);
    }

    #[test]
    fn test_hurst_near_constant_is_finite() {
        let data: Vec<f64> = (0..64).map(|i| 1.0 + (i as f64) * 1e-18).collect();
        let result = compute_hurst(&data);
        assert!(result.h.is_finite());
        assert!((0.0..=1.0).contains(&result.h));
    }

    #[test]
    fn test_hurst_extreme_finite_is_finite() {
        let data: Vec<f64> = (0..64).map(|i| 1e12 + i as f64).collect();
        let result = compute_hurst(&data);
        assert!(result.h.is_finite());
        assert!((0.0..=1.0).contains(&result.h));
    }
}
