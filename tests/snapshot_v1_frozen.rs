// SPDX-License-Identifier: MIT OR Apache-2.0

//! Compatibility checks against frozen schema-v1 payloads in
//! `tests/fixtures/snapshot_v1_frozen.json`.
//!
//! The fixture is historical evidence from a named producer revision. If a test
//! here fails after a representation change, schema-v1 compatibility broke:
//! fix the code or bump the schema, never the fixture.

#![cfg(feature = "serde")]

use std::fmt::Debug;

use kinetic_signals::{
    EMA, EMASnapshot, SMA, SMASnapshot, SNAPSHOT_SCHEMA_VERSION, SnapshotError, VolEstimator,
    VolEstimatorSnapshot,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

const FROZEN_V1_JSON: &str = include_str!("fixtures/snapshot_v1_frozen.json");
const FROZEN_SCHEMA_VERSION: u32 = 1;

trait Estimator: Sized {
    type Snapshot: Serialize + DeserializeOwned + PartialEq + Debug;

    fn construct(arg: usize) -> Self;
    fn snapshot(&self) -> Self::Snapshot;
    fn validate(snapshot: &Self::Snapshot) -> Result<(), SnapshotError>;
    fn from_snapshot(snapshot: &Self::Snapshot) -> Result<Self, SnapshotError>;
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), SnapshotError>;
    fn step(&mut self, x: f64) -> f64;
}

impl Estimator for VolEstimator {
    type Snapshot = VolEstimatorSnapshot;

    fn construct(arg: usize) -> Self {
        Self::new(arg)
    }
    fn snapshot(&self) -> Self::Snapshot {
        self.snapshot()
    }
    fn validate(snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        snapshot.validate()
    }
    fn from_snapshot(snapshot: &Self::Snapshot) -> Result<Self, SnapshotError> {
        Self::from_snapshot(snapshot)
    }
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        self.restore(snapshot)
    }
    fn step(&mut self, x: f64) -> f64 {
        self.push(x as f32);
        f64::from(self.rms())
    }
}

impl Estimator for EMA {
    type Snapshot = EMASnapshot;

    fn construct(arg: usize) -> Self {
        Self::new(arg)
    }
    fn snapshot(&self) -> Self::Snapshot {
        self.snapshot()
    }
    fn validate(snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        snapshot.validate()
    }
    fn from_snapshot(snapshot: &Self::Snapshot) -> Result<Self, SnapshotError> {
        Self::from_snapshot(snapshot)
    }
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        self.restore(snapshot)
    }
    fn step(&mut self, x: f64) -> f64 {
        self.update(x)
    }
}

impl Estimator for SMA {
    type Snapshot = SMASnapshot;

    fn construct(arg: usize) -> Self {
        Self::new(arg)
    }
    fn snapshot(&self) -> Self::Snapshot {
        self.snapshot()
    }
    fn validate(snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        snapshot.validate()
    }
    fn from_snapshot(snapshot: &Self::Snapshot) -> Result<Self, SnapshotError> {
        Self::from_snapshot(snapshot)
    }
    fn restore(&mut self, snapshot: &Self::Snapshot) -> Result<(), SnapshotError> {
        self.restore(snapshot)
    }
    fn step(&mut self, x: f64) -> f64 {
        self.update(x)
    }
}

fn fixture() -> Value {
    serde_json::from_str(FROZEN_V1_JSON).expect("snapshot_v1_frozen.json must be valid JSON")
}

fn section(root: &Value, key: &str) -> Map<String, Value> {
    root[key]
        .as_object()
        .unwrap_or_else(|| panic!("missing fixture section `{key}`"))
        .clone()
}

fn f64s(v: &Value) -> Vec<f64> {
    v.as_array()
        .expect("array of numbers")
        .iter()
        .map(|x| x.as_f64().expect("f64"))
        .collect()
}

fn estimator_kind(case: &Value) -> &str {
    case["estimator"].as_str().expect("estimator name")
}

fn error_name(err: SnapshotError) -> (&'static str, Option<u32>) {
    match err {
        SnapshotError::IncompatibleVersion { found, expected } => {
            assert_eq!(expected, SNAPSHOT_SCHEMA_VERSION);
            ("IncompatibleVersion", Some(found))
        }
        SnapshotError::InvalidCapacity => ("InvalidCapacity", None),
        SnapshotError::InvalidLength => ("InvalidLength", None),
        SnapshotError::NonFinite => ("NonFinite", None),
        SnapshotError::InconsistentState => ("InconsistentState", None),
        SnapshotError::AllocationFailed => ("AllocationFailed", None),
        other => panic!("unmapped SnapshotError variant {other:?}"),
    }
}

fn check_valid<E: Estimator>(name: &str, case: &Value, tolerance: f64) {
    let payload = &case["payload"];
    let snap: E::Snapshot = serde_json::from_value(payload.clone())
        .unwrap_or_else(|e| panic!("{name}: frozen v1 payload no longer decodes: {e}"));

    let reencoded: Value = serde_json::from_str(&serde_json::to_string(&snap).unwrap()).unwrap();
    assert_eq!(
        &reencoded, payload,
        "{name}: schema-v1 field set or values changed on re-encode"
    );

    assert_eq!(E::validate(&snap), Ok(()), "{name}: validate");
    let mut restored = E::from_snapshot(&snap).expect("from_snapshot");
    assert_eq!(restored.snapshot(), snap, "{name}: from_snapshot state");

    let constructor_arg = case["constructor_arg"].as_u64().expect("constructor_arg") as usize;
    let mut reused = E::construct(constructor_arg);
    reused.step(1.0);
    reused.restore(&snap).expect("restore");
    assert_eq!(reused.snapshot(), snap, "{name}: restore state");

    let input = f64s(&case["continuation_input"]);
    let expected = f64s(&case["expected_continuation"]);
    assert_eq!(input.len(), expected.len(), "{name}: fixture shape");
    for (i, (&x, &want)) in input.iter().zip(&expected).enumerate() {
        for (label, est) in [("from_snapshot", &mut restored), ("restore", &mut reused)] {
            let got = est.step(x);
            assert!(
                (got - want).abs() <= tolerance,
                "{name}/{label}: continuation step {i} got {got}, frozen {want}"
            );
        }
    }
}

