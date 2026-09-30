# Tasks: consult-library-first

Work on branch `change/consult-library-first`. It is cut from `main`
after 0.6.0 and the archived `fit-summary-to-command`.

Every group is a red/green pair. The red tasks are written and run
failing first. The green task makes them pass without editing them. A
new test may open red on an unresolved name from design D10
(`LibraryAnswer`, `Stores::consult`, `Stores::record_faults`,
`Provenance::Library`, `verdict_event`, the new parameters). Write it so that it fails on its
assertion once those names exist.

The interface is fixed in design D10 and the exact strings in design
D5. The event schema version stays 3 throughout (design D4). A test
that needs `SCHEMA` changed means the task has been misread.

Fixtures: build a library in a `tempdir` with a `.borax.toml`, item
files under `items/` and artifact records under `.borax/artifacts/`,
written as `Item::to_toml` and `ArtifactRecord::to_toml` write them.
`crates/borax/tests/library.rs` and `dispatch.rs` already do this. A
content hash is the one `Documents::hash` returns for the fixture's
bytes.

An unlistable store is made with `chmod 000` on the directory. Those
tests are `#[cfg(unix)]`, restore the mode before the `tempdir` drops,
and return early when a probe shows the directory is still listable
(running as root).

## 1. What the library says about one file

- [x] 1.1 Red: `crates/borax/tests/library.rs`, `Stores::consult` over
      fixture libraries, one test per row of design D1's table, plus:
      - `Tracked` when the file's hash is the record's newest entry, and
        when it is only an older entry (rollback). `Consulted::item` is
        the linked item, and the identities in the answer are the
        record's and item's canonical text.
      - `Untracked` for a PDF no record names. `Untracked` for a
        byte-identical copy at another path of a recorded artifact.
        `Untracked` for a recorded artifact moved to a path no record
        names. `Untracked` for an unhashable file no record names.
      - `None` for a path outside the root, and for a path inside a
        nested library (`.borax.toml` below the root).
      - Paths are normalised (design D1a). A record naming
        `sub/paper.pdf` answers for the file reached as
        `<root>/sub/./paper.pdf`, as `<root>/x/../sub/paper.pdf`, and as
        a relative spelling from the test's working directory
        (`paths::route`). The same holds for a relative spelling of the
        root. `library_relative` and `excludes` already normalise
        (restoration `1d2954a`); rely on them.
      - `UnrecognisedContent` lists every record naming the path when
        none holds the hash. `Ambiguous` lists every record at the path
        that holds it, in read order. When two records name the path
        and only one holds the hash, that one answers (`Tracked`).
      - `UnreadableItem` when the linked item's file is named
        `<key>.<item-uuid>.toml` and does not parse. It carries that
        path and the parser's message. `DanglingItem` when no item file
        claims the UUID.
      - `UnreadableItem` carrying the `items/` directory when `items/`
        cannot be listed, never `DanglingItem`.
      - `AmbiguousItem` when two item files carry the linked identity.
        It carries both files in read order, and `Consulted::item` is
        `None`.
      - `UnreadableRecords { listed: true, unreadable: 1 }` for a PDF no
        record names when one artifact-record file does not parse. A
        tracked file whose own record parses is still `Tracked`.
        `UnreadableRecords { listed: false, unreadable: 0 }` when
        `.borax/artifacts/` cannot be listed. `Stores::record_faults`
        reports the same two facts, and is `None` for a store read
        whole or absent.
      - `ItemStore::read` and `ArtifactStore::read` over an absent
        directory give an empty store with no fault. Over an unlistable
        one they give a fault whose path is the directory.
      - `borax validate` over a library whose `.borax/artifacts/` cannot
        be listed reports an `unreadable` finding at that directory and
        exits partial (design D2a). `borax status` over it reports the
        counts it reports today.
      - The interim rollback rule (design D1): a record whose history is
        `[hash of work A, hash of work B]`, linked to work B's item,
        answers `Tracked` with work B's item for work A's bytes at its
        path. The test's comment names it an interim policy owned by
        change 15.
