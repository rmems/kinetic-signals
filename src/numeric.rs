// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::real::Real;

pub(crate) fn all_finite<T: Real>(values: &[T]) -> bool {
    values.iter().copied().all(Real::is_finite)
}

pub(crate) fn finite_or_zero(x: f64) -> f64 {
    if x.is_finite() { x } else { 0.0 }
}
