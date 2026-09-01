# kinetic-signals

A Rust library crate for streaming signal feature extraction. Computes Hurst exponent, Hawkes process intensity, surprise anomaly detection, volatility, Shannon entropy, and technical indicators on high-velocity stochastic time-series.

Part of the [rmems](https://github.com/rmems) ecosystem. See [`docs/boundary-matrix.md`](docs/boundary-matrix.md) for what this crate owns vs. neighboring crates.

## Repository map

| Path | Purpose |
|------|---------|
| `src/` | Library code (all public modules + private `real` trait) |
| `examples/demo.rs` | Runnable demo covering all major APIs |
| `tests/` | Integration tests (cross-language parity) |
| `tests/fixtures/shared_vectors.json` | Shared test vectors for SpikeStream.jl parity |
| `docs/boundary-matrix.md` | Architecture ownership and dependency boundaries |
| `REVIEW.md` | Code review guidelines and bot rules |
| `AGENTS.md` | Agent instructions (this file) |
| `.github/workflows/` | CI/CD pipelines |

## Dependencies

**Zero required runtime dependencies.** The crate is self-contained by default.

| Dependency | Type | Purpose |
|------------|------|---------|
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
cargo build --all-features
cargo test                   # Run unit tests + doctests
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
```

## Running the demo

```bash
cargo run --example demo

```

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|

## CI workflows

| Workflow | Trigger | What it does |
|----------|---------|--------------|
| `ci.yml` | push/PR to main | fmt, clippy, build, test, MSRV check, no-default-features build, cargo audit |
| `coverage.yml` | push/PR to main | cargo-llvm-cov + Codecov upload |
| `docker.yml` | push/PR to main | Containerized build + test |

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

## GitHub issue/PR hygiene

- **Relationships:** Wire native GitHub links, not just prose — sub-issue parent
  hierarchy (`sub_issue_write` / `gh issue edit --parent`), issue blocked-by/blocking
  (`gh issue edit --add-blocked-by`/`--add-blocking`; issues only, no such relationship
  exists for PRs; both need `gh` 2.94.0+), and a PR's `Closes #<n>` in its own body
  (the only native relationship a PR itself supports — GraphQL has no separate
  mutation for it; a closing keyword only creates the link when the PR targets the
  repo's default branch). A `## Relationships` section in the body is a
  human-readable summary, not a substitute for the native link.
- **Metadata:** Every issue and PR gets an assignee (default: the repo owner,
  `rmems`), labels matching the repo's existing vocabulary for that kind of change
  (see labels on comparable issues — e.g. a CI-only change uses `chore` + `CI/CD`), and
  the current open milestone (`kinetic-signals — active`). `issue_write` sets these
  for issues; PRs need `gh pr edit --add-assignee/--add-label/--milestone` since
  `create_pull_request`/`update_pull_request` have no fields for any of them.
- **Commits:** Every commit carries the standing attribution trailer
  (`Co-Authored-By: Claude ... <noreply@anthropic.com>` or the equivalent for whichever
  model authored it) — including the first commit on a new branch.
- Full detail, verification commands, and the reasoning behind each of these: the
  `github-issue-pr-hygiene` global Claude Code skill.

## Cursor Cloud specific instructions

Standard commands are documented in the **Build & test** and **Running the demo** sections above. The notes below cover the less obvious environment caveats.

- **Toolchain:** Edition 2024 needs Rust >= 1.85. The base image ships an older `rustc`. The cloud snapshot installs and defaults to a newer `stable` (via `rustup default stable`). Default builds, tests, clippy, and fmt all run under that toolchain.
- **`Cargo.lock` is gitignored** (library crate), and the lockfile is regenerated on a fresh checkout. Run `cargo fetch` to pre-warm the dependency cache.
- **Running the app:** This crate is a library; the "application" is `cargo run --example demo`, which exercises each public API and prints results to stdout (no graphical user interface).
