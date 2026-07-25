// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden-vector parity checks (issue #3 / LIM-31, issue #28 / LIM-201).
//!
//! Canonical shared vectors live in `tests/fixtures/shared_vectors.json`.
//! Tests deserialize that fixture and assert against its values so Rust and
//! any SpikeStream.jl (or future) consumer stay on one source of truth.

use kinetic_signals::{
    VolEstimator, compute_hawkes, compute_hawkes_streaming, compute_hurst, compute_shannon_entropy,
    compute_signal_stats, compute_surprise, compute_surprise_sequence, detect_anomaly,
    hawkes::HawkesParams, surprise::SurpriseParams, surprise::SurpriseResult,
};
use serde_json::Value;

const DEFAULT_TOL: f64 = 1e-6;
const SHARED_VECTORS_JSON: &str = include_str!("fixtures/shared_vectors.json");

fn fixture() -> Value {
    serde_json::from_str(SHARED_VECTORS_JSON).expect("shared_vectors.json must be valid JSON")
}

fn tol(v: &Value) -> f64 {
    v.get("tolerance")
        .and_then(|t| t.as_f64())
        .unwrap_or(DEFAULT_TOL)
}

fn f64s(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("array of numbers")
        .iter()
        .map(|x| x.as_f64().expect("f64"))
        .collect()
}

struct CloseCheck<'a> {
    label: &'a str,
    got: f64,
    expected: f64,
    tolerance: f64,
}

fn assert_close(check: CloseCheck<'_>) {
    assert!(
        (check.got - check.expected).abs() <= check.tolerance,
        "{}: got {} expected {} (tol={})",
        check.label,
        check.got,
        check.expected,
        check.tolerance
    );
}

struct SeriesCheck<'a> {
    label: &'a str,
    got: &'a [f64],
    expected: &'a [f64],
    tolerance: f64,
}

fn assert_series(check: SeriesCheck<'_>) {
    assert_eq!(
        check.got.len(),
        check.expected.len(),
        "{} length mismatch",
        check.label
    );
    for (i, (&g, &e)) in check.got.iter().zip(check.expected.iter()).enumerate() {
        let label = format!("{}[{i}]", check.label);
        assert_close(CloseCheck {
            label: &label,
            got: g,
            expected: e,
            tolerance: check.tolerance,
        });
    }
}

struct HawkesWalk {
    intensities: Vec<f64>,
    decay_sums: Vec<f64>,
}

fn walk_hawkes_streaming(
    events: &[f64],
    params: &HawkesParams,
    initial_decay_sum: f64,
) -> HawkesWalk {
    assert!(!events.is_empty());
    let mut decay_sum = initial_decay_sum;
    let mut last = events[0];
    let mut intensities = Vec::with_capacity(events.len());
    let mut decay_sums = Vec::with_capacity(events.len());
    for (i, &t) in events.iter().enumerate() {
        let last_for_step = if i == 0 { t } else { last };
        let (intensity, new_decay) =
            compute_hawkes_streaming(0.0, t, last_for_step, params, decay_sum);
        intensities.push(intensity);
        decay_sums.push(new_decay);
        decay_sum = new_decay;
        last = t;
    }
    HawkesWalk {
        intensities,
        decay_sums,
    }
}

struct SurpriseExpect {
    surprise: f64,
    z_score: f64,
    log_return: f64,
    expected_return: f64,
    anomaly: bool,
}

struct SurpriseStepCheck<'a> {
    step: usize,
    result: &'a SurpriseResult,
    params: &'a SurpriseParams,
    expected: SurpriseExpect,
    tolerance: f64,
}

fn assert_surprise_step(check: SurpriseStepCheck<'_>) {
    let step = check.step;
    let label_s = format!("surprise[{step}]");
    let label_z = format!("z_score[{step}]");
    let label_lr = format!("log_return[{step}]");
    let label_er = format!("expected_return[{step}]");
    assert_close(CloseCheck {
        label: &label_s,
        got: check.result.surprise,
        expected: check.expected.surprise,
        tolerance: check.tolerance,
    });
    assert_close(CloseCheck {
        label: &label_z,
        got: check.result.z_score,
        expected: check.expected.z_score,
        tolerance: check.tolerance,
    });
    assert_close(CloseCheck {
        label: &label_lr,
        got: check.result.log_return,
        expected: check.expected.log_return,
        tolerance: check.tolerance,
    });
    assert_close(CloseCheck {
        label: &label_er,
        got: check.result.expected_return,
        expected: check.expected.expected_return,
        tolerance: check.tolerance,
    });
    assert_close(CloseCheck {
        label: &format!("mu_dt[{step}]"),
        got: check.result.expected_return,
        expected: check.params.mu * check.params.dt,
        tolerance: check.tolerance,
    });
    assert!(check.result.surprise >= 0.0);
    assert_eq!(check.result.surprise, check.result.z_score.abs());
    assert_eq!(
        detect_anomaly(check.result, check.params),
        check.expected.anomaly
    );
}

