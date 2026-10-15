## Why

The content index recognises a file by its bytes. It does that today by
keeping a complete copy of the record borax resolved for them.
`ContentIndex` (`crates/borax-sources/src/store.rs`, line 362) stores a
serialized `Record` under `content/<hash>`, in the same `FileCache`
root as the response cache (`run.rs`, line 4035). That makes it two
things at once:

- **A second, stale authority.** A record corrected at its source, or
  in a library, is still the old copy in the index. A file outside the
  library with the same bytes is answered with that copy.
- **The only home of some operator decisions.** Outside a library, an
  identifier an operator supplied, or a record accepted over a title
  conflict, is kept nowhere else (`pipeline::remember`,
  `crates/borax/src/pipeline.rs`, line 968). `borax cache --clear`
  removes the whole cache root (`FileCache::clear`, `store.rs`, line
  157), and those decisions go with the service responses. The living
  requirement "borax adopt records what the library already holds" says
  outright that adoption after a clear records nothing.

Inside a library, an artifact record's history entry names the bytes
and, since change 15, the item they were accepted under. It does not
name the identifier they were identified by, or how. Change 15's
proposal deferred "Origins on disk" here. Worse, a re-identification of
a record's newest bytes moves the link and appends nothing
(`settle_history`, `crates/borax/src/library.rs`, line 1439), so the
operator's identifier reaches the library nowhere.

This is change 13a of the roadmap's Phase 4 ("library authority"). The
maintainer split change 13 after its critic gate. 13a reduces the index
to identification evidence and gives the library its half of every
operator decision. 13b (`index-references-libraries`) then adds library
references, and follows 13a. Changes 15 and 12 are done; change 14
follows 13b. Of the roadmap's done-when, 13a delivers two things:

- interrupt-safe writes, library first;
- the living requirement "An operator's answer about a file is
  remembered" naming the library as the durable home.

"Multiple references are preserved" is 13b's.

The Phase 4 library-authority design record (maintainer's notes,
outside the repository) settles what 13a preserves:

1. No operator decision lives only in a disposable cache. Inside a
   library, each artifact history entry records the identification
   evidence of its accepted bytes, the identifier and its origin, beside
   the item it was accepted under.
2. The reduced content index moves out of the cache root into the XDG
   data directory. Afterwards `borax cache --clear` removes service
   responses only.
3. There is no migration of `v1` entries, and no reader or importer for
   them. The reduced format may be called `v2` on disk.
4. Refresh is change 14's.

The record's defaults: an entry is keyed by content hash and holds
identification evidence. The origins are extraction, an operator's
identifier, an operator's acceptance over a conflict, and adoption. The
library is written before the index.

Outside a library, an entry answers with its identifier, which is then
looked up. 13a holds nothing else, so its answer is evidence-first. 13b
puts library references before the evidence, under the settled rule for
choosing among references.

The maintainer settled four further points after the critic gate:

- **No read-merge-write.** Each fact of an entry is its own file,
  written only by its own writer, by atomic replacement.
- **The append rule.** An admission whose decision carries evidence
  appends an entry unless the newest entry already holds the same item
  and evidence.
- **Recovery.** Recovery converges on the library's evidence, and
  `reconcile` restores it.
- **Schema.** Schema 5 is introduced whole here, and 13b only adds to it.

## What Changes

