## Why

Every subcommand's human output ends with the same line, whatever the
subcommand did. `human_summary` in `crates/borax/src/event.rs` builds it
from the `run-finished` totals — `N resolved, N renamed, N skipped`,
plus `already named`, `unmatched`, `not reached` and `findings` clauses
when they are nonzero — and the human sink (`Rendering::emit` in
`crates/borax/src/run.rs`) writes it for every command. That line
describes a rename. Most commands never resolve, rename or skip
anything, so for them it is a line of zeros that answers a question
nobody asked.

An interactive review of 0.6.0 recorded the actual terminal output:

```text
$ borax status test_1
…/test_1: 7 artifacts, 0 items, 0 records, 7 orphans
0 resolved, 0 renamed, 0 skipped

$ borax validate test_1
…/test_1: 0 findings, 7 orphans, 0 missing, 0 unlinked
0 resolved, 0 renamed, 0 skipped

$ borax resolve paper.pdf
…
1 resolved, 0 renamed, 0 skipped
```

`status --identify`, `adopt`, `reconcile`, `config` and `cache` ended
with the same zero line. `resolve` never renames, so its `0 renamed` is
a count of something the command cannot do. A batch `rename`
(`1 resolved, 0 renamed, 0 skipped, 1 already named`) and an
interactive one (`0 resolved, 0 renamed, 1 skipped`) are the cases the
line was designed for, and it is right there.

The maintainer's roadmap accepted the direction: emit no operation
summary for `status`, `status --identify` or `validate`; keep
resolution-appropriate totals for `resolve`, without the renamed count;
give each subcommand an intentional human summary, with no irrelevant
counters in the accepted outputs.

## What Changes

- **The human summary depends on the command.** Each subcommand closes
  its human output with a summary shape of its own:
  - `rename`, batch and interactive: unchanged, byte for byte.
  - `resolve` and `bib`: `N resolved, N skipped`, followed by the
    `unmatched`, `not reached` and `findings` clauses when they are
    nonzero. `renamed` and `already named` are dropped because neither
    command can produce them.
  - `status` (with or without `--identify`), `validate`, `reconcile`,
    `adopt`, `config` and `cache`: no summary line. Each already ends
    on its own report or totals line.
- **A total that decides a partial-success exit is always visible.**
  `session::outcome_for` returns partial success when skipped,
  not-reached or findings is nonzero. The human rendering names each
  such total whenever it is nonzero. `resolve`, `bib` and `rename` name
  it in the summary. `validate`, the only command that reports
  findings, names the count on its `library-validated` line and the
  findings themselves on lines of their own. The other commands
  produce none of the three. If one of them later does, the summary
  shape falls back to a line naming the nonzero totals, so that change
  cannot hide one (design D3).
- **JSON is unchanged.** The `run-finished` event and all seven of its
  counters (`resolved`, `renamed`, `skipped`, `named`, `unmatched`,
  `unreached`, `findings`) are emitted exactly as today, in `--json`
  output and in every run log. The event schema version stays at 3,
  because no event, field or reason changes. The stream is the same.
  Only the human rendering of its last event differs, and it depends
  on nothing but the command that `run-started` names.
- **The context-free rendering of `run-finished` is silent.**
  `event::human_line` and `event::render` render one event with no
  knowledge of the command. They return `None` for `run-finished`, as
  they already do for `run-started`. The summary comes from a function
  that is given the command's summary shape (design D1, D2).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cli`: adds the requirement that each subcommand's human output closes
  with a summary shape fitted to it. The requirement fixes the shape for
  every subcommand, the rule that a nonzero partial-success total is
  never hidden, and that JSON and the run log are unchanged.
- `resolution`: "The run summary reports the skip queue" today says
  every run ends with a summary. It is restated for runs that can skip.
  Each skip is reported with its reason, and the skips are counted in
  `run-finished` and in the command's human summary. A command that
  cannot skip no longer owes a summary line.
- `external-tables`: "Unmatched lookups are reported" says a run counts
  misses "in its summary". It is restated so that the count is in
  `run-finished`, and in the human summary of the commands that have
  one. In `adopt`, the only other command that can miss a lookup,
  every miss is on a line of its own directly above the closing
  totals.

Requirements checked and left unchanged:

- `cli` "JSON Lines output is first-class". Human and JSON remain
  renderings of the same event stream. Human output already omits
  `run-started`, and the summary a command renders is decided by the
  command `run-started` names.
- `cli` "A run reports as it goes". The stream still opens with
  `run-started` and closes with `run-finished`, and this change alters
  neither when a line is written nor what either event carries.
- `cli` "Exit codes distinguish partial success". The exit codes do
  not change.
- `rename` "An interactive run passes over already-named files". The
  summary it requires is the `rename` shape, which does not change.
- `rename` "Quitting an interactive run leaves the rest untouched" and
  "A file already carrying its name is not a skip". Both speak about
  `run-finished` counts, which do not change.
- `extraction` "Extraction failures are typed and non-fatal". The
  failure type reaches the reader as the reason on the file's `skipped`
  event, which this change does not touch. This is the skip report
  that the restated `resolution` requirement describes.

## Impact

- `crates/borax/src/event.rs`: a `Summary` enum naming the summary
  shapes. `human_summary` takes one and returns `Option<String>`.
  `human_line` renders `RunFinished` as `None`.
- `crates/borax/src/cli.rs`: `Command::summary(&self) -> Summary`, an
  exhaustive match beside `Command::name`.
- `crates/borax/src/run.rs`: `Rendering` carries the command's
  `Summary`, set in `dispatch`, and passes it to `human_summary`. The
  JSON path and `Logging` are untouched.
- `crates/borax/tests/`: tests that assert the old line change.
  `dispatch.rs` expects `1 resolved, 0 renamed, 0 skipped` from a
  `resolve` run. `event.rs` reads two summaries through
  `human_line(&Event::RunFinished { .. })`. The tasks list both.
- No JSON consumer is affected, and no event schema version changes.
- Documents a person reads, updated by the doc writer:
  - `docs/manual.org` line 61 says "The closing summary counts each
    outcome separately" in the overview, which applies to every
    subcommand. It needs to say the summary is the rename line and
    that other commands close differently. The example at line 64 is a
    rename run and stays as it is.
  - `docs/manual.org` lines 306–313 and 334–341 quote interactive
    rename summaries (lines 309 and 337). The format does not change;
    confirm the text still reads correctly.
  - `docs/manual.org` line 1128–1131 says "the run summary counts them"
    for unmatched lookups, quoting a rename line. The rename example
    stays. The sentence should say it covers `rename` and `bib`, and
    that `adopt` reports each miss without a count.
  - `docs/manual.org` sections `borax resolve` (line 80), `borax bib`
    (343), `borax status` (393) and `borax validate` (417) describe
    no closing line today. They may say what the output ends with. For
    `validate`, the findings count is on the report line.
  - `CHANGELOG.md` gets an Unreleased "Changed" entry.
  - `openspec/STATE.md` records the built state.

## Deferred

- **A bibliography summary.** `bib` keeps the generic resolution totals
  and only loses the counters it can never produce. A summary of what
  `bib` wrote — entries added, updated, left alone, sidecars written —
  is a separate change the maintainer deferred at lowest priority.
- **Richer library summaries.** `status`, `validate`, `reconcile` and
  `adopt` already close on their own totals line, and this change does
  not reword those lines.
