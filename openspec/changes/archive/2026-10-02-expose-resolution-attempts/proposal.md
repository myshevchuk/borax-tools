## Why

The resolution engine works out the facts the next event schema needs.
It then throws most of them away before anything can report them.
Phase 3 of the maintainer's roadmap replaces the `resolved` and
`skipped` evidence with the sections settled in the Phase 3
resolution-schema design discussion (maintainer's notes, outside the
repository). That replacement is change 9. It can render only what the
engine keeps. Today the engine loses the following, all read at
`c0cfe26` (0.8.0):

- **Failed attempts before a success.** `dispatch::resolve`
  (`crates/borax-sources/src/dispatch.rs`, line 76) collects each
  failure in `attempts` and returns `Resolved { record, source }` on the
  first success. A Crossref outage followed by an OpenAlex answer
  therefore leaves no trace of the outage.
- **Structured outcomes.** A failed lookup's attempts reach the engine
  as `(SourceName, SourceError)` pairs. `pipeline::attempts_of` then
  turns each one into `Attempt { source, error: String }`, which is the
  only form any caller sees.
- **Response-cache provenance.** `Cached::fetch`
  (`crates/borax-sources/src/cache.rs`, line 146) returns a cache hit as
  a plain `Record`. `FileRecord::cached` says so in its own
  documentation: "a response cache hit behind a `Source` is invisible
  from here".
- **Write results.** `Cache::put` (`cache.rs`, line 72),
  `ContentIndex::put` (`crates/borax-sources/src/store.rs`, line 258)
  and `pipeline::remember` all return `()`. `FileCache::put` discards
  the write's error with `let _ =` (`store.rs`, line 186).
- **Why the content index gave no answer.** `pipeline::from_index`
  returns `None` for a miss, for a file that could not be hashed, and
  for `--no-cache` alike.
- **Distinct extraction failures.** `pipeline::skipped_for` merges
  `NoTextLayer` with `NoIdentifierFound`, and `Encrypted` with
  `Unreadable`. `openspec/STATE.md` records this as the known defect
  "Resolution skip reasons merge distinct extraction failures".
  `standing` keeps no other copy of the extractor's answer.
- **Titles when extraction fails.** `pipeline::from_file` (line 245)
  reads the file's titles and then returns `Err` without them when no
  identifier is found. `claims_of` (line 686) reports a file it could
  not open as claiming no title. So an empty claim list can mean
  "never opened", "opened and claims none", or "could not be opened".
- **What the title check concluded.** `conflict::check_title` returns
  `None` both when a title agreed and when nothing counted as evidence.
  A producer's placeholder is therefore indistinguishable from
  agreement.
- **The full candidate behind a conflict.** `Standing::refused` keeps
  the refused record with its identifier and claims. It does not keep
  the failures before the service that answered, where the record was
  retrieved from, or what extraction found.
- **Why a step was skipped.** A `None` library answer covers no
  library, a file outside the library, and a resolution that ended
  early on a content duplicate.

The design discussion settled that change 8 exposes this evidence
inside the engine, with no change to emitted output, so that change 9
can render it without reconstructing or guessing. It also settled that
change 9 makes the single schema bump from 3 to 4. No living
requirement asks a resolution to keep any of this, so the change needs
a proposal and is not a restoration.

## What Changes

- **Each lookup keeps every attempt, in order.** `dispatch::resolve`
  returns the failures that came before the answering service as well
  as the answer. The engine keeps each attempt as a service name with
  a structured outcome: found, not found, unavailable (with its
  message), rate limited, or malformed (with its message). It also
  keeps the identifier that was looked up and whether extraction found
  it or an operator supplied it. If no configured service supports the
  identifier, the lookup is kept with no attempts (design D3).
- **A found record says how the service answered.** `Source::fetch`
  returns the record together with its retrieval: served by the
  response cache, or sent over the network. A network retrieval also
  carries the result of the response-cache write, or no result when no
  cache wraps the source. `Cached` sets both. Every record the engine
  holds can say whether it came from a library item, the content
  index, a service's response cache, or the network (design D4).
- **Store writes report their result.** `Cache::put` and
  `ContentIndex::put` return whether the write was made or why it
  failed, and `pipeline::remember` returns the same. A failed write is
  evidence. It never fails the resolution, the verdict, or the rename
  it followed (design D5).