- [x] 1.2 Green:
      - `LibraryAnswer` in `crates/borax/src/event.rs`, serde only, no
        rendering yet.
      - In `crates/borax/src/library.rs`: `Consulted`, `RecordFaults`,
        `Stores::consult` and `Stores::record_faults`.
        - `consult` uses `library_relative` and `excludes`, which
          normalise both sides, `Account::is_incoming`'s comparison,
          `ArtifactRecord::holds` and `name_uuid`.
      - `store_files`: only `NotFound` is an empty store, and every
        other listing, iteration or metadata failure is a `StoreFault`
        (design D2a). Widen `Finding::Unreadable`'s docstring to a store
        file or directory.
      - Docstrings state the contract, not the rejected designs.

## 2. The event carries it

- [x] 2.1 Red: `crates/borax/tests/event.rs`:
      - `json_line` of a `resolved` event with each `LibraryAnswer`
        variant gives the objects in design D3, tagged by `kind` in
        kebab-case, with `"library":null` for `None`. The same holds for
        `skipped`. Every line carries `"schema":3`.
      - A `resolved` or `skipped` JSON line without a `library` key
        deserializes with `library: None`.
      - `human_line`: `tier: Some("library")` gives
        `<path>: resolved <id> via <source> (from the library)`. Each
        problem kind appends `; the library could not answer: <what>`
        with D5's wording, on a cached `resolved` line and on a
        `skipped` line alike, with one artifact and with several.
        `Tracked` with `tier: Some("supplied")`, `Untracked` and `None`
        leave the line byte-identical to today's.
- [x] 2.2 Red, mechanical: add `library: None` to every existing
      `Event::Resolved`, `Event::Skipped` and `FileRecord` literal under
      `crates/borax/tests/`. Give every exhaustive pattern on those
      variants a `..`. Change no assertion. State in the commit that
      the edit is mechanical.
