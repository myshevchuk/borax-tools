# Design: sectioned-resolved-event

## Context

Everything below was read from source at `834ffbd` (change 8 merged on
top of 0.8.0). The decisions this change carries out come from the
Phase 3 resolution-schema design discussion (maintainer's notes,
outside the repository).

Binding decisions:

1. **One bump.** The schema goes from 3 to 4, here and only here.
   `found`, `cached`, `source`, `tier`, `claims` and `overrode` are
   removed, with no aliases or dual output. Change 10 adds to schema 4
   additively before 0.9.0.
2. **Skips.** A resolution-verdict skip keeps `reason.kind` as a slim
   dispatch key and carries the same sections as `resolved`. Each fact
   is stated once. The kinds are `no-text-layer`,
   `text-without-identifier`, `encrypted`, `unreadable` (which keeps
   its message), `unresolvable` and `conflict`. Skips that are not
   resolution verdicts are unchanged.
3. **Attempts.** Each attempt is a `service` with an `outcome` tagged
   `found`, `not-found`, `unavailable {message}`, `rate-limited` or
   `malformed {message}`. A found attempt carries its `retrieval`.
   Failed attempts before a success survive.
4. **The library.** Two sections: `library`, for what the library said
   or why it was not consulted, and `record_retrieval`, for where the
   record came from.

The rest of the discussion is a set of defaults: the section names,
the uniform not-attempted shape, the content-index statuses, the
extraction and title vocabulary, `no-eligible-service`, the
`match_check` statuses, the `acceptance` values, and cache writes
reported in the section of the store written. This design keeps all of
them, and refines only spelling and nesting where D1 says so.

What the engine holds, from change 8 (`crates/borax/src/evidence.rs`):

- `Evidence { library, content_index, extraction, lookup, match_check
  }` on `FileRecord` and on `Standing`;
- `Evidence::retrieval()` for where the record came from;
- `Unattempted::as_str()` for the kebab-case reasons;
- `pipeline::remember`, which returns an `IndexWrite` that `run.rs`
  drops unreported (`let _remembered`).

None of the engine types derives `Serialize`. Change 8 left that
choice to this change (its D1).

How events are written today:

- `resolved_event(path, &FileRecord)` and `verdict_event(path,
  &Standing)` build `resolved` and resolution `skipped` events.
- `event_for(path, &FileOutcome)` builds a `skipped` from a bare
  `SkipReason`. It is used only by tests.
- The interactive driver (`run::asked`) builds its own `Event::Skipped`
  for a retry's outcome.
- `renaming::Applying::carry_out` builds the rename-time skips.
- `Logging` (`run.rs`) writes a move's `renamed` event to the log
  before the move (`Sink::record`). Its `emit` then skips writing the
  same event a second time, but only if the emitted event is equal to
  the one already recorded (`recorded.take().is_some_and(|last| last ==
  event)`).

## D1. Shape conventions

**Decision.** These rules hold across every section:

- **Keys and values.** Section and field keys are snake_case and values
  are kebab-case, as the stream already writes them (`content_index`,
  `not-attempted`).
- **`status` and `kind`.** `status` tags anything that says *what
  happened*: a step, a part of a step, a service's outcome, an
  acceptance. `kind` tags a value that says *which one*: a library
  answer, a record's retrieval, a skip's reason. The stream already
  uses `kind` for its classifying values (`reason`, `library`,
  `extraction` on `library-extraction`). `status` is new and belongs to
  the steps.
- **Steps that did not run.** Each is exactly `{"status":
  "not-attempted", "reason": "<kebab-case>"}`, at whatever depth the
  step sits, and it is never omitted.
- **Two-step sections.** `content_index` and `extraction` each hold two
  steps, so each is an object with one key per step: `read` and
  `write`, and `result` and `titles`. Every other section is a single
  step and is itself `status`-tagged. The exception is
  `record_retrieval`, which is a place rather than a step: it is
  `kind`-tagged, or `null` when there is no record.
