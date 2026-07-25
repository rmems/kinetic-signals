// SPDX-License-Identifier: MIT OR Apache-2.0

//! Golden-vector parity checks for hurst / entropy / volatility plus fixture inventory
//! (issue #3 / LIM-31, issue #28 / LIM-201).
//!
//! Canonical shared vectors live in `tests/fixtures/shared_vectors.json`.
//! Domain-specific streaming goldens live in sibling integration tests.

mod common;

use common::{
    BoundCtx, CloseCheck, OutputRangeCtx, assert_close, assert_field_in_output_range, f64s,
    fixture, root_tolerance, tol,
};
use kinetic_signals::{VolEstimator, compute_hurst, compute_shannon_entropy};

#[test]
fn fixture_parses_and_documents_required_vector_keys() {
    let root = fixture();
    let _root_tol = root_tolerance();
    let vectors = root["vectors"].as_object().expect("vectors object");
    for key in [
        "hurst",
        "hawkes",
        "hawkes_streaming",
        "hawkes_streaming_nondefault",
        "hawkes_streaming_sequence",
        "hawkes_streaming_sequence_resume",
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
    assert_field_in_output_range(
        &OutputRangeCtx {
            ranges: &v["output_range"],
            bounds: &BoundCtx {
                mu: None,
                bins: None,
            },
            tolerance,
        },
        "h",
        r.h,
    );
    let r2 = compute_hurst(&data);
    assert_close(CloseCheck {
        label: "hurst_deterministic",
        got: r.h,
        expected: r2.h,
        tolerance,
    });
}

#[test]
fn entropy_within_bounds() {
    let root = fixture();
    let v = &root["vectors"]["entropy"];
    let signal = f64s(&v["input"]["data"]);
    let bins = v["input"]["bins"].as_u64().unwrap() as usize;
    let tolerance = tol(v);
    let r = compute_shannon_entropy(&signal, bins);
    let ctx = BoundCtx {
        mu: None,
        bins: Some(bins),
    };
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &ctx,
        tolerance,
    };
    assert_field_in_output_range(&rc, "shannon", r.shannon);
    assert_field_in_output_range(&rc, "relative", r.relative);
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
    assert!(rms.is_finite());
    assert_field_in_output_range(
        &OutputRangeCtx {
            ranges: &v["output_range"],
            bounds: &BoundCtx {
                mu: None,
                bins: None,
            },
            tolerance: tol(v),
        },
        "rms",
        rms as f64,
    );
}
