// SPDX-License-Identifier: MIT OR Apache-2.0

//! End-to-end snapshot/restore replay against `tests/fixtures/snapshot_replay.json`.

use kinetic_signals::{
    EMA, EMASnapshot, RESTORE_OUTPUT_TOLERANCE, SMA, SMASnapshot, SNAPSHOT_SCHEMA_VERSION,
    SnapshotError, VolEstimator, VolEstimatorSnapshot,
};
use serde_json::Value;

const SNAPSHOT_REPLAY_JSON: &str = include_str!("fixtures/snapshot_replay.json");

fn fixture() -> Value {
    serde_json::from_str(SNAPSHOT_REPLAY_JSON).expect("snapshot_replay.json must be valid JSON")
}

fn f64s(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("array of numbers")
        .iter()
        .map(|x| x.as_f64().expect("f64"))
        .collect()
}

fn require_usize(v: &Value, key: &str) -> usize {
    v.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("missing usize field `{key}`")) as usize
}

fn tol() -> f64 {
    fixture()["tolerance"].as_f64().expect("root tolerance")
}

#[test]
fn snapshot_replay_fixture_schema_version_matches_crate() {
    let root = fixture();
    let found = root["schema_version"].as_u64().expect("schema_version") as u32;
    assert_eq!(found, SNAPSHOT_SCHEMA_VERSION);
    assert!((tol() - RESTORE_OUTPUT_TOLERANCE).abs() < f64::EPSILON);
}

#[test]
fn snapshot_vol_estimator_full_window_replay_matches_continuous() {
    let v = &fixture()["vectors"]["vol_estimator_full_window_replay"];
    let input = &v["input"];
    let window = require_usize(input, "window");
    let segment_a = f64s(&input["segment_a"]);
    let segment_b = f64s(&input["segment_b"]);
    assert!(
        segment_a.len() > window,
        "replay fixture must wrap a non-empty rolling window"
    );

    let mut continuous = VolEstimator::new(window);
    for &x in segment_a.iter().chain(&segment_b) {
        continuous.push(x as f32);
    }

    let mut restored = VolEstimator::new(window);
    for &x in &segment_a {
        restored.push(x as f32);
    }
    assert_eq!(restored.len(), window);
    let snap = restored.snapshot();
    assert_eq!(snap.schema_version, SNAPSHOT_SCHEMA_VERSION);
    assert_eq!(snap.capacity, window);
    assert_eq!(snap.samples.len(), window);
    restored.restore(&snap).unwrap();
    for &x in &segment_b {
        restored.push(x as f32);
    }

    let continuous_rms = continuous.rms();
    let restored_rms = restored.rms();
    println!(
        "vol_estimator_full_window_replay: continuous_rms={continuous_rms:.8} restored_rms={restored_rms:.8} abs_diff={:.8e} tol={:.8e} window={window} n_a={} n_b={}",
        (continuous_rms - restored_rms).abs(),
        RESTORE_OUTPUT_TOLERANCE,
        segment_a.len(),
        segment_b.len()
    );
    assert!((continuous_rms - restored_rms).abs() < RESTORE_OUTPUT_TOLERANCE as f32);
    assert_eq!(continuous.len(), restored.len());
}

#[test]
fn snapshot_sma_full_window_replay_matches_continuous() {
    let v = &fixture()["vectors"]["sma_full_window_replay"];
    let input = &v["input"];
    let capacity = require_usize(input, "capacity");
    let segment_a = f64s(&input["segment_a"]);
    let segment_b = f64s(&input["segment_b"]);

    let mut continuous = SMA::new(capacity);
    let mut continuous_last = 0.0;
    for &x in segment_a.iter().chain(&segment_b) {
        continuous_last = continuous.update(x);
    }

    let mut restored = SMA::new(capacity);
    for &x in &segment_a {
        restored.update(x);
    }
    assert_eq!(restored.window.len(), capacity);
    let snap = restored.snapshot();
    restored.restore(&snap).unwrap();
    let mut restored_last = 0.0;
    for &x in &segment_b {
        restored_last = restored.update(x);
    }

    println!(
        "sma_full_window_replay: continuous_last={continuous_last:.8} restored_last={restored_last:.8} abs_diff={:.8e}",
        (continuous_last - restored_last).abs()
    );
    assert_eq!(continuous.window, restored.window);
    assert_eq!(continuous.sum, restored.sum);
    assert_eq!(continuous_last, restored_last);
}

