// SPDX-License-Identifier: MIT OR Apache-2.0

//! RM-1882: bounded, reproducible checks of the documented Hawkes definition.
//!
//! The reference directly sums exp(-beta * (t - t_j)) with Kahan compensation,
//! independently of production helpers and streaming state. Pre-jump means
//! j < prefix_index, NOT t_j < t: earlier same-time events contribute. Post-event
//! includes the current event. For either signed zero beta the analytical sum
//! is the event count, even when subtracting finite timestamps would overflow.
//!
//! Reference precision is f64, not arbitrary precision: compensation reduces
//! summation error but cannot correct timestamp rounding or libm exp error
//! (exp is shared with production). Analytical cases below anchor the oracle.
//! Comparisons use |actual - expected| <= ATOL + RTOL * |expected|. For at most
//! 128 events, 16 * 128 * EPSILON (~4.55e-13) budgets accumulated arithmetic and
//! exp rounding in the direct sum and recurrence. The absolute budget is only
//! 4 * 128 subnormal ULPs (~2.53e-321), allowing gradual-underflow rounding, not
//! a unit-scale floor that could hide wrong answers at tiny normal scales.
//! This is a numerical regression tolerance, not a proof of libm accuracy.
//!
//! Four fixed LCG seeds (including the previous numeric_hardening Hawkes seed)
//! exercise 7 lengths, 9 beta/gap regimes and 8 amplitude pairs: 2,016 histories,
//! at most 128 events each. High RNG bits vary gaps; fixed gap slots guarantee
//! ties, tiny gaps and decay underflow rather than relying on chance. All
//! generated timestamps, differences and beta*gap products are finite by
//! construction and asserted, without rejection sampling or filtering.
//! mu + 128*alpha stays finite by construction; true intensity overflow and
//! invalid inputs belong in numeric_hardening / hawkes_zero_decay instead.
//! Failures print the seed, parameters, complete round-trippable f64 history,
//! zero-based prefix index, comparison label, and expected/actual values.

use kinetic_signals::{HawkesParams, compute_hawkes, compute_hawkes_streaming};

const MAX_EVENTS: usize = 128;
const RTOL: f64 = 16.0 * MAX_EVENTS as f64 * f64::EPSILON;
const ATOL: f64 = 4.0 * MAX_EVENTS as f64 * f64::from_bits(1);
const SEEDS: [u64; 4] = [0, 1, 0x7777_8888_9999_aaaa, u64::MAX];

fn generated_history(seed: u64, count: usize, scale: f64) -> Vec<f64> {
    let mut state = seed;
    let mut time = -scale;
    (0..count)
        .map(|i| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let unit = (state >> 11) as f64 / (1_u64 << 53) as f64;
            let gap = match i % 8 {
                0 | 4 | 6 => 0.0,
                1 => 2.0_f64.powi(-40),
                2 => unit,
                3 => 2.0 + unit,
                5 => 1024.0,
                _ => unit * 8.0,
            };
            time += gap * scale;
            time
        })
        .collect()
}

#[derive(Debug)]
struct Case<'a> {
    seed: u64,
    params: &'a HawkesParams,
    history: &'a [f64],
}

impl Case<'_> {
    fn close(&self, index: usize, label: &str, actual: f64, expected: f64) {
        let tolerance = ATOL + RTOL * expected.abs();
        assert!(
            expected.is_finite()
                && actual.is_finite()
                && tolerance.is_finite()
                && (actual - expected).abs() <= tolerance,
            "seed={:#018x}, {self:?}, prefix={index}, {label}: \
             expected={expected:?}, actual={actual:?}, tolerance={tolerance:?}",
            self.seed,
        );
    }

    fn reference_sum(&self, index: usize, end: usize) -> f64 {
        if self.params.beta == 0.0 {
            return end as f64;
        }
        let mut sum = 0.0;
        let mut correction = 0.0;
        for &event in &self.history[..end] {
            let gap = self.history[index] - event;
            let exponent = -self.params.beta * gap;
            assert!(
                gap.is_finite() && exponent.is_finite(),
                "{self:?}, prefix={index}: expected finite reference intermediates, \
                 actual gap={gap:?}, exponent={exponent:?}"
            );
            let term = exponent.exp() - correction;
            let next = sum + term;
            correction = (next - sum) - term;
            sum = next;
        }
        assert!(
            sum.is_finite() && correction.is_finite(),
            "{self:?}, prefix={index}: expected finite reference accumulation, \
             actual sum={sum:?}, correction={correction:?}"
        );
        sum
    }

    fn check(&self) {
        let p = self.params;
        assert!(
            self.history.len() <= MAX_EVENTS && (p.mu + p.alpha * MAX_EVENTS as f64).is_finite(),
            "{self:?}: expected <= {MAX_EVENTS} events and finite upper intensity bound, \
             actual count={}, bound={:?}",
            self.history.len(),
            p.mu + p.alpha * MAX_EVENTS as f64,
        );
        let empty = compute_hawkes(&[], p);
        assert_eq!(empty.event_count, 0, "{self:?}, empty prefix");
        self.close(0, "empty intensity", empty.intensity, p.mu);
        self.close(0, "empty average", empty.avg_excitation, 0.0);

        let mut state = 0.0;
        let mut last = self.history.first().copied().unwrap_or(0.0);
        let mut previous_intensity = p.mu;
        for (index, &time) in self.history.iter().enumerate() {
            assert!(
                time.is_finite() && time >= last,
                "{self:?}, prefix={index}: expected finite nondecreasing time >= {last:?}, \
                 actual={time:?}"
            );
            let pre_sum = self.reference_sum(index, index);
            let post_sum = self.reference_sum(index, index + 1);
            let expected_pre = p.mu + p.alpha * pre_sum;
            let expected_post = p.mu + p.alpha * post_sum;
            let (pre, new_sum) = compute_hawkes_streaming(previous_intensity, time, last, p, state);
            let post = p.mu + p.alpha * new_sum;
            let batch = compute_hawkes(&self.history[..=index], p);

            assert_eq!(batch.event_count, index + 1, "{self:?}, prefix={index}");
            self.close(index, "stream pre-jump", pre, expected_pre);
            self.close(index, "stream decay state", new_sum, post_sum);
            self.close(index, "stream post-event", post, expected_post);
            self.close(index, "batch post-event", batch.intensity, expected_post);
            self.close(index, "batch vs stream post-event", batch.intensity, post);
            self.close(
                index,
                "batch average excitation",
                batch.avg_excitation,
                p.alpha * post_sum / (index + 1) as f64,
            );
            if p.beta == 0.0 {
                assert_eq!(new_sum, (index + 1) as f64, "{self:?}, prefix={index}");
            }
            state = new_sum;
            last = time;
            previous_intensity = pre;
        }
    }
}

