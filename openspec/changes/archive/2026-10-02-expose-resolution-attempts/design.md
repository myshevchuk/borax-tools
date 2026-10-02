# Design: expose-resolution-attempts

## Context

Everything below was read from source at `c0cfe26` (0.8.0). The
decisions this change carries out were settled in the Phase 3
resolution-schema design discussion (maintainer's notes, outside the
repository). Those decisions are:

- the version moves 3 to 4 exactly once, in change 9;
- change 8 is engine-only;
- attempts carry structured outcomes, including the found attempt and
  its retrieval;
- the library is reported in two sections, what the library said and
  where the record came from;
- a failed cache write is evidence;
- a rejected candidate is never associated with the file's hash.

Section and value names in that discussion are recommendations for the
stream. This design uses them for the engine's vocabulary where they
fit, and change 9 fixes their spelling in the stream.

The code as it stands:

- `Source::fetch` returns `Result<Record, SourceError>`. A real run
  wraps each client as `Cached<Paced<Client>>` (`run::polite`). When
  `network.cache` is off it uses `Paced<Client>` alone. `Cached::fetch`
  returns a hit before asking the source. On a miss it asks the source
  and calls `cache.put`, and it never caches a failure.
- `dispatch::resolve` asks the sources in `priority` order and skips
  any source that is not configured or does not support the
  identifier. It returns `Resolved { record, source }` on the first
  success, which drops the failures gathered before it. It returns
  `Unresolved { attempts }` when every source fails. An identifier no
  configured source supports produces `Unresolved` with no attempts.
  An arXiv identifier under the sources Crossref and OpenAlex is such a
  case, because OpenAlex supports only DOIs and PMIDs.
- `standing` hashes the file and runs the account's content-duplicate
  check. It then asks the library through `Stores::consult`, which
  answers `None` for a file outside the library or in an excluded
  subtree, and finally calls `beyond_library`. That function runs
  `from_index`, `from_file`, `from_sources`, `disagreement` and
  `index.put`, followed by the work-duplicate check. The content index
  is written even under `--no-cache`, because a forced live answer
  replaces a stale entry.
- `FileRecord` holds the facts the schema-3 `resolved` event renders
  as flat fields: `source`, `tier` (a `Provenance`), `found`, `claims`,
  `cached` and `library`. `Standing` holds `found`, `unresolved`,
  `refused`, `duplicated` and `library`.
- The interactive driver (`run::asked`) reads `Standing`'s fields
  directly. It calls `resolve_supplied` for both a supplied identifier
  and a retry. For a retry it overwrites `tier` back to the extracted
  pass and `library` back to the standing's answer. It calls
  `remember` after the move, and by then the `resolved` and `renamed`
  events and any `library-admission` event have already been written.
- `check_title` returns `Option<Conflict>`. It returns `None` when the
  record has no title, when no candidate counts as evidence, and when
  any candidate agrees.

## D1. Engine-only, and what change 9 reads

**Decision.** This change keeps evidence in the engine and does not
change what any run emits. `event.rs` is not touched. Every schema-3
value a `resolved` or `skipped` event carries is computed from the new
evidence by a method named after the field it replaces (D2). The
existing suite, whose output assertions are not edited, therefore
shows that the evidence holds at least everything schema 3 said.

Change 9 reads the engine as follows. This mapping is not normative.

| Schema-4 section (discussion) | Engine source in this change |
|---|---|
| `library` | `Evidence::library` (`Consultation`) |
| `content_index` | `Evidence::content_index` (`IndexEvidence`: `read`, `write`) |
| `extraction` | `Evidence::extraction` (`ExtractionEvidence`: `result`, `titles`) |
| `lookup` | `Evidence::lookup` (`LookupEvidence`) |
| `record_retrieval` | `Evidence::retrieval()` (`RecordRetrieval`) |
| `match_check` | `Evidence::match_check` (`MatchCheck`) |
| `acceptance` | `FileRecord::overrode`, unchanged here |
| `candidate` on a conflict skip | `Standing::refused`, with its own `Evidence` |
| reason `kind` on a resolution skip | `Evidence::extraction.result` together with the verdict |

None of the new types derives `Serialize`. The stream's spelling is
change 9's decision, and adding serde now would fix one before that
decision is made.

**Rejected: emit the new evidence now as schema-3 additions.** The
discussion settled one bump, with the sections and the removals
arriving together. Additive fields would leave `cached` and a
`retrieval` saying overlapping things in one stream for a release.

## D2. One evidence value, and the flat fields derived from it

