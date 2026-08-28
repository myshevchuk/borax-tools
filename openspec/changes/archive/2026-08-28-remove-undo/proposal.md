# Proposal: remove-undo

## Why

`borax undo` restores the exact filenames a run replaced, but that is
almost never the state a user wants. Someone who ran `--apply` wanted
those names gone; when the result disappoints, what they want is a
*better* name, not the old one. Because the pipeline identifies a file
by its content hash rather than its name, correcting the template or the
metadata and re-running already fixes the result — from wherever the
files now sit, including subdirectories a stray `/` created, since input
expansion recurses.

What re-running genuinely cannot reconstruct is the *mapping* from old
name to new one, and the run log already records it: original path, new
path, and content hash per applied rename. Reversing the moves adds no
information the log does not already hold.

Undo is also the less consistent of the two recoveries. `undo_events`
moves the file and emits an event, and nothing else: it never appends to
or corrects the ledger, so every reverted entry keeps pointing at a path
that no longer exists and duplicate detection silently degrades until
`borax ledger rebuild`; and it never moves the sidecar, which was
written beside the *new* name, so it strands `smith2024.pdf.bib` next to
a restored `original.pdf` — one of the two entry points to the sidecar
orphan defect recorded in `STATE.md`. Re-running has neither problem: it
re-admits to the ledger and rewrites the sidecar beside the current
name.

The command is therefore paying a cross-cutting cost to be the worse
answer to the question it exists for. Removing it is cheaper than
fixing it, and nothing is lost that the log does not keep.

## What Changes

- **`borax undo` is removed** (**BREAKING**). The subcommand, its
  engine, and its event vocabulary go: `crates/borax/src/undo.rs`,
  `crates/borax/tests/undo.rs`, `Command::Undo`, `Event::Reverted`, and
  the three skip reasons only undo ever produced —
  `SkipReason::Missing`, `ContentChanged`, and `OriginalTaken`.
- **Run logs stay, and stay mandatory.** With undo gone the apply-run
  log is the *sole* record of what a run moved, so the invariant that
  makes it trustworthy is kept exactly as it is: `rename --apply`
  refuses to run when it has nowhere to record itself, the log is
  created and its planned rename events flushed before the first file
  moves, and `--no-run-log` cannot suppress it. Rename events keep the
  content hash, which is what lets a person or a script verify identity
  during a manual recovery.
- **The XDG state fallback stays**, on the log's own merit rather than
  undo's. An apply run outside any collection still needs somewhere to
  record itself, so it still writes under the state directory and still
  refuses when neither location is available. Only the requirement's
  stated rationale changes.
- **Latest-apply-log selection is removed.** `latest_apply_log`,
  `latest_apply_in`, and `is_apply_log` existed to find the log undo
  would replay and have no other production consumer. Deleting them
  also deletes a latent defect: `is_apply_log` matches any name ending
  `-apply.jsonl`, and `applying()` reports `bib`, `undo` and `ledger
  rebuild` as applying runs too, so a `bib` run following a
  `rename --apply` silently shadowed the rename's log. Nothing selects
  a "latest apply log" any more, so the bug goes with the feature.
- **The `<dry|apply>` suffix in log names stays.** It is what makes a
  preview sort next to the run that applied it, which is a property of
  the listing a person reads, not of undo.
- **The README documents recovery from the log**, so dropping the
  command does not drop the answer to "how do I get the old names
  back". One short section: where logs live, what a `renamed` line
  carries, and how to read the mapping back.

## Capabilities

### New Capabilities

None. This change removes behaviour and re-grounds the rationale of
requirements that survive it.

### Modified Capabilities

- `rename`: the requirement `Undo reverts the last applied run safely`
  is removed outright, with its three scenarios. The neighbouring
  requirement that applied renames are recorded in the run log names no
  consumer and is left exactly as it stands.
- `cli`: the single-binary requirement's subcommand list drops `undo`.
  It also gains `ledger`, which ships today and was never added to the
  list — one clause in a sentence this change is already rewriting,
  recorded here rather than made silently.
- `run-logs`: the XDG-fallback requirement is restated so that its
  reason is the record itself rather than "so undo works everywhere",
  and its scenario stops asserting a subsequent revert.
- `ledger`: the stale-entries requirement keeps its force — disk is the
  source of truth and a stale entry never vetoes an admission — but
  stops citing undo as the way an entry goes stale, since deleting or
  moving a file by hand is now the whole of it.

## Impact

- `crates/borax`: `undo.rs` and `tests/undo.rs` deleted (230 production
  lines, 774 test lines, 21 tests). `cli.rs` loses a `Command` variant
  and its `name`/`paths` arms; `run.rs` loses `Prepared::Undoing`,
  `undo_events`, `recorded_moves`, and the `Undo` arms of `preflight`,
  `emit_events`, `applying` and `expanded`; `event.rs` loses one event
  variant, three skip reasons and their renderings; `runlog.rs` loses
  the three selection functions and their tests.
- `crates/borax/tests/end_to_end.rs` uses `latest_apply_log` in three
  places purely to locate a log to assert on. Those call sites need a
  local helper rather than the production function, so the tests keep
  their coverage while the production surface shrinks.
- `crates/borax-core`: untouched. Nothing in the record model, planner
  or ledger core knows about undo.
- Documentation: `README.md` drops "and can undo" from its opening
  claim and gains the log-recovery section; `CHANGELOG.md` records the
  removal as breaking; `openspec/STATE.md` is updated in both the
  "What is built" narrative and the sidecar-orphan defect, which loses
  one of its two entry points.
- Version: `0.3.0`. Removing a subcommand is incompatible, and per
  `CLAUDE.md` no compatibility is owed before `1.0.0` — the obsolete
  interface is removed cleanly rather than deprecated behind a shim.
