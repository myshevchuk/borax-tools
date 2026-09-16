# Tasks: skip-already-named-files

Work on branch `change/skip-already-named-files`, stacked on `main`
now that `add-interactive-rename` is archived.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it.

Groups 1 to 3 are corrections the duplicate check was hiding and are
what the rest rests on; group 4 is the setting the change is named for.

## 1. Already named is its own outcome

- [ ] 1.1 Red: `crates/borax/tests/event.rs` — `already-named`
      serializes with its path, `Counts::observe` counts it in `named`
      and not in `skipped`, and its human line and summary wording
      (design D3)
- [ ] 1.2 Red: `crates/borax/tests/renaming.rs` — `carry_out` maps
      `PlannedRename::AlreadyNamed` to the new event in both modes
- [ ] 1.3 Red: `crates/borax/tests/session.rs` — a run whose only
      non-resolved outcomes are already named is `Outcome::Success`
- [ ] 1.4 Red: every event carries schema version 2 (design D6)
- [ ] 1.5 Green: `Event::AlreadyNamed`, removal of
      `SkipReason::AlreadyNamed`, `Counts::named`, `SCHEMA` at 2, and
      the renderings; update existing tests that asserted the old skip,
      stating each in the commit

## 2. Filing a file that is already where it belongs

- [ ] 2.1 Red: `crates/borax-core/tests/rename.rs` — a suffix candidate
      equal to the item's own source is `AlreadyNamed`, and the
      exemption that lets a file change only the case of its name still
      works (design D5b)
- [ ] 2.2 Green: the ladder stops at the file's own name
- [ ] 2.3 Red: `crates/borax/tests/renaming.rs` — a file already in the
      rendered subdirectory is already named; one in a subdirectory the
      template no longer renders moves across rather than deeper; one
      further down the tree is filed from where it is; a file not yet in
      it is filed into it as before (design D5a)
- [ ] 2.4 Green: the base a target is joined to is the file's directory
      less the rendered subdirectory, where it already ends with it
- [ ] 2.5 Verify on the scratch collection that reproduced the nesting:
      `default = "[journal]/[auth][year]"`, applied twice, leaves every
      file where the first run filed it

## 3. A file is not its own duplicate

- [ ] 3.1 Red: `crates/borax/tests/pipeline.rs` — a content match whose
      recorded path is the incoming path is not a duplicate and the file
      resolves from the content index without a source being asked; a
      content match at another live path still is
- [ ] 3.2 Red: the same for the work check, with a changed hash
- [ ] 3.3 Red: a file whose own entry is the index's answer does not
      hide a byte-identical copy recorded elsewhere — the lookup goes on
      and reports that copy (design D2)
- [ ] 3.4 Red: the path comparison — an entry reached by a relative
      input path and by one containing `..` recognises the file as
      itself, and on Windows a differently cased spelling does too
- [ ] 3.5 Red: `crates/borax/tests/end_to_end.rs` — apply, then re-run
      over the same collection: every renamed file is already named,
      none is a duplicate, and the exit code is 0
- [ ] 3.6 Green: the lookups take the incoming path and skip its own
      entry (design D2)

## 4. Passing over named files

- [ ] 4.1 Red: `crates/borax/tests/cli.rs` and `config.rs` — the
      `skip-named` pair on `rename` and `config` only, the
      `rename.skip-named` key defaulting to true with its origin
- [ ] 4.2 Red: `crates/borax/tests/dispatch.rs` — an interactive run
      with a scripted asker over named and unnamed fixtures: nothing
      rendered for named files, questions only for the others, summary
      count; with `--no-skip-named` each named file renders as in batch;
      the run log carries both kinds either way
- [ ] 4.3 Red: the hold ends before the question — an asker that reads
      what the terminal has been given sees the file's own resolution
      line already there when it is asked (design D5)
- [ ] 4.4 Red: a sidecar written beside a passed-over file, and a
      failure writing one, are reported even when the file is not
- [ ] 4.5 Green: the setting, and the per-file hold in the interactive
      human renderer (design D5)
- [ ] 4.6 Verify by hand on the scratch collection that reproduced the
      defects (proposal, and Context in `design.md`)

## 5. Documents

- [ ] 5.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, the settings table, duplicate reports, filing into
      subdirectories) and `CHANGELOG.md`, with this proposal as source;
      check the diff
- [ ] 5.2 Record the built state in `openspec/STATE.md`, including the
      two defects this change closes
- [ ] 5.3 `openspec validate skip-already-named-files --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green