**Decision.** A new module, `crates/borax/src/evidence.rs`, holds the
types listed in D12. `FileRecord` becomes `{ record, hash, evidence,
overrode }`. `Standing` becomes `{ verdict, hash, evidence, refused,
duplicated }`. The fields that are removed come back as methods that
return exactly what the field held.

| Removed field | Replacement |
|---|---|
| `FileRecord::source` | `FileRecord::source()`: the service `retrieval()` names, if any |
| `FileRecord::tier` | `FileRecord::tier()`: from `retrieval()`, by the precedence below |
| `FileRecord::found` | `FileRecord::found()`: the lookup's identifier |
| `FileRecord::claims` | `FileRecord::claims()`: the titles if read, else empty |
| `FileRecord::cached` | `FileRecord::cached()`: `retrieval()` is `ContentIndex` |
| `FileRecord::library` | `FileRecord::library()`: the consultation's answer |
| `Standing::found` | `LookupEvidence::extracted()` |
| `Standing::unresolved` | `LookupEvidence::is_conclusive()`, plus the attempts themselves |
| `Standing::library` | `Standing::library()`: `evidence.library.answer()` |

**One source for where the record came from.** `Evidence::retrieval()`
names the step that produced the record the evidence is now about. It
is the only thing `source()`, `tier()` and `cached()` read. It checks
these in order and takes the first that holds:

1. **A found attempt in the lookup.** The answer is `ServiceCache {
   service }` or `Network { service }`, from that attempt's retrieval.
2. **A library answer.** The lookup is `NotAttempted(LibraryAnswered)`
   and the library is `Consulted(Tracked { artifact, item })`, which
   gives `Library { artifact, item }`.
3. **A content-index hit.** `content_index.read` is `Hit`, which gives
   `ContentIndex`.
4. Otherwise there is no record, and the answer is `None`.

The lookup comes first because an operator's lookup can sit on top of
evidence that also records a library answer or an index hit (D11). A
record reached that way came from the service, and the earlier answer
is still what the library or the index said.

`tier()` follows from `retrieval()`:

- a service retrieval gives `Extracted(t)` when the lookup's origin is
  `Origin::Extracted(t)`, and `Supplied` when it is `Origin::Operator`;
- `Library` gives `Provenance::Library`;
- `ContentIndex` gives `None`.

`tier()` does not depend on whether the library answered. An operator
who supplies an identifier for a tracked file therefore gets `tier()`
`Supplied` while `library()` is still `Tracked`. That pair is what the
stream reports today (`crates/borax/tests/dispatch.rs`, the tracked
supply test near line 15122), and it is what `run::reidentified`
reads to decide on a `Relinked` admission.

`cached()` is true only when `retrieval()` is `ContentIndex`. If the
operator supplies an identifier for a file the index answered for, the
record keeps the earlier index read `Hit`, its retrieval is the
service, and `cached()` is false, as `resolve_supplied` reports today.

`Standing::library()` is a method rather than a direct read of
`evidence.library`. It mirrors `FileRecord::library()`, so that the
many existing test reads of `standing.library` become `library()`
calls returning `Option<&LibraryAnswer>`.

`Standing::evidence` is the evidence for the verdict. When the verdict
is `Resolved(file)`, `file.evidence` is equal to it. The same holds for
`refused` on a conflict and for `duplicated.file` on a work duplicate.
A test pins this (task 6.1).

**Why replace rather than add.** If the flat fields stayed beside the
evidence, every fact would be held twice and the two copies could
disagree. The house's pre-1.0 rule is to replace an interface cleanly.
Change 9 removes the stream fields these values feed anyway. Deriving
schema 3 from the evidence now also makes this change prove that the
evidence is complete.

**Rejected: keep the flat fields and add `evidence` beside them.** It
avoids churn in the tests that build a `FileRecord` literally (eleven
of them). The cost is two representations, so a test that checks one
says nothing about the other.

**Rejected: put the evidence only on `Standing`.** A `FileRecord` also
reaches the stream without a `Standing`: from `resolve_supplied`,
inside an `Offer`, and as `Duplicated::file`. Change 9 would then have
to work out its evidence from somewhere else.

## D3. Attempts through dispatch

**Decision.** `dispatch::Resolved` becomes `{ record, source,
retrieval, failures }`. `failures` holds every service asked before
`source`, each with its `SourceError`, in the order asked. `Unresolved`
is unchanged. The ordered attempt list is therefore `failures`
followed by `(source, Ok(retrieval))`, or `Unresolved::attempts`.
`Unresolved::is_conclusive` keeps its meaning.

