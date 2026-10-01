# Tasks: report-extraction-per-file

Work on branch `change/report-extraction-per-file`, cut from `main` at
0.7.0. Nothing below waits on another change.

Every group is a red/green pair. The red tasks are written and run
failing first, touching test files only, and the green task makes them
pass without editing them. A new test may open red on an unresolved
name from design D10 (`Extraction`, `Event::LibraryExtraction`,
`Extraction::is_found`, `pipeline::extraction_of`,
`pipeline::extraction`, `library::extraction_event`). Write it so that
it fails on its assertion once those names exist.

The interface is fixed in design D10, the vocabulary in design D2, and
the exact human strings in design D7. The event schema version stays 3
throughout (design D9). Two things are out of bounds:

- `SCHEMA`, `Counts` and `Command::summary`.
- `pipeline::skipped_for` and every `SkipReason`.

A test that needs either changed means the task has been misread.

Fixtures: the `FakePdf` and `FakeDocuments` fakes already in
`crates/borax/tests/pipeline.rs` and `dispatch.rs` cover every case:

- **Blank page with a title:**
  `FakePdf::new().with_pages(vec![Ok(" \n".to_string())]).with_title("A Title")`.
- **Readable prose with a title:** the same with a page of prose, as in
  `pdf_with_no_identifier()`, plus `.with_title("A Title")`.
- **Blank page whose title holds a DOI:** the blank-page fake with
  `.with_title("10.1234/example")`. `scan_info` scans the Info title,
  so this is `found` (design D2).
- **Encrypted and unreadable:** an open error of
  `ExtractionError::Encrypted` or `ExtractionError::Unreadable { message }`.

The real backend is exercised on the committed corpus in task 3.3.

## 1. The result vocabulary and the mapping

- [x] 1.1 Red: `crates/borax/tests/event.rs`:
      - For each `Extraction` variant, `json_line` of an
        `Event::LibraryExtraction` gives the exact line design D4 shows.
        That is `"schema":3`, `"event":"library-extraction"`, `path`
        as given, and `extraction` tagged by `kind` in kebab-case:
        `found` with `identifier` and `tier`, `no-text-layer`,
        `text-without-identifier`, `encrypted`, and `unreadable` with
        `message`.
      - Each line deserializes back to the same event.
      - `Extraction::is_found` is true for `Found` alone.
      - `Counts::observe` of a `LibraryExtraction` event, of each
        kind, leaves every counter at zero (design D8).
- [x] 1.2 Red: `crates/borax/tests/pipeline.rs`:
      - `extraction_of` maps `Ok(Extracted)` with a DOI and
        `Tier::EmbeddedMetadata` to `Found { identifier: "doi:…",
        tier: "embedded-metadata" }`.
      - `extraction_of` maps an arXiv identifier with a version and
        `Tier::TextLayer` to `Found { identifier: "arXiv:…v2", tier:
        "text-layer" }`.
      - `extraction_of` maps each `ExtractionError` variant to its row
        in design D2's table, and `Unreadable` keeps its message
        unchanged.
      - `extraction` over `FakeDocuments`:
        - the blank page with a title is `NoTextLayer`;
        - readable prose with a title is `TextWithoutIdentifier`;
        - the blank page whose Info title is `10.1234/example` is
          `Found { identifier: "doi:10.1234/example", tier:
          "embedded-metadata" }`;
        - a document with no pages is `NoTextLayer`;
        - an XMP DOI is `Found` with `embedded-metadata`;
        - an open error of `Encrypted` is `Encrypted`;
        - an open error of `Unreadable { message }` is `Unreadable`
          with that message;
        - a `page_text` error of `Unreadable` on the first page is
          `Unreadable`.
      - `extraction` with `page_limit: 0` over a document whose only
        page prints a DOI is `NoTextLayer`.
      - Regression guard, pinning what this change must not touch:
        `resolve_file` over the same blank-page and prose fakes still
        skips both with `SkipReason::NoIdentifier`, and over the
        encrypted one still skips with `SkipReason::Unreadable`
        carrying `"PDF is encrypted"`. Doc comment: the collapse is a
        known deviation that Phase 3 owns.
- [x] 1.3 Green:
      - `Extraction` and `Extraction::is_found` in
        `crates/borax/src/event.rs`, serde only, with
        `Event::LibraryExtraction { path: String, extraction:
        Extraction }`.
      - `human_line` gets a placeholder arm for the new variant that
        returns `None`, so the match stays exhaustive until group 2.
      - `extraction_of` and `extraction` in
        `crates/borax/src/pipeline.rs`. `extraction_of` is an
        exhaustive match with no wildcard arm. `extraction` is
        `from_file` with its claims dropped, passed through
        `extraction_of`.
      - Docstrings state the contract: one file, never fails, no
        service, no cache, no library write. They do not mention the
        rejected designs.

