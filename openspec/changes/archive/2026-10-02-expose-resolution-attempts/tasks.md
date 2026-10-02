# Tasks: expose-resolution-attempts

Work on branch `change/expose-resolution-attempts`, cut from `main` at
0.8.0. This is change 8 of the roadmap's Phase 3, and nothing below
waits on another change.

Groups 1 to 8 are each a red/green pair. The red task is written and
run first. It touches test files only. The green task then makes it
pass without editing those tests. A new test may open red on a name
from design D12 that does not exist yet. Write it so that it fails on
its assertion once the name exists.

The interface is fixed in design D12. The rules for the content index
are in D6, the reasons in D9, and the operator paths in D11.

**This change does not alter emitted output.** JSON lines, human lines,
run logs, `run-finished` counts and exit status stay byte for byte as
they are, and `SCHEMA` stays 3. The following are out of bounds:

- `crates/borax/src/event.rs`: every event, `SkipReason`, `Attempt`,
  `human_line`, `Counts` and `SCHEMA`;
- `pipeline::skipped_for`, `unresolvable` and `attempts_of`;
- `Command::summary` and `session::outcome_for`.

If a test seems to need any of these changed, the task has been
misread.

**Compile-only edits to existing tests are allowed**, and they are the
only edits allowed to an existing test. A red task makes them in the
same batch as its new tests. The category is broad: any structural or
ownership adaptation an existing test needs to compile against the
design D12 types. It never extends to changing what a test expects.
Examples, not a closed list:

- a test `Source` returning `Ok(Fetched::network(record))` where it
  returned `Ok(record)`;
- a caller reading `.record` off a fetched or resolved value;
- a test `Cache` returning `CacheWrite`;
- `let _ =` in front of a `put` used as a statement;
- `.conflict()` after a `check_title` call;
- a read of a removed `FileRecord` or `Standing` field becoming the
  method of the same name (design D2);
- a `FileRecord` literal gaining `evidence`;
- `claims_of(..)` becoming `titles_of(..).claims()`;
- `resolve_supplied` gaining its new arguments, with `.file` and
  `.conflict` becoming the record and `conflict()`;
- a `dispatch::Resolved` literal gaining `retrieval` and `failures`
  (`crates/borax-sources/tests/dispatch.rs`, near lines 122 and 145),
  set to what that source answers with: `Network { stored: None }` and
  the preceding failures, which are empty for the first and
  `[(Crossref, Unavailable { message: "503" })]` for the second;
- `standing.library` or `result.library` becoming `.library()` (design
  D12 adds `Standing::library()`);
- ownership adaptations where a method now returns a borrow, such as
  `.cloned()` or `.as_ref()` when `file.library()`
  (`Option<&LibraryAnswer>`) is compared with an owned value
  (`crates/borax/tests/pipeline.rs`, near line 2869);
- binding a temporary that a borrow outlives, as in `let titles =
  titles_of(..); let claims = titles.claims();`
  (`crates/borax/tests/pipeline.rs`, near line 2012).

An edit that changes an expectation about emitted output, a verdict
or a `SkipReason` is not a compile-only edit, however small. Task 9.1
audits every edit against this rule.
The known-defect regression guard in `crates/borax/tests/pipeline.rs`
(the three `resolve_file_still_skips_…` tests) stays exactly as it is.

**Fixtures.** The fakes in `crates/borax/tests/pipeline.rs` cover
almost every case:

- `FakePdf` with `with_pages`, `with_title` and `with_xmp`;
- `FakeDocuments` with `with_file`, `with_open_error`, and a hash
  error as `a_hash_failure_proceeds_to_open_and_extract_rather_than_skipping`
  builds it;
- `fake_source` and `PanicSource`;
- `CountingCache`;
- `corpus_fixture` for the real backend.

Two things need adding:

- A **write-failing cache** whose `put` returns `CacheWrite::Failed {
  message }` (the shape of `WriteFailingCache` in `dispatch.rs`).