- [x] 2.3 Green: the `library` fields, `#[serde(default)]` and always
      serialized. `Provenance::Library` with `as_str` `"library"`.
      `FileRecord::library`. `human_line` and a private function
      rendering `<what>`. `sources_of` reports `library` for a library
      answer whose provenance names no service, and `cache` as before
      for a content-index answer. Update the `Resolved` docstrings for
      `tier`, `cached`, `claims` and `source` so that each states its
      meaning with the new case. Construct a non-verdict `Skipped`
      (design D3's list) with `library: None`. The verdict skips get
      their answer from tasks 3.3 and 5.5 (design D11), and are not to
      be silenced here.

## 3. The pipeline asks the library first

- [x] 3.1 Red: `crates/borax/tests/pipeline.rs`, `standing` with a
      fixture library, stub `Documents` and `Source`s, and a content
      index over a cache that counts reads and writes:
      - Tracked: the verdict is `Resolved` with the item's record,
        `tier: Some(Provenance::Library)`, `cached: false`, empty
        claims, `found: None`, and `library` `Tracked`.
        `Documents::open` is never called, no source is asked, and the
        content index is neither read nor written. This holds with
        `ResolveConfig::cache` true and false.
      - `verdict_event` of that standing is a `resolved` event with
        `found` equal to the record's identifier, `source` from the
        item's provenance, and `library` `Tracked`. An item with no
        provenance gives `source: "library"`.
      - Each problem kind resolves through the content index or the
        extraction and sources path as an untracked file would. The
        content index is written on a fresh success. The `FileRecord`,
        or the standing when the verdict is a skip, carries the problem.
        `verdict_event` puts it on the `resolved` or `skipped` event.
      - A dangling item whose file carries no identifier: the verdict is
        `NoIdentifier`, and its `verdict_event` carries `DanglingItem`.
      - With an `Account`: a file whose bytes another live record holds
        is a content duplicate with `library: None`, and the library is
        not asked. A library answer is subject to the work check.
      - `library: None` as the argument: behaviour and events are
        today's (`library: None` on every event).
      - `resolve_batch` with a fixture library gives the same events at
        concurrency 1 and 8.
- [x] 3.2 Red, mechanical: pass `None` as the new `library` argument at
      every existing call of `standing` and `resolve_batch` in
      `crates/borax/tests/`. Change no assertion.
- [x] 3.3 Green: `standing` takes `library: Option<&Stores>`. It asks
      the library after the content-duplicate check and before the
      content index, and skips the index read when the library answers.
      A library answer goes through `admissible`'s work check as a
      content-index answer does. Add `Standing::library` and
      `verdict_event`. `resolve_batch` takes the library and builds its
      events with `verdict_event`. `resolve_file` and `event_for` keep
      their signatures. Update the docstrings of `standing`,
      `resolve_file` (the pass list) and `FileRecord`.

## 4. `resolve` reads the run's library

- [x] 4.1 Red: `crates/borax/tests/dispatch.rs`, through `events_for`
      and `dispatch` with `Adapters::collection_root` set to a fixture
      library:
      - The review case: an item whose title starts `REVIEW
        CORRECTION:`, and a content index holding the old record under
        the file's hash. `resolve --json` gives the corrected title,
        `tier` `library`, `cached` false, `claims` `[]`, a `library`
        object of kind `tracked`, and schema 3. The index entry is
        byte-identical afterwards.
      - The same with `--no-cache`. No source is asked and nothing is
        written to the index.
      - An untracked PDF with an index entry: `cached: true`, `library`
        of kind `untracked`.
      - `collection_root: None`: `library: null`, and otherwise the
        event of today.
      - A file under a nested `.borax.toml`: `library: null`.
      - One unparsable file under `.borax/artifacts/`: stderr carries
        D5's one-record warning exactly once, the other tracked files
        still resolve from their items, an unrecorded PDF reports
        `unreadable-records`, and the outcome is unchanged. An
        unlistable `.borax/artifacts/` gives D5's unlistable warning
        once. A library with no faults writes no such warning.
      - Relative inputs, in `crates/borax/tests/binary.rs` with
        `current_dir` set to the library root and `XDG_CACHE_HOME` set
        to a `tempdir`: `borax resolve --json paper.pdf` and
        `borax resolve --json ./sub/../paper.pdf` on a tracked file both
        report `tier` `library` and the item's title, with a stale
        content-index entry present.
      - Human mode: the tracked file's line ends ` (from the library)`,
        and the closing line is `1 resolved, 0 skipped`.
- [x] 4.2 Green: `resolve_events` reads `Stores` once from
      `adapters.collection_root` and passes it to `resolve_batch`. Write
      the D5 warning that `record_faults` calls for. A read never
      refuses the run.

## 5. `rename` reads it under every setting

- [x] 5.1 Red: `crates/borax/tests/dispatch.rs` or `end_to_end.rs`,
      batch runs over a fixture library:
      - `rename --apply` over a tracked file whose item's record renders
        a different name: the file is renamed to the item's name, no
        source is asked, and the artifact record keeps its identity and
        item link.
      - The same with `--no-record`: the same target and the same
        `resolved` event. No duplicate check is made, and no artifact
        record or item is written.
      - A preview reports the same `resolved` event and plan.
      - A tracked file already carrying its item's name: `already-named`,
        with nothing written to the content index.
      - A file at its recorded path whose bytes the record does not
        hold (`unrecognised-content`) resolves by fallback, and its
        `resolved` event carries the problem. Today's record-update
        behaviour is unchanged, and this change does not alter it
        (design, Risks).
- [x] 5.2 Red: interactive runs through a scripted asker:
      - `crates/borax/tests/describe.rs`: for a `resolved` event with
        `tier: "library"`, the `identifier` line has no origin clause,
        `record` reads `Crossref, from the library` (or `the library`
        for `source: "library"`), and `file says` reads `nothing read`.
        A problem kind adds a `library` line with D5's `<what>` after
        `record`. For a `skipped` event it comes after the reason's
        lines.
      - A tracked file is asked the move question with today's choices.
        Answering rename moves it and writes no content-index entry.
      - With skip-named on, a tracked already-named file is shown
        nothing, and its events are in the run log.
      - Supplying a different identifier for a tracked file: the
        `resolved` event has `tier` `supplied` and `library` of kind
        `tracked`, and the `library-admission` `relinked` event follows
        as today.
- [x] 5.3 Red: the consultation survives a rename (design D8, D11).
      Use a file whose record links a missing item (`dangling-item`):
      - Batch `rename` over it when it carries no identifier: the
        `skipped` event (`no-identifier`) carries `dangling-item`. The
        same holds for a conflict skip and an `unresolvable` skip.
      - Batch `rename` when the fallback resolves it and the target is
        taken: the `resolved` event carries `dangling-item`, and the
        `target-taken` skip carries `library: null`.
      - Interactive, no identifier, operator answers Skip: the `skipped`
        event carries `dangling-item`, and so does the description shown
        before the question (`library` line after the reason).
      - Interactive, services unreachable, operator answers Retry and
        the services then answer: the `resolved` event carries
        `dangling-item`. If the retry finds a conflict and the operator
        skips, the conflict skip carries it.
      - Interactive, operator supplies an identifier for the file: the
        `resolved` event carries `dangling-item` with `tier` `supplied`.
      - The snapshot: one applying batch run over two files whose
        records link the same missing item. The first resolves by
        fallback and is admitted. The second's event still carries
        `dangling-item`, not `tracked` with the first file's record,
        under both `--record` and `--no-record`.
- [x] 5.4 Red, audit: find existing tests that run in a library
      (`collection_root` set, or a `.borax.toml` fixture) and assert a
      content-index answer (`cached: true`, `(cached)`, `from an earlier
      run`) for a file an earlier run or fixture recorded with a linked
      item. Where the library now answers, update the assertion to the
      library answer and state each update in the commit. Where the
      test's point was the content index, give the file no artifact
      record instead, and say so.
- [x] 5.5 Green: `preflight` reads the run's `Stores` whenever
      `adapters.collection_root` is set, and keeps that read as an
      immutable snapshot for consultation. The account, and every
      `learn`/`foresee` into its own copy, stays behind the `record`
      setting exactly as today. `rename_events` passes the snapshot to
      `standing`. Carry the answer through `alone`, `asked`, `skipped`,
      Retry and `Settled::Skip` to the events design D11 lists. `resolve_supplied`'s caller carries the standing's
      `library` into the supplied `FileRecord`. In `describe.rs`,
      `whence("library")` is `None`, `record_from` handles a library
      answer, and the `library` line is added. The artifact-store
      warning is written as in 4.2.

## 6. `bib` reads it too

- [x] 6.1 Red: `crates/borax/tests/bib.rs` or `dispatch.rs`: `bib` over
      a tracked file whose item's title is corrected. The `resolved`
      event is the library answer, and the entry written to the master
      `.bib` and to the sidecar carries the corrected title.
- [x] 6.2 Green: `preflight` gives `bib` the run's `Stores`, and
      `resolved_record` resolves through `standing` with no account and
      that library, emitting `verdict_event`.

## 7. Documents

- [x] 7.1 Doc writer (`codex-docs`), update jobs on `docs/manual.org`.
      Sources: this proposal, its design and its spec deltas. The
      implementer does not write these. The orchestrator checks the diff
      against the sources:
      - "What a run does" (lines 19–76): before identifying a file by
        its contents, a file its library tracks takes its record from
        its item. Say what "tracked" means, and that a library that
        cannot answer is reported.
      - `borax resolve` (87–106) and `borax bib` (357–373): both answer
        a tracked file from its library item. `--no-cache` does not
        bypass the library.
      - The interactive description (around 190–222): `from the
        library` on the `record` line, and the `library` line for a
        problem.
      - "Remembering your answer" (around 302–316): inside a library,
        the item the rename linked to answers later runs.
      - Settings: `record` (603–607) does not turn off consulting the
        library. `network.cache` (610–611) does not bypass it.
      - Run logs, the JSONL paragraph (around 1288–1306):
        `resolved.library` and `skipped.library`, the `tier` value
        `library`, and the `source` value `library`.
- [x] 7.2 Doc writer, `CHANGELOG.md`, an Unreleased "Changed" entry:
      inside a library, `resolve`, `rename` and `bib` take a tracked
      file's record from its library item before the content index, and
      `--no-cache` does not bypass it. A library that cannot answer is
      reported, and the file is resolved as before. Also an "Added"
      entry for the `library` field and the `library` values of `tier`
      and `source`, with the schema still 3.
- [x] 7.3 Record the built state in `openspec/STATE.md`, as
      `fit-summary-to-command` did.

## 8. Close

- [ ] 8.1 `openspec validate consult-library-first --strict` and
      `python3 scripts/check-spec-deltas.py` pass.
- [ ] 8.2 `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` pass, as CI runs them.
