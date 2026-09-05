// SPDX-License-Identifier: MIT OR Apache-2.0

use kinetic_signals::{
    HawkesParams, SurpriseParams, VolEstimator, compute_hawkes, compute_hurst, compute_surprise,
    detect_anomaly,
};

#[test]
fn readme_usage_compiles_and_runs() {
    let data: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let result = compute_hurst(&data);
    assert!(result.h.is_finite());

    let params = HawkesParams::default();
    let events = vec![0.0, 0.01, 0.02, 0.1, 0.5];
    let result = compute_hawkes(&events, &params);
    assert!(result.intensity.is_finite());

    let params = SurpriseParams::default();
    let surprise = compute_surprise(150.0, 100.0, &params);
    let _ = detect_anomaly(&surprise, &params);

    let mut vol = VolEstimator::new(64);
    vol.push(0.01);
    vol.push(0.02);
    assert!(vol.rms().is_finite());
}
