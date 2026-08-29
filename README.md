# kinetic-signals

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://opensource.org/licenses/MIT)
[![codecov](https://codecov.io/gh/rmems/kinetic-signals/branch/main/graph/badge.svg)](https://codecov.io/gh/rmems/kinetic-signals)

Streaming feature extraction for high-velocity stochastic signals.

A high-performance, domain-agnostic Rust crate for computing streaming signal statistics, point-process intensity features, and anomaly metrics on stochastic time-series.

## Features

- **Zero required dependencies** - No external crates by default; optional `sentry` feature available
- **Hurst Exponent** - Detects long-term memory and persistence in time-series data
- **Hawkes Process** - Models self-exciting event clusters in point-process streams
- **Surprise** - Detects anomalous transition magnitudes via normalized log-ratio z-scores
- **Volatility (RMS)** - Rolling ring-buffer volatility tracking via `VolEstimator`
- **Shannon Entropy** - Measures signal complexity and information density
- **Indicators** - Moving averages (EMA, SMA) and Z-score tracking
- **Signal Stats** - High-order moments (Skewness, Kurtosis)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kinetic-signals = { git = "https://github.com/rmems/kinetic-signals" }
```

## Usage

```rust
use kinetic_signals::{
    compute_hurst, compute_hawkes, compute_surprise, detect_anomaly,
    hawkes::HawkesParams, surprise::SurpriseParams, VolEstimator,
};

// Hurst Exponent - detect trending vs random behavior
let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
let result = compute_hurst(&data);
println!("H = {:.3}, persistent = {}", result.h, result.is_persistent);

// Hawkes Process - model event clustering
let params = HawkesParams::default();
let events = vec![0.0, 0.01, 0.02, 0.1, 0.5];
let result = compute_hawkes(&events, &params);
println!("Intensity = {:.3}", result.intensity);

// Surprise - detect anomalous transitions
let params = SurpriseParams::default();
let surprise = compute_surprise(150.0, 100.0, &params);
if detect_anomaly(&surprise, &params) {
    println!("ANOMALY DETECTED! z = {:.2}", surprise.z_score);
}

// Volatility - rolling RMS of absolute log-returns
let mut vol = VolEstimator::new(64);
vol.push(0.01);
vol.push(0.02);
println!("RMS vol = {:.4}", vol.rms());
```

### Demo

Run the included demo:

```bash
cargo run --example demo
```

### Development

**MSRV:** Rust >= 1.85 (edition 2024)

```bash
# Build and test (--all-features requires network for sentry crate download)
cargo build
cargo test --all-features

# Lint and format
cargo clippy --all-targets --all-features
cargo fmt

# Run with sentry error reporting
SENTRY_DSN=https://...@... cargo run --example demo --features sentry
```

**Test coverage** (requires `cargo-llvm-cov`):

```bash
# Generate lcov report for CI
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# Open HTML report locally
cargo llvm-cov --all-features --workspace --open
```

Coverage reports are automatically generated and uploaded to [Codecov](https://codecov.io/gh/rmems/kinetic-signals) in CI via the [coverage workflow](.github/workflows/coverage.yml) on every push to `main` and in pull requests. Results are also available via the badge at the top of this README.

**CI workflows:**

- [Build & Test](.github/workflows/ci.yml) — fmt, clippy, build, test
- [Coverage](.github/workflows/coverage.yml) — cargo-llvm-cov + Codecov upload
- [Docker](.github/workflows/docker.yml) — containerized build + test
- [Sentry Release](.github/workflows/sentry-release.yml) — creates Sentry release on tag push

**Docker** (reproducible build):

```bash
docker build -t kinetic-signals .
docker run --rm kinetic-signals
```

### Numeric types

Most APIs use `f64`. `compute_hurst` and the surprise helpers are generic and support `f32` and `f64`. `VolEstimator` consumes `f32` absolute log-returns and computes rolling RMS volatility.

## Performance

Built with aggressive optimizations for real-time inference:

Typical execution times (Ryzen 9 9950X):
- Hurst (100 samples): ~50μs
- Hawkes (10 events): ~5μs
- Surprise: ~100ns

## Upgrading from v0.3.x

v0.4.0 removes the deprecated GBM aliases. Replace with the domain-agnostic names:

| Removed (v0.3.x)                 | Use instead                |
|----------------------------------|----------------------------|
| `compute_gbm_surprise`           | `compute_surprise`         |
| `compute_gbm_surprise_sequence`  | `compute_surprise_sequence`|
| `GBMParams`                      | `SurpriseParams`           |
| `GBMResult`                      | `SurpriseResult`           |
| `gbm::detect_anomaly`            | `surprise::detect_anomaly` |

## Pre-1.0 SemVer / Stability Policy

`kinetic-signals` is pre-1.0 (`0.x.y`) and follows the Cargo/SemVer convention
for that stage:

- **Patch (`0.x.Y`)** — backwards-compatible fixes only: bug fixes, doc
  improvements, performance work, new tests. No public API changes.
- **Minor (`0.X.0`)** — anything that would be a breaking change post-1.0:
  removing or renaming a public item, changing a function signature, tightening
  a precondition, or changing a trait bound on a public generic function.
  Adding a *new* public item (function, struct, field-preserving impl) is
  **not** breaking and may also ship in a minor bump.
- Once the crate reaches `1.0.0`, standard SemVer applies (breaking changes
  require a major bump).

**What counts as public API:** every item reachable from the crate root
(`kinetic_signals::*`), from a `pub mod` (e.g. `kinetic_signals::hawkes::*`),
or via [`prelude`](https://docs.rs/kinetic-signals/latest/kinetic_signals/prelude/index.html).
All three surfaces are kept in sync — see `src/lib.rs`'s `pub use` block and
the `prelude` module.

**Generic scalar types:** `compute_hurst`, `compute_surprise`,
`compute_surprise_sequence`, and `detect_anomaly` are generic over a sealed,
crate-private `Real` trait implemented only for `f32`/`f64`. This is
intentional: it lets the crate support both float widths without exposing an
implementable trait, so it is not itself part of the public API and adding
required methods to it is not a breaking change as long as `f32`/`f64` support
is preserved.

**Thread safety:** every concrete type the crate's own functions produce or
accept (results, params, and estimators such as [`VolEstimator`], including
the `f32`/`f64` instantiations of the generic `Hurst`/`Surprise` types) is
`Send + Sync`. This is enforced by a compile-time assertion in `src/lib.rs`;
removing that guarantee for an existing type would be a breaking change.
Note this covers the instantiations the crate actually uses, not every
theoretically possible instantiation of the unconstrained generic structs
(e.g. `SurpriseResult<T>`) with an arbitrary caller-supplied `T`.

See [`REVIEW.md`](REVIEW.md#breaking-changes) for the contributor-facing
checklist to follow when making a breaking change.

## Cross-language output ranges (SpikeStream.jl alignment)

To keep experimental results consistent between this crate and the Julia
`SpikeStream.jl` implementation, both projects share a single output-range
convention and a shared test-vector file at
[`tests/fixtures/shared_vectors.json`](tests/fixtures/shared_vectors.json).

| Feature      | Output      | Range            |
|--------------|-------------|------------------|
| Hurst        | `h`         | `[0, 1]`         |
| Hawkes       | `intensity` | `[mu, +inf)`     |
| Hawkes       | `avg_excitation` | `[0, +inf)` |
| Surprise     | `surprise`  | `[0, +inf)`      |
| Entropy      | `shannon`   | `[0, ln(bins)]`  |
| Entropy      | `relative`  | `[0, 1]`         |
| Volatility   | `rms`       | `[0, 1]`         |

The Rust side is verified by integration tests that load
`tests/fixtures/shared_vectors.json`:

```bash
cargo test \
  --test cross_language_ranges \
  --test hawkes_fixture_vectors \
  --test surprise_fixture_vectors \
  --test stats_fixture_vectors
```

(`cargo test` alone also runs them.) The Julia side must be validated in
`SpikeStream.jl` against the same `shared_vectors.json` within the documented
tolerance.

## Scope and ownership boundaries

This crate is **domain-agnostic**. It computes streaming signal features (Hurst, Hawkes, surprise, volatility, entropy, indicators) without assuming a specific application domain.

| Does belong | Does NOT belong here |
|-------------|---------------------|
| Generic signal statistics | Spike-train analysis (→ SpikeStream.jl) |
| Point-process intensity | SNN runtime / neuron models (→ neuromod) |
| Anomaly detection primitives | Financial domain adapters (→ DendriteTrader.jl) |

See [`docs/boundary-matrix.md`](docs/boundary-matrix.md) for the full boundary matrix.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE-2.0](LICENSE-APACHE-2.0) or <http://www.apache.org/licenses/LICENSE-2.0>)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Observability (optional Sentry)

Opt-in error monitoring via the optional `sentry` feature. Full guide: [`docs/sentry.md`](docs/sentry.md).

**Setup** — enable the feature and set a DSN:

```toml
[dependencies]
kinetic-signals = { git = "https://github.com/rmems/kinetic-signals", features = ["sentry"] }
```

```bash
export SENTRY_DSN=https://...@...
```

**Usage** — call `init_sentry()` once and keep the guard for the process lifetime (flush on drop, up to 2s):

```rust
// Requires kinetic-signals built with `features = ["sentry"]` (see docs/sentry.md).
let _guard = kinetic_signals::init_sentry();
```

- Returns `Some(ClientInitGuard)` when `SENTRY_DSN` is set and non-empty; `None` otherwise.
- Release is set via `sentry::release_name!()` → `CARGO_PKG_NAME@CARGO_PKG_VERSION` (e.g. `kinetic-signals@0.4.0`).
- Tag pushes `v*` create matching Sentry releases (`kinetic-signals@X.Y.Z`) via [sentry-release.yml](.github/workflows/sentry-release.yml).

Sentry is **never** initialized unless the feature is enabled and the DSN is present. No data is sent by default.

## Authors

Raul Montoya Cardenas
