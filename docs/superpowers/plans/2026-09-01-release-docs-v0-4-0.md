# Release Documentation v0.4.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a curated v0.4.0 changelog and reproducible release policy, and align README/docs.rs-facing guidance with the already registry-ready crate.

**Architecture:** Keep release history in a root `CHANGELOG.md`, with the v0.4.0 entry as the single source for GitHub Release notes. Update README installation, examples, links, and development guidance to describe the pre-publication git dependency and the post-publication crates.io dependency without claiming that publication has occurred.

**Tech Stack:** Markdown, Cargo metadata, Rust doctest validation, `cargo package --list`, and `cargo publish --dry-run`.

**Spec:** GitHub issue #46 — https://github.com/rmems/kinetic-signals/issues/46

## Global Constraints

- Preserve the existing `MIT OR Apache-2.0` licensing and Rust 2024 / Rust 1.85 requirements.
- Keep zero required runtime dependencies; do not add dependencies for documentation.
- Do not claim crates.io or docs.rs availability until publication and build verification occur.
- Keep `SpikeStream.jl` parity provenance accurate and do not expand into the separate repository-consolidation work.
- Use the existing `kinetic-signals` v0.4.0 version and release workflow; do not bump the version in this issue.
- Validate README Rust examples with `cargo test --doc` and package/release content with Cargo dry-run commands.

---

### Task 1: Add the curated changelog and release policy

**Files:**
- Create: `CHANGELOG.md`
- Test: `CHANGELOG.md` content review plus `cargo package --list`

**Interfaces:**
- Produces the canonical `Unreleased` and `0.4.0` release notes that GitHub Release #47 can reuse.

- [ ] **Step 1: Write the changelog content**

Create an `Unreleased` section and a dated `0.4.0` section. The v0.4.0 entry must cover the domain-neutral API rename, Hurst/Hawkes/surprise/entropy/statistics/indicators/volatility APIs, shared SpikeStream.jl vectors, Rust/MSRV/no-default-features/audit/coverage/Docker/Qodana gates, observability ownership by consuming applications, README/rustdoc work, and the dual-license registry metadata.

Add a `Release checklist` section with explicit commands and gates for version consistency, formatting, tests, clippy, package contents, `cargo publish --dry-run`, crates.io publication, docs.rs verification, exact tag, GitHub Release, README transition, and credential safety.

- [ ] **Step 2: Verify the release notes are package-visible**

Run:

```bash
cargo package --list
```

Expected: `CHANGELOG.md` and `README.md` are included in the package, with no secret or unrelated files.

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: add v0.4.0 changelog and release policy"
```

### Task 2: Align README installation, docs, and examples

**Files:**
- Modify: `README.md`
- Test: `tests/readme_usage.rs`

**Interfaces:**
- Consumes the release policy from `CHANGELOG.md`.
- Produces unambiguous pre-release and post-publication installation instructions and examples matching the exported Rust API.

- [ ] **Step 1: Add a README usage compile test**

Add `tests/readme_usage.rs` containing the public API usage shown in README.md, then run:

```bash
cargo test --doc
```

This integration test compiles the README usage against the actual crate API; `cargo test --doc` alone does not compile README code.

- [ ] **Step 2: Update README installation and release guidance**

Explain that the git dependency is the pre-publication path and show the staged post-publication form `kinetic-signals = "0.4"` separately. Link to crates.io/docs.rs only as destinations to verify after publication, and add the changelog/release checklist link. Keep the existing API names and feature flags accurate.

- [ ] **Step 3: Run the documentation tests**

Run:

```bash
cargo test --doc
```

Expected: all README and crate rustdoc examples pass with no warnings or failures.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: align v0.4.0 installation and release guidance"
```

### Task 3: Validate the complete release-documentation deliverable

**Files:**
- Verify: `CHANGELOG.md`, `README.md`, `Cargo.toml`

**Interfaces:**
- Verifies that the changelog is package-visible, README examples compile, and the release policy is reproducible against the exact v0.4.0 manifest.

- [ ] **Step 1: Run formatting and default tests**

```bash
cargo fmt --check
cargo test
```

Expected: both commands pass.

- [ ] **Step 2: Run the publishability checks**

```bash
cargo package --list
cargo publish --dry-run
```

Expected: package listing and dry-run pass without uploading anything.

- [ ] **Step 3: Inspect the final diff**

```bash
git diff --check HEAD~2..HEAD
git status --short
```

Expected: only the intended documentation files changed, with no generated artifacts or credentials.

- [ ] **Step 4: Prepare handoff**

Report the commits, validation results, remaining post-publication gates, and that GitHub Release #47 can derive its notes directly from `CHANGELOG.md`.
