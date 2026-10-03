# Tasks: sectioned-resolved-event

Work on branch `change/sectioned-resolved-event`, cut from `main` at
`834ffbd`. This is change 9 of the roadmap's Phase 3, with change 3
folded in. It builds on change 8's evidence (`crates/borax/src/evidence.rs`).

Groups 1 to 8 are each a red/green pair. The red task is written and
run first, and it touches test files only. The green task then makes it
pass without editing those tests. A new test may open red on a name
from design D12 that does not exist yet. Write it so that it fails on
its assertion once the name exists.

The JSON shape is fixed by design D3 and the illustrative lines at the
end of design.md. The Rust interface is fixed by D12. The human strings
are fixed by D9 (lines) and D10 (description).

**This change alters output, so existing tests change.** Design D14
lists the files and gives the schema-3 to schema-4 mapping. The rules:

- **Kind (a), shape-only edits.** An edit that reshapes a literal or a
  pattern to compile against D12 must not change what the test
  expects. Examples:
  - `overrode: None` becomes `overridden: false`;
  - `Event::Skipped { …, library }` becomes `{ …, sections, candidate }`
    or `{ .. }`;
  - `Attempt` becomes `ServiceAnswer`;
  - `event_for` becomes `resolved_event` or `verdict_event`;
  - `.tier()`, `.cached()`, `.source()`, `.found()` and `.claims()`
    become reads of `evidence`.
- **Kind (b), deliberate rewrites.** Each one follows D14's mapping and
  asserts the schema-4 form of the same fact. No assertion is dropped
  because its field went. Each red task below names the kind-(b)
  rewrites it owns, and makes them in the same batch as its new tests.
- **Kind (c), deletions.** Only the tests D14 lists under (c) are
  deleted or replaced.

Task 9.1 audits every edit to an existing test against these rules.

**Fixtures.** Reuse change 8's fakes in `crates/borax/tests/pipeline.rs`
(`FakePdf`, `FakeDocuments` with `with_file`, `with_open_error` and a
hash error, `fake_source`, `PanicSource`, `CountingCache`, the
write-failing cache, and `Cached` over a `MemoryCache` for the response
cache). In `crates/borax/tests/dispatch.rs`, reuse the interactive
harness and `WriteFailingCache`. The controlled pair from
`report-extraction-per-file` (blank page with a title, prose page with
a title) is the no-text-layer/text-without-identifier case.

**Round-two cases.** Every case below is a test, in the group named:

| Case | Group |
|---|---|
| content-index hit | 3 |
| service cache vs network | 3 |
| failure then success | 3 |
| all not-found | 3 |
| no eligible service | 3 |
| no-text-layer vs text-without-identifier, with titles | 3, 6, 7, 8 |
| encrypted vs unreadable | 3, 7, 8 |
| unhashable | 3 |
| cache-write failure (content index; response cache; rename-time) | 3, 5 |
| conflict skip with candidate | 3, 4 |
| content duplicate and work duplicate | 1, 3, 4 |
| operator override | 4 |

## 1. The schema-4 event vocabulary

- [x] 1.1 Red, in `crates/borax/tests/event.rs`:
      - `SCHEMA == 4`, and every `json_line` carries `"schema":4`.
      - A `resolved` event built from the D12 types serializes to the
        D3 key set, in order: `schema`, `event`, `path`, `identifier`,
        `record`, then the seven section keys. The test builds the
        event from each of the design's illustrative lines (content
        index, network after failure, service cache, library) and
        compares the result with the JSON text written in the test.
      - It carries none of `found`, `cached`, `source`, `tier`,
        `claims` or `overrode`.
      - A resolution `skipped` event (the no-text-layer, unresolvable,
        conflict, content-duplicate and work-duplicate illustrations)
        serializes to `path`, `reason`, the seven sections, and
        `candidate` only on the conflict. The `duplicate` reason keeps
        `reason` (`content` or `work`) and `existing_path`.
      - A non-resolution `skipped` event (`target-taken`, `declined`,
        `rename-failed`, `stranding`) serializes to `schema`, `event`,
        `path` and `reason` and nothing else, with reason fields as in
        schema 3.
      - `SkipReason` kinds serialize as `{"kind":"no-text-layer"}`,
        `text-without-identifier`, `encrypted`, `unreadable` with
        `message`, `unresolvable` and `conflict`.
        `is_resolution_verdict()` is true for exactly those six and
        for `duplicate` (both reasons), and false for every other
        kind.
      - Every not-attempted step serializes as exactly
        `{"status":"not-attempted","reason":R}`.
      - `ServiceOutcome::Found` with `stored: None` omits `stored`.
        `record_retrieval: None` serializes as `null`.
      - `content-index-write` serializes as `{"schema":4,"event":
        "content-index-write","path":…,"write":{"status":"written"}}`,
        or `failed` with `message`.
      - The round-trip test covers the new events: one `resolved`, each
        resolution skip kind, a non-resolution skip, and
        `content-index-write`, all through `json_line` and back to an
        equal `Event`.
      - Kind (b): the `"schema":3` literals near lines 2263–2304 become
        `4`, and the JSON shape assertions on the removed fields and on
        the old reasons are rewritten by D14's mapping.
      - Kind (c): delete the two `…_with_no_library_key_deserializes_…`
        tests.