fn expect_from_step(step: &Value) -> SurpriseExpect {
    SurpriseExpect {
        surprise: step["surprise"].as_f64().unwrap(),
        z_score: step["z_score"].as_f64().unwrap(),
        log_return: step["log_return"].as_f64().unwrap(),
        expected_return: step["expected_return"].as_f64().unwrap(),
        anomaly: step["anomaly"].as_bool().unwrap(),
    }
}

fn assert_surprise_fixture(vector_key: &str) {
    let root = fixture();
    let v = &root["vectors"][vector_key];
    let tolerance = tol(v);
    let values = f64s(&v["input"]["values"]);
    let params = surprise_params_from_json(&v["input"]["params"]);
    let results = compute_surprise_sequence(&values, &params);
    assert_eq!(results.len(), values.len() - 1);

    let steps = v["expected"]["steps"]
        .as_array()
        .expect("expected.steps array");
    assert_eq!(results.len(), steps.len());
    for (i, (r, step)) in results.iter().zip(steps.iter()).enumerate() {
        assert_surprise_step(SurpriseStepCheck {
            step: i,
            result: r,
            params: &params,
            expected: expect_from_step(step),
            tolerance,
        });
    }
}

fn require_f64(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_f64())
        .unwrap_or_else(|| panic!("fixture missing or non-numeric field `{key}`"))
}

fn params_from_json(v: &Value) -> HawkesParams {
    HawkesParams {
        mu: require_f64(v, "mu"),
        alpha: require_f64(v, "alpha"),
        beta: require_f64(v, "beta"),
        dt: require_f64(v, "dt"),
    }
}

fn surprise_params_from_json(v: &Value) -> SurpriseParams {
    SurpriseParams {
        mu: require_f64(v, "mu"),
        sigma: require_f64(v, "sigma"),
        dt: require_f64(v, "dt"),
        threshold: require_f64(v, "threshold"),
    }
}

#[test]
fn fixture_parses_and_documents_required_vector_keys() {
    let root = fixture();
    let vectors = root["vectors"].as_object().expect("vectors object");
    for key in [
        "hurst",
        "hawkes",
        "hawkes_streaming",
        "hawkes_streaming_sequence",
        "surprise",
        "surprise_sequence",
        "surprise_sequence_drift",
        "entropy",
        "volatility",
        "signal_stats",
        "signal_stats_skewed",
    ] {
        assert!(
            vectors.contains_key(key),
            "shared_vectors.json missing vectors.{key}"
        );
    }
}

#[test]
fn hurst_within_unit_interval() {
    let root = fixture();
    let v = &root["vectors"]["hurst"];
    let data = f64s(&v["input"]["data"]);
    let tolerance = tol(v);
    let r = compute_hurst(&data);
    assert!(r.h.is_finite());
    assert!((0.0..=1.0).contains(&r.h), "hurst H out of [0,1]: {}", r.h);
    let r2 = compute_hurst(&data);
    assert_close(CloseCheck {
        label: "hurst_deterministic",
        got: r.h,
        expected: r2.h,
        tolerance,
    });
}

#[test]
fn hawkes_intensity_at_least_baseline() {
    let root = fixture();
    let v = &root["vectors"]["hawkes"];
    let events = f64s(&v["input"]["event_times"]);
    let params = params_from_json(&v["input"]["params"]);
    let r = compute_hawkes(&events, &params);
    assert!(r.intensity.is_finite());
    assert!(r.intensity >= params.mu);
    assert!(r.avg_excitation >= 0.0);
    assert_eq!(r.event_count, events.len());
}

#[test]
fn hawkes_streaming_single_step_matches_fixture() {
    let root = fixture();
    let v = &root["vectors"]["hawkes_streaming"];
    let tolerance = tol(v);
    let input = &v["input"];
    let params = params_from_json(&input["params"]);
    let (intensity, new_decay) = compute_hawkes_streaming(
        input["prev_intensity"].as_f64().unwrap(),
        input["new_event_time"].as_f64().unwrap(),
        input["last_event_time"].as_f64().unwrap(),
        &params,
        input["decay_sum"].as_f64().unwrap(),
    );
    let exp = &v["expected"];
    assert_close(CloseCheck {
        label: "intensity",
        got: intensity,
        expected: exp["intensity"].as_f64().unwrap(),
        tolerance,
    });
    assert_close(CloseCheck {
        label: "new_decay_sum",
        got: new_decay,
        expected: exp["new_decay_sum"].as_f64().unwrap(),
        tolerance,
    });
    if let Some(post) = exp.get("post_event_intensity").and_then(|x| x.as_f64()) {
        let post_got = params.mu + params.alpha * new_decay;
        assert_close(CloseCheck {
            label: "post_event_intensity",
            got: post_got,
            expected: post,
            tolerance,
        });
    }
}

