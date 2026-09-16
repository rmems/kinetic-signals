// SPDX-License-Identifier: MIT OR Apache-2.0

//! Versioned snapshot and restore for stateful streaming estimators.
//!
//! Snapshot payloads capture only the algorithmic state needed to reproduce
//! subsequent outputs. Restore validates schema version, window sizes, sample
//! counts, and finiteness, and never mutates the destination on failure.
//!
//! Numeric comparison tolerance for continuous-vs-restored outputs is
//! [`RESTORE_OUTPUT_TOLERANCE`]: `VolEstimator` uses `f32` arithmetic, while
//! [`crate::EMA`] and [`crate::SMA`] are `f64` and match exactly under that
//! bound.

use std::error::Error;
use std::fmt;

use crate::numeric::{finite_or_zero, stable_mean};

/// Schema version written by [`crate::VolEstimator::snapshot`],
/// [`crate::EMA::snapshot`], and [`crate::SMA::snapshot`].
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Absolute error bound for comparing outputs of a continuously processed
/// estimator against one that was snapshotted and restored between segments.
pub const RESTORE_OUTPUT_TOLERANCE: f64 = 1e-6;

/// Failure when reconstructing an estimator from a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotError {
    /// `schema_version` does not match [`SNAPSHOT_SCHEMA_VERSION`].
    IncompatibleVersion { found: u32, expected: u32 },
    /// Window capacity is zero or otherwise unusable.
    InvalidCapacity,
    /// Occupied sample count is inconsistent with capacity.
    InvalidLength,
    /// A stored scalar or sample is NaN or infinite.
    NonFinite,
    /// Count / sum / initialization flags contradict the rest of the snapshot.
    InconsistentState,
    /// The snapshot asked for a buffer that could not be allocated.
    AllocationFailed,
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleVersion { found, expected } => {
                write!(
                    f,
                    "incompatible snapshot schema version {found} (expected {expected})"
                )
            }
            Self::InvalidCapacity => {
                write!(f, "snapshot capacity must be greater than zero")
            }
            Self::InvalidLength => {
                write!(f, "snapshot sample count is inconsistent with capacity")
            }
            Self::NonFinite => write!(f, "snapshot contains a non-finite value"),
            Self::InconsistentState => {
                write!(f, "snapshot fields are internally inconsistent")
            }
            Self::AllocationFailed => write!(f, "snapshot buffer could not be allocated"),
        }
    }
}

impl Error for SnapshotError {}

pub(crate) fn check_version(found: u32) -> Result<(), SnapshotError> {
    if found != SNAPSHOT_SCHEMA_VERSION {
        Err(SnapshotError::IncompatibleVersion {
            found,
            expected: SNAPSHOT_SCHEMA_VERSION,
        })
    } else {
        Ok(())
    }
}

pub(crate) fn require_finite_f32(x: f32) -> Result<(), SnapshotError> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(SnapshotError::NonFinite)
    }
}

pub(crate) fn require_finite_f64(x: f64) -> Result<(), SnapshotError> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(SnapshotError::NonFinite)
    }
}

pub(crate) fn alloc_zeros_f32(len: usize) -> Result<Vec<f32>, SnapshotError> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|_| SnapshotError::AllocationFailed)?;
    buf.resize(len, 0.0);
    Ok(buf)
}

fn try_reserve_f64(len: usize) -> Result<Vec<f64>, SnapshotError> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len)
        .map_err(|_| SnapshotError::AllocationFailed)?;
    Ok(buf)
}

pub(crate) fn clone_f64_slice(src: &[f64]) -> Result<Vec<f64>, SnapshotError> {
    let mut buf = try_reserve_f64(src.len())?;
    buf.extend_from_slice(src);
    Ok(buf)
}

pub(crate) fn sma_canonical_sum(window: &[f64]) -> f64 {
    match stable_mean(window) {
        Some(mean) => finite_or_zero(mean * window.len() as f64),
        None => 0.0,
    }
}

