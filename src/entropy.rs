// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shannon entropy of a real-valued signal via histogram discretization.
//!
//! Entropy quantifies the average information content (disorder) of a sample
//! distribution. Values near zero indicate a highly peaked / deterministic
//! signal; values near \(\ln(\text{bins})\) indicate a near-uniform distribution.
//!
//! [`compute_shannon_entropy`] bins samples into equal-width histogram bins
//! spanning the observed min–max range, then computes natural-log Shannon
//! entropy and a relative (normalized) form in \([0, 1]\).

use crate::numeric::all_finite;

/// Result of a Shannon entropy computation.
#[derive(Debug, Clone)]
pub struct EntropyResult {
    /// Shannon entropy in nats (\( -\sum p_i \ln p_i \)).
    pub shannon: f64,
    /// Entropy normalized by \(\ln(\text{bins})\), so the range is \([0, 1]\).
    pub relative: f64,
    /// Number of histogram bins that received at least one sample.
    pub bin_count: usize,
}

fn zero_entropy() -> EntropyResult {
    EntropyResult {
        shannon: 0.0,
        relative: 0.0,
        bin_count: 0,
    }
}

/// Compute Shannon entropy of a signal using histogram discretization.
///
/// Returns a zeroed result when `data` has fewer than two samples, `bins`
/// is zero, or any sample is non-finite. A constant (including near-constant
/// with `max == min`) series yields zero entropy with `bin_count == 1`.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::compute_shannon_entropy;
///
/// let data = vec![1.0, 2.0, 3.0, 4.0];
/// let res = compute_shannon_entropy(&data, 4);
/// assert!(res.shannon > 0.0);
/// assert!(res.relative > 0.0 && res.relative <= 1.0);
/// ```
pub fn compute_shannon_entropy(data: &[f64], bins: usize) -> EntropyResult {
    if data.len() < 2 || bins == 0 || !all_finite(data) {
        return zero_entropy();
    }

    let min = data.iter().copied().fold(f64::INFINITY, f64::min);
    let max = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if !range.is_finite() {
        return zero_entropy();
    }
    if range == 0.0 {
        return EntropyResult {
            shannon: 0.0,
            relative: 0.0,
            bin_count: 1,
        };
    }

    let mut histogram = vec![0usize; bins];
    for &x in data {
        let bin = (((x - min) / range) * (bins as f64 - 1e-9)).floor() as usize;
        histogram[bin.min(bins - 1)] += 1;
    }

    let n = data.len() as f64;
    let mut shannon = 0.0;
    let mut actual_bins = 0;
    for &count in &histogram {
        if count > 0 {
            let p = count as f64 / n;
            shannon -= p * p.ln();
            actual_bins += 1;
        }
    }

    let max_entropy = (bins as f64).ln();
    let relative = if max_entropy > 0.0 {
        shannon / max_entropy
    } else {
        0.0
    };

    EntropyResult {
        shannon,
        relative,
        bin_count: actual_bins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_uniform() {
        let data = vec![1.0, 2.0, 3.0, 4.0];
        let res = compute_shannon_entropy(&data, 4);
        assert!(res.shannon > 0.0);
        assert_eq!(res.bin_count, 4);
    }

    #[test]
    fn test_entropy_constant() {
        let data = vec![1.0, 1.0, 1.0, 1.0];
        let res = compute_shannon_entropy(&data, 4);
        assert_eq!(res.shannon, 0.0);
        assert_eq!(res.bin_count, 1);
    }

    #[test]
    fn test_entropy_nonfinite_is_zeroed() {
        let nan = compute_shannon_entropy(&[1.0, f64::NAN, 2.0], 4);
        assert_eq!(nan.shannon, 0.0);
        assert_eq!(nan.relative, 0.0);
        assert_eq!(nan.bin_count, 0);

        let inf = compute_shannon_entropy(&[1.0, f64::INFINITY], 4);
        assert_eq!(inf.bin_count, 0);
        assert_eq!(inf.shannon, 0.0);
    }

    #[test]
    fn test_entropy_near_constant_is_finite() {
        let data = [1.0, 1.0 + 1e-18, 1.0];
        let res = compute_shannon_entropy(&data, 8);
        assert!(res.shannon.is_finite());
        assert!(res.relative.is_finite());
        assert!((0.0..=1.0).contains(&res.relative));
    }

    #[test]
    fn test_entropy_short_and_zero_bins() {
        assert_eq!(compute_shannon_entropy(&[1.0], 4).bin_count, 0);
        assert_eq!(compute_shannon_entropy(&[1.0, 2.0], 0).bin_count, 0);
    }
}
