# Tasks: list-untracked-missing-unlinked

Work on branch `change/list-untracked-missing-unlinked`, cut from
`main` after `report-extraction-per-file` was merged. Nothing below
waits on another change.

Every group is a red/green pair. The red tasks are written first and
run failing, touching test files only. The green task makes them pass
without editing them. A new test may open red on an unresolved name
from design D10:

- `Condition`
- `Event::LibraryCondition`
- `Adoption::Unindexed`
- `library::orphan_events`
- `Validation::conditions`
- `Validation::orphans()`, `missing()` and `unlinked()`

Write each test so that it fails on its assertion once those names
exist.

The interface is fixed in design D10, the event shape in D3, and the
exact human strings in D7. The event schema version stays 3
throughout (D9). Three things are out of bounds:

- `SCHEMA`, `Counts`, `Command::summary` and `session::outcome_for`:
  no exit code changes (D1).
- `item_findings`, `record_findings` and every `Finding` kind (D8).
- `reconcile`, `reconciliation_events` and `Repair`.

A test that needs any of these changed means the task has been
misread.

Several existing tests are updated rather than left alone, because
they assert a whole event sequence or a line list that the new events
now join. Each update is named in its red task. The commit that makes
it states the updated assertion. Each updated test keeps its scenario
doc comment, and a sentence naming the new events is added.

## 1. The vocabulary

- [ ] 1.1 Red: `crates/borax/tests/event.rs`:
      - `json_line` of `Event::LibraryCondition` gives exactly the lines
        of design D3, one per kind:
        - `{"schema":3,"event":"library-condition","path":"sub/new.pdf","condition":{"kind":"orphan"}}`;
        - `missing` with `"id"` and then `"record"` after `"kind"`, as
          D3 shows;
        - `unlinked` with `"id"` after `"kind"`.
      - `json_line` of `Event::LibraryAdoption` with
        `Adoption::Unindexed` is exactly
        `{"schema":3,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"unindexed"}}`.
      - Each line deserializes back to the same event.
      - `all_events()` gains one `LibraryCondition` of each kind and
        one `LibraryAdoption` with `Unindexed`. The existing
        every-event tests then cover them (single line, schema and
        tag, round trip, `render` agreement).
      - `Counts::observe` of each new event leaves every counter at
        zero (D8).
- [ ] 1.2 Green:
      - `Condition` and `Event::LibraryCondition { path: String,
        condition: Condition }` in `crates/borax/src/event.rs`, with
        serde only.
      - `Adoption::Unindexed`.
      - `human_line` gets a placeholder arm for `LibraryCondition`
        that returns `None`, so the match stays exhaustive until
        group 2. `what_was_adopted` gets a placeholder `Unindexed` arm.
      - Docstrings state the contract: what each kind means, what
        `path` and `id` name, and that a condition is neither a skip
        nor a finding. They do not mention the rejected designs.

## 2. The human lines

- [ ] 2.1 Red: `crates/borax/tests/event.rs`:
      - `human_line` gives each string in design D7's table exactly,
        for each condition kind and for `Adoption::Unindexed`. For
        example:
        - `"new.pdf: orphan; no artifact record names it"`;
        - `"gone.pdf: missing; artifact record <id> (.borax/artifacts/<id>.toml) names this path and the library has no artifact here"`;
        - `"items/milner1978.<uuid>.toml: unlinked; no artifact record links item <id>"`;
        - `"new.pdf: the content index holds no record of its bytes, so it is still an orphan"`.
      - A path holding `"\u{1b}[2J"` renders `\x1b[2J` on each
        condition line, a `record` holding it renders the same way on
        the `missing` line, and on a `library-adoption` line of every kind
        (`recorded`, `held`, `unreadable`, `unwritten`, `unindexed`).
        `json_line` of the same event carries the raw character.
      - The existing `library-adoption` lines for a path with no
        control character are unchanged byte for byte. Assert one per
        existing kind.
      - `render(Format::Human, e)` equals `human_line(e)` for every new
        event.
- [ ] 2.2 Green: replace both placeholders with the D7 rendering in
      `crates/borax/src/event.rs`. Pass every condition `path`, and the
      `LibraryAdoption` arm's `path`, through
      `crate::describe::escaped`. Leave every other line's rendering
      alone.