- **Response-cache cases** wrap a fake source in
  `borax_sources::cache::Cached` over a `MemoryCache`.

The controlled cases from `report-extraction-per-file` are reused:

- the blank page with a title: `FakePdf::new().with_pages(vec![Ok("
  \n".to_string())]).with_title("A Title")`;
- the prose page with a title: the same with a page of prose.

## 1. Store writes report their result

- [x] 1.1 Red, in `crates/borax-sources/tests/cache.rs` and
      `crates/borax-sources/tests/store.rs`:
      - `MemoryCache::put` returns `CacheWrite::Written`, and the
        record reads back.
      - `FileCache::put` returns `Written` on success. It returns
        `Failed` with a non-empty message for the invalid key `"Invalid
        Key"`, after which the cache is unchanged as before.
      - `FileCache::put` returns `Failed` with a non-empty message when
        the cache root is an existing regular file, so no directory can
        be created under it. A later `get` misses.
      - `ContentIndex::put` returns what its cache's `put` returned:
        `Written` over a `MemoryCache`, and `Failed { message }`
        carrying the same message over a write-failing cache.
      - Compile-only edits: the test caches in `cache.rs`
        (`SharedCache`), `crates/borax/tests/pipeline.rs`
        (`CountingCache`) and `crates/borax/tests/dispatch.rs`
        (`WriteFailingCache`, which now returns `Failed`) return
        `CacheWrite`, and existing `put` statements gain `let _ =`.
- [x] 1.2 Green:
      - add `CacheWrite`, `#[must_use]`, to
        `crates/borax-sources/src/cache.rs`;
      - change `Cache::put`'s signature, and implement it for
        `MemoryCache` (a poisoned lock is `Failed`);
      - implement `FileCache::put` in `store.rs`, mapping each failure
        to `Failed` with a message;
      - make `ContentIndex::put` return the result;
      - make `ResponseCache` in `crates/borax/src/run.rs` forward it;
      - bind the two `index.put` results in `pipeline.rs` with `let _ =`
        until group 6 records them;
      - update the docstrings of `Cache`, `Cache::put`, `FileCache` and
        `ContentIndex::put`. A write still never fails a run, and it
        now says what happened.

## 2. How a service answered: `Fetched` and `Retrieval`

- [x] 2.1 Red:
      - `crates/borax-sources/tests/cache.rs`:
        - `Cached` over a `MemoryCache` returns `Retrieval::Network {
          stored: Some(CacheWrite::Written) }` on a miss. On a second
          fetch of the same identifier it returns
          `Retrieval::ServiceCache`, and the wrapped source is not asked
          again.
        - `Cached` over a write-failing cache returns the record with
          `Network { stored: Some(Failed { .. }) }`, and the next fetch
          asks the wrapped source again.
        - A failed fetch is still not cached (the existing tests).
      - `crates/borax-sources/tests/pace.rs`: `Paced` returns the
        wrapped source's `Fetched` unchanged, including a
        `ServiceCache` retrieval.
      - `crates/borax-sources/tests/clients.rs`: each of the three
        clients, answering from its cassette, returns `Network { stored:
        None }`.
      - Compile-only edits to every test `Source`:
        - `crates/borax-sources/tests/{cache,dispatch,pace}.rs`;
        - `crates/borax/tests/{pipeline,dispatch,per_file,runlog,streaming}.rs`;
        - the `.record` reads in `clients.rs`, `live.rs` and `cache.rs`.
- [x] 2.2 Green:
      - add `Fetched`, `Fetched::network` and `Retrieval` to
        `source.rs`, and give `Source::fetch` its new return type;
      - adapt the three clients, `Paced` and `Cached`;
      - `dispatch::resolve` and `pipeline` take `.record` and keep
        their behaviour until group 3;
      - update the docstrings of `Source::fetch` and `Cached::fetch` to
        state the retrieval contract.

## 3. Every attempt through dispatch

