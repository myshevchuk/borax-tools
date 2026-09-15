## Why

`borax rename` is a batch tool: it resolves every file, prints what it
would do, and waits for a second invocation with `--apply` to do it.
That is the right shape for a homogeneous directory and the wrong one
for the collections borax actually meets. A folder of papers on one
topic is still a mix of publisher PDFs, free preprints, author
manuscripts and supplementary files, and in practice roughly half of a
real folder renames on the first pass. The rest need a person to look,
and the batch shape makes the person work from a report after the run
rather than at the file while the run is looking at it.

Every remedy for that half is interactive by nature — confirming a
rename with the target in view, supplying the identifier a preprint does
not print, accepting a record whose title the file spells differently —
and each needs a session that can stop at a file and ask. None exists:
`session.rs` carries a tested `interaction`/`confirm` pair that nothing
calls, and the `add-core-pipeline` design named an interactive picker as
the later change it would take.

This change builds that session and makes it what `borax rename` does on
a terminal. It is the first of four changes under one umbrella, and the
only one the other three depend on:

1. `add-interactive-rename` (this change) — the session, the mode
   switch, and a question per rename.
2. `skip-already-named-files` — files that already carry their name are
   passed over without a question and stop counting as skips.
3. `show-record-before-asking` — each question shows the identifier,
   where it was found, the record fetched for it and the service that
   supplied it.
4. `supply-identifiers-interactively` — a file borax could not identify,
   or whose record disagrees with it, can be given an identifier or
   accepted by hand.

## What Changes

- **`borax rename` asks on a terminal.** When stdin is a terminal and
  output is human, a rename run is interactive: each file whose record
  names a free target is put to the operator as a question — rename,
  skip, or quit — and a rename answered yes is carried out on the spot.
- **Batch is behind a flag.** `--batch` gives the run as it is today, a
  preview. `--apply` is a batch-mode flag and selects batch by itself,
  so `borax rename --apply <dir>` does exactly what it did before.
  `--apply --no-batch` is a usage error. A new `rename.batch` setting
  makes batch the default for a user or a collection that wants it;
  configuration can make a run safer and never make it move a file.
- **Scripts are untouched.** A run whose stdin is not a terminal, or
  that asks for `--json`, is a batch run whatever the setting says, as
  the `cli` capability already requires of any run that would prompt.
- **A declined rename is a skip, and its name stays free.** Declining
  leaves the file as it is, reports it skipped with reason `declined`,
  and claims no name, so the file after it is not suffixed on account of
  a move that never happened.
- **Quitting ends the run cleanly.** Files not yet reached are left
  untouched and counted as unreached; the run closes with `run-finished`
  as any run does, and exits with the partial-success code.
- **An interactive run is an applying run for its log.** It can move
  files, so its log is mandatory and is named and framed as an applying
  run's, exactly as `--apply`'s is.

## Capabilities

### Modified Capabilities

- `rename`: the preview-by-default requirement is restated as "nothing
  moves without an explicit decision", of which `--apply` is one and a
  yes to a question naming the target is the other; new requirements
  for choosing the mode, for the question, and for quitting.
- `cli`: `rename` and `config` accept the `--batch` / `--no-batch` pair.
- `run-logs`: an interactive rename run is logged as an applying run.

## Impact

- `crates/borax-core/src/rename.rs`: `Planner` separates deciding a
  target from claiming it, so a proposal the operator declines leaves
  the namespace as it found it. Batch planning is the two in sequence
  and is unchanged.
- `crates/borax/src/session.rs`: the mode decision becomes a pure
  function of the terminal, the output format, the `batch` setting and
  `--apply`; an `Asker` trait carries the question, with an `inquire`
  adapter for terminals and a scripted one for tests. The unused
  `confirm` is replaced by it.
- `crates/borax/src/run.rs`: `rename_events` gains the interactive
  driver — propose, ask, then claim and move, or decline.
- `crates/borax/src/cli.rs`, `config.rs`: the `batch` pair on `rename`
  and `config`, the `rename.batch` key, and the usage error for
  `--apply --no-batch`.
- `crates/borax/src/event.rs`: `run-started` gains `interactive`;
  `SkipReason` gains `declined`; `Counts` gains `unreached`. The event
  schema is not frozen before `1.0.0`, and these are additions a
  consumer that ignores unknown fields reads without change.
- New dependency: `inquire` (MIT, crossterm backend, supports Windows,
  MSRV below the workspace's).
- `openspec/project.md`: the file-safety contract says borax "previews
  by default"; it becomes "moves nothing without an explicit decision",
  which is what this change keeps true.
- Documents a person reads: `docs/manual.org` (`borax rename`, the
  settings table), `README.md`, `CHANGELOG.md`, `openspec/STATE.md`.

## Deferred

- **Interactive `resolve` and `bib`.** Neither moves a file, so neither
  has a decision to put to anyone yet. Supplying an identifier
  (change 4) is the first thing that would make one worth asking, and
  it lands on `rename` first because that is where the answer is used.
- **"Rename all remaining."** A heterogeneous folder is the case this
  change exists for, and an answer that stops asking undoes it. If
  sessions over large uniform folders turn out to want it, it is a
  menu entry, not a redesign.
- **Revisiting a decision within a run.** Once a file is moved or
  skipped the session goes on; there is no "back". A re-run reaches the
  file again, which is the project's answer to undo as well.