## 3. `status` names every orphan

- [ ] 3.1 Red: update the existing tests in
      `crates/borax/tests/dispatch.rs` that the orphan events break.
      Each library they build has no records, so every artifact is an
      orphan:
      - `status_over_a_marked_directory_of_new_pdfs_reports_every_count_unopened`:
        expect twelve `LibraryCondition { path: "paper-NN.pdf",
        Orphan }` events in path order, then the existing
        `LibraryStatus`.
      - The following tests expect, in front of their existing
        sequence, one orphan condition per artifact in survey order:
        - `status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source`
          (`has-doi.pdf`, `no-doi.pdf`);
        - `status_identify_reports_each_artifact_in_survey_order_before_the_totals`
          (`a.pdf`, `sub/b.pdf`);
        - `status_identify_tells_the_two_controlled_cases_apart`;
        - `status_identify_tells_encrypted_and_unreadable_apart`.
      - `status_identify_over_an_unmarked_directory_names_paths_relative_to_it`:
        `events.first()` is now the orphan condition for
        `"has-doi.pdf"`. The existing `LibraryExtraction` assertion
        is made on the first `LibraryExtraction` event.
      - `status_over_an_unmarked_directory_is_reported_as_given_and_writes_nothing`:
        the orphan condition for `"paper.pdf"`, then the existing
        `LibraryStatus`.
      - `status_identify_human_mode_lists_each_artifact_before_the_report_line`:
        three orphan lines (`a-blank.pdf`, `b-prose.pdf`,
        `c-unreadable.pdf`, in the D7 form) in front of the existing
        lines.

      Before committing, run every test in `dispatch.rs`,
      `end_to_end.rs` and `streaming.rs` that runs `status`. Any other
      test that fails only because an orphan condition appears is
      updated the same way and named in the commit. One that fails for
      another reason goes back to the orchestrator.
- [ ] 3.2 Red: `crates/borax/tests/dispatch.rs`, `events_for` with
      `Command::status`:
      - A library holding `kept.pdf`, which a record names at its own
        path, and `new.pdf`. Plain `status` emits exactly
        `LibraryCondition { "new.pdf", Orphan }` and then
        `LibraryStatus { orphans: 1, .. }`. A `CountingDocuments`
        records no open.
      - `sub/deeper/x.pdf` is named `"sub/deeper/x.pdf"`, with `/` on
        every platform.
      - A nested `.borax.toml` directory holding a PDF, a
        `paper.pdf.bib` sidecar, a PDF under `items/`, and (on
        `#[cfg(unix)]`) a symlink to a PDF outside the tree: none of
        them is named.
      - With `--identify`, every `LibraryCondition` precedes every
        `LibraryExtraction`. `LibraryStatus` is last.
      - In every case above, `LibraryStatus.orphans` equals the number
        of `LibraryCondition` events of kind `Orphan`, asserted by
        counting the returned events (D5).
      - No `LibraryCondition` from `status` has kind `Missing` or
        `Unlinked`, even over a library holding a record whose path
        holds no file and an item no record links to (D4).
- [ ] 3.3 Red: `crates/borax/tests/dispatch.rs`, `dispatch` in human
      mode:
      - Plain `status` over two orphans prints the two D7 orphan lines
        in path order, then the report line as the last line. No line
        contains `resolved,` or `skipped`. The outcome is
        `Outcome::Success`.
      - An orphan named `"e\u{1b}[2J.pdf"` renders `e\x1b[2J.pdf:` on
        its line. The `--json` run carries the raw path.
      - The `--json` run ends on `run-finished` with all seven counters
        zero.
- [ ] 3.4 Red: `crates/borax/tests/streaming.rs`, liveness. Model it on
      `status_identify_writes_each_artifact_s_extraction_line_before_the_next_file_is_opened`,
      using the same `LiveDocuments` open snapshots, which are not
      changed:
      - A `tempdir` library with `.borax.toml` and `a.pdf`, `b.pdf` and
        `c.pdf` on disk, none recorded. Run `status --identify --json`.
      - When `a.pdf`, the first file, is opened, the buffer already
        holds three `library-condition` lines, one per orphan.
      - No open-time snapshot holds a `library-status` line.
      - The test fails against an implementation that writes the orphan
        events after the extraction loop (D5).
