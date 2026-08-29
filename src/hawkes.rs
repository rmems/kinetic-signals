// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hawkes self-exciting point process intensity estimation.
//!
//! A Hawkes process models event streams where each event temporarily raises
//! the probability of future events (self-excitation). The conditional
//! intensity at time \( t \) is:
//!
//! \[
//! \lambda(t) = \mu + \sum_{t_i \le t} \alpha\, e^{-\beta (t - t_i)}
//! \]
//!
//! where \(\mu\) is the baseline rate, \(\alpha\) the excitation amplitude,
//! and \(\beta\) the exponential decay rate.
//!
//! [`compute_hawkes`] returns this **post-event** intensity at the last event
//! time (the sum includes that last event, so a single-event history yields
//! \(\mu + \alpha\)). The strict pre-event form uses \(t_i < t\) instead.
//!
//! Use [`compute_hawkes`] for batch estimation over a full event history, or
//! [`compute_hawkes_streaming`] for O(1) online updates.

/// Result of a batch Hawkes intensity estimate.
#[derive(Debug, Clone)]
pub struct HawkesResult {
    /// Conditional intensity \(\lambda(t)\) at the last event time.
    pub intensity: f64,
    /// Number of events in the input history.
    pub event_count: usize,
    /// Mean per-event excitation contribution at the last event time.
    pub avg_excitation: f64,
}

/// Parameters of an exponential-kernel Hawkes process.
#[derive(Debug, Clone)]
pub struct HawkesParams {
    /// Baseline (immigrant) intensity \(\mu \ge 0\).
    pub mu: f64,
    /// Excitation amplitude \(\alpha \ge 0\) added by each event.
    pub alpha: f64,
    /// Exponential decay rate \(\beta > 0\) of excitation.
    pub beta: f64,
    /// Nominal time step (unused by the current estimators; retained for API stability).
    pub dt: f64,
}

impl Default for HawkesParams {
    fn default() -> Self {
        HawkesParams {
            mu: 0.1,
            alpha: 0.5,
            beta: 1.0,
            dt: 0.001,
        }
    }
}

/// Estimate Hawkes intensity at the last event from a full event-time history.
///
/// Returns baseline intensity alone when `event_times` is empty.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::{HawkesParams, compute_hawkes};
///
/// let params = HawkesParams::default();
/// let events = vec![0.0, 0.01, 0.02, 0.1, 0.5];
/// let result = compute_hawkes(&events, &params);
/// assert!(result.intensity >= params.mu);
/// assert_eq!(result.event_count, events.len());
/// ```
pub fn compute_hawkes(event_times: &[f64], params: &HawkesParams) -> HawkesResult {
    if event_times.is_empty() {
        return HawkesResult {
            intensity: params.mu,
            event_count: 0,
            avg_excitation: 0.0,
        };
    }

    let mut excitations: Vec<f64> = Vec::new();

    let &last_time = event_times.last().unwrap();

    for &t in event_times {
        let excitation = params.alpha * (-params.beta * (last_time - t)).exp();
        excitations.push(excitation);
    }

    let intensity = params.mu + excitations.iter().sum::<f64>();

    let avg_excitation = if excitations.is_empty() {
        0.0
    } else {
        excitations.iter().sum::<f64>() / excitations.len() as f64
    };

    HawkesResult {
        intensity,
        event_count: event_times.len(),
        avg_excitation,
    }
}

