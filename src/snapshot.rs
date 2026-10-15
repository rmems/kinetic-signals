// SPDX-License-Identifier: MIT OR Apache-2.0

//! Versioned snapshot and restore for stateful streaming estimators.
//!
//! Snapshot payloads capture only the algorithmic state needed to reproduce
//! subsequent outputs. Restore validates schema version, window sizes, sample
//! counts, and finiteness, and never mutates the destination on failure.
//!
//! # Schema-v1 persistence contract
//!
//! Schema version [`SNAPSHOT_SCHEMA_VERSION`] (`1`) is the checkpoint contract
//! introduced in crate `0.5.0` for [`VolEstimatorSnapshot`], [`EMASnapshot`],
//! and [`SMASnapshot`].
//!
//! **Compatibility window.** Every `0.5.x` patch release must accept a
//! checkpoint that was valid under this contract in an earlier `0.5.x`
//! release: same logical fields, successful restore, and continuation within
//! [`RESTORE_OUTPUT_TOLERANCE`] of the outputs that the producing release would
//! emit for the same finite inputs. A later minor release that leaves
//! `SNAPSHOT_SCHEMA_VERSION` at `1` has the same obligation. A crate minor
//! bump by itself is not permission to reinterpret v1. Historical evidence
//! lives in `tests/fixtures/snapshot_v1_frozen.json`; do not regenerate those
//! payloads or their recorded outputs to absorb a representation or numeric
//! change. While the constant remains `1`, a failure of that fixture means
//! this contract broke.
//!
//! There is no promise that every future minor will read v1. A release that
//! increments the schema must document which versions it still accepts and
//! how to migrate. This release does not define schema v2 or an automatic
//! migration. Current `validate` / `from_snapshot` / `restore` accept only
//! version `1`.
//!
//! `schema_version` does not record the producer crate version, which
//! estimator a payload belongs to, or the codec. Applications that keep
//! checkpoints should store those beside the payload.
//!
//! **When to increment.** Increment [`SNAPSHOT_SCHEMA_VERSION`] when a change
//! would make a previously valid v1 checkpoint fail to decode to the same
//! logical fields, fail validation, or continue outside the tolerance above.
//! That includes adding a field (even one with a default), removing or
//! renaming a field, changing a field's type or serialized representation,
//! changing state ordering or meaning, and tightening validation so state
//! this contract already accepts is rejected. Adding a new snapshot type for
//! some other estimator does not by itself change these three payloads.
//! Rejecting state that this contract already calls invalid does not require
//! a bump. A schema bump is also a breaking crate release under the pre-1.0
//! policy (a minor bump before `1.0`).
//!
//! # Decoding and validation
//!
//! Optional `serde` derives do not call [`VolEstimatorSnapshot::validate`],
//! [`EMASnapshot::validate`], or [`SMASnapshot::validate`]. They are
//! format-agnostic: the application picks the codec and its options. Nothing
//! here promises a permanent canonical byte string for JSON or any other
//! codec. Whitespace, map order, float formatting, and codec versions are not
//! pinned. Persist numeric values losslessly when continuation matters.
//! `serde_json` cannot round-trip `NaN` or infinity (it emits `null`, which
//! does not decode back into `f32` / `f64`).
//!
//! A malformed payload, a missing required field (including `schema_version`),
//! a wrong JSON type, or an integer outside the destination type (for example
//! capacity `-1` as `usize`) fails in the codec. That error is not
//! [`SnapshotError`]. With the derived impls, self-describing formats such as
//! JSON currently ignore unknown extra fields. That is an accident of today's
//! derives, not a supported way to extend v1 and not a promise that a later
//! decoder will keep ignoring them. Writers should emit only the v1 fields
//! and keep application metadata in a side record.
//!
//! After a payload has decoded, validation still applies:
//!
//! | Decoded state | [`SnapshotError`] |
//! | --- | --- |
//! | `schema_version` other than `1` (older or newer) | [`IncompatibleVersion`](SnapshotError::IncompatibleVersion); no fallback |
//! | Declared capacity outside the estimator's range | [`InvalidCapacity`](SnapshotError::InvalidCapacity) |
//! | Volatility ring length differs from `capacity`, or an SMA window is longer than `capacity` | [`InvalidLength`](SnapshotError::InvalidLength) |
//! | A sample or scalar that participates in restored state is non-finite | [`NonFinite`](SnapshotError::NonFinite) |
//! | Ring index out of range, EMA `alpha` outside `(0, 2]`, or an SMA `sum` that disagrees with the window | [`InconsistentState`](SnapshotError::InconsistentState) |
//!
//! A value that decodes but is non-finite in the target width is the second
//! kind of failure. JSON `1e39` becomes `f32::INFINITY` and then
//! [`SnapshotError::NonFinite`], rather than a codec error. An uninitialized
//! EMA may carry a non-finite `value`; validation ignores that `value` and
//! restore sets it to `0.0`. Its `alpha` must still be finite and in `(0, 2]`.
//! An empty SMA window requires `sum == 0.0` exactly. A non-empty SMA `sum`
//! must match the Welford-derived total within the magnitude-scaled,
//! length-capped tolerance documented on [`SMASnapshot::validate`]. That
//! allowance is not [`RESTORE_OUTPUT_TOLERANCE`] itself.
//!
//! `validate` does not allocate an estimator. `from_snapshot` and `restore`
//! can still return [`SnapshotError::AllocationFailed`] if the validated
//! buffer cannot be reserved. When several checks would fail, callers must
//! not depend on which error is reported first. A failed `restore` leaves the
//! destination estimator unchanged.
//!
//! **Untrusted checkpoints.** [`MAX_SNAPSHOT_CAPACITY`] limits estimator
//! construction and the reservation inside `from_snapshot` / `restore`. It
//! does not limit decoding. A codec allocates `samples` or `window` from the
//! encoded container before validation sees the length or the declared
//! capacity, so a hostile payload can force that allocation even when
//! restore would reject it. Bound the transport read and the encoded byte
//! length before decoding, and apply a codec container or depth limit when
//! the format needs one. A byte cap is not a decoded-memory cap for every
//! codec.
//!
//! With the `serde` feature and `serde_json`, decoding and restore fail
//! separately. The checkpoint below is small on purpose. External input needs
//! a byte limit before `from_str`, because a large `window` allocates during
//! decoding even if `capacity` will be rejected:
//!
//! ```
//! # #[cfg(feature = "serde")]
//! # {
//! use kinetic_signals::{SMA, SMASnapshot, SnapshotError};
//!
//! // Missing fields: the codec fails and there is no snapshot to validate.
//! assert!(serde_json::from_str::<SMASnapshot>(r#"{"schema_version":1}"#).is_err());
//!
//! // Unknown schema version: decoding succeeds, restore does not.
//! let future: SMASnapshot = serde_json::from_str(
//!     r#"{"schema_version":2,"capacity":1,"window":[1.0],"sum":1.0}"#,
//! )?;
//! assert!(matches!(
//!     SMA::from_snapshot(&future),
//!     Err(SnapshotError::IncompatibleVersion { found: 2, .. })
//! ));
//!
//! // Over-ceiling capacity: decoding succeeds, restore rejects it before
//! // reserving the estimator window.
//! let oversized: SMASnapshot = serde_json::from_str(
//!     r#"{"schema_version":1,"capacity":1000001,"window":[],"sum":0.0}"#,
//! )?;
//! assert!(matches!(
//!     SMA::from_snapshot(&oversized),
//!     Err(SnapshotError::InvalidCapacity)
//! ));
//! # }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Ordering and numerical continuation
//!
//! | Snapshot | Restored state | Supported capacity |
//! | --- | --- | --- |
//! | [`VolEstimatorSnapshot`] | `samples` is the whole physical ring in storage order, not oldest-first. `pos` is the next write index and `full` is the wrap flag. `full` is stored, not recomputed. Before the ring wraps, RMS uses only the slots before `pos`; every slot must still be finite. Squares accumulate in `f64` and the returned RMS is `f32`. | `1..=`[`MAX_SNAPSHOT_CAPACITY`] |
//! | [`EMASnapshot`] | `alpha`, `initialized`, and the initialized `value`. No sample window. An uninitialized EMA ignores `value` and seeds from the next finite input. | Not applicable |
//! | [`SMASnapshot`] | `window` is oldest-first. Restore keeps the declared capacity, not only the occupied length, and replaces `sum` with the canonical Welford total. A sum that passed the validation tolerance therefore need not round-trip bit for bit. | `0..=`[`MAX_SNAPSHOT_CAPACITY`] |
//!
//! Construction and restore share those capacity ranges, which is the
//! resolution of the SMA capacity contract: every SMA that `SMA::new` can
//! build can round-trip. `SMA` capacity `0` is a no-op (`update` returns `0.0`).
//! The volatility and SMA constructors panic outside those ranges. Restore returns
//! [`SnapshotError::InvalidCapacity`] before it reserves the estimator buffer.
//! Nothing is clamped or truncated. A successful SMA restore reserves at
//! least the declared capacity (`window.capacity() >= capacity`); the exact
//! `Vec` capacity is up to the allocator. `MAX_SNAPSHOT_CAPACITY` is
//! `1_000_000` (about 4 MiB of `f32` or 8 MiB of `f64`).
//!
//! For state produced through the estimator APIs, identical finite inputs,
//! and a codec round-trip that preserves numeric values, processing segment
//! A, restoring, then processing segment B matches uninterrupted A+B within
//! the absolute [`RESTORE_OUTPUT_TOLERANCE`] (`1e-6`). On the same build and
//! target the preserved summation order can make that continuation exact.
//! Portable persistence promises the tolerance, not bit-identical results
//! across compilers, targets, or codecs. The tolerance is a
//! checkpoint-continuation bound, not an error bound against real arithmetic.
//! It does not cover lossy codecs, direct mutation of the public EMA or SMA
//! fields, or EMA arithmetic that overflows to a non-finite value. Non-finite
//! `push` / `update` inputs are ignored and do not appear in snapshots taken
//! through those methods.

