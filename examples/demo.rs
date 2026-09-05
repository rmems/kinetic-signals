// SPDX-License-Identifier: MIT OR Apache-2.0

use kinetic_signals::{
    EMA, SMA, VolEstimator, ZScore, compute_hawkes, compute_hawkes_streaming, compute_hurst,
    compute_shannon_entropy, compute_signal_stats, compute_surprise, compute_surprise_sequence,
    detect_anomaly, hawkes::HawkesParams, surprise::SurpriseParams,
};

fn lcg_next(state: &mut u64) -> u64 {
    // Numerical Recipes LCG constants (good enough for a demo; not crypto-secure)
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    *state
}

fn pseudo_random_f64(state: &mut u64) -> f64 {
    // Map top 53 bits to [0, 1)
    let x = lcg_next(state) >> 11;
    (x as f64) / ((1u64 << 53) as f64)
}

fn main() {
    println!("=== Kinetic Signals Demo v0.4.0 ===\n");

    demo_hurst();
    demo_hawkes();
    demo_hawkes_streaming();
    demo_surprise();
    demo_surprise_sequence();
    demo_volatility();
    demo_entropy();
    demo_signal_stats();
    demo_indicators();
}

fn demo_hurst() {
    println!("--- Hurst Exponent (Memory/Persistence) ---");

    let persistent: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
    let result = compute_hurst(&persistent);
    println!(
        "Trending data H={:.3} (persistent={})",
        result.h, result.is_persistent
    );

    let mut rng = 0x1234_5678_9abc_def0_u64;
    let random: Vec<f64> = (0..100)
        .map(|_| pseudo_random_f64(&mut rng) - 0.5)
        .collect();
    let result = compute_hurst(&random);
    println!(
        "Random data H={:.3} (anti={})",
        result.h, result.is_antipersistent
    );
    println!();
}

fn demo_hawkes() {
    println!("--- Hawkes Process (Event Clustering) ---");

    let params = HawkesParams {
        mu: 0.1,
        alpha: 0.8,
        beta: 2.0,
        dt: 0.001,
    };

    let burst_events: Vec<f64> = vec![0.0, 0.01, 0.02, 0.03, 0.1, 0.5, 0.51, 0.52];
    let result = compute_hawkes(&burst_events, &params);
    println!(
        "Dense burst: intensity={:.3}, events={}, avg_excitation={:.3}",
        result.intensity, result.event_count, result.avg_excitation
    );

    let sparse_events: Vec<f64> = vec![0.0, 1.0, 2.0, 3.0];
    let result = compute_hawkes(&sparse_events, &params);
    println!(
        "Sparse: intensity={:.3}, events={}",
        result.intensity, result.event_count
    );
    println!();
}

fn demo_hawkes_streaming() {
    println!("--- Hawkes Streaming (Online Intensity) ---");

    let params = HawkesParams {
        mu: 0.1,
        alpha: 0.8,
        beta: 2.0,
        dt: 0.001,
    };

    // Online update: process every event (including the first).
    // `compute_hawkes_streaming` returns pre-jump intensity from decayed history,
    // then `new_decay_sum = decayed + 1`. Post-event intensity (batch-comparable)
    // is μ + α·new_decay_sum at the event instant.
    let event_times = [0.0, 0.01, 0.02, 0.03, 0.1, 0.5, 0.51, 0.52];
    let mut intensity = params.mu;
    let mut decay_sum = 0.0_f64;
    // Seed last_t equal to the first event so the first update has dt=0.
    let mut last_t = event_times[0];

    for (i, &t) in event_times.iter().enumerate() {
        let (_pre_jump, new_decay_sum) =
            compute_hawkes_streaming(intensity, t, last_t, &params, decay_sum);
        // Include the event at t in the displayed intensity (matches batch λ).
        let post_event = params.mu + params.alpha * new_decay_sum;
        intensity = post_event;
        decay_sum = new_decay_sum;
        last_t = t;
        println!(
            "t={:.2}: intensity={:.3}, decay_sum={:.3}{}",
            t,
            intensity,
            decay_sum,
            if i == 0 { " (first event)" } else { "" }
        );
    }

    let batch = compute_hawkes(&event_times, &params);
    println!(
        "Final streaming intensity={:.3} (events={}); batch intensity={:.3}",
        intensity,
        event_times.len(),
        batch.intensity
    );
    println!();
}

