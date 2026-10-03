// SPDX-License-Identifier: MIT OR Apache-2.0

//! Regression and property checks for non-finite / ill-conditioned inputs.

use kinetic_signals::{
    EMA, SMA, VolEstimator, ZScore, compute_hawkes, compute_hawkes_streaming, compute_hurst,
    compute_shannon_entropy, compute_signal_stats, compute_surprise, compute_surprise_sequence,
    detect_anomaly, hawkes::HawkesParams, surprise::SurpriseParams,
};

fn lcg_next(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

fn bounded_f64(state: &mut u64, lo: f64, hi: f64) -> f64 {
    let x = lcg_next(state) >> 11;
    let unit = (x as f64) / ((1u64 << 53) as f64);
    lo + unit * (hi - lo)
}

fn bounded_series(state: &mut u64, n: usize, lo: f64, hi: f64) -> Vec<f64> {
    (0..n).map(|_| bounded_f64(state, lo, hi)).collect()
}

#[test]
fn property_hurst_finite_for_bounded_inputs() {
    let mut rng = 0x1111_2222_3333_4444;
    for _ in 0..32 {
        let data = bounded_series(&mut rng, 64, -1e3, 1e3);
        let r = compute_hurst(&data);
        assert!(r.h.is_finite());
        assert!((0.0..=1.0).contains(&r.h));
    }
}

#[test]
fn property_stats_finite_for_bounded_inputs() {
    let mut rng = 0xaaaa_bbbb_cccc_dddd;
    for _ in 0..32 {
        let data = bounded_series(&mut rng, 16, -1e6, 1e6);
        let s = compute_signal_stats(&data);
        assert_eq!(s.count, data.len());
        assert!(s.mean.is_finite());
        assert!(s.variance.is_finite() && s.variance >= 0.0);
        assert!(s.skewness.is_finite());
        assert!(s.kurtosis.is_finite());
    }
}

#[test]
fn property_entropy_finite_for_bounded_inputs() {
    let mut rng = 0xfeed_face_dead_beef;
    for _ in 0..32 {
        let data = bounded_series(&mut rng, 20, -10.0, 10.0);
        let r = compute_shannon_entropy(&data, 8);
        assert!(r.shannon.is_finite() && r.shannon >= 0.0);
        assert!(r.relative.is_finite());
        assert!((0.0..=1.0).contains(&r.relative));
    }
}

#[test]
fn property_surprise_finite_for_positive_bounded_inputs() {
    let mut rng = 0x0123_4567_89ab_cdef;
    let params = SurpriseParams::default();
    for _ in 0..32 {
        let values = bounded_series(&mut rng, 8, 1e-3, 1e3);
        let results = compute_surprise_sequence(&values, &params);
        assert_eq!(results.len(), values.len() - 1);
        for r in &results {
            assert!(r.surprise.is_finite() && r.surprise >= 0.0);
            assert!(r.z_score.is_finite());
            assert!(r.log_return.is_finite());
            assert!(r.expected_return.is_finite());
            assert_eq!(detect_anomaly(r, &params), r.surprise > params.threshold);
        }
    }
}

#[test]
fn hawkes_excitation_overflow_is_not_a_finite_equivalence_case() {
    for beta in [0.0, -0.0, 1.0] {
        let params = HawkesParams {
            mu: 0.0,
            alpha: f64::MAX,
            beta,
            dt: 0.001,
        };
        let first = compute_hawkes(&[0.0], &params);
        assert_eq!(first.event_count, 1);
        assert_eq!(first.intensity, f64::MAX);
        assert_eq!(first.avg_excitation, f64::MAX);

        // Two tied events contribute 2*MAX mathematically, outside f64 range.
        for count in [2, 3] {
            let batch = compute_hawkes(&vec![0.0; count], &params);
            assert_eq!(batch.event_count, 0, "{params:?}, count={count}");
            assert_eq!(batch.intensity, 0.0, "{params:?}, count={count}");
            assert_eq!(batch.avg_excitation, 0.0, "{params:?}, count={count}");
        }

        let first = compute_hawkes_streaming(0.0, 0.0, 0.0, &params, 0.0);
        assert_eq!(first, (0.0, 1.0), "{params:?}");
        let second = compute_hawkes_streaming(first.0, 0.0, 0.0, &params, first.1);
        assert_eq!(second, (f64::MAX, 2.0), "{params:?}");
        assert!((params.mu + params.alpha * second.1).is_infinite());
        // Streaming guards its pre-jump result, not the caller's post-event value.
        let third = compute_hawkes_streaming(second.0, 0.0, 0.0, &params, second.1);
        assert_eq!(third, (0.0, 2.0), "{params:?}");
    }
}

#[test]
fn hawkes_baseline_addition_overflow_preserves_streaming_state() {
    let params = HawkesParams {
        mu: f64::MAX,
        alpha: f64::MAX,
        ..HawkesParams::default()
    };
    let batch = compute_hawkes(&[0.0], &params);
    assert_eq!(batch.event_count, 0);
    assert_eq!(batch.intensity, f64::MAX);
    assert_eq!(batch.avg_excitation, 0.0);
    let (pre, state) = compute_hawkes_streaming(0.0, 0.0, 0.0, &params, 0.0);
    assert_eq!((pre, state), (f64::MAX, 1.0));
    assert_eq!(
        compute_hawkes_streaming(pre, 0.0, 0.0, &params, state),
        (f64::MAX, 1.0)
    );
}

#[test]
fn property_indicators_finite_for_bounded_inputs() {
    let mut rng = 0x1357_2468_ace0_bdf0;
    for _ in 0..32 {
        let mut ema = EMA::new(5);
        let mut sma = SMA::new(5);
        let mut vol = VolEstimator::new(5);
        for _ in 0..12 {
            let x = bounded_f64(&mut rng, -1e3, 1e3);
            let e = ema.update(x);
            let s = sma.update(x);
            vol.push(x.abs() as f32 * 1e-4);
            assert!(e.is_finite());
            assert!(s.is_finite());
            let z = ZScore::compute(x, s, 1.0);
            assert!(z.is_finite());
        }
        assert!(vol.rms().is_finite());
        assert!((0.0..=1.0).contains(&vol.rms()));
    }
}

#[test]
fn constant_windows_are_explicit_sentinels() {
    let hurst = compute_hurst(&[2.5_f64; 64]);
    assert_eq!(hurst.h, 0.5);
    assert!(!hurst.is_persistent && !hurst.is_antipersistent);

    let stats = compute_signal_stats(&[2.5; 8]);
    assert_eq!(stats.variance, 0.0);
    assert_eq!(stats.skewness, 0.0);
    assert_eq!(stats.kurtosis, 0.0);

    let entropy = compute_shannon_entropy(&[2.5; 8], 6);
    assert_eq!(entropy.shannon, 0.0);
    assert_eq!(entropy.bin_count, 1);

    let surprise = compute_surprise(2.5, 2.5, &SurpriseParams::default());
    assert_eq!(surprise.log_return, 0.0);
    assert_eq!(surprise.z_score, 0.0);
}

#[test]
fn nonfinite_inputs_are_explicit_sentinels() {
    let params = HawkesParams::default();
    let hawkes = compute_hawkes(&[0.0, f64::NAN], &params);
    assert_eq!(hawkes.event_count, 0);
    assert_eq!(hawkes.intensity, params.mu);

    let stats = compute_signal_stats(&[1.0, f64::INFINITY]);
    assert_eq!(stats.count, 0);

    let entropy = compute_shannon_entropy(&[1.0, f64::NEG_INFINITY], 4);
    assert_eq!(entropy.bin_count, 0);

    let surprise = compute_surprise(f64::NAN, 1.0, &SurpriseParams::default());
    assert_eq!(surprise.surprise, 0.0);
}

#[test]
fn sma_agrees_with_independent_window_mean() {
    let values = [1.0, 4.0, 2.0, 8.0, 3.0, 5.0];
    let cap = 3;
    let mut sma = SMA::new(cap);
    let mut window = Vec::new();
    for &v in &values {
        if window.len() == cap {
            window.remove(0);
        }
        window.push(v);
        let got = sma.update(v);
        let expected = window.iter().sum::<f64>() / window.len() as f64;
        assert!((got - expected).abs() < 1e-12);
    }
}