- [x] 1.2 Green, in `crates/borax/src/event.rs`:
      - set `SCHEMA = 4`;
      - add the D12 section types and `Event::ContentIndexWrite`;
      - reshape `Event::Resolved` and `Event::Skipped`;
      - reshape `SkipReason`, and add `is_resolution_verdict`;
      - remove `Attempt` and `Overridden`;
      - `Counts::observe` ignores `ContentIndexWrite`;
      - docstrings state each type's contract (D3 meanings).

      The rest of the crate keeps compiling through group 3. Callers
      may build placeholder sections only until group 3 replaces them,
      and no placeholder survives task 3.2.

## 2. Projecting the evidence onto the sections

- [x] 2.1 Red, in `crates/borax/tests/pipeline.rs` (a `sections`
      block). Build `Evidence` values directly and assert
      `Evidence::sections(acceptance)` field by field, covering every
      row of D3 and D5:
      - each `Consultation`, every `LibraryAnswer` kind nested under
        `answer`;
      - each `IndexRead` and `IndexWrite` (including
        `Attempted(Failed { message })`);
      - each `ExtractionStep` result kind and each `Titles` state,
        `Read(vec![])` included;
      - `LookupEvidence::Attempted` with attempts, which becomes
        `attempted` with `origin` `extracted` or `operator` and no
        tier;
      - `Attempted` with no attempts, which becomes
        `no-eligible-service`;
      - each `ServiceAttempt` outcome by D5's table, including
        `Network { stored: None }`, which becomes `stored` not
        attempted with reason `cache-bypassed`;
      - `record_retrieval` from `Evidence::retrieval()` for all four
        places and `None`;
      - each `MatchCheck`, with `Insufficient` reasons `record-untitled`,
        `no-titles` and `no-evidence`;
      - `acceptance` passed through;
      - `Unattempted::CacheBypassed.as_str() == "cache-bypassed"`, and
        every reason string in the output equals some
        `Unattempted::as_str()`.
- [x] 2.2 Green: in `evidence.rs`, add `Unattempted::CacheBypassed` and
      `Evidence::sections`, with matches that have no wildcard arms.
      Update the module docstring: the evidence is still not serialized
      itself, and `sections` is its projection.

## 3. Resolution events from the engine