In the engine, `LookupEvidence::Attempted` holds `ServiceAttempt {
service, outcome: Result<Retrieval, SourceError> }` items. The five
outcomes the discussion names are `Ok(_)` and the four `SourceError`
variants, so no second vocabulary is introduced. The discussion
dropped the roadmap's suggested "other" outcome because `malformed` is
the fourth kind that already exists.

**No eligible service** is a lookup that was attempted with no
attempts. That is unambiguous, because dispatch never asks a source and
then records nothing. `LookupEvidence::no_eligible_service()` names the
case for change 9.

**Rejected: one `Vec<Attempt>` on both `Resolved` and `Unresolved`.**
That would change `Unresolved`'s type, and `run.rs`, `is_conclusive`
and two test files read it. Nothing would be gained, since the success
is always last and always a single item.

## D4. How a service answered: `Fetched` through `Source::fetch`

**Decision.** `Source::fetch` returns `Result<Fetched, SourceError>`,
where `Fetched { record, retrieval }` and `Retrieval` is either
`ServiceCache` or `Network { stored: Option<CacheWrite> }`.

- The clients return `Fetched::network(record)`, which means `Network
  { stored: None }`.
- `Paced` passes the inner answer through.
- On a hit, `Cached` returns `Retrieval::ServiceCache`. On a miss it
  asks the inner source and, on success, writes the record and sets
  `stored` to the write's result. A failure is still never cached.
- `stored: None` means no response cache wraps the source in this run.
  The run's `network.cache` setting decides that.

This is also the only way the run can tell the content index's
`cached` from a service-cache hit. Today a reader of `FileRecord`
cannot make that distinction.

**Rejected: a provided `Source::retrieve` method whose default calls
`fetch`.** It would avoid changing twelve test fakes. It would also
give the trait two entry points to one operation. A wrapper that
forgot to forward `retrieve` would then report a cache hit as network
traffic with no compiler error. The fakes need a one-token edit
(`Ok(Fetched::network(record))`), which is a smaller cost than a
silent misreport.

**Rejected: move the response cache into `dispatch::resolve`.** Asking
the cache in dispatch would make retrieval visible without changing
the trait. However, it would move a run-level setting
(`effective.config().cache`, which `run::execute` reads once and
passes to `polite`) into a per-file decision. A directory whose `.borax.toml` sets `cache = false`
would then bypass the response cache for its files, which is a change
in behaviour. It would also delete `Cached`, which is tested on its
own in `crates/borax-sources/tests/cache.rs`.

## D5. Write results, and where the rename-time write is reported

**Decision.** `#[must_use] enum CacheWrite { Written, Failed { message
} }` lives in `borax_sources::cache`. `Cache::put`, `ContentIndex::put`
and the run's `ResponseCache` return it.

- `FileCache::put` reports an invalid key, a serialisation error and
  an I/O error from `write_atomically` as `Failed` with a message.
- `MemoryCache::put` reports a poisoned lock.
- Reads stay as they are: a read failure is a miss.

`#[must_use]` is deliberate. A discarded write result is the defect
being fixed, so ignoring one should take a visible `let _ =`. Existing
tests that call `put` as a statement gain that prefix. The edit only
makes them compile cleanly under `clippy -D warnings`.

There are three content-index writes, each with its own reporting
point:

1. **The batch write in `beyond_library`.** It is part of the
   verdict's evidence as `IndexWrite::Attempted(CacheWrite)`.
2. **A record an operator reached** (supplied, retried, or accepted
   over a conflict). It is not written when it is reached. Its
   evidence records the write as
   `NotAttempted(Unattempted::AwaitingAcceptance)`, because nothing has
   been written yet. A supplied or retried record gets that value from
   `resolve_supplied` (D11).

   The file's own refused record does not pass through
   `resolve_supplied`. It reaches the operator from
   `Standing::refused` with the write `NotAttempted(Refused)`, and today
   `run::accepted` only sets `overrode` on a clone of it. Accepting it
   goes through `pipeline::accept` instead, which changes `Refused` to
   `AwaitingAcceptance` (D10). The `resolved` event that reports the
   accepted record therefore states that the write still waits for the
   move. Events and their order are unchanged.
3. **The write made at the move.** `remember` returns an `IndexWrite`
   for it: `Attempted(_)`, or `NotAttempted(Unhashable)` when the file
   has no hash. That result belongs to the move and not to the
   verdict. The `resolved` event is already written by the time the
   move happens, and the move's own events are written before the
   index is.

So the reporting point this change settles is the move. Change 8's
driver keeps the value from `remember` beside the move's outcome and
renders nothing from it. Change 9 chooses the event that carries it.
To put it on `renamed`, change 9 has to make the write after the
filesystem move and before that event is emitted. That reorders a
best-effort write and moves no event.

