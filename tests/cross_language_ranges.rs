// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden-vector parity checks (issue #3 / LIM-31, issue #28 / LIM-201).
//!
//! This test pins the documented output-range convention and reference values
//! for `kinetic-signals`. The canonical shared vectors and their expected
//! ranges / reference outputs live in `tests/fixtures/shared_vectors.json`;
//! the values below mirror that fixture so consumers can assert against
//! identical inputs.
//!
//! These fixtures are the Rust-side golden set for determinism. SpikeStream.jl
//! no longer implements Hurst/Hawkes/GBM proxies; any future binding parity
//! should re-validate against the same JSON within the documented tolerances.

use kinetic_signals::{
    VolEstimator, compute_hawkes, compute_hawkes_streaming, compute_hurst, compute_shannon_entropy,
    compute_signal_stats, compute_surprise, compute_surprise_sequence, detect_anomaly,
    hawkes::HawkesParams, surprise::SurpriseParams,
};

const TOL: f64 = 1e-6;

/// Fixture content — keeps tests honest about which keys the JSON documents.
const SHARED_VECTORS_JSON: &str = include_str!("fixtures/shared_vectors.json");

// Shared input vectors (mirror of tests/fixtures/shared_vectors.json).
fn hurst_trending() -> Vec<f64> {
    (0..64).map(|i| i as f64 * 0.1).collect()
}

fn hawkes_events() -> Vec<f64> {
    vec![0.0, 0.01, 0.02, 0.03, 0.1, 0.5, 0.51, 0.52]
}

fn entropy_signal() -> Vec<f64> {
    vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 5.0, 6.0]
}

fn vol_returns() -> Vec<f32> {
    vec![0.01, 0.02, 0.015, 0.03, 0.012, 0.025, 0.018]
}

fn surprise_sequence_values() -> Vec<f64> {
    vec![100.0, 100.0, 101.0, 150.0, 148.0]
}

fn signal_stats_data() -> Vec<f64> {
    vec![1.0, 2.0, 3.0, 4.0, 5.0]
}

fn signal_stats_skewed_data() -> Vec<f64> {
    vec![1.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 5.0, 10.0]
}

fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn fixture_documents_required_vector_keys() {
    // Assert the JSON documents every vector key exercised by this suite
    // (batch + streaming expansions from GH#28 / LIM-201).
    for key in [
        "\"hurst\"",
        "\"hawkes\"",
        "\"hawkes_streaming\"",
        "\"hawkes_streaming_sequence\"",
        "\"surprise\"",
        "\"surprise_sequence\"",
        "\"entropy\"",
        "\"volatility\"",
        "\"signal_stats\"",
        "\"signal_stats_skewed\"",
        "\"tolerance\"",
    ] {
        assert!(
            SHARED_VECTORS_JSON.contains(key),
            "shared_vectors.json missing key {key}"
        );
    }
}

#[test]
fn hurst_within_unit_interval() {
    let r = compute_hurst(&hurst_trending());
    assert!(r.h.is_finite());
    assert!((0.0..=1.0).contains(&r.h), "hurst H out of [0,1]: {}", r.h);
    // Deterministic.
    let r2 = compute_hurst(&hurst_trending());
    assert!((r.h - r2.h).abs() < TOL);
}

#[test]
fn hawkes_intensity_at_least_baseline() {
    let params = HawkesParams::default();
    let r = compute_hawkes(&hawkes_events(), &params);
    assert!(r.intensity.is_finite());
    assert!(
        r.intensity >= params.mu,
        "intensity {} below baseline mu {}",
        r.intensity,
        params.mu
    );
    assert!(r.avg_excitation >= 0.0);
    assert_eq!(r.event_count, hawkes_events().len());
}

#[test]
fn hawkes_streaming_single_step_matches_expected() {
    // Mirror of vectors.hawkes_streaming in shared_vectors.json.
    let params = HawkesParams {
        mu: 0.1,
        alpha: 0.5,
        beta: 1.0,
        dt: 0.001,
    };
    let (intensity, new_decay_sum) = compute_hawkes_streaming(0.1, 0.51, 0.5, &params, 2.5);

    assert!(intensity.is_finite() && new_decay_sum.is_finite());
    assert!(
        intensity >= params.mu,
        "streaming intensity {intensity} below mu {}",
        params.mu
    );
    assert!(new_decay_sum >= 0.0);
    assert!(
        approx_eq(intensity, 1.33756229218646, TOL),
        "intensity {intensity} != expected"
    );
    assert!(
        approx_eq(new_decay_sum, 3.47512458437292, TOL),
        "new_decay_sum {new_decay_sum} != expected"
    );
}

#[test]
fn hawkes_streaming_sequence_matches_expected() {
    // Mirror of vectors.hawkes_streaming_sequence in shared_vectors.json.
    let params = HawkesParams::default();
    let events = hawkes_events();
    let expected_intensities = [
        0.1,
        0.595024916874584,
        1.0851242535279617,
        1.4847206757819063,
        1.3633660501544487,
        1.845820264794339,
        2.3234739797901476,
    ];
    let expected_decay_sums = [
        1.0,
        1.990049833749168,
        2.9702485070559233,
        3.7694413515638123,
        3.526732100308897,
        4.491640529588677,
        5.446947959580295,
    ];

    let mut decay_sum = 0.0;
    let mut last = events[0];
    let mut intensities = Vec::new();
    let mut decay_sums = Vec::new();

    for &t in &events[1..] {
        let (intensity, new_decay) = compute_hawkes_streaming(0.0, t, last, &params, decay_sum);
        intensities.push(intensity);
        decay_sums.push(new_decay);
        decay_sum = new_decay;
        last = t;
    }

    assert_eq!(intensities.len(), events.len() - 1);
    assert_eq!(intensities.len(), expected_intensities.len());
    for (i, (&got, &exp)) in intensities
        .iter()
        .zip(expected_intensities.iter())
        .enumerate()
    {
        assert!(
            approx_eq(got, exp, TOL),
            "intensity[{i}]: got {got}, expected {exp}"
        );
        assert!(got >= params.mu);
    }
    for (i, (&got, &exp)) in decay_sums
        .iter()
        .zip(expected_decay_sums.iter())
        .enumerate()
    {
        assert!(
            approx_eq(got, exp, TOL),
            "decay_sum[{i}]: got {got}, expected {exp}"
        );
        assert!(got >= 0.0);
    }
}

