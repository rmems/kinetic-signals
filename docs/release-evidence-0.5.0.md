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
| Candidate SHA | `29ae4bf` (PR head at qualification time; re-run on the exact publish commit) |
| Rust toolchain | 1.98.1 (CI pin and MSRV); local verification on 1.99.0 |
| cargo-semver-checks | 0.51.0 |
| MSRV note | Rust 1.98.1 was already the 0.4.0 MSRV (`rust-version` in the v0.4.0 `Cargo.toml`); it is unchanged, not new in 0.5.0 |

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
