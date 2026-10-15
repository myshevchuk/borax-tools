# Design: reduce-content-index

## Context

Everything below was read from source at `da6706d`, the merge of
`library-identity`. This is change 13a of the roadmap's Phase 4: the
first half of change 13. The maintainer split it after the critic gate.

- **13a (this change):**
  - the content index moves to the data directory;
  - it holds identification evidence only;
  - history entries carry evidence;
  - outside answers look the evidence up;
  - offline `adopt` reads the response cache;
  - `cache --clear` stops touching the index;
  - the event schema becomes 5.
- **13b (`index-references-libraries`, later):** library references —
  reading, agreement, writing, restoration, root repair — and `adopt`
  taking a record through a reference.

Changes 15 (`separate-change-from-acceptance`) and 12
(`library-identity`) are done, and change 14 (`refresh-as-candidate`)
follows 13b.

**Settled by the maintainer** (the Phase 4 library-authority design
record, maintainer's notes, outside the repository):

1. No operator decision lives only in a disposable cache. Inside a
   library, each artifact history entry records the identification
   evidence of its accepted bytes, the identifier and its origin, beside
   the item it was accepted under.
2. The reduced content index moves out of the cache root into the XDG
   data directory. Afterwards `borax cache --clear` removes service
   responses only.
3. Choosing among several library references (13b).
4. No migration of `v1` entries, and no reader or importer for them.
   The reduced format may be called `v2` on disk.
5. Refresh is change 14's.

**Settled by the maintainer for this change, after the critic gate:**

- **One fact per file, no read-merge-write.** An entry is a directory.
  Each fact is its own file, and only its own writer writes it, by
  atomic replacement. No writer reads, merges and rewrites another
  writer's fact. 13b adds one file per reference under `refs/` on the
  same terms.
- **The append rule** for an admission whose decision carries evidence
  (D3).
- **Recovery** converges on the library's evidence, and `reconcile` is
  the command that restores it (D5).
- **Schema 5, whole, here.** 13b may only add to schema 5, which the
  `cli` rule allows while no release carries 5.

**Decided by the orchestrator earlier and kept:** the paths (D1); the
storage seam with an in-memory implementation (D2); offline adoption
through the response cache, never the network (D7); evidence answers
without a title check (D6); schema 5 applied by the `cli` rule (D8).

What the engine does today, at the points this change touches:

- **Where the index lives.** `borax_sources::store::cache_root`
  (`crates/borax-sources/src/store.rs`, line 43) tries `XDG_CACHE_HOME`
  then `HOME/.cache` on Unix, and `LOCALAPPDATA` then `XDG_CACHE_HOME`
  on Windows (`CANDIDATES`, lines 64 and 69). It appends
  `borax/<FORMAT_VERSION>` (`"v1"`, line 27). `run::execute` opens the
  index as `ContentIndex::new(response_cache())` (`run.rs`, line 4035):
  the same kind of `FileCache` over the same root as the response cache
  (`run::polite`, line 4132), or a `MemoryCache` when no root qualifies
  (`response_cache`, near line 4146). That case is not reported.
- **What the index holds.** `ContentIndex<C: Cache>` (`store.rs`, line
  362) stores a whole `Record` under `content_key(hash)` (line 120), at
  `<root>/content/sha256-<hex>.json`. `Cache`
  (`crates/borax-sources/src/cache.rs`, line 81) is typed to `Record`.
- **`cache --clear`** is `cache::clear` → `FileCache::clear`
  (`store.rs`, line 157): `remove_dir_all` of the whole cache root,
  index included.
- **Reading and writing the index.**
  - `pipeline::index_read` (`crates/borax/src/pipeline.rs`, line 178)
    returns the record on a hit.
  - `beyond_library` (line 645) turns a hit into `indexed_record` (line
    208), so `record_retrieval` is `content-index` (`evidence.rs`,
    `Evidence::retrieval`, line 173).
  - The resolved record is written at line 716 unless refused, whatever
    `--no-cache` says.
  - `pipeline::accept` (line 943) and `pipeline::remember` (line 968)
    hold back and then write the operator's record at the move, from
    the rename driver (`run.rs`, lines 1686–1699), reported as
    `Event::ContentIndexWrite` (`event.rs`, line 250).
- **Adoption.** `library::adopt` (`crates/borax/src/library.rs`, line
  3136) takes `lookup: &dyn Fn(&ContentHash) -> Option<Record>`, which
  `run::adopt_events` (`run.rs`, line 3963) fills with
  `adapters.index.get(hash)`. An unanswered orphan is
  `Adoption::Unindexed` (`event.rs`, line 796).
- **History entries.** `HashEntry` (`crates/borax-core/src/library.rs`,
  line 440) holds `hash`, `run`, `timestamp`, `tool_version` and
  change 15's optional `item`. `history_table` (`library.rs`, line 2042)
  writes `item` only when present.
- **When an admission appends.** `settle_history` (line 1439) returns
  without appending when the admitted bytes are already held and either
  nothing was decided or the bytes are the newest entry's and answer
  with the resulting link. Two tests pin that a re-identification of the
  newest bytes appends nothing:
  - `admit_reidentifying_the_newest_bytes_moves_the_link_and_appends_nothing`
    (`crates/borax/tests/library.rs`, line 4912);
  - `transition_f_relinking_the_newest_bytes_appends_nothing_until_later_bytes_restate_it`
    (line 5886).
- **Reconcile's appended entry** is built in step 4 for an accepted
  change (near line 2531).
- **The schema.** `event::SCHEMA` is 4 (`event.rs`, line 29), first
  released in 0.9.0.
- **Retry.** The interactive driver's `Answer::Retry` (`run.rs`, near
  line 2247) asks again only for `lookup.extracted()`.
- **A defect found on the way, not fixed here.** On Windows the
  run-log state root (`runlog.rs`, line 79) and the cache root are both
  `LOCALAPPDATA\borax\v1`, so `cache --clear` there removes run logs
  of runs outside a library. It bears on D1 and is in the proposal's
  Deferred.

## D1. Where the index lives

**Decision.**

- `borax_sources::store::data_root(lookup)`, mirroring `cache_root`:
  - Unix: `XDG_DATA_HOME`, then `HOME` with `.local/share`;
  - Windows: `APPDATA`, then `XDG_DATA_HOME`.

  An unset, empty or relative variable is skipped. Then `borax` and
  `INDEX_FORMAT_VERSION = "v2"` are appended. The root is not created
  or checked. `default_data_root()` applies it to the process
  environment.
- No data root: `run::execute` opens the index over a `MemoryCache`, as
  today with no cache root. Nothing is reported.
- The response cache, its root, `v1` and `FileCache::clear` are
  unchanged. Old `v1` entries under its `content/` are read by nothing,
  counted by `borax cache` as the files they are, and removed by
  `cache --clear` with the rest.
- **Why `APPDATA` on Windows.** Its base differs from the cache root's
  (`LOCALAPPDATA`), so no `remove_dir_all` of the cache root can reach
  the index whatever version segments later carry. The state root's
  collision with the cache root (Context) is what a version-segment
  separation costs. It is also the counterpart of `XDG_DATA_HOME` in
  the usual mapping, and borax's configuration directory already uses
  it (`config.rs`, line 1088). Operator identifiers are the user's, so
  roaming them is the right default.

**Rejected:**

- `LOCALAPPDATA` (`borax\v2` beside `borax\v1`): separated only by the
  version segment.
- Keeping the index under the cache root and teaching `cache --clear`
  to spare `content/`: settled otherwise.
- The XDG state directory: state is for data "not important or portable
  enough for the data directory", and operator decisions are the
  opposite.
- Reporting a run with no data root: today's run with no cache root is
  not reported, and every binary test runs with `env_clear`.

## D2. An entry is a directory, one fact per file

**Decision.** A file's entry is the directory
`<data root>/content/<hash>/`, where `<hash>` is `sha256-<hex>`:

| File | Holds | Written only by |
|---|---|---|
| `extracted.json` | `{"identifier":"doi:…"}` | a resolution that reached a record by extraction and a lookup, unrefused, and `remember` at the move after a successful retry of that lookup (D4) |
| `operator.json` | `{"identifier":"doi:…","origin":"supplied"}` or `"overridden"` | `remember` at the move (D4), and `reconcile`'s restoration (D5) |
| `refs/` | reserved for 13b's library references, one file each | nothing in this change |