- **Order.** The section keys follow the pipeline. Change 10's
  `identifier_input` goes between `extraction` and `lookup`. An added
  key is something a consumer that ignores unknown keys reads
  unchanged, so no second bump is needed (the `cli` requirement "JSON
  Lines output is first-class").

Refinements to the discussion's defaults, all spelling or nesting:

- `content_index` has `read` and `write` sub-objects instead of a
  section-level status beside a `write`. Each part is then a uniform
  step.
- `extraction.result` uses `status` with change 4's five kind names
  (`found`, `no-text-layer`, …) as its values. The `library-extraction`
  event keeps `kind` and does not change.
- `library` nests the schema-3 `LibraryAnswer` unchanged under `answer`.
  Its kinds and fields are exactly what schema 3 put in `library`.
- `lookup.origin` is the string `extracted` or `operator`. It does not
  repeat the tier, which is `extraction.result.tier`, so the pass is
  stated once.

**Rejected: tag everything with `kind`.** "Not attempted" is not a kind
of library answer or of extraction result. Making it one would put a
step's absence into the vocabulary of its answers. That is the
conflation the uniform not-attempted shape exists to avoid.

**Rejected: flatten `LibraryAnswer`'s kinds into the `library` status**
(for example `{"status": "dangling-item", …}`). The maintainer's
decision 4 says `library` *keeps* the `LibraryAnswer`. Nesting it keeps
its eleven-kind vocabulary byte for byte, so a consumer's schema-3
dispatch code moves one level down instead of being rewritten.

## D2. Serde: event-side types, projected from the engine

**Decision.** The serializable section types are new event-side types
in `crates/borax/src/event.rs`, next to `LibraryAnswer`, `Extraction`
and `Claim`, which is where the stream's vocabulary already lives. Each
derives `Serialize` and `Deserialize`. A single projection,
`Evidence::sections(&self, acceptance) -> Sections` in `evidence.rs`,
maps the engine's evidence onto them. Neither `event.rs` nor
`evidence.rs` needs a new dependency: `evidence.rs` already imports
from `event.rs`.

Why event-side types:

- **The round trip.** `Event` derives `Deserialize`, and
  `crates/borax/tests/event.rs` (`every_event_round_trips_through_json_line_and_back`)
  holds every event to a round trip. `Serialize` on the engine types
  would therefore need `Deserialize` on `Identifier`, `Tier`,
  `SourceName`, `SourceError`, `CacheWrite`, `Retrieval`, `Conflict`
  and `Insufficient`. Those live in `borax-core`, `borax-pdf` and
  `borax-sources`, crates that know nothing about the stream. That
  would put serde derives and attributes into three crates to satisfy
  one crate's format.
- **The engine's shapes are not the stream's.**
  - `ServiceAttempt::outcome` is a `Result<Retrieval, SourceError>`,
    which serde writes as `{"Ok": …}`.
  - `Conflict::field` is `&'static str`, which cannot be deserialized.
  - `Origin::Extracted(Tier)` repeats the tier that D1 states once.
  - `Retrieval::Network { stored: None }` has to become a not-attempted
    step with a reason.

  Serde attributes could bend each of these, but every bend would be an
  attribute on a type in another crate.
- **House style.** `Extraction` (change 4) was built the same way. It
  is an event-side type that `pipeline::extraction_of` fills from the
  extractor's `Result`.

The cost is a second set of type definitions and one mapping function.
The mapping is total and has no logic beyond renaming. Any engine
variant without a mapping fails to compile, because the projection
matches without wildcard arms, as `extraction_of` does.

**Rejected: `#[derive(Serialize, Deserialize)]` on the engine types with
`#[serde(remote)]` or `serialize_with` adapters.** This keeps one set of
types, but it splits each section's spelling between attributes in
four crates. It also still needs custom code for the `Result` and the
`&'static str`.

**Rejected: serialize-only events.** Dropping `Deserialize` from `Event`
would delete the round-trip guard, which is the cheapest test that the
stream is self-consistent.

## D3. Field table

This table is the normative shape. The spec deltas require the
behaviour, and this table fixes the spelling. Every section is present
on every `resolved` event and on every resolution-verdict `skipped`
event, in the order listed. N/A ("not attempted") is always
`{"status":"not-attempted","reason":R}`. The reason R is a kebab-case
name from `Unattempted::as_str`:

- `no-library`
- `outside-library`
- `content-duplicate`
- `library-answered`
- `content-index-hit`
- `extraction-failed`
- `no-record`
- `refused`
- `unhashable`
- `awaiting-acceptance`
- `cache-bypassed` (new; D11)

### Top level

| Key | Values | Present |
|---|---|---|
| `schema` | `4` | every event |
| `event` | `resolved`, `skipped`, `content-index-write`, … | every event |
| `path` | the file as the run names it | every event |
| `identifier` | the record's preferred identifier (`doi:…`, `arXiv:…`, `pmid:…`, `isbn:…`), or `""` if it carries none | `resolved` |
| `record` | the full CSL-JSON record, provenance in `record.borax.provenance` | `resolved` |
| `reason` | `{"kind": K}`, plus `message` when K is `unreadable`, and `reason` and `existing_path` when K is `duplicate`, as in schema 3 (D4) | `skipped` |
| `candidate` | the full CSL-JSON record the title check refused | `skipped` with reason `conflict`, only |

### `library`

| `status` | Fields | When |
|---|---|---|
| `consulted` | `answer`: a schema-3 `LibraryAnswer` object, tagged by `kind`: `tracked {artifact, item}`, `untracked`, `unrecognised-content {artifacts}`, `ambiguous {artifacts}`, `no-item {artifact}`, `dangling-item {artifact, item}`, `unreadable-item {artifact, item, path, message}`, `ambiguous-item {artifact, item, files}`, `unhashable {artifacts}`, `unreadable-records {listed, unreadable}` | the library was asked |
| `not-attempted` | `reason`: `no-library`, `outside-library`, or `content-duplicate` (the content-duplicate check settled the file first) | not asked |

### `content_index`

`{"read": READ, "write": WRITE}`

| READ `status` | Fields | When |
|---|---|---|
| `hit` | | the index held a record for the hash |
| `miss` | | it held none |
| `bypassed` | | the run turned the cache off (`--no-cache`, `network.cache = false`) |
| `unavailable` | `message`: the hashing error | the file could not be hashed (outranks `bypassed`) |
| `not-attempted` | `reason`: `content-duplicate`, `library-answered` | a content duplicate or the library settled the file first |

| WRITE `status` | Fields | When |
|---|---|---|
| `written` | | the write was made |
| `failed` | `message`: the store's | the write failed; never a failure of anything else |
| `not-attempted` | `reason`, the earliest in pipeline order: `content-duplicate`, `library-answered`, `content-index-hit`, `extraction-failed`, `no-record`, `refused`, `awaiting-acceptance`, `unhashable` | no write was made |

### `extraction`

`{"result": RESULT, "titles": TITLES}`

| RESULT `status` | Fields | When |
|---|---|---|
| `found` | `identifier` (`doi:…`), `tier` (`embedded-metadata`, `text-layer`) | a pass found an identifier |
| `no-text-layer` | | no page read held text |
| `text-without-identifier` | | the pages held text and no identifier |
| `encrypted` | | the file needs a password |
| `unreadable` | `message`: the reader's | the file could not be opened or parsed |
| `not-attempted` | `reason`: `content-duplicate`, `library-answered`, `content-index-hit` | extraction did not run |

| TITLES `status` | Fields | When |
|---|---|---|
| `read` | `claims`: `[{"from": "xmp" or "info", "title": …}]`, possibly `[]` | the file was opened; `[]` means it claims no title |
| `failed` | `message`: the open error's | the file could not be opened |
| `not-attempted` | `reason`: `content-duplicate`, `library-answered`, `content-index-hit` | the file was not opened |

RESULT and TITLES are independent. An operator's lookup after a
content-index hit reports RESULT not attempted and TITLES `read`,
because `resolve_supplied` reads the titles for its check.

### `lookup`

| `status` | Fields | When |
|---|---|---|
| `attempted` | `identifier`, `origin` (`extracted` or `operator`), `attempts`: non-empty `[ATTEMPT]` in the order asked | at least one service was asked |
| `no-eligible-service` | `identifier`, `origin` | no configured service supports the identifier |
| `not-attempted` | `reason`: `content-duplicate`, `library-answered`, `content-index-hit`, `extraction-failed` | nothing was looked up |

ATTEMPT is `{"service": S, "outcome": OUTCOME}`, where S is
`crossref`, `openalex`, `arxiv`, `datacite` or `pubmed`.

| OUTCOME `status` | Fields | When |
|---|---|---|
| `found` | `retrieval`: `service-cache` or `network`; `stored`: WRITE (`written`, `failed {message}`, or `not-attempted` with `cache-bypassed`), present exactly when `retrieval` is `network` | the service supplied the record; always the last attempt |
| `not-found` | | the service does not hold the identifier |
| `unavailable` | `message` | unreachable, or a server error |
| `rate-limited` | | the service asked the run to slow down |
| `malformed` | `message` | a response that is not a record |

`origin` `extracted` covers a lookup an operator asked to be made again
over the file's own identifier. `operator` is an identifier the
operator supplied. When `origin` is `extracted`, the pass that read the
identifier is `extraction.result.tier`.

### `record_retrieval`

| `kind` | Fields | When |
|---|---|---|
| `library` | `artifact`, `item` | the library item answered |
| `content-index` | | the content index answered |
| `service-cache` | `service` | a service's response cache answered; no request was sent |
| `network` | `service` | the service answered over the network |
| (value `null`) | | the verdict reached no record |

On a conflict skip, the value describes the candidate. On a
work-duplicate skip, it describes the record the file resolved to. On a
content-duplicate skip it is `null`. An operator's
lookup over a tracked or indexed file reports the service, not the
library or the index (change 8's precedence in `Evidence::retrieval`).

### `match_check`

| `status` | Fields | When |
|---|---|---|
| `agreed` | | a claimed title names the record's work |
| `conflict` | `field`, `extracted`, `resolved`, `similarity` | the titles name another work, on a conflict skip or on an accepted record alike |
| `insufficient-evidence` | `reason`: `record-untitled`, `no-titles`, `no-evidence` | too little to judge |
| `not-attempted` | `reason`: `content-duplicate`, `library-answered`, `content-index-hit`, `extraction-failed`, `no-record` | the check was not made |

### `acceptance`

| `status` | When |
|---|---|
| `automatic` | a record was used and no conflict was overridden to use it |
| `overridden` | an operator accepted the record over `match_check`'s conflict |
| `not-applicable` | the verdict is a skip, including a conflict skip and a work duplicate, whose record is reported but not used |

### `content-index-write` (new event, D8)

| Key | Values |
|---|---|
| `path` | the file at the path it holds after the move (as on `library-admission`) |
| `write` | WRITE: `written`, or `failed` with `message` |

## D4. The skip

**Decision.** A `skipped` event is a resolution verdict exactly when its
`reason.kind` is one of the seven resolution kinds below: the six the
maintainer's decision 2 names, and `duplicate`.
`SkipReason::is_resolution_verdict()` says which kinds those are.

`duplicate` is a verdict because `standing` is the only place that
builds it (`pipeline::content_duplicate` and `pipeline::admissible`);
no later step of a run produces one. Both kinds, by content and by
work, are therefore resolution verdicts whatever the run does next. The
maintainer settled this on 2026-10-03, after the first draft of this
change had followed the discussion's list, which named "duplicate"
among the skips that are not verdicts.

| `reason.kind` | Reason fields | Facts that moved out of the reason |
|---|---|---|
| `no-text-layer` | | `extraction.result` |
| `text-without-identifier` | | `extraction.result` |
| `encrypted` | | `extraction.result` |
| `unreadable` | `message` | `extraction.result` (`message` too; the maintainer kept it on the reason) |
| `unresolvable` | | `lookup` (identifier, origin, attempts); the tier in `extraction.result` |
| `conflict` | | `match_check` (field, extracted, resolved, similarity); `candidate` |
| `duplicate` | `reason` (`content` or `work`), `existing_path`, as in schema 3 | nothing: neither field is evidence the sections hold |

Two invariants hold, and tests pin them:

- A resolution skip carries all seven sections.
- A non-resolution skip carries `path` and `reason` and nothing else.
  Its reason is spelled as it was in schema 3.

The non-resolution kinds are `target-taken`, `unnameable`, `declined`,
`rename-failed`, `bib-write-failed`, `unciteable`, `sidecar-taken`,
`unrecordable` and `stranding`. Schema 3's `library: null` on these is
removed. `library` is now a section, and a section's
absence is how a consumer knows the skip is not a verdict.

`unreadable`'s `message` is stated twice, on the reason and in
`extraction.result`. That follows the maintainer's decision 2, and the
facts-once rule yields to it here only.

**Duplicates.** Each duplicate skip carries `Standing::evidence`
projected, with `acceptance` `not-applicable` and no `candidate`:

- **By content.** The verdict is reached before any step runs, so the
  evidence is change 8's `Evidence::not_attempted(ContentDuplicate)`.
  `library`, both `content_index` steps, both `extraction` steps,
  `lookup` and `match_check` are each not attempted with reason
  `content-duplicate`, and `record_retrieval` is `null`.
- **By work.** The file resolved, and the library's work check made the
  verdict a duplicate. `Standing::evidence` equals
  `duplicated.file.evidence` (change 8's D2), so the sections are the
  evidence of the record the file resolved to, and `record_retrieval`
  says where that record came from. This keeps the `LibraryAnswer` that
  schema 3's work-duplicate skip carried, which is the only report of
  it, since a batch rename emits no `resolved` event for the file.

A work duplicate's record is reported through its evidence, not as
`candidate`. `candidate` stays the conflict's refused record alone, so
the key keeps one meaning. The interactive run's "file it as another
artifact" answer already reports that record on the `resolved` event it
emits.

**Rejected: a separate event tag for resolution skips.** It would avoid
the invariant, but the skip queue and the `skipped` counter are counted
from `skipped`. The living requirement "The run summary reports the
skip queue" counts those events.

**Engine.** `skipped_for(&ExtractionError)` maps:

| Error | Kind |
|---|---|
| `NoTextLayer` | `no-text-layer` |
| `NoIdentifierFound` | `text-without-identifier` |
| `Encrypted` | `encrypted` |
| `Unreadable { message }` | `unreadable { message }` |

There is no wildcard arm. `SkipReason::Unresolvable` and
`SkipReason::Conflict` become unit variants, and `NoIdentifier` is
removed.

## D5. Attempts

**Decision.** `ServiceAnswer { service, outcome }` is built from
change 8's `ServiceAttempt`:

| Engine `outcome` | `ServiceOutcome` |
|---|---|
| `Ok(Retrieval::ServiceCache)` | `Found { retrieval: ServiceCache, stored: None }` |
| `Ok(Retrieval::Network { stored: Some(CacheWrite::Written) })` | `Found { retrieval: Network, stored: Some(WriteStep::Written) }` |
| `Ok(Retrieval::Network { stored: Some(CacheWrite::Failed { message }) })` | `Found { retrieval: Network, stored: Some(WriteStep::Failed { message }) }` |
| `Ok(Retrieval::Network { stored: None })` | `Found { retrieval: Network, stored: Some(WriteStep::NotAttempted { reason: "cache-bypassed" }) }` |
| `Err(NotFound)` | `NotFound` |
| `Err(Unavailable { message })` | `Unavailable { message }` |
| `Err(RateLimited)` | `RateLimited` |
| `Err(Malformed { message })` | `Malformed { message }` |

`LookupEvidence::Attempted` with no attempts becomes `LookupStep::
NoEligibleService`. Any other `Attempted` becomes `LookupStep::
Attempted`. `attempts_of(&Unresolved)` returns `Vec<ServiceAnswer>` for
the interactive report of a failed supply, which is a candidate's and
reaches no event.

**`stored` is absent on a response-cache hit.** Nothing was fetched, so
there was no write to make. That is not a step that did not run.

**Rejected: a lookup-level summary status** (`found`, `not-found`,
`failed`). It can be derived from the attempts, so it would be a second
statement of the same facts that could disagree with the first. The
`unresolvable` reason already says no service supplied a record.

## D6. Where the record came from, and who wrote its fields

**Decision.** `record_retrieval` is `Evidence::retrieval()` projected:

- `Library` → `library {artifact, item}`
- `ContentIndex` → `content-index`
- `ServiceCache { service }` → `service-cache {service}`
- `Network { service }` → `network {service}`
- `None` → `null`

Which services wrote the record's fields is `record.borax.provenance`,
which every record already carries. The stream adds no field for it.

The human line still names services ("via crossref"). The rule moves
from `pipeline::sources_of` to `event::services_of(record, retrieval)`,
which the human line and the description share:

- a `service-cache` or `network` retrieval names that service;
- otherwise, the services `record.borax.provenance` names other than
  `extraction`, in the fixed order Crossref, OpenAlex, arXiv, DataCite,
  PubMed, then sidecar;
- none, when the provenance names none of them.

The schema-3 stand-ins `cache` and `library` go. A store is never named
as though it were a service.

## D7. Acceptance

**Decision.** `acceptance` carries only its status. `overridden` points
to `match_check`, which still holds the `conflict` with its field, both
values and the similarity. Change 8's `pipeline::accept` leaves the
match check as `Conflict` on purpose (its D10). Copying the details
into `acceptance` would state them twice.

In the engine, `FileRecord::overrode: Option<Overridden>` becomes
`FileRecord::overridden: bool`, for the same reason. `accept` sets it
when `match_check` is `Conflict`. `FileRecord::acceptance()` gives
`Overridden` or `Automatic`. Every resolution skip gives
`NotApplicable`. `Overridden`, the type, is removed.
`run::reidentified` reads `overridden`, together with the lookup's
origin.

**What `automatic` means in change 9.** A record was used, and no
title-check conflict was overridden to use it. It does not mean "no
person was involved": every move in an interactive run is an operator's
answer, and that answer is the rename, not an acceptance over evidence.
A record an operator *supplied*, whose titles did not conflict, also
reports `automatic` here. Change 10 introduces `pending`, `accepted` and
`rejected` for operator candidates, and is expected to report that case
as `accepted`. Changes 9 and 10 both land before 0.9.0 ships, so no
released stream carries the interim meaning.

The maintainer accepted this interim value on 2026-10-03, on that
condition: **change 10 changes the `acceptance` of a supplied record
without a conflict from `automatic` to its own value before 0.9.0
ships.** A consumer built against an unreleased build between the two
changes is not owed the interim value.

**Rejected: `overridden` carrying `{field, extracted, resolved,
similarity}`.** It was schema 3's `overrode` under a new name, and the
same values would sit in `match_check` on the same event.

## D8. Where the rename-time content-index write is reported

Change 8 left this open (its D5), noting that putting it on `renamed`
means making the write after the move and before that event.

**Decision: a new event, `content-index-write {path, write}`.** It is
emitted immediately after `pipeline::remember`. That means after the
file's `renamed` event and its `library-admission` event, if there is
one, and before its sidecar. It is emitted only when `remember` ran,
that is, when an operator-reached record (supplied, retried, or
accepted over a conflict) was moved. `remember` cannot return
`NotAttempted(Unhashable)` after a `renamed`, because an applying
rename of an unhashable file is refused as `unrecordable` first. So
`write` is always `written` or `failed`.

Why not `renamed`:

- **The log and the stream would diverge.** `moved` passes the intended
  `renamed` event to `Sink::record`, which writes and flushes it to the
  run log *before* the move. That is what the `run-logs` requirement
  "Apply-run logs are mandatory and flushed before mutation" demands.
  The write's result does not exist yet at that point. If the emitted
  `renamed` then carried the result, it would differ from the recorded
  one. `Logging::emit` would see `last != event` and write a second
  `renamed` line to the log. The stream would carry one, against the
  `run-logs` scenario "Run log equals --json output". The only
  alternative is to weaken that equality check, which would leave the
  log's `renamed` without the write forever.
- **It cannot be "before the move" either.** The record is written
  only once the move is made, and a move that did not happen is not an
  acceptance (the living requirement "An operator's answer about a file
  is remembered").

Why not `resolved`: the verdict is emitted before the move and already
reports the write as `not-attempted` with reason `awaiting-acceptance`.
Holding `resolved` until after the move would reorder the stream. That
is change 8's rejected option, and the reason still holds.

Why not `library-admission`: that event is about the library's
stores. It is emitted only when the admission has something to say,
and runs outside a library emit none.

**Event order for an operator's accepted record:**

1. `resolved`, with `content_index.write` `{"status":
   "not-attempted", "reason": "awaiting-acceptance"}`;
2. `renamed`, recorded first, then emitted;
3. `library-admission`, if any;
4. `content-index-write`;
5. `sidecar`, if any.

The write is made where it is made today. Only its result is now
emitted. The event goes through `Sink::emit`, so it reaches the log and
the stream alike, best-effort, as every non-move event does.

**Counts and exit.** `Counts::observe` ignores the event, and it
changes no exit status. A `failed` write is evidence, not a failure
(change 8's "A failed cache write is evidence, not a failure").

**Human line.** `written` renders nothing, as an ordinary admission
renders nothing. `failed` renders
`<path>: the content index could not keep this answer (<message>)`,
escaped. That line names the store and not the rename, so the rename is
never reported as having failed.

**Rejected: emit only on failure.** Then an absent event would mean
either "written" or "the run ended before the write". A verdict that
said `awaiting-acceptance` would have no closing statement.

## D9. The human lines (change 3) and escaping

**The `resolved` line:**

```text
<path>: resolved <identifier>[ to <work>][ via <services>], from <where>[; the library could not answer: <what>]
```

- `<work>` is `"<title>"` followed by ` (<authors>, <year>)`.
  - The title is left out when the record has none, or only blank
    text.
  - The parenthesis holds whichever of authors and year exist, and is
    left out when neither does.
  - With neither title nor parenthesis, ` to <work>` is left out.
- `<authors>` uses family names. One author: `Smith`. Two: `Smith and
  Jones`. Three or more: `Smith et al.`
- `<year>` is `record.issued.year`.
- `<services>` is `services_of` (D6), joined with `, `, and ` via …` is
  left out when it is empty.
- `<where>` is one of `the library`, `the content index`, `the response
  cache` or `the network`.
- The library suffix is unchanged.

Examples:

```text
paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network
paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the content index
paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the library
hand.pdf: resolved doi:10.1000/xyz to "A Note" (Doe and Roe, 2020), from the library
```

`(cached)` and `(from the library)` go. Their meaning moves into the
`from` clause, which now also tells the response cache from the network.

**Resolution skip lines** (`<path>: skipped, <clause>`):

| Kind | Clause |
|---|---|
| `no-text-layer` | `no identifier found; the pages read hold no text` |
| `text-without-identifier` | `no identifier found in its metadata or the pages read` |
| `encrypted` | `encrypted, so no identifier could be read` |
| `unreadable` | `unreadable (<message>)` |
| `unresolvable`, attempted | `no source had a record for <lookup.identifier> (<service>: <answer>; …)` |
| `unresolvable`, `no-eligible-service` | `no configured service could be asked about <lookup.identifier>` |
| `conflict` | `<field> disagrees <N>% (file says <extracted>, record says <resolved>)` |

The first four reuse the `library-extraction` clauses, so `status
--identify` and a skip say the same thing about the same file. Each
`<answer>` is `SourceError`'s `Display` text (`not found`,
`unavailable: …`, `rate limited`, `malformed response: …`), unchanged
from schema 3's `Attempt::error`. The two `duplicate` clauses (`same
bytes already archived at …`, `same work already archived at …
(different file)`) and the non-resolution skip clauses are unchanged.

**Escaping.** `human_line` passes the whole text after `<path>: ` of
every `resolved`, `skipped` and `content-index-write` line through
`describe::escaped`. That is one place, as the `STATE.md` defect asks.
Borax's own wording contains no control characters, so escaping the
whole clause escapes exactly the values taken from records, files,
stores and services:

- the title, the author names and the identifier;
- a reader's, store's or service's message;
- the library problem's paths and parser messages;
- the conflict's two titles.

The path is printed as every other line prints it. Whether file names
should be escaped across all human output is not this change's
question.

**The defect.** "Human output other than the description passes
metadata through unescaped" is narrowed, not closed. The
metadata-conflict line it names is fixed here, along with every other
`resolved` and `skipped` line. Adoption's `unreadable` and `unwritten`
messages are not touched by this change, and they stay as the entry's
remainder. Fixing them is a small separate change, and keeping them out
keeps this change's human surface to the lines it rewrites.

## D10. The interactive description

**Decision.** `describe` keeps its signature (`&Event`) and reads the
sections. The changes, line by line:

- **`identifier`** comes from `lookup`: its `identifier`, with `from
  embedded metadata` or `from the text layer` from
  `extraction.result.tier` when `origin` is `extracted`, and `supplied`
  when it is `operator`. A lookup that was not attempted names the
  record's own identifier with no clause, as today.
- **`record`** names `services_of` (D6). It adds `from an earlier run`
  for a `content-index` retrieval and `from the library` for a
  `library` retrieval, or reads `the library` when no service is named.
  This is today's wording, now read from `record_retrieval` instead of
  `cached` and `tier`.
- **`file says`** gives the titles' state:

  | `extraction.titles` | Line |
  |---|---|
  | `read`, non-empty | each title with `(XMP)` or `(document info)`, as today |
  | `read`, `[]` | `no title in its metadata` |
  | `failed` | `could not be opened (<message>)` |
  | `not-attempted`, `content-index-hit` | `not read; an earlier run answered` |
  | `not-attempted`, `library-answered` | `not read; the library answered` |

- **`conflict`** is shown when `acceptance` is `overridden`, from
  `match_check`.
- **Failed verdicts:**

  | Reason kind | Lines after `file` |
  |---|---|
  | `no-text-layer` | `identifier  none found; the pages read hold no text`, then `file says` |
  | `text-without-identifier` | `identifier  none found in its metadata or the pages read`, then `file says` |
  | `encrypted` | `identifier  none read; the file is encrypted`, then `file says` |
  | `unreadable` | `unreadable  <message>`, then `file says` |
  | `unresolvable` | `identifier` (from `lookup`, as above), the `no record` block from `lookup.attempts` (or `no source was asked` for `no-eligible-service`), then `file says` |
  | `conflict` | `title` (`match_check.resolved`), `file says` (`match_check.extracted`), `conflict` — as today, read from `match_check` |

  The `library` line follows the reason lines, as today.

Showing `file says` on a failed verdict is new. The event now carries
the titles through a failed extraction, and they are what an operator
supplying an identifier has to go on, which is the round-two
observation the discussion turned into change 8's titles state. The
`cli` requirement "A question describes whichever verdict it is asking
about" still holds, because the description shows only what the
verdict's own event carries.

The conflict skip's description does not render the full `candidate`
record. That would change what the operator is shown about a conflict
beyond this change's brief, and a later change may decide it.

`Candidate::Unheld { attempts: &[ServiceAnswer] }` replaces
`&[Attempt]`. `answered` renders each answer with `SourceError`'s
wording (D9).

## D11. Engine surface changes

- **`Unattempted::CacheBypassed`** (`"cache-bypassed"`): the
  `stored` reason for `Retrieval::Network { stored: None }`. It gives
  the one not-attempted reason that change 8's vocabulary lacked a name
  for, so every reason in the stream still comes from
  `Unattempted::as_str`. No engine section holds it; only the
  projection produces it.
- **Removed, with no alias:**
  - `pipeline::event_for`, since a skip cannot be rendered from a bare
    `SkipReason`;
  - `pipeline::unresolvable`, since `SkipReason::Unresolvable` carries
    nothing;
  - `pipeline::sources_of`, which moves to `event::services_of`;
  - `Provenance`;
  - `FileRecord::source`, `tier`, `found`, `claims` and `cached`;
  - `event::Attempt` and `event::Overridden`.

  Change 8's D2 says the five methods existed "to return exactly what
  the field held". The fields are gone, and `services_of` reads the
  event rather than the `FileRecord`.
- **Retyped:**
  - `FileRecord::conflict()` returns `Option<&Conflict>`, the match
    check's;
  - `attempts_of` returns `Vec<ServiceAnswer>`;
  - `FileRecord::overrode` becomes `overridden: bool` (D7).
- **Added:**
  - `FileRecord::acceptance()`;
  - `pipeline::resolution_skip(path, reason, &Evidence, candidate:
    Option<&Record>) -> Event`, used by `verdict_event` and by the
    interactive driver's retry paths, which now build no
    `Event::Skipped` by hand;
  - `SkipReason::is_resolution_verdict()`.
- **`run.rs`:**
  - `Settled::Skip { file, reason, library }` becomes `{ file, event }`
    or `{ file, reason, sections, candidate }`. Either is the
    implementer's choice, as long as `skipped` replays the held
    verdict exactly.
  - `situation`'s arm `NoIdentifier | Unreadable` becomes the four
    extraction kinds. Its behaviour does not change: an encrypted file
    was `Unreadable` before, and it is still offered Supply.
  - `let _remembered` becomes the `content-index-write` emission (D8).

## D12. The interface the tests are written against

```rust
// crates/borax/src/event.rs
pub const SCHEMA: u32 = 4;

pub enum Event {
    // …
    Resolved {
        path: PathBuf,
        identifier: String,
        record: Box<Record>,
        /// Flattened: the seven section keys sit beside `record`.
        sections: Box<Sections>,
    },
    Skipped {
        path: PathBuf,
        reason: SkipReason,
        /// Flattened; `Some` exactly when `reason.is_resolution_verdict()`
        /// (the six D4 kinds and `Duplicate`).
        sections: Option<Box<Sections>>,
        /// `Some` exactly when `reason` is `SkipReason::Conflict`.
        candidate: Option<Box<Record>>,
    },
    /// After `renamed` (and any `library-admission`) for a record an
    /// operator reached. `path` is the file after the move.
    ContentIndexWrite { path: PathBuf, write: WriteStep },
    // … every other variant unchanged
}

// All below: Debug, Clone, PartialEq, Serialize, Deserialize.
// Enums tagged "status" unless noted; variants and unit values kebab-case.
pub struct Sections {
    pub library: LibraryStep,
    pub content_index: ContentIndexSection,
    pub extraction: ExtractionSection,
    pub lookup: LookupStep,
    pub record_retrieval: Option<RetrievedFrom>,   // null when None
    pub match_check: MatchCheckStep,
    pub acceptance: Acceptance,
}
pub enum LibraryStep { Consulted { answer: LibraryAnswer }, NotAttempted { reason: String } }
pub struct ContentIndexSection { pub read: IndexReadStep, pub write: WriteStep }
pub enum IndexReadStep {
    Hit, Miss, Bypassed, Unavailable { message: String }, NotAttempted { reason: String },
}
pub enum WriteStep { Written, Failed { message: String }, NotAttempted { reason: String } }
pub struct ExtractionSection { pub result: ExtractionResultStep, pub titles: TitlesStep }
pub enum ExtractionResultStep {
    Found { identifier: String, tier: String },
    NoTextLayer, TextWithoutIdentifier, Encrypted,
    Unreadable { message: String },
    NotAttempted { reason: String },
}
pub enum TitlesStep { Read { claims: Vec<Claim> }, Failed { message: String }, NotAttempted { reason: String } }
pub enum LookupStep {
    Attempted { identifier: String, origin: IdentifierOrigin, attempts: Vec<ServiceAnswer> },
    NoEligibleService { identifier: String, origin: IdentifierOrigin },
    NotAttempted { reason: String },
}
pub enum IdentifierOrigin { Extracted, Operator }          // plain kebab strings; Copy, Eq
pub struct ServiceAnswer { pub service: String, pub outcome: ServiceOutcome }
pub enum ServiceOutcome {
    Found { retrieval: FetchedFrom, stored: Option<WriteStep> },  // `stored` omitted when None
    NotFound,
    Unavailable { message: String },
    RateLimited,
    Malformed { message: String },
}
pub enum FetchedFrom { ServiceCache, Network }              // plain kebab strings; Copy, Eq
pub enum RetrievedFrom {                                    // tagged "kind"
    Library { artifact: String, item: String },
    ContentIndex,
    ServiceCache { service: String },
    Network { service: String },
}
pub enum MatchCheckStep {
    Agreed,
    Conflict { field: String, extracted: String, resolved: String, similarity: f64 },
    InsufficientEvidence { reason: String },               // record-untitled | no-titles | no-evidence
    NotAttempted { reason: String },
}
pub enum Acceptance { NotApplicable, Automatic, Overridden }  // Copy, Eq

pub enum SkipReason {                                       // tagged "kind", as today
    NoTextLayer, TextWithoutIdentifier, Encrypted, Unreadable { message: String },
    Unresolvable, Conflict,
    TargetTaken { target: PathBuf }, Unnameable, Declined, RenameFailed { message: String },
    BibWriteFailed { message: String }, Unciteable, SidecarTaken { target: PathBuf },
    Unrecordable { message: String }, Duplicate { reason: DuplicateReason, existing_path: PathBuf },
    Stranding { id: String },
}
impl SkipReason { pub fn is_resolution_verdict(&self) -> bool; }

/// The services that supplied `record`, as D6 names them.
pub(crate) fn services_of(record: &Record, retrieval: Option<&RetrievedFrom>) -> Vec<&'static str>;
// Removed: Attempt, Overridden.

// crates/borax/src/evidence.rs
impl Evidence {
    /// Every section, with `acceptance` as given; reasons via `Unattempted::as_str`.
    pub fn sections(&self, acceptance: Acceptance) -> Sections;
}
pub enum Unattempted { /* change 8's ten, */ CacheBypassed }

// crates/borax/src/pipeline.rs
pub struct FileRecord {
    pub record: Record,
    pub hash: Option<ContentHash>,
    pub evidence: Evidence,
    pub overridden: bool,
}
impl FileRecord {
    pub fn library(&self) -> Option<&LibraryAnswer>;          // unchanged
    pub fn conflict(&self) -> Option<&Conflict>;              // retyped
    pub fn acceptance(&self) -> Acceptance;                   // Overridden | Automatic
}
pub fn resolved_event(path: &Path, file: &FileRecord) -> Event;
pub fn verdict_event(path: &Path, standing: &Standing) -> Event;
pub fn resolution_skip(path: &Path, reason: SkipReason, evidence: &Evidence,
    candidate: Option<&Record>) -> Event;                     // reason must be a resolution kind
pub fn attempts_of(unresolved: &Unresolved) -> Vec<ServiceAnswer>;
pub fn accept(file: FileRecord) -> FileRecord;                // sets `overridden` on a Conflict
// Removed: event_for, unresolvable, sources_of, Provenance,
// FileRecord::{source, tier, found, claims, cached}.
```

The serde attributes are the implementer's: flatten, tags,
`skip_serializing_if`, and boxing. They must produce the JSON of D3 and
the examples below, and they must keep the round trip. A flattened
`Option<Box<Sections>>` that serializes nothing for `None` and
deserializes `None` when the keys are absent is the expected route.
Listing the seven fields in both variants is an acceptable alternative.
The Rust field names above are fixed, because tests construct events
with them.

`identifier_input` (change 10) will be a new `Sections` field. Tests
that build `Sections` literals will then need a compile-only edit.
Building them through `Evidence::sections` wherever a test is not
about the projection itself keeps that edit small.

## D13. How the specifications change

ADDED requirements state the schema-4 contract section by section:

- `resolution`: eight requirements;
- `extraction`: one requirement, for the extraction section and the
  failure kinds on a skip;
- `rename`: one requirement, for the description's title states.

Each requirement's first line carries its SHALL.

MODIFIED requirements are the living ones whose text names a removed
field, a schema-3 reason, or "schema 3". Each is restated whole, with
every scenario name kept:

- `resolution`:
  - "A resolution reports the evidence it was checked against";
  - "A resolution says whether its library answered";
  - "A file its library tracks resolves from its library item";
  - "An operator can supply an identifier";
- `cli` "A question describes whichever verdict it is asking about";
- `rename`:
  - "An interactive run asks about files it could not settle";
  - "An interactive question says whether the library answered".

Where a MODIFIED requirement deliberately drops schema-3 prose, a
`<!-- drops: … -->` marker sits directly above it, as
`scripts/check-spec-deltas.py` expects. Those prose drops are:

- the `cache` and `library` source stand-ins;
- "These are additions … version SHALL NOT change";
- the `unresolvable` reason's schema-3 additions.

The search for the removed field names over `openspec/specs/` found
references only in these requirements. It searched for `found`,
`cached`, `source`, `tier`, `claims`, `overrode`, "schema 3", "schema
version" and `no-identifier`. `library/spec.md`'s two "does not change
the event schema version" sentences describe those changes' own
additions and stay true. `run-logs` names no event field.

## D14. Existing tests: what changes and how

This change alters output, so tests that assert schema-3 output have to
change. The test stage rewrites their expectations deliberately, in the
red batch, under three rules:

1. **Same fact, new address.** Each rewritten assertion must state the
   schema-4 form of the fact the old assertion stated, using the
   mapping below. An assertion is never dropped because its field went.
2. **Deletions are listed.** A test is deleted only when the behaviour
   it guarded no longer exists. The cases are listed under (c), and
   every other test survives.
3. **Shape-only edits change nothing.** An edit that only reshapes a
   literal or a pattern (kind (a)) must not change what the test
   expects.

**Mapping, schema 3 → schema 4:**

| Schema-3 assertion | Schema-4 assertion |
|---|---|
| `schema == 3` | `schema == 4` (`SCHEMA`) |
| `found == X` after a lookup | `lookup.identifier == X` |
| `found` on an index or library answer (the record's own identifier) | `lookup == {status: not-attempted, reason: content-index-hit / library-answered}` and top-level `identifier == X` |
| `tier == "embedded-metadata"` / `"text-layer"` | `extraction.result.tier == …` and `lookup.origin == "extracted"` |
| `tier == "supplied"` | `lookup.origin == "operator"` |
| `tier == "library"` | `record_retrieval.kind == "library"` |
| `tier == null` | `record_retrieval.kind == "content-index"` |
| `cached == true` | `record_retrieval.kind == "content-index"` and `content_index.read.status == "hit"` |
| `cached == false` | `record_retrieval.kind` equal to the actual place (`network`, `service-cache` or `library`) |
| `source == S` where a service answered | `record_retrieval.service == S` |
| `source == S` on an index or library answer | the human line's `via S`; or `record.borax.provenance` |
| `source == "cache"` / `"library"` | `record_retrieval.kind`, and no `via` in the human line |
| `claims == [..]` | `extraction.titles == {status: read, claims: [..]}` |
| `claims == []` because the file was not opened | `extraction.titles.status == "not-attempted"` with its reason |
| `overrode == {..}` | `match_check == {status: conflict, ..}` and `acceptance.status == "overridden"` |
| `overrode == null` | `acceptance.status == "automatic"` |
| `library == null` on a resolution event | `library == {status: not-attempted, reason: no-library / outside-library}` |
| `library == {kind: ..}` | `library == {status: consulted, answer: {kind: ..}}` |
| `library == null` on a non-verdict skip | the `library` key is absent |
| `library` on a `duplicate` skip | `library` section: `{status: not-attempted, reason: content-duplicate}` for content, the `consulted` answer or `not-attempted` reason for work |
| reason `no-identifier` | `no-text-layer` or `text-without-identifier`, whichever the fixture is; `status --identify` on the same fixture says which |
| reason `unreadable` for an encrypted file | `encrypted` |
| reason `unresolvable {found, tier, attempts}` | reason `{kind: unresolvable}`, plus `lookup` and `extraction.result.tier` |
| reason `conflict {field, ..}` | reason `{kind: conflict}`, plus `match_check` and `candidate` |
| `Attempt {source, error}` | `ServiceAnswer {service, outcome}` |
| human `… via crossref (cached)` | `… via crossref, from the content index`, with the work |
| human `… (from the library)` | `…, from the library` |
| description `file says  nothing read` | the D10 state line |

**Files and the kinds of edit.** Kind (a) is a shape-only edit. Kind
(b) is a deliberate expectation rewrite following the mapping. Kind (c)
is a deletion or replacement.

- `crates/borax/tests/event.rs`:
  - (a) `Event::Resolved` and `Event::Skipped` literals, `Attempt` and
    `Overridden` imports and literals, `event_for` calls (to
    `resolved_event`), and the round-trip fixture list;
  - (b) the `SCHEMA` and `"schema":3` literals, including the
    `library-condition` and `library-adoption` JSON strings near lines
    2263–2304; the `human_line` exact strings for `resolved` and
    `skipped`; the JSON shape assertions on `found`, `cached`, `tier`,
    `claims`, `overrode`, `source`, `library` and the skip reasons;
  - (c) delete `a_resolved_line_with_no_library_key_deserializes_with_library_none`
    and `a_skipped_line_with_no_library_key_deserializes_with_library_none`,
    because schema 4 does not read schema-3 lines (pre-1.0 rule).
- `crates/borax/tests/pipeline.rs`:
  - (a) `overrode` becomes `overridden`; `.tier()`, `.cached()`,
    `.source()`, `.found()` and `.claims()` become evidence reads;
    `event_for` becomes `verdict_event` or `resolved_event`;
    `file.conflict()` patterns change;
  - (b) `SkipReason::NoIdentifier`, `Unresolvable {..}` and
    `Conflict {..}` outcomes;
  - (c) the three `resolve_file_still_skips_…` regression-guard tests
    (near line 5376) are replaced by tests asserting `NoTextLayer`,
    `TextWithoutIdentifier` and `Encrypted`. Their comment block goes
    with them.
- `crates/borax/tests/dispatch.rs`:
  - (a) the `Event::Skipped { …, library }` patterns (about 110 uses of
    `library:`) and the `Event::Resolved` destructuring;
  - (b) human output containing `(cached)`, `via` or `no identifier
    found`; JSON `tier`, `cached` and `library`; the `SCHEMA` and
    "schema 3" checks near lines 13857–14021; `no-identifier`.
- `crates/borax/tests/describe.rs`:
  - (a) `Event` literals, `Attempt`, `Overridden`;
  - (b) the expected description lines for `file says`, the failed
    verdicts and the `record` line.
- `crates/borax/tests/end_to_end.rs`:
  - (b) `BATCH`'s `Skipped("no-identifier")` for `no-identifier.pdf`
    becomes `text-without-identifier` (the same file's
    `library-extraction` result at line ~1243). The entry for
    `doi-past-page-range.pdf` follows that fixture's `status
    --identify` result. `encrypted-user-password.pdf` becomes
    `encrypted`. The JSON `tier`, `source`, `found` and `cached`
    assertions near lines 450–457 and 1161 also change.
- `crates/borax/tests/binary.rs`: (b) `resolved["tier"] == "library"`
  and `resolved["library"]["kind"]` (near line 471).
- `crates/borax/tests/renaming.rs`, `bib.rs` and `per_file.rs`: (a)
  `FileRecord` literals, and `Event::Skipped` literals drop `library`;
  (b) any `NoIdentifier`.
- `crates/borax/tests/library.rs`, `runlog.rs` and `run.rs`: (a) or (b)
  where they read the removed fields. The `found:` hits in
  `library.rs` are local variables and need nothing.

Nothing in `crates/borax-sources/tests/` changes. That crate's types
are untouched.

## Not changed, deliberately

- The engine's resolution:
  - which steps run;
  - which services are asked, and in what order;
  - what is cached, under which key, and when it is written;
  - what `--no-cache` bypasses;
  - which record is refused;
  - every `Evidence` value change 8 produces.
- `run-finished`, `Counts`, `Command::summary` and exit status.
- Every event other than `resolved`, `skipped` and the new
  `content-index-write`.
- Non-resolution skip reasons, the `duplicate` reason's fields, and the
  human clauses of both, apart from the escaping.
- The interactive menus, their order, and what each answer does.

## Risks / Trade-offs

- **A breaking stream change before any consumer exists.**
  `openspec/STATE.md` notes that no external consumer has built
  against the stream yet. That makes this the cheapest time to
  reshape it, and it also means the shape is untested by use. The
  sections are designed so that change 10 can only add to them.
- **Test churn.** `dispatch.rs` alone has about 110 `library:` pattern
  sites. D14's mapping keeps the rewrite mechanical where it can be,
  and keeps it auditable where it is not (task 9.1).
- **The interactive human paths** are the thinnest-covered part of the
  suite (`STATE.md`). D10 changes the description, and D8 adds a
  line. Groups 6 and 7 test both through the dispatch harness, not
  only through `describe`.
- **Flatten and internally tagged enums.** serde supports flatten
  inside internally tagged variants, but it buffers the input. The
  round-trip test guards deserialization. If `f64` similarity or nested
  tags fail to round-trip through the buffer, the fallback in D12
  (seven explicit fields) applies.

## Illustrative schema-4 lines

These are pretty-printed. In the stream, each is one line. Record
bodies are abbreviated with `…`, and identities are shortened.

**A content-index hit** (no library):

```json
{"schema":4,"event":"resolved","path":"papers/smith.pdf",
 "identifier":"doi:10.1039/c5ay00042d",
 "record":{"type":"article-journal","title":"Determination of things","author":[{"family":"Smith","given":"Ann"}],"issued":{"date-parts":[[2015]]},"DOI":"10.1039/c5ay00042d","borax":{"provenance":{"DOI":"crossref","author":"crossref","issued":"crossref","title":"crossref"}}},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"hit"},"write":{"status":"not-attempted","reason":"content-index-hit"}},
 "extraction":{"result":{"status":"not-attempted","reason":"content-index-hit"},
               "titles":{"status":"not-attempted","reason":"content-index-hit"}},
 "lookup":{"status":"not-attempted","reason":"content-index-hit"},
 "record_retrieval":{"kind":"content-index"},
 "match_check":{"status":"not-attempted","reason":"content-index-hit"},
 "acceptance":{"status":"automatic"}}
```

Human: `papers/smith.pdf: resolved doi:10.1039/c5ay00042d to
"Determination of things" (Smith, 2015) via crossref, from the content
index`

**A fresh network resolution after a failed attempt** (inside a
library that does not track the file):

```json
{"schema":4,"event":"resolved","path":"papers/smith.pdf",
 "identifier":"doi:10.1039/c5ay00042d","record":{…,"borax":{"provenance":{…:"openalex"}}},
 "library":{"status":"consulted","answer":{"kind":"untracked"}},
 "content_index":{"read":{"status":"miss"},"write":{"status":"written"}},
 "extraction":{"result":{"status":"found","identifier":"doi:10.1039/c5ay00042d","tier":"text-layer"},
               "titles":{"status":"read","claims":[{"from":"info","title":"Determination of things"}]}},
 "lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"extracted",
           "attempts":[
             {"service":"crossref","outcome":{"status":"unavailable","message":"HTTP 503"}},
             {"service":"openalex","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}]},
 "record_retrieval":{"kind":"network","service":"openalex"},
 "match_check":{"status":"agreed"},
 "acceptance":{"status":"automatic"}}
```

**A service-cache answer** (a second file citing the same DOI; the file
claims no title):

```json
{"schema":4,"event":"resolved","path":"papers/copy-of-smith.pdf",
 "identifier":"doi:10.1039/c5ay00042d","record":{…},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"written"}},
 "extraction":{"result":{"status":"found","identifier":"doi:10.1039/c5ay00042d","tier":"embedded-metadata"},
               "titles":{"status":"read","claims":[]}},
 "lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"extracted",
           "attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"service-cache"}}]},
 "record_retrieval":{"kind":"service-cache","service":"crossref"},
 "match_check":{"status":"insufficient-evidence","reason":"no-titles"},
 "acceptance":{"status":"automatic"}}
```

The same file under `--no-cache` would show `content_index.read`
`{"status":"bypassed"}`. Its found attempt would read
`{"status":"found","retrieval":"network","stored":{"status":"not-attempted","reason":"cache-bypassed"}}`.

**A library answer:**

```json
{"schema":4,"event":"resolved","path":"lib/smith2015.pdf",
 "identifier":"doi:10.1039/c5ay00042d","record":{…},
 "library":{"status":"consulted","answer":{"kind":"tracked","artifact":"0190c3a2-…","item":"0190c3a3-…"}},
 "content_index":{"read":{"status":"not-attempted","reason":"library-answered"},
                  "write":{"status":"not-attempted","reason":"library-answered"}},
 "extraction":{"result":{"status":"not-attempted","reason":"library-answered"},
               "titles":{"status":"not-attempted","reason":"library-answered"}},
 "lookup":{"status":"not-attempted","reason":"library-answered"},
 "record_retrieval":{"kind":"library","artifact":"0190c3a2-…","item":"0190c3a3-…"},
 "match_check":{"status":"not-attempted","reason":"library-answered"},
 "acceptance":{"status":"automatic"}}
```

**A no-text-layer skip with titles** (a blank scan whose document info
carries a title):

```json
{"schema":4,"event":"skipped","path":"scan.pdf","reason":{"kind":"no-text-layer"},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"extraction-failed"}},
 "extraction":{"result":{"status":"no-text-layer"},
               "titles":{"status":"read","claims":[{"from":"info","title":"A Title"}]}},
 "lookup":{"status":"not-attempted","reason":"extraction-failed"},
 "record_retrieval":null,
 "match_check":{"status":"not-attempted","reason":"extraction-failed"},
 "acceptance":{"status":"not-applicable"}}
