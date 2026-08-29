// SPDX-License-Identifier: MIT OR Apache-2.0

//! Rolling RMS volatility estimator — zero-alloc, fixed-size ring buffer.

/// Rolling RMS volatility estimator over a fixed window.
///
/// Stores absolute log-returns in a circular buffer and computes
/// `sqrt(mean(r²))` over the window. Clamped to [0, 1].
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
    pub fn push(&mut self, abs_log_return: f32) {
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
        let sum_sq: f32 = self.buf[..n].iter().map(|r| r * r).sum();
        (sum_sq / n as f32).sqrt().clamp(0.0, 1.0)
    }

    /// Number of samples currently in the buffer.
    pub fn len(&self) -> usize {
        if self.full { self.cap } else { self.pos }
    }

    /// True if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
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
        // RMS([0.1, 0.2, 0.3]) = sqrt((0.01+0.04+0.09)/3) ≈ 0.2160
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
}