#[test]
fn snapshot_ema_replay_matches_continuous() {
    let v = &fixture()["vectors"]["ema_replay"];
    let input = &v["input"];
    let period = require_usize(input, "period");
    let segment_a = f64s(&input["segment_a"]);
    let segment_b = f64s(&input["segment_b"]);

    let mut continuous = EMA::new(period);
    let mut continuous_last = 0.0;
    for &x in segment_a.iter().chain(&segment_b) {
        continuous_last = continuous.update(x);
    }

    let mut restored = EMA::new(period);
    for &x in &segment_a {
        restored.update(x);
    }
    let snap = restored.snapshot();
    restored.restore(&snap).unwrap();
    let mut restored_last = 0.0;
    for &x in &segment_b {
        restored_last = restored.update(x);
    }

    println!(
        "ema_replay: continuous_last={continuous_last:.8} restored_last={restored_last:.8} abs_diff={:.8e}",
        (continuous_last - restored_last).abs()
    );
    assert!((continuous_last - restored_last).abs() < RESTORE_OUTPUT_TOLERANCE);
}

#[test]
fn snapshot_restore_incompatible_version_does_not_mutate() {
    let mut vol = VolEstimator::new(2);
    vol.push(0.1);
    let before = vol.rms();
    let bad = VolEstimatorSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION + 1,
        capacity: 2,
        pos: 1,
        full: false,
        samples: vec![0.1, 0.0],
    };
    assert!(matches!(
        vol.restore(&bad),
        Err(SnapshotError::IncompatibleVersion {
            found,
            expected: SNAPSHOT_SCHEMA_VERSION
        }) if found == SNAPSHOT_SCHEMA_VERSION + 1
    ));
    assert_eq!(vol.rms(), before);

    let mut ema = EMA::new(4);
    ema.update(10.0);
    let ema_before = ema.value;
    let bad_ema = EMASnapshot {
        schema_version: 0,
        value: 99.0,
        alpha: ema.alpha,
        initialized: true,
    };
    assert!(matches!(
        ema.restore(&bad_ema),
        Err(SnapshotError::IncompatibleVersion { found: 0, .. })
    ));
    assert_eq!(ema.value, ema_before);

    let mut sma = SMA::new(2);
    sma.update(1.0);
    let sma_before = sma.window.clone();
    let bad_sma = SMASnapshot {
        schema_version: 7,
        capacity: 2,
        window: vec![9.0],
        sum: 9.0,
    };
    assert!(matches!(
        sma.restore(&bad_sma),
        Err(SnapshotError::IncompatibleVersion { found: 7, .. })
    ));
    assert_eq!(sma.window, sma_before);
}

#[test]
fn from_snapshot_rejects_zero_capacity() {
    let vol = VolEstimatorSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: 0,
        pos: 0,
        full: false,
        samples: vec![],
    };
    assert!(matches!(
        VolEstimator::from_snapshot(&vol),
        Err(SnapshotError::InvalidCapacity)
    ));
}

#[test]
fn snapshot_sma_zero_capacity_round_trips() {
    let sma = SMA::new(0);
    let restored = SMA::from_snapshot(&sma.snapshot()).unwrap();
    assert_eq!(restored.capacity, 0);
    assert!(restored.window.is_empty());
    assert_eq!(restored.sum, 0.0);
}

