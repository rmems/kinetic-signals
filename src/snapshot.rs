// SPDX-License-Identifier: MIT OR Apache-2.0

//! Versioned snapshot and restore for stateful streaming estimators.
//!
//! Snapshot payloads capture only the algorithmic state needed to reproduce
//! subsequent outputs. Restore validates schema version, window sizes, sample
//! counts, and finiteness, and never mutates the destination on failure.
//!
//! # Schema-v1 persistence contract
//!
//! All `0.5.x` patch releases preserve valid v1 checkpoints, their logical
//! fields, and the continuation guarantee below. Later minor releases that
//! retain v1 must do the same. A breaking snapshot change requires incrementing
//! [`SNAPSHOT_SCHEMA_VERSION`] and a breaking crate release: adding a field
//! (even defaulted), removing/renaming one, changing its type or serialized
//! representation, changing state ordering/meaning, or changing validation or
//! restore so valid v1 state is rejected or continues outside the guarantee.
//! Rejecting state already invalid under v1 does not require a bump. Future
//! minor releases need not support v1 after a schema bump, but must document
//! supported versions and migration. No v2 or automatic migration is defined.
//!
//! Optional `serde` derives are format-agnostic, not a promise of canonical
//! JSON or any permanent codec byte representation. Applications own codec
//! selection/options and should retain producer crate version, estimator type,
//! and codec/version metadata separately. Current JSON decoding ignores extra
//! fields; this is not a supported extension mechanism or permanent guarantee.
//! Schema-v1 writers must emit only the defined fields.
//!
//! # Decoding and validation
//!
//! Deserialization does not call `validate`. Malformed input, missing required
//! fields (including `schema_version`), wrong types, or out-of-range integers
//! fail with the codec's error, not [`SnapshotError`]. Codec and target-width
//! constraints apply separately from snapshot validity. Successfully decoded
//! snapshots must still pass validation: any version other than `1` returns
//! [`SnapshotError::IncompatibleVersion`], unsupported capacity returns
//! [`SnapshotError::InvalidCapacity`], invalid sample counts return
//! [`SnapshotError::InvalidLength`], non-finite participating state returns
//! [`SnapshotError::NonFinite`], and contradictory fields return
//! [`SnapshotError::InconsistentState`]. See each snapshot's `validate` rules.
//! Do not depend on error precedence for multiply-invalid state.
//! `from_snapshot`/`restore` can also return [`SnapshotError::AllocationFailed`]
//! while reserving a validated estimator buffer; `validate` does not allocate
//! an estimator. A failed restore always leaves its destination unchanged.
//!
//! **Untrusted input:** decoding `samples`/`window` can allocate before
//! validation runs. [`MAX_SNAPSHOT_CAPACITY`] limits estimator construction and
//! restore, not deserialization. Bound transport/file reads and input bytes
//! before decoding, and use codec container/depth limits or bounded decoding
//! where needed; byte limits alone do not bound decoded memory for every codec.
//!
//! With `serde` enabled, decoding and restore can fail independently:
//!
//! ```rust
//! # #[cfg(feature = "serde")]
//! # {
//! use kinetic_signals::{SMA, SMASnapshot, SnapshotError};
//!
//! // Missing required fields: no snapshot exists to validate.
//! assert!(serde_json::from_str::<SMASnapshot>(r#"{"schema_version":1}"#).is_err());
//!
//! // This small example is trusted. For external input, impose limits BEFORE
//! // this call: decoding window can allocate even if capacity will be rejected.
//! let decoded: SMASnapshot = serde_json::from_str(
//!     r#"{"schema_version":1,"capacity":1000001,"window":[],"sum":0.0}"#,
//! )?;
//! assert!(matches!(SMA::from_snapshot(&decoded), Err(SnapshotError::InvalidCapacity)));
//! # }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Ordering and numerical continuation
//!
//! [`VolEstimatorSnapshot`] preserves the physical `f32` ring, next write
//! index, and wrap flag, not chronological order; RMS accumulation uses `f64`
//! and returns `f32`. [`EMASnapshot`] preserves `alpha`, initialization, and
//! the initialized `f64` value; an uninitialized value is ignored and reset to
//! zero. [`SMASnapshot`] stores an oldest-first `f64` window and declared
//! capacity; restore reserves at least that capacity and canonicalizes `sum`.
//! A valid sum with tolerated rounding drift need not restore bit-for-bit.
//! Construction and restore share capacity ranges: volatility
//! `1..=MAX_SNAPSHOT_CAPACITY`, SMA `0..=MAX_SNAPSHOT_CAPACITY` (zero is a no-op).
//!
//! For valid API-produced state, identical continuation inputs, and a codec
//! round-trip preserving numerical values, outputs after restore match
//! uninterrupted processing within the absolute [`RESTORE_OUTPUT_TOLERANCE`]
//! (`1e-6`). Exact continuation is possible on the same build/target; portable
//! persistence does not promise bit-identical results across crate/compiler/
//! target or codec changes. This is not an accuracy bound against ideal real
//! arithmetic, and excludes lossy codecs, direct public-field mutation, and
//! EMA arithmetic overflow producing non-finite state.

use std::error::Error;
use std::fmt;

use crate::numeric::{finite_or_zero, stable_mean};

/// Schema version written by [`crate::VolEstimator::snapshot`],
/// [`crate::EMA::snapshot`], and [`crate::SMA::snapshot`].
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Largest supported window capacity for [`crate::VolEstimator`] and
/// [`crate::SMA`] construction and snapshot restore.
///
/// `1_000_000` samples is about 4 MiB as `f32` (`VolEstimator`) or 8 MiB as
/// `f64` (`SMA`). Bounding capacity keeps an externally supplied snapshot from
/// requesting an unbounded allocation during restore (`VolEstimator` ring and
/// declared `SMA` window capacity), and caps the reservation a constructor
/// makes for the same window.
/// It does not bound deserialization: a codec may allocate snapshot containers
/// before restore validates them. Apply input and codec limits separately.
pub const MAX_SNAPSHOT_CAPACITY: usize = 1_000_000;

