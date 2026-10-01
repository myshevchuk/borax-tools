## Why

`borax status --identify` opens every artifact in the library, runs
the extraction passes over it, and keeps one bit of the answer.
`run::status_events` in `crates/borax/src/run.rs` filters the survey's
artifacts with `pipeline::from_file(..).is_ok()` and reports only how
many passed (`identifiable` on `library-status`). The identifier each
file yielded, the pass that found it, and the reason a file failed are
all computed and then dropped.

The interactive review of 0.6.0 recorded the result (reviews
`interactive-review.org`, "Identifier extraction: borax status
--identify"):

```text
$ borax status --identify
…/test_1: 7 artifacts, 0 items, 0 records, 7 orphans, 5 identifiable
```

Two files did not yield identifiers, and the output named neither file
and gave no reason, so the review could not determine why they failed.
An unreadable PDF and a readable PDF with no identifier add to the same
missing count. A second observation in the same review applies too: in
a library with five records and five identifiable files, the totals do
not show whether those are the same five files.

`borax-pdf` already tells the failures apart. `ExtractionError` in
`crates/borax-pdf/src/source.rs` has four variants: `Unreadable`,
`Encrypted`, `NoTextLayer` (every page the text pass read was blank)
and `NoIdentifierFound` (text was present and held no identifier).
`tiered::extract` (`crates/borax-pdf/src/tiered.rs`, line 104) chooses
between the last two. The review's controlled case shows what is lost
when they are merged. It used two synthetic PDFs, a blank page and a
page of readable prose without an identifier, both with an embedded
title. Both PDFs ended up as the same `no-identifier` result.

The maintainer's roadmap schedules this as change 4 of Phase 2. It
asks for a per-file result for every file `status --identify` inspects,
built as an extraction operation that later work can reuse over files
the caller selects. That later work is Phase 3's resolution evidence,
an inspection in the style of `validate` inside `status`, or a future
public extraction command. No living requirement asks `status` to
report per file, so this needs a proposal rather than a restoration.

## What Changes

- **Every artifact `status --identify` inspects gets a result.** The
  survey's artifacts are still the selection, exactly the files
  `status` already counts. Each artifact is extracted once and
  reported once, in survey order, as a new `library-extraction`
  event. The event carries the artifact's library-relative path and
  what extraction made of it (design D4, D6).
- **The result keeps the distinctions `borax-pdf` makes.** A result is
  one of five kinds: `found` (with the identifier and the pass that
  found it), `no-text-layer`, `text-without-identifier`, `encrypted`,
  or `unreadable` (with the reader's message). The blank page with an
  embedded title is `no-text-layer`, and readable prose with no
  identifier is `text-without-identifier`. A title affects the result
  only through an identifier the metadata pass already finds in it, as
  `scan_info` scans the Info `Title` today. This change reports no
  title evidence (design D2).
- **Extraction becomes a reusable operation in the `borax` library
  crate.** `pipeline::extraction` runs over one file the caller chose
  and returns the result, built on the same `from_file` that `resolve`
  uses. `pipeline::extraction_of` is a pure, exhaustive mapping from
  `borax-pdf`'s result to the reported vocabulary. Neither function
  can add a file to the caller's selection (design D1, D3).
- **The count agrees with the per-file results.** `library-status`
  keeps `identifiable`, calculated from the same results the events
  report, and is still the last event before `run-finished`. Its human
  line keeps its wording and is still the last line of human output
  (design D5).
- **Human output writes one line per artifact.** Each line names the
  path and gives the identifier and its pass, or the failure. The
  path, the identifier and any message are escaped before they reach
  the terminal, so this change does not widen the known unescaped-
  metadata defect (design D7).
- **A failed extraction is not a skip.** `run-finished` counts nothing
  for a `library-extraction` event, `status` keeps no summary line,
  and the exit status does not depend on what extraction found
  (design D8).
- **The event schema stays at version 3.** `library-extraction` is a
  new event, and nothing existing changes meaning (design D9).

Explicitly out of scope:

- **Default inspections, verbosity and cost flags.** `--identify` still
  extracts from every surveyed PDF, tracked or not, and the human
  rendering has no quieter mode. Whether `--identify` should skip
  tracked files, or whether a flag should limit output to failures,
  belongs to the roadmap's deferred Phase 2 policy.
- **`resolve`, `rename` and `bib` skip reasons.** `skipped_for` in
  `pipeline.rs` still maps both `NoTextLayer` and `NoIdentifierFound`
  to `no-identifier`, and maps `Encrypted` to `unreadable`. Restoring
  those distinctions in skip events needs a schema bump, and Phase 3
  owns it. Keeping title claims when extraction fails is also Phase
  3's work.
- **Human vocabulary.** The report line keeps "identifiable" and
  "orphans". Rewording them ("PDFs with extracted identifiers",
  "untracked PDFs") is a later change. Only the new per-file line has
  wording of its own.
- **A public `extract` or `identify` command.** The operation stays
  inside the `borax` library crate until a frontend needs it.
- **Exit status.** No exit code changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `library`: MODIFIED "borax reports a library it has never seen".
  `status --identify` reports each surveyed artifact's extraction
  result as a `library-extraction` event, ahead of the totals, and
  never reports a file the survey did not count. The identifiable
  count equals the number of `found` results. The human line is one
  per artifact, with outside text escaped. A failed extraction is
  neither a skip nor a finding. All three existing scenarios are kept,
  and new scenarios cover a success, the blank page with a title,
  readable text without an identifier, encrypted versus unreadable,
  the selection boundary, the exit status, and escaping.
- `extraction`: ADDED "Extraction reports one result for each file it
  is given". It requires exactly one result per given file, in the
  order given, with no file added, and defines the five result kinds
  that may not be merged. A title in metadata counts only through an
  identifier the metadata pass recognises in it. A caller's library
  context can shape how a result is reported but cannot widen the
  selection. This requirement does not govern skip reasons.

Requirements checked and left unchanged:

- `extraction` "Extraction failures are typed and non-fatal". It
  already requires the four failure modes to be distinguished. The
  per-file result meets that for `status --identify`. The resolving
  commands' skip reasons still merge two of the modes, which is a
  deviation from this requirement, and Phase 3 owns it. Task 4.3
  records it in `openspec/STATE.md` so it is stated rather than
  inherited silently.
- `extraction` "Tiered extraction stops at the first hit", "Text-layer
  pass scans a bounded page range" and "Extraction is offline". The
  passes, their order, the page limit and the lack of network access
  are unchanged. The new operation runs the same passes through
  `from_file`.
- `cli` "JSON Lines output is first-class". A new event is an addition
  under its rule (design D9).
- `cli` "A run reports as it goes". Each `library-extraction` event is
  written when its artifact's extraction finishes, and `library-status`
  is written after all of them, as `validate` writes its findings
  before its totals.
- `cli` "A run's human summary fits its command". `status`, with or
  without `--identify`, still has no summary line and still ends on
  the report line. A `library-extraction` event adds no skip, finding
  or unreached total, so the fallback summary line never fires.
- `cli` "Exit codes distinguish partial success". A failed extraction
  is not a skip (design D8).
- `library` "A library is one directory tree" and "An artifact is a
  document in the tree, and one without a record is a worklist item".
  The selection is the survey's artifacts, so nested libraries,
  symlinks, sidecars and store files are left out, as those
  requirements already say.
- `resolution` "The run summary reports the skip queue". `status`
  skips nothing, before and after this change.

## Impact

- `crates/borax/src/event.rs`:
  - `Extraction`, the reported vocabulary;
  - `Event::LibraryExtraction { path, extraction }`;
  - the human line for the new event, escaped through
    `describe::escaped`;
  - `SCHEMA` stays 3, and `Counts::observe` is unchanged.
- `crates/borax/src/pipeline.rs`:
  - `extraction_of`, the pure mapping;
  - `extraction`, the per-file operation over `from_file`;
  - `skipped_for` is not touched.
- `crates/borax/src/library.rs`: `extraction_event`, which places a
  result at its library-relative path. `survey` and `status_event` keep
  their signatures. `survey` still opens no document.
- `crates/borax/src/run.rs`: `status_events` emits one event per
  surveyed artifact as each one finishes, then `library-status` with
  the count of `found` results. The `.is_ok()` reduction is removed.
- `crates/borax/src/describe.rs`: unchanged. Its `escaped` gains a
  caller in `event.rs`.
- Tests: `status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source`
  in `crates/borax/tests/dispatch.rs` expects exactly one event today.
  It is updated to expect the two `library-extraction` events in front
  of that one (task 3.1).
- Documents a person reads, written by the doc writer (task 4):
  - `docs/manual.org`, the `borax status` section and the JSONL
    schema paragraph;
  - `CHANGELOG.md`.
- `openspec/STATE.md` records the built state.
- No new dependency, configuration key or flag.

## Deferred

- **The architecture's second user.** Change 5,
  `list-untracked-missing-unlinked`, builds on the boundary design D1
  records. It is not designed here.
- **Comparing an extracted identifier with the library item.** This
  belongs to resolution evidence (Phase 3) or a validate-in-status
  inspection. Either can call `pipeline::extraction` over the files it
  selects.
- **Restoring the distinctions in skip reasons** and **keeping title
  claims through extraction failure**. Both are Phase 3, under its one
  intentional schema bump. Phase 3 can reuse this change's kind names
  or rename them. Nothing here fixes its choice (design D2).