- **Each writer replaces its own file whole**, through
  `write_atomically`, which creates the directory. It reads no other
  fact to decide what to write. Nothing reads, merges and rewrites a
  file another writer writes. Two writers of different facts never
  meet. Two operator decisions about the same bytes leave the one
  written last, and that is the intended meaning.
- **Reading an entry** reads each named file. Any other name in the
  directory, such as a `.tmp-*` an interrupted write left, is not a
  fact. Each fact is `Absent`, `Held` or `Unreadable` (cannot be read,
  or does not parse).
- **What it answers with** (`IndexEntry::answer`, D10):

  | `operator.json` | `extracted.json` | Answer |
  |---|---|---|
  | `Held(e)` | any | `e` |
  | `Unreadable` | any | none |
  | `Absent` | `Held(id)` | `{id, extracted}` |
  | `Absent` | `Absent` or `Unreadable` | none |

  No answer reads as a miss, reported `miss` as an unreadable entry is
  reported today. An unreadable `operator.json` withholds the extracted
  identifier because that is exactly the identifier an operator may
  have corrected. The file is then extracted and looked up, which
  rewrites `extracted.json`, and the next operator decision rewrites
  `operator.json`. Inside a library, the next `reconcile` rewrites it
  from the history (D5).
- **Spelling.**
  - `identifier` is `Identifier`'s `Display` text (`doi:…`, `arXiv:…`,
    `pmid:…`, `isbn:…`), read back with `identifier::supplied`.
  - `origin` in `operator.json` must be `supplied` or `overridden`, and
    `extracted.json` has none: its file name is its origin.
  - Unknown keys are ignored.
- **No record** is kept in any file.

**The storage seam.**

- A new trait, `IndexStore`, reads and writes fact text by key
  (`content/<hash>/extracted`, `content/<hash>/operator`):
  - `FileCache` implements it over its root through `entry_path`;
  - `MemoryCache` implements it with a second map beside its records.
- `ContentIndex<S>` keeps its type parameter, with the new API in an
  `impl<S: IndexStore>` block (D10).
- So `ContentIndex::new(MemoryCache::new())` (399 test sites) and
  `index: &index` (295) compile unchanged.
- Each custom test store that implements only `Cache` gains a small
  `IndexStore` impl (D14).

**Rejected:**

- **One JSON entry per hash, updated by read-merge-write.** The critic
  traced it losing an operator's evidence to a concurrent extraction
  writer holding an earlier read. With no lock (none may be a
  precondition), only separate files make that impossible.
- **The "never replace operator evidence" rule with a new
  `operator-evidence` reason.** Precedence at read time does the same
  job without a writer having to read first. The reason is gone.
- **A `MemoryIndex` type, or a generic `MemoryCache<V>`.** Either would
  cost 399 test edits, or inference from a single impl, for nothing a
  test asserts.
- **An `adopted.json`.** See D7.

## D3. Identification evidence, on history entries and in the index

**Decision.** `IdentificationEvidence { identifier, origin }` and
`EvidenceOrigin`:

| Word | Meaning | Where |
|---|---|---|
| `extracted` | an extraction pass read it from these bytes | history, `extracted.json` |
| `supplied` | an operator supplied it for these bytes | history, `operator.json` |
| `overridden` | an operator accepted its record over a title conflict | history, `operator.json` |
| `adopted` | `borax adopt` recorded these bytes on it | history only |

`supplied` and `overridden` are the operator origins. The word is
`overridden`, not `accepted`, because `acceptance` `accepted` already
names the supplied-without-conflict case. An operator who supplied an
identifier and renamed over its conflict gives `overridden`.

**On a history entry.** `HashEntry` gains `evidence:
Option<IdentificationEvidence>`, written by `history_table` after
`item` as `identifier` and `origin`, only when present:

- both or neither: one without the other, an `identifier` that does
  not parse, or an `origin` outside the four makes the record a file
  that does not parse, as a non-canonical `item` does;
- an entry written before this change has neither, reads unchanged,
  and is never rewritten;
- `ArtifactRecord::evidence_for(hash)` is the newest entry holding
  `hash` with evidence;
- `ArtifactRecord::operator_evidence_for(hash)` is the newest entry
  holding `hash` whose evidence has an operator origin (D5).

**Which evidence an admission records** (`FileRecord::identification()`):

