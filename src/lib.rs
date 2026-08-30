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
//! use kinetic_signals::{compute_hurst, compute_surprise, SurpriseParams};
//!
//! let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
//! let h_result = compute_hurst(&data);
//!
//! let params = SurpriseParams::default();
//! let surprise = compute_surprise(150.0, 100.0, &params);
//! ```
//!
pub mod entropy;
pub mod hawkes;
pub mod hurst;
pub mod indicators;
mod real;
pub mod stats;
pub mod surprise;
pub mod volatility;

/// Initialize Sentry from the `SENTRY_DSN` environment variable.
///
/// Returns `Some(guard)` if the DSN is set and non-empty, `None` otherwise.
/// The guard must be kept alive for the duration of the program — when dropped,
/// Sentry flushes pending events (up to 2 seconds).
///
/// Only available when the `sentry` feature is enabled.
#[cfg(feature = "sentry")]
pub fn init_sentry() -> Option<sentry::ClientInitGuard> {
    // SAFETY: env::var is safe; only env::set_var/remove_var are unsafe in edition 2024.
    match std::env::var("SENTRY_DSN") {
        Ok(dsn) if !dsn.is_empty() => {
            let guard = sentry::init((
                dsn,
                sentry::ClientOptions {
                    release: sentry::release_name!(),
                    ..Default::default()
                },
            ));
            Some(guard)
        }
        _ => None,
    }
}

pub use entropy::{EntropyResult, compute_shannon_entropy};
pub use hawkes::{HawkesParams, HawkesResult, compute_hawkes, compute_hawkes_streaming};
pub use hurst::{HurstResult, compute_hurst};
pub use indicators::{EMA, SMA, ZScore};
pub use stats::{SignalStats, compute_signal_stats};
pub use surprise::{
    SurpriseParams, SurpriseResult, compute_surprise, compute_surprise_sequence, detect_anomaly,
};
pub use volatility::VolEstimator;

/// Convenience glob-import of every public type and function from the
/// crate's computation modules (entropy, hawkes, hurst, indicators, stats,
/// surprise, volatility). `init_sentry`, a crate-root utility gated behind
/// the `sentry` feature, is intentionally **not** re-exported here: it is
/// setup/observability plumbing, not a signal-processing primitive.
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
