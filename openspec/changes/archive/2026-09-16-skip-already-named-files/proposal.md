## Why

The folder a person runs borax on is rarely new. Papers arrive a few at
a time into a folder that already holds the ones borax renamed last
week, and a run over that folder reaches every file again. What a
re-run says about those files today is wrong in three different ways,
all reproduced on a copy of the real-PDF corpus:

- **A file already carrying its name is a skip.** It is reported
  `skipped, already carries that name` and counted in `skipped`, so a
  run over a folder that is entirely in order exits with the
  partial-success code. A script cannot tell "everything is named" from
  "some files need attention", which is the one distinction the
  partial-success code exists to draw.
- **A file borax admitted is a duplicate of itself.** In a collection
  with a ledger, a file renamed by an applied run is found by its hash
  on the next run, and the ledger entry that matches is the one
  recording that very file at that very path. It is reported `skipped,
  same bytes already archived at <its own name>`. The duplicate check is
  meant to keep a second copy out of the collection, and here it names
  the first copy as a copy of itself.
- **In an interactive session, both are noise.** `add-interactive-rename`
  asks nothing about such files, but still prints a line for each. In a
  folder of three new papers and two hundred renamed ones, the three
  questions are lost in two hundred lines saying nothing happened.

## What Changes

- **Already named is not a skip.** A file whose name is the one its
  record implies is reported by its own `already-named` event, counted
  in its own `named` total, and does not make a run exit with the
  partial-success code.
- **A file is not a duplicate of itself.** A ledger entry that records
  the incoming file at its own path — matched by content or by work —
  is not a duplicate report. The file continues through resolution and
  planning like any other, which in the usual case ends in
  `already-named` without a network request, since the content index
  answers for a file borax has resolved before.
- **An interactive run passes over named files.** With the new
  `rename.skip-named` setting on, which is its default, an interactive
  run renders nothing for an already-named file: no resolution line, no
  outcome line, no question. The run summary counts them. With it off
  (`--no-skip-named`), they are reported as a batch run reports them.
- **"Already named" means named by the current template.** Whether a
  file is passed over is decided by rendering its record through the
  configuration in force, not by whether a ledger holds it. A file named
  under an old template is not already named under a new one, and is
  proposed like any other file.
- **Filing into a subdirectory is idempotent.** A template rendering
  `[journal]/[auth][year]` files a paper into `Nature/`. Re-run today
  over the filed collection, it proposes `Nature/Nature/Zeng2026.pdf`,
  and does so again on every later run. The rendered subdirectory is
  where the file belongs, not another level to add: a file already
  sitting in it is at its target, and one sitting in the wrong
  subdirectory moves across rather than deeper.
- **A file is never proposed a move onto its own name.** Where a
  collision suffix lands on the name the file already carries — the
  second of two works that render one name, re-run — the file is
  already named. Proposing it was a move the filesystem would refuse,
  since borax never overwrites, itself included.
- **The stream announces the change.** Removing a skip reason is not an
  addition a consumer can ignore, so the event schema version goes to
  2.

The two filing defects are not new. They are what the duplicate check
was hiding: a file it stopped never reached the planner, so nobody saw
what the planner would have said about it. Removing a check that was
wrong is what exposes them, which is why they are fixed here rather
than left for a reader to trip over.

## Capabilities

### Modified Capabilities

- `rename`: an already-named file is reported as its own outcome rather
  than a skip; an interactive run passes over such files by a setting;
  filing into a subdirectory is idempotent; a suffix that lands on the
  file's own name is already-named rather than a move.
- `ledger`: the duplicate checks pass over an entry recording the
  incoming file at its own path and keep looking, so a genuine copy
  elsewhere is still found.
- `cli`: `rename` and `config` accept the `--skip-named` /
  `--no-skip-named` pair; the event schema version goes to 2; an
  interactive run may hold a file's lines until its planning outcome is
  known, which is before its question.

## Impact

- `crates/borax/src/event.rs`: `Event::AlreadyNamed { path }` replaces
  `SkipReason::AlreadyNamed`; `Counts` gains `named`; the human summary
  reports it. A consumer that matched `skipped` events with reason
  `already-named` now matches `already-named` events instead. The event
  schema is not frozen before `1.0.0`.
- `crates/borax/src/renaming.rs`: `Applying::carry_out` maps
  `PlannedRename::AlreadyNamed` to the new event.
- `crates/borax/src/pipeline.rs`, `ledger.rs`,
  `crates/borax-core/src/ledger.rs`: the lookups skip an entry
  recording the incoming file itself and go on to the next candidate,
  rather than the caller discarding what came back — the index keeps
  one entry per hash and per identifier, so filtering the winner would
  hide a second copy behind the file's own entry.
- `crates/borax-core/src/rename.rs`: a suffix candidate equal to the
  file's own name is `AlreadyNamed`.
- `crates/borax/src/renaming.rs`: a rendered subdirectory the file
  already sits in is where it belongs rather than another level to
  add.
- `crates/borax/src/run.rs`: the interactive renderer holds a file's
  lines until its fate is known, so a passed-over file's resolution line
  is never printed.
- `crates/borax/src/cli.rs`, `config.rs`: the `skip-named` pair and the
  `rename.skip-named` key.
- Documents a person reads: `docs/manual.org` (`borax rename`, the
  settings table, the ledger's duplicate reports), `CHANGELOG.md`,
  `openspec/STATE.md`.

## Deferred

- **Passing over named files in batch output.** A batch run over a large
  collection prints a line per already-named file too. This change keeps
  batch output as the complete per-file account a person reads after the
  fact and scopes the setting to interactive runs, as asked. Extending
  it is a question of what a batch report is for, not of mechanism.
- **Compacting superseded ledger rows.** A file renamed again leaves
  its earlier row in the JSONL file. Nothing reads it — the index keeps
  the latest entry for a hash and for an identifier, so lookups find
  the new path and no staleness is reported — but the file grows with
  rows that describe nowhere. `borax ledger rebuild` compacts them
  today, and making an applied rename supersede its own earlier row is
  a `ledger` change of its own.
- **Offering a named file for re-identification.** A file borax once
  named from the wrong identifier is already named and passed over.
  `supply-identifiers-interactively` gives `--no-skip-named` runs a
  question for such files; this change only prints them.