fn demo_surprise() {
    println!("--- Surprise (Transition Anomalies) ---");

    let params = SurpriseParams {
        mu: 0.0,
        sigma: 0.15,
        dt: 0.001,
        threshold: 3.0,
    };

    let normal = compute_surprise(100.5, 100.0, &params);
    println!(
        "Normal: surprise={:.3}, z={:.2}",
        normal.surprise, normal.z_score
    );

    let spike = compute_surprise(150.0, 100.0, &params);
    println!(
        "SPIKE: surprise={:.3}, z={:.2} (ANOMALY={})",
        spike.surprise,
        spike.z_score,
        detect_anomaly(&spike, &params)
    );

    let drop = compute_surprise(50.0, 100.0, &params);
    println!(
        "DROP: surprise={:.3}, z={:.2} (ANOMALY={})",
        drop.surprise,
        drop.z_score,
        detect_anomaly(&drop, &params)
    );
    println!();
}

fn demo_surprise_sequence() {
    println!("--- Surprise Sequence + Anomaly Detection ---");

    let params = SurpriseParams {
        mu: 0.0,
        sigma: 0.15,
        dt: 0.001,
        threshold: 3.0,
    };

    // Calm steps + one large jump + one large drop; tiny post-drop move stays sub-threshold
    let series = vec![100.0, 100.5, 101.0, 150.0, 148.5, 50.0, 50.05];
    let results = compute_surprise_sequence(&series, &params);

    let mut anomaly_count = 0usize;
    let mut max_surprise = 0.0_f64;

    for (i, r) in results.iter().enumerate() {
        let is_anomaly = detect_anomaly(r, &params);
        if is_anomaly {
            anomaly_count += 1;
        }
        if r.surprise > max_surprise {
            max_surprise = r.surprise;
        }
        let flag = if is_anomaly { " ANOMALY" } else { "" };
        println!(
            "  step {} ({} → {}): surprise={:.3}, z={:.2}{}",
            i + 1,
            series[i],
            series[i + 1],
            r.surprise,
            r.z_score,
            flag
        );
    }

    println!(
        "Summary: transitions={}, anomalies={}, max_surprise={:.3}",
        results.len(),
        anomaly_count,
        max_surprise
    );
    println!();
}

fn demo_volatility() {
    println!("--- Volatility (Signal Power) ---");

    let abs_log_returns = vec![0.01_f32, 0.02, 0.015, 0.03, 0.012, 0.025, 0.018];

    let mut estimator = VolEstimator::new(5);
    for &r in &abs_log_returns {
        estimator.push(r);
    }

    println!(
        "Rolling RMS volatility={:.4} (window={}, samples={})",
        estimator.rms(),
        5,
        estimator.len()
    );
    println!();
}

fn demo_entropy() {
    println!("--- Shannon Entropy (Complexity) ---");

    let low_entropy = vec![1.0, 1.0, 1.1, 1.0, 0.9, 1.0];
    let res1 = compute_shannon_entropy(&low_entropy, 10);
    println!(
        "Low complexity: H={:.3}, relative={:.3}, bins={}",
        res1.shannon, res1.relative, res1.bin_count
    );

    let mut rng = 0x0bad_f00d_dead_beef_u64;
    let high_entropy: Vec<f64> = (0..100).map(|_| pseudo_random_f64(&mut rng)).collect();
    let res2 = compute_shannon_entropy(&high_entropy, 10);
    println!(
        "High complexity: H={:.3}, relative={:.3}, bins={}",
        res2.shannon, res2.relative, res2.bin_count
    );
    println!();
}

fn demo_signal_stats() {
    println!("--- Signal Stats (High-Order Moments) ---");

    let symmetric = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let s1 = compute_signal_stats(&symmetric);
    println!(
        "Symmetric: mean={:.3}, var={:.3}, skew={:.3}, kurt={:.3}, n={}",
        s1.mean, s1.variance, s1.skewness, s1.kurtosis, s1.count
    );

    // Right-skewed: many small values, one large outlier
    let skewed = vec![1.0, 1.1, 1.2, 1.0, 1.05, 10.0];
    let s2 = compute_signal_stats(&skewed);
    println!(
        "Right-skewed: mean={:.3}, var={:.3}, skew={:.3}, kurt={:.3}, n={}",
        s2.mean, s2.variance, s2.skewness, s2.kurtosis, s2.count
    );
    println!();
}

fn demo_indicators() {
    println!("--- Indicators (EMA / SMA / ZScore) ---");

    let prices = [100.0, 102.0, 101.0, 105.0, 110.0, 108.0, 112.0];

    let mut ema = EMA::new(3);
    let mut sma = SMA::new(3);

    println!("price | EMA(3) | SMA(3) | z vs mean/std of series");

    let stats = compute_signal_stats(&prices);
    let std = stats.variance.sqrt();

    for &p in &prices {
        let e = ema.update(p);
        let s = sma.update(p);
        let z = ZScore::compute(p, stats.mean, std);
        println!("{:5.1} | {:6.3} | {:6.3} | {:+.3}", p, e, s, z);
    }

    println!(
        "Series mean={:.3}, std={:.3} (used for ZScore)",
        stats.mean, std
    );
    println!();
}
