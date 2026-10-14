## Why

The content index recognises a file by its bytes. Today it does that by
keeping a complete copy of the record borax resolved for them:
`ContentIndex` (`crates/borax-sources/src/store.rs`, line 362) stores a
serialized `Record` under `content/<hash>` in the same `FileCache` root
as the response cache (`run.rs`, line 4035). That makes it two things
at once:

- **A second, stale authority.** A record corrected in a library is
  still the old record in the index. A file outside the library, such
  as a copy or a download of the same bytes, is answered with the old
  copy.
- **The only home of some operator decisions.** Outside a library, an
  identifier an operator supplied, or a record accepted over a title
  conflict, is kept nowhere else (`pipeline::remember`,
  `crates/borax/src/pipeline.rs`, line 968). `borax cache --clear`
  removes the whole cache root (`FileCache::clear`, `store.rs`, line
  157), so it deletes those decisions with the service responses. The
  living requirement "The library's text is authoritative and is read
  directly" calls the index a cache, and the requirement "borax adopt
  records what the library already holds" says plainly that adoption
  after a clear records nothing.

Inside a library, an artifact record's history entry names the bytes
and, since change 15, the item they were accepted under. It does not
name the identifier they were identified by or how (change 15's
proposal deferred "Origins on disk" here). So the library cannot yet be
the durable home the design record makes it.

This is change 13 of the roadmap's Phase 4 ("library authority"), the
third of its four. Changes 15 (`separate-change-from-acceptance`) and 12
(`library-identity`) are done; change 14 (`refresh-as-candidate`)
follows. The roadmap's done-when: migration is interrupt-safe, multiple
references are preserved, and the living requirement "An operator's
answer about a file is remembered" names the library as the durable
home for accepted associations. "Migration" means the write order for
new decisions, since nothing old is migrated (settled, below).

The Phase 4 library-authority design record (maintainer's notes,
outside the repository) settles the following, and this change
preserves it:

1. No operator decision lives only in a disposable cache. Inside a
   library, each artifact history entry records the identification
   evidence of its accepted bytes, the identifier and its origin, beside
   the item it was accepted under.
2. The reduced content index moves out of the cache root into the XDG
   data directory. Afterwards `borax cache --clear` removes service
   responses only.
3. Several reachable library references answer only when their items
   carry the same preferred identifier. Otherwise none is chosen, the
   entry's own identification evidence is looked up, and the event names
   the disagreeing references. Two locations of one library are two
   references. A run inside a library is not ambiguous for its own
   tracked files.
4. No migration of `v1` entries, and no reader or importer for them. The
   reduced format may be called `v2` on disk.
5. Refresh is change 14's.

Its defaults, kept here and refined only in spelling: an entry holds
identification evidence and library references; outside a library an
entry's answer begins with its identifier; an unavailable reference is
preserved, its cause reported, and its stale root repaired by a run
inside the library; the write order is item, then artifact record with
its evidence, then reference; the four origins are extraction, an
operator's identifier, an operator's acceptance over a conflict, and
adoption.

## What Changes

