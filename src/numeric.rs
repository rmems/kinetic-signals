// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::real::Real;

pub(crate) fn all_finite<T: Real>(values: &[T]) -> bool {
    values.iter().copied().all(Real::is_finite)
}

pub(crate) fn finite_or_zero(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}

pub(crate) fn welford_mean(data: &[f64]) -> f64 {
    let mut mean = 0.0;
    for (i, &x) in data.iter().enumerate() {
        mean += (x - mean) / (i + 1) as f64;
    }
    mean
}

pub(crate) fn stable_mean(data: &[f64]) -> Option<f64> {
    if data.is_empty() {
        return None;
    }
    let w = welford_mean(data);
    if w.is_finite() {
        return Some(w);
    }
    let mean = data.iter().copied().sum::<f64>() / data.len() as f64;
    if mean.is_finite() { Some(mean) } else { None }
}