- [x] 3.1 Red, in `crates/borax/tests/pipeline.rs`. Each case runs
      `standing` and asserts on `verdict_event`, or on
      `resolved_event` for a resolved file:
      - **Content-index hit**: D3's content-index illustration, section
        for section.
      - **Service cache vs network**: two files carrying one DOI through
        `Cached` over a `MemoryCache`. The first gives `network` with
        `stored` `written`. The second gives `service-cache` with no
        `stored`, and its source is asked once in total. Under
        `config(false)`, with no `Cached` wrapper: `content_index.read`
        is `bypassed`, `stored` is not attempted for `cache-bypassed`,
        and `write` is `written`.
      - **Failure then success**: Crossref `Unavailable`, then OpenAlex
        answers. The attempts are in that order, and `record_retrieval`
        is `network` naming OpenAlex.
      - **All not-found**: reason `{"kind":"unresolvable"}`, every
        outcome `not-found`, `match_check` and `content_index.write`
        not attempted for `no-record`, and `record_retrieval` `null`.
      - **No eligible service**: an arXiv identifier against sources
        that support only DOIs. `lookup` is `no-eligible-service` with
        the identifier and `extracted`, and the reason is
        `unresolvable`.
      - **No-text-layer vs text-without-identifier with titles**: the
        controlled pair. The reasons are `no-text-layer` and
        `text-without-identifier`, both report titles `read` with `A
        Title` from `info`, and `lookup` is not attempted for
        `extraction-failed`.
      - **Encrypted vs unreadable**: open errors `Encrypted` and
        `Unreadable { message }`. The reasons are `encrypted` and
        `unreadable` with the message, the extraction results match,
        and titles are `failed`.
      - **Unhashable**: a hash error with the DOI resolving.
        `content_index.read` is `unavailable` with the message, `write`
        is not attempted for `unhashable`, and the file resolves. The
        same holds under `config(false)`.
      - **Cache-write failure**: with a write-failing index, `write` is
        `failed` with the message, and the verdict is `resolved`
        exactly as with a working index. With a write-failing
        response cache under `Cached`, the found attempt's `stored` is
        `failed`.
      - **Conflict skip with candidate**: reason `{"kind":"conflict"}`.
        `candidate` equals the refused record, `match_check` is
        `conflict` with the similarity, `content_index.write` is not
        attempted for `refused`, `record_retrieval` is `network`
        naming Crossref, and `acceptance` is `not-applicable`.
      - **Library answer**: D3's library illustration.
      - **Content duplicate**: through `standing` with an `Account`
        holding the file's bytes, `verdict_event` gives a `skipped`
        event with the `duplicate` reason of kind `content`, sections
        equal to `Evidence::not_attempted(ContentDuplicate)` projected
        (every step not attempted with reason `content-duplicate`,
        `record_retrieval` `null`), `acceptance` `not-applicable`, and
        no `candidate`. This is D3's content-duplicate illustration.
      - **Work duplicate**: through `standing` with an `Account` whose
        library holds another file of the work the DOI resolves to,
        `verdict_event` gives a `skipped` event with the `duplicate`
        reason of kind `work`, sections equal to
        `duplicated.file.evidence` projected (`library` as the
        library answered, the lookup with its attempts,
        `record_retrieval` naming the service), `acceptance`
        `not-applicable`, and no `candidate`. A second case runs it
        through `Cached` to give `service-cache`. This is D3's
        work-duplicate illustration.
      - **Non-resolution skips**: `target-taken` and `unnameable`,
        from `renaming::Applying::carry_out`, give `skipped` events
        whose `sections` and `candidate` are `None`.
      - **`resolution_skip`**: it builds the same event `verdict_event`
        gives for a standing with that reason and evidence.
      - Kind (b): the outcome assertions on `SkipReason::NoIdentifier`,
        `Unresolvable { .. }` and `Conflict { .. }`.
      - Kind (c): replace the three `resolve_file_still_skips_…` tests
        and their comment block with
        `resolve_file_skips_a_blank_page_as_no_text_layer`,
        `resolve_file_skips_prose_as_text_without_identifier` and
        `resolve_file_skips_an_encrypted_file_as_encrypted`.
      - Kind (a) across `pipeline.rs`, `renaming.rs`, `bib.rs` and
        `per_file.rs`, as the preamble lists. That includes the
        `Event::Resolved` literals in `renaming.rs` (near lines 1202,
        1216 and 1269) and `per_file.rs` (near line 264), which take
        `sections`. Design D14's table lists every site per file.
- [x] 3.2 Green, in `crates/borax/src/pipeline.rs`:
      - `skipped_for` maps each failure to its own kind (D4 table, no
        wildcard arm);
      - add `resolution_skip`;
      - `verdict_event` and `resolved_event` build sections through
        `Evidence::sections`, and the conflict skip carries `candidate`
        from `Standing::refused`;
      - replace `overrode` with `overridden`, and add
        `FileRecord::acceptance` and the retyped `conflict()`;
      - `attempts_of` returns `ServiceAnswer`s;
      - remove `event_for`, `unresolvable`, `sources_of`, `Provenance`
        and the five projection methods.

      In `crates/borax/src/renaming.rs`, `carry_out`'s skips carry no
      sections. Update `resolve_file`'s docstring, whose mapping
      paragraph describes the merged reasons.

## 4. Acceptance and the interactive driver