- [ ] 3.5 Red: `crates/borax/tests/library.rs`, `orphan_events` over a
      `survey` of a fixture library. It gives one
      `Event::LibraryCondition { path, Orphan }` per entry of
      `survey.orphans`, in that order, with library-relative,
      `/`-separated paths. Cover an orphan at the root and one two
      directories down. A recorded artifact is absent.
- [ ] 3.6 Green:
      - `library::orphan_events` in `crates/borax/src/library.rs`. Share
        the library-relative fallback with `extraction_event` through
        one private helper (D10).
      - `run::status_events` writes `orphan_events(&surveyed)` straight
        after the survey, before the extraction loop, whether or not
        `identify` is set.
      - Update the docstrings of `status_events`, `Survey::orphans` and
        `status_event` so they state the new contract.

## 4. `validate` names every condition it counts

- [ ] 4.1 Red: update existing tests to the new `Validation` and to the
      new events:
      - `crates/borax/tests/library.rs`, the mechanical change from
        `result.orphans`, `.missing` and `.unlinked` to
        `result.orphans()`, `.missing()` and `.unlinked()`, with every
        expected value unchanged, in:
        - `validate_over_a_library_of_only_orphans_is_clean`;
        - `validate_reports_a_records_missing_artifact_as_a_count_not_a_finding`;
        - `validate_counts_a_record_inside_a_nested_library_as_missing`;
        - `validate_reports_an_unlinked_item_as_a_count_not_a_finding`;
        - `a_half_written_record_is_a_finding_about_itself_and_the_rest_of_the_library_is_unaffected`.
      - `crates/borax/tests/dispatch.rs`,
        `validate_emits_one_finding_event_then_the_totals`. The fixture
        holds the dangling record (`id` `LIB_UUID_A`, path
        `"orphaned-link.pdf"`, which holds no file) and `orphan.pdf`.
        Expect four events in this order:
        - the `LibraryFinding`;
        - `LibraryCondition { "orphan.pdf", Orphan }`;
        - `LibraryCondition { "orphaned-link.pdf", Missing { id:
          LIB_UUID_A, record: ".borax/artifacts/<LIB_UUID_A>.toml" }
          }`, the file `write_lib_artifact_record` writes;
        - the existing `LibraryValidated`.

        Rename the test to say what it now asserts, keeping its doc
        comment's scenario reference.
      - No other existing `validate_reports_*` test is edited.