| The record was reached by | Evidence |
|---|---|
| the file's own lookup, origin extracted, retried or not | `{lookup id, extracted}` |
| an operator's lookup, acceptance `accepted` | `{supplied id, supplied}` |
| any lookup, acceptance `overridden` | `{lookup id, overridden}` |
| a lookup of the index's identifier (D6), or a retry of one | `{that id, the answering fact's origin}` |
| the run's library (tracked) | none |

Other writers:

| Write | Evidence |
|---|---|
| `reconcile` step 3, accepted on the extracted identifier | `{id, extracted}` |
| `reconcile --accept` | none: it accepts bytes as the artifact, not an identifier |
| a restatement (change 15's Rule 2) | none: the earlier entry for those bytes keeps its own |
| `adopt` | `{the identifier its record was found by, adopted}` (D7) |

**The append rule (critic finding 1, settled).**

The defect: change 15's Rule 1 appends `(H, I)` for a decision unless
`H` is already the newest entry's hash and answers with `I` after the
admission. A re-identification of the newest bytes therefore moves the
link and appends nothing, so the operator's identifier would reach the
library nowhere.

The rule: an admission whose decision carries evidence (an operator's
`supplied` or `overridden`, or extraction at a mint) appends `(H, I,
evidence)` unless all three hold:

- `H` is the newest entry's hash;
- `H` answers with `I` after the admission;
- that newest entry already carries the same evidence.

A decision carrying no evidence keeps change 15's rule. An ordinary
re-run decides nothing and appends nothing, as before.

What this does and does not change:

- The history still grows by at most one entry per operator
  re-identification, plus at most one restatement.
- For bytes that are not newest, change 15 already appends whatever the
  evidence. That is what keeps sequence (a) of its D6 right: a restored
  version re-identified moves the link, and the newer bytes keep their
  own item.
- `settle_history` gains one condition, the evidence comparison on the
  newest entry.

Transitions (`A(X, e)` is bytes A under item X with evidence e; `s(y)`
is `{y, supplied}`):

| Sequence | History afterwards |
|---|---|
| `[A(X, x)]`, link X; re-identify A as Y from `s(y)` | `[A(X, x), A(Y, s(y))]`, link Y |
| then the same `s(y)` again | unchanged (newest A, answers Y, carries `s(y)`) |
| then edited B accepted under Y (transition f continued) | `[A(X, x), A(Y, s(y)), B(Y, …)]`, with no restatement because the newest entry already names Y. Same length as change 15's result, with A's operator evidence where change 15 had a restatement |
| `[A(X, x)]`, link X; re-identify A as X from `s(x2)` | `[A(X, x), A(X, s(x2))]`: the item was right, but the operator's identifier is new |

**Rejected: writing the operator's evidence on the newest entry in
place.** History is append-only. `append_history` never rewrites an
entry, and the interrupted-write guarantee assumes so.

**Rejected: an `adopted` fact in the index.** See D7.

## D4. When the index's evidence is written

**Decision.**

- **`extracted.json`:** at resolution, where today's record is written
  (`pipeline.rs`, line 716), with the same not-attempted reasons:
  library, index or duplicate settled; extraction failed; no record;
  refused; awaiting acceptance; unhashable. It is written whatever
  `operator.json` holds, which it never touches. Under `--no-cache` it
  replaces the earlier extracted identifier, as the record is replaced
  today. A batch `resolve` outside any library therefore keeps writing
  evidence for every file it resolves.
- **`operator.json`:** by `remember` at the move, for an operator's
  record (supplied, or accepted over a conflict), on today's terms:
  only once the file moved, never for an abandoned candidate, inside or
  outside a library, under `--no-record` too. Inside a library it is
  written after the admission's item and record, so the library holds
  the decision first. Also by `reconcile` (D5).
- **`remember` writes through the writer for the evidence's own
  origin** (settled after the second critic pass). The rename driver
  remembers whenever the record on offer is not the file's first
  resolution (`remember: !on.kept`, `run.rs` near line 2176), and a
  successful retry of the file's own lookup is marked `kept: false`
  (near line 2280). So `remember` meets three kinds of evidence:

  | `file.identification()` | Written by `remember` |
  |---|---|
  | `{id, supplied}` or `{id, overridden}` | `put_operator` → `operator.json` |
  | `{id, extracted}`: a successful retry of the file's own extracted lookup | `put_extracted` → `extracted.json`, at the move, reported by `content-index-write` as any write is. No `operator.json` |
  | `{id, origin}` from `Origin::Indexed`: a retry of an index-held lookup (D6) | nothing: the index already holds that evidence. `NotAttempted(ContentIndexHit)`, and no event |
  | none (no hash) | nothing: `NotAttempted(Unhashable)`, as today |

  This keeps each file's single writer. `extracted.json` is still
  written only for an identifier extraction found and a lookup
  confirmed: at the resolution, or at the move when the confirming
  lookup was a retry, whose resolution event reports the write
  `awaiting-acceptance`. The retry's title check is today's
  (`resolve_supplied`), so an agreeing or insufficient check gives
  `extracted`. A conflict the operator overrides gives `overridden`,
  which goes to `operator.json`.
- `pipeline::accept` is unchanged.

## D5. Recovery converges on the library's evidence (critic finding 4)

**Decision.** The library is authoritative for the evidence of bytes
its records hold. `borax reconcile` is the command that restores it to
the index. After its own writes, it reads every artifact record and,
for each distinct hash in each history:

- `e = record.operator_evidence_for(hash)`;
- when `e` exists and the index's `operator.json` for `hash` is not
  `Held(e)` (absent, unreadable, or naming another identifier or
  origin), it writes `e` to `operator.json`. When the same hash appears
  in several records with different operator evidence, the record read
  last in path order wins, and the next reconcile writes the same;
- when `e` does not exist, it writes nothing and leaves any
  `operator.json` there alone: the same bytes may have been answered
  outside the library.

Each restoration is reported as `content-index-write`. A failure costs
nothing else.

- **Restoration writes nothing else:** no `extracted.json`, which any
  later resolution re-derives, and no library state, so an untouched
  library still gets no diff.
- **It does not depend on the library's identity.**
- **Only `reconcile` restores.**
  - `status` and `validate` are read-only by requirement.
  - A tracked file's resolution never reads or writes the index.
  - An applying run writes operator evidence only for a decision it
    made itself.
- **Competing operator decisions.** The library's wins at the next
  reconcile. Until then the index answers with the decision written
  last, which may be one made for a copy outside the library.

**The critic's sequence, traced.**

1. Bytes H. The index holds `extracted.json` = X from an earlier batch
   resolve.
2. An interactive run in library L: the operator supplies Y and
   renames.
3. The admission writes the item, then the record, whose new entry is
   `(H, I_Y, s(Y))`.
4. Interrupted before `remember`.
   - State: the index holds `extracted` X and no `operator.json`; L
     holds `s(Y)`.
   - An outside run over a copy answers X, the stale fallback, until
     recovery.
5. `borax reconcile` in L: `operator_evidence_for(H)` = `s(Y)`, and
   `operator.json` is absent, so it writes `s(Y)` and emits one
   `content-index-write`.
   - State: `extracted` X and `operator` `s(Y)`, which answers Y.

The uninterrupted run ends with `extracted` X (step 1) and `operator`
`s(Y)` (`remember`): **equal**.

The same rule converges when the operator re-identifies the newest
bytes, because D3's append rule guarantees the library holds `s(Y)`.

**Interrupted outside a library.** No library holds the decision, so
the rename stands with nothing remembered, as today's best-effort
remembering already allows.

**Cost.** One small read per distinct hash per reconcile. Today an
untouched reconcile reads the records and stats the artifacts, so this
is the new floor.

**Rejected:**

- **Restoring from an applying run over the file.** A tracked file is
  answered by the library and never touches the index, and making it
  write would contradict "A library answer SHALL neither read nor write
  the content index".
- **Restoring `extracted.json` too.** Re-derivable, and writing it
  needs nothing from the library.
- **Letting the index's newer operator decision win.** The library is
  the durable home by the settled decision. An outside decision about
  bytes the library holds is a correction the operator can make in the
  library.

## D6. What an index answer means

**Decision.** For a file the run's library does not answer for, with
the cache on and the file hashed (otherwise as today):

1. `IndexEntry::answer()` is none: `read` `miss`. Extraction, the
   lookup, the title check and `extracted.json` follow, as today.
2. It is `e`: `read` `hit` with `evidence` `e`.
   - `e.identifier` is looked up through the run's sources, the
     response cache first where one stands in front, with origin
     `content-index`.
   - The file is not opened: extraction and its titles are not
     attempted, for `content-index-hit`.
   - The title check is not made, for `content-index-hit`, whatever
     `e.origin`:
     - `overridden` is the operator's acceptance ("Ambiguity is
       skipped, never guessed": "SHALL NOT be judged again");
     - `supplied` is an identifier whose record the operator was shown;
     - `extracted` was written only for an unrefused record of these
       same bytes.
   - Found: resolved. `record_retrieval` is the service's response
     cache or the network, and the evidence write is not attempted,
     for `content-index-hit`.
   - Not found: skipped `unresolvable`, with the attempts, and not sent
     to extraction, which would find the same identifier. Retry is
     offered as for any inconclusive lookup.

   **Retry of an index-held lookup** (settled after the second critic
   pass):

   - It repeats the identifier with the stored evidence's origin
     (`Origin::Indexed(o)`, reported `content-index`), and keeps the
     earlier rounds.
   - It runs **no** title check: the titles stay not attempted for
     `content-index-hit`, as for every evidence answer.
   - It writes **no** evidence: the index already holds it, and
     `remember` returns not attempted (D4).

   It does not use `pipeline::resolve_supplied`, which reads the file's
   titles and checks them unconditionally (`pipeline.rs`, near line
   834). It uses a new `pipeline::resolve_indexed_again(identifier,
   origin, sources, prior)` (D10). That function looks the identifier
   up and keeps `prior`'s rounds. It leaves `prior`'s extraction,
   titles and `match_check` as they are (not attempted for
   `content-index-hit`) and sets the content-index write not attempted
   for `content-index-hit`. A record found is resolved, and nothing it
   reaches is a conflict.

A record reached this way goes through the work check and the
interactive question as any record does. This is "evidence-first" only
because 13a holds nothing else. 13b puts references before the
evidence (its notes record why).

**Rejected:**

- **Re-running the title check.** It would open every file the index
  knows, which is half of what the index saves, and would judge an
  operator's acceptance again.
- **Falling back to extraction when the lookup finds nothing.** The same
  bytes give the same identifier, and an outage would cost a needless
  open.

## D7. Adoption

**Decision.** For each orphan that is readable and not `held`, `adopt`
asks `IndexEntry::answer()` for its hash. It queries no service,
extracts nothing, and opens nothing beyond hashing.

- **An answer `e`.** The response cache under `Adapters::cache_root`
  is read by `cache::key(service, e.identifier)` for each service of
  `dispatch::priority(identifier)` (`dispatch.rs`, line 62), whatever
  the `sources` setting: `adopt` consumes no setting, and a stored
  answer is what that service gave. The first record found is used:
  - `Recorded`, with `retrieval` `service-cache` naming that service;
  - the history entry carries `{e.identifier, adopted}`.

  With no record found, it is `unanswered`, carrying `identifier`.
- **No answer:** `unindexed`, the kind kept with its meaning, "nothing
  in the index".
- **Adoption writes nothing to the index.** The identifier is already
  there, and adoption's own origin belongs to the library.
- `library::adopt` keeps its `lookup` seam, now returning
  `Result<Adopted, Unadopted>`.

**Rejected:**

- **The network as a last resort.** `adopt` is the offline way to
  record a library. `borax resolve` over an `unanswered` orphan fetches
  its record, and then `adopt` records it.
- **Writing an `adopted` fact.** It would duplicate a fact the index
  already holds, under an origin that describes the library's act, not
  the bytes' identification.

## D8. The schema: 5

The `cli` requirement changes the version when "a consumer that reads
the stream correctly today could read a later stream wrongly", and fixes
a version at its first release. Schema 4 shipped in 0.9.0. Released
meanings that change here:

| Key or value | Schema 4 | Schema 5 |
|---|---|---|
| `content_index.read` `hit` | a record served; nothing ran after it | an identifier answered with; the lookup runs after it |
| `content_index.write` | the record written | the extracted identifier written |
| `record_retrieval` `content-index` | the index's copy | removed |
| `content-index-write` | the move's remembering of an operator's record | every evidence write made after a verdict: the move's, for an operator's record or a retried extracted lookup, and `reconcile`'s restorations |
| `library-adoption` `unindexed` | no record for the bytes | no evidence for the bytes; evidence without a cached record is the new `unanswered` |
| `extraction`, `lookup`, `match_check` not attempted for `content-index-hit` | always together | `lookup` is now made after a hit |

The version is **5**. 13b may add to schema 5 freely until a release
emits it.

Added in 13a:

- `content_index.read` `hit`: `evidence`, with `identifier` and
  `origin` (`extracted`, `supplied` or `overridden`).
- `lookup.origin`: `content-index`.
- `content-index-write`: `hash`, and `entry`, tagged `kind`, here only
  `evidence` (with `identifier`, and `origin` `extracted`, `supplied` or
  `overridden`). `write` is `written` or `failed`.
- `library-adoption` `recorded`: `retrieval`, here only `service-cache`;
  and the kind `unanswered`, with `identifier`.

No new not-attempted reason. `operator-evidence` from the first draft is
gone.

**Rejected: keeping 4.** There is no additive spelling of "the index no
longer holds records".

## D9. Human lines and the description

Every value from an entry or a file is escaped, as the text after
`<path>: ` already is in `human_line`.

- **`resolved` retrieval clause:** `from the content index` goes with
  its kind. An evidence answer ends `from the response cache` or `from
  the network`.
- **`content-index-write` failed:** `<path>: the content index could not
  keep this answer (<message>)`, unchanged, for a restoration too. A
  written one renders nothing.
- **`library-adoption` `unanswered`:** `the content index knows its
  bytes as <id>, but no response cache holds that record, so it is still
  an orphan; borax resolve fetches it`. Every other kind is unchanged.
- **The interactive description** (`describe.rs`):
  - an evidence answer's identifier line is `<id>, found by an earlier
    run`, `<id>, supplied in an earlier run`, or `<id>, accepted over a
    title conflict in an earlier run`, as the read's `evidence.origin`
    says;
  - the `record` line names the service that answered, as for any
    lookup;
  - `file says` keeps `not read; an earlier run answered` for
    `content-index-hit`;
  - the `from an earlier run` record line goes with its retrieval kind.

## D10. The interface the tests are written against

`borax-core` (`crates/borax-core/src/library.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceOrigin { Extracted, Supplied, Overridden, Adopted }
impl EvidenceOrigin {
    pub fn as_str(self) -> &'static str;          // extracted, supplied, overridden, adopted
    pub fn parse(text: &str) -> Option<EvidenceOrigin>;
    pub fn is_operator(self) -> bool;             // Supplied, Overridden
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentificationEvidence { pub identifier: Identifier, pub origin: EvidenceOrigin }

// HashEntry gains, after `item`: `pub evidence: Option<IdentificationEvidence>`,
// written as `identifier` and `origin`, both or neither (D3).

impl ArtifactRecord {
    /// The newest entry holding `hash` that carries evidence.
    pub fn evidence_for(&self, hash: &ContentHash) -> Option<&IdentificationEvidence>;
    /// The newest entry holding `hash` whose evidence has an operator origin.
    pub fn operator_evidence_for(&self, hash: &ContentHash) -> Option<&IdentificationEvidence>;
}
```

`borax-core`, a new module `crates/borax-core/src/index.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact<T> { Absent, Held(T), Unreadable(String) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub extracted: Fact<Identifier>,
    pub operator: Fact<IdentificationEvidence>,
}
impl IndexEntry {
    /// D2's table.
    pub fn answer(&self) -> Option<IdentificationEvidence>;
}
/// `{"identifier":"…"}`.
pub fn extracted_to_json(identifier: &Identifier) -> String;
pub fn extracted_from_json(text: &str) -> Result<Identifier, Malformed>;
/// `{"identifier":"…","origin":"supplied"|"overridden"}`; any other
/// origin is `Malformed`.
pub fn operator_to_json(evidence: &IdentificationEvidence) -> String;
pub fn operator_from_json(text: &str) -> Result<IdentificationEvidence, Malformed>;
```

`borax-sources` (`crates/borax-sources/src/store.rs`):

```rust
pub const INDEX_FORMAT_VERSION: &str = "v2";
pub fn data_root(lookup: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf>;
pub fn default_data_root() -> Option<PathBuf>;

/// `content/<hash>/extracted` and `content/<hash>/operator`.
pub fn extracted_key(hash: &ContentHash) -> String;
pub fn operator_key(hash: &ContentHash) -> String;

/// Where content-index facts are kept. `Sync` as `Cache` is.
pub trait IndexStore: Sync {
    /// The fact's text: `None` absent, `Some(Err(message))` unreadable.
    fn read_fact(&self, key: &str) -> Option<Result<String, String>>;
    /// Replace the fact whole, atomically.
    fn write_fact(&self, key: &str, text: &str) -> CacheWrite;
}
impl IndexStore for FileCache;   // entry_path(root, key), write_atomically
impl IndexStore for MemoryCache; // a second map

impl<S: IndexStore> ContentIndex<S> {
    /// Both facts, parsed.
    pub fn entry(&self, hash: &ContentHash) -> IndexEntry;
    pub fn put_extracted(&self, hash: &ContentHash, identifier: &Identifier) -> CacheWrite;
    /// `Failed` without writing for evidence of a non-operator origin.
    pub fn put_operator(&self, hash: &ContentHash, evidence: &IdentificationEvidence) -> CacheWrite;
}
```

`ContentIndex::get`/`put` over `Record` (the `impl<C: Cache>` block)
are removed in group 4 (D12).

`borax` (`crates/borax/src/`):

```rust
// evidence.rs
// IndexRead::Hit(IdentificationEvidence)            (was Hit)
// Origin::Indexed(EvidenceOrigin)                   (new)
// RecordRetrieval::ContentIndex                     (removed)
// Unattempted                                       (unchanged)

// pipeline.rs
impl FileRecord { pub fn identification(&self) -> Option<IdentificationEvidence>; } // D3
/// D4: writes `file.identification()` through the writer for its
/// origin: `put_extracted` for `extracted`, `put_operator` for
/// `supplied`/`overridden`. `NotAttempted(ContentIndexHit)` for an
/// `Origin::Indexed` lookup, and `NotAttempted(Unhashable)` with no hash.
pub fn remember<S: IndexStore>(index: &ContentIndex<S>, file: &FileRecord) -> IndexWrite;
/// D6: a retry of an index-held lookup. No title check and no titles
/// read; `prior`'s rounds kept; the content-index write not attempted
/// for `content-index-hit`.
pub fn resolve_indexed_again(
    identifier: &Identifier,
    origin: EvidenceOrigin,
    sources: &[&dyn Source],
    prior: &Evidence,
) -> Result<FileRecord, Unresolved>;
// index_read, standing, resolve_file, resolve_batch: `C: Cache` becomes `S: IndexStore`

// library.rs
// Admitting gains `pub evidence: Option<&'a IdentificationEvidence>`.
pub struct Adopted { pub record: Record, pub identifier: Identifier, pub service: SourceName }
pub enum Unadopted { Unindexed, Unanswered { identifier: Identifier } }
pub fn adopt(root: &Path, lookup: &dyn Fn(&ContentHash) -> Result<Adopted, Unadopted>,
             key: &mut dyn FnMut(&Record, &ContentHash) -> Option<String>,
             run: RunId, timestamp: &str, tool_version: &str) -> Adoptions;
/// D5: every (full path, hash, operator evidence) the library's records owe the index.
pub fn operator_decisions(root: &Path) -> Vec<OperatorDecision>;
pub struct OperatorDecision { pub path: PathBuf, pub hash: ContentHash, pub evidence: IdentificationEvidence }

// event.rs
pub const SCHEMA: u32 = 5;
// IndexReadStep::Hit { evidence: EvidenceStep }, EvidenceStep { identifier: String, origin: String }
// IdentifierOrigin gains ContentIndex ("content-index")
// RetrievedFrom loses ContentIndex
// Event::ContentIndexWrite { path, hash: String, entry: WrittenEntry, write: WriteStep }
// #[serde(tag = "kind")] WrittenEntry::Evidence { identifier: String, origin: String }
// Adoption::Recorded { id, item, retrieval: RetrievedFrom }; Adoption::Unanswered { identifier: String }

// run.rs
// Adapters<'a, S: IndexStore> (was C: Cache; `S: Cache + IndexStore` in group 3, D12)
```

Keeping their signatures: `admit`, `Admitted`, `reconcile`,
`Reconciling`, `Stores::consult`, `standing`, `resolve_file`,
`resolve_batch` (bounds aside), `resolve_supplied` and `accept`.

The private parts are the implementer's:

- how the rename driver passes the evidence to `recorded`;
- how retry finds the file's own lookup and chooses between
  `resolve_supplied` (extracted) and `resolve_indexed_again` (indexed);
- the enum `execute` opens the index over.

## D11. Sequences traced

`H` is a file's hash. "Outside" means a run over a copy of the file
with no library above it.

**(a) Interrupted operator admission:** D5's trace.

**(b) An operator's answer outside any library.**

1. Interactive rename of `scan.pdf`; the operator supplies `doi:X`,
   and the file moves.
2. `operator.json` = `s(X)`, and a `content-index-write`.
3. `borax cache --clear`, then a batch run online: `hit` with `s(X)`,
   `doi:X` looked up over the network, no question, no extraction. The
   network answer is written to the response cache (`stored` `written`).
4. The same run again, offline: `hit` with `s(X)`, and `doi:X` is
   answered from the response cache step 3 refilled (`retrieval`
   `service-cache`). No request is sent, and the file is resolved.
5. Branch, from step 3: a second `borax cache --clear`, or a step 3
   whose response-cache write failed (`stored` `failed`), and then the
   offline run: `unresolvable`, every attempt `unavailable`. The
   identifier is kept in `operator.json`, and the next online run
   resolves it.

**(c) `--no-cache` over operator evidence.** `operator.json` = `s(X)`.

1. `borax resolve --no-cache`: `read` `bypassed`; extraction finds
   `doi:Y`; `extracted.json` = Y, `written`.
2. The next run: `hit` with `s(X)`, and `doi:X` is looked up.

**(d) A late extraction write.**

1. Resolution R1 starts and misses.
2. An interactive run remembers `s(X)` for H.
3. R1 then writes `extracted.json` = Y.

Both files exist, and the entry answers X. In the first draft, R1's
read-merge-write would have erased X.

**(e) Supply → clear cache → batch → offline adopt in library L2.**

- The supplied `s(X)` survives the clear.
- The online batch looks X up and refills the response cache.
- An offline `adopt` in L2 then records from Crossref's cached answer,
  with history `{X, adopted}`, and writes nothing to the index.
- If the batch's lookup failed, `adopt` reports `unanswered` with X.

**(f) Re-identifying the newest bytes in a library.**

1. Record `[A(X, x)]`, link X. The operator supplies `doi:y` (item Y)
   and renames.
2. History `[A(X, x), A(Y, s(y))]`, link Y. `operator.json` = `s(y)`.
3. Deleting the data root, then `reconcile`: `operator.json` = `s(y)`
   again.

Under the old rule, step 2 appended nothing, and step 3 had nothing to
restore from: critic finding 1.

**(g) An unreadable `operator.json`** beside `extracted.json` = X.

- Read: `miss`. Extraction finds X, looks it up, checks the title, and
  writes `extracted.json` again.
- In a library whose history holds `s(Y)` for H, the next reconcile
  rewrites `operator.json` = `s(Y)`.

**(h) The data root deleted, inside library L.**

- Tracked files still resolve from L.
- Outside copies miss and extract.
- `reconcile` restores every operator decision L holds. Extracted
  evidence returns as files are resolved again.

**(i) `cache --clear` alone.** The cache root is removed and the data
root is untouched. Every evidence answer then costs a network lookup
once.

## D12. Group boundaries and green points (critic finding 6)

Each group's type change lands with every consumer of that type, in one
red/green pair. Every group ends compiling, with the whole suite green.

| Group | Lands | Why it is green at its end |
|---|---|---|
| 1 | `HashEntry::evidence` and its parse; `Admitting::evidence`; the append rule; reconcile's extracted evidence; `FileRecord::identification` for today's origins | Every `HashEntry` and `Admitting` literal, `src` and tests, is edited in the same pair, and nothing else changes type |
| 2 | `borax_core::index`; `data_root`; `IndexStore` for `FileCache` and `MemoryCache`; `ContentIndex`'s evidence API **beside** its record API | Purely additive: no existing consumer changes |
| 3 | The resolution switch: pipeline, `remember` with its per-origin writers (an extracted retry included), the evidence engine, every event change but adoption's, schema 5, the description, `Adapters<S: Cache + IndexStore>`, `IndexStore` impls on custom test stores | Every consumer of `IndexRead`, `RecordRetrieval`, `Origin`, `IndexReadStep`, `RetrievedFrom`, `IdentifierOrigin`, `ContentIndexWrite` and `SCHEMA` changes here. `adopt` still reads the record API. That API exists, and its tests seed it |
| 4 | Adoption: the `lookup` seam, `Adoption::Recorded::retrieval`, `Unanswered`, the record API removed, `Adapters<S: IndexStore>` | The record API's last consumers (`adopt_events` and the adoption tests) change in the same pair |
| 5 | `reconcile` restores operator evidence (D5) | Additive behaviour, through the event shape group 3 fixed |
| 6 | `execute` opens the index over the data root; the binary tests | One call site, and the binary tests |
| 7 | Retry of an index-origin lookup, through `resolve_indexed_again` | Additive. Until it lands, Retry on such a lookup does nothing, as Retry with no own identifier does today. `remember`'s `Indexed` arm lands in group 3, so it is already in place |

**Interim states, stated so nobody mistakes them for the contract:**

- **After group 3, before group 4:** a real `adopt` reads the record
  API, which no run writes any more, so it adopts nothing a current
  run resolved.
- **After group 3, before group 6:** the real binary keeps facts under
  the cache root, so `cache --clear` still removes them.

Both are closed within the change. Neither is a stopping point: the
change merges after group 9.

**Rejected:**

- **Converting `ContentIndex::get` in group 2.** That is the critic's
  counterexample: its consumers land later.
- **Groups 3 and 4 as one pair.** It is possible, but the pair would
  hold the whole event sweep and the whole adoption sweep. The record
  API surviving one group is cheaper than that.

## D13. How the specifications change

ADDED:

- `resolution`:
  - "The content index keeps identification evidence, one fact per
    file": D1, D2, D4, and the definition that keeps five requirements
    unchanged;
  - "A content-index entry answers with its identifier": D6.
- `library`: "A library's operator decisions are restored to the
  content index": D5.

MODIFIED, each restated whole with every living paragraph and scenario
name kept, so no `<!-- drops: -->` marker is used.

In `resolution`:

- "Responses are cached locally";
- "A resolution reports the evidence it was checked against";
- "An operator's answer about a file is remembered";
- "A resolution says whether its library answered";
- "A resolution keeps every service it asked, in order";
- "A resolution says where its record came from";
- "A resolution says what the content index did";
- "A resolution says why a step was not taken";
- "Resolution events carry their evidence in sections";
- "A resolution reports what the content index did and every cache
  write";
- "Each service asked is reported with its outcome";
- "A resolution reports where its record was retrieved";
- "What became of remembering an operator's answer is reported";
- "Human resolution lines name the work, where it came from, and why a
  file was skipped".

In `library`:

- "An artifact record is authoritative state keyed by an artifact
  identity": evidence, and Rule 1's exception;
- "An applying run records what it admits": re-identification of the
  newest bytes;
- "The library's text is authoritative and is read directly";
- "Validation defines a well-formed library";
- "borax adopt records what the library already holds".

In `rename`:

- "An interactive question shows what the answer rests on";
- "An interactive question says whether the library answered".

The proposal lists what was checked and left unchanged. "A file its
library tracks resolves from its library item" is among them, since
nothing here answers a copy through the library.

## D14. Existing tests: what changes and how (critic finding 7)

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is.

**How the sweep was made.** A script split every file under
`crates/*/tests/` into its `fn`, `struct` and `impl` items and read
each body. It does not just match names. A body was flagged when it
used any of:

- a type or value 13a changes:
  - `ContentIndex::new(` over anything but `MemoryCache::new()`;
  - `impl … Cache for`;
  - `index.put(`, `index.get(`;
  - `HashEntry {`, `Admitting {`, `library::adopt(`, `remember(`;
  - `IndexRead::Hit`, `IndexReadStep::Hit`, `IndexWrite::`;
  - `RetrievedFrom::ContentIndex`, `RecordRetrieval::ContentIndex`,
    `content-index-hit`, `ContentIndexHit`, `from the content index`,
    `"content-index"`, `content_index.read`;
  - `ContentIndexWrite`, `content-index-write`;
  - `Adoption::`, `unindexed`;
  - `SCHEMA`, `Value::from(4)`, `"schema":4`;
  - `IdentifierOrigin::`, `"origin"`;
  - `earlier run`, `FORMAT_VERSION`, `content_key`, `entry_path`,
    `cache --clear`;
  - `reidentified: true`;
- or two or more calls of `events_for`, `standing`, `resolve_file`,
  `resolve_batch` or `dispatch` sharing an index (a second run may
  meet the first's evidence).

That flagged 313 items in 16 files. Each flagged body was then read
for whether 13a changes what it builds or expects. Those that do are
below. The rest construct an unchanged type, or run twice over separate
indexes, or with sources that answer every call, and assert nothing a
lookup changes. Line numbers drift as earlier groups edit a file. A
site the test stage cannot find, or one it finds that is not here, is
reported back.

**Shape-only (a):**

| Group | File | Site | Edit |
|---|---|---|---|
| 1 | `crates/borax-core/tests/library.rs` | `hash_entry` (39) | `evidence: None` |
| 1 | `crates/borax/tests/library.rs` | `hash_entry` (101), `entry_naming` (112), the literal in `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded` (2419) | `evidence: None` (the last also (b), below) |
| 1 | | the 37 `Admitting { … }` literals, `admitting` (9348) among them | `evidence: None`. With no evidence, change 15's rule is unchanged, so every expectation holds except the two rewrites below |
| 1 | `crates/borax/tests/dispatch.rs` | `lib_hash_entry` (7180), `hash_entry_for` (10602), `seed_tracked_file` (17167), `seed_artifact_record` (17808), and the literals in `resolve_answers_restored_bytes_from_the_item_they_were_accepted_under` (17707, 17714) | `evidence: None` |
| 1 | `crates/borax/tests/pipeline.rs` | `record_at` (3052) | `evidence: None` |
| 1 | `crates/borax/tests/binary.rs` | `seed_tracked_binary_fixture` (401) | `evidence: None` |
| 3 | `crates/borax/tests/pipeline.rs` | `WriteFailingCache` (near 5987), `CountingCache` (near 5999) | an `IndexStore` impl. `WriteFailingCache`'s fails every write. `CountingCache`'s counts reads and writes into the same counters, so `a_tracked_file_resolves_from_its_item_with_no_extraction_no_source_and_no_index_traffic` keeps its zero |
| 3 | `crates/borax/tests/dispatch.rs` | `WriteFailingCache` (near 6288) | an `IndexStore` impl failing every write |
| 3 | `crates/borax/tests/runlog.rs` | `WriteFailingCache` (near 797) | the same |
| 3 | `crates/borax-sources/tests/store.rs` | `WriteFailingCache` (near 24) | the same |
| 3 | `crates/borax/tests/event.rs` | `json_line_of_resolved_carries_every_library_answer_variant_tagged_by_kind` (3291, two assertions), `json_line_of_skipped_carries_every_library_answer_variant_tagged_by_kind` (3394), `library_not_attempted_replaces_the_schema_3_null` (3420) | `Value::from(4)` becomes `Value::from(5)` on the `schema` key |
| 3 | | every `"schema":4` literal: `json_line_of_library_extraction_matches_design_d4_for_every_kind`, `json_line_of_library_condition_matches_design_d3_for_every_kind`, `library_status_without_an_identity_is_written_without_an_id_key`, `library_status_carries_the_id_directly_after_the_root`, `library_validated_without_an_identity_is_written_without_an_id_key`, `library_validated_carries_the_id_directly_after_the_root`, `json_line_of_library_adoption_unindexed_matches_design_d6` (~4075) | `"schema":5`. The last is here, beside `SCHEMA = 5`, and not in group 4: `unindexed`'s JSON shape does not change, so group 4 has no edit for it |
| 3 | | the `content_index_write` helper (~367) | builds the event with a fixed `hash` and an `entry` of kind `evidence`. Its other callers, `all_events`, `json_line_event_tag_is_the_variant_name_in_kebab_case`, `a_plausible_run_renders_as_json_lines_ending_in_the_summary` and `counts_observe_ignores_content_index_write`, keep their expectations |
| 3 | `crates/borax/tests/dispatch.rs` | the `sections_for` helper (451) | its `(None, true)` branch, which builds a content-index retrieval, is rewritten for an evidence answer or removed. No caller reaches it: all six `resolved_event` callers (`resolve_emits_resolved_then_skipped_for_a_mixed_batch`, `rename_preview_emits_resolved_and_planned_and_moves_nothing`, `rename_preview_with_no_journal_succeeds`, `rename_apply_emits_renamed_carrying_the_hash_and_moves_the_file`, `bib_emits_resolved_then_the_bib_events_and_the_fake_bib_files_received_the_writes`, `bib_over_a_library_holding_an_artifactless_item_emits_no_entry_for_it`) pass `cached: false`, and keep their expectations |
| 4 | | `human_line_of_library_adoption_escapes_control_characters_in_the_path_for_every_kind` (4233), `human_line_of_library_adoption_is_unchanged_for_a_path_with_no_control_character` (4269) | `Adoption::Recorded` gains `retrieval`. The first also gains an `Unanswered` case, the new kind's escaping (b) |

**Deliberate rewrites (b).**

The general translation for a test that seeds a record:

- `index.put(&hash, &record)` becomes `index.put_extracted(&hash,
  &doi)`, or `put_operator` where the test is about an operator's
  answer;
- a `fake_source` or `KeyedSource` holding `record` under that DOI is
  added where the test had none;
- the fact the test pins (not opened, gated, provenance named) is
  restated for an evidence answer: one lookup, origin `content-index`,
  retrieval from the service.

| Group | File | Test | New form |
|---|---|---|---|
| 1 | `crates/borax/tests/library.rs` | `admit_reidentifying_the_newest_bytes_moves_the_link_and_appends_nothing` (4912) | passes `evidence: Some(s(y))`, and expects `[entry_a, (H, Y, s(y))]` with the link Y. Renamed `admit_reidentifying_the_newest_bytes_moves_the_link_and_records_the_operators_evidence` |
| 1 | | `transition_f_relinking_the_newest_bytes_appends_nothing_until_later_bytes_restate_it` (5886) | the re-identification passes `s(y)`. The intermediate assertion is `[A(X), A(Y, s(y))]`, the final `[A(X), A(Y, s(y)), B(Y)]`, still three entries, and `history[1].evidence` is `s(y)`. Renamed `transition_f_relinking_the_newest_bytes_records_the_operators_evidence_at_once` |
| 1 | | `fast_path_miss_appends_the_new_hash_after_the_ones_already_recorded` | its appended entry carries the extracted evidence |
| 1 | `crates/borax/tests/dispatch.rs` | `a_reidentified_newest_version_moves_the_link_and_appends_nothing` (13026) | the real driver now appends `(H, item for doi, s(doi))`. Renamed `a_reidentified_newest_version_moves_the_link_and_records_the_supplied_identifier` |
| 2 | `crates/borax-sources/tests/store.rs` | none | group 2 is additive |
| 3 | `crates/borax/tests/pipeline.rs` | `content_index_hit_is_returned_without_opening_the_file`, `a_content_index_hit_marks_every_later_section_not_attempted`, `a_content_match_at_the_incoming_path_is_not_a_duplicate_and_resolves_from_the_index`, `standing_defaults_identifier_input_to_not_asked_on_every_verdict_but_a_content_duplicate` | the general translation |
| 3 | | `a_content_index_hit_reports_its_record_retrieval_as_content_index` | renamed `a_content_index_answer_reports_the_lookups_service_as_its_record_retrieval` |
| 3 | | `cache_false_bypasses_the_content_index`, `bypassed_with_cache_false_still_writes_the_index`, `a_successful_resolution_is_written_to_the_content_index`, `a_successful_resolution_is_written_to_the_index_even_with_cache_bypassed`, `a_dangling_item_problem_falls_back_and_writes_the_content_index` | `index.entry(&hash).extracted` is `Held(doi)` |
| 3 | | `a_conflict_candidate_keeps_its_whole_evidence`, `a_refused_record_is_never_remembered_and_a_later_run_still_conflicts`, `an_earlier_index_entry_survives_a_no_cache_conflict` | the entry's facts, unchanged or absent |
| 3 | | `remember_writes_a_record_a_later_resolve_then_answers_with` | renamed `remember_writes_operator_evidence_a_later_resolve_then_looks_up` |
| 3 | | `remember_over_a_write_failing_cache_reports_the_failure`, `remember_with_no_hash_is_not_attempted_as_unhashable` | `remember(index, &file)` |
| 3 | | `a_second_identical_batch_is_served_from_the_index_and_never_touches_a_source` | the second batch's sources are a `Cached` source over a `MemoryCache` that the first batch filled, wrapping a `PanicSource`. Kept: no extraction (`open_calls`) and no network. Renamed `a_second_identical_batch_is_answered_through_the_index_and_the_response_cache` |
| 3 | | `a_renamed_file_with_identical_content_is_served_from_the_index_without_opening_or_resolving` | the same, with the response cache; "without resolving" becomes "without a network request" |
| 3 | | `resolve_supplied_after_a_content_index_hit_re_reads_titles_and_keeps_the_read`, `prior_content_index_hit`, `evidence_via_content_index_hit`, `sections_projects_every_index_read_and_write`, `sections_projects_record_retrieval_for_every_place_and_none`, `a_fresh_resolution_with_no_library_carries_the_whole_evidence` | `IndexRead::Hit(evidence)`. `RecordRetrieval::ContentIndex` is dropped from the "every place" list. The evidence hit's lookup is attempted |
| 3 | `crates/borax/tests/dispatch.rs` | `a_batch_cached_resolution_names_its_provenance_not_the_cache`, `an_untracked_file_answered_by_the_content_index_is_not_opened_for_the_gate`, `a_content_index_answer_at_a_recorded_path_with_changed_bytes_is_gated_on_extraction`, `resolve_reports_untracked_for_an_unrecorded_file_inside_a_library`, `resolve_reports_the_librarys_corrected_item_over_a_stale_index_entry` | the general translation |
| 3 | | `a_rename_from_a_supplied_identifier_is_written_to_the_content_index`, `skipping_after_a_supplied_identifier_leaves_the_content_index_as_it_was`, `quitting_after_a_supplied_identifier_leaves_the_content_index_as_it_was`, `skipping_a_conflict_that_could_have_been_overridden_writes_nothing`, `a_supplied_identifier_resolving_into_a_taken_target_reports_and_reasks`, `a_resolved_candidate_that_led_nowhere_leaves_the_files_own_record_to_cite`, `a_rejected_candidate_is_never_written_to_the_content_index` | `index.entry` asserts `operator` `Held(s(doi))`, or `Absent`, or the earlier facts unchanged |
| 3 | | `a_content_index_write_that_fails_leaves_the_rename_standing_and_asks_again_next_time` | `WriteFailingCache` gains `IndexStore`. The second run still asks, because `operator.json` was never written |
| 3 | | `resolve_no_cache_still_reads_the_library_and_writes_nothing_to_the_index`, `interactive_tracked_file_offers_the_move_question_and_writes_nothing_to_the_index`, `tracked_file_already_named_reports_already_named_and_writes_nothing` | `index.entry(&hash)` holds both facts `Absent` |
| 3 | | `a_supplied_identifier_renamed_is_followed_by_content_index_write`, `a_rename_time_content_index_write_failure_is_reported_without_failing_the_rename`, `accepting_a_supplied_candidate_with_agreeing_titles_reports_accepted`, `the_round_two_session_reports_every_submission_on_the_final_skip` | `ContentIndexWrite` gains `hash` and `entry` |
| 3 | | `a_batch_run_over_three_artifacts_of_one_item_skips_none_of_them`, `a_batch_content_duplicate_skip_carries_every_section_not_attempted` | the content-index retrieval and `hit` shapes |
| 3 | `crates/borax/tests/end_to_end.rs` | `a_rename_from_a_supplied_identifier_is_recognised_offline_by_a_later_batch_run` | both runs' sources are `Cached` over one shared `MemoryCache`. The second run sends no transport request, its `record_retrieval.kind` is `service-cache`, and its `lookup.origin` is `content-index` |
| 3 | `crates/borax/tests/event.rs` | `schema_is_four` | renamed `schema_is_five` |
| 3 | | `json_line_of_content_index_write_has_exactly_the_documented_field_set` (~1019) | its exact field set becomes `entry`, `event`, `hash`, `path`, `schema`, `write`, and it asserts `entry`'s `kind` `evidence` with `identifier` and `origin` |
| 3 | | `all_events`, `evidence_via_content_index_hit`, `sections_via_content_index`, `a_content_index_answer_whose_provenance_names_crossref_reports_crossref`, `human_line_appends_the_library_problem_clause_to_a_content_index_resolved_line`, `human_line_of_resolved_shows_the_work_and_the_from_clause`, `an_arxiv_found_identifier_survives_a_doi_carrying_record`, `a_record_whose_provenance_names_no_service_names_no_via` | the content-index retrieval becomes `service-cache`, with `from the response cache`; `hit` carries `evidence`; `ContentIndexWrite` gains `hash` and `entry` |
| 3 | `crates/borax/tests/describe.rs` | `Fixture` and its content-index case, `a_content_index_answer_is_d1s_worked_example`, `a_cached_answer_names_no_origin_for_its_identifier` | the identifier line names `found by an earlier run`; the record line names the service |
| 3 | `crates/borax/tests/renaming.rs` | `sections_via_content_index` | `hit` with `evidence`, `service-cache` retrieval |
| 3 | `crates/borax/tests/runlog.rs` | `run_log_contains_resolved_renamed_and_content_index_write_in_order`, `run_log_reports_a_failed_content_index_write_without_failing_the_rename`, `run_log_contains_exactly_the_json_stdout_stream_including_framing_events` | the event's new fields |
| 6 | `crates/borax/tests/binary.rs` | `seed_stale_content_index`, used by `resolve_reports_a_tracked_file_the_same_way_for_every_relative_spelling` | seeds an `extracted.json` for another DOI under `XDG_DATA_HOME/borax/v2/content/<hash>/`, and the run sets `XDG_DATA_HOME` |
| 4 | `crates/borax/tests/dispatch.rs` | the `adopt_over` helper (14931) | takes the response-cache root and passes `cache_root: Some(responses)` instead of `None`. Its `PanicSource` and its no-open and no-rename assertions stay. Its callers rely on it: `adopt_records_each_orphan_the_content_index_answers_for`, `adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent`, `adopt_holds_an_orphan_whose_bytes_a_record_already_holds` and `adopt_every_orphan_is_accounted_for` seed that root with the records they adopt. `adopt_reads_neither_a_sidecar_nor_the_retired_ledger` passes an empty `tempdir` and keeps its expectation |
| 4 | `crates/borax/tests/dispatch.rs` | `adopt_records_each_orphan_the_content_index_answers_for`, `adopt_leaves_the_unknown_and_the_recorded_alone_and_is_idempotent`, `adopt_holds_an_orphan_whose_bytes_a_record_already_holds`, `adopt_every_orphan_is_accounted_for`, `adopt_outside_any_library_is_refused_and_writes_nothing`, `an_adoption_into_a_library_with_a_faulty_identity_file_warns_once` | seeded with `put_extracted` and a response-cache entry under `cache_root` (a `tempdir`) by `FileCache::put(&cache::key(SourceName::Crossref, &doi), &record)`. `Recorded` gains `retrieval` |
| 4 | | `adopt_after_the_cache_is_cleared_adopts_nothing_and_succeeds`, `adopt_after_the_cache_is_cleared_still_tells_unindexed_held_and_unreadable_apart` | the facts survive the clear: an evidence-only orphan is `unanswered` with its identifier, and a no-entry one `unindexed`. The first is renamed `adopt_after_the_cache_is_cleared_records_nothing_from_evidence_alone_and_succeeds` |
| 4 | | `adopt_human_mode_lists_an_unindexed_orphan_before_the_totals_line`, `adopt_human_mode_ends_on_its_own_totals_line_with_no_summary` | kept: their orphans have no entry |
| 4 | `crates/borax/tests/event.rs` | `all_events` (adoption cases), `render_human_equals_human_line_for_every_library_condition_and_unindexed`, `counts_observe_of_library_adoption_unindexed_counts_nothing` | `Unanswered` added beside `Unindexed` |
| 4 | `crates/borax/tests/library.rs` | the four `library::adopt(` calls (`adopt_that_records_an_orphan_mints_the_identity`, `adopt_that_records_nothing_mints_nothing`, `adopt_into_a_library_with_a_faulty_identity_file_records_and_reports_the_fault`, `adopt_whose_mint_fails_records_nothing_and_reports_unwritten_naming_the_identity_file`) | the `lookup` closure returns `Result<Adopted, Unadopted>` |
| 4 | `crates/borax-sources/tests/store.rs` | `content_index_round_trips_a_record`, `content_index_put_returns_written_over_memory_cache`, `content_index_put_returns_what_its_cache_returned`, `content_index_misses_an_unknown_hash`, `renamed_file_with_identical_content_is_served_from_the_content_index`, `entries_survive_a_fresh_file_cache_opened_on_the_same_root` | over the evidence API, as the record API goes. Renamed `content_index_round_trips_both_facts` and `renamed_file_with_identical_content_is_answered_by_the_content_index` |

**Fixture helpers re-checked** (second critic pass). Every helper
used by a test whose expectation group 3 or 4 changes was read again:

- Reclassified as changed:
  - `adopt_over` (group 4);
  - `sections_for`, and event.rs's `content_index_write` (group 3).
- Unchanged:
  - `outside_the_stores`: the response cache is a `tempdir` outside the
    library, and adoption only reads it;
  - `adoptions`;
  - `assert_every_orphan_is_accounted_for`: it matches `Recorded { .. }`
    and counts every other kind as left, `unanswered` included;
  - `KeyedSource`, `FakeDocuments` and `ScriptedAsker`;
  - pipeline.rs's `default_content_index`, and event.rs's
    `evidence_via_lookup` and `sections_via_network`: they build `Miss`;
  - `prior_tracked`;
  - `seed_tracked_file` and `seed_artifact_record`: only group 1's
    shape edit.

Flagged by the sweep and **unchanged**:

- the `cache_root_*`, `entry_path_*`, `content_key_*` and `file_cache_*`
  tests in `store.rs`: their API stays;
- the `cache` parse and help tests in `cli.rs`, and `cache.rs`,
  `streaming.rs`: the cache command and module stay;
- every `IdentifierOrigin::Extracted`/`Operator` constructor
  (`event.rs`, `describe.rs`, `per_file.rs`, `dispatch.rs`): the enum
  only gains a variant, and no test matches it exhaustively;
- the multi-run tests with separate indexes or sources answering every
  call, which assert nothing a lookup changes:
  - `a_preview_writes_nothing_but_an_applying_run_over_the_same_file_does`;
  - `a_failed_store_write_leaves_the_rename_standing_and_the_next_run_records_it`;
  - `recording_catches_a_content_duplicate_but_no_record_admits_it_silently`;
  - `a_batch_resolved_with_one_worker_or_eight_produces_the_same_run`;
  - `resolve_batch_with_a_library_gives_the_same_events_at_concurrency_one_and_eight`;
  - `a_preview_followed_by_its_apply_leaves_two_files_that_sort_adjacently`;
  - the `reconcile_*`, `validate_*` and `status_*` pairs;
- `an_accepted_interactive_rename_is_recorded_exactly_as_an_apply_run_records_it`:
  it builds `HashEntry { item: None, ..entry }`, and both runs record
  the same extracted evidence;
- the remaining `transition_*` and `admit_*` tests: their admissions
  carry no evidence, so change 15's rule applies unchanged.

## Not changed, deliberately

- What any library answers for its own files, and that it is asked
  first. "A file its library tracks resolves from its library item" is
  untouched: an untracked copy resolves through the index as before.
- Duplicate checks, the acceptance gate, and minting. The gate extracts
  from a file whose extraction did not run, as today, even when the
  index's evidence for those bytes is `extracted`.
- `FileCache::clear`, `cache::inspect`, `borax cache`'s events, and the
  response cache.
- `status` and `validate` write nothing. `--no-record` still writes the
  operator's evidence at the move, as it writes the record today.
- `run-started`.

## Risks / Trade-offs

- **Offline resolution outside a library now needs the response
  cache.** Settled.
- **Recovery is by `reconcile` alone.** Until it runs, an interrupted
  operator admission's copies answer with the earlier extracted
  identifier (D5).
- **The library's operator decision overwrites a different one made for
  a copy outside the library**, at the next reconcile (D5).
- **An unreadable `operator.json` sends its bytes to extraction every
  run** until something rewrites it (D2).
- **Two interim states** inside the change (D12).
- **Reconcile reads one small file per distinct hash** (D5).

## Illustrative schema-5 lines

An evidence answer outside any library:

```json
{"schema":5,"event":"resolved","path":"/home/u/Downloads/paper.pdf","identifier":"doi:10.1039/c5ay00042d","record":{…},"library":{"status":"not-attempted","reason":"no-library"},"content_index":{"read":{"status":"hit","evidence":{"identifier":"doi:10.1039/c5ay00042d","origin":"extracted"}},"write":{"status":"not-attempted","reason":"content-index-hit"}},"extraction":{"result":{"status":"not-attempted","reason":"content-index-hit"},"titles":{"status":"not-attempted","reason":"content-index-hit"}},"identifier_input":{"status":"not-attempted","reason":"not-asked"},"lookup":{"status":"attempted","identifier":"doi:10.1039/c5ay00042d","origin":"content-index","attempts":[{"service":"crossref","outcome":{"status":"found","retrieval":"service-cache"}}]},"record_retrieval":{"kind":"service-cache","service":"crossref"},"match_check":{"status":"not-attempted","reason":"content-index-hit"},"acceptance":"automatic"}
```

Writes:

```json
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"evidence","identifier":"doi:10.1000/x","origin":"supplied"},"write":{"status":"written"}}
```

Adoption:

```json
{"schema":5,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"recorded","id":"0190…","item":"0190…","retrieval":{"kind":"service-cache","service":"crossref"}}}
{"schema":5,"event":"library-adoption","path":"scan.pdf","adoption":{"kind":"unanswered","identifier":"doi:10.1000/z"}}
{"schema":5,"event":"library-adoption","path":"other.pdf","adoption":{"kind":"unindexed"}}
```

Human lines:

```text
/home/u/Downloads/paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the response cache
scan.pdf: the content index knows its bytes as doi:10.1000/z, but no response cache holds that record, so it is still an orphan; borax resolve fetches it
```

On disk:

```text
~/.local/share/borax/v2/content/sha256-3f…/extracted.json   {"identifier":"doi:10.1000/y"}
~/.local/share/borax/v2/content/sha256-3f…/operator.json    {"identifier":"doi:10.1000/x","origin":"supplied"}
```

```toml
[[history]]
hash = "sha256-…"
run = "20261014T120000Z"
timestamp = "20261014T120000Z"
tool_version = "0.10.0"
item = "0190c3a2-7f21-7abc-8def-0123456789ad"
identifier = "doi:10.1000/x"
origin = "supplied"
```
