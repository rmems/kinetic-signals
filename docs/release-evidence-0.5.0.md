# Release evidence: kinetic-signals 0.4.0 → 0.5.0

Qualification record for the v0.5.0 release candidate. Tracks
[RM-1722](https://linear.app/rpd-34/issue/RM-1722); produced under
[RM-1879](https://linear.app/rpd-34/issue/RM-1879). This record is
qualification evidence only — it does not authorize merge, tag, or
publication.

## Revisions and tools

| Item | Value |
|------|-------|
| Baseline release | `v0.4.0` tag `a46f8c947040dcd8e19c74dcb6c16ff1321231cf` (crates.io `kinetic-signals 0.4.0`) |
| Candidate SHA | `309a86f` (PR head at qualification time; CI re-runs the gate on every commit, so this records where the manual results were obtained — re-run on the exact publish commit) |
| Rust toolchain | Development/CI pin 1.99.0 (`rust-toolchain.toml`, Docker, fmt/clippy/test). [RM-2052](https://linear.app/rpd-34/issue/RM-2052) moved that pin from 1.98.1 before publication (1.99.0 released 2026-10-01; rechecked 2026-10-14). The manual results below were recorded at `309a86f` and must be re-run on the publish commit. |
| cargo-semver-checks | 0.51.0 |
| MSRV note | 0.4.0's MSRV was 1.98.1 and 0.5.0 keeps it. The minimum-toolchain job runs `cargo +1.98.1 check`. The development toolchain is newer on purpose; the crate does not use 1.99-only language features. |

## Automated checks

### Warnings-denied rustdoc

```bash
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features serde
```

Result: PASS (both configurations). Enforced in CI job `rustdoc`.

### Public API comparison

```bash
cargo semver-checks check-release --baseline-version 0.4.0
cargo semver-checks check-release --baseline-version 0.4.0 --release-type patch
```

Results:

- Derived-check run (0.4.0 → 0.5.0): "no semver update required" —
  the minor bump satisfies the pre-1.0 SemVer policy for a breaking
  release.
- `--release-type patch` run (enumerates every detectable API break):
  **229 checks pass, 31 skipped, 0 failures** — no signature-level
  breaking changes detected. All new items (snapshot/restore APIs,
  `*_into` output paths, `serde` impls, `MAX_ENTROPY_BINS`) are
  additive.

Enforced in CI job `api-diff`, which runs both commands above
(`--baseline-version 0.4.0` fetches the published 0.4.0 crate; the
patch-strict run fails on any future signature-level break that a
0.x minor bump would otherwise mask).

## Human review of behavioral changes

cargo-semver-checks compares API signatures and cannot prove that
tightened preconditions or changed behavior are safe. The following
0.5.0 changes were reviewed by hand against the CHANGELOG "Breaking"
section and README contract tables:

| Change | Review finding |
|--------|----------------|
| `VolEstimator::new` panics when `capacity > MAX_SNAPSHOT_CAPACITY` (`1_000_000`) | Intentional, documented. Aligns live construction with the untrusted-restore ceiling. Callers needing larger windows stay on 0.4.x. |
| `SMA::new` panics when `capacity > MAX_SNAPSHOT_CAPACITY` | Intentional, documented. Same shared ceiling; `capacity == 0` still accepted (no-op estimator). |
| `compute_shannon_entropy` / `compute_shannon_entropy_into` reject `bins > MAX_ENTROPY_BINS` with the `bin_count == 0` sentinel | Intentional, documented. Rejection precedes the constant-series branch so oversized requests never read as real zero-entropy results; all valid `bins` unchanged. |
| Prelude glob additions (`compute_surprise_sequence_into`, `compute_shannon_entropy_into`, `surprise_sequence_len`, `MAX_ENTROPY_BINS`, snapshot types) | Reviewed per the README pre-1.0 policy: glob additions can collide with downstream combined globs; migration notes in "Upgrading from v0.4.x". |
| Hawkes zero-decay semantics | Resolved under [RM-1871](https://linear.app/rpd-34/issue/RM-1871): `beta == 0` (incl. `-0.0`) is the non-decaying limit (decay factor exactly `1.0`) through one shared helper; not a breaking change. Verify its issue state stays closed at publish time. |
| `serde` feature (optional, off by default) | Additive; derive impls on snapshot types only. Default build remains zero-dependency. |

## Re-run procedure on the publish commit

1. Check out the exact publish commit.
2. Re-run every command above; update "Candidate SHA" and results.
3. Confirm `cargo package --list`, `cargo publish --dry-run`, and the
   full CI matrix are green on that commit.
4. Link this file from the RM-1722 evidence thread before any
   publication authorization is requested.
