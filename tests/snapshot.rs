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
fn vol_estimator_full_window_replay_matches_continuous() {
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
fn sma_full_window_replay_matches_continuous() {
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
fn ema_replay_matches_continuous() {
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
fn restore_incompatible_version_does_not_mutate_destination() {
    let mut vol = VolEstimator::new(2);
    vol.push(0.1);
    let before = vol.rms();
    let bad = VolEstimatorSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION + 1,
        capacity: 2,
        samples: vec![0.1],
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
        samples: vec![],
    };
    assert_eq!(
        VolEstimator::from_snapshot(&vol),
        Err(SnapshotError::InvalidCapacity)
    );

    let sma = SMASnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        capacity: 0,
        window: vec![],
        sum: 0.0,
    };
    assert_eq!(
        SMA::from_snapshot(&sma),
        Err(SnapshotError::InvalidCapacity)
    );
}

#[cfg(feature = "serde")]
#[test]
fn serde_roundtrip_preserves_vol_estimator_outputs() {
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
