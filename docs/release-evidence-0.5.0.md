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
| Candidate SHA | `97a4b8c00020bc424c1ac30355d9c0e14beadce3` (qualification re-run 2026-10-15, on top of `main` `fbcb9bd`). Not an approved publish commit. CI re-runs the gate on every later commit. |
| Rust toolchain | Development/CI pin 1.99.0 (`rust-toolchain.toml`, Docker, fmt/clippy/test). [RM-2052](https://linear.app/rpd-34/issue/RM-2052) moved that pin from 1.98.1 before publication (1.99.0 released 2026-10-01). This re-run used `rustc 1.99.0 (b940084d7 2026-09-28)`. |
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
| `SnapshotError::InvalidCapacity` display text | Corrected in `97a4b8c` before publication. The variant covers a zero volatility capacity and any capacity above `MAX_SNAPSHOT_CAPACITY`. The previous string ("must be greater than zero") was false for an over-ceiling restore and for `SMA`, where capacity `0` is valid. The variant itself is unchanged. |

## Re-run procedure on the publish commit

1. Check out the exact publish commit.
2. Re-run every command above; update "Candidate SHA" and results.
3. Confirm `cargo package --list`, `cargo publish --dry-run`, and the
   full CI matrix are green on that commit.
4. Link this file from the RM-1722 evidence thread before any
   publication authorization is requested.

## Qualification re-run (2026-10-15)

Measured on `97a4b8c00020bc424c1ac30355d9c0e14beadce3`.
Parent `main` was `fbcb9bd9d23ebec0bfe33cf9de0e3f97ea9b5e75`.
This commit is the release candidate that was executed locally. It is
not a tag, a GitHub Release, or a crates.io upload.

### Registry absence (immediately before any publication)

| Surface | Result |
|---------|--------|
| crates.io `kinetic-signals` | newest version `0.4.0` only (published 2026-09-10) |
| docs.rs `/crate/kinetic-signals/0.5.0` | HTTP 404 |
| Git tag `v0.5.0` | absent (`v0.4.0` → `a46f8c947040dcd8e19c74dcb6c16ff1321231cf`) |
| GitHub Releases | `v0.4.0` only |

### Commands and results

| Command | Result |
|---------|--------|
| `cargo test` | pass |
| `cargo test --all-features` | pass (serde frozen-payload tests included) |
| `cargo test --no-default-features` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo fmt --check` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` | pass |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features serde` | pass |
| `cargo semver-checks check-release --baseline-version 0.4.0` | pass ("no semver update required"; breaking checks skipped because 0.4.0→0.5.0 is a pre-1.0 minor) |
| `cargo semver-checks check-release --baseline-version 0.4.0 --release-type patch` | pass (229 pass, 31 skip, 0 fail; cargo-semver-checks 0.51.0) |
| `cargo audit` | pass (0 advisories; cargo-audit 0.22.2, 1329 advisories loaded) |
| `cargo +1.98.1 check` | pass (MSRV; `rust-toolchain.toml` would otherwise select 1.99.0) |
| `scripts/verify_package.sh` | pass |
| `cargo publish --dry-run` | pass (upload aborted by dry run) |

Packaged archive from that revision:

- path: `target/package/kinetic-signals-0.5.0.crate`
- sha256: `1a17f6911fa10736fdef98b7c9f0ee7250c6f8f4121e53196000b869d1cde85a`
- 20 files, 180.8KiB (50.1KiB compressed)
- `benches/` absent; declared `[[bench]] output_reuse` is dropped by the include allowlist
- isolated default and `serde` consumers both restored VolEstimator, EMA, and SMA and matched continuous continuation

`cargo publish --dry-run` and a green local suite are not publication.

### Still required on the authorized publish commit

The changelog heading stays `[Unreleased]`, and the README still tells
crates.io users to depend on `0.4` with a git dependency for unreleased
0.5.0 APIs. That text is accurate while 0.5.0 is absent from the
registry. The publish commit, after explicit authorization, must:

1. Date `## [0.5.0] - YYYY-MM-DD` and point the compare link at `v0.4.0...v0.5.0`.
2. Switch the README installation snippets to `kinetic-signals = "0.5"` only after the registry version is live (changelog release checklist, step 5).
3. Re-run this table on that exact commit, then tag `v0.5.0`, create the GitHub Release from the same notes, and verify crates.io, docs.rs, and a fresh registry consumer.

The 0.4.0 changelog entry no longer says crates.io availability is still
unverified. That release is the published `0.4.0` from 2026-09-10.