- [x] 3.1 Red, in `crates/borax-sources/tests/dispatch.rs`:
      - When the first source answers, `Resolved` carries `source`, the
        source's `retrieval` and empty `failures`.
      - When Crossref is `Unavailable { message }` and OpenAlex answers,
        `failures` is `[(Crossref, Unavailable { message })]` and
        `source` is OpenAlex.
      - When Crossref is `RateLimited` and OpenAlex is `Malformed {
        message }`, the result is `Unresolved` with both, in that
        order, and `is_conclusive()` is false.
      - A configured source that does not support the identifier
        appears in neither `failures` nor `attempts`. When no
        configured source supports it, the result is `Unresolved` with
        empty `attempts`.
      - Two calls give equal results.
      - In `crates/borax-sources/tests/cache.rs` (the `resolve` test
        near line 370), resolving twice through `Cached` gives
        `ServiceCache` the second time.
- [x] 3.2 Green: `Resolved { record, source, retrieval, failures }` in
      `dispatch.rs`, with the docstring saying the ordered attempts are
      `failures` followed by the answering source. `Unresolved` is
      unchanged.

## 4. Extraction evidence and the three title states

- [x] 4.1 Red, in `crates/borax/tests/pipeline.rs`:
      - `from_file` over:
        - the blank page with a title: `extracted` is
          `Err(NoTextLayer)`, and `titles` is `Titles::Read` holding one
          `Claim { from: Info, title: "A Title" }`;
        - the prose page with a title: `Err(NoIdentifierFound)`, and the
          same `Read`;
        - prose with no title: `Err(NoIdentifierFound)` and
          `Read(vec![])`;
        - an open error of `Encrypted`: `Err(Encrypted)`, and `Failed {
          message: "PDF is encrypted" }`;
        - an open error of `Unreadable { message }`: that error, and
          `Failed` with the same message;
        - a title and a first page whose `page_text` is `Err(Unreadable
          { .. })`: `Err(Unreadable { .. })`, and `Read` holding the
          title;
        - an XMP DOI with both an XMP and an Info title: `Ok(..)` with
          `embedded-metadata`, and `Read` with the XMP claim then the
          Info claim.
      - `titles_of` gives `Read`, `Read(vec![])` and `Failed` for the
        titled, untitled and unopenable fakes. `Titles::claims()` is
        empty for `Failed` and `NotAttempted`.
      - With `RealDocuments` over `corpus_fixture`:
        - `no-identifier.pdf` gives `Read` holding its Info title;
        - `encrypted-user-password.pdf` and `malformed-truncated.pdf`
          give `Failed`;
        - `no-text-layer.pdf` gives `Read`.
      - `pipeline::extraction` gives the same result as before for
        every case (the existing tests stay as they are).
- [x] 4.2 Green:
      - create `crates/borax/src/evidence.rs` with `Titles`,
        `Titles::claims`, `Unattempted` and `Unattempted::as_str`;
      - add `FileRead` and the new `from_file` to `pipeline.rs`;
      - add `titles_of` and delete `claims_of`;
      - adapt `extraction`, `standing` and `resolve_supplied` to the new
        types, with no change in behaviour.

## 5. What the title check concluded

- [x] 5.1 Red:
      - `crates/borax-sources/tests/conflict.rs`:
        - `check_title` gives `Agreed` for an agreeing title, and also
          for a placeholder next to an agreeing real title.
        - It gives `Conflict` with the same `Conflict` value as before
          for a disagreeing one.
        - It gives `Insufficient(NoTitles)` for no candidates.
        - It gives `Insufficient(NoEvidence)` when the only candidates
          are a producer placeholder, a filename and a bare DOI.
        - It gives `Insufficient(RecordUntitled)` for a record with no
          title, and for one whose title is only function words, in
          both cases even with candidates.
        - `TitleCheck::conflict()` is `Some` for `Conflict` alone.
      - `crates/borax/tests/pipeline.rs`:
        - `title_check(&Titles::Read(..), record)` maps each
          `TitleCheck` onto `MatchCheck`.
        - `title_check(&Titles::Failed { .. }, record)` is
          `Insufficient(NoTitles)`.
        - `title_check(&Titles::NotAttempted(r), record)` is
          `MatchCheck::NotAttempted(r)`.