A failed write never changes a verdict, a rename's outcome, a count or
the exit status. The living requirement "An operator's answer about a
file is remembered" already says this for the rename-time write, and
the new requirement says it for both stores.

**Rejected: hold the `resolved` event until after the move so that it
can carry the write.** It would reorder the stream that a run log
pre-flushes, and the write's outcome is a fact about the move anyway.

**Rejected: report a failed write as a skip or a finding.** That would
turn a lost convenience into a partial run, which is the opposite of
what the store's own documentation promises ("a broken cache must
never fail a run").

## D6. What the content index did

**Decision.**

```rust
pub struct IndexEvidence { pub read: IndexRead, pub write: IndexWrite }
pub enum IndexRead {
    Hit, Miss, Bypassed,
    Unavailable { message: String },
    NotAttempted(Unattempted),
}
pub enum IndexWrite { Attempted(CacheWrite), NotAttempted(Unattempted) }
```

`read` is decided in pipeline order:

1. A content duplicate or a library answer gives `NotAttempted` with
   that reason.
2. Otherwise a file that could not be hashed gives `Unavailable`, with
   the hashing error's message.
3. Otherwise `--no-cache` gives `Bypassed`.
4. Otherwise the read gives `Hit` or `Miss`.

**Unavailable before Bypassed.** Both are true of an unhashable file
under `--no-cache`. The missing hash is the fact with consequences:
nothing can be written to the index for the file, and an applying run
cannot record its move. The bypass is a run setting that a reader
already knows from the command line, so the missing hash is reported.

`write` takes the earliest reason in pipeline order that made the
write moot:

1. a content duplicate, or the library or content index answered;
2. extraction found no identifier;
3. no service held it (`NoRecord`);
4. the title check refused the record (`Refused`);
5. the record awaits acceptance;
6. the file has no hash (`Unhashable`).

Otherwise the write was attempted, and its `CacheWrite` is kept. So an
unhashable file whose extraction failed reports `ExtractionFailed`,
since the hash matters only once a record exists.

`from_index` is replaced by a function that returns the record together
with its `IndexRead` (`index_read` in D12). Without that, the three
`None` cases cannot be told apart.

## D7. Extraction evidence and the three title states

**Decision.** `from_file` returns `FileRead { extracted:
Result<Extracted, ExtractionError>, titles: Titles }`. It no longer
returns a `Result` of a pair.

- When `open` fails, `titles` is `Failed { message }`, where `message`
  is the open error's text, and `extracted` is that error.
- When `open` succeeds, `titles` is `Read(claims)` before extraction
  runs. It stays `Read` whatever extraction returns, including a page
  error that makes the result `Unreadable`.

A file that opens and then fails a page read therefore reports titles
read and the result unreadable. Under the old `Result` the titles were
lost.

`Titles` is `Read(Vec<Claim>) | Failed { message } |
NotAttempted(Unattempted)`. `Read(vec![])` means the file was opened
and claims no title, which differs from both failure and not trying.
`titles_of(path, documents) -> Titles` replaces `claims_of`, which
reported an unopenable file as claiming nothing.