- [x] 4.1 Red, in `crates/borax/tests/dispatch.rs` (JSON stream of an
      interactive run through the harness) and `pipeline.rs`:
      - **Operator override**: renaming over the file's own conflict.
        The `resolved` event carries `match_check` `conflict` with the
        field, both titles and the similarity, `acceptance`
        `overridden`, and `content_index.write` not attempted for
        `awaiting-acceptance`. In `pipeline.rs`, `accept` sets
        `overridden` and leaves `match_check` alone. A record without a
        conflict gets `overridden == false`.
      - **Supplied identifier**: `lookup.origin` is `operator`,
        `acceptance` is `automatic`, and `extraction.result` stays as
        the file's own while the titles are `read`.
      - **Retry after an outage that now answers**: `lookup.origin` is
        `extracted`.
      - **Retry that finds every service saying not found**: the held
        skip is `unresolvable` with the second lookup's attempts, and
        the next menu offers no retry.
      - **Retry that lands on a conflict**: the held skip is `conflict`
        with `candidate`.
      - **Supply over a tracked file**: `library` is `consulted`
        `tracked`, and `record_retrieval` names the service.
      - **Skipping a file with no identifier**: it is reported with the
        extraction kind, not `declined`. An encrypted file is still
        offered Supply.
      - **A batch rename's conflict and unresolvable skips**: they carry
        sections, while `target-taken` and `declined` skips carry
        none.
      - **Duplicates in a run**: a batch `rename --apply` over a
        library reports a byte-identical copy as a `duplicate`
        (`content`) skip with every section not attempted for
        `content-duplicate`, and a second file of a held work as a
        `duplicate` (`work`) skip carrying its record's evidence,
        `library` included. In an interactive run, declining to file a
        second artifact reports the same work-duplicate skip with the
        same sections, and filing it emits a `resolved` event instead.
      - Kind (b): the `tier`, `cached`, `library` and `no-identifier`
        assertions in `dispatch.rs`, and the `SCHEMA` / "schema 3"
        checks near lines 13857–14021.
- [x] 4.2 Green, in `crates/borax/src/run.rs`:
      - `Settled::Skip` carries the held verdict's sections and
        candidate (D11);
      - `situation` matches the four extraction kinds;
      - the retry paths build their skips through `resolution_skip`;
      - `alone` passes no `library` for non-resolution skips;
      - `reidentified` reads the lookup's origin and `overridden`.

## 5. The rename-time content-index write

- [x] 5.1 Red, in `crates/borax/tests/dispatch.rs` and
      `crates/borax/tests/runlog.rs`:
      - **A supplied identifier renamed**: the JSON stream holds
        `resolved` (`awaiting-acceptance`), `renamed`, then
        `content-index-write` with the target path and `written`, with
        nothing about the file in between except a `library-admission`
        where the run records one. The run log holds the same events
        once each, and the log equals the `--json` stdout.
      - **Rename-time write failure**: with `WriteFailingCache` as the
        index, `write` is `failed` with the message, the file is
        renamed, `run-finished` counts are unchanged, and the exit
        status is what it is with a working index.
      - **No event**: none on a batch `--apply` run, a declined move, a
        skip, a quit, or a supplied record kept under `Keep this name`.
- [x] 5.2 Green: in `run.rs`, replace `let _remembered` with an emit of
      `Event::ContentIndexWrite` from `remember`'s `IndexWrite` (D8).
      An `IndexWrite::NotAttempted` emits nothing. Update the comment
      there, which says the event is change 9's to choose.

## 6. Human lines and escaping

- [x] 6.1 Red, in `crates/borax/tests/event.rs` (`human_line`) and in
      `dispatch.rs` (human output of a batch run):
      - The `resolved` line for each `record_retrieval`, matching the
        D9 examples exactly. Further cases:
        - authors: one, two (`Smith and Jones`), three or more
          (`Smith et al.`);
        - no title; no authors; no year; none of the three, which
          leaves out ` to …`;
        - no service named, which leaves out ` via …`;
        - two provenance services, Crossref named before OpenAlex;
        - the library-problem suffix after `from the content index`.
      - The resolution skip lines of D9's table: each extraction kind;
        unresolvable, attempted and `no-eligible-service`; conflict.
      - Escaping: a title, an author and a claimed title carrying
        `\x1b[2J`, a service message and a library item path carrying
        a control character, all shown as `\x1b`. The same holds for a
        non-resolution skip's message (`rename-failed`).
      - `content-index-write`: `written` renders nothing, and `failed`
        renders D8's line with the message escaped.
      - Kind (b): the exact-string `human_line` tests for `resolved`
        (`via …`, `(cached)`, `(from the library)`) and for the old
        skip reasons, and the human output assertions in `dispatch.rs`.
