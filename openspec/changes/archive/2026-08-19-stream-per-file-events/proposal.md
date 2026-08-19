# Proposal: stream-per-file-events

## Why

A `borax rename` run over a real library is unreadable. Two things make
it so, and they compound.

The run reports in phases. `rename_events` groups the inputs by parent
directory and, for each group, resolves *every* file before planning
*any* rename — so the output is a block of resolve lines followed by a
block of rename lines, per directory. The blocks do not line up: the
second one omits the files that were skipped, and with subdirectories in
play the two kinds of block alternate down the run. Nothing here is
non-deterministic — two runs over the same corpus produce byte-identical
output — but a reader cannot pair a file's verdict with its fate without
scrolling and searching, which on a corpus of 149 PDFs is the same as not
being told.

The run also says nothing until it is over. `dispatch` collects the whole
event stream into a `Vec<Event>` and renders it only after `events_for`
returns. On a warm cache that is invisible. On a cold one it is minutes of
silence, and a user watching a network-bound run has no way to tell
progress from a hang.

Both are the same complaint: the output is a transcript assembled after
the fact rather than a report of what is happening.

## What Changes

- **Report per file, not per phase.** A rename run interleaves resolve
  and rename for each file in turn — resolve, plan, move, next — so
  every file's lines are adjacent and in input order.
- **Emit events as they happen.** `events_for` writes into a sink
  instead of returning a `Vec`, so each event reaches stdout when it
  occurs rather than when the run ends. The counts behind
  `Event::RunFinished` and the exit code are accumulated as events pass
  through.
- **Sidecar output joins its file.** A sidecar write is per-file work
  and is reported with the file it belongs to. The master `.bib` merge
  stays once per directory group: it is a read-modify-write of the whole
  file, and doing it per PDF would cost O(n²) bytes over a batch.
- Planning becomes incremental. `borax_core::rename::plan` already
  carries a `claimed` set across a batch; that state is exposed as a
  planner a caller drives one input at a time, and the batch entry point
  becomes a loop over it. Every decision it makes is unchanged —
  collision suffixes stay deterministic and preview stays identical to
  what `--apply` would do.

No event schema changes and no new fields: this changes the order events
are emitted in and the moment they are written, not what they say.

## Capabilities

### Modified Capabilities

- `cli`: the event stream gains an emission-order and liveness contract —
  events are written as they occur, and a file's events are contiguous.
- `rename`: a run's unit of work is one file rather than one batch phase,
  stated in a way that pins the ordering without weakening the existing
  collision guarantees.

## Out of scope

- Concurrent resolution during `rename`. `rename` resolves serially
  today; `resolve` honours `concurrency` and `rename` does not. Making
  rename concurrent is in tension with live per-file reporting and is a
  separate decision.
- Any change to the master `.bib` merge strategy.
- Progress indicators, spinners, or TTY detection. Liveness here means
  events are written when they happen, nothing more.