/// Canonical ring-buffer state for [`crate::VolEstimator`].
///
/// `samples` is the physical ring (`samples.len() == capacity`) in storage
/// order, not rotated oldest-first, so [`crate::VolEstimator::rms`] summation
/// order is preserved across restore.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VolEstimatorSnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Ring capacity (`> 0`).
    pub capacity: usize,
    /// Next write index (`< capacity`).
    pub pos: usize,
    /// Whether the ring has wrapped.
    pub full: bool,
    /// Physical ring slots, length equal to `capacity`.
    pub samples: Vec<f32>,
}

/// Maximum capacity accepted by [`VolEstimatorSnapshot::validate`].
///
/// Snapshots claiming a capacity above this threshold cannot be soundly
/// allocated by [`crate::VolEstimator::from_snapshot`] on typical 64-bit
/// systems and are unconditionally rejected with
/// [`SnapshotError::InvalidCapacity`] before any allocation is attempted.
pub const MAX_SNAPSHOT_CAPACITY: usize = 1_000_000;

impl VolEstimatorSnapshot {
    /// Check version, capacity, layout, and finiteness without allocating an
    /// estimator.
    ///
    /// A `capacity` of zero or above [`MAX_SNAPSHOT_CAPACITY`] is rejected
    /// with [`SnapshotError::InvalidCapacity`].
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.capacity == 0 || self.capacity > MAX_SNAPSHOT_CAPACITY {
            return Err(SnapshotError::InvalidCapacity);
        }
        if self.samples.len() != self.capacity {
            return Err(SnapshotError::InvalidLength);
        }
        if self.pos >= self.capacity {
            return Err(SnapshotError::InconsistentState);
        }
        for &x in &self.samples {
            require_finite_f32(x)?;
        }
        Ok(())
    }
}

/// Canonical state for [`crate::EMA`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EMASnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Current EMA value; ignored on restore when `initialized` is `false`.
    pub value: f64,
    /// Smoothing factor \(\alpha\).
    pub alpha: f64,
    /// Whether at least one sample has been observed.
    pub initialized: bool,
}

impl EMASnapshot {
    /// Check version and finiteness without allocating an estimator.
    ///
    /// `alpha` must be finite and `> 0`. Values greater than 1 are accepted
    /// so [`crate::EMA::new`] with `period == 0` (`α = 2`) can round-trip.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        require_finite_f64(self.alpha)?;
        if self.alpha <= 0.0 {
            return Err(SnapshotError::InconsistentState);
        }
        if self.initialized {
            require_finite_f64(self.value)?;
        }
        Ok(())
    }
}

/// Canonical state for [`crate::SMA`].
///
/// `window` is oldest-first. `sum` must match the Welford-derived total
/// [`crate::SMA::update`] stores (window mean times count, or `0.0` when
/// empty). Capacity `0` is valid when `window` is empty and `sum` is `0.0`,
/// matching [`crate::SMA::new`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SMASnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Maximum number of samples retained (`0` is a no-op estimator).
    pub capacity: usize,
    /// Samples currently in the window, oldest first.
    pub window: Vec<f64>,
    /// Welford-derived sum of `window`.
    pub sum: f64,
}

