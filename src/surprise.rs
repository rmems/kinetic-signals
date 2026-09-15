// SPDX-License-Identifier: MIT OR Apache-2.0

//! Domain-agnostic surprise detection.
//!
//! Computes a normalized "surprise" score for a transition between two
//! consecutive positive samples of a stochastic signal. The score is the
//! absolute z-score of the observed log-ratio relative to an expected drift,
//! scaled by the per-step standard deviation.
//!
//! This is a generic signal-processing primitive: it makes no financial-domain
//! assumptions. It can be applied to any strictly positive signal (sensor
//! magnitudes, firing rates, power readings, asset prices, etc.).
//!
//! Supports any scalar type implementing the crate's private `Real` trait
//! (`f32` / `f64`).

use crate::real::Real;

/// Result of a single surprise computation.
#[derive(Debug, Clone)]
pub struct SurpriseResult<T = f64> {
    /// Absolute z-score of the observed transition (always `>= 0`).
    pub surprise: T,
    /// Natural log-ratio of `current / previous`.
    pub log_return: T,
    /// Expected drift over one step (`mu * dt`).
    pub expected_return: T,
    /// Signed z-score of the observed transition.
    pub z_score: T,
}

/// Parameters controlling surprise detection.
#[derive(Debug, Clone)]
pub struct SurpriseParams<T = f64> {
    /// Expected drift rate.
    pub mu: T,
    /// Per-unit-time volatility.
    pub sigma: T,
    /// Time step between samples.
    pub dt: T,
    /// Absolute z-score above which a transition is flagged anomalous.
    pub threshold: T,
}

impl<T> Default for SurpriseParams<T>
where
    T: Real,
{
    fn default() -> Self {
        SurpriseParams {
            mu: T::zero(),
            sigma: T::from_f64(0.1),
            dt: T::from_f64(0.001),
            threshold: T::from_f64(3.0),
        }
    }
}

fn params_finite<T: Real>(params: &SurpriseParams<T>) -> bool {
    params.mu.is_finite()
        && params.sigma.is_finite()
        && params.dt.is_finite()
        && params.threshold.is_finite()
}

fn expected_return<T: Real>(params: &SurpriseParams<T>) -> T {
    if params.mu.is_finite() && params.dt.is_finite() {
        params.mu * params.dt
    } else {
        T::zero()
    }
}

fn zeroed<T: Real>(params: &SurpriseParams<T>) -> SurpriseResult<T> {
    SurpriseResult {
        surprise: T::zero(),
        log_return: T::zero(),
        expected_return: expected_return(params),
        z_score: T::zero(),
    }
}

/// Compute the surprise score for a single transition.
///
/// Returns a zeroed result (no surprise) if either value is non-positive or
/// non-finite, if any parameter is non-finite, or if `dt < 0` (the log-ratio
/// / z-score is undefined). A non-positive `sigma` yields `z_score = 0`
/// while still reporting the log-ratio of valid positive samples.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::{SurpriseParams, compute_surprise, detect_anomaly};
///
/// let params = SurpriseParams::default();
/// let result = compute_surprise(150.0, 100.0, &params);
/// assert!(result.surprise >= 0.0);
/// if detect_anomaly(&result, &params) {
///     println!("anomalous transition: z = {:.2}", result.z_score);
/// }
/// ```
pub fn compute_surprise<T>(
    current_value: T,
    previous_value: T,
    params: &SurpriseParams<T>,
) -> SurpriseResult<T>
where
    T: Real,
{
    if !params_finite(params)
        || !current_value.is_finite()
        || !previous_value.is_finite()
        || previous_value <= T::zero()
        || current_value <= T::zero()
        || params.dt < T::zero()
    {
        return zeroed(params);
    }

    let log_return = (current_value / previous_value).ln();
    let expected_return = params.mu * params.dt;
    let std_dev = params.sigma * params.dt.sqrt();
    let z_score = if std_dev > T::zero() && std_dev.is_finite() && log_return.is_finite() {
        (log_return - expected_return) / std_dev
    } else {
        T::zero()
    };
    let z_score = if z_score.is_finite() {
        z_score
    } else {
        T::zero()
    };

    SurpriseResult {
        surprise: z_score.abs(),
        log_return: if log_return.is_finite() {
            log_return
        } else {
            T::zero()
        },
        expected_return,
        z_score,
    }
}