`ExtractionEvidence { result: ExtractionStep, titles }`, where
`ExtractionStep` is `Ran(Extraction) | NotAttempted(Unattempted)`.
`Extraction` is `event::Extraction`, the five-kind vocabulary that
change 4 built. `pipeline::extraction_of` fills `result` from the
extractor's answer, as `pipeline::extraction` already does for `status
--identify`. Both commands therefore keep identical results for the
same file.

`skipped_for` is not touched. The verdict still carries `NoIdentifier`
or `Unreadable` as before, and the known-defect regression guard in
`crates/borax/tests/pipeline.rs` stays green. The evidence carries the
distinction the verdict merges, and change 9 restores it in the
stream.

`Failed` carries a message although the discussion named the state
without one. The reason is `resolve_supplied`, which reads titles
through `titles_of` after a content-index hit, when the extraction
result is `NotAttempted`. Without the message, a failure to read the
titles there would have no stated cause. Change 9 decides whether to
render it.

**Rejected: `Option<Vec<Claim>>`.** That has two states where three
are needed.

## D8. What the title check concluded

**Decision.** `check_title` returns `TitleCheck`: `Agreed`,
`Conflict(Conflict)`, or `Insufficient(Insufficient)`. `Insufficient`
is one of three causes, checked in this order:

1. `RecordUntitled`: the record has no title, or its title has no
   content words. This is the check that already returns first.
2. `NoTitles`: no candidates were given.
3. `NoEvidence`: candidates were given and `is_title_evidence`
   dismissed every one.

Agreement and conflict keep today's rules exactly.
`TitleCheck::conflict(self) -> Option<Conflict>` gives back today's
answer. The fifteen `check_title` calls in
`crates/borax-sources/tests/conflict.rs` gain `.conflict()` and keep
their assertions.

In the engine, `MatchCheck` is `Agreed | Conflict(Conflict) |
Insufficient(Insufficient) | NotAttempted(Unattempted)`. The check
runs over `Titles::claims()`. Titles that `Failed` therefore reach the
check as no candidates and give `Insufficient(NoTitles)`.

**Scope call.** The roadmap's list for change 8 does not name the
match check. It is included because the discussion's `match_check`
section separates `agreed` from `insufficient-evidence`. Without this
change, change 9 could produce that distinction only by running
`check_title`'s rules a second time outside the function, which is
reconstruction. Tasks 5.x isolate this group so that it can be
dropped or moved without touching the rest.

**Rejected: classify in `pipeline` with `is_title_evidence`.** It would
copy `check_title`'s order of checks and its record-title test into a
second place, and the two copies could drift apart.

## D9. Why a step was not taken: one reason vocabulary

**Decision.** There is one enum, `Unattempted`, which every section's
`NotAttempted` and `Consultation::NotConsulted` carry. A single
`as_str` gives each reason its kebab-case name for change 9. The
reasons, and the sections that can carry each one:

| Reason | Library | Index read | Index write | Extraction, titles | Lookup | Match check |
|---|---|---|---|---|---|---|
| `NoLibrary`: the run has no library | yes | | | | | |
| `OutsideLibrary`: beyond its root, or in a subtree it excludes | yes | | | | | |
| `ContentDuplicate`: the account already holds these bytes | yes | yes | yes | yes | yes | yes |
| `LibraryAnswered` | | yes | yes | yes | yes | yes |
| `ContentIndexHit` | | | yes | yes | yes | yes |
| `ExtractionFailed`: no identifier was found | | | yes | | yes | yes |
| `NoRecord`: no service held it, or none could be asked | | | yes | | | yes |
| `Refused`: the title check refused the record | | | yes | | | |
| `Unhashable`: the file has no content hash | | | yes | | | |
| `AwaitingAcceptance`: written when the operator's move is made | | | yes | | | |

`Evidence::not_attempted(reason)` builds the evidence of a verdict
reached before any step ran, which is the content duplicate. Every
section is `NotAttempted(reason)`, and the library is
`NotConsulted(reason)`.

`OutsideLibrary` covers both "outside" and "excluded" because
`Stores::consult` answers `None` for both. Separating them would mean
changing `consult`, and nothing in Phase 3 asks for that.

**Rejected: a reason enum per section.** It would make some impossible
states unrepresentable, but at the cost of seven enums repeating the
same five reasons. The discussion asks for one uniform not-attempted
shape in the stream, and one enum is what that shape renders from.
The table above is tested (tasks 6.x) rather than enforced by types.

## D10. The conflict candidate, and the hash it is never stored under

**Decision.** `Standing::refused` keeps the refused `FileRecord`, and
its `evidence` now holds:

- the lookup with every attempt and the origin;
- where the record was retrieved from;
- the extraction result and titles;
- `match_check: Conflict(..)`;
- `content_index.write: NotAttempted(Refused)`.

`Standing::evidence` equals it (D2). Behaviour does not change: no
`index.put` runs for a refused record, and what the index already held
for the hash is left as it was. The new tests pin both the write's
absence and an earlier entry surviving a `--no-cache` conflict
(task 7.1).

The response cache still holds the service's answer under the
identifier, written by `Cached` before the title check ran. That entry
records what the service holds for an identifier and says nothing
about the file, so it does not associate the candidate with the file's
hash. A later run asks the content index, misses, extracts again, and
the title check refuses the record again.

**Accepting the candidate.** An operator who renames a file over its
own conflict accepts the refused record. Its evidence then changes in
exactly two places, both made by `pipeline::accept(file: FileRecord)
-> FileRecord`:

- `overrode` becomes the `Overridden` built from `match_check`'s
  `Conflict`, with the same field, titles and similarity the skip
  carried.
- `content_index.write` changes from `NotAttempted(Refused)` to
  `NotAttempted(AwaitingAcceptance)`.

`match_check` stays `Conflict`, since the check's conclusion has not
changed. Overriding it is what `overrode` records, and change 9 will
render that as `acceptance`. Any other record keeps its write as it
was when `accept` sees it: a record the index already holds keeps
`Attempted(..)` or `NotAttempted(ContentIndexHit)`, and an operator's
lookup keeps `AwaitingAcceptance`. A record that has no `Conflict`
gets `overrode` `None`.

`run::accepted` keeps its place in the driver and delegates to
`accept`. The `Offer::conflict` it used to read is replaced by the
match check that `accept` reads. The rename-time write is still made
only on the move, through `remember`, as before.

Keeping the candidate does not put it in front of anyone. The batch
path still skips the file, only an operator can accept the record
(living requirement "Ambiguity is skipped, never guessed"), and no
search or suggestion interface is introduced.

## D11. Operator lookups

**Decision.** The new signature is:

```rust
pub fn resolve_supplied(
    path: &Path,
    identifier: &Identifier,
    origin: Origin,
    documents: &dyn Documents,
    sources: &[&dyn Source],
    prior: &Evidence,
) -> Result<FileRecord, Unresolved>
```

The returned record's evidence is `prior` with four sections
replaced:

- `lookup`: `Attempted { identifier, origin, attempts }`;
- `titles`: from `titles_of`, read now as today;
- `match_check`: from `check_title` over those titles;
- `content_index.write`: `NotAttempted(AwaitingAcceptance)`.

The library answer, the index read and the extraction result come from
`prior`, so the driver no longer copies `library` across. A retry
passes `Origin::Extracted(tier)`, so the driver no longer overwrites
`tier`. `Supplied` is removed, and its `conflict` becomes
`FileRecord::conflict()`, the schema-3 `SkipReason::Conflict` projected
from `match_check`.

**Two kinds of evidence in the driver.** The interactive driver holds
the file's own resolution evidence apart from any candidate's:

- **The file's own evidence** starts as `Standing::evidence`. It is
  what the held verdict reports and what `situation` reads to decide
  whether a retry is offered (`lookup.is_conclusive()`). This takes the
  place of the `unresolved` local that `run::asked` keeps today.
- **A candidate's evidence** is the evidence on the `FileRecord`
  inside an `Offer` that a supplied identifier reached. It is reported
  only if the operator accepts that candidate.

What each operator answer does to them:

| Answer and outcome | File's own evidence | Candidate |
|---|---|---|
| Retry, `Ok(file)` | becomes `file.evidence`, and `own` becomes that record | none |
| Retry, `Err(unresolved)` | becomes `unheld_evidence(&own, identifier, Origin::Extracted(tier), &unresolved)` | none |
| Supply, `Ok(file)` | unchanged | `file`, with its own evidence |
| Supply, `Err(unresolved)` | unchanged | unchanged; the attempts reach the description's report alone |

A failed retry is the file's own resolution, so it replaces the file's
lookup, as today's driver replaces both `held` and `unresolved`
(`run.rs`, around line 2112). Suppose the first lookup hit an outage
and the retry then finds every service answering not found. The
lookup is then conclusive, and the next menu no longer offers a retry.
A failed supply is a candidate that led nowhere: its attempts belong
to it and never touch the file's own evidence. So a file whose own
lookup was an outage is still offered a retry after a failed supply.

`pipeline::unheld_evidence(prior, identifier, origin, unresolved) ->
Evidence` builds the evidence of a lookup that no service answered. It
is `prior` with:

- `lookup` set to `Attempted { identifier, origin, attempts }`, with
  the attempts taken from `unresolved` in order and each one `Err`;
- `match_check` and `content_index.write` set to
  `NotAttempted(NoRecord)`;
- the titles, extraction result, library answer and index read left as
  `prior` has them.

`unheld_evidence` is a pure function, so the retry rule can be tested
in the engine and not only through the terminal. Change 9 uses the
same function to give a failed operator lookup its lookup section.

A retry's evidence covers the retry. The failed lookup it replaced is
not kept beside it. A history of what the operator tried is part of
operator-input evidence, which is change 10's.

## D12. The interface the tests are written against

```rust
// crates/borax-sources/src/cache.rs
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheWrite { Written, Failed { message: String } }
pub trait Cache: Sync {
    fn get(&self, key: &str) -> Option<Record>;
    fn put(&self, key: &str, record: &Record) -> CacheWrite;
}

