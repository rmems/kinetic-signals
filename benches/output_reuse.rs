// SPDX-License-Identifier: MIT OR Apache-2.0

//! Timing evidence for allocating vs caller-buffer surprise-sequence reuse.
//!
//! Allocation counts for the same path live in `tests/output_reuse.rs`
//! (test allocator). This bench is the `cargo bench` half of that evidence.

use std::hint::black_box;
use std::time::Instant;

use kinetic_signals::{SurpriseParams, compute_surprise_sequence, compute_surprise_sequence_into};

fn nsec_per_iter(iters: u32, elapsed: std::time::Duration) -> f64 {
    elapsed.as_secs_f64() * 1e9 / f64::from(iters)
}

fn main() {
    let params = SurpriseParams::default();
    let values: Vec<f64> = (0..256).map(|i| 100.0 + (i as f64) * 0.01).collect();
    let needed = values.len() - 1;
    let warmup = 200u32;
    let iters = 5_000u32;

    for _ in 0..warmup {
        let _ = black_box(compute_surprise_sequence(
            black_box(&values),
            black_box(&params),
        ));
    }
    let start = Instant::now();
    for _ in 0..iters {
        let _ = black_box(compute_surprise_sequence(
            black_box(&values),
            black_box(&params),
        ));
    }
    let allocating_ns = nsec_per_iter(iters, start.elapsed());

    let mut out = Vec::with_capacity(needed);
    for _ in 0..warmup {
        compute_surprise_sequence_into(black_box(&values), black_box(&params), black_box(&mut out));
        black_box(out.len());
    }
    let cap = out.capacity();
    let ptr = out.as_ptr();
    let start = Instant::now();
    for _ in 0..iters {
        compute_surprise_sequence_into(black_box(&values), black_box(&params), black_box(&mut out));
        black_box(out.len());
    }
    let reuse_ns = nsec_per_iter(iters, start.elapsed());

    assert_eq!(out.len(), needed);
    assert_eq!(out.capacity(), cap);
    assert_eq!(out.as_ptr(), ptr);

    println!(
        "surprise_sequence n={} transitions={}",
        values.len(),
        needed
    );
    println!("  allocating: {allocating_ns:.1} ns/iter");
    println!("  reuse:      {reuse_ns:.1} ns/iter");
    println!(
        "  speedup:    {:.2}x (reuse / allocating = {:.3})",
        allocating_ns / reuse_ns,
        reuse_ns / allocating_ns
    );
    println!("  reuse buffer pointer and capacity unchanged over {iters} timed iterations");
}