impl SMASnapshot {
    /// Check version, capacity, length, finiteness, and Welford-derived sum.
    ///
    /// The stored `sum` is compared against the canonical recomputation with
    /// a tolerance scaled by magnitude and window length (base
    /// [`RESTORE_OUTPUT_TOLERANCE`], so benign rounding drift validates
    /// while genuine inconsistencies are rejected.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.window.len() > self.capacity {
            return Err(SnapshotError::InvalidLength);
        }
        for &x in &self.window {
            require_finite_f64(x)?;
        }
        require_finite_f64(self.sum)?;
        // Bit-exact comparison is too strict: the stored Welford-derived sum
        // and `sma_canonical_sum` (stable mean times count) can differ by
        // benign rounding drift. Compare with a tolerance scaled by magnitude
        // and window length instead, still rejecting genuine inconsistencies.
        let canonical = sma_canonical_sum(&self.window);
        let scale = self.sum.abs().max(canonical.abs()).max(1.0);
        let tolerance = RESTORE_OUTPUT_TOLERANCE * scale * self.window.len().max(1) as f64;
        if (self.sum - canonical).abs() > tolerance {
            return Err(SnapshotError::InconsistentState);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_error_display_and_equality() {
        let err = SnapshotError::IncompatibleVersion {
            found: 9,
            expected: SNAPSHOT_SCHEMA_VERSION,
        };
        assert_eq!(
            err.to_string(),
            format!("incompatible snapshot schema version 9 (expected {SNAPSHOT_SCHEMA_VERSION})")
        );
        assert_ne!(err, SnapshotError::NonFinite);
    }

    #[test]
    fn vol_snapshot_rejects_bad_version_capacity_length_and_nan() {
        let mut snap = VolEstimatorSnapshot {
            schema_version: 99,
            capacity: 2,
            pos: 0,
            full: true,
            samples: vec![0.1, 0.2],
        };
        assert!(matches!(
            snap.validate(),
            Err(SnapshotError::IncompatibleVersion {
                found: 99,
                expected: SNAPSHOT_SCHEMA_VERSION
            })
        ));
        snap.schema_version = SNAPSHOT_SCHEMA_VERSION;
        snap.capacity = 0;
        snap.samples = vec![];
        snap.pos = 0;
        snap.full = false;
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidCapacity));
        snap.capacity = 2;
        snap.samples = vec![0.1];
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidLength));
        snap.samples = vec![0.1, 0.2];
        snap.pos = 2;
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
        snap.pos = 0;
        snap.samples = vec![f32::NAN, 0.2];
        assert_eq!(snap.validate(), Err(SnapshotError::NonFinite));
    }

    #[test]
    fn ema_snapshot_rejects_non_finite_alpha_and_initialized_value() {
        let mut snap = EMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            value: 1.0,
            alpha: f64::INFINITY,
            initialized: false,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::NonFinite));
        snap.alpha = 0.5;
        snap.initialized = true;
        snap.value = f64::NAN;
        assert_eq!(snap.validate(), Err(SnapshotError::NonFinite));
        snap.initialized = false;
        assert_eq!(snap.validate(), Ok(()));
        snap.alpha = 0.0;
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
        snap.alpha = -0.5;
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
        snap.alpha = 2.0;
        assert_eq!(snap.validate(), Ok(()));
    }

    #[test]
    fn sma_snapshot_rejects_empty_window_with_nonzero_sum() {
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 3,
            window: vec![],
            sum: 1.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
    }

    #[test]
    fn sma_snapshot_rejects_wildly_inconsistent_sum() {
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 1,
            window: vec![1.0],
            sum: 100.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
    }

    #[test]
    fn sma_snapshot_accepts_zero_capacity_empty_window() {
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 0,
            window: vec![],
            sum: 0.0,
        };
        assert_eq!(snap.validate(), Ok(()));
    }

    #[test]
    fn sma_snapshot_rejects_zero_capacity_with_samples() {
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 0,
            window: vec![1.0],
            sum: 1.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidLength));
    }

    #[test]
    fn clone_f64_slice_round_trips_and_maps_overflow_len() {
        assert_eq!(clone_f64_slice(&[1.0, 2.0]).unwrap(), vec![1.0, 2.0]);
        assert_eq!(
            try_reserve_f64(usize::MAX),
            Err(SnapshotError::AllocationFailed)
        );
    }

    #[test]
    fn vol_snapshot_rejects_oversized_capacity() {
        let snap = VolEstimatorSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: MAX_SNAPSHOT_CAPACITY + 1,
            pos: 0,
            full: false,
            samples: vec![],
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidCapacity));
    }

    #[test]
    fn sma_snapshot_rejects_inconsistent_sum_multi_element() {
        // Window sums to 6.0 but stored sum is off by more than tolerance.
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 3,
            window: vec![1.0, 2.0, 3.0],
            sum: 600.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
    }

    #[test]
    fn sma_snapshot_accepts_sum_within_scaled_tolerance() {
        // Benign rounding drift: the stored sum differs from the canonical
        // recomputation by far less than the scaled tolerance. Bit-exact
        // comparison would wrongly reject this snapshot.
        let window = vec![0.1, 0.2, 0.3];
        let canonical = sma_canonical_sum(&window);
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 3,
            window,
            sum: canonical + 1e-9,
        };
        assert_ne!(snap.sum, canonical);
        assert_eq!(snap.validate(), Ok(()));
    }
}
