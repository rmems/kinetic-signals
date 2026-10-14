#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Pre-publication package gate (Linear RM-1880): verify that the packaged
# `.crate` artifact — not the repository checkout — builds and runs for
# isolated default-feature and `serde`-feature consumers, and that snapshot
# continuation (serialize -> deserialize -> validate -> restore -> continue)
# matches an uninterrupted estimator for VolEstimator, EMA, and SMA.
#
# Usage: scripts/verify_package.sh [EVIDENCE_DIR]
#
# Artifacts (package list, archive hash, normalized manifest, consumer
# sources and outputs) are written under EVIDENCE_DIR
# (default: target/package-evidence). Exit non-zero on any failed check.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

EVIDENCE_DIR="${1:-$REPO_ROOT/target/package-evidence}"
mkdir -p "$EVIDENCE_DIR"
EVIDENCE_DIR="$(cd "$EVIDENCE_DIR" && pwd)"
WORK="$EVIDENCE_DIR/work"
EXTRACT_DIR="$WORK/extracted"
CONSUMERS_DIR="$WORK/consumers"
rm -rf "$EXTRACT_DIR" "$CONSUMERS_DIR" "$EVIDENCE_DIR/consumers"
mkdir -p "$EXTRACT_DIR" "$CONSUMERS_DIR"

REV="$(git rev-parse HEAD)"
SHORT_REV="$(git rev-parse --short HEAD)"

step() { printf '\n=== %s ===\n' "$1"; }
fail() { echo "FAIL: $1" >&2; exit 1; }

step "Record candidate revision"
echo "$REV" | tee "$EVIDENCE_DIR/revision.txt"

step "cargo package --list"
cargo package --list | tee "$EVIDENCE_DIR/cargo-package-list.txt"

step "Audit packaged file list"
for required in Cargo.toml src/lib.rs README.md CHANGELOG.md LICENSE-MIT LICENSE-APACHE-2.0; do
    grep -qxF "$required" "$EVIDENCE_DIR/cargo-package-list.txt" \
        || fail "required file missing from package: $required"
done
if grep -nE '(^|/)(tests/|benches/|docs/|\.github/|examples/.*\.json|Dockerfile|codecov\.yml|AGENTS\.md|CLAUDE\.md|REVIEW\.md|target/|\.env|.*\.(key|pem)$)' \
    "$EVIDENCE_DIR/cargo-package-list.txt"; then
    fail "repository-only files leaked into package"
fi
if grep -E '^benches/' "$EVIDENCE_DIR/cargo-package-list.txt"; then
    BENCH_PACKED=yes
else
    BENCH_PACKED=no
fi
echo "benches/ present in archive: $BENCH_PACKED"

