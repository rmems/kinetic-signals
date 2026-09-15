// SPDX-License-Identifier: MIT OR Apache-2.0

//! Allocation-reuse tests for the documented hot output paths.
//!
//! Empty / short / exact-window / multi-window cases compare the allocating
//! APIs with their `*_into` cores. A test-only counting allocator records
//! steady-state output allocations when a pre-sized buffer is reused.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use kinetic_signals::{
    SurpriseParams, compute_shannon_entropy, compute_shannon_entropy_into,
    compute_surprise_sequence, compute_surprise_sequence_into, surprise_sequence_len,
};

struct CountingAlloc;

thread_local! {
    static THREAD_ALLOCS: Cell<u64> = const { Cell::new(0) };
}

fn bump_thread_allocs() {
    let _ = THREAD_ALLOCS.try_with(|c| c.set(c.get().saturating_add(1)));
}

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump_thread_allocs();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump_thread_allocs();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump_thread_allocs();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn thread_allocs() -> u64 {
    THREAD_ALLOCS.with(Cell::get)
}

fn assert_surprise_eq(
    left: &[kinetic_signals::SurpriseResult],
    right: &[kinetic_signals::SurpriseResult],
) {
    assert_eq!(left.len(), right.len());
    for (a, b) in left.iter().zip(right) {
        assert_eq!(a.surprise, b.surprise);
        assert_eq!(a.log_return, b.log_return);
        assert_eq!(a.expected_return, b.expected_return);
        assert_eq!(a.z_score, b.z_score);
    }
}

#[test]
fn surprise_into_matches_allocating_empty_short_exact_multi() {
    let params = SurpriseParams::default();
    let cases: &[&[f64]] = &[
        &[],
        &[100.0],
        &[100.0, 101.0],
        &[100.0, 100.5, 101.0, 100.8, 150.0, 149.5],
    ];
    let mut out = Vec::new();
    for &values in cases {
        let allocated = compute_surprise_sequence(values, &params);
        compute_surprise_sequence_into(values, &params, &mut out);
        assert_eq!(out.len(), surprise_sequence_len(values.len()));
        assert_surprise_eq(&allocated, &out);
    }
}

#[test]
fn entropy_into_matches_allocating_empty_short_exact_multi() {
    let cases: &[(&[f64], usize)] = &[
        (&[], 8),
        (&[1.0], 8),
        (&[1.0, 2.0], 2),
        (&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 4),
    ];
    let mut histogram = Vec::new();
    for &(data, bins) in cases {
        let allocated = compute_shannon_entropy(data, bins);
        let reused = compute_shannon_entropy_into(data, bins, &mut histogram);
        assert_eq!(allocated.shannon, reused.shannon);
        assert_eq!(allocated.relative, reused.relative);
        assert_eq!(allocated.bin_count, reused.bin_count);
    }
}

#[test]
fn surprise_presized_reuse_has_no_steady_state_output_allocation() {
    let params = SurpriseParams::default();
    let values: Vec<f64> = (0..64).map(|i| 100.0 + i as f64 * 0.01).collect();
    let needed = surprise_sequence_len(values.len());
    let mut out = Vec::with_capacity(needed);

    compute_surprise_sequence_into(&values, &params, &mut out);
    assert_eq!(out.len(), needed);
    let ptr = out.as_ptr();
    let cap = out.capacity();

    let before = thread_allocs();
    for _ in 0..256 {
        compute_surprise_sequence_into(&values, &params, &mut out);
    }
    let after = thread_allocs();
    assert_eq!(out.len(), needed);
    assert_eq!(out.capacity(), cap);
    assert_eq!(out.as_ptr(), ptr);
    assert_eq!(
        after,
        before,
        "pre-sized surprise reuse allocated {} extra times",
        after - before
    );
}

#[test]
fn entropy_presized_reuse_has_no_steady_state_output_allocation() {
    let data: Vec<f64> = (0..64).map(|i| i as f64).collect();
    let bins = 16;
    let mut histogram = Vec::with_capacity(bins);

    let first = compute_shannon_entropy_into(&data, bins, &mut histogram);
    assert_eq!(histogram.len(), bins);
    let ptr = histogram.as_ptr();
    let cap = histogram.capacity();

    let before = thread_allocs();
    for _ in 0..256 {
        compute_shannon_entropy_into(&data, bins, &mut histogram);
    }
    let after = thread_allocs();
    let reused = compute_shannon_entropy_into(&data, bins, &mut histogram);
    assert_eq!(reused.shannon, first.shannon);
    assert_eq!(histogram.len(), bins);
    assert_eq!(histogram.capacity(), cap);
    assert_eq!(histogram.as_ptr(), ptr);
    assert_eq!(
        after,
        before,
        "pre-sized entropy reuse allocated {} extra times",
        after - before
    );
}

#[test]
fn allocating_surprise_sequence_records_output_allocation() {
    let params = SurpriseParams::default();
    let values: Vec<f64> = (0..32).map(|i| 100.0 + i as f64).collect();
    let before = thread_allocs();
    let allocated = compute_surprise_sequence(&values, &params);
    let after = thread_allocs();
    assert_eq!(allocated.len(), surprise_sequence_len(values.len()));
    assert!(
        after > before,
        "allocating surprise sequence should perform at least one output allocation"
    );
}
