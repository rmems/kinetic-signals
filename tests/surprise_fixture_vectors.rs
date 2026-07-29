// SPDX-License-Identifier: MIT OR Apache-2.0

//! Surprise golden-vector parity checks (issue #28 / LIM-201).

mod common;
#[path = "common/size.rs"]
mod size;

use common::{
    BoundCtx, CloseCheck, OutputRangeCtx, assert_close, assert_field_in_output_range, f64s,
    fixture, require_f64, tol,
};
use kinetic_signals::{
    compute_surprise, compute_surprise_sequence, detect_anomaly, surprise::SurpriseParams,
    surprise::SurpriseResult,
};
use serde_json::Value;
use size::{SizeCtx, parse_size_contract};

fn surprise_params_from_json(v: &Value) -> SurpriseParams {
    SurpriseParams {
        mu: require_f64(v, "mu"),
        sigma: require_f64(v, "sigma"),
        dt: require_f64(v, "dt"),
        threshold: require_f64(v, "threshold"),
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
    let expected_len = parse_size_contract(
        &v["output_range"]["length"],
        &SizeCtx {
            event_times: None,
            values: Some(values.len()),
            data: None,
        },
    );
    assert_eq!(results.len(), expected_len);

    let steps = v["expected"]["steps"]
        .as_array()
        .expect("expected.steps array");
    assert_eq!(results.len(), steps.len());
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &BoundCtx {
            mu: None,
            bins: None,
        },
        tolerance,
    };
    for (i, (r, step)) in results.iter().zip(steps.iter()).enumerate() {
        assert_surprise_step(SurpriseStepCheck {
            step: i,
            result: r,
            params: &params,
            expected: expect_from_step(step),
            tolerance,
        });
        assert_field_in_output_range(&rc, "surprise", r.surprise);
        assert_field_in_output_range(&rc, "z_score", r.z_score);
        assert_field_in_output_range(&rc, "expected_return", r.expected_return);
    }
}

#[test]
fn surprise_is_nonnegative_and_anomaly_consistent() {
    let root = fixture();
    let v = &root["vectors"]["surprise"];
    let tolerance = tol(v);
    let input = &v["input"];
    let params = surprise_params_from_json(&input["params"]);
    let prev = require_f64(input, "previous_value");
    let curr = require_f64(input, "current_value");
    let calm = compute_surprise(prev, prev, &params);
    assert!(calm.surprise.is_finite() && calm.surprise >= 0.0);
    assert!(calm.surprise <= params.threshold);
    let spike = compute_surprise(curr, prev, &params);
    assert!(spike.surprise.is_finite() && spike.surprise >= 0.0);
    assert_eq!(spike.surprise, spike.z_score.abs());
    assert!(spike.surprise > params.threshold);
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &BoundCtx {
            mu: None,
            bins: None,
        },
        tolerance,
    };
    assert_field_in_output_range(&rc, "surprise", spike.surprise);
    assert_field_in_output_range(&rc, "z_score", spike.z_score);
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
fn surprise_sequence_nondefault_matches_fixture() {
    assert_surprise_fixture("surprise_sequence_nondefault");
}

#[test]
fn surprise_sequence_short_input_empty() {
    let params = SurpriseParams::<f64>::default();
    assert!(compute_surprise_sequence(&[100.0], &params).is_empty());
}
