# kinetic-signals

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](https://opensource.org/licenses/MIT)
[![codecov](https://codecov.io/gh/rmems/kinetic-signals/branch/main/graph/badge.svg)](https://codecov.io/gh/rmems/kinetic-signals)

Streaming feature extraction for high-velocity stochastic signals.

A high-performance, domain-agnostic Rust crate for computing streaming signal statistics, point-process intensity features, and anomaly metrics on stochastic time-series.

## Features

- **Zero required runtime dependencies** - The crate is self-contained by default; an optional `serde` feature serializes snapshots. Consuming applications own observability integrations
- **Hurst Exponent** - Detects long-term memory and persistence in time-series data
- **Hawkes Process** - Models self-exciting event clusters in point-process streams
- **Surprise** - Detects anomalous transition magnitudes via normalized log-ratio z-scores
- **Volatility (RMS)** - Rolling ring-buffer volatility tracking via `VolEstimator`
- **Shannon Entropy** - Measures signal complexity and information density
- **Indicators** - Moving averages (EMA, SMA) and Z-score tracking
- **Signal Stats** - High-order moments (Skewness, Kurtosis)
- **Snapshot / restore** - Versioned checkpoints for `VolEstimator`, `EMA`, and `SMA`

## Installation

The current published release on
[crates.io](https://crates.io/crates/kinetic-signals) is `0.4.x`:

```toml
[dependencies]
kinetic-signals = "0.4"
```

The published `0.4.x` release does **not** include the snapshot/restore APIs
or the optional `serde` feature — those land in the upcoming `0.5.0`, which is
not yet published. To use them today, depend on the unreleased `main` branch:

```toml
[dependencies]
kinetic-signals = { git = "https://github.com/rmems/kinetic-signals" }
```

After `0.5.0` is published and verified on
[crates.io](https://crates.io/crates/kinetic-signals), the registry
dependency will become:

```toml
# upcoming 0.5.0 — not yet published
kinetic-signals = "0.5"
```

See the [changelog](CHANGELOG.md#release-checklist) for the release history
and reproducible publication gates. The crates.io and docs.rs destinations for
`0.5.0` are prepared in advance and remain unverified until publication
completes.

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

Snapshot / restore is new in the upcoming `0.5.0` (unreleased; use the `main`
git dependency from [Installation](#installation) until it is published) and is
not available on the published `0.4.x` release:

```rust
use kinetic_signals::VolEstimator;

let mut vol = VolEstimator::new(64);
vol.push(0.01);
vol.push(0.02);

// Snapshot / restore — checkpoint a rolling window and resume
let snap = vol.snapshot();
let mut resumed = VolEstimator::from_snapshot(&snap).expect("valid snapshot");
resumed.push(0.015);
```

### Buffer reuse (hot output paths)

High-frequency telemetry loops can keep a caller-owned `Vec` and pass it to
the `*_into` APIs instead of allocating a new output on every window.

> The `*_into` buffer-reuse APIs are new in the upcoming `0.5.0` (unreleased;
> use the `main` git dependency from [Installation](#installation) until it is
> published) and are not available on the published `0.4.x` release.

| Path | Allocating wrapper | Reuse API | Output length |
|------|--------------------|-----------|---------------|
| Surprise sequence | `compute_surprise_sequence` | `compute_surprise_sequence_into` | `surprise_sequence_len(n)` = `n.saturating_sub(1)` |
| Shannon entropy histogram | `compute_shannon_entropy` | `compute_shannon_entropy_into` | `bins` on a non-degenerate, in-policy input; `0` otherwise |

Both wrappers delegate to the `*_into` core, so results are identical.

**Overwrite / resize:** each `_into` call **resizes** the buffer to the
current window's output length (entropy **clears** on degenerate inputs, and
also on an out-of-policy `bins` greater than `MAX_ENTROPY_BINS` =
`MAX_SNAPSHOT_CAPACITY` = `1_000_000`, before any allocation), then
**overwrites** every slot. Capacity is never shrunk. A buffer whose
`capacity()` is already large enough performs **no output allocation** in
steady state.

**Aliasing:** the input slice and output `Vec` use different element types
(`f64`/`f32` vs `SurpriseResult`, or `f64` vs `usize`), so they cannot alias
in safe Rust. Inputs are read-only; only the caller buffer is written.

```rust
use kinetic_signals::{
    SurpriseParams, compute_surprise_sequence_into, surprise_sequence_len,
};

let params = SurpriseParams::default();
let window = [100.0, 100.5, 101.0, 100.8];
let mut out = Vec::with_capacity(surprise_sequence_len(window.len()));
compute_surprise_sequence_into(&window, &params, &mut out);
assert_eq!(out.len(), 3);
```

### Demo

Run the included demo:

```bash
cargo run --example demo
```

### Development

**MSRV:** Rust >= 1.98.1 (edition 2024)

The complete release validation sequence is in the
[v0.4.0 changelog](CHANGELOG.md#release-checklist); `cargo publish --dry-run`
validates the package without uploading it.

```bash
# Build and test
cargo build
cargo test --all-features

# Lint and format
cargo clippy --all-targets --all-features
cargo fmt

```

**Test coverage** (requires `cargo-llvm-cov`):

```bash
# Generate lcov report for CI
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# Open HTML report locally
cargo llvm-cov --all-features --workspace --open
```

Coverage reports are automatically generated and uploaded to [Codecov](https://codecov.io/gh/rmems/kinetic-signals) in CI via the [coverage workflow](https://github.com/rmems/kinetic-signals/blob/main/.github/workflows/coverage.yml) on every push to `main` and in pull requests. Results are also available via the badge at the top of this README.

**CI workflows:**

- [Build & Test](https://github.com/rmems/kinetic-signals/blob/main/.github/workflows/ci.yml) — fmt, clippy, build, test
- [Coverage](https://github.com/rmems/kinetic-signals/blob/main/.github/workflows/coverage.yml) — cargo-llvm-cov + Codecov upload
- [Docker](https://github.com/rmems/kinetic-signals/blob/main/.github/workflows/docker.yml) — containerized build + test

**Docker** (reproducible build):

```bash
docker build -t kinetic-signals .
docker run --rm kinetic-signals
```

### Numeric types

Most APIs use `f64`. `compute_hurst` and the surprise helpers are generic and support `f32` and `f64`. `VolEstimator` consumes `f32` absolute log-returns and computes rolling RMS volatility.

### Snapshot and restore

Stateful streaming estimators (`VolEstimator`, `EMA`, `SMA`) expose
`snapshot`, `restore`, and `from_snapshot`. Snapshots carry explicit
`schema_version` (`SNAPSHOT_SCHEMA_VERSION`, currently `1`).

#### Schema-v1 persistence and upgrades

Schema v1 is the checkpoint contract introduced in `0.5.0`. All `0.5.x`
patch releases must read valid v1 checkpoints produced by earlier `0.5.x`
releases, retain the same logical payload fields, and preserve the continuation
contract below. A later minor release that retains schema v1 must preserve
that contract too; a crate minor bump alone is not permission to reinterpret
v1. A breaking snapshot change requires a schema-version increment and a
breaking crate release under the [SemVer policy](#pre-10-semver--stability-policy).
There is no promise that every future minor release will read v1: a release
that changes the schema must document its supported versions and migration
path. Keep the producer crate version, estimator type, codec/version, and
codec options alongside long-lived checkpoints; `schema_version` alone does
not identify those application-owned details.

Increment `SNAPSHOT_SCHEMA_VERSION` when adding (even with a default), removing,
or renaming a snapshot field; changing its type or serialized representation;
changing state ordering or field meaning; or changing validation or restore
semantics so previously valid v1 state is rejected or resumes differently
beyond the continuation guarantee. Adding a separate snapshot type does not
by itself change these three payloads. Fixes that reject state already invalid
under the documented v1 rules do not require a bump. No v2 format or automatic
migration is defined here: current restore accepts only version `1`.

The frozen-payload work in [PR #78 (RM-1881)](https://github.com/rmems/kinetic-signals/pull/78)
qualifies this policy with historical payloads and continuation outputs. Keep
frozen payloads and their provenance unchanged; add cases for additional
compatibility evidence rather than regenerating historical ones. Logical
re-encoding equality is compatible with the fixed-field policy, not a
canonical-byte promise. Tolerated SMA sum drift is valid v1 state, but restore
canonicalizes it; an exact restored-snapshot comparison is appropriate only
for cases whose stored sum is already canonical.

#### Decode errors versus snapshot errors

Decoding and validation are separate steps. Optional Serde derives do **not**
call `validate`. A malformed codec payload, missing required field (including
`schema_version`), wrong field type, or integer outside the target Rust type's
range fails deserialization with the **codec's error**, not `SnapshotError`.
For example, JSON cannot decode `null` as a snapshot float or `-1` as a
`usize` capacity. Codec and target-width constraints still apply even to an
otherwise valid logical snapshot. With today's derives, self-describing
formats such as JSON ignore unknown extra fields; this is not a supported
schema-extension mechanism or a permanent decoder guarantee. Write only the
defined v1 fields and store application metadata separately.

After successful decoding, `validate`, `from_snapshot`, and `restore` reject:

| Decoded state | `SnapshotError` |
|---------------|-----------------|
| Any schema version other than `1` (older or newer) | `IncompatibleVersion { found, expected }`; no fallback or migration |
| Unsupported declared capacity | `InvalidCapacity` |
| Volatility ring length unequal to capacity, or SMA window longer than capacity | `InvalidLength` |
| Non-finite samples/scalars that participate in restored state | `NonFinite` |
| Out-of-range ring write index, EMA alpha outside `(0, 2]`, or inconsistent SMA sum | `InconsistentState` |

`from_snapshot` and `restore` can additionally return `AllocationFailed` if
the validated estimator buffer cannot be reserved; `validate` does not allocate
an estimator. All restore failures leave the existing destination unchanged.
For state with several faults, do not depend on which fault is reported first.
An uninitialized EMA's `value` is ignored (even if non-finite) and reset to
`0.0`; its `alpha` must always be finite. An empty SMA requires an exactly zero
sum. A nonempty SMA sum must agree with the Welford-derived window total within
the documented magnitude-scaled, capped-length validation tolerance; this is
not the same bound as the absolute output-continuation tolerance.

#### State ordering, capacity, and continuation

| Snapshot | Ordering and restored state | Supported capacity |
|----------|-----------------------------|--------------------|
| `VolEstimatorSnapshot` | `samples` is the entire physical ring, **not** oldest-first; preserve `pos` (next write index) and `full`. Before wrapping, only slots before `pos` contribute to RMS; all slots must be finite. RMS squares and accumulates in `f64`, returning `f32`. | `1..=MAX_SNAPSHOT_CAPACITY` |
| `EMASnapshot` | Preserve `alpha`, `initialized`, and the initialized `value`; no sample container. An uninitialized EMA resumes by seeding from its next finite input. | Not applicable |
| `SMASnapshot` | `window` is **oldest-first**; preserve declared capacity, not just occupied length. Restore recomputes the canonical Welford-derived `sum`, so an accepted drifted sum need not round-trip bit-for-bit. | `0..=MAX_SNAPSHOT_CAPACITY` |

`MAX_SNAPSHOT_CAPACITY` is `1_000_000` (about 4 MiB of `f32` or 8 MiB of
`f64` samples). As resolved in [issue #64](https://github.com/rmems/kinetic-signals/issues/64),
construction and restore share these capacity ranges: constructors panic
outside them, while restore returns `InvalidCapacity` before reserving the
estimator buffer. SMA capacity `0` is a valid no-op (`update` returns `0.0`);
successful restore reserves at least the declared capacity, without promising
an allocator's exact `Vec::capacity()`. Neither windows nor capacities are
silently clamped or truncated.

For valid state produced through the supported estimator APIs and a codec
round-trip that preserves its numerical values, processing A, snapshotting,
restoring, then processing B matches uninterrupted A+B outputs within the
absolute `RESTORE_OUTPUT_TOLERANCE` (`1e-6`). On the same build/target with
identical inputs, the preserved RMS summation order and EMA/SMA arithmetic
also permit exact continuation; portable persistence promises the tolerance,
not bit-identical results across crate/compiler/target or codec changes.
This is a checkpoint-continuation guarantee, not an accuracy bound against
ideal real arithmetic. It does not cover lossy codecs, direct mutation of
public EMA/SMA fields, or EMA arithmetic overflow that makes live state
non-finite. Non-finite `push`/`update` inputs are ignored, but that alone does
not guarantee that every caller-modified or overflowed state can be restored.

The optional `serde` feature derives `Serialize` / `Deserialize` on snapshot
types. Snapshots and the `serde` feature ship in the upcoming `0.5.0`; until it
is published, depend on the unreleased `main` branch. Pair the feature with a
format crate such as `serde_json` in the consuming application:

```toml
# unreleased main (snapshot + serde APIs)
kinetic-signals = { git = "https://github.com/rmems/kinetic-signals", features = ["serde"] }
serde_json = "1"

# once 0.5.0 is published and verified on crates.io:
# kinetic-signals = { version = "0.5", features = ["serde"] }
```

Serde support is **format-agnostic**: the consuming application selects and
configures the codec. There is no permanent canonical JSON or other codec byte
representation; whitespace, map order, float formatting, and codec versions
are not pinned. Persist numerical values losslessly if continuation matters.

**Untrusted checkpoints:** `MAX_SNAPSHOT_CAPACITY` bounds estimator allocation
at restore time, not decoding. A decoder can allocate a large `samples` or
`window` container before validation sees its length or declared capacity.
Apply application-specific transport/input byte limits **before decoding**,
plus codec container/depth limits or a bounded decoder when required. A byte
limit is not itself a guarantee of bounded decoded memory for every codec.
For example, this application accepts only small JSON checkpoints (64 KiB,
not enough for every supported window):

```rust
use kinetic_signals::{SMA, SMASnapshot};

fn decode_small_sma(bytes: &[u8]) -> Result<SMA, Box<dyn std::error::Error>> {
    // Also cap the transport/file read before it allocates this byte slice.
    if bytes.len() > 64 * 1024 {
        return Err("checkpoint exceeds this application's byte limit".into());
    }
    // This step can allocate window, and can fail with serde_json::Error.
    let snapshot: SMASnapshot = serde_json::from_slice(bytes)?;
    // This step validates first, then reserves the estimator's declared capacity.
    Ok(SMA::from_snapshot(&snapshot)?)
}
```

### Numeric input contract

Public numerical APIs expect **finite** inputs. Non-finite values (`NaN`, `±Inf`) and other ill-conditioned cases produce a documented finite sentinel rather than an accidental `NaN`. Intentionally undefined results (empty history, constant R/S, non-positive surprise samples, near-zero variance) use the same sentinels and are covered by tests.

| API | Invalid / ill-conditioned input | Chosen behavior |
|-----|---------------------------------|-----------------|
| `compute_hurst` | `len < 32`, any non-finite sample, constant / near-constant windows, underdetermined log-log fit | `h = 0.5`, both persistence flags `false` |
| `compute_hawkes` | empty history, any non-finite time or parameter, `mu < 0` / `alpha < 0` / `beta < 0`, overflowed excitation sum | `event_count = 0`, `intensity = μ` (or `0` if `μ` is non-finite), `avg_excitation = 0` |
| `compute_hawkes` / `compute_hawkes_streaming` | `beta == 0` (including `-0.0`) | **valid** non-decaying limit: decay factor is exactly `1.0`, excitation does not decay (holds even for an overflowing finite gap) |
| `compute_hawkes` | negative inter-event gap (non-monotonic times) | empty-history sentinel (`event_count = 0`) |
| `compute_hawkes_streaming` | backwards tick (`new_event_time < last_event_time`) | ignore the tick; keep `decay_sum`; intensity is `μ + α · decay_sum` |
| `compute_hawkes_streaming` | non-finite time, `decay_sum`, or parameters | `(μ, decay_sum)` with non-finite fields replaced by `0`; finite decay history is preserved |
| `compute_hawkes` + streaming | same finite monotone history, stream started at decay `0` | post-event `μ + α · decay_sum` matches batch intensity (pre-jump streaming return is `μ + α · decayed_sum`) |
| `compute_surprise` / `compute_surprise_sequence` | non-finite or non-positive sample, non-finite params, `dt < 0` | zeroed result (`surprise = z_score = log_return = 0`); sequence length still `values.len() - 1` |
| `compute_surprise` | `sigma ≤ 0` with valid positive samples | `z_score = surprise = 0`; `log_return` still reported |
| `detect_anomaly` | non-finite surprise or threshold | `false` (not an anomaly) |
| `compute_shannon_entropy` | `len < 2`, `bins == 0`, any non-finite sample, overflowed `max - min` range | zeroed result (`bin_count = 0`) |
| `compute_shannon_entropy` / `compute_shannon_entropy_into` | `bins > MAX_ENTROPY_BINS` (`= MAX_SNAPSHOT_CAPACITY = 1_000_000`, ≈ 8 MiB of `usize`) | out of policy: zeroed result (`bin_count = 0`) before any allocation; `*_into` buffer is cleared (`len == 0`, capacity retained); resolution is not silently reduced |
| `compute_shannon_entropy` | constant series (`max == min`) with `bins` within policy | `shannon = 0`, `bin_count = 1` (an oversized `bins` is rejected first with `bin_count = 0`, per the row above) |
| `compute_signal_stats` | empty slice, any non-finite sample, or overflowing second moment | all zeros, `count = 0` |
| `compute_signal_stats` | constant / near-zero variance | `skewness = kurtosis = 0` |
| `VolEstimator::push` / `rms` | non-finite push; empty window | push ignored; empty `rms = 0`; output clamped to `[0, 1]` |
| `EMA::update` / `SMA::update` | non-finite sample | state unchanged; current value returned (`0` if uninitialized / empty) |
| `SMA::new` | `capacity == 0` | construction succeeds; `update` is a no-op returning `0.0` |
| `SMA::new` | `capacity > MAX_SNAPSHOT_CAPACITY` (`1_000_000`, ≈ 8 MiB of `f64`) | panic (`capacity must be <= MAX_SNAPSHOT_CAPACITY`); intentional 0.5.0 break vs unbounded 0.4.x constructor |
| `VolEstimator::new` | `capacity == 0` | panic (`capacity must be > 0`) |
| `VolEstimator::new` | `capacity > MAX_SNAPSHOT_CAPACITY` (`1_000_000`, ≈ 4 MiB of `f32`) | panic (`capacity must be <= MAX_SNAPSHOT_CAPACITY`); intentional 0.5.0 break vs unbounded 0.4.x constructors |
| `ZScore::compute` | non-finite argument, `std_dev ≤ 1e-12`, or overflowing quotient | `0.0` |

Shared-vector goldens are unchanged: the sentinels apply only to invalid or degenerate inputs, not to the finite fixture histories.

## Performance

Built with aggressive optimizations for real-time inference:

Typical execution times (Ryzen 9 9950X):
- Hurst (100 samples): ~50μs
- Hawkes (10 events): ~5μs
- Surprise: ~100ns

## Upgrading from v0.4.x

v0.5.0 adds buffer-reuse APIs and snapshot/restore APIs. Existing
allocating functions, `push` / `update`, and batch functions are unchanged,
with one tightened input range: `compute_shannon_entropy` and
`compute_shannon_entropy_into` now reject a `bins` greater than
`MAX_ENTROPY_BINS` (`= MAX_SNAPSHOT_CAPACITY = 1_000_000`) with the zeroed
sentinel (`bin_count == 0`) before any histogram allocation, instead of
attempting an unbounded reservation on non-degenerate input. A caller that
previously passed a value such as `bins == 1_000_001` and received a computed
entropy now gets the rejected-request sentinel; treat `bin_count == 0` as a
rejected resolution, not a real zero-entropy result. The rejection runs before
the constant-series branch, so an oversized `bins` yields `bin_count == 0` for
every input distribution (a constant series only yields `bin_count == 1` when
`bins` is within policy). Behavior for valid `bins` (`1..=MAX_ENTROPY_BINS`) is
unchanged. This mirrors the `SMA::new` / `VolEstimator::new` allocation ceiling
below and is an intentional 0.5.0 hardening; callers that need a larger
histogram must stay on 0.4.x or reduce the resolution.

`VolEstimator::new` now panics if `capacity` exceeds `MAX_SNAPSHOT_CAPACITY`
(`1_000_000`, about 4 MiB of `f32` samples). Windows larger than that
compiled and ran on 0.4.x; the ceiling is an intentional 0.5.0 breaking
change so construction matches snapshot restore. `SMA::new` now panics if
`capacity` exceeds `MAX_SNAPSHOT_CAPACITY` (`1_000_000`, about 8 MiB of `f64`
samples), matching VolEstimator and SMA snapshot restore; capacity `0` is
still valid (a no-op estimator). Callers that need a larger window must stay
on 0.4.x or split the window into multiple smaller SMAs.

The new names are also exported by `prelude`:

| New in v0.5.0 | Role |
|---------------|------|
| `compute_surprise_sequence_into` | Reuse a caller `Vec<SurpriseResult>` |
| `compute_shannon_entropy_into` | Reuse a caller histogram `Vec<usize>` |
| `surprise_sequence_len` | Output length: `n.saturating_sub(1)` |
| `MAX_ENTROPY_BINS` | Supported histogram `bins` ceiling for the entropy entry points (`= MAX_SNAPSHOT_CAPACITY = 1_000_000`) |
| `VolEstimatorSnapshot`, `EMASnapshot`, `SMASnapshot` | Versioned estimator snapshot structs |
| `SNAPSHOT_SCHEMA_VERSION`, `RESTORE_OUTPUT_TOLERANCE` | Snapshot schema constants |
| `SnapshotError` | Typed validation error on snapshot restore |

If `use kinetic_signals::prelude::*;` is combined with another glob import
that already defines one of those names, the compiler will report an
ambiguous glob re-export. Replace the colliding glob with an explicit import,
or qualify the kinetic-signals item (`kinetic_signals::compute_surprise_sequence_into`).

Enable serde traits on snapshot types with `features = ["serde"]` (add a
format crate such as `serde_json` separately).

## Upgrading from v0.3.x

v0.4.0 removes the deprecated GBM aliases. Replace with the domain-agnostic names:

| Removed (v0.3.x)                 | Use instead                |
|----------------------------------|----------------------------|
| `compute_gbm_surprise`           | `compute_surprise`         |
| `compute_gbm_surprise_sequence`  | `compute_surprise_sequence`|
| `GBMParams`                      | `SurpriseParams`           |
| `GBMResult`                      | `SurpriseResult`           |
| `gbm::detect_anomaly`            | `surprise::detect_anomaly` |

Also remove `features = ["sentry"]` from the dependency declaration and
delete any calls to `init_sentry()`. Observability setup now belongs in the
consuming application rather than in this crate; see the migration notes in
[`CHANGELOG.md`](CHANGELOG.md#040---2026-09-10).

## Pre-1.0 SemVer / Stability Policy

`kinetic-signals` is pre-1.0 (`0.x.y`) and follows the Cargo/SemVer convention
for that stage:

- **Patch (`0.x.Y`)** — backwards-compatible fixes only: bug fixes, doc
  improvements, performance work, new tests. No public API changes.
- **Minor (`0.X.0`)** — anything that would be a breaking change post-1.0:
  removing or renaming a public item, changing a function signature, tightening
  a precondition, or changing a trait bound on a public generic function.
  Adding a *new* public function or struct is usually **not** breaking and may
  also ship in a minor bump. Four additions are the exception, needing the
  same compatibility review as a breaking change rather than an automatic
  minor bump: an inherent method or a trait impl (can shadow a downstream
  trait method or make method resolution/type inference ambiguous); a
  **field on an existing public struct** (none of this crate's public structs
  are `#[non_exhaustive]`, so a new field breaks a downstream full struct
  literal, e.g. `HawkesParams { mu: 1.0, alpha: 0.5, beta: 1.0, dt: 0.01 }`,
  or an exhaustive destructure without `..`, e.g. `let HawkesParams { mu,
  alpha, beta, dt } = p;` — a destructure or literal that already uses `..`
  is unaffected, since `..` matches/fills any remaining fields); and **any new
  item added to a module re-exported by `prelude`** (the prelude re-exports
  via glob, `pub use ...::*`, so a new name can collide with a downstream
  glob import that combines `kinetic_signals::prelude::*` with another
  glob exporting the same name — see the `prelude` module's own doc comment
  in `src/lib.rs`).
- Once the crate reaches `1.0.0`, standard SemVer applies (breaking changes
  require a major bump).

**What counts as public API:** every item reachable from the crate root
(`kinetic_signals::*`), from a `pub mod` (e.g. `kinetic_signals::hawkes::*`),
or via [`prelude`](https://docs.rs/kinetic-signals/latest/kinetic_signals/prelude/index.html);
the Cargo feature names in `[features]` (optional `serde` gates
`Serialize`/`Deserialize` on snapshot types; the default build stays
zero-dependency), and every
**existing** trait implementation on a public type (e.g. `Default` for
`HawkesParams`, `Clone` for the result structs) — removing one breaks
downstream code that relies on it, the same as removing a function. The crate
root and `pub mod` surfaces are kept in sync with
the `prelude`.

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

See [`REVIEW.md`](https://github.com/rmems/kinetic-signals/blob/main/REVIEW.md#breaking-changes) for the contributor-facing
checklist to follow when making a breaking change.

## Cross-language output ranges (SpikeStream.jl alignment)

To keep experimental results consistent between this crate and the Julia
`SpikeStream.jl` implementation, both projects share a single output-range
convention and a shared test-vector file at
[`tests/fixtures/shared_vectors.json`](https://github.com/rmems/kinetic-signals/blob/main/tests/fixtures/shared_vectors.json).

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
[`tests/fixtures/shared_vectors.json`](https://github.com/rmems/kinetic-signals/blob/main/tests/fixtures/shared_vectors.json):

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

See [`docs/boundary-matrix.md`](https://github.com/rmems/kinetic-signals/blob/main/docs/boundary-matrix.md) for the full boundary matrix.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE-2.0](LICENSE-APACHE-2.0) or <http://www.apache.org/licenses/LICENSE-2.0>)

- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Observability

Observability integrations are intentionally owned by consuming applications.
`kinetic-signals` performs signal feature extraction and does not initialize
telemetry clients or send network events.

## Authors

Raul Montoya Cardenas