- [x] 6.2 Green, in `event.rs`:
      - rewrite the `resolved` arm of `human_line`, and
        `skipped_because`, which now takes the sections;
      - move `sources_of` to `services_of`;
      - escape the clause after `<path>: ` once, in `human_line`, for
        `resolved`, `skipped` and `content-index-write`.

## 7. The interactive description

- [x] 7.1 Red, in `crates/borax/tests/describe.rs`, and in
      `dispatch.rs` for what an interactive run's terminal shows:
      - `file says` in each state of D10's table. Read with none gives
        `no title in its metadata`, failed gives `could not be opened
        (…)`, the index gives `not read; an earlier run answered`, and
        the library gives `not read; the library answered`.
      - Failed verdicts per D10's table:
        - the blank page with a title shows `identifier  none found;
          the pages read hold no text` and `file says  A Title
          (document info)`;
        - the prose page with a title shows the text-without-identifier
          line;
        - an encrypted file shows `identifier  none read; the file is
          encrypted` and the failed-titles line;
        - an unresolvable file shows the `no record` block from
          `lookup` and its titles;
        - `no-eligible-service` shows `no source was asked`.
      - A conflict skip's lines come from `match_check`, as today's.
      - An accepted conflict's `conflict` line comes from `match_check`
        when `acceptance` is `overridden`.
      - The `identifier` and `record` lines read `lookup`,
        `extraction.result.tier` and `record_retrieval`, giving
        today's wording.
      - Kind (b): the expected `nothing read` lines and the failed
        verdict lines. Kind (a): the `Event`, `Attempt` and
        `Overridden` literals.
- [x] 7.2 Green, in `describe.rs`: render from the sections;
      `Candidate::Unheld` takes `&[ServiceAnswer]`; update `describe`'s
      docstring list of lines.

## 8. The real binary

- [x] 8.1 Red:
      - In `crates/borax/tests/end_to_end.rs`, the stream-shape test's
        `assert_eq!(event["schema"], Value::from(3))` (near line 389)
        asserts `SCHEMA` (4). `BATCH` expects
        `text-without-identifier` for `no-identifier.pdf` and
        `encrypted` for `encrypted-user-password.pdf`. For
        `doi-past-page-range.pdf`, run `borax status --identify` on the
        fixture and expect whatever kind it reports. The `tier`,
        `source`, `found` and `cached` assertions (near lines 450–457
        and 1161) become `extraction.result.tier`, `record_retrieval`,
        `lookup.identifier` and `record_retrieval.kind ==
        "content-index"`.
      - In `crates/borax/tests/binary.rs`, the library-answer test
        (near line 471) asserts `record_retrieval.kind == "library"`
        and `library.answer.kind == "tracked"`.
      - A test runs `borax resolve --json` and `borax rename --apply
        --json` over the fixtures. It checks every `resolved` and
        `skipped` line against the D3 key set, and makes the
        removed-field assertions task 9.3 names.
- [x] 8.2 Green: nothing beyond groups 1–7 is expected. If something is,
      the fix goes in the group that owns it, and the report says so.

## 9. Audit and verification

- [x] 9.1 Audit every edit to a pre-existing test against the preamble.
      Each one must be kind (a), kind (b) following D14's mapping, or
      a kind-(c) deletion that D14 lists. Report any test that needed
      anything else.
- [x] 9.2 Run `cargo test --workspace`, `cargo clippy --workspace
      --all-targets -- -D warnings` and `cargo fmt --check`.
- [x] 9.3 Audit the removed fields at their former JSON locations, not
      as bare strings. Schema 4 still uses `found` (a status in
      `extraction.result` and an attempt outcome), `tier` (in
      `extraction.result`) and `claims` (in `extraction.titles`), and
      the unchanged `library-extraction` tests assert `found` and
      `tier`. A repository-wide string ban therefore cannot pass. The
      audit is:
      - a test that runs `borax resolve --json` and `borax rename
        --apply --json` over the fixtures (task 8.1) and asserts that
        no `resolved` event has a top-level `found`, `cached`,
        `source`, `tier`, `claims` or `overrode` key, and that no
        resolution `skipped` event's `reason` object has a `found`,
        `tier`, `attempts`, `field`, `extracted`, `resolved` or
        `similarity` key;
      - a whole-word search of `crates/borax/src/` for `Attempt`,
        `Provenance`, `event_for` and `NoIdentifier`, a search for the
        removed `Overridden` struct (any `Overridden` not written as
        `Acceptance::Overridden`, which D12 adds), and a search for
        `"no-identifier"`, which must find nothing (`rg -w`, so
        `ServiceAttempt`, `Unattempted` and `attempts` do not match);
      - a search of `crates/*/tests/` for `"schema":3`, `"schema": 3`
        and `Value::from(3)` used as a schema version, which must find
        nothing.