```

Human: `scan.pdf: skipped, no identifier found; the pages read hold no
text`

**An unresolvable skip:**

```json
{"schema":4,"event":"skipped","path":"manuscript.pdf","reason":{"kind":"unresolvable"},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"no-record"}},
 "extraction":{"result":{"status":"found","identifier":"doi:10.1021/jacs.4c01234.author","tier":"text-layer"},
               "titles":{"status":"read","claims":[]}},
 "lookup":{"status":"attempted","identifier":"doi:10.1021/jacs.4c01234.author","origin":"extracted",
           "attempts":[{"service":"crossref","outcome":{"status":"not-found"}},
                       {"service":"openalex","outcome":{"status":"not-found"}}]},
 "record_retrieval":null,
 "match_check":{"status":"not-attempted","reason":"no-record"},
 "acceptance":{"status":"not-applicable"}}
```

Human: `manuscript.pdf: skipped, no source had a record for
doi:10.1021/jacs.4c01234.author (crossref: not found; openalex: not
found)`

The same verdict for an arXiv identifier under Crossref and OpenAlex
alone carries `"lookup":{"status":"no-eligible-service","identifier":"arXiv:2401.12345","origin":"extracted"}`.
Its human clause is `no configured service could be asked about
arXiv:2401.12345`.

**A content-duplicate skip** (a rename run inside a library that
already holds these bytes):

```json
{"schema":4,"event":"skipped","path":"inbox/smith-copy.pdf",
 "reason":{"kind":"duplicate","reason":"content","existing_path":"/lib/smith2015.pdf"},
 "library":{"status":"not-attempted","reason":"content-duplicate"},
 "content_index":{"read":{"status":"not-attempted","reason":"content-duplicate"},
                  "write":{"status":"not-attempted","reason":"content-duplicate"}},
 "extraction":{"result":{"status":"not-attempted","reason":"content-duplicate"},
               "titles":{"status":"not-attempted","reason":"content-duplicate"}},
 "lookup":{"status":"not-attempted","reason":"content-duplicate"},
 "record_retrieval":null,
 "match_check":{"status":"not-attempted","reason":"content-duplicate"},
 "acceptance":{"status":"not-applicable"}}