- [x] 5.2 Green:
      - add `TitleCheck`, `Insufficient` and the new `check_title`
        return type in `crates/borax-sources/src/conflict.rs`, with the
        three causes in D8's order and today's agreement and conflict
        rules unchanged;
      - add `MatchCheck` to `evidence.rs`;
      - add `title_check` to `pipeline.rs` and delete `disagreement`;
      - the docstring of `check_title` states all three outcomes.

## 6. Evidence on every verdict

- [x] 6.1 Red, in `crates/borax/tests/pipeline.rs`, through `standing`.
      Each case asserts the whole `Evidence` unless a line says
      otherwise.
      - **A fresh resolution**, with no library given:
        - library `NotConsulted(NoLibrary)`;
        - index read `Miss`, write `Attempted(Written)`;
        - extraction `Ran(Found { .. })`, titles `Read`;
        - lookup `Attempted` with the identifier, the
          `Origin::Extracted(tier)` and one `Ok(Network { stored: None
          })` attempt;
        - `retrieval()` is `Network { service: Crossref }`.
      - **Invariant** (design D2): `standing.evidence` equals
        `file.evidence` for a resolved verdict, `refused.evidence` for
        a conflict, and `duplicated.file.evidence` for a work duplicate.
      - **A failure before a success**: Crossref `Unavailable` and
        OpenAlex answering give attempts `[Crossref Err(Unavailable),
        OpenAlex Ok(..)]`. `file.source()` is OpenAlex and the
        `resolved` event's `source` is `"openalex"`, as before.
      - **Ways of failing**:
        - `RateLimited` then `Malformed` gives an `Unresolvable` verdict
          identical to before, with the structured attempts in the
          evidence and `lookup.is_conclusive()` false;
        - all `NotFound` gives `is_conclusive()` true;
        - title check and write are `NotAttempted(NoRecord)`.
      - **No eligible service**: a text-layer arXiv identifier, with
        `FakeSource`s named Crossref and OpenAlex built with `supports:
        false`, which stands for "supports only DOIs" here, gives
        lookup `Attempted` with no attempts and `no_eligible_service()`
        true. The verdict is the same `Unresolvable` as before.
      - **Response cache**: two files carry the same DOI and the
        sources are wrapped in `Cached` over one `MemoryCache`.
        - The first gives `Network { stored: Some(Written) }` and
          `retrieval()` `Network { service: Crossref }`.
        - The second gives `ServiceCache` and `retrieval()`
          `ServiceCache { service: Crossref }`.
        - `cached()` is false for both, as before.
      - **Content index read**:
        - `Hit`: `retrieval()` is `ContentIndex`, and the write, titles,
          extraction, lookup and title check are each
          `NotAttempted(ContentIndexHit)`;
        - `Bypassed` with `cache: false`, with the write
          `Attempted(Written)`;
        - `Unavailable` with the hash error's message when hashing
          fails, under both `cache: true` and `cache: false`;
        - an unhashable file that resolves has write
          `NotAttempted(Unhashable)`.
      - **Failed write**: an index over a write-failing cache gives the
        write `Attempted(Failed { .. })`. The verdict is `Resolved` with
        the same record, `hash` and schema-3 projections as over a
        working index.
      - **Library**:
        - a tracked file gives `Consulted(Tracked { .. })`, every later
          section `NotAttempted(LibraryAnswered)`, and `retrieval()`
          `Library { artifact, item }` naming them;
        - a dangling item gives `Consulted(DanglingItem { .. })`, with
          the rest as for an untracked file;
        - `Stores` for another root gives
          `NotConsulted(OutsideLibrary)`.
      - **Extraction failed**:
        - the blank page with a title gives extraction
          `Ran(NoTextLayer)` and titles `Read` holding `"A Title"`;
        - the prose page with a title gives
          `Ran(TextWithoutIdentifier)` and the same titles;
        - lookup, title check and write are
          `NotAttempted(ExtractionFailed)` for both;
        - the verdicts stay `SkipReason::NoIdentifier`.
      - **Encrypted and unreadable**: these give `Ran(Encrypted)` and
        `Ran(Unreadable { message })`, with titles `Failed` for both.
        Both verdicts stay `SkipReason::Unreadable`, as before.
      - **Content duplicate**: a `standing` whose account holds the
        file's bytes gives evidence equal to
        `Evidence::not_attempted(ContentDuplicate)`.
      - **Reason names**: `Unattempted::as_str` gives `no-library`,
        `outside-library`, `content-duplicate`, `library-answered`,
        `content-index-hit`, `extraction-failed`, `no-record`,
        `refused`, `unhashable` and `awaiting-acceptance`.
      - **Schema-3 projections**: `resolved_event` of the
        failure-before-success and response-cache records carries the
        `source`, `tier`, `found`, `claims`, `cached` and `library`
        values the same inputs gave before this change.
      - Compile-only edits as the preamble lists, in `pipeline.rs`,
        `event.rs`, `bib.rs` and `renaming.rs`.