#[test]
fn surprise_is_nonnegative_and_anomaly_consistent() {
    let params = SurpriseParams::<f64>::default();

    let calm = compute_surprise(100.0, 100.0, &params);
    assert!(calm.surprise.is_finite() && calm.surprise >= 0.0);
    assert!(calm.surprise <= params.threshold);

    let spike = compute_surprise(150.0, 100.0, &params);
    assert!(spike.surprise >= 0.0);
    assert_eq!(spike.surprise, spike.z_score.abs());
    assert!(
        spike.surprise > params.threshold,
        "spike should be anomalous"
    );
}

#[test]
fn surprise_sequence_matches_expected() {
    // Mirror of vectors.surprise_sequence in shared_vectors.json.
    let params = SurpriseParams::<f64>::default();
    let values = surprise_sequence_values();
    let results = compute_surprise_sequence(&values, &params);

    assert_eq!(results.len(), values.len() - 1);

    let expected = [
        (0.0, 0.0, 0.0, false),
        (
            3.1465708968257626,
            3.1465708968257626,
            0.009950330853168092,
            true,
        ),
        (
            125.07275443799473,
            125.07275443799473,
            0.3955147772549963,
            true,
        ),
        (
            4.244731732831435,
            -4.244731732831435,
            -0.013423020332140663,
            true,
        ),
    ];

    for (i, r) in results.iter().enumerate() {
        let (exp_s, exp_z, exp_lr, exp_anom) = expected[i];
        assert!(
            approx_eq(r.surprise, exp_s, TOL),
            "surprise[{i}]: got {}, expected {exp_s}",
            r.surprise
        );
        assert!(
            approx_eq(r.z_score, exp_z, TOL),
            "z_score[{i}]: got {}, expected {exp_z}",
            r.z_score
        );
        assert!(
            approx_eq(r.log_return, exp_lr, TOL),
            "log_return[{i}]: got {}, expected {exp_lr}",
            r.log_return
        );
        assert!(r.surprise >= 0.0);
        assert_eq!(r.surprise, r.z_score.abs());
        assert_eq!(detect_anomaly(r, &params), exp_anom);
    }

    // Short input yields empty sequence.
    assert!(compute_surprise_sequence(&values[..1], &params).is_empty());
}

#[test]
fn entropy_within_bounds() {
    let bins = 8;
    let r = compute_shannon_entropy(&entropy_signal(), bins);
    let max_entropy = (bins as f64).ln();
    assert!(r.shannon >= 0.0 && r.shannon <= max_entropy + TOL);
    assert!((0.0..=1.0 + TOL).contains(&r.relative));
    assert!(r.bin_count <= bins);
}

#[test]
fn volatility_rms_nonnegative() {
    let mut est = VolEstimator::new(5);
    for &x in &vol_returns() {
        est.push(x);
    }
    let rms = est.rms();
    assert!(rms >= 0.0, "rms must be non-negative: {rms}");
    assert!(rms.is_finite());
}

#[test]
fn signal_stats_matches_expected() {
    // Mirror of vectors.signal_stats in shared_vectors.json.
    let stats = compute_signal_stats(&signal_stats_data());
    assert_eq!(stats.count, 5);
    assert!(approx_eq(stats.mean, 3.0, TOL), "mean {}", stats.mean);
    assert!(
        approx_eq(stats.variance, 2.0, TOL),
        "variance {}",
        stats.variance
    );
    assert!(
        approx_eq(stats.skewness, 0.0, TOL),
        "skewness {}",
        stats.skewness
    );
    assert!(
        approx_eq(stats.kurtosis, -1.3, TOL),
        "kurtosis {}",
        stats.kurtosis
    );
    assert!(stats.variance >= 0.0);

    let empty = compute_signal_stats(&[]);
    assert_eq!(empty.count, 0);
    assert_eq!(empty.mean, 0.0);
    assert_eq!(empty.variance, 0.0);
}

#[test]
fn signal_stats_skewed_matches_expected() {
    // Mirror of vectors.signal_stats_skewed in shared_vectors.json.
    let stats = compute_signal_stats(&signal_stats_skewed_data());
    assert_eq!(stats.count, 10);
    assert!(approx_eq(stats.mean, 3.7, TOL), "mean {}", stats.mean);
    assert!(
        approx_eq(stats.variance, 5.61, TOL),
        "variance {}",
        stats.variance
    );
    assert!(
        approx_eq(stats.skewness, 1.6689330728161367, TOL),
        "skewness {}",
        stats.skewness
    );
    assert!(
        approx_eq(stats.kurtosis, 2.238725728502387, TOL),
        "kurtosis {}",
        stats.kurtosis
    );
    assert!(
        stats.skewness > 0.0,
        "right-skewed series should have skew > 0"
    );
}
