#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use borax_core::identifier::{Doi, Identifier};
use borax_core::record::{EntryType, Record};
use borax_sources::pace::{
    DEFAULT_CONCURRENCY, DEFAULT_MIN_INTERVAL, Paced, delay_before, map_bounded,
};
use borax_sources::source::{Source, SourceError, SourceName};

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

// ---------------------------------------------------------------------
// Paced
// ---------------------------------------------------------------------

/// A [`Source`] that answers instantly and counts its calls.
struct InstantSource {
    calls: std::sync::atomic::AtomicUsize,
}

impl InstantSource {
    fn new() -> InstantSource {
        InstantSource {
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl Source for InstantSource {
    fn name(&self) -> SourceName {
        SourceName::Crossref
    }

    fn supports(&self, _identifier: &Identifier) -> bool {
        true
    }

    fn fetch(&self, _identifier: &Identifier) -> Result<Record, SourceError> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Record::new(EntryType::Article))
    }
}

fn an_identifier() -> Identifier {
    Identifier::Doi(Doi::parse("10.1000/paced").unwrap())
}

#[test]
fn the_first_request_is_not_delayed() {
    let paced = Paced::new(InstantSource::new(), Duration::from_millis(200));

    let started = Instant::now();
    paced.fetch(&an_identifier()).unwrap();

    assert!(
        started.elapsed() < Duration::from_millis(200),
        "nothing has been asked yet, so there is nothing to wait for"
    );
}

#[test]
fn a_second_request_waits_out_the_interval() {
    let interval = Duration::from_millis(120);
    let paced = Paced::new(InstantSource::new(), interval);

    let started = Instant::now();
    paced.fetch(&an_identifier()).unwrap();
    paced.fetch(&an_identifier()).unwrap();

    assert!(
        started.elapsed() >= interval,
        "two requests to one service must be at least {interval:?} apart, got {:?}",
        started.elapsed()
    );
}

#[test]
fn pacing_delays_rather_than_dropping_requests() {
    let source = InstantSource::new();
    let paced = Paced::new(source, Duration::from_millis(1));

    for _ in 0..3 {
        paced.fetch(&an_identifier()).unwrap();
    }

    assert_eq!(paced.source().calls(), 3);
}

#[test]
fn a_zero_interval_paces_nothing() {
    let paced = Paced::new(InstantSource::new(), Duration::ZERO);

    let started = Instant::now();
    for _ in 0..5 {
        paced.fetch(&an_identifier()).unwrap();
    }

    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn the_wrapped_source_keeps_its_name_and_support_answer() {
    let paced = Paced::new(InstantSource::new(), Duration::ZERO);

    assert_eq!(paced.name(), SourceName::Crossref);
    assert!(paced.supports(&an_identifier()));
}