fn check_decode_failure<E: Estimator>(name: &str, case: &Value) {
    let decoded = serde_json::from_value::<E::Snapshot>(case["payload"].clone());
    assert!(
        decoded.is_err(),
        "{name}: malformed payload unexpectedly decoded: {decoded:?}"
    );
}

fn check_validation_failure<E: Estimator>(name: &str, case: &Value, seed_arg: usize) {
    let snap: E::Snapshot = serde_json::from_value(case["payload"].clone())
        .unwrap_or_else(|e| panic!("{name}: payload must decode before validation: {e}"));
    let want_name = case["expected_error"].as_str().expect("expected_error");
    let want_found = case.get("found").map(|v| v.as_u64().expect("found") as u32);
    let want = (want_name, want_found);

    let err = E::validate(&snap).expect_err("validate must reject");
    assert_eq!(error_name(err), want, "{name}: validate");
    let err = E::from_snapshot(&snap)
        .err()
        .unwrap_or_else(|| panic!("{name}: from_snapshot must reject"));
    assert_eq!(error_name(err), want, "{name}: from_snapshot");

    let mut dest = E::construct(seed_arg);
    dest.step(1.0);
    let before = dest.snapshot();
    let err = dest.restore(&snap).expect_err("restore must reject");
    assert_eq!(error_name(err), want, "{name}: restore");
    assert_eq!(
        dest.snapshot(),
        before,
        "{name}: failed restore mutated state"
    );
}

#[test]
fn frozen_v1_fixture_targets_current_schema_version() {
    let root = fixture();
    assert_eq!(root["schema_version"].as_u64(), Some(1));
    assert_eq!(
        SNAPSHOT_SCHEMA_VERSION, FROZEN_SCHEMA_VERSION,
        "SNAPSHOT_SCHEMA_VERSION changed: keep these v1 fixtures as-is and decide \
         (per the schema-v1 compatibility policy) whether v1 payloads must still restore"
    );
    for (name, case) in section(&root, "valid") {
        assert_eq!(
            case["payload"]["schema_version"].as_u64(),
            Some(u64::from(FROZEN_SCHEMA_VERSION)),
            "{name}"
        );
    }
}

#[test]
fn frozen_v1_fixture_records_provenance() {
    let root = fixture();
    let provenance = root["provenance"].as_object().expect("provenance");
    for key in [
        "producer_crate",
        "producer_version",
        "producer_commit",
        "codec",
        "procedure",
        "policy",
    ] {
        let value = provenance
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("provenance.{key} missing"));
        assert!(!value.is_empty(), "provenance.{key} empty");
    }
    assert_eq!(provenance["producer_crate"], "kinetic-signals");
    let commit = provenance["producer_commit"].as_str().unwrap();
    assert_eq!(commit.len(), 40, "producer_commit must be a full SHA");
    assert!(commit.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn frozen_v1_fixture_covers_every_estimator() {
    let root = fixture();
    for section_name in ["valid", "decode_failures", "validation_failures"] {
        let cases = section(&root, section_name);
        for kind in ["VolEstimator", "EMA", "SMA"] {
            assert!(
                cases.values().any(|c| estimator_kind(c) == kind),
                "{section_name} has no {kind} case"
            );
        }
    }
    let validation_failures = section(&root, "validation_failures");
    for kind in ["VolEstimator", "EMA", "SMA"] {
        assert!(
            validation_failures
                .values()
                .any(|c| c["expected_error"] == "IncompatibleVersion" && estimator_kind(c) == kind),
            "no incompatible-schema case for {kind}"
        );
    }
}

#[test]
fn frozen_v1_payloads_decode_restore_and_continue() {
    let root = fixture();
    let tolerance = root["tolerance"].as_f64().expect("tolerance");
    for (name, case) in section(&root, "valid") {
        match estimator_kind(&case) {
            "VolEstimator" => check_valid::<VolEstimator>(&name, &case, tolerance),
            "EMA" => check_valid::<EMA>(&name, &case, tolerance),
            "SMA" => check_valid::<SMA>(&name, &case, tolerance),
            other => panic!("{name}: unknown estimator {other}"),
        }
    }
}

#[test]
fn frozen_v1_malformed_payloads_fail_to_decode() {
    for (name, case) in section(&fixture(), "decode_failures") {
        match estimator_kind(&case) {
            "VolEstimator" => check_decode_failure::<VolEstimator>(&name, &case),
            "EMA" => check_decode_failure::<EMA>(&name, &case),
            "SMA" => check_decode_failure::<SMA>(&name, &case),
            other => panic!("{name}: unknown estimator {other}"),
        }
    }
}

#[test]
fn frozen_v1_invalid_payloads_decode_but_fail_validation_without_mutating() {
    for (name, case) in section(&fixture(), "validation_failures") {
        match estimator_kind(&case) {
            "VolEstimator" => check_validation_failure::<VolEstimator>(&name, &case, 2),
            "EMA" => check_validation_failure::<EMA>(&name, &case, 4),
            "SMA" => check_validation_failure::<SMA>(&name, &case, 2),
            other => panic!("{name}: unknown estimator {other}"),
        }
    }
}
