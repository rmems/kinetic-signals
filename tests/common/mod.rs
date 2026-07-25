// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared fixture loading and assertion primitives for golden-vector tests.
//!
//! Each integration test binary only uses a subset of these helpers.

#![allow(dead_code)]

use kinetic_signals::{hawkes::HawkesParams, surprise::SurpriseParams};
use serde_json::Value;

const SHARED_VECTORS_JSON: &str = include_str!("../fixtures/shared_vectors.json");

pub fn fixture() -> Value {
    serde_json::from_str(SHARED_VECTORS_JSON).expect("shared_vectors.json must be valid JSON")
}

pub fn root_tolerance() -> f64 {
    require_f64(&fixture(), "tolerance")
}

pub fn tol(v: &Value) -> f64 {
    match v.get("tolerance") {
        None => root_tolerance(),
        Some(t) => t
            .as_f64()
            .unwrap_or_else(|| panic!("fixture vector `tolerance` must be numeric")),
    }
}

pub fn f64s(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("array of numbers")
        .iter()
        .map(|x| x.as_f64().expect("f64"))
        .collect()
}

pub struct CloseCheck<'a> {
    pub label: &'a str,
    pub got: f64,
    pub expected: f64,
    pub tolerance: f64,
}

pub fn assert_close(check: CloseCheck<'_>) {
    assert!(
        (check.got - check.expected).abs() <= check.tolerance,
        "{}: got {} expected {} (tol={})",
        check.label,
        check.got,
        check.expected,
        check.tolerance
    );
}

pub struct SeriesCheck<'a> {
    pub label: &'a str,
    pub got: &'a [f64],
    pub expected: &'a [f64],
    pub tolerance: f64,
}

pub fn assert_series(check: SeriesCheck<'_>) {
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

pub struct BoundCtx {
    pub mu: Option<f64>,
    pub bins: Option<usize>,
}

fn parse_range_endpoint(v: &Value, ctx: &BoundCtx) -> f64 {
    if let Some(n) = v.as_f64() {
        return n;
    }
    match v.as_str() {
        Some("Inf") => f64::INFINITY,
        Some("-Inf") => f64::NEG_INFINITY,
        Some("mu") => ctx
            .mu
            .unwrap_or_else(|| panic!("range endpoint `mu` requires params context")),
        Some("ln(bins)") => {
            let bins = ctx
                .bins
                .unwrap_or_else(|| panic!("range endpoint `ln(bins)` requires bins context"));
            (bins as f64).ln()
        }
        other => panic!("unsupported output_range endpoint: {other:?}"),
    }
}

fn range_pair(range: &Value, ctx: &BoundCtx) -> (f64, f64) {
    let arr = range
        .as_array()
        .unwrap_or_else(|| panic!("output_range entry must be a [lo, hi] array"));
    assert_eq!(arr.len(), 2, "output_range entry must have length 2");
    (
        parse_range_endpoint(&arr[0], ctx),
        parse_range_endpoint(&arr[1], ctx),
    )
}

pub struct InRangeCheck<'a> {
    pub label: &'a str,
    pub got: f64,
    pub lo: f64,
    pub hi: f64,
    pub tolerance: f64,
}

pub fn assert_in_range(check: InRangeCheck<'_>) {
    assert!(
        check.got.is_finite(),
        "{}: got non-finite value {}",
        check.label,
        check.got
    );
    assert!(
        check.got + check.tolerance >= check.lo && check.got - check.tolerance <= check.hi,
        "{}: got {} outside [{}, {}] (tol={})",
        check.label,
        check.got,
        check.lo,
        check.hi,
        check.tolerance
    );
}

pub struct OutputRangeCtx<'a> {
    pub ranges: &'a Value,
    pub bounds: &'a BoundCtx,
    pub tolerance: f64,
}

pub fn assert_field_in_output_range(rc: &OutputRangeCtx<'_>, field: &str, got: f64) {
    let (lo, hi) = range_pair(&rc.ranges[field], rc.bounds);
    assert_in_range(InRangeCheck {
        label: field,
        got,
        lo,
        hi,
        tolerance: rc.tolerance,
    });
}

pub fn assert_series_in_output_range(rc: &OutputRangeCtx<'_>, field: &str, values: &[f64]) {
    for &got in values {
        assert_field_in_output_range(rc, field, got);
    }
}

pub struct SizeCtx {
    pub event_times: Option<usize>,
    pub values: Option<usize>,
    pub data: Option<usize>,
}

pub fn parse_size_contract(expr: &Value, ctx: &SizeCtx) -> usize {
    if let Some(n) = expr.as_u64() {
        return n as usize;
    }
    if let Some(n) = expr.as_i64() {
        return n as usize;
    }
    match expr.as_str() {
        Some("event_times.len()") => ctx
            .event_times
            .unwrap_or_else(|| panic!("size contract event_times.len() needs event_times len")),
        Some("values.len() - 1") => {
            let n = ctx
                .values
                .unwrap_or_else(|| panic!("size contract values.len()-1 needs values len"));
            n.saturating_sub(1)
        }
        Some("data.len()") => ctx
            .data
            .unwrap_or_else(|| panic!("size contract data.len() needs data len")),
        other => panic!("unsupported size contract: {other:?}"),
    }
}

pub fn require_f64(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_f64())
        .unwrap_or_else(|| panic!("fixture missing or non-numeric field `{key}`"))
}

pub fn params_from_json(v: &Value) -> HawkesParams {
    HawkesParams {
        mu: require_f64(v, "mu"),
        alpha: require_f64(v, "alpha"),
        beta: require_f64(v, "beta"),
        dt: require_f64(v, "dt"),
    }
}

pub fn surprise_params_from_json(v: &Value) -> SurpriseParams {
    SurpriseParams {
        mu: require_f64(v, "mu"),
        sigma: require_f64(v, "sigma"),
        dt: require_f64(v, "dt"),
        threshold: require_f64(v, "threshold"),
    }
}
