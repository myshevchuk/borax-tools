# Tasks: stream-per-file-events

TDD throughout: every behaviour lands as a failing test first, then the
implementation, then refactor with the suite green. The three parts are
independent and land in this order because each is green on its own.

## 1. Incremental planning (borax-core)

- [x] 1.1 Test: a `Planner` driven one input at a time produces, for
      every case the batch tests cover, exactly what `plan` produces for
      the same inputs as a batch — collisions, suffix ladder,
      case-insensitive folding, `AlreadyNamed` by twin, the source
      exemption
- [x] 1.2 Implement `Planner::new` / `Planner::plan`, holding the
      `claimed` set and the `existing` snapshot
- [ ] 1.3 Refactor `plan(items, existing, policy)` into a fold over
      `Planner::plan`, so the batch entry point cannot drift from the
      incremental one

## 2. The event sink (borax)

- [ ] 2.1 Test: `dispatch` writes a run's lines in event order, and a
      run whose events are produced slowly writes them without waiting
      for the last (asserted through a `Streams.out` that records the
      order writes arrive in relative to the adapters being called)
- [ ] 2.2 Test: a run ended by a `Diagnostic` writes neither
      `run-started` nor `run-finished` to stdout, for each of the three
      fatal cases — uncompilable template, `--apply` without a journal,
      `cache` with no cache directory
- [ ] 2.3 Add `Sink` with the `Vec<Event>` implementation, and the
      rendering-and-counting sink `dispatch` uses
- [ ] 2.4 Split `preflight` (fallible, produces `Prepared`) from
      `emit_events` (infallible, writes into a sink); keep `events_for`
      as the wrapper that preflights and collects, so
      `tests/dispatch.rs` is unchanged
- [ ] 2.5 Rewrite `dispatch` to emit `RunStarted`, stream the body, and
      emit `RunFinished` from the counts the sink accumulated

## 3. Per-file rename reporting (borax)

- [ ] 3.1 Test: a rename run over a directory of files that variously
      resolve, skip, collide and are already named emits each file's
      events contiguously and in input order
- [ ] 3.2 Test: the same batch produces the same names, suffixes and
      journal entries as before the change — the ordering assertion of
      3.1 must not be able to pass by changing a decision
- [ ] 3.3 Test: a run spanning a directory and its subdirectory reports
      each file under its own configuration, with no interleaving
      between the two groups' files
- [ ] 3.4 Rewrite `rename_events` as a per-directory loop holding a
      `Planner` and the directory snapshot, resolving, planning,
      applying and journaling one file at a time
- [ ] 3.5 Make the subdirectory listing lazy and memoized per group,
      replacing the eager whole-batch scan in `occupied`
- [ ] 3.6 Split `write_bib`: a per-file sidecar write called from the
      loop, and the master merge left at the end of the group
- [ ] 3.7 Apply the same per-file loop to `bib_events`, whose two-phase
      shape is the same one

## 4. Wrap-up

- [ ] 4.1 Run the suite on Linux, macOS and Windows via CI
- [ ] 4.2 Verify against the local real-PDF corpus that a rename run
      over a nested tree reads as one file per block
- [ ] 4.3 `CHANGELOG.md`: a `Changed` bullet under `[Unreleased]` for
      the reporting order and liveness
- [ ] 4.4 Update `openspec/STATE.md` if this change alters anything it
      asserts, then archive the change
