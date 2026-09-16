# Tasks: skip-already-named-files

Work on branch `change/skip-already-named-files`, stacked on `main`
now that `add-interactive-rename` is archived.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it.

Groups 1 to 3 are corrections the duplicate check was hiding and are
what the rest rests on; group 4 is the setting the change is named for.

## 1. Already named is its own outcome

- [x] 1.1 Red: `crates/borax/tests/event.rs` — `already-named`
      serializes with its path, `Counts::observe` counts it in `named`
      and not in `skipped`, and its human line and summary wording
      (design D3)
- [x] 1.2 Red: `crates/borax/tests/renaming.rs` — `carry_out` maps
      `PlannedRename::AlreadyNamed` to the new event in both modes
- [x] 1.3 Red: `crates/borax/tests/session.rs` — a run whose only
      non-resolved outcomes are already named is `Outcome::Success`
- [x] 1.4 Red: every event carries schema version 2 (design D6)
- [x] 1.5 Green: `Event::AlreadyNamed`, removal of
      `SkipReason::AlreadyNamed`, `Counts::named`, `SCHEMA` at 2, and
      the renderings; update existing tests that asserted the old skip,
      stating each in the commit

## 2. Filing a file that is already where it belongs

- [x] 2.1 Red: `crates/borax-core/tests/rename.rs` — a suffix candidate
      equal to the item's own source is `AlreadyNamed`, and the
      exemption that lets a file change only the case of its name still
      works (design D5b)
- [x] 2.2 Green: the ladder stops at the file's own name
- [x] 2.3 Red: `crates/borax/tests/renaming.rs` — with a collection
      root: a file already in the rendered subdirectory is already
      named; one in a subdirectory the template no longer renders moves
      across rather than deeper; one further down the tree is filed
      where it belongs; a file not yet filed is filed into it. With no
      collection root: the target is joined to the file's own directory,
      as today (design D5a)
- [x] 2.4 Green: `Planning` takes the collection root and joins a
      rendered subdirectory to it, falling back to the file's own
      directory when there is none
- [x] 2.5 Verified on the scratch collection that reproduced the
      nesting: `[journal]/[auth][year]` applied twice leaves every file
      where the first run filed it, and the second run reports each
      already named

## 3. A file is not its own duplicate

- [x] 3.1 Red: `crates/borax/tests/pipeline.rs` — a content match whose
      recorded path is the incoming path is not a duplicate and the file
      resolves from the content index without a source being asked; a
      content match at another live path still is
- [x] 3.2 Red: the same for the work check, with a changed hash
- [x] 3.3 Red: a file whose own entry is the index's answer does not
      hide a byte-identical copy recorded elsewhere — the lookup goes on
      and reports that copy (design D2)
- [x] 3.4 Red: the path comparison — an entry reached by a relative
      input path and by one containing `..` recognises the file as
      itself, and on Windows a differently cased spelling does too
- [x] 3.5 Red: `crates/borax/tests/end_to_end.rs` — apply, then re-run
      over the same collection: every renamed file is already named,
      none is a duplicate, and the exit code is 0
- [x] 3.6 Green: the lookups take the incoming path and skip its own
      entry (design D2)

## 4. Passing over named files

- [x] 4.1 Red: `crates/borax/tests/cli.rs` and `config.rs` — the
      `skip-named` pair on `rename` and `config` only, the
      `rename.skip-named` key defaulting to true with its origin
- [x] 4.2 Red: `crates/borax/tests/dispatch.rs` — an interactive run
      with a scripted asker over named and unnamed fixtures: nothing
      rendered for named files, questions only for the others, summary
      count; with `--no-skip-named` each named file renders as in batch;
      the run log carries both kinds either way
- [x] 4.3 Red: the hold ends before the question — an asker that reads
      what the terminal has been given sees the file's own resolution
      line already there when it is asked (design D5)
- [x] 4.4 Red: a sidecar written beside a passed-over file, and a
      failure writing one, are reported even when the file is not
- [x] 4.5 Green: the setting, and the per-file hold in the interactive
      human renderer (design D5)
- [x] 4.6 Verified on the scratch collection that reproduced the
      defects: a collection in order exits 0 and reports no duplicates
      of its own files. The interactive pass-over itself is covered by
      the suite through a scripted asker, not by hand

## 5. Documents

- [x] 5.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, the settings table, duplicate reports, filing into
      subdirectories) and `CHANGELOG.md`, with this proposal as source;
      check the diff
- [x] 5.2 Record the built state in `openspec/STATE.md`, including the
      two defects this change closes
- [x] 5.3 `openspec validate skip-already-named-files --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green