- [x] 6.2 Green:
      - complete `evidence.rs` as design D12 gives it, including
        `Evidence::retrieval` with design D2's precedence (a found
        attempt, then a library answer whose lookup was not attempted,
        then a content-index hit). `source()`, `tier()` and `cached()`
        read only `retrieval()`;
      - add `Standing::library()`;
      - change `FileRecord` and `Standing` as D2 gives them, with the
        projection methods;
      - in `standing`, `beyond_library`, `library_record` and
        `indexed_record`, build the evidence in pipeline order, and
        record the batch write;
      - add `index_read` and delete `from_index`;
      - make `resolved_event` and `sources_of` read the projections;
      - adapt `crates/borax/src/run.rs` (`asked`, `situation`,
        `elsewhere`, `reidentified`) to read the evidence. Its output is
        identical.
      - Docstrings state each type's contract and none mentions the
        rejected designs.

## 7. The conflict candidate

These tests pin behaviour that already holds, so they may pass as soon
as group 6 is green. Report that rather than weakening them.

- [x] 7.1 Red, in `crates/borax/tests/pipeline.rs`. The conflict file
      has a text-layer DOI and an Info title that disagrees with
      Crossref's record. Its source is wrapped in `Cached` over a
      `MemoryCache`.
      - The verdict is the same `SkipReason::Conflict` as before.
      - `refused.evidence` holds:
        - lookup `Attempted` with the DOI,
          `Origin::Extracted(TextLayer)` and `[Crossref Ok(Network {
          stored: Some(Written) })]`;
        - extraction `Ran(Found { .. "text-layer" })`;
        - titles `Read` holding the Info title;
        - title check `Conflict` whose `similarity` equals the skip's;
        - write `NotAttempted(Refused)`.
      - `retrieval()` is `Network { service: Crossref }`.
      - After the run, `index.get(&hash)` is `None`. A second `standing`
        over the same file is a conflict again, and its attempt is now
        `ServiceCache`, because the response cache holds the answer
        under the identifier.
      - The index already holds record A for the file's hash and
        `cache: false` resolves the file to a refused record B. After
        the run, `index.get(&hash)` is still A.
      - Crossref `Unavailable` with OpenAlex answering a conflicting
        record gives a `refused.evidence` that keeps both attempts in
        order.
- [x] 7.2 Green: whatever group 6 left undone for a refused record. No
      `index.put` may be added on the conflict path.

## 8. Operator lookups and the rename-time write