/// Compute surprise scores for every consecutive transition in `values`.
///
/// Each pair is evaluated independently: a non-finite or non-positive sample
/// zeroes that step without dropping the sequence length
/// (`values.len().saturating_sub(1)`).
pub fn compute_surprise_sequence<T>(
    values: &[T],
    params: &SurpriseParams<T>,
) -> Vec<SurpriseResult<T>>
where
    T: Real,
{
    if values.len() < 2 {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(values.len() - 1);
    for i in 1..values.len() {
        results.push(compute_surprise(values[i], values[i - 1], params));
    }
    results
}

/// Return `true` if the result's surprise exceeds the configured threshold.
///
/// Non-finite surprise or threshold values are not treated as anomalies.
pub fn detect_anomaly<T>(result: &SurpriseResult<T>, params: &SurpriseParams<T>) -> bool
where
    T: Real,
{
    result.surprise.is_finite()
        && params.threshold.is_finite()
        && result.surprise > params.threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surprise_normal() {
        let params = SurpriseParams::default();
        let result = compute_surprise(100.0, 99.0, &params);
        assert!(result.log_return > 0.0_f64);
    }

    #[test]
    fn test_surprise_spike() {
        let params = SurpriseParams::default();
        let result = compute_surprise(200.0, 100.0, &params);
        assert!(result.surprise > 1.0_f64);
    }

    #[test]
    fn test_surprise_zero_protection() {
        let params = SurpriseParams::default();
        let result = compute_surprise(0.0, 100.0, &params);
        assert_eq!(result.surprise, 0.0_f64);
    }

    #[test]
    fn test_surprise_f32_support() {
        let params = SurpriseParams::<f32>::default();
        let result = compute_surprise(100.5_f32, 100.0_f32, &params);
        assert!(result.surprise >= 0.0_f32);
    }

    #[test]
    fn test_surprise_sequence_normal() {
        let params = SurpriseParams {
            mu: 0.0,
            sigma: 0.15,
            dt: 0.001,
            threshold: 3.0,
        };
        let values = vec![100.0_f64, 100.5, 100.2, 100.8, 100.4];
        let results = compute_surprise_sequence(&values, &params);
        assert_eq!(results.len(), values.len() - 1);
        for r in &results {
            assert!(r.surprise.is_finite());
            assert!(r.surprise >= 0.0);
            assert!(r.surprise <= params.threshold);
        }
    }

    #[test]
    fn test_surprise_sequence_anomalous_spike() {
        let params = SurpriseParams {
            mu: 0.0,
            sigma: 0.15,
            dt: 0.001,
            threshold: 3.0,
        };
        let values = vec![100.0, 100.5, 150.0, 149.5];
        let results = compute_surprise_sequence(&values, &params);
        assert_eq!(results.len(), 3);
        assert!(results[0].surprise <= params.threshold);
        assert!(results[1].surprise > params.threshold);
        assert!(results[1].surprise > results[0].surprise);
    }

    #[test]
    fn test_surprise_sequence_short_input() {
        let params = SurpriseParams::default();
        assert!(compute_surprise_sequence(&[], &params).is_empty());
        assert!(compute_surprise_sequence(&[1.0], &params).is_empty());
    }

    #[test]
    fn test_detect_anomaly_above_threshold() {
        let params = SurpriseParams {
            mu: 0.0,
            sigma: 0.15,
            dt: 0.001,
            threshold: 3.0,
        };
        let spike = compute_surprise(150.0, 100.0, &params);
        assert!(spike.surprise > params.threshold);
        assert!(detect_anomaly(&spike, &params));
    }

    #[test]
    fn test_detect_anomaly_below_threshold() {
        let params = SurpriseParams {
            mu: 0.0,
            sigma: 0.15,
            dt: 0.001,
            threshold: 3.0,
        };
        let calm = compute_surprise(100.5, 100.0, &params);
        assert!(calm.surprise <= params.threshold);
        assert!(!detect_anomaly(&calm, &params));
    }

    #[test]
    fn test_detect_anomaly_at_threshold() {
        let params = SurpriseParams {
            mu: 0.0,
            sigma: 0.1,
            dt: 0.001,
            threshold: 2.0,
        };
        let result = SurpriseResult {
            surprise: 2.0,
            log_return: 0.0,
            expected_return: 0.0,
            z_score: 2.0,
        };
        assert!(!detect_anomaly(&result, &params));
    }

    #[test]
    fn test_surprise_nonfinite_is_zeroed() {
        let params = SurpriseParams::default();
        for (cur, prev) in [
            (f64::NAN, 100.0),
            (100.0, f64::NAN),
            (f64::INFINITY, 100.0),
            (100.0, f64::NEG_INFINITY),
        ] {
            let r = compute_surprise(cur, prev, &params);
            assert_eq!(r.surprise, 0.0);
            assert_eq!(r.z_score, 0.0);
            assert_eq!(r.log_return, 0.0);
            assert!(r.expected_return.is_finite());
        }
    }

    #[test]
    fn test_surprise_equal_values_zero_log_return() {
        let params = SurpriseParams::default();
        let r = compute_surprise(42.0, 42.0, &params);
        assert_eq!(r.log_return, 0.0);
        assert!(r.surprise.is_finite());
        assert_eq!(r.z_score, 0.0);
    }

    #[test]
    fn test_surprise_zero_sigma_zero_z() {
        let params = SurpriseParams {
            sigma: 0.0,
            ..SurpriseParams::default()
        };
        let r = compute_surprise(200.0, 100.0, &params);
        assert_eq!(r.z_score, 0.0);
        assert_eq!(r.surprise, 0.0);
        assert!(r.log_return.is_finite() && r.log_return > 0.0);
    }

    #[test]
    fn test_detect_anomaly_nonfinite_is_false() {
        let params = SurpriseParams::default();
        let inf = SurpriseResult {
            surprise: f64::INFINITY,
            log_return: 0.0,
            expected_return: 0.0,
            z_score: f64::INFINITY,
        };
        assert!(!detect_anomaly(&inf, &params));
    }

    #[test]
    fn test_surprise_sequence_nan_step_zeroed_keeps_length() {
        let params = SurpriseParams::default();
        let values = [100.0, f64::NAN, 101.0];
        let results = compute_surprise_sequence(&values, &params);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].surprise, 0.0);
        assert_eq!(results[1].surprise, 0.0);
    }
}
