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

## Capabilities

### Modified Capabilities

- `rename`: an already-named file is reported as its own outcome rather
  than a skip; an interactive run passes over such files by a setting.
- `ledger`: the duplicate checks exclude an entry recording the
  incoming file at its own path.
- `cli`: `rename` and `config` accept the `--skip-named` /
  `--no-skip-named` pair.

## Impact

- `crates/borax/src/event.rs`: `Event::AlreadyNamed { path }` replaces
  `SkipReason::AlreadyNamed`; `Counts` gains `named`; the human summary
  reports it. A consumer that matched `skipped` events with reason
  `already-named` now matches `already-named` events instead. The event
  schema is not frozen before `1.0.0`.
- `crates/borax/src/renaming.rs`: `Applying::carry_out` maps
  `PlannedRename::AlreadyNamed` to the new event.
- `crates/borax/src/pipeline.rs`, `ledger.rs`: both duplicate checks
  ignore a match whose recorded path is the incoming file's own.
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
- **Re-admitting a file renamed under a new template.** When a file the
  ledger admitted is renamed again, its old entry names a path that no
  longer exists, and the next run warns that the ledger holds stale
  entries. That is the existing behaviour for any moved file and is
  cured by `borax ledger rebuild`; making an applied rename supersede its
  file's earlier entry is a `ledger` change of its own.
- **Offering a named file for re-identification.** A file borax once
  named from the wrong identifier is already named and passed over.
  `supply-identifiers-interactively` gives `--no-skip-named` runs a
  question for such files; this change only prints them.