/// Absolute error bound for comparing outputs of a continuously processed
/// estimator against one that was snapshotted and restored between segments.
pub const RESTORE_OUTPUT_TOLERANCE: f64 = 1e-6;

/// Upper bound on the window-length multiplier used when comparing a stored
/// SMA sum to the canonical recomputation. Unbounded `len` scaling lets a
/// large (but valid) window accept a completely mismatched sum, and can
/// overflow the tolerance to infinity.
const SMA_SUM_TOLERANCE_LEN_CAP: f64 = 64.0;

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

pub(crate) fn clone_f64_slice_at_capacity(
    src: &[f64],
    capacity: usize,
) -> Result<Vec<f64>, SnapshotError> {
    let mut buf = try_reserve_f64(capacity)?;
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

impl VolEstimatorSnapshot {
    /// Check version, capacity, layout, and finiteness without allocating an
    /// estimator.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.capacity == 0 {
            return Err(SnapshotError::InvalidCapacity);
        }
        if self.capacity > MAX_SNAPSHOT_CAPACITY {
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
    /// `alpha` must be finite and in `(0, 2]`, matching values produced by
    /// [`crate::EMA::new`], including `period == 0` (`α = 2`).
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        require_finite_f64(self.alpha)?;
        if !(0.0 < self.alpha && self.alpha <= 2.0) {
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
/// matching [`crate::SMA::new`]. SMA construction and restore share one
/// capacity ceiling, `0..=`[`MAX_SNAPSHOT_CAPACITY`]: restore rejects
/// `capacity` above it before reserving the declared buffer, just as the
/// constructor panics above it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SMASnapshot {
    /// Must equal [`SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Maximum number of samples retained (`0` is a no-op estimator;
    /// restore also requires `capacity <= MAX_SNAPSHOT_CAPACITY`).
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
    /// a tolerance scaled by the canonical magnitude and a *capped* window
    /// length (base [`RESTORE_OUTPUT_TOLERANCE`]). An empty window requires
    /// an exact zero `sum` — there is no rounding drift to tolerate.
    pub fn validate(&self) -> Result<(), SnapshotError> {
        check_version(self.schema_version)?;
        if self.capacity > MAX_SNAPSHOT_CAPACITY {
            return Err(SnapshotError::InvalidCapacity);
        }
        if self.window.len() > self.capacity {
            return Err(SnapshotError::InvalidLength);
        }
        for &x in &self.window {
            require_finite_f64(x)?;
        }
        require_finite_f64(self.sum)?;
        // An empty window has no recomputation drift: require an exact zero.
        if self.window.is_empty() {
            if self.sum != 0.0 {
                return Err(SnapshotError::InconsistentState);
            }
            return Ok(());
        }
        // Bit-exact comparison is too strict: the stored Welford-derived sum
        // and `sma_canonical_sum` (stable mean times count) can differ by
        // benign rounding drift. Scale by the *canonical* magnitude only
        // (a huge stored `sum` must not inflate the allowance) and cap the
        // length factor so large windows cannot accept mismatched totals.
        let canonical = sma_canonical_sum(&self.window);
        let scale = canonical.abs().max(1.0);
        let len_scale = (self.window.len() as f64).min(SMA_SUM_TOLERANCE_LEN_CAP);
        let tolerance = RESTORE_OUTPUT_TOLERANCE * scale * len_scale;
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
    fn sma_snapshot_rejects_empty_window_with_sub_tolerance_nonzero_sum() {
        // Without an exact-zero empty-window check, `len().max(1)` grants a
        // 1e-6 absolute allowance and this sum would wrongly validate.
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 3,
            window: vec![],
            sum: 1e-9,
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
    fn sma_snapshot_rejects_capacity_above_supported_limit() {
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: usize::MAX,
            window: vec![],
            sum: 0.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InvalidCapacity));
    }

    #[test]
    fn clone_f64_slice_at_capacity_round_trips_and_maps_overflow_len() {
        assert_eq!(
            clone_f64_slice_at_capacity(&[1.0, 2.0], 4).unwrap(),
            vec![1.0, 2.0]
        );
        assert_eq!(
            try_reserve_f64(usize::MAX),
            Err(SnapshotError::AllocationFailed)
        );
    }

    #[test]
    fn vol_snapshot_rejects_capacity_above_supported_limit() {
        let snap = VolEstimatorSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: usize::MAX,
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

    #[test]
    fn sma_snapshot_rejects_mismatched_sum_on_capped_large_window() {
        // Uncapped `tolerance *= window.len()` accepts sum=0 for a full
        // 1_000_000-wide window of ones (tolerance equals the canonical
        // total). The length cap must reject that mismatch.
        let window = vec![1.0; MAX_SNAPSHOT_CAPACITY];
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: MAX_SNAPSHOT_CAPACITY,
            window,
            sum: 0.0,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
    }

    #[test]
    fn sma_snapshot_rejects_max_sum_on_large_zero_window() {
        // Deriving scale from the stored sum lets `f64::MAX` overflow the
        // tolerance to infinity. Scale from the canonical total only.
        let window = vec![0.0; MAX_SNAPSHOT_CAPACITY];
        let snap = SMASnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: MAX_SNAPSHOT_CAPACITY,
            window,
            sum: f64::MAX,
        };
        assert_eq!(snap.validate(), Err(SnapshotError::InconsistentState));
    }
}