#[test]
fn snapshot_restore_rejects_invalid_length_and_non_finite_without_mutating() {
    let mut vol = VolEstimator::new(2);
    vol.push(0.1);
    let before_rms = vol.rms();
    let before_len = vol.len();
    let oversized = VolEstimatorSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: 2,
        pos: 0,
        full: false,
        samples: vec![0.1, 0.2, 0.3],
    };
    assert_eq!(vol.restore(&oversized), Err(SnapshotError::InvalidLength));
    assert_eq!(vol.len(), before_len);
    assert_eq!(vol.rms(), before_rms);

    let mut sma = SMA::new(2);
    sma.update(1.0);
    let sma_before = sma.clone();
    let nan_window = SMASnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: 2,
        window: vec![f64::NAN],
        sum: 0.0,
    };
    assert_eq!(sma.restore(&nan_window), Err(SnapshotError::NonFinite));
    assert_eq!(sma.window, sma_before.window);
    assert_eq!(sma.sum, sma_before.sum);

    let inf_sum = SMASnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: 2,
        window: vec![1.0],
        sum: f64::INFINITY,
    };
    assert_eq!(sma.restore(&inf_sum), Err(SnapshotError::NonFinite));
    assert_eq!(sma.window, sma_before.window);
    assert_eq!(sma.sum, sma_before.sum);

    let mut ema = EMA::new(4);
    ema.update(10.0);
    let ema_before = ema.clone();
    let nan_alpha = EMASnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        value: ema.value,
        alpha: f64::NAN,
        initialized: true,
    };
    assert_eq!(ema.restore(&nan_alpha), Err(SnapshotError::NonFinite));
    assert_eq!(ema.value, ema_before.value);
    assert_eq!(ema.alpha, ema_before.alpha);
}

#[test]
fn snapshot_from_snapshot_rejects_unallocatable_capacity() {
    let snap = VolEstimatorSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: usize::MAX,
        pos: 0,
        full: false,
        samples: vec![],
    };
    assert!(matches!(
        VolEstimator::from_snapshot(&snap),
        Err(SnapshotError::InvalidLength)
    ));
}

#[test]
fn snapshot_restore_skips_non_finite_inputs_like_live_estimators() {
    let mut vol_with_nan = VolEstimator::new(3);
    vol_with_nan.push(0.1);
    vol_with_nan.push(f32::NAN);
    vol_with_nan.push(0.2);
    let mut vol_clean = VolEstimator::new(3);
    vol_clean.push(0.1);
    vol_clean.push(0.2);
    let mut vol_restored = VolEstimator::from_snapshot(&vol_with_nan.snapshot()).unwrap();
    vol_with_nan.push(0.3);
    vol_clean.push(0.3);
    vol_restored.push(0.3);
    assert_eq!(vol_restored.rms(), vol_clean.rms());
    assert_eq!(vol_with_nan.rms(), vol_clean.rms());

    let mut ema_with_nan = EMA::new(5);
    ema_with_nan.update(10.0);
    ema_with_nan.update(f64::INFINITY);
    ema_with_nan.update(12.0);
    let mut ema_clean = EMA::new(5);
    ema_clean.update(10.0);
    ema_clean.update(12.0);
    let mut ema_restored = EMA::from_snapshot(&ema_with_nan.snapshot()).unwrap();
    ema_with_nan.update(11.0);
    ema_clean.update(11.0);
    ema_restored.update(11.0);
    assert_eq!(ema_restored.value, ema_clean.value);
    assert_eq!(ema_with_nan.value, ema_clean.value);

    let mut sma_with_nan = SMA::new(3);
    sma_with_nan.update(1.0);
    sma_with_nan.update(f64::NAN);
    sma_with_nan.update(2.0);
    sma_with_nan.update(3.0);
    let mut sma_clean = SMA::new(3);
    sma_clean.update(1.0);
    sma_clean.update(2.0);
    sma_clean.update(3.0);
    let mut sma_restored = SMA::from_snapshot(&sma_with_nan.snapshot()).unwrap();
    sma_with_nan.update(4.0);
    sma_clean.update(4.0);
    sma_restored.update(4.0);
    assert_eq!(sma_restored.window, sma_clean.window);
    assert_eq!(sma_restored.sum, sma_clean.sum);
    assert_eq!(sma_with_nan.sum, sma_clean.sum);
}

#[cfg(feature = "serde")]
#[test]
fn snapshot_serde_roundtrip_preserves_vol_estimator_outputs() {
    let mut src = VolEstimator::new(4);
    for x in [0.01_f32, 0.04, 0.03, 0.02, 0.05] {
        src.push(x);
    }
    let json = serde_json::to_string(&src.snapshot()).unwrap();
    let decoded: VolEstimatorSnapshot = serde_json::from_str(&json).unwrap();
    let mut restored = VolEstimator::from_snapshot(&decoded).unwrap();
    src.push(0.06);
    restored.push(0.06);
    assert!((src.rms() - restored.rms()).abs() < RESTORE_OUTPUT_TOLERANCE as f32);
}
