// SPDX-License-Identifier: MIT OR Apache-2.0

//! Streaming technical indicators for real-valued signals.
//!
//! Provides lightweight, allocation-conscious estimators suitable for
//! high-velocity update loops:
//!
//! - [`EMA`] — exponential moving average
//! - [`SMA`] — fixed-window simple moving average
//! - [`ZScore`] — z-score (standard-score) normalization helper

/// Exponential moving average (EMA) for streaming data.
///
/// Smoothing factor \(\alpha = 2 / (\text{period} + 1)\). The first
/// [`update`](EMA::update) seeds the average; subsequent calls blend the new
/// sample with the previous value.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::EMA;
///
/// let mut ema = EMA::new(9);
/// assert_eq!(ema.update(100.0), 100.0);
/// let next = ema.update(110.0);
/// assert!(next > 100.0 && next < 110.0);
/// ```
#[derive(Debug, Clone)]
pub struct EMA {
    /// Current EMA value.
    pub value: f64,
    /// Smoothing factor \(\alpha \in (0, 1]\).
    pub alpha: f64,
    /// Whether at least one sample has been observed.
    pub initialized: bool,
}

impl EMA {
    /// Create an EMA with the classic period-based \(\alpha\).
    pub fn new(period: usize) -> Self {
        let alpha = 2.0 / (period as f64 + 1.0);
        Self {
            value: 0.0,
            alpha,
            initialized: false,
        }
    }

    /// Incorporate `new_value` and return the updated EMA.
    pub fn update(&mut self, new_value: f64) -> f64 {
        if !self.initialized {
            self.value = new_value;
            self.initialized = true;
        } else {
            self.value = self.alpha * new_value + (1.0 - self.alpha) * self.value;
        }
        self.value
    }
}

/// Z-score tracking helper for signal normalization.
///
/// `mean`/`std_dev` are a caller-managed value pair (e.g. from an upstream
/// mean/variance tracker) kept alongside the crate's other stateful types;
/// they are not read by [`compute`](ZScore::compute), which is a pure
/// function of its three arguments and does not require constructing a
/// `ZScore` value at all.
#[derive(Debug, Clone)]
pub struct ZScore {
    /// Reference mean (caller-managed; not updated automatically).
    pub mean: f64,
    /// Reference standard deviation (caller-managed).
    pub std_dev: f64,
}

impl ZScore {
    /// Return \((value - mean) / std_dev\), or `0.0` if `std_dev` is near zero.
    pub fn compute(value: f64, mean: f64, std_dev: f64) -> f64 {
        if std_dev > 1e-12 {
            (value - mean) / std_dev
        } else {
            0.0
        }
    }
}

/// Simple moving average (SMA) over a fixed-capacity window.
///
/// When the window is full, the oldest sample is dropped on each update so
/// memory stays O(capacity).
///
/// # Example
///
/// ```rust
/// use kinetic_signals::SMA;
///
/// let mut sma = SMA::new(3);
/// sma.update(1.0);
/// sma.update(2.0);
/// assert_eq!(sma.update(3.0), 2.0);
/// assert_eq!(sma.update(4.0), 3.0);
/// ```
#[derive(Debug, Clone)]
pub struct SMA {
    /// Samples currently in the window (oldest first).
    pub window: Vec<f64>,
    /// Maximum number of samples retained.
    pub capacity: usize,
    /// Running sum of samples in `window`.
    pub sum: f64,
}

impl SMA {
    /// Create an SMA that retains at most `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            window: Vec::with_capacity(capacity),
            capacity,
            sum: 0.0,
        }
    }

    /// Incorporate `new_value` and return the updated window mean.
    pub fn update(&mut self, new_value: f64) -> f64 {
        if self.window.len() == self.capacity {
            self.sum -= self.window.remove(0);
        }
        self.window.push(new_value);
        self.sum += new_value;
        self.sum / self.window.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ema() {
        let mut ema = EMA::new(9);
        assert_eq!(ema.update(100.0), 100.0);
        let next = ema.update(110.0);
        assert!(next > 100.0 && next < 110.0);
    }

    #[test]
    fn test_sma() {
        let mut sma = SMA::new(3);
        sma.update(1.0);
        sma.update(2.0);
        assert_eq!(sma.update(3.0), 2.0);
        assert_eq!(sma.update(4.0), 3.0);
    }

    #[test]
    fn test_zscore_known_mean_std() {
        assert!((ZScore::compute(110.0, 100.0, 10.0) - 1.0).abs() < 1e-12);
        assert!((ZScore::compute(80.0, 100.0, 10.0) - (-2.0)).abs() < 1e-12);
        assert!((ZScore::compute(100.0, 100.0, 10.0)).abs() < 1e-12);
    }

    #[test]
    fn test_zscore_zero_std() {
        assert_eq!(ZScore::compute(42.0, 42.0, 0.0), 0.0);
        assert_eq!(ZScore::compute(100.0, 50.0, 1e-13), 0.0);
    }

    #[test]
    fn test_zscore_multiple_values_known_distribution() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let n = data.len() as f64;
        let mean = data.iter().sum::<f64>() / n;
        let variance = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let std = variance.sqrt();

        assert!((mean - 5.0).abs() < 1e-12);
        assert!((std - 2.0).abs() < 1e-12);

        let z_min = ZScore::compute(2.0, mean, std);
        let z_mean = ZScore::compute(5.0, mean, std);
        let z_max = ZScore::compute(9.0, mean, std);

        assert!((z_min - (-1.5)).abs() < 1e-12);
        assert!(z_mean.abs() < 1e-12);
        assert!((z_max - 2.0).abs() < 1e-12);

        for &v in &data {
            let z = ZScore::compute(v, mean, std);
            assert!(z.is_finite());
        }
    }
}