## 2. The human line

- [x] 2.1 Red: `crates/borax/tests/event.rs`:
      - `human_line` of `LibraryExtraction` gives each string in
        design D7's table exactly, including the `from the file`
        fallback for a `tier` that is neither pass.
      - Each control character in the path, the identifier and an
        `unreadable` message is written as `\xNN`. For example, a
        message holding `"\u{1b}[2J"` renders `\x1b[2J`. `json_line`
        of the same event carries the raw character, encoded by JSON.
      - `render(Format::Human, e)` equals `human_line(e)` for every
        kind.
- [x] 2.2 Green: replace the placeholder arm with the D7 rendering in
      `crates/borax/src/event.rs`, passing the path, the identifier and
      the message through `crate::describe::escaped`. Do not change any
      existing line's rendering.

## 3. `status --identify` reports every artifact

- [x] 3.1 Red: `crates/borax/tests/dispatch.rs`, `events_for` with
      `Command::status(.., true)`:
      - Update
        `status_identify_counts_artifacts_yielding_an_identifier_and_queries_no_source`
        to expect, in order:
        - `LibraryExtraction { path: "has-doi.pdf", Found { … } }`;
        - `LibraryExtraction { path: "no-doi.pdf",
          TextWithoutIdentifier }`;
        - the existing `LibraryStatus { identifiable: Some(1), .. }`.

        Keep its scenario doc comment, add "Each artifact is reported
        with what extraction found", and state the updated assertion
        in the commit.
      - A library with `a.pdf` (XMP DOI) and `sub/b.pdf` (arXiv
        identifier on its first page, none in metadata) gives events
        in survey order. `sub/b.pdf` has the path `"sub/b.pdf"`, with
        `/` on every platform. `LibraryStatus` comes last, with
        `identifiable: Some(2)`.
      - The two controlled cases in one library give `NoTextLayer` for
        the blank page and `TextWithoutIdentifier` for the prose, with
        `identifiable: Some(0)`. Adding the blank page whose title
        holds a DOI gives `Found` with `embedded-metadata` for it and
        `identifiable: Some(1)`.
      - Encrypted and unreadable open errors give `Encrypted` and
        `Unreadable { message }`.
      - In every case above, `identifiable` equals the number of
        `LibraryExtraction` events whose result `is_found()`, asserted
        by counting the returned events (design D5).
      - `status` without `--identify` over the same library emits no
        `LibraryExtraction` event.
      - An unmarked directory (`collection_root: None`) names paths
        relative to the directory given.
- [x] 3.2 Red: `crates/borax/tests/dispatch.rs`, the selection
      boundary. Use a `Documents` fake that records every path passed
      to `open` and `hash`. The library holds:
      - `paper.pdf`;
      - `paper.pdf.bib`;
      - a PDF under `items/`;
      - a PDF under `.borax/`;
      - a nested `.borax.toml` directory holding a PDF;
      - on `#[cfg(unix)]`, a symlink to a PDF outside the tree.

      Exactly one `LibraryExtraction` event is emitted, for
      `"paper.pdf"`, and the fake saw `paper.pdf` alone.
- [x] 3.3 Red: `crates/borax/tests/end_to_end.rs`, the real backend.
      Run `status --identify --json` through `invoke` over
      `library_of` copies of these fixtures:
      - `publisher-info-doi.pdf`;
      - `arxiv-new-id.pdf`;
      - `no-text-layer.pdf`;
      - `no-identifier.pdf`;
      - `encrypted-user-password.pdf`;
      - `malformed-truncated.pdf`.

      Expected results, in path order:
      - `publisher-info-doi.pdf`: `found` with
        `doi:10.1234/borax.2024.001` and `embedded-metadata`;
      - `arxiv-new-id.pdf`: `found` with `arXiv:2401.12345v2` and
        `text-layer`;
      - `no-text-layer.pdf`: `no-text-layer`;
      - `no-identifier.pdf`: `text-without-identifier` (this fixture
        carries an Info `Title`);
      - `encrypted-user-password.pdf`: `encrypted`;
      - `malformed-truncated.pdf`: `unreadable` with a non-empty
        `message`.

      `library-status` is the last event before `run-finished`, with
      `identifiable` 2. `run-finished` counts zero skipped, the outcome
      is `Outcome::Success`, and the transport saw no request. Avoid
      `publisher-text-doi.pdf` and `doi-on-third-page.pdf`, which the
      corpus README records as red on the pure backend.