- **The content index says what it did.** For each file the engine
  records one of: hit; miss; bypassed; unavailable because the file
  could not be hashed (with the hashing error); or not asked, with the
  reason. It also records the content-index write: made, failed, or not
  attempted with its reason (design D6).
- **Extraction evidence survives failure.** `from_file` returns the
  extractor's result and the file's titles independently. The engine
  keeps the result in the five-kind vocabulary that change 4 defined.
  It keeps the titles in one of three states: read (possibly empty),
  failed because the file could not be opened, or not attempted with
  the reason. `titles_of` replaces `claims_of` (design D7).
- **The title check says what it concluded.** `check_title` returns
  one of agreed, conflict, or insufficient evidence. Insufficient
  evidence carries its reason: the file claims no title, no claimed
  title counts as evidence, or the record has no title (design D8).
- **A refused candidate keeps all of its evidence and is never
  remembered.** The record the title check refuses is still kept,
  carrying the same evidence as any other record. The content-index
  write for it is recorded as not attempted because the record was
  refused. The existing guarantee that a refused record is never
  stored under the file's hash is kept and now tested (design D10).
- **Every step that did not run records why.** The library, the
  content index, the titles, extraction, the lookup, the title check
  and the content-index write each record the reason they did not run,
  drawn from one vocabulary of reasons (design D9).
- **One evidence value replaces the flat fields.** `FileRecord` and
  `Standing` each carry an `Evidence` value. It replaces `FileRecord`'s
  `source`, `tier`, `found`, `claims`, `cached` and `library`, and
  `Standing`'s `found`, `unresolved` and `library`. Methods derive the
  schema-3 values from the evidence, so the existing stream is produced
  from the new evidence, and no fact is stored in two shapes.

  `source()`, `tier()` and `cached()` all read one derivation of where
  the current record came from. An operator's lookup over a tracked or
  indexed file therefore still reports `tier: supplied` and `cached:
  false`, as today (design D2).
- **Operator lookups carry evidence too.** `resolve_supplied` takes the
  identifier's origin and the file's existing evidence. It returns a
  `FileRecord` whose evidence covers the new lookup. A retry therefore
  no longer has to overwrite `tier` afterwards.

  The driver keeps the file's own evidence apart from a candidate's.
  A failed retry replaces the file's own lookup, so a retry that ends
  conclusively is not offered again. A failed supply leaves the file's
  own lookup alone.

  Accepting the file's own conflict goes through `pipeline::accept`,
  which records the content-index write as waiting for the move rather
  than as refused (design D10, D11).
- **Emitted output does not change.** JSON events, human lines, run
  logs, `run-finished` counts and exit status stay as they are, and
  `SCHEMA` stays 3 (design D1).

Explicitly out of scope:

- **Every change to emitted output.** That includes the schema-4 bump,
  the new event sections, removing `found`, `cached`, `source`, `tier`,
  `claims` and `overrode`, and restoring the distinct extraction
  failures in `skipped`. All of it belongs to change 9. This change
  specifies no event field.
- **Operator-input history and pending acceptance.** These are change
  10. A retry's evidence covers its own lookup, and the failed lookup
  it replaced is not kept (design D11).
- **Title search and candidate suggestion.** Keeping a refused
  candidate's evidence does not schedule an interface that offers
  candidates or searches by title. `openspec/STATE.md` lists title
  search as not built, and it stays a separate change.