/// Online Hawkes update for a newly observed event.
///
/// Maintains a running decayed event-count sum so each step is O(1).
///
/// # Parameters
///
/// * `_prev_intensity` — previous intensity (currently unused; reserved for API stability)
/// * `new_event_time` — timestamp of the newly arrived event
/// * `last_event_time` — timestamp of the previous event
/// * `params` — Hawkes process parameters
/// * `decay_sum` — running sum of past events after exponential decay
///
/// # Returns
///
/// A tuple `(new_intensity, new_decay_sum)`:
/// - `new_intensity` — \(\mu + \alpha \cdot\) decayed sum just before incorporating the new event
/// - `new_decay_sum` — decayed sum after appending the new event (pass this on the next call)
///
/// # Example
///
/// ```rust
/// use kinetic_signals::{HawkesParams, compute_hawkes_streaming};
///
/// let params = HawkesParams::default();
/// let mut decay_sum = 0.0;
/// let mut last_t = 0.0;
///
/// for &t in &[0.1, 0.15, 0.5] {
///     let (intensity, new_sum) =
///         compute_hawkes_streaming(0.0, t, last_t, &params, decay_sum);
///     decay_sum = new_sum;
///     last_t = t;
///     assert!(intensity >= params.mu);
/// }
/// ```
pub fn compute_hawkes_streaming(
    _prev_intensity: f64,
    new_event_time: f64,
    last_event_time: f64,
    params: &HawkesParams,
    decay_sum: f64,
) -> (f64, f64) {
    let dt = new_event_time - last_event_time;

    let decayed_sum = decay_sum * (-params.beta * dt).exp();

    let new_intensity = params.mu + params.alpha * decayed_sum;

    let new_decay_sum = decayed_sum + 1.0;

    (new_intensity, new_decay_sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hawkes_basic() {
        let events = vec![0.0, 0.1, 0.2, 0.3, 0.4];
        let params = HawkesParams::default();
        let result = compute_hawkes(&events, &params);
        assert!(result.intensity > params.mu);
        assert_eq!(result.event_count, 5);
    }

    #[test]
    fn test_hawkes_empty() {
        let events: Vec<f64> = vec![];
        let params = HawkesParams::default();
        let result = compute_hawkes(&events, &params);
        assert_eq!(result.intensity, params.mu);
        assert_eq!(result.event_count, 0);
    }

    #[test]
    fn test_hawkes_streaming_single_event() {
        let params = HawkesParams::default();
        let (intensity, decay_sum) = compute_hawkes_streaming(params.mu, 0.0, 0.0, &params, 0.0);
        assert!((intensity - params.mu).abs() < 1e-12);
        assert!((decay_sum - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_hawkes_streaming_empty_prior() {
        let params = HawkesParams {
            mu: 0.2,
            alpha: 0.5,
            beta: 1.0,
            dt: 0.001,
        };
        let (intensity, decay_sum) = compute_hawkes_streaming(0.0, 1.0, 0.0, &params, 0.0);
        assert!((intensity - params.mu).abs() < 1e-12);
        assert!((decay_sum - 1.0).abs() < 1e-12);
    }

    #[test]
    fn test_hawkes_streaming_incremental() {
        let params = HawkesParams {
            mu: 0.1,
            alpha: 0.8,
            beta: 2.0,
            dt: 0.001,
        };

        let events = [0.0, 0.01, 0.02, 0.5];
        let mut intensity = params.mu;
        let mut decay_sum = 0.0;
        let mut last_t = events[0];

        let (i0, d0) = compute_hawkes_streaming(intensity, last_t, last_t, &params, decay_sum);
        intensity = i0;
        decay_sum = d0;
        assert!((intensity - params.mu).abs() < 1e-12);
        assert!((decay_sum - 1.0).abs() < 1e-12);

        for &t in &events[1..] {
            let (i, d) = compute_hawkes_streaming(intensity, t, last_t, &params, decay_sum);
            intensity = i;
            decay_sum = d;
            last_t = t;
            assert!(intensity >= params.mu);
            assert!(decay_sum > 0.0);
            assert!(intensity.is_finite());
            assert!(decay_sum.is_finite());
        }

        assert!(intensity > params.mu);

        let (sparse_i, _) =
            compute_hawkes_streaming(intensity, last_t + 10.0, last_t, &params, decay_sum);
        assert!(sparse_i < intensity);
        assert!((sparse_i - params.mu).abs() < 0.01);
    }
}
