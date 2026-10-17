## Why

Since `reduce-content-index` (13a), the content index keeps one thing
about a file's bytes outside a library: identification evidence, an
identifier that is looked up again (`borax_core::index::IndexEntry`,
`crates/borax-core/src/index.rs`, line 33). The entry directory
`content/<hash>/refs/` is reserved and nothing writes or reads it. So a
library's corrections never reach a copy of its files elsewhere:

- **A library's answer stops at its root.** An item corrected by hand
  is what the library answers for its own files
  (`Stores::consult`, `crates/borax/src/library.rs`, line 1130), but a
  copy of the same bytes outside the library is answered by looking up
  the index's identifier again (`pipeline::beyond_library`,
  `crates/borax/src/pipeline.rs`, line 711). The copy gets the
  service's record, not the library's.
- **Adoption cannot use another library.** `borax adopt` finds an
  orphan's record only in the response cache, by the index's
  identifier (`run::held_answer`, `crates/borax/src/run.rs`, line
  4069). A file another library already holds, with its item, is
  `unanswered` after `borax cache --clear`.
- **A library's identity names it nowhere.** Change 12 gave every
  library a location-independent identity (`library::identity`,
  `library.rs`, line 381) so that another location could name it. No
  location does yet.

This is change 13b of the roadmap's Phase 4 ("library authority"), the
last part of change 13 and the one before change 14 (refresh). It adds
library references to the reduced index. Of the roadmap's done-when it
delivers what 13a left: multiple references are preserved, and
unavailable ones are kept and reported rather than pruned. 13a already
made the living requirement "An operator's answer about a file is
remembered" name the library as the durable home, and this change keeps
that.

The Phase 4 library-authority design record (maintainer's notes,
outside the repository) settles what 13b preserves:

1. One reachable reference answers from its item. Several reachable
   references answer only when their items carry the same preferred
   identifier. Otherwise none is chosen, the entry's own evidence is
   looked up, and the event names the disagreeing references.
2. Two locations of one library are two references.
3. A run inside a library answers for its own tracked files.
4. No migration, and no format-version handling.

Its defaults, kept here:

- A reference names the library identity, its last-known root, the
  artifact and the item.
- The item and the artifact record are written first, the reference
  after.
- An unavailable reference is preserved, never pruned automatically,
  and its cause is reported: the root is missing; it holds no library,
  or another library; or it cannot be read.
- A run inside the library repairs a stale root. This change names
  which runs.

The maintainer settled four further points for this change:

- **Precedence.** References come before evidence for every file the
  run's library does not answer for. 13a called outside answers
  "evidence-first" only because it held nothing else. This change
  supersedes that wording: the order is reference, then evidence, then
  extraction. The design record's "begins with its identifier"
  describes an entry with no reference that answers.
- **Identity.** A reference is written only after the artifact record it
  names is on disk, and only when the library's identity, read at that
  moment, is `Present`. A successful admission does not imply one, and
  nothing mints an identity for a reference.
- **No read-merge-write.** Each reference is its own file under
  `content/<hash>/refs/`, named so that one (library, root, artifact)
  key maps to exactly one file name. It is created or replaced by atomic
  replacement and never merged. A stale-root repair writes the new
  reference's file, then removes the stale one's.
- **Recovery claims match what is visited.** `borax reconcile` is the
  restoring command, over every hash of every history. `borax adopt`
  restores nothing for files already recorded.

## What Changes

- **References on disk.** `content/<hash>/refs/<name>.json` holds
  `library`, `root`, `artifact` and `item`. `<name>` is the
  `sha256-<hex>` of the library identity, the root and the artifact
  identity joined by NUL, so a key has one file name and the item does
  not change it. Each file is replaced whole by its writer, and no
  writer reads another fact to decide what to write. Design D1.
- **Who writes them, and when.** Three writers:
  - an applying rename, after each admission that returns, for the
    admitted bytes;
  - `borax adopt`, for each orphan it records;
  - `borax reconcile`, for every hash in every history.

  Each writer:
  - reads the library's identity first, and writes nothing unless it is
    `Present`;
  - names the item `ArtifactRecord::item_for` gives for the bytes after
    the write;
  - writes nothing for bytes whose record answers with no item.

  Nothing reports an absent or unreadable identity beyond what change
  12 already reports. Design D2.
- **Writing is an upsert, and it repairs.** A reference already held
  with the same item is left as it is, and nothing is reported for it.
  A different item, or an unreadable file, is replaced. Where the key
  has no file yet, every reference to the same library and artifact at
  a root that is now missing, or that holds no library or another one,
  is stale. The new file is written, then each stale one removed. A
  reference whose own key already has a file is never removed, and
  neither is a reference to an unreadable root. Design D3.
