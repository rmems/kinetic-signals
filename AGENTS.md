# kinetic-signals

A Rust library crate for streaming signal feature extraction. Computes Hurst exponent, Hawkes process intensity, surprise anomaly detection, volatility, Shannon entropy, and technical indicators on high-velocity stochastic time-series.

Part of the [rmems](https://github.com/rmems) ecosystem. See [`docs/boundary-matrix.md`](docs/boundary-matrix.md) for what this crate owns vs. neighboring crates.

## Repository map

| Path | Purpose |
|------|---------|
| `src/` | Library code (all public modules + private `real` trait) |
| `examples/demo.rs` | Runnable demo covering all major APIs |
| `tests/` | Integration tests (cross-language parity, sentry feature) |
| `tests/fixtures/shared_vectors.json` | Shared test vectors for SpikeStream.jl parity |
| `docs/boundary-matrix.md` | Architecture ownership and dependency boundaries |
| `docs/sentry.md` | Optional Sentry SDK integration guide |
| `REVIEW.md` | Code review guidelines and bot rules |
| `AGENTS.md` | Agent instructions (this file) |
| `.github/workflows/` | CI/CD pipelines |

## Dependencies

**Zero required runtime dependencies.** The crate is self-contained by default.

| Dependency | Type | Purpose |
|------------|------|---------|
| `sentry` 0.48.2 | optional | Error monitoring (feature-gated) |
| `serial_test` 3.0 | dev | Serial test execution for env var tests |
| `temp-env` 0.3.6 | dev | Safe environment variable manipulation |
| `serde_json` 1 | dev | Deserialize shared golden fixtures in tests |

## Toolchain

- **Edition:** 2024 (requires Rust >= 1.85)
- **MSRV:** 1.85.0 (verified in CI)
- **No system dependencies** required for the library itself

## Build & test

```bash
cargo build                  # Build (zero deps)
cargo build --all-features   # Build with sentry
cargo test                   # Run unit tests + doctests
cargo test --all-features    # Include sentry feature tests
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## Running the demo

```bash
cargo run --example demo

# With Sentry error reporting:
SENTRY_DSN=https://...@... cargo run --example demo --features sentry
```

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `sentry` | off | Enables `init_sentry()` and pulls in `sentry` crate |

## CI workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `ci.yml` | push/PR to main | fmt, clippy, build, test, MSRV check, no-default-features build, cargo audit |
| `coverage.yml` | push/PR to main | cargo-llvm-cov + Codecov upload |
| `docker.yml` | push/PR to main | Containerized build + test |
| `sentry-release.yml` | tag push `v*` | Creates Sentry release |

## Code style

- **Formatting:** `cargo fmt` (rustfmt)
- **Linting:** `cargo clippy --all-targets --all-features -- -D warnings`
- **Comments:** No comments unless the reason is non-obvious. Never explain what the code does.
- **Headers:** All source files include a license identifier header (see the license files in the repo root)
- **Unsafe:** Avoid. Edition 2024 marks `env::set_var`/`env::remove_var` as unsafe — use `temp-env` crate in tests.

## Testing

- **Unit tests:** Inline `#[cfg(test)]` modules in `src/` files
- **Integration tests:** `tests/` directory
- **Cross-language parity:** `tests/fixtures/shared_vectors.json` shared with SpikeStream.jl
- **Thread-safety:** Compile-time `Send + Sync` assertions in `src/lib.rs`

## PR instructions

- **Naming:** Conventional commits — `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`
- **Scope:** One issue per PR; multi-issue PRs require justification in the PR description
- **Breaking changes:** Bump version for removed/renamed public items (see REVIEW.md for semver rules)
- **Required:** All CI checks must pass and zero unresolved review threads before merge. Exceptions: docs-only PRs may skip coverage checks; maintainer approval required for any override.

## Cursor Cloud specific instructions

Standard commands are documented in the **Build & test** and **Running the demo** sections above. The notes below cover the less obvious environment caveats.

- **Toolchain:** Edition 2024 needs Rust >= 1.85. The base image ships an older `rustc`. The cloud snapshot installs and defaults to a newer `stable` (via `rustup default stable`). Default builds, tests, clippy, and fmt all run under that toolchain.
- **`--all-features` / `sentry` needs system OpenSSL:** The `sentry` feature pulls in `openssl-sys`, which requires `libssl-dev` and `pkg-config`. These packages are already present in the snapshot.
- **Without them:** `cargo build --all-features` and `cargo test --all-features` fail with a "Could not find directory of OpenSSL installation" error. Default builds and tests do not require them.
- **`Cargo.lock` is gitignored** (library crate), and the lockfile is regenerated on a fresh checkout. Run `cargo fetch` to pre-warm the dependency cache.
- **Running the app:** This crate is a library; the "application" is `cargo run --example demo`, which exercises each public API and prints results to stdout (no graphical user interface).