- [x] 3.4 Red: `crates/borax/tests/dispatch.rs`, `dispatch` in human
      mode:
      - Over a library holding only the two controlled cases and an
        unreadable file, the output is one D7 line per artifact in path
        order, then the report line ending `0 identifiable` as the last
        line. No line contains `resolved,` or `skipped`. The outcome is
        `Outcome::Success`.
      - The same run with `--json` ends on `run-finished` with all
        seven counters zero.
      - An unreadable artifact whose message holds `\u{1b}` renders
        `\x1b` on its line.
- [x] 3.5 Red: `crates/borax/tests/streaming.rs`, liveness, modelled on
      `rename_preview_writes_each_file_s_resolved_line_before_the_next_file_is_hashed`.
      - `LiveDocuments` snapshots the shared writer when a file is
        hashed, and `status --identify` never hashes. Give the fake a
        second record, written the same way when `open` is called,
        with its own accessor. Leave the existing `hash` snapshots and
        the test that reads them unchanged.
      - Make a `tempdir` library with a `.borax.toml` and `a.pdf`,
        `b.pdf` and `c.pdf` on disk, because the survey walks the real
        tree. Give the fake entries at those paths: `a.pdf` with an
        XMP DOI, `b.pdf` with a page of prose, `c.pdf` with an XMP
        DOI.
      - Run `dispatch` with `Command::status(Some(root), true)` and
        `--json`, writing stdout to a `SharedWriter` over the buffer
        the fake watches.
      - When `b.pdf` is opened, the buffer already holds `a.pdf`'s
        `library-extraction` line. When `c.pdf` is opened, it holds
        the lines for `a.pdf` and `b.pdf`. No open-time snapshot holds
        a `library-status` line.
      - The test fails against an implementation that collects the
        events and emits them after the loop (design D5).
- [x] 3.6 Red: `crates/borax/tests/library.rs`, `extraction_event`
      over a `survey` of a fixture library gives
      `Event::LibraryExtraction` with the artifact's library-relative,
      `/`-separated path and the extraction unchanged. This covers an
      artifact at the root and one two directories down.
- [x] 3.7 Green:
      - `library::extraction_event` in `crates/borax/src/library.rs`,
        through `library_relative`, with the fallback design D6
        states.
      - `run::status_events` in `crates/borax/src/run.rs` emits each
        artifact's event as its extraction finishes, counts
        `is_found()` results, and then emits `status_event(&surveyed,
        Some(count))`. Without `identify` it is unchanged. Remove the
        `.is_ok()` filter.
      - Update the docstrings of `status_events`, `Survey` (whose
        "identifiable count" sentence now names per-file results),
        `status_event` and `Event::LibraryStatus` so they state the
        new contract.

## 4. Documents

- [x] 4.1 Doc writer (`codex-docs`), an update job on `docs/manual.org`.
      Sources: this proposal, its design and its spec deltas. The
      implementer does not write it. The orchestrator checks the diff
      against the sources.
      - `borax status` (lines 489–515): `--identify` now lists each
        artifact with its identifier and the pass that found it, or
        why extraction failed. Give the five outcomes with their D7
        lines. The lines come before the report line, which stays
        last. Failures do not change the exit status, and
        "identifiable" is the number of artifacts listed with an
        identifier.
      - The JSONL schema paragraph (lines 1449–1461):
        `library-extraction` carries `path` (library-relative) and
        `extraction`, whose `kind` is `found` (with `identifier` and
        `tier`), `no-text-layer`, `text-without-identifier`,
        `encrypted`, or `unreadable` (with `message`). It precedes
        `library-status`. Skip reasons are unchanged, and the schema
        is still 3.
- [x] 4.2 Doc writer, `CHANGELOG.md`, under Unreleased:
      - An "Added" entry for the `library-extraction` event and its
        five kinds, with the schema still 3.
      - A "Changed" entry: human output of `status --identify` lists
        each artifact's result before the report line, with exit
        status unchanged.
- [x] 4.3 Record the built state in `openspec/STATE.md`, as
      `consult-library-first` did:
      - a paragraph under "What is built" naming the D1 boundary that
        change 5 follows;
      - under "Known defects", an entry stating that resolution skip
        reasons merge `NoTextLayer` and `NoIdentifierFound` into
        `no-identifier`, and report `Encrypted` as `unreadable`.
        That contradicts the `extraction` requirement "Extraction
        failures are typed and non-fatal". `status --identify` now
        reports them distinctly, and Phase 3 owns the restoration
        under its schema bump.

## 5. Close

- [x] 5.1 `openspec validate report-extraction-per-file --strict` and
      `python3 scripts/check-spec-deltas.py` pass.
- [x] 5.2 `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` pass, as CI runs them.
