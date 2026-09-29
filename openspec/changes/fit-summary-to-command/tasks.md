# Tasks: fit-summary-to-command

Work on branch `change/fit-summary-to-command`, which sits directly on
`main` after 0.6.0. Nothing below waits on another change.

Every group is a red/green pair. The red task is written and run
failing first, and the implementation task makes it pass without
touching it. A new test may open red on an unresolved import of
`Summary` or `Command::summary`. It must be written so that it fails on
its assertion once those names exist.

The API the tests are written against is fixed in design D1:
`borax::event::Summary` (`Renaming`, `Resolution`, `Validation`,
`Silent`), `borax::event::human_summary(Summary, &Counts, usize) ->
Option<String>`, and `borax::cli::Command::summary(&self) -> Summary`.
The exact strings are in design D3.

JSON is out of bounds throughout. No task changes an event, a field,
`Counts`, `SCHEMA` or `json_line`, and a test that needs one changed is
a sign the task has been misread.

## 1. The summary shapes, as pure functions

- [ ] 1.1 Red: `crates/borax/tests/event.rs`, one test per shape
      (design D3):
      - `Renaming` reproduces today's line byte for byte, including
        `already named` with `(not shown)` and `(N not shown)`, and the
        `unmatched`, `not reached` and `findings` clauses.
      - `Resolution` gives `1 resolved, 0 skipped` for one resolution,
        `0 resolved, 0 skipped` for an empty run, and adds the
        `unmatched`, `not reached` and `findings` clauses only when
        nonzero. Its line never contains `renamed` or `already named`,
        even with those counts set.
      - `Silent` is `None` for all-zero counts, and for counts whose
        only nonzero totals are resolved, renamed, named or unmatched.
      - `Validation` is `None` when findings alone is nonzero.
- [ ] 1.2 Red: `crates/borax/tests/event.rs`, the rule in design D3a.
      For every `Summary` variant and each of skipped, unreached and
      findings set nonzero on its own, the result is `Some` and names
      that total in its clause wording. The one excepted pair is
      `Validation` with findings, which is `None`. Also: `Silent` with
      skipped 2 and unreached 1 gives exactly `2 skipped, 1 not
      reached`.
- [ ] 1.3 Red: `crates/borax/tests/event.rs`,
      `human_line(&Event::RunFinished { .. })` is `None` (design D2).
      Move the two existing summary tests that read through
      `human_line` to `human_summary(Summary::Renaming, …, 0)`:
      `the_summary_line_names_unmatched_lookups_when_there_were_any`
      and
      `the_summary_line_says_nothing_about_unmatched_lookups_when_there_were_none`.
      Keep their spec-scenario doc comments, and state the move in the
      commit. Replace `human_line_of_run_finished_is_not_silent`, which
      asserts `is_some()` and contradicts D2, with the new `None`
      assertion rather than keeping both.
- [ ] 1.4 Red: `crates/borax/tests/cli.rs`, `Command::summary` maps
      `rename` to `Renaming`, `resolve` and `bib` to `Resolution`,
      `validate` to `Validation`, and `status` (with and without
      `--identify`), `reconcile`, `adopt`, `config` and `cache` to
      `Silent`. Build each command through the same parse path the
      existing `cli.rs` tests use.
- [x] 1.5 Green: add `Summary` and change `human_summary` in
      `crates/borax/src/event.rs`. `human_line` renders `RunFinished`
      as `None`, and its docstring stops claiming `RunStarted` is the
      only silent event. Add `Command::summary` in
      `crates/borax/src/cli.rs` as an exhaustive match with no wildcard
      arm.

## 2. The human sink uses the command's shape

- [ ] 2.1 Red: `crates/borax/tests/dispatch.rs`, human mode through
      `dispatch`:
      - `status` over a library of unrecorded PDFs: the last line is
        the `library-status` report, and no line contains `resolved,`.
      - `status --identify`: the same.
      - `validate` over a library with a finding: the last line is the
        `library-validated` line, and the outcome is `Outcome::Partial`.
      - `reconcile`, `adopt`, `config` and `cache`: none ends with a
        summary line, and each ends on its own report or totals line.
- [ ] 2.2 Red: `crates/borax/tests/dispatch.rs`, `resolve` in human
      mode. In
      `human_format_omits_run_started_but_still_ends_with_the_summary_line`,
      update the assertion that expects
      `1 resolved, 0 renamed, 0 skipped` to expect `1 resolved,
      0 skipped`. Add a run with one skip, whose last line is
      `1 resolved, 1 skipped` and whose outcome is `Outcome::Partial`.
      State the updated assertion in the commit.
- [ ] 2.3 Red: `crates/borax/tests/dispatch.rs`, `bib` in human mode.
      The last line has no `renamed` clause, and a run with a table
      miss ends `…, 1 unmatched`.
- [ ] 2.4 Red, the regression guard: `crates/borax/tests/dispatch.rs`.
      A batch `rename` over one already-named file ends
      `1 resolved, 0 renamed, 0 skipped, 1 already named`. The existing
      interactive tests asserting `2 already named (not shown)` and
      `2 not reached` stay as they are and must stay green.
- [ ] 2.5 Red: `crates/borax/tests/dispatch.rs`, JSON and the run log
      are unchanged (design D5).
      - `status --json`: the last line is `run-finished` with all seven
        counters and schema 3.
      - A human-mode `status` inside a library with the run log on
        writes a log whose last line is the same `run-finished` event,
        though stdout showed no summary.
- [x] 2.6 Green: in `crates/borax/src/run.rs`, give `Rendering` a
      `summary: Summary` field set from `cli.command.summary()` in
      `dispatch`. `Rendering::emit` renders `(RunFinished, Human)`
      through `human_summary(self.summary, counts, self.hidden)` and
      writes nothing on `None`. Update the docstrings on `Rendering`
      and `dispatch` that describe the closing line. Leave `Logging`
      and the JSON path alone.

## 3. Documents

- [ ] 3.1 Run `codex-docs` update jobs with this proposal and its
      design as sources, and check the diff against both:
      - `docs/manual.org`: the overview at line 61 ("The closing
        summary counts each outcome separately") becomes the rename
        summary, with the other commands closing on their own lines.
        The examples at lines 64, 309 and 337 are rename runs and
        stay. The unmatched-lookups passage at lines 1128–1131 names
        `rename` and `bib` as the commands that count misses in their
        summary, and says `adopt` reports each miss without a count.
        The `borax resolve`, `borax bib`, `borax status` and `borax
        validate` sections may say what their output ends with.
      - `CHANGELOG.md`: an Unreleased "Changed" entry. Library, `config`
        and `cache` commands no longer print the resolved/renamed/
        skipped line. `resolve` and `bib` drop `renamed`. `rename` is
        unchanged. JSON and run logs are unchanged.
- [ ] 3.2 Record the built state in `openspec/STATE.md`.
- [ ] 3.3 `openspec validate fit-summary-to-command --strict` and
      `python3 scripts/check-spec-deltas.py` pass, and the full suite
      is green.