- **Reading them.** For a file the run's library does not answer for,
  with the cache on and the file hashed, each reference is reached:
  - a missing root is `unavailable` `missing`;
  - a root that holds no library, or another one, is `unavailable`
    `other-library`;
  - an unreadable root or identity file is `unavailable` `unreadable`;
  - an item no single readable file carries is `item-unavailable`;
  - otherwise the reference is reachable, and the item it stores
    answers.

  Each root is read at most once per run. Reading writes nothing.
  Design D4.
- **Answering.** Items agree when they carry the same preferred
  identifier (DOI, then arXiv, PMID, ISBN), or carry none and are one
  item of one library. When one reference is reachable, or several
  agree, the first in entry order answers. Entry order is by root, then
  library, then artifact. That answer looks nothing up, extracts
  nothing, checks no title and writes no evidence. Otherwise the entry's
  evidence answers as in 13a, and failing that, extraction. A tracked
  file never reads its entry; an untracked copy inside a library does.
  Design D5.
- **Adoption.** A reference that answers comes before the response
  cache. Its record is adopted with retrieval `library-reference` and
  no evidence on the history entry. `unanswered` now also covers an
  entry whose references did not answer, with or without an
  identifier. `adopt` writes the references of what it records. Design
  D6.
- **Schema 5, added to.** No release carries schema 5, so the `cli` rule
  lets it change. Every line a 13a build writes for an entry with no
  reference is unchanged.
  - Added: `references` on `content_index.read` `hit` and `miss`; the
    retrieval `library-reference`; the entry kind `reference` with
    `repaired_from`; `references` on `unanswered`.
  - Changed meanings 13a defined:
    - a `hit` answered by a reference carries no `evidence`, and is
      followed by no lookup;
    - `content-index-write` reports reference writes, in batch renames
      and adoptions too;
    - `unanswered` may carry no `identifier`;
    - `adopt` writes to the index.

  Design D7.
- **Human lines and the description.** The retrieval clause becomes
  `from the library at <root>`. One clause names each reference that
  could not answer, and one names a disagreement. A failed reference
  write has a line of its own. The interactive description names the
  referenced library on its `record` line, and adds `reference` and
  `references` lines. Design D8.
- **Interface.** Design D9.
  - `borax_core::index`: `LibraryReference` and its JSON and name.
  - `borax_sources::store`: `IndexStore` gains `list_facts` and
    `remove_fact`, and `ContentIndex` gains `references`, `reference`,
    `put_reference` and `remove_reference`.
  - A new `borax::references` module.

Explicitly out of scope:

- refresh (change 14);
- pruning, listing or inspecting references: any later pruning is an
  explicit operation of its own;
