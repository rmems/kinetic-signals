// SPDX-License-Identifier: MIT OR Apache-2.0

//! Higher-order moments of a real-valued signal.
//!
//! [`compute_signal_stats`] computes mean, variance, skewness, and excess
//! kurtosis in a single pass after the mean is known (two passes total over
//! the slice). Suitable for batch feature extraction; for streaming variance
//! prefer [`crate::VolEstimator`].

use crate::numeric::{all_finite, stable_mean};

/// Central moments and shape descriptors of a signal sample.
#[derive(Debug, Clone)]
pub struct SignalStats {
    /// Arithmetic mean.
    pub mean: f64,
    /// Population variance (\( m_2 / n \), not Bessel-corrected).
    pub variance: f64,
    /// Sample skewness (\( m_3 / \sigma^3 \)); `0.0` when variance is near zero.
    pub skewness: f64,
    /// Excess kurtosis (\( m_4 / \sigma^4 - 3 \)); `0.0` for a Gaussian or degenerate series.
    pub kurtosis: f64,
    /// Number of samples used.
    pub count: usize,
}

fn empty_stats() -> SignalStats {
    SignalStats {
        mean: 0.0,
        variance: 0.0,
        skewness: 0.0,
        kurtosis: 0.0,
        count: 0,
    }
}

/// Compute high-order moments for a signal using a single-pass algorithm
/// (after the mean is computed).
///
/// Returns an all-zero result for an empty slice. Any non-finite sample
/// yields the same empty sentinel (`count = 0`) so a poisoned window cannot
/// emit `NaN` moments. Overflow of second-or-higher moments (extreme
/// opposite-signed magnitudes) also yields the empty sentinel. Constant and
/// near-constant series return `0` skewness
/// and kurtosis instead of dividing by a vanishing standard deviation.
/// The mean is accumulated with Welford's method so extreme finite magnitudes
/// do not overflow the first-pass sum.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::compute_signal_stats;
///
/// let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
/// let stats = compute_signal_stats(&data);
/// assert_eq!(stats.mean, 3.0);
/// assert_eq!(stats.count, 5);
/// assert!(stats.variance > 0.0);
/// ```
pub fn compute_signal_stats(data: &[f64]) -> SignalStats {
    if data.is_empty() || !all_finite(data) {
        return empty_stats();
    }

    let n = data.len();
    let n_f = n as f64;
    let Some(mean) = stable_mean(data) else {
        return empty_stats();
    };

    let mut m2 = 0.0;
    let mut m3 = 0.0;
    let mut m4 = 0.0;
    for &x in data {
        let diff = x - mean;
        let d2 = diff * diff;
        m2 += d2;
        m3 += d2 * diff;
        m4 += d2 * d2;
    }

    let var = m2 / n_f;
    if !mean.is_finite() || !var.is_finite() {
        return empty_stats();
    }
    let std = var.sqrt();
    let skew = if std > 1e-12 && std.is_finite() && m3.is_finite() {
        (m3 / n_f) / (std * var)
    } else {
        0.0
    };
    let kurt = if var > 1e-12 && var.is_finite() && m4.is_finite() {
        (m4 / n_f) / (var * var) - 3.0
    } else {
        0.0
    };

    SignalStats {
        mean,
        variance: var,
        skewness: if skew.is_finite() { skew } else { 0.0 },
        kurtosis: if kurt.is_finite() { kurt } else { 0.0 },
        count: n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_stats() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let stats = compute_signal_stats(&data);
        assert_eq!(stats.mean, 3.0);
        assert!(stats.variance > 0.0);
        assert!(stats.skewness.abs() < 0.1);
    }

    #[test]
    fn test_signal_stats_empty() {
        let stats = compute_signal_stats(&[]);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.mean, 0.0);
        assert_eq!(stats.variance, 0.0);
        assert_eq!(stats.skewness, 0.0);
        assert_eq!(stats.kurtosis, 0.0);
    }

    #[test]
    fn test_signal_stats_single_element() {
        let stats = compute_signal_stats(&[7.5]);
        assert_eq!(stats.count, 1);
        assert_eq!(stats.mean, 7.5);
        assert_eq!(stats.variance, 0.0);
        assert_eq!(stats.skewness, 0.0);
        assert_eq!(stats.kurtosis, 0.0);
    }

    #[test]
    fn test_signal_stats_constant_values() {
        let data = vec![3.0, 3.0, 3.0, 3.0, 3.0];
        let stats = compute_signal_stats(&data);
        assert_eq!(stats.count, 5);
        assert_eq!(stats.mean, 3.0);
        assert_eq!(stats.variance, 0.0);
        assert_eq!(stats.skewness, 0.0);
        assert_eq!(stats.kurtosis, 0.0);
    }

    #[test]
    fn test_signal_stats_nonfinite_is_empty() {
        let nan = compute_signal_stats(&[1.0, f64::NAN, 3.0]);
        assert_eq!(nan.count, 0);
        assert_eq!(nan.mean, 0.0);
        assert!(nan.variance.is_finite() && nan.skewness.is_finite());

        let inf = compute_signal_stats(&[1.0, f64::INFINITY]);
        assert_eq!(inf.count, 0);
        assert_eq!(inf.mean, 0.0);
    }

    #[test]
    fn test_signal_stats_near_constant_no_nan() {
        let data = [1.0, 1.0 + 1e-18, 1.0];
        let stats = compute_signal_stats(&data);
        assert_eq!(stats.count, 3);
        assert!(stats.mean.is_finite());
        assert!(stats.variance.is_finite());
        assert!(stats.skewness.is_finite());
        assert!(stats.kurtosis.is_finite());
    }

    #[test]
    fn test_signal_stats_extreme_finite_mean() {
        let data = [1e12, 1e12 + 1.0, 1e12 + 2.0];
        let stats = compute_signal_stats(&data);
        assert_eq!(stats.count, 3);
        assert!((stats.mean - (1e12 + 1.0)).abs() < 1e-3);
        assert!(stats.variance.is_finite() && stats.variance > 0.0);
        assert!(stats.skewness.is_finite());
        assert!(stats.kurtosis.is_finite());
    }

    #[test]
    fn test_signal_stats_overflow_spread_is_empty() {
        let stats = compute_signal_stats(&[1e200, -1e200]);
        assert_eq!(stats.count, 0);
        assert_eq!(stats.mean, 0.0);
        assert_eq!(stats.variance, 0.0);
    }

    #[test]
    fn test_signal_stats_opposite_max_mean_not_inf() {
        let stats = compute_signal_stats(&[f64::MAX, -f64::MAX]);
        assert!(stats.mean.is_finite());
        assert!(stats.variance.is_finite());
    }
}
