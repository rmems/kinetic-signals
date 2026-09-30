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

use crate::numeric::all_finite;
use crate::snapshot::MAX_SNAPSHOT_CAPACITY;

/// Supported ceiling for the histogram `bins` requested from the entropy
/// entry points.
///
/// A request above this value is out of policy: the entropy functions return
/// the zeroed sentinel instead of attempting an unbounded histogram
/// allocation (see [`compute_shannon_entropy`] and
/// [`compute_shannon_entropy_into`]). The ceiling reuses the crate-wide
/// [`MAX_SNAPSHOT_CAPACITY`](crate::MAX_SNAPSHOT_CAPACITY) (`1_000_000`), a
/// histogram of about 8 MiB of `usize` counts, matching the allocation
/// rationale shared by `SMA::new` and `VolEstimator::new`.
pub const MAX_ENTROPY_BINS: usize = MAX_SNAPSHOT_CAPACITY;

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
/// Allocates a fresh histogram `Vec`. Prefer [`compute_shannon_entropy_into`]
/// when a caller-owned bin buffer can be reused across windows.
///
/// Returns a zeroed result when `data` has fewer than two samples, `bins`
/// is zero, or any sample is non-finite. A range that overflows `f64`
/// (finite values near opposite extremes) is treated the same way: equal-width
/// bins are undefined, so the empty sentinel is returned (`bin_count == 0`).
/// A constant (including near-constant with `max == min`) series yields zero
/// entropy with `bin_count == 1`.
///
/// # Resource policy
///
/// Supported `bins` are `1..=`[`MAX_ENTROPY_BINS`] (`1_000_000`). A request
/// above that ceiling is out of policy and returns the zeroed sentinel
/// (`bin_count == 0`) before any histogram allocation, rather than attempting
/// an unbounded reservation. The requested resolution is rejected outright,
/// never silently reduced. Because this wrapper delegates to
/// [`compute_shannon_entropy_into`], both entry points share this policy.
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
/// - Degenerate inputs (`data.len() < 2`, `bins == 0`, a non-finite sample,
///   an overflowing min–max range, or a constant series) **clear** `histogram`
///   (`len == 0`) and return the same result as [`compute_shannon_entropy`].
///   Capacity is retained.
/// - An out-of-policy `bins` (greater than [`MAX_ENTROPY_BINS`], `1_000_000`)
///   is handled the same way **before** any resize: `histogram` is **cleared**
///   (`len == 0`, capacity retained) and the zeroed sentinel is returned
///   (`bin_count == 0`). The requested resolution is rejected, never silently
///   reduced.
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
    if data.len() < 2 || bins == 0 || !all_finite(data) {
        histogram.clear();
        return zero_entropy();
    }

    let min = data.iter().copied().fold(f64::INFINITY, f64::min);
    let max = data.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;
    if !range.is_finite() {
        histogram.clear();
        return zero_entropy();
    }
    if range == 0.0 {
        histogram.clear();
        return EntropyResult {
            shannon: 0.0,
            relative: 0.0,
            bin_count: 1,
        };
    }

    if bins > MAX_ENTROPY_BINS {
        histogram.clear();
        return zero_entropy();
    }

    if histogram.len() != bins {
        histogram.clear();
        histogram.resize(bins, 0);
    } else {
        histogram.fill(0);
    }
    for &x in data {
        let bin = (((x - min) / range) * (bins as f64 - 1e-9)).floor() as usize;
        histogram[bin.min(bins - 1)] += 1;
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
            (&[1.0, f64::NAN, 2.0], 4),
            (&[f64::MAX, f64::MIN], 4),
        ];
        for &(data, bins) in cases {
            let allocated = compute_shannon_entropy(data, bins);
            let reused = compute_shannon_entropy_into(data, bins, &mut histogram);
            assert_entropy_eq(&allocated, &reused);
            if histogram.is_empty() {
                assert!(allocated.bin_count <= 1);
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

    #[test]
    fn test_entropy_bins_one_is_valid() {
        let data = [1.0, 2.0, 3.0, 4.0];
        let mut histogram = Vec::new();
        let res = compute_shannon_entropy_into(&data, 1, &mut histogram);
        assert_eq!(histogram.len(), 1);
        assert_eq!(histogram[0], data.len());
        assert_eq!(res.bin_count, 1);
        assert_eq!(res.shannon, 0.0);
        assert_entropy_eq(&res, &compute_shannon_entropy(&data, 1));
    }

    #[test]
    fn test_entropy_bins_at_ceiling_allocates() {
        let data = [1.0, 2.0, 3.0, 4.0];
        let mut histogram = Vec::new();
        let res = compute_shannon_entropy_into(&data, MAX_ENTROPY_BINS, &mut histogram);
        assert_eq!(histogram.len(), MAX_ENTROPY_BINS);
        assert_eq!(histogram.iter().sum::<usize>(), data.len());
        assert!(res.shannon > 0.0);
        assert_eq!(res.bin_count, data.len());
    }

    #[test]
    fn test_entropy_bins_above_ceiling_is_out_of_policy() {
        let data = [1.0, 2.0, 3.0, 4.0];
        let mut histogram = vec![7usize; 5];
        let res = compute_shannon_entropy_into(&data, MAX_ENTROPY_BINS + 1, &mut histogram);
        assert!(histogram.is_empty());
        assert_eq!(res.bin_count, 0);
        assert_eq!(res.shannon, 0.0);
        assert_eq!(res.relative, 0.0);
        assert_entropy_eq(&res, &compute_shannon_entropy(&data, MAX_ENTROPY_BINS + 1));
    }

    #[test]
    fn test_entropy_bins_usize_max_nondegenerate_is_out_of_policy() {
        let data = [1.0, 2.0, 3.0, 4.0];
        let mut histogram = vec![7usize; 5];
        let res = compute_shannon_entropy_into(&data, usize::MAX, &mut histogram);
        assert!(histogram.is_empty());
        assert_eq!(res.bin_count, 0);
        assert_eq!(res.shannon, 0.0);
        assert_entropy_eq(&res, &compute_shannon_entropy(&data, usize::MAX));
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

    #[test]
    fn test_entropy_overflow_range_is_empty() {
        let res = compute_shannon_entropy(&[f64::MAX, f64::MIN], 4);
        assert_eq!(res.bin_count, 0);
        assert_eq!(res.shannon, 0.0);
        assert_eq!(res.relative, 0.0);
    }
}
