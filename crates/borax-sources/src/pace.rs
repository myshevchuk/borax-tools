//! Being a good citizen: how long to wait, and how many at once.
//!
//! borax queries services that answer for free. Both halves of this
//! module exist to keep a batch of files from looking like an attack:
//! requests to one service are spaced out, and the whole run is capped
//! at a fixed number of threads in flight.

use std::sync::{Mutex, PoisonError};
use std::thread;
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
    match since_last {
        None => Duration::ZERO,
        Some(elapsed) => min_interval.saturating_sub(elapsed),
    }
}

/// The inputs still to be run, each paired with the position its output
/// belongs at.
///
/// Shared behind a mutex: handing out the position with the input is
/// what lets a worker write its result back where the caller expects it,
/// whatever order the workers finish in.
struct Queue<I> {
    next: usize,
    inputs: std::vec::IntoIter<I>,
}

impl<I> Queue<I> {
    /// The next input and its position, or `None` once every input has
    /// been handed out. Each input is handed out exactly once.
    fn take(&mut self) -> Option<(usize, I)> {
        let input = self.inputs.next()?;
        let index = self.next;
        self.next += 1;
        Some((index, input))
    }
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
    let count = inputs.len();
    if count == 0 {
        return Vec::new();
    }
    let workers = concurrency.clamp(1, count);

    let queue = Mutex::new(Queue {
        next: 0,
        inputs: inputs.into_iter(),
    });
    // One slot per input, filled by position rather than by completion
    // order. A job runs with no lock held, so a panicking job cannot
    // leave either mutex poisoned mid-update; where a poisoned lock is
    // conceivable the guard is taken anyway, since neither structure can
    // be observed in a torn state.
    let slots: Mutex<Vec<Option<O>>> = Mutex::new((0..count).map(|_| None).collect());

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().unwrap_or_else(PoisonError::into_inner).take();
                    let Some((index, input)) = next else {
                        break;
                    };

                    let output = job(input);

                    if let Some(slot) = slots
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get_mut(index)
                    {
                        *slot = Some(output);
                    }
                }
            });
        }
    });

    slots
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .flatten()
        .collect()
}
