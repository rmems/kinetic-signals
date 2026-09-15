# Changelog

All notable changes to `kinetic-signals` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use [Cargo's SemVer conventions](https://doc.rust-lang.org/cargo/reference/semver.html).

## [Unreleased]

Crate version **0.5.0**.

### Added

- Caller-owned buffer reuse for the hot batch output paths:
  `compute_surprise_sequence_into` and `compute_shannon_entropy_into`.
  The existing allocating functions delegate to these cores and remain
  behaviorally identical. Buffer length, overwrite, capacity retention, and
  aliasing are documented on the new APIs and in the README.
  `surprise_sequence_len` is the output-length contract for surprise sequences.

### Breaking

- The prelude glob now also exports `compute_surprise_sequence_into`,
  `compute_shannon_entropy_into`, and `surprise_sequence_len`. Downstream
  `use kinetic_signals::prelude::*` combined with another glob that already
  defines those names becomes ambiguous; qualify the path or use explicit
  imports. See "Upgrading from v0.4.x" in the README.

### Removed

- Dropped the Qodana GitHub Actions workflow and `QODANA_TOKEN` / `QODANA_ENDPOINT` usage after membership expired.

## [0.4.0] - 2026-09-10

First planned public crates.io release of the reusable, domain-agnostic streaming signal feature library. This entry describes the repository state prepared for publication; crates.io and docs.rs availability must still be verified after the upload.

### Added

- Hurst exponent estimation for long-memory and persistence detection.
- Hawkes-process intensity and streaming intensity features for self-exciting point-process events.
- Normalized surprise and anomaly detection for transition magnitudes.
- Rolling RMS volatility through `VolEstimator`.
- Shannon entropy, EMA, SMA, Z-score, skewness, and kurtosis signal features.
- Shared JSON test vectors for Rust and SpikeStream.jl output-range parity.
- Public rustdoc, runnable API demo coverage, and a boundary matrix describing ownership and cross-repository handoff points.

### Changed

- Replaced financial-domain GBM naming with the domain-neutral surprise API: `compute_surprise`, `compute_surprise_sequence`, `SurpriseParams`, `SurpriseResult`, and `surprise::detect_anomaly`.
- Documented the pre-1.0 SemVer and public API stability policy.
- Prepared registry metadata, dual `MIT OR Apache-2.0` licensing, README package metadata, and docs.rs configuration for the first publication.

### Breaking changes

- Raised the MSRV from Rust 1.85.0 to Rust 1.98.1 for the Rust 2024 crate.
- Removed the GBM aliases: `compute_gbm_surprise`,
  `compute_gbm_surprise_sequence`, `GBMParams`, `GBMResult`, and
  `gbm::detect_anomaly`. Migrate to `compute_surprise`,
  `compute_surprise_sequence`, `SurpriseParams`, `SurpriseResult`, and
  `surprise::detect_anomaly`, respectively.
- Removed the pre-publication `sentry` Cargo feature and `init_sentry()` API.
  Consumers upgrading from a pre-release build that enabled `features =
  ["sentry"]` must remove that feature and initialize observability in the
  consuming application instead.

### Quality and release infrastructure

- Added minimum-toolchain validation for the Rust 2024 edition.
- Added no-default-features, formatting, clippy, unit/integration, coverage, Docker, cargo-audit, and Qodana CI gates.
- Kept the crate free of runtime dependencies; observability belongs to consuming applications.

## Release checklist

Run these checks on the exact release commit before publishing:

```bash
cargo fmt --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo build --no-default-features
cargo package --list
cargo publish --dry-run
```

Before the irreversible upload, verify that the version, changelog heading, README guidance, and intended tag agree; the package contains no credentials or local artifacts; the package metadata and links are correct; the public API and SemVer audit is complete; and all required CI checks are green.

After approval and upload:

1. Verify the crate page and metadata on [crates.io](https://crates.io/crates/kinetic-signals).
2. Verify the published version builds and renders on [docs.rs](https://docs.rs/kinetic-signals).
3. Create and push the exact `v0.4.0` tag.
4. Create the GitHub Release from this `0.4.0` entry, without diverging release notes.
5. Change README installation guidance to `kinetic-signals = "0.4"` only after the registry version is live.
6. Observability release integration is owned by consuming applications; no
   crate-side release gate remains.

Do not use `--allow-dirty`, `--no-verify`, or committed registry credentials to bypass a failed gate.

[Unreleased]: https://github.com/rmems/kinetic-signals/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/rmems/kinetic-signals/releases/tag/v0.4.0