#[test]
fn hawkes_generated_prefixes_match_definition() {
    let regimes = [
        (0.0, 1.0),
        (-0.0, 1e300),
        (1e-12, 1e-12),
        (1e-12, 1e12),
        (1.0, 1.0),
        (1.0, 1e300),
        (1e12, 1e-12),
        (f64::MIN_POSITIVE, 1e300),
        (f64::MAX, f64::MIN_POSITIVE),
    ];
    let amplitudes = [
        (0.0, 0.0),
        (0.0, 0.75),
        (0.125, 0.875),
        (1e-200, 3e-200),
        (f64::MIN_POSITIVE, f64::MIN_POSITIVE),
        (0.0, 1e-310),
        (f64::MAX / 8.0, f64::MAX / (4 * MAX_EVENTS) as f64),
        (f64::MAX, 0.0),
    ];
    for seed in SEEDS {
        for (beta, scale) in regimes {
            for count in [0, 1, 2, 3, 17, 64, MAX_EVENTS] {
                let history = generated_history(seed, count, scale);
                for (mu, alpha) in amplitudes {
                    let params = HawkesParams {
                        mu,
                        alpha,
                        beta,
                        dt: 0.001,
                    };
                    Case {
                        seed,
                        params: &params,
                        history: &history,
                    }
                    .check();
                }
            }
        }
    }
}

#[test]
fn hawkes_ties_follow_prefix_order_not_strict_timestamp_order() {
    let params = HawkesParams {
        mu: 0.25,
        alpha: 2.0,
        beta: std::f64::consts::LN_2,
        dt: 0.001,
    };
    let case = Case {
        seed: 0,
        params: &params,
        history: &[-1.0, -1.0, 0.0, 0.0, 2.0],
    };
    // Unit gaps halve each prior contribution; a two-unit gap quarters it.
    let pre = [0.25, 2.25, 2.25, 4.25, 1.75];
    let post = [2.25, 4.25, 4.25, 6.25, 3.75];
    for index in 0..case.history.len() {
        case.close(
            index,
            "analytical pre-jump oracle",
            params.mu + params.alpha * case.reference_sum(index, index),
            pre[index],
        );
        case.close(
            index,
            "analytical post-event oracle",
            params.mu + params.alpha * case.reference_sum(index, index + 1),
            post[index],
        );
    }
    case.check();
}

#[test]
fn hawkes_decay_underflow_discards_prior_events_but_not_current_ties() {
    let params = HawkesParams {
        mu: 0.125,
        alpha: 0.75,
        beta: 1.0,
        dt: 0.001,
    };
    let case = Case {
        seed: 0,
        params: &params,
        history: &[0.0, 800.0, 800.0, 1600.0],
    };
    // exp(-800) rounds to zero in f64; tied events retain a factor of one.
    for (index, expected) in [0.0, 0.0, 1.0, 0.0].into_iter().enumerate() {
        assert_eq!(
            case.reference_sum(index, index),
            expected,
            "{case:?}, prefix={index}"
        );
    }
    case.check();
}

#[test]
fn hawkes_signed_zero_decay_analytical_even_for_overflowing_time_span() {
    for beta in [0.0, -0.0] {
        let params = HawkesParams {
            mu: 0.125,
            alpha: 0.875,
            beta,
            dt: 0.001,
        };
        Case {
            seed: 0,
            params: &params,
            history: &[-f64::MAX, -f64::MAX, f64::MAX, f64::MAX],
        }
        .check();
    }
}
