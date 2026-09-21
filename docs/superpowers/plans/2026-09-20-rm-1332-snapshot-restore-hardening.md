# RM-1332 Snapshot Restore Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every supported `VolEstimator`, `EMA`, and `SMA` snapshot restore safely and preserve the constructor-visible state contract required for `kinetic-signals` 0.5.0.

**Architecture:** Keep the existing versioned snapshot schema and fallible restore design. Remove the inconsistent Vol snapshot capacity ceiling, restore SMA storage with the declared capacity rather than only the occupied length, and bound EMA snapshot alpha to the constructor-produced range. The integration tests in `tests/snapshot.rs` remain the public behavior boundary, with targeted unit validation tests in `src/snapshot.rs` where appropriate.

**Tech Stack:** Rust 2024, standard library allocation APIs, Cargo tests, optional serde feature.

**Spec:** Linear RM-1332, https://linear.app/rpd-34/issue/RM-1332/featstate-add-deterministic-snapshot-and-restore-for-kinetic-signals

## Global Constraints

- Rust edition is 2024 and MSRV is 1.98.1.
- The crate has zero required runtime dependencies; serde remains optional.
- All production source files retain the SPDX license header.
- Use test-driven development: each regression must fail against `main` before its production fix is written.
- Work in the current clean checkout; no cloud delegate or worktree is used for RM-1332.
- The eventual PR is one focused issue, references RM-1332, and has a `Co-Authored-By: Codex <noreply@openai.com>` trailer.

---

### Task 1: Make large valid VolEstimator snapshots round-trip

**Files:**
- Modify: `tests/snapshot.rs:207-219`
- Modify: `src/snapshot.rs:146-175`
- Modify: `src/lib.rs:69-73`

**Interfaces:**
- Consumes: `VolEstimator::new`, `VolEstimator::snapshot`, and `VolEstimator::from_snapshot`.
- Produces: A snapshot validator that accepts all positive capacities represented by a valid in-memory estimator and maps impossible restore allocation to `SnapshotError::AllocationFailed`.

- [ ] **Step 1: Write the failing integration test**

```rust
#[test]
fn snapshot_vol_large_valid_capacity_round_trips() {
    let mut vol = VolEstimator::new(1_000_001);
    vol.push(0.25);
    let restored = VolEstimator::from_snapshot(&vol.snapshot()).unwrap();
    assert_eq!(restored.len(), 1);
    assert_eq!(restored.rms(), 0.25);
}
```

- [ ] **Step 2: Verify the test fails for the current capacity ceiling**

Run: `cargo test --test snapshot snapshot_vol_large_valid_capacity_round_trips`

Expected: FAIL because `VolEstimator::from_snapshot` returns `SnapshotError::InvalidCapacity`.

- [ ] **Step 3: Write the minimal implementation**

Retain `MAX_SNAPSHOT_CAPACITY` as the public upper bound and enforce it in both `VolEstimator::new` and snapshot validation. Keep zero-capacity, layout, position, finiteness, and fallible allocation validation intact.

- [ ] **Step 4: Verify the targeted tests pass**

Run each targeted test separately: `cargo test --test snapshot snapshot_vol_large_valid_capacity_round_trips` and `cargo test --test snapshot snapshot_vol_rejects_capacity_above_supported_limit_without_mutating`.

Expected: PASS; the maximum supported snapshot round-trips, while an over-limit payload is rejected without mutating the estimator.

- [ ] **Step 5: Commit**

```bash
git add src/snapshot.rs src/lib.rs tests/snapshot.rs
git commit -m "fix(snapshot): align Vol restore capacity with constructor"
```

### Task 2: Reject unsupported EMA alpha atomically

**Files:**
- Modify: `tests/snapshot.rs:270-281`
- Modify: `src/snapshot.rs:192-207`
- Test: `src/snapshot.rs:297-318`

**Interfaces:**
- Consumes: `EMASnapshot::validate` and `EMA::restore`.
- Produces: Validation for `0 < alpha <= 2`, preserving `EMA::new(0)` compatibility and keeping restore atomic.

- [ ] **Step 1: Write the failing integration test**

```rust
#[test]
fn snapshot_restore_rejects_unsupported_ema_alpha_without_mutating() {
    let mut ema = EMA::new(4);
    ema.update(10.0);
    let before = ema.clone();
    let invalid = EMASnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        value: 1.0,
        alpha: 3.0,
        initialized: true,
    };
    assert_eq!(ema.restore(&invalid), Err(SnapshotError::InconsistentState));
    assert_eq!(ema.value, before.value);
    assert_eq!(ema.alpha, before.alpha);
    assert_eq!(ema.initialized, before.initialized);
}
```

- [ ] **Step 2: Verify the test fails because alpha 3.0 is accepted**

Run: `cargo test --test snapshot snapshot_restore_rejects_unsupported_ema_alpha_without_mutating`