- **The index moves and is reduced.** It lives at
  `$XDG_DATA_HOME/borax/v2/`, else `$HOME/.local/share/borax/v2/`; on
  Windows at `%APPDATA%\borax\v2\`, else `XDG_DATA_HOME`. A file's
  entry is the directory `content/<hash>/`, holding:
  - `extracted.json`: the identifier an extraction found and a lookup
    confirmed, written only by resolution;
  - `operator.json`: an operator's identifier with origin `supplied` or
    `overridden`, written only for an operator's decision.

  `refs/` is reserved for 13b. No file holds a record, and no writer
  reads, merges and rewrites another's file. With no data directory, the
  index is held in memory for the run, as it is today with no cache
  directory. `borax cache --clear` therefore no longer touches the
  index. `v1` entries are neither read nor migrated. Design D1, D2.
- **Reading an entry.** The operator's evidence takes precedence over
  the extracted. An unreadable `operator.json` stops the entry from
  answering, since the extracted identifier beside it may be the one the
  operator corrected. No answer reads as a miss. Design D2.
- **History entries carry evidence.** An entry gains `identifier` and
  `origin`, both or neither, written only when present, as change 15's
  `item` is. The origins are `extracted`, `supplied`, `overridden` and
  `adopted`. An entry written before this change carries neither and
  reads unchanged. Design D3.
- **The append rule.** An admission whose decision carries evidence
  appends `(hash, item, evidence)` unless all three hold:
  - the bytes are the newest entry's;
  - they answer with that item afterwards;
  - that entry already carries the same evidence.

  So re-identifying the newest bytes records the operator's
  identifier. An ordinary re-run still appends nothing. Design D3.
- **Evidence answers.** For a file its library does not answer for, the
  entry's identifier is looked up through the response cache or the
  network, with origin `content-index`. Nothing is extracted and no
  title check is made, so an operator's acceptance stands. An identifier
  no service answers for is an `unresolvable` skip. Design D6.
- **Recovery.** `borax reconcile` writes each record's newest operator
  evidence, per hash, to `operator.json` wherever the index lacks it or
  differs. An operator admission interrupted before `remember` then
  ends where an uninterrupted one would. The library's decision wins
  over a different one made outside it. Design D5.
- **Adoption, offline.** An orphan's record comes from the response
  cache, by the entry's identifier, in resolution's service priority
  order, whatever `sources` says. It never comes from the network. The
  history entry gets origin `adopted`, and adoption writes nothing to the
  index. `recorded` gains `retrieval`. The new kind `unanswered` means
  evidence with no cached record, and `unindexed` keeps meaning "nothing
  in the index". Design D7.
- **Schema 5.** These released schema-4 values change meaning:
  - `content_index.read` `hit`;
  - `content_index.write`;
  - `content-index-write`;
  - `library-adoption` `unindexed`.

  `record_retrieval` `content-index` is removed. Added:
  - `hit`'s `evidence`;
  - `lookup.origin` `content-index`;
  - `content-index-write`'s `hash` and `entry`;
  - `recorded.retrieval`;
  - `unanswered`.

  There is no new not-attempted reason. Design D8.
- **Human lines.** An evidence answer ends `from the response cache` or
  `from the network`. There is a line for an `unanswered` orphan. The
  interactive description names an index-held identifier as found,
  supplied, or accepted over a conflict "in an earlier run". Design D9.
- **Interface.** Design D10.
  - `borax_core::library`: `EvidenceOrigin` and
    `IdentificationEvidence`; `HashEntry::evidence`;
    `ArtifactRecord::evidence_for` and `operator_evidence_for`.
  - A new `borax_core::index` module: `Fact` and `IndexEntry`.
  - `borax_sources::store`: `data_root` and `IndexStore`, implemented by
    `FileCache` and `MemoryCache`. `ContentIndex` keeps its type
    parameter, with `entry`, `put_extracted` and `put_operator`.

Explicitly out of scope:

- library references, and everything that reads, writes, restores or
  repairs them (13b). The design notes for 13b are kept outside the
  repository;
- refresh (change 14);
- a command to inspect, prune or rebuild the index beyond what
  `reconcile` restores;
- the Windows collision of the state and cache roots (Deferred).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "The content index keeps identification evidence, one fact per
      file": what an entry holds and where, its writers and readers,
      precedence, durability, and what the remaining "record in the
      content index" phrases mean
    - "A content-index entry answers with its identifier": the lookup
      with no extraction and no title check, and `unresolvable`
  - MODIFIED:
    - "Responses are cached locally": the index is not a cache, and
      survives `cache --clear`
    - "A resolution reports the evidence it was checked against": the
      content-index scenarios name the lookup's service
    - "An operator's answer about a file is remembered": the library is
      the durable home, and reconcile restores the index's copy; two
      scenarios are added
    - "A resolution says whether its library answered": schema 5, and
      the problem line ends `from the response cache`
    - "A resolution keeps every service it asked, in order": the content
      index as an identifier's origin, and retry keeping it
    - "A resolution says where its record came from": the content index
      is no longer a place a record comes from
    - "A resolution says what the content index did": what an answer
      is, and what is written
    - "A resolution says why a step was not taken": a content-index
      answer makes its lookup
    - "Resolution events carry their evidence in sections": schema 5,
      with the released meanings that changed
    - "A resolution reports what the content index did and every cache
      write": `hit`'s `evidence`
    - "Each service asked is reported with its outcome": `origin`
      `content-index`
    - "A resolution reports where its record was retrieved":
      `content-index` is not reported
    - "What became of remembering an operator's answer is reported": the
      event's `hash` and `entry`, and reconcile's restorations
    - "Human resolution lines name the work, where it came from, and why
      a file was skipped": `the content index` is never a retrieval
      place
- `library`:
  - ADDED: "A library's operator decisions are restored to the content
    index": what reconcile writes, when, and what wins
  - MODIFIED:
    - "An artifact record is authoritative state keyed by an artifact
      identity": entries carry identification evidence; Rule 1's
      exception requires the same evidence
    - "An applying run records what it admits": re-identifying the
      newest bytes appends the operator's entry
    - "The library's text is authoritative and is read directly": the
      content index is not a cache, and is derived from the library
    - "Validation defines a well-formed library": evidence is never a
      finding; malformed evidence makes a record unparseable
    - "borax adopt records what the library already holds": the response
      cache by evidence, `unanswered`, and no writes to the index
- `rename`, MODIFIED:
  - "An interactive question shows what the answer rests on": an
    index-held identifier is named as an earlier run's
  - "An interactive question says whether the library answered": its
    fallback scenario

Requirements checked and left unchanged:

- `resolution`:
  - "A file its library tracks resolves from its library item". An
    untracked copy still resolves through the index, extraction and the
    services, and nothing here answers it through the library.
  - "Ambiguity is skipped, never guessed". Under the ADDED definition,
    the accepted record's evidence is what is written, and the ADDED
    answering requirement does not judge it again.
  - "A library that cannot answer for a file says so". "Resolves from
    the content index" reads through the ADDED definition.
  - "A failed cache write is evidence, not a failure". It covers every
    index write, restorations included.
  - "A refused record is kept as evidence and never remembered". No
    evidence is written for a refused record.
  - "A resolution says what the title check concluded". Its
    content-index scenario holds.
- `library`:
  - "Reconciliation repairs a stale path by a bounded walk". Restoration
    writes the index, not library state, so an untouched library still
    gets no diff.
  - "A library carries an identity that does not depend on where it is"
    and "A library's first store write mints its identity". 13a does not
    read the identity.
- `ledger`, "Duplicate detection operates at two levels with distinct
  reasons". Its scenario "Re-running over an admitted file" still holds:
  the library answers for an admitted file, and no source is queried.
- `extraction`: `content-index-hit` remains the reason extraction and
  titles are not attempted on an index answer.
- `bib-output`: "the content index" as a source of knowledge reads
  through the ADDED definition.
- `cli`:
  - "JSON Lines output is first-class" is applied, not changed.
  - "Single binary with subcommands" already says `cache` clears "the
    response cache".
  - "A subcommand accepts only the settings it consumes" holds: `adopt`
    reads every service's cached answer, whatever `sources` names.
- `run-logs`: unchanged.

## Impact

- `crates/borax-core/src/library.rs`:
  - `EvidenceOrigin` and `IdentificationEvidence`;
  - `HashEntry::evidence` and its parse rule;
  - `ArtifactRecord::evidence_for` and `operator_evidence_for`.
- `crates/borax-core/src/index.rs` (new): `Fact`, `IndexEntry` and the
  fact JSON.
- `crates/borax-sources/src/store.rs`:
  - `INDEX_FORMAT_VERSION`, `data_root` and `default_data_root`;
  - `extracted_key` and `operator_key`;
  - `IndexStore`, and `ContentIndex`'s evidence API;
  - the record API is removed in group 4;
  - `FORMAT_VERSION`'s docstring stops naming the index.
- `crates/borax-sources/src/cache.rs`: `MemoryCache`'s second map.
- `crates/borax/src/evidence.rs`: `IndexRead::Hit(evidence)` and
  `Origin::Indexed`; `RecordRetrieval::ContentIndex` is removed.
- `crates/borax/src/pipeline.rs`:
  - the index read answers by evidence;
  - the write is of extracted evidence;
  - `FileRecord::identification`;
  - `remember(index, file)`.
- `crates/borax/src/library.rs`:
  - `Admitting::evidence`, and the append rule in `settle_history`;
  - reconcile's accepted change records extracted evidence;
  - `history_table` writes the two keys;
  - `adopt`'s `lookup` returns `Result<Adopted, Unadopted>`;
  - `operator_decisions`.
- `crates/borax/src/event.rs`:
  - `SCHEMA = 5`;
  - `IndexReadStep::Hit`'s `evidence`;
  - `IdentifierOrigin::ContentIndex`;
  - `RetrievedFrom::ContentIndex` is removed;
  - `ContentIndexWrite`'s `hash` and `entry`;
  - `Adoption::Recorded::retrieval` and `Adoption::Unanswered`;
  - the human lines.
- `crates/borax/src/describe.rs`: the identifier line (D9).
- `crates/borax/src/run.rs`:
  - `execute` opens the index over the data root;
  - `Adapters<S: IndexStore>`;
  - the rename driver passes evidence to admissions;
  - `adopt_events` reads the response cache;
  - `reconcile_events` restores operator evidence;
  - retry repeats a content-index lookup.
- Tests: design D14 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`:
    - "What a run does";
    - `borax resolve`'s retrieval clause;
    - `borax rename`'s passages on remembering an answer;
    - `borax cache`;
    - `borax reconcile`;
    - `borax adopt`;
    - "Choosing how to record existing files";
    - the `cache` setting;
    - "Run logs", for schema 5;
  - `CHANGELOG.md`: an Unreleased entry.
- `openspec/STATE.md` (implementer): a paragraph for this change, and
  the Windows defect.
- No new dependency, configuration key, flag or warning.

## Deferred

- **Library references** (13b): reading, agreement, writing,
  restoration, root repair, and adoption through a reference.
- **Restoring extracted evidence from the library.** Resolution
  re-derives it. Only operator evidence is restored.
- **The Windows state and cache roots.** Both are `LOCALAPPDATA\borax\v1`
  (`runlog.rs`, line 79; `store.rs`, line 69), so `borax cache --clear`
  there removes the run logs of runs outside a library. That goes
  against the `cli` requirement that `cache` clears the response cache,
  and the `run-logs` requirement that such a run "leaves a readable
  record". The fix is a restoration, not this change's. It was found
  while choosing D1 and is not verified on Windows, which CI does not
  run.
- **Inspecting the index.** `borax cache` reports the response cache
  only.