- [x] 9.4 Run `openspec validate sectioned-resolved-event --strict` and
      `python3 scripts/check-spec-deltas.py`.
- [x] 9.5 By hand (the `resolve` part was run over the corpus; the
      interactive pty part was not, and is covered by the dispatch and
      run-log tests instead), over a slice of the real-PDF corpus (outside the
      repository), run `borax resolve` and `borax resolve --json`, and
      an interactive `borax rename` through a pty. Check:
      - the new human line;
      - a blank scan's skip;
      - an encrypted file's skip;
      - the description's `file says` states;
      - one supplied identifier producing `content-index-write`.

## 10. Documents and state

- [x] 10.1 Doc writer (`codex-docs`), update job on `docs/manual.org`,
      Org, for users of the CLI. Sources: this change's design.md (D3,
      D8, D9, D10 and the illustrative lines), proposal.md, and the
      implemented `human_line` and `describe`. Passages:
      - `** borax resolve`: the tracked-file line `<path>: resolved
        <identifier> via <source> (from the library)`, which becomes
        the D9 line and its `from` clause.
      - `*** The interactive session`:
        - the example description's `file says   nothing read`;
        - the paragraph explaining `record` (`from an earlier run`,
          `from the library`);
        - the `file says` paragraph ("It reads ~nothing read~ when …");
        - the skip paragraph ("such as ~no identifier found~");
        - "A file with no identifier shows ~identifier  none found in
          the file~";
        - "An unreadable file shows the error on an ~unreadable~ line",
          which now distinguishes encrypted.
      - "*Remembering your answer.*": the `content-index-write` event,
        and the line a failed write prints.
      - `* Run logs`:
        - the paragraphs on `found`, `claims`, `resolved.source`,
          `cached`, `tier` and `overrode`;
        - the `library` field and its kinds, which now nest under
          `library.answer`;
        - the `unresolvable` reason's `found`, `tier` and `attempts`;
        - "The event schema version remains 3" and "Schema 3 adds the
          library events";
        - the `"schema":3` example lines (`library-extraction`,
          `library-condition`, and the `renamed` example under
          "Recovering the names a run replaced").

        Replace them with the schema-4 sections, the slim skip reasons
        and `candidate`, and the new event.
- [x] 10.2 Doc writer, update job on `README.md`, Markdown. Its two
      console examples (`resolved … via crossref`, `(cached)`, and
      `skipped, no identifier found`) take the D9 lines.
- [x] 10.3 Doc writer, update job on `CHANGELOG.md`, Markdown. Under
      `## [Unreleased]`, add a `### Changed` entry marked breaking that
      names the event schema bump from 3 to 4. It should cover:
      - the removed fields;
      - the sections;
      - the slim resolution reasons and their four extraction kinds;
      - `candidate`;
      - non-resolution skips losing `library`;
      - the new `content-index-write` event;
      - the new human `resolve` line;
      - the escaping.

      The existing Unreleased entry says "the schema remains at version
      3" about change 8. That entry stays as a statement about change 8,
      and the new entry says the schema is now 4.
- [x] 10.4 Implementer, `openspec/STATE.md`:
      - add a paragraph for this change after the change-8 paragraph,
        and replace that paragraph's last sentence, which says change 9
        renders the evidence;
      - close the defect "Resolution skip reasons merge distinct
        extraction failures" by deleting it, and its regression guard
        is gone (task 3.1);
      - narrow "Human output other than the description passes metadata
        through unescaped" to adoption's `unreadable` and `unwritten`
        messages, saying that the conflict skip line and every
        `resolved` and `skipped` line are now escaped;
      - set "Last reviewed".
- [x] 10.5 The orchestrator checks each doc-writer diff against its
      sources for invented facts and against the old text for dropped
      ones, and answers or removes every `TODO(docs)` marker.
