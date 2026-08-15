//! Being a good citizen: how long to wait, and how many at once.
//!
//! borax queries services that answer for free. Both halves of this
//! module exist to keep a batch of files from looking like an attack:
//! requests to one service are spaced out, and the whole run is capped
//! at a fixed number of threads in flight.

use std::time::Duration;

/// The gap borax leaves between two requests to the same service.
///
/// Crossref's polite pool asks for roughly this; the other services
/// publish no figure, and this is well inside what any of them will
/// tolerate.
pub const DEFAULT_MIN_INTERVAL: Duration = Duration::from_millis(100);

/// How many requests may be in flight across the whole run.
///
/// Bounded because the alternative — one thread per file — turns a
/// directory of five hundred PDFs into five hundred simultaneous
/// connections.
pub const DEFAULT_CONCURRENCY: usize = 4;

/// How long to wait before issuing a request, given when the last one
/// to the same service went out.
///
/// `Duration::ZERO` when no request has been made yet, or when at
/// least `min_interval` has already elapsed. Pure arithmetic — the
/// waiting itself belongs to the caller, which is what makes the
/// policy testable without a clock.
pub fn delay_before(since_last: Option<Duration>, min_interval: Duration) -> Duration {
    let _ = (since_last, min_interval);
    todo!("compute the delay before the next request")
}

/// Run `job` over every input on at most `concurrency` threads,
/// returning the results in the order the inputs were given.
///
/// Order is restored after the fact, so results never depend on which
/// thread finished first — the pipeline's determinism requirement
/// reaches into its concurrency. `concurrency` is clamped to at least
/// one, and an empty input runs nothing.
///
/// A panic in `job` propagates once every thread has been joined,
/// rather than leaving the run half-finished with no report.
pub fn map_bounded<I, O, F>(inputs: Vec<I>, concurrency: usize, job: F) -> Vec<O>
where
    I: Send,
    O: Send,
    F: Fn(I) -> O + Sync,
{
    let _ = (inputs, concurrency, job);
    todo!("run the jobs on a bounded pool, preserving input order")
}
