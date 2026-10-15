# Changelog

All notable changes to `kinetic-signals` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use [Cargo's SemVer conventions](https://doc.rust-lang.org/cargo/reference/semver.html).

## [Unreleased]

Crate version **0.5.0**.

### Fixed

- Both entropy entry points now define an explicit supported `bins` ceiling,
  `MAX_ENTROPY_BINS` (`= MAX_SNAPSHOT_CAPACITY = 1_000_000`, about 8 MiB of
  `usize` counts, matching the `SMA` / `VolEstimator` allocation rationale).
  A request above the ceiling is rejected before any resize with the existing
  zeroed sentinel (`bin_count == 0`), and `compute_shannon_entropy_into`
  clears the caller buffer (`len == 0`, capacity retained), the same as the
  other degenerate entropy branches; previously a caller-controlled `bins`
  such as `usize::MAX` on non-degenerate input attempted an unbounded
  histogram allocation with no policy. The requested resolution is never
  silently reduced, and all behavior for valid `bins` (`1..=MAX_ENTROPY_BINS`),
  including buffer reuse and every finite / non-finite / overflow / constant
  sentinel, is unchanged. The allocating wrapper delegates to the core, so
  both entry points share the policy.
- The Hawkes zero-decay contract is now explicit and consistent across
  batch, streaming, docs, and tests. `beta >= 0` is the accepted contract;
  `beta == 0` (including `-0.0`) is the non-decaying limit whose decay factor
  is exactly `1.0`, so excitation accumulates without decaying. Both
  `compute_hawkes` and `compute_hawkes_streaming` now route the decay factor
  through one shared helper, so `beta == 0` yields a decay factor of `1.0`
  even for a very large / overflowing finite gap (previously `0 * inf`
  produced `NaN`, which the batch API turned into the empty sentinel and the
  streaming API skipped). Behavior for any positive `beta` is unchanged. The
  `HawkesParams::beta` doc now matches validation, the README numeric-input
  contract documents `beta == 0` / `-0.0` as valid, and a focused
  `tests/hawkes_zero_decay.rs` covers it. No public precondition was
  tightened, so this is not a breaking change and needs no SemVer bump.
- Snapshot restore now enforces the same `1..=MAX_SNAPSHOT_CAPACITY` bound as
  `VolEstimator::new` for `VolEstimatorSnapshot`, enforces the SMA
  `0..=MAX_SNAPSHOT_CAPACITY` range shared by `SMA::new` and restore by
  rejecting a declared `capacity` above `MAX_SNAPSHOT_CAPACITY` before
  reserving the window,
  rejects EMA smoothing factors outside `(0, 2]` atomically, and reserves an
  SMA snapshot's declared capacity before restoring its occupied window.
  `SMASnapshot::validate` compares the stored sum to the Welford-derived
  total with a magnitude-scaled tolerance whose window-length factor is
  capped, and requires an exact zero sum when the window is empty.
- Public numerical APIs now treat non-finite inputs (`NaN`, `±Inf`) and ill-conditioned windows (constant / near-constant, near-zero variance, non-monotonic Hawkes times) as documented finite sentinels instead of propagating accidental `NaN` or `Inf`.
- `compute_hurst` no longer reports `h = 0` (antipersistent) for a NaN series: `f64::max` was swallowing NaN during the `[0, 1]` clamp. Constant and non-finite series now use the existing underdetermined sentinel `h = 0.5`.
- `compute_signal_stats` accumulates the mean with Welford's method; `SMA` recomputes the window mean with Welford after each accepted sample; `VolEstimator::rms` squares in `f64` before clamping.
- Overflow of finite extreme magnitudes (second moments, Hawkes excitation sums, surprise `mu * dt`, z-score quotients, Hurst R/S) now yields the same documented empty/underdetermined sentinels instead of `Inf` results with a normal-looking count or a clamped false Hurst.
- `SMA::new(0)` remains constructible (a no-op `update`), so zero capacity is not the breaking constructor panic; only a `capacity` above `MAX_SNAPSHOT_CAPACITY` now panics (see the Breaking section). Hawkes streaming preserves finite decay state when a tick or parameter is invalid, and batch/streaming both reject non-monotonic event times instead of disagreeing.

### Added

- Versioned `snapshot` / `restore` / `from_snapshot` for `VolEstimator`, `EMA`, and `SMA`, with `SnapshotError` validation (schema version, capacity, length, layout, non-finite values, and unallocatable buffers) that does not mutate the destination on failure. `SMASnapshot::sum` is checked against the same Welford-derived window total `SMA::update` stores; `SMA::new(0)` round-trips; SMA window restore uses fallible reservation.
- Optional `serde` feature for snapshot `Serialize` / `Deserialize`. Default and `--no-default-features` builds stay zero-dependency.
- Replay fixture `tests/fixtures/snapshot_replay.json` covering a wrapped, non-empty `VolEstimator` window plus SMA/EMA A+B vs snapshot/restore.
- Caller-owned buffer reuse for the hot batch output paths:
  `compute_surprise_sequence_into` and `compute_shannon_entropy_into`.
  The existing allocating functions delegate to these cores and remain
  behaviorally identical. Buffer length, overwrite, capacity retention, and
  aliasing are documented on the new APIs and in the README.
  `surprise_sequence_len` is the output-length contract for surprise sequences.
- `MAX_ENTROPY_BINS` (re-exported from the crate root): the supported ceiling
  for histogram `bins` on the entropy entry points, aliased to
  `MAX_SNAPSHOT_CAPACITY` (`1_000_000`). This is an additive, non-breaking
  public item; no existing behavior for valid `bins` changed.
- Crate- and per-API documentation of the finite-input / finite-output contract, plus regression and property tests for constant, near-constant, extreme finite, NaN, and `±Inf` cases. Batch Hawkes intensity is asserted to match the streaming post-event form `μ + α · decay_sum`.

### Breaking

- Bumped the crate version to `0.5.0` for new inherent methods (`snapshot`, `restore`, `from_snapshot`) and prelude-exported snapshot types / `serde` feature, per the pre-1.0 SemVer policy. Estimator math and existing method signatures are unchanged aside from the `VolEstimator::new` capacity ceiling below.
- `VolEstimator::new` now panics when `capacity > MAX_SNAPSHOT_CAPACITY`
  (`1_000_000`, about 4 MiB of `f32` samples). Previously the constructor
  accepted any positive window. This tighter public precondition is an
  intentional 0.5.0 breaking change so live construction and untrusted
  restore share one allocation ceiling. Callers that need a larger ring
  must stay on 0.4.x or split the window.
- `SMA::new` now panics when `capacity > MAX_SNAPSHOT_CAPACITY`
  (`1_000_000`, about 8 MiB of `f64` samples). Previously the constructor
  accepted any capacity. This tighter public precondition is an intentional
  0.5.0 breaking change so live construction and SMA snapshot restore share
  one allocation ceiling. `capacity == 0` remains supported (a no-op
  estimator whose `update` returns `0.0`). Callers that need a larger window
  must stay on 0.4.x or split the window into multiple smaller SMAs.
- `compute_shannon_entropy` and `compute_shannon_entropy_into` now reject a
  `bins` above `MAX_ENTROPY_BINS` (`1_000_000`) with the zeroed sentinel
  (`bin_count == 0`) instead of attempting an unbounded histogram allocation
  on non-degenerate input. This tightens the accepted input range of two
  existing public functions: a caller that previously relied on
  `bins > 1_000_000` (for example `bins == 1_000_001`) now receives the
  rejected-request sentinel rather than a computed entropy, and must not
  mistake it for a real zero-entropy result. The rejection is applied before
  the constant-series branch, so an oversized `bins` returns `bin_count == 0`
  for every input distribution. All behavior for valid `bins`
  (`1..=MAX_ENTROPY_BINS`) is unchanged. Callers that need a larger histogram
  must stay on 0.4.x or reduce the resolution.
- The prelude glob now also exports `compute_surprise_sequence_into`,
  `compute_shannon_entropy_into`, `surprise_sequence_len`, and
  `MAX_ENTROPY_BINS`. Downstream
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
3. Create and push the exact `v0.5.0` tag.
4. Create the GitHub Release from the `[Unreleased]` `0.5.0` entry, without diverging release notes.
5. Change README installation guidance to `kinetic-signals = "0.5"` only after the registry version is live.
6. Observability release integration is owned by consuming applications; no
   crate-side release gate remains.

Do not use `--allow-dirty`, `--no-verify`, or committed registry credentials to bypass a failed gate.

[Unreleased]: https://github.com/rmems/kinetic-signals/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/rmems/kinetic-signals/releases/tag/v0.4.0