Expected: FAIL because restore returns `Ok(())`.

- [ ] **Step 3: Write the minimal implementation**

Change the snapshot validator from `alpha <= 0.0` to `!(0.0 < alpha && alpha <= 2.0)` and update rustdoc to state the complete accepted range.

- [ ] **Step 4: Verify the targeted tests pass**

Run: `cargo test --test snapshot snapshot_restore_rejects_unsupported_ema_alpha_without_mutating`

Expected: PASS; the existing `alpha == 2.0` unit test remains green.

- [ ] **Step 5: Commit**

```bash
git add src/snapshot.rs tests/snapshot.rs
git commit -m "fix(snapshot): bound EMA restore alpha"
```

### Task 3: Preserve SMA’s declared reusable capacity on restore

**Files:**
- Modify: `tests/snapshot.rs:221-228`
- Modify: `src/indicators.rs:12-16,231-237`
- Modify: `src/snapshot.rs:102-116`

**Interfaces:**
- Consumes: `SMA::snapshot` and `SMA::from_snapshot`.
- Produces: An SMA restored with `window.capacity() >= snapshot.capacity` and the same values, capacity, and canonical sum.

- [ ] **Step 1: Write the failing integration test**

```rust
#[test]
fn snapshot_sma_restore_preserves_declared_buffer_capacity() {
    let mut sma = SMA::new(64);
    sma.update(1.0);
    let restored = SMA::from_snapshot(&sma.snapshot()).unwrap();
    assert_eq!(restored.capacity, 64);
    assert!(restored.window.capacity() >= 64);
    assert_eq!(restored.window, vec![1.0]);
}
```

- [ ] **Step 2: Verify the test fails because restore reserves only occupied length**

Run: `cargo test --test snapshot snapshot_sma_restore_preserves_declared_buffer_capacity`

Expected: FAIL because the restored vector capacity is smaller than 64.

- [ ] **Step 3: Write the minimal implementation**

Add a crate-private helper that fallibly reserves `snapshot.capacity`, then extends from `snapshot.window`. Use it from `SMA::from_snapshot` instead of cloning only the occupied slice.

- [ ] **Step 4: Verify the targeted test passes**

Run: `cargo test --test snapshot snapshot_sma_restore_preserves_declared_buffer_capacity`

Expected: PASS; restore retains future update allocation capacity.

- [ ] **Step 5: Commit**

```bash
git add src/indicators.rs src/snapshot.rs tests/snapshot.rs
git commit -m "fix(snapshot): preserve SMA restore capacity"
```

### Task 4: Verify the snapshot surface and prepare delivery evidence

**Files:**
- Modify: `tests/snapshot.rs:349-362`
- Modify: `CHANGELOG.md:16-35`

**Interfaces:**
- Consumes: the corrected snapshot APIs under default and `serde` features.
- Produces: serde regression coverage for VolEstimator, EMA, and SMA plus an unreleased changelog correction that does not claim publication.

- [ ] **Step 1: Write failing serde round-trip tests for EMA and SMA**

```rust
let ema_json = serde_json::to_string(&ema.snapshot()).unwrap();
let ema_snapshot: EMASnapshot = serde_json::from_str(&ema_json).unwrap();
assert_eq!(EMA::from_snapshot(&ema_snapshot).unwrap().value, ema.value);

let sma_json = serde_json::to_string(&sma.snapshot()).unwrap();
let sma_snapshot: SMASnapshot = serde_json::from_str(&sma_json).unwrap();
assert_eq!(SMA::from_snapshot(&sma_snapshot).unwrap().window, sma.window);
```

- [ ] **Step 2: Verify the new serde tests fail only if the intended behavior is absent**

Run: `cargo test --all-features --test snapshot snapshot_serde_roundtrip_preserves`

Expected: The tests should compile and pass immediately if serialization support is already correct; record this as characterization coverage rather than changing production code.

- [ ] **Step 3: Update the unreleased changelog entry**

Add a precise fixed-item note for the Vol capacity invariant, EMA alpha bound, and SMA capacity preservation. Do not add a release date or publication claim.

- [ ] **Step 4: Run the required validation matrix**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo test --all-features
cargo test --no-default-features
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features
cargo test --release --all-features
cargo package
cargo publish --dry-run
```

Expected: Every command succeeds without modifying package-visible files.

- [ ] **Step 5: Commit and open the RM-1332 PR**

```bash
git add CHANGELOG.md tests/snapshot.rs
git commit -m "test(snapshot): cover serde restore regressions"
git push -u origin codex/rm-1332-snapshot-restore-hardening
```

Open one PR with `Closes #<GitHub issue>` only if an in-repository GitHub issue exists for this exact scope; otherwise reference RM-1332 in the body. Add assignee `rmems`, matching labels, the `kinetic-signals — active` milestone, and a comment with the full validation evidence.