- **`run-finished` counters.** No count is added or changed.
- **Cache semantics.** What is cached, when, under which key, and what
  `--no-cache` bypasses are all unchanged. A cache read failure is
  still a miss.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`: seven ADDED requirements.
  - "A resolution keeps every service it asked, in order"
  - "A resolution says where its record came from"
  - "A resolution says what the content index did"
  - "A failed cache write is evidence, not a failure"
  - "A refused record is kept as evidence and never remembered"
  - "A resolution says what the title check concluded"
  - "A resolution says why a step was not taken"

  Each one states what a resolution retains and distinguishes. None
  names an event field, so each stays true when change 9 renders the
  facts.
- `extraction`: ADDED "A resolution keeps extraction's result and the
  file's titles". It covers the five result kinds held without merging
  whatever the skip reason says, the three title states, and titles
  kept through a failed identifier search.

Requirements checked and left unchanged:

- `resolution` "A resolution reports the evidence it was checked
  against" and "A resolution says whether its library answered".
  Change 9 modifies both. Every schema-3 field they require is still
  emitted, now derived from the evidence (design D2).
- `resolution` "Sources are queried by identifier type and priority".
  The order and the fall-through are unchanged. Only what is kept
  changes.
- `resolution` "Responses are cached locally". The same reads and
  writes happen under the same keys, the `--no-cache` bypass is
  unchanged, and the content index is still written even when the
  cache is bypassed.
- `resolution` "An operator's answer about a file is remembered". Its
  rule that a write that could not be made is never reported as a
  failure of the rename is restated more generally by the new
  requirement "A failed cache write is evidence, not a failure", and
  the two agree.
- `resolution` "Ambiguity is skipped, never guessed". A conflict is
  still a skip, and only an operator can accept one.
- `extraction` "Extraction failures are typed and non-fatal". This
  change does not modify it. The engine now keeps the four failure
  modes apart (design D7). The skip reasons in the stream still merge
  them, and change 9 owns that. Task 10.2 updates the known-defect
  entry in `openspec/STATE.md` to match.
- `extraction` "Extraction reports one result for each file it is
  given". `pipeline::extraction` returns the same results as before.
- `cli` "JSON Lines output is first-class" and "Exit codes distinguish
  partial success". Neither the output nor the exit status changes.

## Impact

- `crates/borax-sources/src/cache.rs`:
  - `CacheWrite`, the result of a write;
  - `Cache::put` returns it;
  - `MemoryCache` reports a poisoned lock as a failed write;
  - `Cached::fetch` sets the retrieval and records the write.
- `crates/borax-sources/src/store.rs`: `FileCache::put` and
  `ContentIndex::put` return `CacheWrite`. An invalid key, a
  serialisation error and an I/O error each become `Failed` with a
  message.
- `crates/borax-sources/src/source.rs`: `Fetched` and `Retrieval`, and
  `Source::fetch` returns `Result<Fetched, SourceError>`. The three
  clients and `Paced` (`pace.rs`) are adapted.
- `crates/borax-sources/src/dispatch.rs`: `Resolved` gains `retrieval`
  and `failures`. `Unresolved` is unchanged.
- `crates/borax-sources/src/conflict.rs`: `TitleCheck` and
  `Insufficient`, and `check_title` returns `TitleCheck`.
- `crates/borax/src/evidence.rs` (new): the evidence types of design
  D12.
- `crates/borax/src/pipeline.rs`:
  - `FileRecord` and `Standing` carry `Evidence`, with methods for the
    schema-3 values;
  - `from_file` returns `FileRead`;
  - `titles_of` replaces `claims_of`;
  - `title_check` replaces `disagreement`;
  - `resolve_supplied` takes `origin` and `prior` and returns
    `Result<FileRecord, Unresolved>`, so `Supplied` is removed;
  - `remember` returns `IndexWrite`;
  - `unheld_evidence` and `accept` are new;
  - `Standing::library()` is new;
  - `skipped_for` is not touched.
- `crates/borax/src/run.rs`: adapted to the new types, with identical
  output. The run's `ResponseCache` forwards `CacheWrite`.
- `crates/borax/src/event.rs`: unchanged. `SCHEMA` stays 3 and no
  event, field or human line changes.
- Tests: every test `Source` and `Cache` implementation is updated to
  the new signatures. That is twelve `fetch` implementations and three
  `put` implementations across the two crates' test directories, plus
  the callers in `crates/borax-sources/tests/clients.rs`, `live.rs` and
  `cache.rs` that read a fetched or resolved record. Existing reads of
  `FileRecord`'s removed fields become method calls. No assertion about
  emitted output changes (tasks preamble).
- Documents a person reads:
  - `CHANGELOG.md`, through the doc writer (task 10.1);
  - `docs/manual.org` is not affected. It describes emitted fields and
    observable behaviour, and neither changes here. Its account of
    best-effort memory, of `network.cache`, and of the JSONL fields
    `found`, `claims`, `source`, `cached`, `tier` and `library` all
    stay true (task 10.3).
- `openspec/STATE.md` records the built state (task 10.2).
- No new dependency, configuration key, flag or event.

## Deferred

- **Rendering.** Change 9 turns this evidence into the schema-4
  sections. Design D1 sketches how it reads each one.
- **Where the rename-time content-index write is reported.** This
  change makes the write's result available at the move and records on
  the verdict that the write is still to come. Change 9 chooses which
  event carries the result (design D5).
- **The library's not-consulted reasons in finer detail.** "Outside
  the library" covers both a file beyond the library root and a file
  in an excluded subtree, because `Stores::consult` answers `None` for
  both (design D9).