- [ ] 4.2 Red: `crates/borax/tests/library.rs`, `validate`:
      - A library holding the following gives `conditions` equal to
        `[("new.pdf", Orphan), ("gone.pdf", Missing { id, record }),
        ("items/milner1978.<uuid>.toml", Unlinked { id })]`, with no
        findings:
        - `new.pdf`, which no record names;
        - a record naming `gone.pdf`, which holds no file;
        - an item file `items/milner1978.<uuid>.toml` that no record
          links.
      - Order within each kind: two orphans in path order, two missing
        records in store read order, two unlinked items in item store
        read order. The kinds come in the order orphan, missing,
        unlinked.
      - A record naming `nested/kept.pdf` under a nested
        `.borax.toml`, with the file present, gives one `Missing` and
        no `Orphan`.
        Here `record` is the record file's library-relative path,
        `.borax/artifacts/<file>.toml`.
      - Two item files carrying one identity, neither linked, give two
        `Unlinked` conditions and one `DuplicateIdentity` finding. This
        is today's count of 2, unchanged.
      - Two artifact record files carrying one identity, both naming
        `gone.pdf` where no file is (scenario "Two records of one
        identity are both named missing"): one `DuplicateIdentity`
        finding, and two `Missing` conditions with the same `path` and
        `id` whose `record` values are the two different record files,
        in store read order. `missing()` is 2, today's count.
      - The gate (D8): one library holding one instance of each of
        these findings:
        - an unreadable item file;
        - an item name that disagrees with its identity;
        - an item with an unparseable source field;
        - a duplicate item identity;
        - an unreadable record;
        - a record name that disagrees with its identity;
        - a non-relative path;
        - an empty history;
        - a malformed hash;
        - a history entry without a run;
        - a dangling item link;
        - a duplicate artifact identity.

        It also holds an orphan, a clean record whose path holds no
        file, and a clean item no record links. The findings equal,
        kind for kind and file for file, what the existing per-finding
        tests pin. The orphan, the clean missing record and the clean
        unlinked item each produce a condition and no finding.
        `conditions` is asserted in full. A record carrying a finding
        whose path holds no file is also `missing`, and an item
        carrying a finding that nothing links is also `unlinked`. The
        doc comment says so, because the two reports are independent.
        The unlistable store directory is left to its existing tests,
        because it cannot share a library with readable records.
      - For each fixture above, `orphans()`, `missing()` and
        `unlinked()` equal what the pre-change counting expressions
        give: `survey(root).orphans.len()`; `library::missing(…).len()`
        with `validate`'s `exists`; and the items with no
        `by_item` record. Compute these in the test, so the totals are
        shown unchanged for every fixture (D5).
- [ ] 4.3 Red: `crates/borax/tests/dispatch.rs`, `validate` through
      `events_for` and `dispatch`:
      - Over the three-condition library of 4.2, the events are the
        three `LibraryCondition`s, then `LibraryValidated { findings:
        0, orphans: 1, missing: 1, unlinked: 1 }`. The outcome is
        `Outcome::Success` (scenarios "A condition does not fail a
        run", "A missing artifact alone exits 0").
      - Over a library with only the missing record, the outcome is
        `Outcome::Success`. `run-finished` counts zero findings.
      - Over a library holding a dangling link whose file is present,
        an orphan and an unlinked item, the events are the finding, the
        orphan condition, the unlinked condition, then the totals. The
        outcome is `Outcome::Partial` ("Conditions are named beside the
        findings").
      - In human mode over the three-condition library: the three D7
        lines in kind order, then the existing
        `<root>: 0 findings, 1 orphans, 1 missing, 1 unlinked` line as
        the last line, with no summary line after it.
      - Over the two-records-of-one-identity library of 4.2, the
        events are the `DuplicateIdentity` finding, then two `missing`
        conditions that differ in `record` alone, then the totals with
        `missing: 2`. The outcome is `Outcome::Partial`, because of the
        finding.
      - In every case, each total on `LibraryValidated` equals the
        number of `LibraryCondition` events of its kind.
- [ ] 4.4 Green:
      - In `crates/borax/src/library.rs`, `Validation` replaces
        `orphans`, `missing` and `unlinked` with `conditions:
        Vec<(String, Condition)>` and the three counting methods.
      - `validate` builds `conditions` from `survey.orphans` (through
        the shared path helper), from the records over
        `ArtifactStore::files` that fail the same `exists` test
        `library::missing` applies (the record's `path` verbatim, its
        `id`, and its file through the shared path helper as `record`),
        and from the
        unlinked items over `ItemStore::files` (item file relative to
        the root, and the item's `id`). Use the filters it counts with
        today.
      - `validation_events` writes findings, then conditions, then
        `LibraryValidated` with the three counts from the methods.
      - Update the docstrings of `Validation`, `validate`,
        `validation_events` and `Event::LibraryValidated`.

## 5. `adopt` reports what it leaves unindexed

- [ ] 5.1 Red: `crates/borax/tests/dispatch.rs`:
      - Update
        `adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent`:
        - in the first run, `adoptions(&events)` holds
          `("unknown.pdf", Adoption::Unindexed)` beside `known.pdf`'s
          `Recorded`;
        - the second run's `adoptions(&again)` is exactly
          `[("unknown.pdf", Adoption::Unindexed)]`, replacing
          `is_empty()`;
        - the snapshot and stat assertions are unchanged, so the second
          run still writes nothing.
      - New ("Every orphan is accounted for"): three orphans — one the
        index answers for, one whose bytes an existing record holds,
        and one the index holds nothing for. In walk order the
        adoptions are `Recorded`, `Held` and `Unindexed`, one each, and
        `adopted_totals` is `(1, 2)`.
      - New: in every adopt test that runs `adopt_over`, assert two
        things:
        - the number of `library-adoption` events equals the orphan
          count `library::survey` gave before the run;
        - `adopted_totals(..).1` equals the number of adoption events
          that are not `Recorded`.

        A shared helper may assert both. Adding the helper call to an
        existing adopt test is an addition, not an edit of its
        assertions.
      - Update `adopt_after_the_cache_is_cleared_adopts_nothing_and_succeeds`:
        the JSON output also holds two `library-adoption` lines with
        `"kind":"unindexed"`, for `one.pdf` and `two.pdf`. Both are
        readable, and no record holds their bytes. The outcome is still
        `Outcome::Success`.
      - New: after `cache --clear`, a library holding a readable orphan
        no record holds, an orphan whose bytes an existing record holds,
        and (on `#[cfg(unix)]`) an orphan that cannot be read gives
        `Unindexed`, `Held` and `Unreadable` respectively. Clearing the
        cache does not make every orphan `unindexed` (D6).
      - New: in human mode, a library with one unindexed orphan prints
        its D7 line, then the `library-adopted` line as the last line,
        with no summary line. The outcome is `Outcome::Success`.
- [ ] 5.2 Green:
      - In `library::adopt`, push `(relative, Adoption::Unindexed)`
        where the `None` arm of `lookup` now `continue`s.
      - Update the docstrings of `adopt`, `Adoptions::adoptions`,
        `Adoption` and `Event::LibraryAdoption`. "An orphan the content
        index had no record for is not among them", and the matching
        sentence on the event, become the new contract: every orphan
        appears exactly once.

## 6. Documents

- [ ] 6.1 Doc writer (`codex-docs`), an update job on `docs/manual.org`.
      Sources: this proposal, its design and its spec delta. The
      implementer does not write it. The orchestrator checks the diff
      against the sources.
      - `borax status`: plain `status` now lists each orphan, in the D7
        line form, before the report line. Under `--identify` the
        orphan lines come before the extraction lines. The report line
        is unchanged and stays last.
      - `borax validate`: after the findings, it lists each orphan,
        missing record and unlinked item, in D7 form, then the totals
        line. Say what `missing` means here, and that it differs from
        `reconcile`'s `missing` (D4). The findings list and the exit
        status are unchanged. A condition never makes `validate` exit
        partial.
      - `borax adopt`: an orphan the content index holds nothing for is
        now reported on its own line. Every orphan gets exactly one
        line.
      - A short passage, in the `validate` section or beside the exit
        codes, on detecting conditions from a script. Exit status does
        not reflect them. Give the two `jq -e` recipes from design D1,
        and point to `library-condition` and `library-repair` for which
        objects are affected.
      - The JSONL schema paragraph:
        - `library-condition` carries `path` and `condition`, whose
          `kind` is `orphan`, `missing` (with `id` and `record`, the
          record file) or `unlinked` (with `id`), and what each `path`
          names;
        - where it comes in `status` and `validate` streams;
        - the new `adoption` kind `unindexed`;
        - the schema is still 3.
- [ ] 6.2 Doc writer, `CHANGELOG.md`, under Unreleased:
      - "Added": the `library-condition` event and its three kinds, and
        the `unindexed` adoption kind, with the schema still 3.
      - "Changed":
        - plain `status` lists each orphan before the report line;
        - `validate` lists each orphan, missing record and unlinked
          item before its totals line;
        - `adopt` reports every orphan, including those the content
          index cannot answer for, so a consumer counting
          `library-adoption` events should dispatch on `adoption.kind`;
        - exit status is unchanged.
- [ ] 6.3 Record the built state in `openspec/STATE.md`:
      - Under "What is built", a paragraph naming:
        - the second use of the D1 boundary;
        - the `library-condition` event;
        - `status` naming orphans only;
        - `validate` naming all three;
        - `adopt` accounting for every orphan;
        - the exit-status decision, with its reason and the deferred
          opt-in route.
      - Under "Known defects", an entry: `adopt` and `reconcile`
        collect their per-object events and write them after the last
        file is hashed. That deviates from the `cli` requirement "A run
        reports as it goes", and restoring it means passing a sink into
        `library::adopt` and `library::reconcile`.

## 7. Close

- [ ] 7.1 `openspec validate list-untracked-missing-unlinked --strict`
      and `python3 scripts/check-spec-deltas.py` pass.
- [ ] 7.2 `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` pass, as CI runs them.
