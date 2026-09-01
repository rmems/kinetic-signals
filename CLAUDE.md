# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                  # zero deps
cargo build --all-features
cargo test                   # unit tests + doctests
cargo test --all-features
cargo test <name-substring>  # run a single test, e.g. `cargo test compute_hurst`
cargo test --test cross_language_ranges   # run one integration test file
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
cargo run --example demo
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps               # must pass under default features too
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo package --list && cargo package         # what actually ships to crates.io
```

MSRV: Rust >= 1.85 (edition 2024). `Cargo.lock` is gitignored (library crate) —
CI commands must not use `--locked`, it fails on a fresh checkout with no lockfile.

## Architecture

- Domain-agnostic streaming signal-feature library: Hurst exponent, Hawkes process,
  surprise/anomaly detection, volatility, Shannon entropy, indicators (EMA/SMA/Z-score),
  signal stats. Each feature is its own top-level module (`src/hurst.rs`, `src/hawkes.rs`,
  etc.) with a crate-root re-export and a `prelude` glob re-export.
- **Sealed generic-float trait** (`src/real.rs`, `mod real` — not `pub`): `compute_hurst`,
  `compute_surprise`, `compute_surprise_sequence`, `detect_anomaly` are generic over the
  crate-private `Real` trait, implemented only for `f32`/`f64`. This lets those functions
  support both float widths without exposing an implementable public trait — `Real` is
  never part of the public API even though it constrains public generic functions. The
  compile-time `Send + Sync` assertions in `src/lib.rs` (`_assert_send_sync`) only cover
  the concrete `f32`/`f64` instantiations, not an arbitrary caller-supplied generic `T`.
- **Three public API surfaces, one deliberate gap**: crate root (`kinetic_signals::*`),
  each `pub mod`, and `prelude` (glob re-export of the seven computation modules) are kept
  are kept in sync. Adding a new module item is usually non-breaking pre-1.0; adding
  a new trait impl or inherent method is not automatically safe (glob re-exports can create
  downstream ambiguity or ordinary API conflicts) — see the "Pre-1.0 SemVer / Stability
  Policy" section of `README.md` before changing what's exported where.
- **Cross-language parity contract**: `tests/fixtures/shared_vectors.json` is a shared
  golden-vector file also consumed by the Julia `SpikeStream.jl` project. The output-range
  conventions in `README.md`'s cross-language table (e.g. Hurst `h` in `[0, 1]`) are a
  cross-repo contract — changing one side without the other breaks parity. The
  `tests/*_fixture_vectors.rs` and `tests/cross_language_ranges.rs` integration tests verify
  the Rust side against the shared fixture.
- **Domain scope**: this crate owns generic signal statistics and point-process intensity.
  It does NOT own spike-train analysis (→ SpikeStream.jl), SNN runtime/neuron models (→
  neuromod), or financial domain adapters (→ DendriteTrader.jl / metabolic-ledger). See
  `docs/boundary-matrix.md` before adding anything that looks domain-specific.
- **CI** (`.github/workflows/ci.yml`): fmt, clippy, build+test, MSRV check, no-default-features
  build, `cargo audit`, and a packaging gate (`cargo package --list` / `cargo publish
  --dry-run` from a clean checkout — also without `--locked`, same reason as above). Separate
  workflows: `coverage.yml` (cargo-llvm-cov → Codecov), and `docker.yml`.
- Code review scope, per-bot handling conventions, and merge criteria are defined in
  `REVIEW.md`; GitHub issue/PR relationship and metadata conventions are in the "GitHub
  issue/PR hygiene" section of `AGENTS.md`.