use std::error::Error;
use std::fmt;

use crate::numeric::{finite_or_zero, stable_mean};

/// Schema version written by [`crate::VolEstimator::snapshot`],
/// [`crate::EMA::snapshot`], and [`crate::SMA::snapshot`].
///
/// Current restore accepts only this value (`1`). The
/// [schema-v1 persistence contract](crate::snapshot#schema-v1-persistence-contract)
/// states which releases must keep reading those checkpoints and which edits
/// require a bump. No schema v2 is defined.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Largest supported window capacity for [`crate::VolEstimator`] and
/// [`crate::SMA`] construction and snapshot restore.
///
/// `1_000_000` samples is about 4 MiB as `f32` (`VolEstimator`) or 8 MiB as
/// `f64` (`SMA`). Bounding capacity keeps an externally supplied snapshot from
/// requesting an unbounded allocation during restore (`VolEstimator` ring and
/// declared `SMA` window capacity), and caps the reservation a constructor
/// makes for the same window.
///
/// The limit applies to construction and to `from_snapshot` / `restore`. It
/// does not apply to decoding: a codec can allocate `samples` or `window`
/// before validation runs. Bound encoded input separately.
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
///
/// Codec and deserialization failures are not this type. A payload that does
/// not decode never reaches these variants. See the [decoding and validation
/// rules](crate::snapshot#decoding-and-validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SnapshotError {
    /// `schema_version` does not match [`SNAPSHOT_SCHEMA_VERSION`].
    ///
    /// Both older and newer versions take this path. There is no fallback and
    /// no migration.
    IncompatibleVersion { found: u32, expected: u32 },
    /// Declared capacity is outside the estimator's supported range.
    ///
    /// [`crate::VolEstimator`] requires `1..=`[`MAX_SNAPSHOT_CAPACITY`].
    /// [`crate::SMA`] allows `0` (a no-op estimator) through the same ceiling.
    /// The `Display` text mentions a zero capacity; an over-ceiling capacity
    /// returns this same variant.
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
