// SPDX-License-Identifier: MIT OR Apache-2.0

use kinetic_signals::{HawkesParams, compute_hawkes, compute_hawkes_streaming};

fn streaming_decay_sum(events: &[f64], params: &HawkesParams) -> f64 {
    let mut decay_sum = 0.0;
    let mut last = events[0];
    for &t in events {
        let (_, d) = compute_hawkes_streaming(0.0, t, last, params, decay_sum);
        decay_sum = d;
        last = t;
    }
    decay_sum
}

#[test]
fn zero_beta_does_not_decay() {
    let params = HawkesParams {
        mu: 0.2,
        alpha: 0.5,
        beta: 0.0,
        dt: 0.001,
    };
    let events = [0.0, 1.0, 5.0, 100.0, 1_000.0];
    let batch = compute_hawkes(&events, &params);

    let expected = params.mu + params.alpha * events.len() as f64;
    assert_eq!(batch.event_count, events.len());
    assert!((batch.intensity - expected).abs() < 1e-12);

    let decay_sum = streaming_decay_sum(&events, &params);
    let post = params.mu + params.alpha * decay_sum;
    assert!((post - batch.intensity).abs() < 1e-12);
    assert!(post.is_finite() && batch.intensity.is_finite());
}

#[test]
fn negative_zero_beta_matches_positive_zero() {
    let params = HawkesParams {
        mu: 0.2,
        alpha: 0.5,
        beta: -0.0,
        dt: 0.001,
    };
    let events = [0.0, 2.0, 40.0, 900.0];
    let batch = compute_hawkes(&events, &params);

    let expected = params.mu + params.alpha * events.len() as f64;
    assert_eq!(batch.event_count, events.len());
    assert!((batch.intensity - expected).abs() < 1e-12);

    let decay_sum = streaming_decay_sum(&events, &params);
    let post = params.mu + params.alpha * decay_sum;
    assert!((post - batch.intensity).abs() < 1e-12);
}

#[test]
fn small_positive_beta_still_decays() {
    let params = HawkesParams {
        mu: 0.1,
        alpha: 0.5,
        beta: 1e-3,
        dt: 0.001,
    };
    let events = [0.0, 1.0, 2.0, 3.0, 4.0];
    let batch = compute_hawkes(&events, &params);
    assert_eq!(batch.event_count, events.len());

    let no_decay = params.mu + params.alpha * events.len() as f64;
    assert!(batch.intensity < no_decay);
    assert!(batch.intensity > params.mu);

    let decay_sum = streaming_decay_sum(&events, &params);
    let post = params.mu + params.alpha * decay_sum;
    assert!((post - batch.intensity).abs() < 1e-12);
}

#[test]
fn negative_beta_rejected() {
    let params = HawkesParams {
        mu: 0.3,
        alpha: 0.5,
        beta: -1.0,
        dt: 0.001,
    };
    let batch = compute_hawkes(&[0.0, 1.0, 2.0], &params);
    assert_eq!(batch.event_count, 0);
    assert_eq!(batch.intensity, params.mu);
    assert_eq!(batch.avg_excitation, 0.0);

    let (i, d) = compute_hawkes_streaming(0.0, 1.0, 0.0, &params, 2.0);
    assert_eq!(i, params.mu);
    assert_eq!(d, 2.0);
}

#[test]
fn nonfinite_beta_rejected() {
    for beta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let params = HawkesParams {
            mu: 0.25,
            alpha: 0.5,
            beta,
            dt: 0.001,
        };
        let batch = compute_hawkes(&[0.0, 1.0, 2.0], &params);
        assert_eq!(batch.event_count, 0);
        assert_eq!(batch.intensity, params.mu);

        let (i, d) = compute_hawkes_streaming(0.0, 1.0, 0.0, &params, 3.0);
        assert_eq!(i, params.mu);
        assert_eq!(d, 3.0);
    }
}

#[test]
fn repeated_timestamps_zero_gap() {
    let params = HawkesParams {
        mu: 0.1,
        alpha: 0.5,
        beta: 2.0,
        dt: 0.001,
    };
    let events = [1.0, 1.0, 1.0, 1.0];
    let batch = compute_hawkes(&events, &params);
    let expected = params.mu + params.alpha * events.len() as f64;
    assert_eq!(batch.event_count, events.len());
    assert!((batch.intensity - expected).abs() < 1e-12);

    let decay_sum = streaming_decay_sum(&events, &params);
    let post = params.mu + params.alpha * decay_sum;
    assert!((post - batch.intensity).abs() < 1e-12);
}

#[test]
fn large_finite_gap_zero_beta_no_nan() {
    let params = HawkesParams {
        mu: 0.2,
        alpha: 0.5,
        beta: 0.0,
        dt: 0.001,
    };
    let events = [0.0, f64::MAX];
    let batch = compute_hawkes(&events, &params);

    let expected = params.mu + params.alpha * events.len() as f64;
    assert_eq!(batch.event_count, events.len());
    assert!(batch.intensity.is_finite());
    assert!((batch.intensity - expected).abs() < 1e-12);

    let decay_sum = streaming_decay_sum(&events, &params);
    let post = params.mu + params.alpha * decay_sum;
    assert!(post.is_finite());
    assert!((post - batch.intensity).abs() < 1e-12);
}

#[test]
fn large_finite_gap_positive_beta_decays_to_baseline() {
    let params = HawkesParams {
        mu: 0.2,
        alpha: 0.5,
        beta: 1.0,
        dt: 0.001,
    };
    let events = [0.0, f64::MAX];
    let batch = compute_hawkes(&events, &params);
    assert_eq!(batch.event_count, events.len());
    assert!(batch.intensity.is_finite());
    assert!((batch.intensity - (params.mu + params.alpha)).abs() < 1e-9);
}
