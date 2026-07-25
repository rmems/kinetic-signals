// SPDX-License-Identifier: MIT OR Apache-2.0

//! Signal-stats golden-vector parity checks (issue #28 / LIM-201).

mod common;

use common::{
    BoundCtx, CloseCheck, OutputRangeCtx, SizeCtx, assert_close, assert_field_in_output_range,
    f64s, fixture, parse_size_contract, tol,
};
use kinetic_signals::compute_signal_stats;

fn assert_signal_stats_fixture(vector_key: &str) {
    let root = fixture();
    let v = &root["vectors"][vector_key];
    let tolerance = tol(v);
    let data = f64s(&v["input"]["data"]);
    let stats = compute_signal_stats(&data);
    let exp = &v["expected"];
    let count_contract = parse_size_contract(
        &v["output_range"]["count"],
        &SizeCtx {
            event_times: None,
            values: None,
            data: Some(data.len()),
        },
    );
    assert_eq!(stats.count, count_contract);
    assert_eq!(stats.count, exp["count"].as_u64().unwrap() as usize);
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &BoundCtx {
            mu: None,
            bins: None,
        },
        tolerance,
    };
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
        assert_field_in_output_range(&rc, label, got);
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
