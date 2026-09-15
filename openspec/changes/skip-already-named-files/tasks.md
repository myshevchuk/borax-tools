# Tasks: skip-already-named-files

Work on branch `change/skip-already-named-files`, stacked on
`change/add-interactive-rename`. Implement after that change is
archived. Its `cli` delta is a copy of that change's version of the
subcommand-surface requirement with one edit, so re-copy it from the
living spec if that change's wording moves before it archives.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it.

## 1. Already named is its own outcome

- [ ] 1.1 Red: `crates/borax/tests/event.rs` — `already-named`
      serializes with its path, `Counts::observe` counts it in `named`
      and not in `skipped`, and its human line and summary wording
      (design D3)
- [ ] 1.2 Red: `crates/borax/tests/renaming.rs` — `carry_out` maps
      `PlannedRename::AlreadyNamed` to the new event in both modes
- [ ] 1.3 Red: `crates/borax/tests/session.rs` — a run whose only
      non-resolved outcomes are already named is `Outcome::Success`
- [ ] 1.4 Green: `Event::AlreadyNamed`, removal of
      `SkipReason::AlreadyNamed`, `Counts::named`, renderings; update
      existing tests that asserted the old skip, stating each in the
      commit

## 2. A file is not its own duplicate

- [ ] 2.1 Red: `crates/borax/tests/pipeline.rs` — a content match whose
      recorded path is the incoming path is not a duplicate and the file
      resolves from the content index without a source being asked; a
      content match at another live path still is
- [ ] 2.2 Red: the same for the work check, with a changed hash
- [ ] 2.3 Red: `crates/borax/tests/end_to_end.rs` — apply, then re-run
      over the same collection: every renamed file is already named,
      none is a duplicate, and the exit code is 0
- [ ] 2.4 Green: exclude the self-match in both checks (design D2)

## 3. Passing over named files

- [ ] 3.1 Red: `crates/borax/tests/cli.rs` and `config.rs` — the
      `skip-named` pair on `rename` and `config` only, the
      `rename.skip-named` key defaulting to true with its origin
- [ ] 3.2 Red: `crates/borax/tests/run.rs` — an interactive run with a
      scripted asker over named and unnamed fixtures: nothing rendered
      for named files, questions only for the others, summary count;
      with `--no-skip-named` each named file renders as in batch; the run
      log carries both kinds either way
- [ ] 3.3 Green: the setting, and the per-file hold in the interactive
      human renderer (design D5)
- [ ] 3.4 Verify by hand on the scratch collection that reproduced the
      defects (proposal, Context in `design.md`)

## 4. Documents

- [ ] 4.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, the settings table, duplicate reports) and `CHANGELOG.md`,
      with this proposal as source; check the diff
- [ ] 4.2 Record the built state in `openspec/STATE.md`
- [ ] 4.3 `openspec validate skip-already-named-files --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green
