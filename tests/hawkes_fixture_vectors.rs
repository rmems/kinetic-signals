// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hawkes golden-vector parity checks (issue #28 / LIM-201).

mod common;

use common::{
    BoundCtx, CloseCheck, OutputRangeCtx, SeriesCheck, SizeCtx, assert_close,
    assert_field_in_output_range, assert_series, f64s, fixture, params_from_json,
    parse_size_contract, require_f64, tol,
};
use kinetic_signals::{compute_hawkes, compute_hawkes_streaming, hawkes::HawkesParams};

struct HawkesWalk {
    intensities: Vec<f64>,
    decay_sums: Vec<f64>,
}

/// Walk streaming Hawkes over `events`.
///
/// `initial_last_event_time`: when `Some`, used as `last_event_time` for the first
/// step (stream resume). When `None`, the first event is seeded with dt=0.
fn walk_hawkes_streaming(
    events: &[f64],
    params: &HawkesParams,
    initial_decay_sum: f64,
    initial_last_event_time: Option<f64>,
) -> HawkesWalk {
    assert!(!events.is_empty());
    let mut decay_sum = initial_decay_sum;
    let mut last = initial_last_event_time.unwrap_or(events[0]);
    let mut intensities = Vec::with_capacity(events.len());
    let mut decay_sums = Vec::with_capacity(events.len());
    for &t in events {
        let (intensity, new_decay) = compute_hawkes_streaming(0.0, t, last, params, decay_sum);
        intensities.push(intensity);
        decay_sums.push(new_decay);
        decay_sum = new_decay;
        last = t;
    }
    HawkesWalk {
        intensities,
        decay_sums,
    }
}

fn assert_hawkes_streaming_step_fixture(vector_key: &str) {
    let root = fixture();
    let v = &root["vectors"][vector_key];
    let tolerance = tol(v);
    let input = &v["input"];
    let params = params_from_json(&input["params"]);
    let (intensity, new_decay) = compute_hawkes_streaming(
        require_f64(input, "prev_intensity"),
        require_f64(input, "new_event_time"),
        require_f64(input, "last_event_time"),
        &params,
        require_f64(input, "decay_sum"),
    );
    let exp = &v["expected"];
    assert_close(CloseCheck {
        label: "intensity",
        got: intensity,
        expected: require_f64(exp, "intensity"),
        tolerance,
    });
    assert_close(CloseCheck {
        label: "new_decay_sum",
        got: new_decay,
        expected: require_f64(exp, "new_decay_sum"),
        tolerance,
    });
    let post_got = params.mu + params.alpha * new_decay;
    assert_close(CloseCheck {
        label: "post_event_intensity",
        got: post_got,
        expected: require_f64(exp, "post_event_intensity"),
        tolerance,
    });
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &BoundCtx {
            mu: Some(params.mu),
            bins: None,
        },
        tolerance,
    };
    assert_field_in_output_range(&rc, "intensity", intensity);
    assert_field_in_output_range(&rc, "new_decay_sum", new_decay);
}

fn resume_last_event_time(
    vector_key: &str,
    input: &serde_json::Value,
    initial_decay: f64,
) -> Option<f64> {
    match input.get("initial_last_event_time") {
        Some(_) => Some(require_f64(input, "initial_last_event_time")),
        None if initial_decay != 0.0 => {
            panic!(
                "vector `{vector_key}`: nonzero initial_decay_sum requires initial_last_event_time"
            );
        }
        None => None,
    }
}

fn assert_hawkes_walk_goldens(
    walk: &HawkesWalk,
    exp: &serde_json::Value,
    events: &[f64],
    params: &HawkesParams,
    initial_decay: f64,
    tolerance: f64,
) {
    assert_series(SeriesCheck {
        label: "intensity",
        got: &walk.intensities,
        expected: &f64s(&exp["intensities"]),
        tolerance,
    });
    assert_series(SeriesCheck {
        label: "decay_sum",
        got: &walk.decay_sums,
        expected: &f64s(&exp["decay_sums"]),
        tolerance,
    });
    let post = params.mu + params.alpha * walk.decay_sums.last().copied().unwrap_or(0.0);
    if initial_decay == 0.0 {
        let batch = compute_hawkes(events, params);
        assert_close(CloseCheck {
            label: "batch_vs_stream_post",
            got: post,
            expected: batch.intensity,
            tolerance,
        });
    }
    assert_close(CloseCheck {
        label: "post_event_final",
        got: post,
        expected: require_f64(exp, "post_event_final_intensity"),
        tolerance,
    });
}

fn assert_hawkes_sequence_fixture(vector_key: &str) {
    let root = fixture();
    let v = &root["vectors"][vector_key];
    let tolerance = tol(v);
    let input = &v["input"];
    let events = f64s(&input["event_times"]);
    let params = params_from_json(&input["params"]);
    let initial_decay = require_f64(input, "initial_decay_sum");
    let initial_last = resume_last_event_time(vector_key, input, initial_decay);
    let walk = walk_hawkes_streaming(&events, &params, initial_decay, initial_last);
    let expected_len = parse_size_contract(
        &v["output_range"]["length"],
        &SizeCtx {
            event_times: Some(events.len()),
            values: None,
            data: None,
        },
    );
    assert_eq!(walk.intensities.len(), expected_len);
    assert_eq!(walk.decay_sums.len(), expected_len);
    assert_hawkes_walk_goldens(
        &walk,
        &v["expected"],
        &events,
        &params,
        initial_decay,
        tolerance,
    );
}

#[test]
fn hawkes_intensity_at_least_baseline() {
    let root = fixture();
    let v = &root["vectors"]["hawkes"];
    let events = f64s(&v["input"]["event_times"]);
    let params = params_from_json(&v["input"]["params"]);
    let r = compute_hawkes(&events, &params);
    assert!(r.intensity.is_finite());
    let ctx = BoundCtx {
        mu: Some(params.mu),
        bins: None,
    };
    let rc = OutputRangeCtx {
        ranges: &v["output_range"],
        bounds: &ctx,
        tolerance: tol(v),
    };
    assert_field_in_output_range(&rc, "intensity", r.intensity);
    assert_field_in_output_range(&rc, "avg_excitation", r.avg_excitation);
    assert_eq!(r.event_count, events.len());
}

#[test]
fn hawkes_streaming_single_step_matches_fixture() {
    assert_hawkes_streaming_step_fixture("hawkes_streaming");
}

#[test]
fn hawkes_streaming_nondefault_matches_fixture() {
    assert_hawkes_streaming_step_fixture("hawkes_streaming_nondefault");
}

#[test]
fn hawkes_streaming_sequence_matches_fixture() {
    assert_hawkes_sequence_fixture("hawkes_streaming_sequence");
}

#[test]
fn hawkes_streaming_sequence_resume_matches_fixture() {
    assert_hawkes_sequence_fixture("hawkes_streaming_sequence_resume");
}
