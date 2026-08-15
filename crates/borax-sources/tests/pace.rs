#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use borax_sources::pace::{DEFAULT_CONCURRENCY, DEFAULT_MIN_INTERVAL, delay_before, map_bounded};

// --- constants ---

#[test]
fn default_min_interval_is_one_hundred_milliseconds() {
    assert_eq!(DEFAULT_MIN_INTERVAL, Duration::from_millis(100));
}

#[test]
fn default_concurrency_is_four() {
    assert_eq!(DEFAULT_CONCURRENCY, 4);
}

// --- delay_before() ---

#[test]
fn delay_before_first_request_is_zero() {
    assert_eq!(
        delay_before(None, Duration::from_millis(100)),
        Duration::ZERO
    );
}

#[test]
fn delay_before_is_zero_once_the_interval_has_elapsed() {
    let min_interval = Duration::from_millis(100);

    assert_eq!(
        delay_before(Some(min_interval), min_interval),
        Duration::ZERO
    );
    assert_eq!(
        delay_before(Some(Duration::from_millis(150)), min_interval),
        Duration::ZERO
    );
}

#[test]
fn delay_before_waits_out_the_remainder_of_the_interval() {
    let delay = delay_before(Some(Duration::from_millis(30)), Duration::from_millis(100));
    assert_eq!(delay, Duration::from_millis(70));
}

#[test]
fn delay_before_with_no_elapsed_time_waits_the_full_interval() {
    let delay = delay_before(Some(Duration::ZERO), Duration::from_millis(100));
    assert_eq!(delay, Duration::from_millis(100));
}

#[test]
fn delay_before_is_always_zero_when_min_interval_is_zero() {
    assert_eq!(delay_before(None, Duration::ZERO), Duration::ZERO);
    assert_eq!(
        delay_before(Some(Duration::ZERO), Duration::ZERO),
        Duration::ZERO
    );
    assert_eq!(
        delay_before(Some(Duration::from_millis(50)), Duration::ZERO),
        Duration::ZERO
    );
}

// --- map_bounded() ---

#[test]
fn map_bounded_on_empty_input_runs_nothing() {
    let ran = AtomicUsize::new(0);

    let result: Vec<i32> = map_bounded(Vec::<i32>::new(), 4, |i| {
        ran.fetch_add(1, Ordering::SeqCst);
        i
    });

    assert!(result.is_empty());
    assert_eq!(ran.load(Ordering::SeqCst), 0);
}

#[test]
fn map_bounded_preserves_input_order_regardless_of_completion_order() {
    let inputs: Vec<usize> = (0..20).collect();

    // Earlier indices sleep longer, so a job-order-dependent
    // implementation would return results out of order.
    let result = map_bounded(inputs, 4, |i| {
        thread::sleep(Duration::from_millis((20 - i) as u64 * 2));
        i * 10
    });

    let expected: Vec<usize> = (0..20).map(|i| i * 10).collect();
    assert_eq!(result, expected);
}

#[test]
fn map_bounded_processes_every_input_exactly_once() {
    let processed = AtomicUsize::new(0);
    let inputs: Vec<usize> = (0..20).collect();
    let count = inputs.len();

    map_bounded(inputs, 4, |_i| {
        processed.fetch_add(1, Ordering::SeqCst);
    });

    assert_eq!(processed.load(Ordering::SeqCst), count);
}

#[test]
fn map_bounded_bounds_concurrency_at_the_requested_limit() {
    let live = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let inputs: Vec<usize> = (0..20).collect();

    map_bounded(inputs, 3, |_i| {
        let now = live.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(20));
        live.fetch_sub(1, Ordering::SeqCst);
    });

    assert!(peak.load(Ordering::SeqCst) <= 3);
}

#[test]
fn map_bounded_runs_more_than_one_job_at_a_time_when_concurrency_allows() {
    let live = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let inputs: Vec<usize> = (0..20).collect();

    map_bounded(inputs, 4, |_i| {
        let now = live.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(20));
        live.fetch_sub(1, Ordering::SeqCst);
    });

    assert!(peak.load(Ordering::SeqCst) > 1);
}

#[test]
fn map_bounded_clamps_zero_concurrency_to_at_least_one() {
    let inputs: Vec<usize> = (0..5).collect();

    let result = map_bounded(inputs, 0, |i| i * 2);

    assert_eq!(result, vec![0, 2, 4, 6, 8]);
}

#[test]
fn map_bounded_with_concurrency_one_runs_serially() {
    let live = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let inputs: Vec<usize> = (0..5).collect();

    let result = map_bounded(inputs, 1, |i| {
        let now = live.fetch_add(1, Ordering::SeqCst) + 1;
        peak.fetch_max(now, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(5));
        live.fetch_sub(1, Ordering::SeqCst);
        i * 2
    });

    assert_eq!(result, vec![0, 2, 4, 6, 8]);
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

#[test]
fn map_bounded_with_concurrency_greater_than_input_count_still_works() {
    let inputs: Vec<usize> = (0..3).collect();

    let result = map_bounded(inputs, 100, |i| i + 1);

    assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn map_bounded_supports_a_non_copy_output_type() {
    let inputs: Vec<usize> = (0..5).collect();

    let result = map_bounded(inputs, 3, |i| format!("job-{i}"));

    let expected: Vec<String> = (0..5).map(|i| format!("job-{i}")).collect();
    assert_eq!(result, expected);
}
