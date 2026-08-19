# Design: stream-per-file-events

## Context

`run.rs` builds a run's output as a value. `events_for` returns
`Result<Vec<Event>, Diagnostic>`, and `dispatch` renders that vector
after the fact. Everything about the current reporting follows from
that shape: a run cannot say anything until it can say everything, and
the order of the vector is whatever order was convenient to build it in
— which, for `rename`, is phase by phase inside each directory group.

Two properties are worth keeping while changing this.

**A fatal run emits no events at all.** `dispatch` documents that the
stream always opens with `RunStarted` and closes with `RunFinished`, so
a JSON consumer can tell a run that found nothing from a run that was
cut off. Today that holds trivially: a `Diagnostic` short-circuits
before the first event is written.

**Nothing is touched before every template compiles.** `rename_events`
compiles the template table for every directory the run spans before it
resolves a single file, so a template that will not compile costs no
network and moves no file. Today that is a comment and an ordering
inside one function.

## Decisions

### A sink, and `events_for` kept as a collecting wrapper

Commands write into `&mut dyn Sink` instead of returning a `Vec`:

```rust
pub trait Sink {
    fn emit(&mut self, event: Event);
}

impl Sink for Vec<Event> { … }
```

`dispatch` passes a sink that renders each event, writes the line, and
folds it into the run's `Counts`. `StdoutLock` is a `LineWriter`, so a
`writeln!` per event reaches the terminal without an explicit flush;
liveness costs nothing but the ordering.

`events_for` stays, as the two-line wrapper that preflights and then
collects into a `Vec`. That is what the ~20 assertions in
`tests/dispatch.rs` are written against, and a vector is the right
shape for asserting on a whole run's output — including, now, its
order. The streaming path and the collecting path run the same code, so
neither can drift from the other.

### Preflight, so the fatal-before-first-event invariant is structural

Streaming means `dispatch` must write `RunStarted` before it knows
whether the run will fail. Rather than weaken the framing contract —
an unterminated stream on stdout, or a `RunFinished` that reports a
fatal run as an empty one — the checks that can be fatal move ahead of
the stream:

```rust
fn preflight(command, configs, adapters) -> Result<Prepared, Diagnostic>
fn emit_events(prepared, command, configs, adapters, sink)
```

`Prepared` carries what the checks produced: the compiled template
table per directory group for `rename` and `bib`, the journal `--apply`
demanded, the cache root `cache` needs. Every command that can fail
does so here, and `emit_events` is infallible — which is the only way
"a `Diagnostic` means nothing was emitted" survives as a guarantee
rather than a habit.

The compile-everything-first rule becomes the type: `emit_events`
cannot reach a file without already holding the template table for its
directory.

### An incremental planner in `borax-core`

`plan` already threads a `claimed: BTreeSet<String>` across the batch —
the set that makes preview agree with `--apply` about which file gets
the unsuffixed name. Per-file reporting needs that state to outlive a
single call, so it becomes the thing the caller holds:

```rust
pub struct Planner { claimed: BTreeSet<String>, existing: BTreeMap<String, Option<String>> }
impl Planner {
    pub fn new(existing: BTreeMap<String, Option<String>>) -> Self;
    pub fn plan(&mut self, item: &PlanInput, policy: CollisionPolicy) -> PlanItem;
}
```

`plan(items, existing, policy)` becomes a fold over `Planner::plan`, so
the existing tests cover the incremental path too and any divergence
between them is a compile error rather than a behaviour difference.

### The directory snapshot stays eager; subdirectory listings go lazy

`occupied` currently scans the file's own directory and every
subdirectory the *batch's* targets reach into, before planning
anything. Per file, the own-directory scan still happens once per group
— it hashes every file there, and doing that per PDF would be
quadratic — while a subdirectory is listed the first time a target
names it, memoized for the rest of the group.

This is the one place laziness could have changed a decision, and it
does not: `claimed` gains `S/…` keys later than it used to, and the
only file whose planning could consult them is one whose own target
lies in `S`, which is exactly the file that triggers the listing.

### Sidecars per file, the master `.bib` per group

`write_bib` does two independent things: a per-file sidecar write, and
a read-modify-write merge into the master `.bib`. The first is per-file
work and moves into the per-file loop. The second stays once per
directory group — merging per PDF would rewrite the whole master file
`n` times, and the merge's own duplicate-key suffixing wants to see the
batch.

So a group's stream is: one contiguous block of lines per file, then
the master-bibliography lines. With both bib destinations unconfigured
— the default — the second part is empty and a group is nothing but
its files.

## Risks

- **`rename` stays serial.** It is serial today, so this is no
  regression, but per-file live reporting and concurrent resolution
  pull against each other, and choosing liveness here forecloses the
  obvious way to make `rename` honour `concurrency`. Recovering it
  later means either reporting out of order or buffering completions to
  restore input order — a real design question, deferred deliberately.
- **`RunStarted` moves earlier in wall-clock terms** for JSON
  consumers. Its position in the stream, and the guarantee that a fatal
  run produces neither it nor `RunFinished`, are unchanged.
