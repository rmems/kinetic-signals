// SPDX-License-Identifier: MIT OR Apache-2.0

//! # kinetic-signals
//!
//! Streaming feature extraction for high-velocity stochastic signals.
//!
//! A high-performance, domain-agnostic Rust crate for computing streaming
//! statistics, long-memory estimates, point-process intensity, and normalized
//! surprise metrics.
//!
//! ## Features
//!
//! - **Hurst Exponent** - Detects long-term memory and persistence in time-series data
//! - **Hawkes Process** - Models self-exciting event clusters in point processes
//! - **Surprise** - Detects anomalous transition magnitudes via normalized log-ratio z-scores
//! - **Volatility** - Real-time variance and standard deviation tracking
//! - **Shannon Entropy** - Measures signal complexity and information density
//! - **Indicators** - Moving averages (EMA, SMA) and Z-score tracking
//! - **Snapshot / restore** - Versioned checkpoints for `VolEstimator`, `EMA`, and `SMA`
//! - **Buffer reuse** - [`compute_surprise_sequence_into`] and
//!   [`compute_shannon_entropy_into`] write into caller-owned `Vec`s
//!
//! ## Performance (Ryzen 9 9950X)
//!
//! - Hurst (100 samples): ~50μs
//! - Hawkes (10 events): ~5μs
//! - Surprise: ~100ns
//!
//! ## Example
//!
//! ```rust
//! use kinetic_signals::{
//!     SurpriseParams, compute_hurst, compute_surprise, compute_surprise_sequence_into,
//! };
//!
//! let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
//! let h_result = compute_hurst(&data);
//!
//! let params = SurpriseParams::default();
//! let surprise = compute_surprise(150.0, 100.0, &params);
//!
//! let mut buf = Vec::new();
//! compute_surprise_sequence_into(&[100.0, 101.0, 102.0], &params, &mut buf);
//! assert_eq!(buf.len(), 2);
//! ```
//!
//! ## Numeric contract
//!
//! Public numerical APIs expect finite inputs. Non-finite values (`NaN`,
//! `±Inf`) and other ill-conditioned cases (empty/short series, constant
//! windows, non-positive surprise samples, near-zero variance) yield a
//! **documented finite sentinel** rather than an accidental `NaN`. See each
//! function's rustdoc and the README "Numeric input contract" table.
//!
pub mod entropy;
pub mod hawkes;
pub mod hurst;
pub mod indicators;
mod numeric;
mod real;
pub mod snapshot;
pub mod stats;
pub mod surprise;
pub mod volatility;

pub use entropy::{EntropyResult, compute_shannon_entropy, compute_shannon_entropy_into};
pub use hawkes::{HawkesParams, HawkesResult, compute_hawkes, compute_hawkes_streaming};
pub use hurst::{HurstResult, compute_hurst};
pub use indicators::{EMA, SMA, ZScore};
pub use snapshot::{
    EMASnapshot, RESTORE_OUTPUT_TOLERANCE, SMASnapshot, SNAPSHOT_SCHEMA_VERSION, SnapshotError,
    VolEstimatorSnapshot,
};
pub use stats::{SignalStats, compute_signal_stats};
pub use surprise::{
    SurpriseParams, SurpriseResult, compute_surprise, compute_surprise_sequence,
    compute_surprise_sequence_into, detect_anomaly, surprise_sequence_len,
};
pub use volatility::VolEstimator;

/// Convenience glob-import of every public type and function from the
/// crate's computation modules (entropy, hawkes, hurst, indicators, snapshot,
/// stats, surprise, volatility). Application-level observability integrations are
/// intentionally outside this signal-processing crate.
///
/// The prelude is covered by the crate's pre-1.0 SemVer policy (see the
/// "Pre-1.0 SemVer / Stability Policy" section of `README.md`): removing or
/// renaming an item exported here follows the same breaking-change rules as
/// removing it from its owning module. Adding a new public item is usually
/// safe, but because the prelude re-exports via glob (`pub use ...::*`), a
/// new name can still collide with a downstream glob import, and a new
/// trait implementation can make previously-unambiguous method calls
/// ambiguous; treat such additions with the same compatibility review as
/// any other public API change.
pub mod prelude {
    pub use crate::entropy::*;
    pub use crate::hawkes::*;
    pub use crate::hurst::*;
    pub use crate::indicators::*;
    pub use crate::snapshot::*;
    pub use crate::stats::*;
    pub use crate::surprise::*;
    pub use crate::volatility::*;
}

/// Compile-time assertion: all public types are `Send + Sync`.
/// If this fails, update docs/boundary-matrix.md thread-safety section.
/// MAINTENANCE: Add new public types here when they are added to any module.
fn _assert_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VolEstimator>();
    assert_send_sync::<VolEstimatorSnapshot>();
    assert_send_sync::<EMASnapshot>();
    assert_send_sync::<SMASnapshot>();
    assert_send_sync::<SnapshotError>();
    assert_send_sync::<HurstResult>();
    assert_send_sync::<HurstResult<f32>>();
    assert_send_sync::<HawkesResult>();
    assert_send_sync::<HawkesParams>();
    assert_send_sync::<surprise::SurpriseResult>();
    assert_send_sync::<surprise::SurpriseResult<f32>>();
    assert_send_sync::<surprise::SurpriseParams>();
    assert_send_sync::<surprise::SurpriseParams<f32>>();
    assert_send_sync::<EntropyResult>();
    assert_send_sync::<SignalStats>();
    assert_send_sync::<EMA>();
    assert_send_sync::<SMA>();
    assert_send_sync::<ZScore>();
}