- restoring extracted evidence (13a's Deferred);
- the Windows collision of the state and cache roots (13a's Deferred).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `resolution`:
  - ADDED:
    - "The content index keeps references to libraries, one file each":
      the layout, the file, its writers, durability
    - "A library reference answers for a file before the entry's
      evidence": reaching a reference, agreement, precedence, inside a
      library
    - "What became of a library reference write is reported": the
      `content-index-write` event of kind `reference`, and its order
  - MODIFIED:
    - "Responses are cached locally": a renamed file whose bytes a
      library holds
    - "A resolution reports the evidence it was checked against": a
      referenced library's provenance
    - "An operator's answer about a file is remembered": references
      answer before the evidence
    - "A file its library tracks resolves from its library item": the
      scenario "A copy is not the artifact"
    - "A resolution says where its record came from": a referenced
      library's item
    - "A resolution says what the content index did": answering through
      a reference
    - "A resolution says why a step was not taken": a reference answer
      makes no lookup
    - "Resolution events carry their evidence in sections": 13b's
      additions to schema 5, and the meanings that changed
    - "A resolution reports what the content index did and every cache
      write": `hit` and `miss` with `references`
    - "A resolution reports where its record was retrieved":
      `library-reference`
    - "What became of remembering an operator's answer is reported": its
      order after a reference write, and a batch run's events
    - "Human resolution lines name the work, where it came from, and why
      a file was skipped": `the library at <root>`, the reference
      clauses, the failed reference line
    - "The content index keeps identification evidence, one fact per
      file": `refs/` is written and read now
    - "A content-index entry answers with its identifier": only where no
      reference answers
- `library`:
  - ADDED: "A library's references are written after its records, and
    reconcile restores and repairs them"
  - MODIFIED:
    - "The library's text is authoritative and is read directly":
      references are derived from the library too
    - "borax adopt records what the library already holds": references
      first, `unanswered` extended, references written
- `rename`, MODIFIED: "An interactive question shows what the answer
  rests on": a reference answer's identifier, `record` line and
  reference lines

Requirements checked and left unchanged:

- `resolution`:
  - "A library that cannot answer for a file says so": resolving the
    file "as untracked" goes through the content index, references
    included.
  - "A resolution says whether its library answered": it is about the
    run's library. A reference answer leaves that section as the
    library said, `untracked` for a copy inside it, and is reported by
    `record_retrieval` alone.
  - "A failed cache write is evidence, not a failure": it covers every
    index write, reference writes included.
  - "A refused record is kept as evidence and never remembered", and
    "Ambiguity is skipped, never guessed": a reference answer makes no
    title check, and so refuses nothing.
  - "Each service asked is reported with its outcome": a reference
    answer asks no service, and `lookup` is not attempted.
- `library`:
  - "An artifact record is authoritative state keyed by an artifact
    identity": an entry adopted or admitted through a reference is for
    "bytes no identifier established", and carries no evidence.
  - "A library's operator decisions are restored to the content index":
    its restoration is of operator evidence, and reference restoration
    is the ADDED requirement's.
  - "A library carries an identity that does not depend on where it
    is", and "A library's first store write mints its identity": a
    reference reads the identity and never mints one. Nothing reports
    an absent identity, which "nothing reports it as incomplete"
    requires.
  - "An applying run records what it admits", and "Changed bytes become
    an artifact's history only once accepted": a pending, outside or
    unwritten admission writes no reference.
  - "Reconciliation repairs a stale path by a bounded walk": reference
    writes are to the index, so an untouched library still gets no
    diff.
- `rename`:
  - "An interactive question says whether the library answered": it is
    about `record_retrieval` `library`, the run's own.
  - "An interactive description says whether the file's titles were
    read": a reference answer keeps `not read; an earlier run answered`.
  - "A candidate the operator abandoned leaves nothing behind": no
    admission, so no reference.
- `cli`, "JSON Lines output is first-class": applied, not changed.
- `extraction`, `ledger`, `bib-output`, `run-logs`: unchanged.

## Impact

- `crates/borax-core/src/index.rs`: `LibraryReference`,
  `reference_to_json`, `reference_from_json`,
  `LibraryReference::name`.
- `crates/borax-sources/src/store.rs`:
  - `references_prefix` and `reference_key`;
  - `IndexStore::list_facts` and `remove_fact`, for `FileCache`;
  - `ContentIndex`'s reference API.
- `crates/borax-sources/src/cache.rs`: `MemoryCache`'s `list_facts` and
  `remove_fact`.
- `crates/borax/src/references.rs` (new):
  - `Libraries`, `Reach`, `ReferenceRead`, `ReferenceStatus`;
  - `settle`, `read` and `keep`.
- `crates/borax/src/library.rs`:
  - `Admitted::answers_with`;
  - `ItemStore::answer`;
  - `OwedReference` and `references_owed`;
  - `Adopted` and `Unadopted::Unanswered` reshaped, `Adoptions::owed`.
- `crates/borax/src/evidence.rs`:
  - `IndexRead::Referenced`;
  - `IndexEvidence::references`;
  - `RecordRetrieval::LibraryReference`.
- `crates/borax/src/pipeline.rs`:
  - `standing_in`, and `beyond_library` reading references;
  - `preferred_identifier`;
  - `resolve_batch` sharing one `Libraries`.
- `crates/borax/src/event.rs`:
  - `IndexReadStep::Hit`'s optional `evidence`, and `references` on
    `Hit` and `Miss`;
  - `ReferenceStep`, `RetrievedFrom::LibraryReference`,
    `WrittenEntry::Reference`;
  - `Adoption::Unanswered` reshaped;
  - the human lines.
- `crates/borax/src/describe.rs`: the `record` line, and the
  `reference` and `references` lines.
- `crates/borax/src/run.rs`:
  - `IndexHome`'s two new methods;
  - the rename driver's and `bib`'s shared `Libraries`;
  - the rename driver writes references;
  - `reconcile_events` restores references;
  - `adopt_events` reads and writes them.
- Tests: design D13 lists every existing test that changes, and how.
- Documents a person reads, written by the doc writer:
  - `docs/manual.org`;
  - `CHANGELOG.md`.
- `openspec/STATE.md` (implementer).
- No new dependency, configuration key, flag or warning.

## Deferred

- **Pruning a reference.** An unavailable reference stays until an
  explicit operation removes it. None exists yet. A copy deleted after
  it was referenced is reported `missing` by every outside run that
  reaches its bytes.
- **Inspecting the index.** Nothing lists an entry's references except
  a resolution that reads them.
- **A reference whose stale-root removal failed.** Both files stay,
  and no later run removes the stale one (D3).
