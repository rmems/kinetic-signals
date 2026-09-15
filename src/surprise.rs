// SPDX-License-Identifier: MIT OR Apache-2.0

//! Domain-agnostic surprise detection.
//!
//! Computes a normalized "surprise" score for a transition between two
//! consecutive positive samples of a stochastic signal. The score is the
//! absolute z-score of the observed log-ratio relative to an expected drift,
//! scaled by the per-step standard deviation.
//!
//! [`compute_surprise_sequence`] / [`compute_surprise_sequence_into`] score
//! every consecutive pair. The allocating function is a thin wrapper around
//! the `*_into` core so telemetry loops can reuse a caller-owned `Vec`.
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

/// Compute the surprise score for a single transition.
///
/// Returns a zeroed result (no surprise) if either value is non-positive,
/// since the log-ratio is undefined for non-positive inputs.
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
    if previous_value <= T::zero() || current_value <= T::zero() {
        return SurpriseResult {
            surprise: T::zero(),
            log_return: T::zero(),
            expected_return: params.mu * params.dt,
            z_score: T::zero(),
        };
    }

    let log_return = (current_value / previous_value).ln();

    let expected_return = params.mu * params.dt;

    let std_dev = params.sigma * params.dt.sqrt();

    let z_score = if std_dev > T::zero() {
        (log_return - expected_return) / std_dev
    } else {
        T::zero()
    };

    let surprise = z_score.abs();

    SurpriseResult {
        surprise,
        log_return,
        expected_return,
        z_score,
    }
}

/// Number of consecutive transitions (and therefore output slots) in a
/// surprise sequence of `n` samples.
///
/// This is `n.saturating_sub(1)`: empty and single-sample inputs produce no
/// transitions.
pub fn surprise_sequence_len(n: usize) -> usize {
    n.saturating_sub(1)
}

/// Compute surprise scores for every consecutive transition in `values`.
///
/// Allocates a fresh output `Vec`. Prefer [`compute_surprise_sequence_into`]
/// when a caller-owned buffer can be reused across windows.
///
/// The returned length is [`surprise_sequence_len`] applied to `values.len()`.
pub fn compute_surprise_sequence<T>(
    values: &[T],
    params: &SurpriseParams<T>,
) -> Vec<SurpriseResult<T>>
where
    T: Real,
{
    let mut out = Vec::with_capacity(surprise_sequence_len(values.len()));
    compute_surprise_sequence_into(values, params, &mut out);
    out
}

/// Write surprise scores for every consecutive transition into `out`.
///
/// This is the hot output path for batch surprise: high-frequency telemetry
/// loops can keep one `Vec` and reuse it for each window instead of allocating
/// on every call.
///
/// # Buffer length and overwrite
///
/// - `out` is resized to `surprise_sequence_len(values.len())`. Growing
///   past the current **capacity** may allocate; shrinking only truncates.
/// - Every slot is then **overwritten** with the result for `values[i-1]` →
///   `values[i]`. This function never calls `shrink_to_fit`.
/// - If `out.capacity() >= surprise_sequence_len(values.len())` before the
///   call, **no output allocation** is performed.
///
/// # Aliasing
///
/// `values` is borrowed immutably and `out` is borrowed mutably for the
/// duration of the call. In safe Rust they cannot alias: the input element
/// type `T` (`f32` / `f64`) is distinct from [`SurpriseResult<T>`]. Results
/// are written only to `out`; `values` is never mutated.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::{SurpriseParams, compute_surprise_sequence_into};
///
/// let params = SurpriseParams::default();
/// let window = [100.0, 100.5, 101.0, 100.8];
/// let mut out = Vec::with_capacity(window.len() - 1);
/// compute_surprise_sequence_into(&window, &params, &mut out);
/// assert_eq!(out.len(), 3);
///
/// // Next window: same buffer, no output allocation when capacity is enough.
/// let next = [100.8, 101.2, 150.0];
/// compute_surprise_sequence_into(&next, &params, &mut out);
/// assert_eq!(out.len(), 2);
/// ```
pub fn compute_surprise_sequence_into<T>(
    values: &[T],
    params: &SurpriseParams<T>,
    out: &mut Vec<SurpriseResult<T>>,
) where
    T: Real,
{
    let n = surprise_sequence_len(values.len());
    if out.len() != n {
        out.resize_with(n, || SurpriseResult {
            surprise: T::zero(),
            log_return: T::zero(),
            expected_return: T::zero(),
            z_score: T::zero(),
        });
    }
    for (slot, pair) in out.iter_mut().zip(values.windows(2)) {
        *slot = compute_surprise(pair[1], pair[0], params);
    }
}

/// Return `true` if the result's surprise exceeds the configured threshold.
pub fn detect_anomaly<T>(result: &SurpriseResult<T>, params: &SurpriseParams<T>) -> bool
where
    T: Real,
{
    result.surprise > params.threshold
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

    fn assert_results_match(left: &[SurpriseResult], right: &[SurpriseResult]) {
        assert_eq!(left.len(), right.len());
        for (a, b) in left.iter().zip(right) {
            assert_eq!(a.surprise, b.surprise);
            assert_eq!(a.log_return, b.log_return);
            assert_eq!(a.expected_return, b.expected_return);
            assert_eq!(a.z_score, b.z_score);
        }
    }

    fn assert_allocating_matches_into(values: &[f64], params: &SurpriseParams) {
        let allocated = compute_surprise_sequence(values, params);
        let mut reused = vec![SurpriseResult {
            surprise: f64::NAN,
            log_return: f64::NAN,
            expected_return: f64::NAN,
            z_score: f64::NAN,
        }];
        compute_surprise_sequence_into(values, params, &mut reused);
        assert_eq!(reused.len(), surprise_sequence_len(values.len()));
        assert_results_match(&allocated, &reused);
    }

    #[test]
    fn test_surprise_sequence_into_matches_allocating() {
        let params = SurpriseParams::default();
        assert_allocating_matches_into(&[], &params);
        assert_allocating_matches_into(&[1.0], &params);
        assert_allocating_matches_into(&[100.0, 101.0], &params);
        assert_allocating_matches_into(&[100.0, 100.5, 100.2, 100.8, 100.4], &params);
    }

    #[test]
    fn test_surprise_sequence_into_resizes_and_keeps_capacity() {
        let params = SurpriseParams::default();
        let mut out: Vec<SurpriseResult> = Vec::new();

        compute_surprise_sequence_into(&[], &params, &mut out);
        assert!(out.is_empty());

        compute_surprise_sequence_into(&[42.0], &params, &mut out);
        assert!(out.is_empty());

        let exact = [100.0, 110.0];
        compute_surprise_sequence_into(&exact, &params, &mut out);
        assert_eq!(out.len(), 1);
        let cap_after_exact = out.capacity();
        assert!(cap_after_exact >= 1);

        let multi = [100.0, 100.5, 101.0, 150.0, 149.0];
        compute_surprise_sequence_into(&multi, &params, &mut out);
        assert_eq!(out.len(), 4);
        let cap_after_multi = out.capacity();
        assert!(cap_after_multi >= cap_after_exact);
        assert!(cap_after_multi >= 4);

        compute_surprise_sequence_into(&exact, &params, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out.capacity(), cap_after_multi);
    }

    #[test]
    fn test_surprise_sequence_len() {
        assert_eq!(surprise_sequence_len(0), 0);
        assert_eq!(surprise_sequence_len(1), 0);
        assert_eq!(surprise_sequence_len(2), 1);
        assert_eq!(surprise_sequence_len(8), 7);
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
}