```

**A work-duplicate skip** (a batch rename reaching a scan of a paper
whose published PDF the library already holds; the library does not
track the scan, and Crossref's answer was cached by an earlier file):

```json
{"schema":4,"event":"skipped","path":"inbox/smith-scan.pdf",
 "reason":{"kind":"duplicate","reason":"work","existing_path":"/lib/smith2015.pdf"},
 "library":{"status":"consulted","answer":{"kind":"untracked"}},
 "content_index":{"read":{"status":"miss"},"write":{"status":"written"}},
 "extraction":{"result":{"status":"found","identifier":"doi:10.1039/c5ay00042d","tier":"text-layer"},
               "titles":{"status":"read","claims":[]}},
 "lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"extracted",
           "attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"service-cache"}}]},
 "record_retrieval":{"kind":"service-cache","service":"crossref"},
 "match_check":{"status":"insufficient-evidence","reason":"no-titles"},
 "acceptance":{"status":"not-applicable"}}
```

Human (unchanged): `inbox/smith-scan.pdf: skipped, same work already
archived at /lib/smith2015.pdf (different file)`

The `content_index.write` is `written` because `beyond_library` writes
the record under the file's hash before the work check runs, as it
does today. This change does not alter that.

**A conflict skip with its candidate:**

```json
{"schema":4,"event":"skipped","path":"preprint.pdf","reason":{"kind":"conflict"},
 "library":{"status":"not-attempted","reason":"no-library"},
 "content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"refused"}},
 "extraction":{"result":{"status":"found","identifier":"doi:10.1000/ref","tier":"text-layer"},
               "titles":{"status":"read","claims":[{"from":"xmp","title":"Graphene on copper"}]}},
 "lookup":{"status":"attempted","identifier":"doi:10.1000/ref","origin":"extracted",
           "attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"network","stored":{"status":"written"}}}]},
 "record_retrieval":{"kind":"network","service":"crossref"},
 "match_check":{"status":"conflict","field":"title","extracted":"Graphene on copper",
                "resolved":"A survey of something else","similarity":0.08},
 "acceptance":{"status":"not-applicable"},
 "candidate":{"type":"article-journal","title":"A survey of something else",…,"DOI":"10.1000/ref"}}
```

Human: `preprint.pdf: skipped, title disagrees 8% (file says Graphene on
copper, record says A survey of something else)`

**The same file, accepted by its operator and moved:**

```json
{"schema":4,"event":"resolved","path":"preprint.pdf","identifier":"doi:10.1000/ref","record":{…},
 …,"content_index":{"read":{"status":"miss"},"write":{"status":"not-attempted","reason":"awaiting-acceptance"}},
 …,"match_check":{"status":"conflict","field":"title",…,"similarity":0.08},
 "acceptance":{"status":"overridden"}}
{"schema":4,"event":"renamed","path":"preprint.pdf","target":"doe2020_ASurvey.pdf","hash":"sha256-…"}
{"schema":4,"event":"content-index-write","path":"doe2020_ASurvey.pdf","write":{"status":"written"}}
```
