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
//! [`compute_shannon_entropy_into`] writes the same histogram into a
//! caller-owned buffer so successive windows can reuse it.

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

/// Compute Shannon entropy of a signal using histogram discretization.
///
/// Allocates a fresh histogram `Vec`. Prefer [`compute_shannon_entropy_into`]
/// when a caller-owned bin buffer can be reused across windows.
///
/// Returns a zeroed result when `data` has fewer than two samples or `bins`
/// is zero. A constant series yields zero entropy with `bin_count == 1`.
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
    let mut histogram = Vec::new();
    compute_shannon_entropy_into(data, bins, &mut histogram)
}

/// Compute Shannon entropy, writing histogram counts into `histogram`.
///
/// This is the hot output path for batch entropy: high-frequency telemetry
/// loops can keep one bin buffer and reuse it for each window instead of
/// allocating on every call.
///
/// # Buffer length and overwrite
///
/// - Degenerate inputs (`data.len() < 2`, `bins == 0`, or a constant series)
///   **clear** `histogram` (`len == 0`) and return the same result as
///   [`compute_shannon_entropy`]. Capacity is retained.
/// - Otherwise `histogram` is resized to `bins` if needed, **zeroed**, then
///   overwritten with occupancy counts. After return, `histogram.len() == bins`
///   and `histogram[i]` is the count for bin `i`.
/// - Remaining **capacity is retained** (this function never calls
///   `shrink_to_fit`). If `histogram.capacity() >= bins` on a non-degenerate
///   call, no histogram allocation is performed.
///
/// # Aliasing
///
/// `data` is borrowed immutably and `histogram` is borrowed mutably for the
/// duration of the call. In safe Rust they cannot alias (`f64` vs `usize`).
/// Counts are written only to `histogram`; `data` is never mutated.
///
/// # Example
///
/// ```rust
/// use kinetic_signals::compute_shannon_entropy_into;
///
/// let window = [1.0, 2.0, 3.0, 4.0];
/// let mut histogram = Vec::with_capacity(8);
/// let res = compute_shannon_entropy_into(&window, 8, &mut histogram);
/// assert_eq!(histogram.len(), 8);
/// assert!(res.shannon > 0.0);
///
/// let next = [1.0, 1.1, 1.2, 9.0];
/// let res = compute_shannon_entropy_into(&next, 8, &mut histogram);
/// assert_eq!(histogram.len(), 8);
/// assert!(res.relative <= 1.0);
/// ```
pub fn compute_shannon_entropy_into(
    data: &[f64],
    bins: usize,
    histogram: &mut Vec<usize>,
) -> EntropyResult {
    if data.len() < 2 || bins == 0 {
        histogram.clear();
        return EntropyResult {
            shannon: 0.0,
            relative: 0.0,
            bin_count: 0,
        };
    }

    let min = data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
    let max = data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let range = max - min;

    if range == 0.0 {
        histogram.clear();
        return EntropyResult {
            shannon: 0.0,
            relative: 0.0,
            bin_count: 1,
        };
    }

    if histogram.len() != bins {
        histogram.clear();
        histogram.resize(bins, 0);
    } else {
        histogram.fill(0);
    }
    for &x in data {
        let bin = (((x - min) / range) * (bins as f64 - 1e-9)).floor() as usize;
        let bin = bin.min(bins - 1);
        histogram[bin] += 1;
    }

    let n = data.len() as f64;
    let mut shannon = 0.0;
    let mut actual_bins = 0;

    for &count in histogram.iter() {
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

    fn assert_entropy_eq(a: &EntropyResult, b: &EntropyResult) {
        assert_eq!(a.shannon, b.shannon);
        assert_eq!(a.relative, b.relative);
        assert_eq!(a.bin_count, b.bin_count);
    }

    #[test]
    fn test_entropy_into_matches_allocating() {
        let mut histogram = vec![99usize; 3];
        let cases: &[(&[f64], usize)] = &[
            (&[], 4),
            (&[1.0], 4),
            (&[1.0, 2.0], 0),
            (&[1.0, 1.0], 4),
            (&[1.0, 2.0], 2),
            (&[1.0, 2.0, 3.0, 4.0], 4),
            (&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 4),
        ];
        for &(data, bins) in cases {
            let allocated = compute_shannon_entropy(data, bins);
            let reused = compute_shannon_entropy_into(data, bins, &mut histogram);
            assert_entropy_eq(&allocated, &reused);
            if data.len() < 2 || bins == 0 || data.iter().all(|&x| x == data[0]) {
                assert!(histogram.is_empty());
            } else {
                assert_eq!(histogram.len(), bins);
                assert_eq!(histogram.iter().sum::<usize>(), data.len());
            }
        }
    }

    #[test]
    fn test_entropy_into_resizes_and_keeps_capacity() {
        let mut histogram = Vec::new();
        let data = [1.0, 2.0, 3.0, 4.0];

        compute_shannon_entropy_into(&data, 4, &mut histogram);
        assert_eq!(histogram.len(), 4);
        let cap = histogram.capacity();
        assert!(cap >= 4);

        compute_shannon_entropy_into(&data, 2, &mut histogram);
        assert_eq!(histogram.len(), 2);
        assert_eq!(histogram.capacity(), cap);

        compute_shannon_entropy_into(&[], 4, &mut histogram);
        assert!(histogram.is_empty());
        assert_eq!(histogram.capacity(), cap);
    }

    #[test]
    fn test_entropy_degenerate_does_not_reserve_huge_bins() {
        let empty = compute_shannon_entropy(&[], usize::MAX);
        assert_eq!(empty.bin_count, 0);
        let short = compute_shannon_entropy(&[1.0], usize::MAX);
        assert_eq!(short.bin_count, 0);
        let constant = compute_shannon_entropy(&[1.0, 1.0], usize::MAX);
        assert_eq!(constant.bin_count, 1);
        assert_eq!(constant.shannon, 0.0);
    }
}