#[test]
fn hawkes_streaming_sequence_matches_fixture() {
    let root = fixture();
    let v = &root["vectors"]["hawkes_streaming_sequence"];
    let tolerance = tol(v);
    let events = f64s(&v["input"]["event_times"]);
    let params = params_from_json(v["input"].get("params").unwrap_or(&Value::Null));
    let initial_decay = v["input"]["initial_decay_sum"].as_f64().unwrap_or(0.0);
    let walk = walk_hawkes_streaming(&events, &params, initial_decay);
    let exp = &v["expected"];
    assert_series(SeriesCheck {
        label: "intensity",
        got: &walk.intensities,
        expected: &f64s(&exp["intensities"]),
        tolerance,
    });
    assert_series(SeriesCheck {
        label: "decay_sum",
        got: &walk.decay_sums,
        expected: &f64s(&exp["decay_sums"]),
        tolerance,
    });
    let post = params.mu + params.alpha * walk.decay_sums.last().copied().unwrap_or(0.0);
    // batch compute_hawkes always starts from zero decay; only equate when fixture does too
    if initial_decay == 0.0 {
        let batch = compute_hawkes(&events, &params);
        assert_close(CloseCheck {
            label: "batch_vs_stream_post",
            got: post,
            expected: batch.intensity,
            tolerance,
        });
    }
    if let Some(e) = exp
        .get("post_event_final_intensity")
        .and_then(|x| x.as_f64())
    {
        assert_close(CloseCheck {
            label: "post_event_final",
            got: post,
            expected: e,
            tolerance,
        });
    }
}

#[test]
fn surprise_is_nonnegative_and_anomaly_consistent() {
    let root = fixture();
    let v = &root["vectors"]["surprise"];
    let input = &v["input"];
    let params = surprise_params_from_json(&input["params"]);
    let prev = input["previous_value"].as_f64().unwrap();
    let curr = input["current_value"].as_f64().unwrap();
    let calm = compute_surprise(prev, prev, &params);
    assert!(calm.surprise.is_finite() && calm.surprise >= 0.0);
    assert!(calm.surprise <= params.threshold);
    let spike = compute_surprise(curr, prev, &params);
    assert!(spike.surprise >= 0.0);
    assert_eq!(spike.surprise, spike.z_score.abs());
    assert!(spike.surprise > params.threshold);
}

#[test]
fn surprise_sequence_matches_fixture() {
    assert_surprise_fixture("surprise_sequence");
}

#[test]
fn surprise_sequence_drift_matches_fixture() {
    assert_surprise_fixture("surprise_sequence_drift");
}

#[test]
fn surprise_sequence_short_input_empty() {
    let params = SurpriseParams::<f64>::default();
    assert!(compute_surprise_sequence(&[100.0], &params).is_empty());
}

#[test]
fn entropy_within_bounds() {
    let root = fixture();
    let v = &root["vectors"]["entropy"];
    let signal = f64s(&v["input"]["data"]);
    let bins = v["input"]["bins"].as_u64().unwrap() as usize;
    let tolerance = tol(v);
    let r = compute_shannon_entropy(&signal, bins);
    let max_entropy = (bins as f64).ln();
    assert!(r.shannon >= 0.0 && r.shannon <= max_entropy + tolerance);
    assert!((0.0..=1.0 + tolerance).contains(&r.relative));
    assert!(r.bin_count <= bins);
}

#[test]
fn volatility_rms_nonnegative() {
    let root = fixture();
    let v = &root["vectors"]["volatility"];
    let returns = f64s(&v["input"]["abs_log_returns"]);
    let window = v["input"]["window"].as_u64().unwrap() as usize;
    let mut est = VolEstimator::new(window);
    for x in returns {
        est.push(x as f32);
    }
    let rms = est.rms();
    assert!(rms >= 0.0 && rms.is_finite());
}

fn assert_signal_stats_fixture(vector_key: &str) {
    let root = fixture();
    let v = &root["vectors"][vector_key];
    let tolerance = tol(v);
    let data = f64s(&v["input"]["data"]);
    let stats = compute_signal_stats(&data);
    let exp = &v["expected"];
    assert_eq!(stats.count, exp["count"].as_u64().unwrap() as usize);
    for (label, got) in [
        ("mean", stats.mean),
        ("variance", stats.variance),
        ("skewness", stats.skewness),
        ("kurtosis", stats.kurtosis),
    ] {
        assert_close(CloseCheck {
            label,
            got,
            expected: exp[label].as_f64().unwrap(),
            tolerance,
        });
    }
}

#[test]
fn signal_stats_matches_fixture() {
    assert_signal_stats_fixture("signal_stats");
}

#[test]
fn signal_stats_empty_is_zero() {
    let empty = compute_signal_stats(&[]);
    assert_eq!(empty.count, 0);
    assert_eq!(empty.mean, 0.0);
    assert_eq!(empty.variance, 0.0);
}

#[test]
fn signal_stats_skewed_matches_fixture() {
    assert_signal_stats_fixture("signal_stats_skewed");
}
