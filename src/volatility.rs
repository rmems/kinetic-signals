// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rolling RMS volatility estimator — zero-alloc, fixed-size ring buffer.

use crate::snapshot::{
    SNAPSHOT_SCHEMA_VERSION, SnapshotError, VolEstimatorSnapshot, alloc_zeros_f32,
};

/// Rolling RMS volatility estimator over a fixed window.
///
/// Stores absolute log-returns in a circular buffer and computes
/// `sqrt(mean(r²))` over the window. Clamped to [0, 1]. Non-finite pushes
/// are ignored. Squares are accumulated in `f64` so extreme finite `f32`
/// samples do not overflow before the clamp.
///
/// # Example
/// ```rust
/// use kinetic_signals::VolEstimator;
///
/// let mut v = VolEstimator::new(50);
/// v.push(0.01);
/// v.push(0.02);
/// let vol = v.rms();
/// assert!(vol > 0.0);
/// ```
#[derive(Debug, Clone)]
pub struct VolEstimator {
    buf: Vec<f32>,
    pos: usize,
    full: bool,
    cap: usize,
}

impl VolEstimator {
    /// Create a new estimator with the given window size.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is `0`.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        Self {
            buf: vec![0.0; capacity],
            pos: 0,
            full: false,
            cap: capacity,
        }
    }

    /// Push one absolute log-return into the ring buffer.
    ///
    /// Non-finite values are ignored so a `NaN`/`Inf` tick cannot poison RMS.
    pub fn push(&mut self, abs_log_return: f32) {
        if !abs_log_return.is_finite() {
            return;
        }
        self.buf[self.pos] = abs_log_return;
        self.pos += 1;
        if self.pos >= self.cap {
            self.pos = 0;
            self.full = true;
        }
    }

    /// RMS volatility: `sqrt(mean(r²))` over the window, clamped to [0, 1].
    pub fn rms(&self) -> f32 {
        let n = if self.full { self.cap } else { self.pos };
        if n == 0 {
            return 0.0;
        }
        let sum_sq: f64 = self.buf[..n]
            .iter()
            .map(|&r| {
                let x = f64::from(r);
                x * x
            })
            .sum();
        (sum_sq / n as f64).sqrt().clamp(0.0, 1.0) as f32
    }

    /// Number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        if self.full { self.cap } else { self.pos }
    }

    /// True if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Capture the physical ring, write head, and wrap flag.
    ///
    /// The snapshot schema version is [`crate::SNAPSHOT_SCHEMA_VERSION`].
    /// Restore with [`Self::restore`] or [`Self::from_snapshot`]. Subsequent
    /// [`Self::rms`] / [`Self::push`] outputs match a continuously processed
    /// estimator within [`crate::RESTORE_OUTPUT_TOLERANCE`]. Storage order is
    /// preserved so `f32` summation is bit-identical after restore.
    ///
    /// # Example
    /// ```rust
    /// use kinetic_signals::VolEstimator;
    ///
    /// let mut a = VolEstimator::new(4);
    /// a.push(0.01);
    /// a.push(0.02);
    /// let snap = a.snapshot();
    /// assert_eq!(snap.schema_version, kinetic_signals::SNAPSHOT_SCHEMA_VERSION);
    /// let mut b = VolEstimator::from_snapshot(&snap).unwrap();
    /// a.push(0.03);
    /// b.push(0.03);
    /// assert_eq!(a.rms(), b.rms());
    /// ```
    pub fn snapshot(&self) -> VolEstimatorSnapshot {
        VolEstimatorSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: self.cap,
            pos: self.pos,
            full: self.full,
            samples: self.buf.clone(),
        }
    }

    /// Build an estimator from a validated snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`crate::SnapshotError`] when the schema version, capacity,
    /// sample count, or finiteness checks fail, or when the ring buffer cannot
    /// be allocated. No estimator is constructed.
    pub fn from_snapshot(snapshot: &VolEstimatorSnapshot) -> Result<Self, SnapshotError> {
        snapshot.validate()?;
        let mut buf = alloc_zeros_f32(snapshot.capacity)?;
        buf.copy_from_slice(&snapshot.samples);
        Ok(Self {
            buf,
            pos: snapshot.pos,
            full: snapshot.full,
            cap: snapshot.capacity,
        })
    }

    /// Replace `self` with a restored snapshot.
    ///
    /// On failure `self` is left unchanged.
    pub fn restore(&mut self, snapshot: &VolEstimatorSnapshot) -> Result<(), SnapshotError> {
        *self = Self::from_snapshot(snapshot)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rms_three_values() {
        let mut v = VolEstimator::new(3);
        v.push(0.1);
        v.push(0.2);
        v.push(0.3);
        assert!((v.rms() - 0.2160).abs() < 0.01);
    }

    #[test]
    fn test_empty() {
        let v = VolEstimator::new(10);
        assert_eq!(v.rms(), 0.0);
        assert!(v.is_empty());
    }

    #[test]
    fn test_ring_overflow() {
        let mut v = VolEstimator::new(3);
        for i in 0..10 {
            v.push(i as f32 * 0.1);
        }
        assert_eq!(v.len(), 3);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_vol_estimator_zero_capacity_panics() {
        let _ = VolEstimator::new(0);
    }

    #[test]
    fn snapshot_restore_matches_continuous_after_wrap() {
        let series: Vec<f32> = (1..12).map(|i| i as f32 * 0.01).collect();
        let (a, b) = series.split_at(7);

        let mut continuous = VolEstimator::new(4);
        for &x in a.iter().chain(b) {
            continuous.push(x);
        }

        let mut restored = VolEstimator::new(4);
        for &x in a {
            restored.push(x);
        }
        let snap = restored.snapshot();
        restored.restore(&snap).unwrap();
        for &x in b {
            restored.push(x);
        }

        assert_eq!(continuous.rms(), restored.rms());
        assert_eq!(continuous.len(), restored.len());
    }

    #[test]
    fn snapshot_preserves_rms_summation_order_after_wrap() {
        let cap = 10_000;
        let mut v = VolEstimator::new(cap);
        for _ in 0..cap {
            v.push(0.01);
        }
        for _ in 0..cap / 2 {
            v.push(1.0);
        }
        let restored = VolEstimator::from_snapshot(&v.snapshot()).unwrap();
        assert_eq!(v.rms(), restored.rms());
    }

    #[test]
    fn restore_rejects_non_finite_without_mutating() {
        let mut est = VolEstimator::new(3);
        est.push(0.1);
        est.push(0.2);
        let before_rms = est.rms();
        let before_len = est.len();
        let bad = VolEstimatorSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            capacity: 3,
            pos: 2,
            full: false,
            samples: vec![0.1, f32::NAN, 0.0],
        };
        assert_eq!(est.restore(&bad), Err(SnapshotError::NonFinite));
        assert_eq!(est.len(), before_len);
        assert_eq!(est.rms(), before_rms);
    }

    #[test]
    fn test_rms_skips_nonfinite() {
        let mut v = VolEstimator::new(3);
        v.push(0.1);
        v.push(f32::NAN);
        v.push(f32::INFINITY);
        v.push(0.2);
        assert_eq!(v.len(), 2);
        let rms = v.rms();
        assert!(rms.is_finite());
        let expected = ((0.1f64.powi(2) + 0.2f64.powi(2)) / 2.0).sqrt() as f32;
        assert!((rms - expected).abs() < 1e-6);
    }

    #[test]
    fn test_rms_extreme_finite_clamped() {
        let mut v = VolEstimator::new(2);
        v.push(1e20);
        v.push(1e20);
        let rms = v.rms();
        assert!(rms.is_finite());
        assert_eq!(rms, 1.0);
    }

    #[test]
    fn test_rms_constant_zero() {
        let mut v = VolEstimator::new(4);
        for _ in 0..4 {
            v.push(0.0);
        }
        assert_eq!(v.rms(), 0.0);
    }
}
