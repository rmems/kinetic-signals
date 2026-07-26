// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shared fixture loading and assertion primitives for golden-vector tests.

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

struct InRangeCheck<'a> {
    label: &'a str,
    got: f64,
    lo: f64,
    hi: f64,
    tolerance: f64,
}

fn assert_in_range(check: InRangeCheck<'_>) {
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

pub fn require_f64(v: &Value, key: &str) -> f64 {
    v.get(key)
        .and_then(|x| x.as_f64())
        .unwrap_or_else(|| panic!("fixture missing or non-numeric field `{key}`"))
}