- **The index moves and shrinks.** It lives at `$XDG_DATA_HOME/borax/v2/`
  (`$HOME/.local/share/borax/v2/` without it; `%APPDATA%\borax\v2\`, then
  `XDG_DATA_HOME`, on Windows), one JSON file per content hash under
  `content/`. An entry holds identification evidence (an identifier and
  its origin) and zero or more library references (library identity,
  last-known root, artifact, item), and no record. With no data
  directory it is held in memory for the run, as the index is today
  with no cache directory. The response cache stays where it is, and
  `borax cache --clear` therefore no longer touches the index. Old `v1`
  entries are neither read nor migrated. Design D1, D2.
- **Origins.** `extracted`, `supplied`, `overridden` (an operator's
  acceptance over a conflict) and `adopted`. Design D3.
- **History entries carry evidence.** A history entry gains `identifier`
  and `origin`, written only when present, both or neither, as change
  15's `item` is. An entry written before carries none and reads
  unchanged. Design D3.
- **Evidence is written where records were.** The resolution write and
  `remember` at the move write evidence, not records. Evidence of an
  operator's origin is never replaced by extracted evidence: a
  `--no-cache` run reports that write not attempted for the new reason
  `operator-evidence`. Design D4.
- **References follow the library.** A reference is written only after
  the artifact record it names is on disk, and only when the library's
  identity, read at that moment, is present:
  - after an applying rename's admission, written or unchanged;
  - after an adoption;
  - by `borax reconcile`, for every record's newest bytes, which
    restores what an interruption or a deleted index lost.

  A missing identity warns once and mints nothing. The write is idempotent,
  repairs a reference whose root no longer holds that library, and never
  removes one. Design D5, D6.
- **What an index entry answers.** For a file the run's library does not
  answer for, the entry's references are tried first: one reachable
  reference, or several that agree, answer with the referenced item, no
  lookup and no open. Otherwise the entry's identifier is looked up
  through the response cache or the network, with no extraction and no
  title check; an operator's acceptance stands. Otherwise extraction
  runs as on a miss. Each reference is reported `answered`, `agreed`,
  `disagreeing`, `unavailable` (`missing`, `other-library`,
  `unreadable`) or `item-unavailable`. Design D7, D8.
- **Adoption, offline.** An orphan's record comes from a reference that
  answers, else from the response cache by the entry's identifier, never
  the network. `recorded` names where it came from; an entry that cannot
  supply a record is the new `unanswered`, and `unindexed` keeps meaning
  "no entry". Design D9.
- **Schema 5.** Released schema-4 values change meaning or go:
  - `content_index.read` `hit`, `content_index.write` and
    `content-index-write` change meaning;
  - `record_retrieval` `content-index` goes.

  `hit` gains `evidence` and `references`; `lookup.origin` gains
  `content-index`; `record_retrieval` gains `library-reference`;
  `content-index-write` gains `hash` and `entry`; `library-adoption`
  gains `retrieval` and `unanswered`. Design D10.
- **Human lines.** `from the library at <root>`, a clause per
  unavailable reference and one for disagreement, a line for a reference
  that could not be kept, the `unanswered` adoption line, and the
  no-identity warning. The interactive description names an
  index-held identifier's origin and a reference's root. Design D11.
- **Interface.** In `borax_core::library`:
  - `EvidenceOrigin` and `IdentificationEvidence`;
  - `HashEntry::evidence` and `ArtifactRecord::evidence_for`.

  A new `borax_core::index` holds `LibraryReference` and `IndexEntry`.
  `borax_sources::store` gains `data_root` and `IndexStore`, implemented
  by `FileCache` and `MemoryCache`, over which `ContentIndex` is now
  generic. A new `borax::references` module is added. Design D12.

Explicitly out of scope:

- refresh (change 14);
- a command to show, prune or rebuild the index, beyond what `borax
  reconcile` restores;
- minting a library identity for a reference;
- naming the library's identity on `run-started`, which change 12
  deferred here: a reference reads the identity when it is written, so
  nothing needs it there;
- the Windows collision of the state and cache roots (Deferred).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "The content index keeps identification evidence and library
      references": what an entry holds, where, its durability, which
      writes make evidence, the operator-evidence rule, and what the
      remaining "record in the content index" phrases mean
    - "A content-index entry answers through its references, then its
      identifier": outcomes, agreement, the evidence fallback without a
      title check, and the extraction fallback
  - MODIFIED:
    - "Responses are cached locally": the index is not a cache, and
      survives `cache --clear`
    - "A resolution reports the evidence it was checked against": the
      content-index scenarios answer through a reference
    - "An operator's answer about a file is remembered": the library is
      the durable home; evidence and references; two scenarios added
    - "A file its library tracks resolves from its library item": an
      untracked copy can be answered by the library's reference
    - "A resolution says whether its library answered": schema 5, and
      the problem line ends `from the response cache`
    - "A resolution keeps every service it asked, in order": the content
      index as an identifier's origin
    - "A resolution says where its record came from": a library
      reference replaces the content index as a place
    - "A resolution says what the content index did": an entry held,
      and `operator-evidence`
    - "A resolution says why a step was not taken": the content-index
      scenario split in two
    - "Resolution events carry their evidence in sections": schema 5,
      `operator-evidence`, two scenarios
    - "A resolution reports what the content index did and every cache
      write": `hit`'s `evidence` and `references`
    - "Each service asked is reported with its outcome": `origin`
      `content-index`
    - "A resolution reports where its record was retrieved":
      `library-reference`
    - "What became of remembering an operator's answer is reported": the
      event covers reference writes
    - "Human resolution lines name the work, where it came from, and why
      a file was skipped": `the library at <root>` and the reference
      clauses
- `library`:
  - ADDED: "A library decision is referenced from the content index
    after it is written": when, under which identity, in what order,
    restoration, repair, and no pruning
  - MODIFIED:
    - "An artifact record is authoritative state keyed by an artifact
      identity": entries carry identification evidence
    - "The library's text is authoritative and is read directly": the
      content index is not a cache, and is derived from the library
    - "Validation defines a well-formed library": evidence is never a
      finding; malformed evidence is an unparseable record
    - "borax adopt records what the library already holds": references,
      the response cache, `unanswered`
- `rename`, MODIFIED:
  - "An interactive question shows what the answer rests on": an
    index-held identifier's origin, and a reference's root
  - "An interactive question says whether the library answered": its
    fallback scenario

Requirements checked and left unchanged:

- `resolution`:
  - "Ambiguity is skipped, never guessed". Under the ADDED definition,
    the accepted record's evidence is what is written, and the ADDED
    answering requirement does not judge it again.
  - "A library that cannot answer for a file says so". "Resolves from
    the content index" now reads through the ADDED definition. Its kinds
    and fallback are unchanged.
  - "A failed cache write is evidence, not a failure". Reference writes
    are covered by "every write borax makes to either store".
  - "A refused record is kept as evidence and never remembered". No
    evidence is written for a refused record, which is what "never
    remembered" now means.
  - "A resolution says what the title check concluded". Its content-index
    scenario holds for both kinds of answer.
- `library`:
  - "An applying run records what it admits". The reference comes after
    the writes it governs, under the ADDED requirement, and `--no-record`
    makes no admission.
  - "Reconciliation repairs a stale path by a bounded walk". Restoring
    references writes to the index, not library state, so an untouched
    library still gets no diff.
  - "A library carries an identity that does not depend on where it is"
    and "A library's first store write mints its identity". References
    read the identity and never mint it.
- `ledger`, "Duplicate detection operates at two levels with distinct
  reasons". The scenario "Re-running over an admitted file" still holds:
  the library answers for an admitted file, and no source is queried.
- `extraction`: `content-index-hit` remains the reason extraction and
  titles are not attempted on either kind of index answer.
- `bib-output`: "the content index" as a source of knowledge reads
  through the ADDED definition.
- `cli`:
  - "JSON Lines output is first-class" is applied, not changed;
  - "Single binary with subcommands" already says `cache` clears "the
    response cache";
  - "A subcommand accepts only the settings it consumes" holds, because
    `adopt` reads every service's cached answer whatever `sources`
    names.
- `run-logs`: unchanged.

## Impact

- `crates/borax-core/src/library.rs`: `EvidenceOrigin`,
  `IdentificationEvidence`, `HashEntry::evidence` and its parse rule,
  `ArtifactRecord::evidence_for`.
- `crates/borax-core/src/index.rs` (new): `LibraryReference`,
  `IndexEntry` and `Referred`, with the evidence rule and the upsert.
- `crates/borax-sources/src/store.rs`: `INDEX_FORMAT_VERSION`,
  `data_root`, `default_data_root`, `IndexStore`, `ContentIndex` over it.
  `FORMAT_VERSION`'s docstring loses the index.
- `crates/borax-sources/src/cache.rs`: `MemoryCache`'s second map.
- `crates/borax/src/references.rs` (new): `Libraries`, `answer`,
  `refer`, `restore`, `adoptable`, `unreferenced_warning`.
- `crates/borax/src/evidence.rs`:
  - `IndexRead::Hit(IndexHit)`;
  - `Origin::Indexed`;
  - `RecordRetrieval::LibraryReference`;
  - `Unattempted::OperatorEvidence`.
- `crates/borax/src/pipeline.rs`:
  - the index read answers by references, then evidence;
  - the write is of evidence;
  - `FileRecord::identification`;
  - `remember(index, file)`.
- `crates/borax/src/library.rs`:
  - `Admitting::evidence`, written by `admit` and by reconcile's accepted
    change;
  - `history_table` writes the two keys;
  - `adopt`'s `lookup` returns `Result<Adopted, Unadopted>`.
- `crates/borax/src/event.rs`:
  - `SCHEMA = 5`;
  - `ReferenceOutcome` and its statuses;
  - `IndexReadStep::Hit`'s fields;
  - `IdentifierOrigin::ContentIndex`;
  - `RetrievedFrom::LibraryReference`, with `ContentIndex` removed;
  - `ContentIndexWrite`'s `hash` and `entry`;
  - `Adoption::Recorded::retrieval` and `Adoption::Unanswered`;
  - the human lines.
- `crates/borax/src/describe.rs`: the identifier and record lines (D11).
- `crates/borax/src/run.rs`:
  - `execute` opens the index over the data root;
  - `Adapters<S: IndexStore>`;
  - the rename driver writes references after admissions and passes a
    `Libraries`;
  - `adopt_events` uses `adoptable` and writes references;
  - `reconcile_events` calls `restore`;
  - retry repeats a content-index lookup;
  - `Aftermath::unreferenced`.
- Tests: design D15 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`:
    - "What a run does";
    - `borax resolve`'s retrieval clause;
    - `borax rename`'s interactive passages on remembering an answer;
    - `borax cache`;
    - `borax reconcile`;
    - `borax adopt`;
    - "Choosing how to record existing files";
    - the `cache` setting;
    - "Run logs", for schema 5;
  - `CHANGELOG.md`: an Unreleased entry.
- `openspec/STATE.md` (implementer): a paragraph for this change.
- No new dependency, configuration key or flag. One run warning is
  added.

## Deferred

- **Pruning references.** An unavailable reference stays until an
  explicit operation removes it, and none exists yet.
- **Minting for references.** A library no store write has reached since
  0.10 gets no references. Closing that reopens change 12's minting
  rule.
- **The Windows state and cache roots.** Both are
  `LOCALAPPDATA\borax\v1` (`runlog.rs`, line 79; `store.rs`, line 69),
  so `borax cache --clear` there removes the run logs of runs outside a
  library, against the `cli` requirement that `cache` clears the
  response cache and the `run-logs` requirement that such a run "leaves
  a readable record". That defect is not this change's, and fixing it
  is a restoration. Found while choosing D1, it is not verified on
  Windows, which CI does not run.
- **Inspecting the index.** `borax cache` reports the response cache
  only, and nothing reports the index's size or its references.