// crates/borax-sources/src/source.rs
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched { pub record: Record, pub retrieval: Retrieval }
impl Fetched {
    /// `record` as sent by the service, with no response cache in front.
    pub fn network(record: Record) -> Fetched;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Retrieval { ServiceCache, Network { stored: Option<CacheWrite> } }
pub trait Source: Sync {
    fn name(&self) -> SourceName;
    fn supports(&self, identifier: &Identifier) -> bool;
    fn fetch(&self, identifier: &Identifier) -> Result<Fetched, SourceError>;
}

// crates/borax-sources/src/store.rs
impl<C: Cache> ContentIndex<C> {
    pub fn put(&self, hash: &ContentHash, record: &Record) -> CacheWrite;
}

// crates/borax-sources/src/dispatch.rs
pub struct Resolved {
    pub record: Record,
    pub source: SourceName,
    pub retrieval: Retrieval,
    pub failures: Vec<(SourceName, SourceError)>,
}
// Unresolved: unchanged.

// crates/borax-sources/src/conflict.rs
#[derive(Debug, Clone, PartialEq)]
pub enum TitleCheck { Agreed, Conflict(Conflict), Insufficient(Insufficient) }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insufficient { RecordUntitled, NoTitles, NoEvidence }
impl TitleCheck { pub fn conflict(self) -> Option<Conflict>; }
pub fn check_title(candidates: &[&str], record: &Record) -> TitleCheck;

// crates/borax/src/evidence.rs (all: Debug, Clone, PartialEq)
pub struct Evidence {
    pub library: Consultation,
    pub content_index: IndexEvidence,
    pub extraction: ExtractionEvidence,
    pub lookup: LookupEvidence,
    pub match_check: MatchCheck,
}
impl Evidence {
    pub fn not_attempted(reason: Unattempted) -> Evidence;
    /// Where the record came from: the step that produced it, by D2's
    /// precedence (found attempt, then library answer, then index hit).
    /// The one source for FileRecord::source, tier and cached.
    pub fn retrieval(&self) -> Option<RecordRetrieval>;
}
pub enum Consultation { Consulted(LibraryAnswer), NotConsulted(Unattempted) }
impl Consultation { pub fn answer(&self) -> Option<&LibraryAnswer>; }
pub struct IndexEvidence { pub read: IndexRead, pub write: IndexWrite }
pub enum IndexRead { Hit, Miss, Bypassed, Unavailable { message: String }, NotAttempted(Unattempted) }
pub enum IndexWrite { Attempted(CacheWrite), NotAttempted(Unattempted) }
pub struct ExtractionEvidence { pub result: ExtractionStep, pub titles: Titles }
pub enum ExtractionStep { Ran(Extraction), NotAttempted(Unattempted) }
pub enum Titles { Read(Vec<Claim>), Failed { message: String }, NotAttempted(Unattempted) }
impl Titles { pub fn claims(&self) -> &[Claim]; }
pub enum LookupEvidence {
    Attempted { identifier: Identifier, origin: Origin, attempts: Vec<ServiceAttempt> },
    NotAttempted(Unattempted),
}
impl LookupEvidence {
    pub fn extracted(&self) -> Option<(&Identifier, Tier)>;
    pub fn is_conclusive(&self) -> bool;      // as Unresolved::is_conclusive; false once found
    pub fn no_eligible_service(&self) -> bool; // Attempted with no attempts
}
pub enum Origin { Extracted(Tier), Operator }  // Copy, Eq
pub struct ServiceAttempt { pub service: SourceName, pub outcome: Result<Retrieval, SourceError> }
pub enum RecordRetrieval {
    Library { artifact: String, item: String },
    ContentIndex,
    ServiceCache { service: SourceName },
    Network { service: SourceName },
}
pub enum MatchCheck {
    Agreed, Conflict(Conflict), Insufficient(Insufficient), NotAttempted(Unattempted),
}
pub enum Unattempted {   // Copy, Eq
    NoLibrary, OutsideLibrary, ContentDuplicate, LibraryAnswered, ContentIndexHit,
    ExtractionFailed, NoRecord, Refused, Unhashable, AwaitingAcceptance,
}
impl Unattempted { pub fn as_str(self) -> &'static str; } // kebab-case

// crates/borax/src/pipeline.rs
pub struct FileRecord {
    pub record: Record,
    pub hash: Option<ContentHash>,
    pub evidence: Evidence,
    pub overrode: Option<Overridden>,
}
impl FileRecord {
    pub fn source(&self) -> Option<SourceName>;
    pub fn tier(&self) -> Option<Provenance>;
    pub fn found(&self) -> Option<&Identifier>;
    pub fn claims(&self) -> &[Claim];
    pub fn cached(&self) -> bool;
    pub fn library(&self) -> Option<&LibraryAnswer>;
    pub fn conflict(&self) -> Option<SkipReason>;
}
pub struct Standing {
    pub verdict: FileOutcome,
    pub hash: Option<ContentHash>,
    pub evidence: Evidence,
    pub refused: Option<FileRecord>,
    pub duplicated: Option<Duplicated>,
}
impl Standing {
    pub fn library(&self) -> Option<&LibraryAnswer>;
}
pub struct FileRead { pub extracted: Result<Extracted, ExtractionError>, pub titles: Titles }
pub fn from_file(path: &Path, documents: &dyn Documents, config: &ExtractionConfig) -> FileRead;
pub fn titles_of(path: &Path, documents: &dyn Documents) -> Titles;
pub fn index_read<C: Cache>(hash: Result<&ContentHash, &str>, index: &ContentIndex<C>,
    config: &ResolveConfig) -> (Option<Record>, IndexRead);
pub fn title_check(titles: &Titles, record: &Record) -> MatchCheck;
pub fn resolve_supplied(path: &Path, identifier: &Identifier, origin: Origin,
    documents: &dyn Documents, sources: &[&dyn Source], prior: &Evidence)
    -> Result<FileRecord, Unresolved>;
pub fn unheld_evidence(prior: &Evidence, identifier: &Identifier, origin: Origin,
    unresolved: &Unresolved) -> Evidence;
pub fn accept(file: FileRecord) -> FileRecord;
pub fn remember<C: Cache>(index: &ContentIndex<C>, hash: Option<&ContentHash>,
    record: &Record) -> IndexWrite;
// standing, resolve_file, resolve_batch, verdict_event, event_for,
// resolved_event, extraction, extraction_of, unresolvable, attempts_of:
// signatures unchanged.
```

`index_read` receives the hash result as `Result<&ContentHash, &str>`,
where the `&str` is the hashing error's message, so that
`Unavailable` can carry it. The implementer may reshape this one
helper as long as `standing`'s evidence is what D6 states. No test
calls `index_read` directly, so its signature stays the implementer's
call.

`disagreement`, `claims_of`, `from_index`, `Supplied` and the removed
fields are deleted outright, with no alias. That follows the pre-1.0
rule, and the only callers are this crate and its tests.

## D13. How the specifications change

**Decision.** Every delta is ADDED: seven requirements in
`resolution` and one in `extraction`. Each is phrased as what a
resolution *retains and distinguishes*, and each is testable through
the `borax` library crate as `crates/borax/tests/pipeline.rs` tests
today. None names a JSON field or an event, so none becomes false when
change 9 renders the facts.

Nothing is MODIFIED. "A resolution reports the evidence it was
checked against" and "A resolution says whether its library answered"
describe the schema-3 stream, which this change keeps byte for byte.
Change 9 owns both. "Extraction failures are typed and non-fatal" is
still not met by the stream's skip reasons. The engine now holds the
four modes apart, and STATE.md says so (task 10.2). Amending that
requirement to fit the stream would bend an authority to match the
code.

## Not changed, deliberately

- `event.rs`: `SCHEMA`, every event, every `SkipReason`, `Attempt`,
  every human line and `Counts`.
- `pipeline::skipped_for`, `unresolvable` and `attempts_of`, which
  still build the schema-3 skip.
- Which sources are asked, their order, pacing, cache keys, and what
  `--no-cache` bypasses.
- When anything is written. The batch write happens as it resolves,
  the operator's write happens on the move, and no refused record is
  ever written.
- The interactive menus and their order. `situation` reads conclusiveness
  from the evidence and gives the same answer.

## Risks / Trade-offs

- **Churn in the tests.** The trait changes touch twelve `fetch` fakes
  and three `put` fakes. Field-to-method edits and `FileRecord`
  literals touch `pipeline.rs`, `event.rs`, `bib.rs` and `renaming.rs`.
  All of these edits are mechanical, and the tasks preamble forbids
  changing any assertion about output, so the suite still guards the
  stream.
- **The interactive driver is the least-covered path**
  (`openspec/STATE.md`, "the interactive human-output paths are the
  thinnest-covered part of the suite"). Removing `Standing`'s fields
  rewires `run::asked`. The existing dispatch tests of supply, retry,
  override and quit run unchanged, among them
  `an_unreachable_service_offers_a_retry_before_supplying_an_identifier`,
  `an_identifier_no_service_holds_offers_no_retry_when_the_answer_is_conclusive`
  and `interactive_retry_after_an_outage_keeps_the_dangling_item_problem`.
  Task 8.1 adds the engine-level checks on the retry's origin, and
  task 8.2 the dispatch-level one.
- **`#[must_use]` on `CacheWrite`** makes `put` noisier at call sites
  that do not care. There are none in production after this change.
  In tests the cost is a `let _ =`.
- **`OutsideLibrary` is coarse** (D9). If a consumer of schema 4 needs
  to tell an excluded subtree from a file beyond the root,
  `Stores::consult` has to say which, and that is a later change.