- [x] 8.1 Red, in `crates/borax/tests/pipeline.rs`:
      - `resolve_supplied` with `Origin::Operator` and the `standing`
        evidence of a prose file with no identifier gives `Ok(file)`
        where:
        - lookup is `Attempted { identifier, Operator, [..] }`;
        - titles are re-read;
        - the title check is judged on them;
        - write is `NotAttempted(AwaitingAcceptance)`;
        - library, index read and extraction result are `prior`'s;
        - `file.tier()` is `Some(Provenance::Supplied)`.
      - The same call with a conflicting record gives a
        `file.conflict()` equal to the `SkipReason::Conflict` that
        `Supplied::conflict` held before.
      - With `Origin::Extracted(TextLayer)`, `file.tier()` is
        `Some(Provenance::Extracted(TextLayer))`.
      - **Supply after an index hit.** The call is given the evidence
        of a file the content index answered for, so its read is `Hit`
        and its titles are `NotAttempted(ContentIndexHit)`. The
        result's titles become `Read`, its `content_index.read` stays
        `Hit`, and `retrieval()` is `Network { .. }`. `cached()` is
        false and `tier()` is `Some(Provenance::Supplied)`, which is
        what today's `resolve_supplied` reports (design D2).
      - **Supply on a tracked file.** The call is given the evidence of
        a tracked file's library answer. The result's `tier()` is
        `Some(Provenance::Supplied)`, `library()` is still `Some(Tracked
        { .. })`, `cached()` is false, and `retrieval()` is the service.
      - A failure then a success keeps both attempts.
      - `Err(Unresolved)` is unchanged.
      - `unheld_evidence(prior, identifier, Origin::Extracted(t),
        &unresolved)`:
        - the lookup is `Attempted` with `unresolved`'s attempts in
          order;
        - `match_check` and the write are `NotAttempted(NoRecord)`;
        - every other section equals `prior`'s;
        - `lookup.is_conclusive()` is true for an all-`NotFound`
          `Unresolved` and false when any attempt is `Unavailable`.
      - `accept`:
        - Over a conflict's `Standing::refused`, the record is
          unchanged. `overrode` is `Some(Overridden { .. })` with the
          same field, titles and similarity as the
          `SkipReason::Conflict` verdict, `match_check` is still
          `Conflict`, and the write is now
          `NotAttempted(AwaitingAcceptance)`. Every other section is
          unchanged.
        - Over a resolved record whose write is `Attempted(Written)`,
          `overrode` is `None` and the evidence is unchanged.
        - Over a supplied record already `AwaitingAcceptance`, the
          write stays as it is.
      - `remember` gives:
        - `Attempted(Written)` with a working index, after which the
          record is stored;
        - `Attempted(Failed { .. })` over a write-failing cache;
        - `NotAttempted(Unhashable)` with no hash, with nothing
          written.
- [x] 8.2 Red, in `crates/borax/tests/dispatch.rs`:
      - If no existing test asserts the `tier` of a record reached by
        retrying after an outage, add one asserting that it is the
        extraction pass, not `supplied`.
      - **Outage, retry, then not found.** No existing test covers this
        (the retry tests near lines 4683, 15214 and 15280 cover other
        paths). Add one: the file's DOI meets an outage, the operator
        answers Retry, and every service now answers `NotFound`. The
        next question's choices do not contain `Answer::Retry`. The
        file's `skipped` event is `unresolvable`, with the retry's
        attempts. This needs a test `Source` that answers from a
        sequence, `Unavailable` and then `NotFound`, alongside
        `FlakySource`, whose second answer is always a record.
      - **Outage, failed supply, retry still offered.** The file's DOI
        meets an outage, and the operator supplies a DOI that every
        service answers `NotFound`. The next question still offers
        `Answer::Retry` first, because the failed supply did not
        replace the file's own lookup (design D11).
      - **Overriding a conflict keeps its event order.** In the run of
        `overriding_a_conflict_reports_what_was_overridden_and_renames`
        (near line 5367), the file's `resolved` event comes before its
        `renamed` event, with nothing about that file between them, as
        before. The existing test asserts only counts, so add the order
        assertion as a new test. Do not edit the existing one.
      - `a_content_index_write_that_fails_leaves_the_rename_standing_and_asks_again_next_time`
        stays green with its assertions untouched.
