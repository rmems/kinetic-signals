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

/// Canonical ring-buffer state for [`crate::VolEstimator`].
///
/// `samples` are oldest-first occupied slots (`samples.len() <= capacity`).
/// Restore rebuilds a compact ring with the write head after the newest sample.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VolEstimatorSnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Ring capacity (`> 0`).
    pub capacity: usize,
    /// Occupied samples, oldest first.
    pub samples: Vec<f32>,
}

impl VolEstimatorSnapshot {
    /// Check version, capacity, length, and finiteness without allocating an
    /// estimator.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.capacity == 0 {
            return Err(SnapshotError::InvalidCapacity);
        }
        if self.samples.len() > self.capacity {
            return Err(SnapshotError::InvalidLength);
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
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        require_finite_f64(self.alpha)?;
        if self.initialized {
            require_finite_f64(self.value)?;
        }
        Ok(())
    }
}

/// Canonical state for [`crate::SMA`].
///
/// `window` is oldest-first. `sum` is the running sum used by
/// [`crate::SMA::update`] and is restored as-is so subsequent outputs match
/// the original estimator (it may drift from `window.iter().sum()`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SMASnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Maximum number of samples retained (`> 0`).
    pub capacity: usize,
    /// Samples currently in the window, oldest first.
    pub window: Vec<f64>,
    /// Running sum of `window`.
    pub sum: f64,
}

impl SMASnapshot {
    /// Check version, capacity, length, finiteness, and empty-window sum.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.capacity == 0 {
            return Err(SnapshotError::InvalidCapacity);
        }
        if self.window.len() > self.capacity {
            return Err(SnapshotError::InvalidLength);
        }
        for &x in &self.window {
            require_finite_f64(x)?;
        }
        require_finite_f64(self.sum)?;
        if self.window.is_empty() && self.sum != 0.0 {
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
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidCapacity));
        snap.capacity = 1;
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidLength));
        snap.capacity = 2;
        snap.samples = vec![f32::NAN];
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
}