step "cargo package"
cargo package
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
PKG_VERSION="$(sed -n 's/^version *= *"\([^"]*\)"/\1/p' Cargo.toml | head -1)"
CRATE="$TARGET_DIR/package/kinetic-signals-$PKG_VERSION.crate"
[ -f "$CRATE" ] || fail "no .crate produced"
SHA256="$(sha256sum "$CRATE" | awk '{print $1}')"
echo "$CRATE $SHA256" | tee "$EVIDENCE_DIR/crate-sha256.txt"

step "Extract .crate outside workspace"
tar -xzf "$CRATE" -C "$EXTRACT_DIR"
PKG_BASENAME="$(basename "$CRATE" .crate)"
PKG_DIR="$EXTRACT_DIR/$PKG_BASENAME"
[ -d "$PKG_DIR" ] || fail "extract failed: $PKG_DIR not found"
cp "$PKG_DIR/Cargo.toml" "$EVIDENCE_DIR/normalized-Cargo.toml"
cp "$PKG_DIR/Cargo.toml.orig" "$EVIDENCE_DIR/Cargo.toml.orig" 2>/dev/null || true

step "Bench/package relationship"
# [[bench]] output_reuse is declared in Cargo.toml but benches/ is excluded
# by the manifest `include` list. Only a real failure in the extracted
# artifact — not the declaration itself — is a publication problem.
if grep -q '^\[\[bench\]\]' "$PKG_DIR/Cargo.toml"; then
    if [ ! -f "$PKG_DIR/benches/output_reuse.rs" ]; then
        echo "NOTE: normalized manifest declares [[bench]] output_reuse but" \
            "benches/ is not packaged (include allowlist). Downstream" \
            "dependency builds do not compile bench targets, so this is" \
            "benign unless shown otherwise below." | tee "$EVIDENCE_DIR/bench-note.txt"
    fi
fi

write_consumers() {
    local default_dir="$CONSUMERS_DIR/consumer-default"
    local serde_dir="$CONSUMERS_DIR/consumer-serde"

    mkdir -p "$default_dir/src" "$serde_dir/src"

    cat > "$default_dir/Cargo.toml" <<EOF
[package]
name = "consumer-default"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
kinetic-signals = { path = "$PKG_DIR" }
EOF

    cat > "$serde_dir/Cargo.toml" <<EOF
[package]
name = "consumer-serde"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
kinetic-signals = { path = "$PKG_DIR", features = ["serde"] }
serde_json = "1"
EOF

    cat > "$default_dir/src/main.rs" <<'EOF'
use kinetic_signals::{
    EMA, RESTORE_OUTPUT_TOLERANCE, SMA, SNAPSHOT_SCHEMA_VERSION, VolEstimator,
    compute_hurst, compute_surprise, SurpriseParams,
};

fn main() {
    // Exercise non-snapshot APIs to prove the packaged crate's public surface.
    let h = compute_hurst(&[1.0_f64, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    assert!(h.h.is_finite());
    let params = SurpriseParams::default();
    let surprise = compute_surprise(150.0_f64, 100.0, &params);
    assert!(surprise.z_score.is_finite());

    // Snapshot -> validate -> restore -> continue, without serde.
    let seg_a = [0.01_f32, -0.02, 0.03, -0.01, 0.02, -0.03, 0.01];
    let seg_b = [0.02_f32, -0.01, 0.03, -0.02, 0.01];

    let mut continuous = VolEstimator::new(5);
    for &x in seg_a.iter().chain(&seg_b) {
        continuous.push(x);
    }
    let mut restored = VolEstimator::new(5);
    for &x in &seg_a {
        restored.push(x);
    }
    let snap = restored.snapshot();
    assert_eq!(snap.schema_version, SNAPSHOT_SCHEMA_VERSION);
    snap.validate().expect("vol snapshot must validate");
    let mut restored = VolEstimator::from_snapshot(&snap).expect("vol restore");
    for &x in &seg_b {
        restored.push(x);
    }
    assert!((continuous.rms() - restored.rms()).abs() < RESTORE_OUTPUT_TOLERANCE as f32);

    // EMA / SMA snapshot + restore without serde.
    let seg_a = [100.0_f64, 101.0, 99.5, 102.0, 100.5];
    let seg_b = [103.0_f64, 101.5, 104.0];

    let mut continuous = EMA::new(9);
    let mut cont_last = 0.0;
    for &x in seg_a.iter().chain(&seg_b) {
        cont_last = continuous.update(x);
    }
    let mut ema = EMA::new(9);
    for &x in &seg_a {
        ema.update(x);
    }
    let snap = ema.snapshot();
    snap.validate().expect("ema snapshot must validate");
    let mut restored = EMA::from_snapshot(&snap).expect("ema restore");
    let mut rest_last = 0.0;
    for &x in &seg_b {
        rest_last = restored.update(x);
    }
    assert_eq!(cont_last, rest_last);

    let mut continuous = SMA::new(4);
    let mut cont_last = 0.0;
    for &x in seg_a.iter().chain(&seg_b) {
        cont_last = continuous.update(x);
    }
    let mut sma = SMA::new(4);
    for &x in &seg_a {
        sma.update(x);
    }
    let snap = sma.snapshot();
    snap.validate().expect("sma snapshot must validate");
    let mut restored = SMA::from_snapshot(&snap).expect("sma restore");
    let mut rest_last = 0.0;
    for &x in &seg_b {
        rest_last = restored.update(x);
    }
    assert_eq!(continuous.window, restored.window);
    assert_eq!(continuous.sum, restored.sum);
    assert_eq!(cont_last, rest_last);

    println!("default consumer: OK (VolEstimator + EMA + SMA continuation verified)");
}
EOF

    cat > "$serde_dir/src/main.rs" <<'EOF'
use kinetic_signals::{
    EMA, EMASnapshot, RESTORE_OUTPUT_TOLERANCE, SMA, SMASnapshot,
    SNAPSHOT_SCHEMA_VERSION, VolEstimator, VolEstimatorSnapshot,
};

fn main() {
    // VolEstimator: serialize -> deserialize -> validate -> restore -> continue.
    let seg_a = [0.01_f32, -0.02, 0.03, -0.01, 0.02, -0.03, 0.01];
    let seg_b = [0.02_f32, -0.01, 0.03, -0.02, 0.01];
    let mut continuous = VolEstimator::new(5);
    for &x in seg_a.iter().chain(&seg_b) {
        continuous.push(x);
    }
    let mut est = VolEstimator::new(5);
    for &x in &seg_a {
        est.push(x);
    }
    let snap: VolEstimatorSnapshot =
        serde_json::from_str(&serde_json::to_string(&est.snapshot()).unwrap()).unwrap();
    assert_eq!(snap.schema_version, SNAPSHOT_SCHEMA_VERSION);
    snap.validate().expect("vol snapshot must validate");
    let mut restored = VolEstimator::from_snapshot(&snap).expect("vol restore");
    for &x in &seg_b {
        restored.push(x);
    }
    assert!((continuous.rms() - restored.rms()).abs() < RESTORE_OUTPUT_TOLERANCE as f32);
    assert_eq!(continuous.len(), restored.len());

    // EMA
    let seg_a = [100.0_f64, 101.0, 99.5, 102.0, 100.5];
    let seg_b = [103.0_f64, 101.5, 104.0];
    let mut continuous = EMA::new(9);
    let mut cont_last = 0.0;
    for &x in seg_a.iter().chain(&seg_b) {
        cont_last = continuous.update(x);
    }
    let mut ema = EMA::new(9);
    for &x in &seg_a {
        ema.update(x);
    }
    let snap: EMASnapshot =
        serde_json::from_str(&serde_json::to_string(&ema.snapshot()).unwrap()).unwrap();
    snap.validate().expect("ema snapshot must validate");
    let mut restored = EMA::from_snapshot(&snap).expect("ema restore");
    let mut rest_last = 0.0;
    for &x in &seg_b {
        rest_last = restored.update(x);
    }
    assert_eq!(cont_last, rest_last);

    // SMA
    let mut continuous = SMA::new(4);
    let mut cont_last = 0.0;
    for &x in seg_a.iter().chain(&seg_b) {
        cont_last = continuous.update(x);
    }
    let mut sma = SMA::new(4);
    for &x in &seg_a {
        sma.update(x);
    }
    let snap: SMASnapshot =
        serde_json::from_str(&serde_json::to_string(&sma.snapshot()).unwrap()).unwrap();
    snap.validate().expect("sma snapshot must validate");
    let mut restored = SMA::from_snapshot(&snap).expect("sma restore");
    let mut rest_last = 0.0;
    for &x in &seg_b {
        rest_last = restored.update(x);
    }
    assert_eq!(continuous.window, restored.window);
    assert_eq!(continuous.sum, restored.sum);
    assert_eq!(cont_last, rest_last);

    println!("serde consumer: OK (VolEstimator + EMA + SMA continuation verified)");
}
EOF
}

step "Generate isolated consumer crates"
write_consumers
cp -r "$CONSUMERS_DIR" "$EVIDENCE_DIR/consumers"

step "Build + run default-feature consumer"
cargo run --quiet --manifest-path "$CONSUMERS_DIR/consumer-default/Cargo.toml" \
    | tee "$EVIDENCE_DIR/consumer-default.txt"

step "Build + run serde-feature consumer"
cargo run --quiet --manifest-path "$CONSUMERS_DIR/consumer-serde/Cargo.toml" \
    | tee "$EVIDENCE_DIR/consumer-serde.txt"

step "Evidence report"
{
    echo "# Package gate evidence (RM-1880)"
    echo
    echo "- revision: $REV ($SHORT_REV)"
    echo "- archive: $CRATE"
    echo "- sha256: $SHA256"
    echo "- benches/ in archive: $BENCH_PACKED"
    echo
    echo '```'
    cat "$EVIDENCE_DIR/cargo-package-list.txt"
    echo '```'
    echo
    echo "## Normalized manifest"
    echo '```toml'
    cat "$PKG_DIR/Cargo.toml"
    echo '```'
    echo
    echo "## Consumers"
    echo '```'
    cat "$EVIDENCE_DIR/consumer-default.txt" "$EVIDENCE_DIR/consumer-serde.txt"
    echo '```'
} > "$EVIDENCE_DIR/report.md"

echo
echo "PASS: package gate complete. Evidence in $EVIDENCE_DIR"