- [x] 8.3 Green:
      - give `resolve_supplied` its D11 signature, delete `Supplied`,
        and add `FileRecord::conflict`;
      - make `remember` return `IndexWrite`;
      - add `pipeline::unheld_evidence` and `pipeline::accept`
        (design D10, D11);
      - in `run::asked`:
        - keep the file's own evidence apart from a candidate's, as
          D11's table gives it, in place of the `unresolved` local;
        - Supply passes `Origin::Operator` and the file's own
          evidence. A failed Supply leaves the file's own evidence
          untouched;
        - Retry passes `Origin::Extracted(tier)`. A failed Retry
          replaces the file's own evidence with `unheld_evidence`, and
          `situation` reads conclusiveness from it;
        - the `tier` and `library` overrides go;
        - `run::accepted` delegates to `pipeline::accept`, so the
          original conflict's write becomes `AwaitingAcceptance`. Its
          `overrode` comes from the match check, not from
          `Offer::conflict`.
      - In the rename loop, bind `remember`'s result to a named value
        beside the move's outcome, with a comment saying change 9
        renders it (design D5). It emits nothing.

## 9. Output is unchanged

- [x] 9.1 Run `cargo test --workspace`. Diff `crates/*/tests` against
      `main` and confirm that every changed line in an existing test is
      one of the compile-only kinds the preamble lists. List any that
      are not, and send them to the orchestrator rather than keeping
      them.

## 10. Documents and state

- [x] 10.1 Doc writer (`codex-docs`), an update job on `CHANGELOG.md`
      under Unreleased. Its sources are this proposal, its design and
      its spec deltas. The implementer does not write it, and the
      orchestrator checks the diff against those sources. It is one
      "Changed" entry saying that:
      - resolution now keeps per-file evidence internally: ordered
        service attempts with structured outcomes, whether the response
        cache or the network answered, cache write results, the content
        index's status, distinct extraction results, titles kept
        through extraction failure, the title check's conclusion, and
        why a step was not taken;
      - no command's output, event, exit status or event schema version
        (still 3) changes;
      - the `borax-sources` and `borax` library interfaces changed as
        design D12 lists, with `claims_of`, `disagreement`, `from_index`
        and `Supplied` removed.
- [x] 10.2 Record the built state in `openspec/STATE.md`:
      - a paragraph under "What is built", after
        `list-untracked-missing-unlinked`, naming the evidence model
        (design D2), the reporting point for the rename-time write
        (D5), and that change 9 renders it under schema 4;
      - amend the known defect "Resolution skip reasons merge distinct
        extraction failures": the engine now holds the four modes
        apart in `Evidence::extraction`, the stream still merges them,
        the regression guard still pins that, and change 9 restores
        them.
- [x] 10.3 `docs/manual.org` is not changed. The orchestrator confirms,
      with the suite green, that these passages are still true:
      - the account of remembering an accepted answer, including its
        best-effort paragraph (lines 370–391);
      - `network.cache` (lines 769–772);
      - the JSONL paragraphs on `found`, `claims`, `source`, `cached`,
        `tier`, `library` and `unresolvable` (lines 1449–1524).

      A passage this change made false is a defect in the change and
      not a documentation task.

## 11. Close

- [x] 11.1 `openspec validate expose-resolution-attempts --strict` and
      `python3 scripts/check-spec-deltas.py` pass.
- [x] 11.2 `cargo fmt --all --check`,
      `cargo clippy --workspace --all-targets -- -D warnings` and
      `cargo test --workspace` pass, as CI runs them.
