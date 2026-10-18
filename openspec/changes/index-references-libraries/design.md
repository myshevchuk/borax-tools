# Design: index-references-libraries

## Context

Everything below was read from source at `2ab0034`, the merge of
`reduce-content-index`. This is change 13b of the roadmap's Phase 4, the
second half of change 13. 13a (`reduce-content-index`) is archived at
`openspec/changes/archive/2026-10-17-reduce-content-index/`, and change
14 (`refresh-as-candidate`) follows this one.

**Settled by the maintainer** (the Phase 4 library-authority design
record, maintainer's notes, outside the repository, and the split of
change 13):

1. A reference names the library identity, its last-known root, the
   artifact identity and the item identity.
2. A reference is written only after the artifact record it names is on
   disk, and only when the library's identity, read at that moment, is
   `Present`. Change 12 said a successful admission does not imply one,
   and nothing mints an identity for a reference.
3. One reachable reference answers from its item. Several reachable
   references answer only when their items carry the same preferred
   identifier. Otherwise none is chosen, the entry's own evidence is
   looked up, and the event names the disagreeing references. Two
   locations of one library are two references.
4. A run inside a library answers for its own tracked files. References
   come before evidence for everything else. 13a was evidence-first only
   because it held nothing else, and this change states the new order
   as superseding 13a's wording.
5. Unavailable references are preserved and never pruned automatically.
   The causes reported are: the root is missing; it holds no library, or
   a library with another identity; it cannot be read.
6. **No read-merge-write** (critic finding 2). Each reference is its own
   file under `content/<hash>/refs/`, named so that one (library, root,
   artifact) key maps to exactly one file name. It is created or replaced
   by atomic replacement, never merged. A stale-root repair writes the
   new reference's file, then removes the stale one.
7. No migration, and no format-version handling. Schema 5 is unreleased,
   and this change may add to it. It must not change the meaning of
   anything 13a defined without saying so and why (D7).

**Accepted critic findings, resolved here:**

- **Finding 3:** a record that links no item has no reference (D2, D3).
- **Finding 5:** recovery claims match what is visited (D3, D10):
  - `reconcile` is the restoring command, over every hash of every
    history;
  - an applying admission writes the admitted bytes' reference only;
  - `adopt` writes only the references of the orphans it records.

**Decided by the designer and recorded below:** the file name (D1); the
upsert and its repair rule (D3); entry order (D5); no report for an
absent or unreadable identity (D2); no evidence on an entry adopted
through a reference (D6); the JSON shapes and which 13a meanings
change (D7); the human wording (D8).

What the engine does today, at the points this change touches:

- **The entry.** `ContentIndex<S>`
  (`crates/borax-sources/src/store.rs`, line 485) reads and writes
  `extracted` and `operator` facts through `IndexStore`:
  - the trait is at line 196, with `read_fact` and `write_fact` only;
  - `IndexEntry` is at `crates/borax-core/src/index.rs`, line 33;
  - `entry_path` (line 142) turns a key into a file and admits only
    `a-z`, `0-9`, `.`, `-` and `_` in a segment;
  - `write_atomically` (line 347) writes a temporary named `.tmp-…`
    (line 467) and renames it over the file.

  Nothing lists an entry's directory or removes a fact. The
  implementors are:
  - `FileCache` (line 295);
  - `MemoryCache` (`crates/borax-sources/src/cache.rs`, line 144);
  - `run::IndexHome` (`crates/borax/src/run.rs`, line 4465);
  - six test stores (D13).
- **Resolution.** `pipeline::standing` (`crates/borax/src/pipeline.rs`,
  line 650) asks the run's library first:
  - a tracked file is answered by `library_record` (line 298), and its
    entry is never read;
  - everything else goes to `beyond_library` (line 711), which reads
    the entry through `index_read` (line 211) and on a hit looks the
    evidence up in `indexed_standing` (line 244).

  `resolve_batch` (line 1399) calls `standing` from a thread pool. The
  rename driver (`run.rs`, line 1461) and `bib`'s `resolved_record`
  (line 3338) call it per file.
- **The library's own answer.** `Stores::consult` (`library.rs`, line
  1130) finds the item by `ItemStore::carrying` (line 716, private).
  For an item no file carries, it attributes a store fault through
  `Stores::item_fault` (line 1263, private), giving
  `AcceptedItemCause` (`crates/borax/src/event.rs`, line 627) for
  bytes accepted under another item.
- **Admission.** `admit` (`library.rs`, line 2972) writes the item, then
  the artifact record, through the run's `StoreWriter` (line 472), which
  mints the identity before a write (`ensure_identity`, line 421).
  - It returns `Admitted { artifact, item, relinked_from }` (line 2873).
    `item` is the record's link afterwards, which is not the item the
    record answers with for restored bytes.
  - `ArtifactRecord::item_for` (`crates/borax-core/src/library.rs`, line
    678) gives that item. It is `None` when the answer is the link and
    the record links none.
  - `admit` returns `Ok` for an admission whose `rewrite` renders the
    record unchanged (`library.rs`, line 2166), and that writes nothing
    and mints nothing.
- **The rename driver.**
  - `recorded` (`run.rs`, line 3107) calls `admit` (line 3157) for
    every file it records. It does not call it for a file with no
    hash, nor for one it reports `outside` or `pending`.
  - The driver emits the admissions, then `remember`'s
    `content-index-write` (line 1694).
- **Reconcile.** `reconcile_events` (`run.rs`, line 3933) emits the
  repairs, then the `not-pending` skips, then
  `restore_operator_decisions` (line 3990), then the totals.
  `library::operator_decisions` (`library.rs`, line 2727) already walks
  every hash of every history.
- **Adopt.**
  - `library::adopt` (`library.rs`, line 3255) admits only orphans
    (`orphans`, line 1550, excludes every path a record names).
  - Its `lookup` is `run::held_answer` (`run.rs`, line 4069), which reads
    the entry's evidence and then the response cache.
  - `Adopted` (`library.rs`, line 3196) carries `identifier` and
    `service`. `Unadopted::Unanswered` (line 3208) carries `identifier`.
- **The identity.** `library::identity(root)` (`library.rs`, line 381)
  reads `.borax/library.toml` as `Absent`, `Present(LibraryId)` or
  `Faulty(StoreFault)`, and writes nothing. `identity_fault_warning`
  (line 525) is the one warning change 12 defined.
- **The stream.** `IndexReadStep::Hit { evidence }` (`event.rs`, line
  1080) and `RetrievedFrom` (line 1321) have no reference.
  `WrittenEntry` (line 1122) has the one kind `evidence`, and
  `Adoption::Unanswered` (line 783) requires `identifier`.
- **The preferred identifier.** `pipeline::identifier_of`
  (`pipeline.rs`, line 1348, private) is the record's DOI, else arXiv,
  else PMID, else ISBN. It is the `resolved` event's `identifier`.

## D1. A reference on disk

**Decision.** A reference is the file
`<data root>/content/<hash>/refs/<name>.json`:

```json
{"library":"0190c3a2-…","root":"/home/u/papers","artifact":"0190c3a2-…","item":"0190c3a2-…"}
```

- **`<name>`** is `sha256-<hex>`: `borax_core::content::hash_bytes`
  over the UTF-8 bytes of `<library> NUL <root> NUL <artifact>`.
  - Neither a UUID's text nor a path can hold NUL, so distinct keys
    give distinct input.
  - The item is not in the key, so a reference whose item changes keeps
    its file.
  - The name is lowercase hexadecimal, a segment `entry_path` admits.
- **`root`** is the library root's text, absolute and normalised
  lexically (`paths::lexical`), as `Stores::read` normalises it. A root
  that is not UTF-8 text cannot be written into JSON. Such a library
  gets no reference, and each attempt is reported `failed`, with a
  message saying the root is not UTF-8 text.
- **Identities** are canonical UUID text, as `LibraryId`, `ArtifactId`
  and `ItemId` render them.
- **A reader** takes the four keys and ignores any other. A file missing
  one, or with one that does not parse, is unreadable.
- **Atomic replacement.** A writer replaces its file through
  `write_atomically`, reading no other fact to decide what to write.
  Removal is `fs::remove_file`, and a file already absent counts as
  removed. Writing a reference touches no `extracted.json` and no
  `operator.json`, and writing either of those touches no reference.
- **Reading an entry's references** lists `refs/`. Every name ending
  `.json` whose stem is a segment `entry_path` admits is read. A
  temporary is not one, being `.tmp-…` with no `.json`. A file that
  cannot be read or does not parse is skipped, as an unreadable
  `extracted.json` is ignored. A `refs/` that cannot be listed reads as
  no references. Neither is reported (Risks).
- **Concurrency.** Two writers of different keys write different files
  and never meet. Two writers of one key write the same library, root
  and artifact, and differ at most in the item: the last write stands,
  and the next `reconcile` in that library rewrites it if it is wrong.
  A repair removes a stale file (D3) only for a root that is missing or
  holds no library or another one. Only a peer still writing under that
  root could race it, and such a peer's own store writes are failing
  too.
- **No record and no evidence** is kept in a reference file.

**Rejected:**

- **The item in the name.** A changed item would leave the old file
  beside the new one, and the reader would see two references for one
  key.
- **A name derived by escaping the root.** Roots have characters
  `entry_path` refuses, and an escape scheme is a second format to
  keep.
- **A single `refs.json` per entry.** That is the read-merge-write the
  settled decision rules out.
- **A `written_at` field to order by.** Nothing else needs it, and the
  record says the file holds the four keys.

## D2. Who writes a reference, and when

**Decision.** Three writers, each writing after the store writes it
depends on:

| Writer | For | Item |
|---|---|---|
| an applying rename, after each `admit` that returned `Ok` (minted, updated, changed, relinked, separated, or written unchanged) | the admitted hash and `Admitted::artifact` | `Admitted::answers_with`, the item `item_for(hash)` gives after the admission (D9) |
| `borax adopt`, after its pass | each orphan it recorded, from `Adoptions::owed` | the same, for the record it minted |
| `borax reconcile`, after its repairs and its operator restorations | every distinct hash of every record's history (`library::references_owed`), the store read afresh | `item_for(hash)`; a hash it gives none for is skipped |

**Never:** a preview, `--no-record`, `status`, `validate`, `resolve`,
`bib`, `cache`, `config`. An admission reported `pending`, `outside` or
`unwritten` writes none. A file with no hash writes none. A separation
writes the new artifact's reference, and the old record's references
stay.

**The identity.**

- The rename driver reads `library::identity(root)` immediately before
  each reference write.
- `adopt` and `reconcile` read it once, after their own store writes
  and before their first reference write. A store write earlier in the
  same run may have minted it.
- Only `Present(L)` writes, naming `L`. Nothing mints for a reference.
- `Absent` writes nothing and reports nothing. The living requirement
  "A library carries an identity that does not depend on where it is"
  says a library with no identity file is well formed and "nothing
  reports it as incomplete". The library gains an identity at its next
  store write, and its references at the writer's next run after that.
- `Faulty` writes nothing, and nothing beyond change 12's warning is
  reported. A run that made a store write already warns, with
  `identity_fault_warning`, that the library "has none another location
  can name". `borax validate` reports the file.

**A record that links no item (finding 3).** `item_for(hash)` is `None`
for its newest bytes. No reference is written for them, no item is
invented, and nothing is reported. `borax validate` and `borax status`
already count the record as `unlinked`. A reference written while the
record linked an item stays as it is (D3). Older bytes whose own entry
names an item get `item_for`'s item, as the coordinator's finding 5
directive says. Inside the library, `Stores::consult` reports such
bytes `no-item`, because it checks the link first. So the outside answer
differs from the inside one only for a record whose link is damaged
(Risks).

**Order.** The order is the item, the artifact record with its
evidence, the reference, and then `remember`'s operator evidence. An
interruption leaves the library's decision without its reference, never
the reference alone.

**Rejected:**

- **A warning for an absent identity**: the first draft's
  `unreferenced_warning`. It reports a well-formed library as
  incomplete, and its advice, "the next store write gives it one", is
  nothing the operator can act on.
- **A new warning for a faulty identity.** It would make a reconcile
  that writes nothing warn. `a_reconcile_over_an_untouched_library_with_a_faulty_identity_file_does_not_warn`
  pins change 12's choice that it does not.
- **Minting for a reference.** Settled.
- **Writing a reference for a record linking no item under its last
  link.** That would be inventing an item.

## D3. Writing is an upsert, and it repairs stale roots

**Decision.** `references::keep(index, hash, r = (L, R, A, I),
libraries)`:

1. **The key's own file.** Read the file `r`'s key names.
   - It holds a reference with item `I`: nothing is written or
     reported.
   - It holds another item, or cannot be read or parsed: it is replaced
     with `r`. That is how a hand re-link reaches the index at the next
     reconcile.
   - It is absent: go to step 2.
2. **Repair.** Of the entry's readable references, the stale ones are
   those naming `L` and `A` at another root `R′` (D4) where:
   - `R′` is missing, or is not a directory;
   - its identity is `Absent`;
   - its identity is `Present` with another identity.

   A root that holds `L` is a live copy and not stale. So is a root
   whose metadata or identity cannot be read. `r` is written, and only
   then is each stale file removed:
   - a write that fails removes nothing;
   - a removal that fails leaves that file, and the event reports
     `failed` with a message naming the file it could not remove.
3. **Never** is a reference removed otherwise:
   - not one whose key already had a file in step 1, stale or not;
   - not an unreadable one;
   - not one for another artifact or another library.

   There is no automatic pruning.

**Two writers racing over one stale reference.** Writers at two live
roots of one library may each find the same stale reference S, write
their own key's file, and each remove S. Neither file is lost, because
the two keys name different files. The second removal finds S absent,
which counts as removed (D1), so both report `written` with S's root in
`repaired_from`.

The test makes the interleaving deterministic through the one seam
`keep` reaches the store by, `IndexStore`. A test store wraps a
`MemoryCache`, and its `remove_fact` waits on a shared
`std::sync::Barrier` of two before removing. `keep` lists, then writes,
then removes, so by the time either thread passes the barrier both have
found S and written their own file. No sleep or retry is involved
(task 3.1).

`keep` returns what it wrote: the reference, the roots it removed as
`repaired_from`, and the write's result. It returns nothing when step 1
found the same item. Each return is one `content-index-write` event
(D7).

**Who repairs, and what.**

- **`reconcile`** repairs every hash of every history, so a moved
  library's historical bytes are repaired too.
- **An applying rename** repairs the admitted hash only.
- **`adopt`** repairs the hash of each orphan it records. That is rare,
  since a new record rarely has a stale reference.
- **`resolve`, `status` and `validate`** repair nothing. A tracked file
  never reads its entry, and the requirement "A library answer SHALL
  neither read nor write the content index" holds.

So "a run inside the library repairs that reference's stale root", in
the record's words, means a `reconcile`, or an applying run that admits
the file.

**What a reconcile restores (finding 5).** After its own writes, a
reconcile restores every reference the library's records owe that the
index lacks, or holds with another item or unreadably. That includes
the reference an interrupted admission never wrote, and references for
bytes accepted after that interruption and before the reconcile. Nothing
else restores references:

- **`adopt`** never visits a recorded file. `orphans` excludes every
  path a record names.
- **An applying run** writes only the reference of what it admits.

**Cost.** For each distinct hash of each history:

- one read of the key's file;
- where it is absent, one listing of `refs/`, a read of each reference
  in it, and one stat and identity read of each other root of the same
  library.

The stat and identity read are made once per run (D4). Once the index
holds the library's references, a reconcile over 2,000 records reads
2,000 small files.

**Rejected:**

- **Removing every stale reference of the same library and artifact,
  whether or not the key had a file.** This is the deleted-copy case
  (D10 b): the original's reconcile would delete the copy's reference,
  which is automatic pruning by another name.
- **Removing before writing.** An interruption between the two would
  leave no reference at all. Settled the other way.
- **Treating an unreadable root as stale.** A library on an unmounted
  disk is not gone.

## D4. Reaching a reference

**Decision.** `Libraries::reach(reference)` decides, in this order,
the first that holds:

| Condition | Status |
|---|---|
| `fs::metadata(root)` is `NotFound`, a dangling link included | `unavailable`, cause `missing` |
| any other metadata error | `unavailable`, cause `unreadable`, with `path` the root and `message` |
| the root is not a directory | `unavailable`, cause `other-library`, no `found` |
| `identity(root)` is `Absent` | `unavailable`, cause `other-library`, no `found` |
| `identity(root)` is `Faulty(f)` | `unavailable`, cause `unreadable`, with `f.path` and `f.message` |
| `identity(root)` is `Present(id)`, `id` not the reference's library | `unavailable`, cause `other-library`, `found` `id` |
| the root's item store holds no item `I`, and a fault is attributed to it | `item-unavailable`, cause `unreadable`, with `path` and `message` |
| it holds no item `I`, and no fault is | `item-unavailable`, cause `dangling` |
| several files carry `I` | `item-unavailable`, cause `ambiguous`, with `files` |
| exactly one file carries `I` | reachable, with that item |

- The item comes from `ItemStore::answer(root, I)` (D9). The library's
  own resolution uses the same attribution
  (`Stores::item_fault`, `library.rs`, line 1263).
  - An item is found by the `id` inside its file, never by the file
    name, as "An item file's name carries a legible key and its
    identity" requires.
  - The answer is the item the reference **stores**. The referenced
    artifact record is not read. D3's step 1 keeps the stored item
    current at each reconcile.
- **The marker is not consulted.** A root whose `.borax.toml` was
  removed and whose `.borax/library.toml` still names `L` is reachable.
  The identity is what a reference names, and "Which files a library
  answers for SHALL NOT depend on its identity" works the other way
  round.
- **Once per run.** `Libraries` reads each root at most once per run:
  its metadata and identity once, and its item store once, the first
  time a reference to it names the identity it holds. `resolve_batch`
  shares one across its workers, behind a `Mutex`. The rename driver,
  `bib`, `adopt` and `reconcile` each hold one for the run. What a root
  holds is the snapshot first read. `standing` and `resolve_file` keep
  their signatures and use a fresh one per call.
- **Reading writes nothing**: no fact, no file of the referenced
  library, no identity.

**Rejected:**

- **Reading the referenced artifact record.** It costs a store read per
  file, and the record's current link is what a reconcile there writes
  into the reference anyway.
- **Following the marker.** A library is named by its identity, and a
  marker-only check would call a library whose marker was edited away
  gone.

## D5. Answering: references, then evidence, then extraction

**Decision.** For a file the run's library does not answer for, read
the entry where 13a reads it: the cache on and the file hashed.

1. **Reach every reference**, in entry order: by `root` text, then
   `library`, then `artifact`. `references::settle` then judges the
   reachable ones (pure, D9).
   - None reachable: no reference answers.
   - **Agreement.** Every reachable item carries the same preferred
     identifier, or every one carries none and all are one item of one
     library. `pipeline::preferred_identifier` is DOI, else arXiv, else
     PMID, else ISBN, as the `resolved` event's `identifier` is.
     Identifiers compare as `Identifier` values. An item carrying one
     and an item carrying none disagree.
   - Agreeing: the first reachable reference is `answered`, the other
     reachable ones `agreed`.
   - Disagreeing: each reachable one is `disagreeing`, with its item's
     preferred identifier, absent when it carries none. None answers.
   - Unavailable references keep their status in every case.
2. **A reference answered.**
   - The file's record is that item's record, provenance as stored.
   - `content_index.read` is a `hit` with `references` and no
     `evidence`.
   - Extraction, the titles, the lookup, the title check and the
     content-index write are not attempted, for `content-index-hit`.
     `identifier_input` is not attempted, for `not-asked`.
   - `record_retrieval` is `library-reference`, and `acceptance` is
     `automatic`.
   - `FileRecord::identification()` is `None`: no lookup reached the
     record. So an admission of such a file records no evidence, the
     case "bytes no identifier established". `remember` is not
     attempted, for `content-index-hit`.
   - The work check applies, and an interactive run puts the question it
     calls for.
3. **No reference answered, and the entry's evidence answers**: 13a's
   evidence answer, with `references` on the `hit`.
4. **Neither**: `miss`, with `references` when there were any, and
   extraction as in 13a.

**Inside a library.**

- A tracked file is answered by its item and never reads its entry,
  references included.
- Any other file reads its entry like a file outside, references to its
  own library included. That covers an untracked copy, or a file whose
  record cannot answer. Its `library` section stays what the library
  said: `untracked`, or the problem. This is what the living scenario
  "A copy is not the artifact" changes to (MODIFIED).
- A reference answer is never `record_retrieval` `library`, which
  stays the run's own library's.

**Superseding 13a's wording.** 13a's proposal and its requirement "A
content-index entry answers with its identifier" describe outside
answers as evidence-first. That was the whole entry then. The order is
now reference, evidence, extraction. The record's default that an
outside answer "begins with its identifier" describes an entry with no
reference that answers.

**Rejected:**

- **Evidence before references.** That makes a reachable library's
  correction lose to the index's own identifier, against the settled
  choice rule.
- **Choosing the most recently written reference.** There is no
  timestamp in the file (D1), and recency is not authority.
- **Using a reference to the run's own library for a file inside it.**
  The library already said what it knows about that path. Its answer
  about these bytes elsewhere in the tree is exactly a reference.

## D6. Adoption through a reference

**Decision.** `held_answer` gains a first source:

1. **References**, settled as in D5.
   - One answered: `Adopted { record: item's record, identifier: None,
     retrieval: library-reference }`.
   - The history entry carries no evidence: the record was reached by
     the bytes' reference, not by an identifier. That is "bytes no
     identifier established" in the living requirement on history
     entries.
2. **Else the evidence** (13a), through the response cache:
   - found: `Adopted { identifier: Some(e.identifier), retrieval:
     service-cache }`;
   - not found: `Unadopted::Unanswered { identifier: Some(…),
     references }`.
3. **Else**:
   - references were read: `Unanswered { identifier: None, references }`;
   - none: `Unindexed`. Its meaning narrows (D7): no evidence that
     answers and no readable reference, where 13a's was no evidence.

After the pass, `Adoptions::owed` lists each recorded orphan's full
path, hash, artifact and `answers_with`. `adopt_events` reads the
identity and `keep`s each one, then reports the `missed` lookups and
the totals.

**What adoption restores.** Only the references of the orphans it
records. An admission interrupted after its artifact record is not
completed by `adopt`, since the file is recorded and not an orphan.
`reconcile` completes it (D3).

**Rejected:**

- **The item's preferred identifier with origin `adopted`**: the first
  draft's D9. It says an identifier found the record when one did not.
- **The response cache before references.** That is against the settled
  precedence.

## D7. The schema: 5, added to

**JSON.**

- **`content_index.read`.** `hit` and `miss` gain `references`, an array
  in entry order, present only when the entry held at least one
  readable reference. `hit`'s `evidence` is present exactly when the
  evidence answered. A reference answer is `{"status":"hit",
  "references":[…]}`.
- **A reference** is `{"library", "root", "artifact", "item",
  "status", …}`. `status` is one of:
  - `answered`;
  - `agreed`;
  - `disagreeing`, with `identifier` when the item carries one;
  - `unavailable`, with `cause`:
    - `missing`;
    - `other-library`, with `found` when the root holds an identity;
    - `unreadable`, with `path` and `message`;
  - `item-unavailable`, with `cause`:
    - `dangling`;
    - `unreadable`, with `path` and `message`;
    - `ambiguous`, with `files`. These are `AcceptedItemCause`'s
      spellings.
- **`record_retrieval`** gains `library-reference`, with `library`,
  `root`, `artifact` and `item`.
- **`content-index-write`** gains `entry` kind `reference`, with
  `library`, `root`, `artifact` and `item`, and `repaired_from`, an
  array of roots present only when non-empty. Its `path` is:
  - in a rename, where the run left the file: the path it moved to, or
    the one it already carried;
  - in `adopt`, the orphan's full path;
  - in `reconcile`, the full path the record names.

  `write` is `written` or `failed`.
- **`library-adoption`.** `recorded.retrieval` may be
  `library-reference`. `unanswered`'s `identifier` is present only when
  the entry held evidence, and it gains `references`, present only when
  non-empty.

**Order of events.**

- **A rename:** the file's `renamed` or `already-named` event, then its
  `library-admission` events, then the reference write's
  `content-index-write`, then `remember`'s.
- **`adopt`:** the `library-adoption` events, then the reference writes
  in orphan order, then the `missed` lookups, then the totals.
- **`reconcile`:** the repairs, then the `not-pending` skips, then the
  operator restorations, then the reference writes, then the totals.

None counts toward `run-finished`, and a failure changes no exit status.

**What changes in meanings 13a defined**, unreleased in schema 5:

| Value | 13a | 13b |
|---|---|---|
| `content_index.read` `hit` | always carries `evidence`, and is followed by a lookup | carries `evidence` only when the evidence answered; a reference answer has none, and no lookup follows (`lookup` not attempted for `content-index-hit`) |
| `content-index-write` | every evidence write after a verdict; none in a batch run or `adopt` | also every reference write: by an applying run, batch included, by `adopt`, and by `reconcile` |
| `library-adoption` `unanswered` | evidence with no cached record; always `identifier` | the entry answered with nothing that produced a record; `identifier` when it held evidence |
| `adopt` and the index | writes nothing to the index | writes references, never evidence |
| `library-adoption` `unindexed` | the entry answers with no evidence | the entry answers with no evidence **and** holds no readable reference: an entry holding only references is `recorded` through one that answers, or `unanswered` |
| `library-adoption` `recorded.retrieval` | only `service-cache` | also `library-reference` |
| `content_index.read` `miss` | the entry answers with no evidence; nothing else was in it | neither a reference nor the evidence answered; it may carry the `references` that did not |
| `record_retrieval` after a `hit` | always the lookup's service, `service-cache` or `network` | also `library-reference`, for a reference answer |

**Re-checked and unchanged:** `lookup.origin` `content-index` still
means a lookup of the entry's evidence, and is never reported for a
reference answer, which makes no lookup. `content_index.write` keeps
its meaning, and its `content-index-hit` reason now also follows a
reference answer. `acceptance` `automatic` already covered a record no
lookup reached. The `record_retrieval` kinds `library`, `service-cache`
and `network` mean what they did.

**Why without a bump.** The `cli` requirement fixes a schema version at
the first release that emits it, and none has emitted 5. Every line a
13a build writes for an entry with no reference is byte-identical in
13b, because `references` and `repaired_from` are left out when empty.
So the changed meanings show only in lines about references, which no
13a build writes.

**Rejected:**

- **`references` always present.** It would change every 13a `hit`,
  `miss` and `unanswered` line, and every test pinning one, for a key
  that says nothing.
- **`evidence` on a reference answer.** It would name evidence that
  played no part, and the description reads `hit`'s `evidence` as
  "where the identifier came from".
- **A separate kind for an adoption whose references could not
  answer.** `unanswered` already means "the index answered with
  something that led to no record".

## D8. Human lines and the description

Every value from an entry, a reference or a library store is escaped,
as the text after `<path>: ` already is.

- **The retrieval clause** of a `resolved` line reads `, from the
  library at <root>` for `library-reference`. `via` names the services
  the item's provenance names, as for `library`
  (`event::services_of`, line 1386).
- **Clauses**, after `; the library could not answer: …` and before
  the rejected-candidate clause, in entry order:
  - for each `unavailable` or `item-unavailable` reference, `; a
    library reference could not answer: ` and one of:
    - `<root> is missing`;
    - `<root> holds no library`, for `other-library` with no `found`;
    - `<root> holds another library (<found>)`;
    - `<path> could not be read (<message>)`, for `unreadable`, either
      status;
    - `the library at <root> holds no item <item>`, for `dangling`;
    - `the library at <root> holds item <item> in several files`;
  - one clause for a disagreement: `; library references disagree: `
    and, for each `disagreeing` reference, `the library at <root> names
    <identifier>`, or `names no identifier`, joined by `, `.

  `answered` and `agreed` add nothing. The clauses go on `resolved`
  lines and on resolution `skipped` lines alike.
- **A failed reference write**: `<path>: the content index could not
  keep its reference to the library at <root> (<message>)`. A written
  one renders nothing.
- **`library-adoption` `unanswered`**:
  - with an `identifier`: 13a's line, then the clauses;
  - without one: `no library reference the content index holds for its
    bytes answered, and it holds no identifier for them, so it is still
    an orphan`, then the clauses.
- **The interactive description** (`describe.rs`):
  - a reference answer looks nothing up, so the identifier line already
    names the record's own identifier with no origin (`looked_up`, line
    320);
  - the `record` line is `<services>, from the library at <root>`, or
    `the library at <root>` when the provenance names none
    (`record_from`, line 599);
  - `file says` keeps `not read; an earlier run answered`;
  - after the `library` line, one `reference` line per reference that
    could not answer, with the text after `could not answer: `;
  - after those, one `references` line for a disagreement, `disagree:`
    and the clause's list;
  - a skip's description carries the same two kinds of line, after its
    reason and its `library` line.

## D9. The interface the tests are written against

`borax-core`, `crates/borax-core/src/index.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryReference {
    pub library: LibraryId,
    pub root: PathBuf,
    pub artifact: ArtifactId,
    pub item: ItemId,
}
impl LibraryReference {
    /// `sha256-<hex>` of library, root and artifact joined by NUL (D1);
    /// `None` when `root` is not UTF-8 text.
    pub fn name(&self) -> Option<String>;
    /// Whether `other` names the same library, root and artifact.
    pub fn same_key(&self, other: &LibraryReference) -> bool;
}
/// `{"library":…,"root":…,"artifact":…,"item":…}`; `None` for a root
/// that is not UTF-8 text.
pub fn reference_to_json(reference: &LibraryReference) -> Option<String>;
pub fn reference_from_json(text: &str) -> Result<LibraryReference, Malformed>;
```

`borax-sources`, `crates/borax-sources/src/store.rs`:

```rust
/// `content/<hash>/refs`.
pub fn references_prefix(hash: &ContentHash) -> String;
/// `content/<hash>/refs/<name>`.
pub fn reference_key(hash: &ContentHash, name: &str) -> String;

pub trait IndexStore: Sync {
    fn read_fact(&self, key: &str) -> Option<Result<String, String>>;
    fn write_fact(&self, key: &str, text: &str) -> CacheWrite;
    /// The keys of the facts directly under `prefix`, sorted: `Ok(vec![])`
    /// when there are none or the directory is absent, `Err(message)`
    /// when it cannot be listed.
    fn list_facts(&self, prefix: &str) -> Result<Vec<String>, String>;
    /// Remove the fact under `key`: `Written` once it is gone, an absent
    /// one included; `Failed` otherwise, leaving it.
    fn remove_fact(&self, key: &str) -> CacheWrite;
}

impl<S: IndexStore> ContentIndex<S> {
    /// Every readable reference of `hash`'s entry, in entry order (D5).
    pub fn references(&self, hash: &ContentHash) -> Vec<LibraryReference>;
    /// The file `reference`'s key names, parsed.
    pub fn reference(&self, hash: &ContentHash, reference: &LibraryReference) -> Fact<LibraryReference>;
    /// Replace that file whole. `Failed`, writing nothing, for a root
    /// that is not UTF-8 text.
    pub fn put_reference(&self, hash: &ContentHash, reference: &LibraryReference) -> CacheWrite;
    pub fn remove_reference(&self, hash: &ContentHash, reference: &LibraryReference) -> CacheWrite;
}
```

`IndexEntry` and `ContentIndex::entry` are unchanged: an entry's
evidence is read as 13a reads it.

`borax`, a new `crates/borax/src/references.rs`:

```rust
pub enum Unavailable { Missing, OtherLibrary { found: Option<LibraryId> }, Unreadable { path: PathBuf, message: String } }
pub enum Reach { Reachable(Item), Unavailable(Unavailable), ItemUnavailable(AcceptedItemCause) }
pub enum ReferenceStatus {
    Answered, Agreed, Disagreeing { identifier: Option<Identifier> },
    Unavailable(Unavailable), ItemUnavailable(AcceptedItemCause),
}
pub struct ReferenceRead { pub reference: LibraryReference, pub status: ReferenceStatus }

/// The roots a run has read (D4). `Sync`.
pub struct Libraries { /* Mutex<BTreeMap<PathBuf, …>> */ }
impl Libraries {
    pub fn new() -> Libraries;
    pub fn reach(&self, reference: &LibraryReference) -> Reach;
    /// D3's staleness: missing, not a directory, no identity, another identity.
    pub fn is_stale(&self, root: &Path, library: &LibraryId) -> bool;
}
/// D5, pure: the answer, and every reference's status, in the order given.
pub fn settle(reached: Vec<(LibraryReference, Reach)>) -> (Option<(LibraryReference, Item)>, Vec<ReferenceRead>);
/// Read and settle `hash`'s references.
pub fn read<S: IndexStore>(index: &ContentIndex<S>, hash: &ContentHash, libraries: &Libraries)
    -> (Option<(LibraryReference, Item)>, Vec<ReferenceRead>);
pub struct Kept { pub reference: LibraryReference, pub repaired_from: Vec<PathBuf>, pub write: CacheWrite }
/// D3. `None` when the key already held the same item.
pub fn keep<S: IndexStore>(index: &ContentIndex<S>, hash: &ContentHash, reference: &LibraryReference,
    libraries: &Libraries) -> Option<Kept>;
```

`borax::library`:

```rust
// Admitted gains `pub answers_with: ItemId`: item_for(hash) after the admission.
impl ItemStore {
    /// The one item of identity `id`, or why there is none, attributed as
    /// Stores::consult attributes it for `root`'s store.
    pub fn answer(&self, root: &Path, id: &ItemId) -> Result<&Item, AcceptedItemCause>;
}
pub struct OwedReference { pub path: PathBuf, pub hash: ContentHash, pub artifact: ArtifactId, pub item: ItemId }
/// D2/D3: every distinct hash of every record, path order then oldest
/// first, with item_for's item; a hash it gives none for is left out.
pub fn references_owed(root: &Path) -> Vec<OwedReference>;
pub struct Adopted { pub record: Record, pub identifier: Option<Identifier>, pub retrieval: RetrievedFrom }
pub enum Unadopted { Unindexed, Unanswered { identifier: Option<Identifier>, references: Vec<ReferenceRead> } }
// Adoptions gains `pub owed: Vec<OwedReference>`.
```

`borax::evidence`:

```rust
// IndexRead gains Referenced: a library reference answered.
// IndexEvidence gains `pub references: Vec<ReferenceRead>`, between read and write.
// RecordRetrieval gains LibraryReference { reference: LibraryReference }.
```

`borax::pipeline`:

```rust
pub fn preferred_identifier(record: &Record) -> Option<Identifier>;
/// `standing`, reading references through `libraries`.
pub fn standing_in<S: IndexStore>(path: &Path, documents: &dyn Documents, sources: &[&dyn Source],
    index: &ContentIndex<S>, config: &ResolveConfig, account: Option<&Account<'_>>,
    library: Option<&Stores>, libraries: &Libraries) -> Standing;
// standing, resolve_file and resolve_batch keep their signatures.
```

`borax::event`:

```rust
// IndexReadStep::Hit { evidence: Option<EvidenceStep>, references: Vec<ReferenceStep> }
// IndexReadStep::Miss { references: Vec<ReferenceStep> }
//   (both skip-serialized when None or empty)
// ReferenceStep { library, root, artifact, item, #[serde(flatten)] status: ReferenceStatusStep }
// #[serde(tag = "status")] ReferenceStatusStep: Answered, Agreed,
//   Disagreeing { identifier: Option<String> },
//   Unavailable { #[serde(flatten)] cause: UnavailableCause },
//   ItemUnavailable { #[serde(flatten)] cause: AcceptedItemCause }
// #[serde(tag = "cause")] UnavailableCause: Missing, OtherLibrary { found: Option<String> },
//   Unreadable { path, message }
// RetrievedFrom::LibraryReference { library, root, artifact, item }
// WrittenEntry::Reference { library, root, artifact, item, repaired_from: Vec<PathBuf> }
// Adoption::Unanswered { identifier: Option<String>, references: Vec<ReferenceStep> }
```

The doubly flattened tagging is the shape the critic judged
expressible. Round trips through `serde_json` cover every status and
cause (task 4.1). If one cannot round-trip, the JSON in D7 is the
contract, and the Rust shape is the implementer's to change.

The private parts are the implementer's:

- how `recorded` hands the admission to the reference write;
- how `Libraries` holds a root;
- how `IndexHome` forwards the two new methods.

## D10. Sequences traced

`L` is a library identity, `R1`, `R2` roots, `A` an artifact record,
`H` its bytes, `I` and `J` items, and `C` a copy of `H` outside every
library. "Outside" is `borax resolve C`.

**(a) An admission interrupted after its record, then reconcile, then
outside.**

1. In `L` at `R1`, `borax rename --apply F` resolves `F` (bytes `H`)
   by extraction: `extracted.json` = X. It moves `F`. The admission
   writes item `I`, then `A` with entry `(H, I, {X, extracted})`. The
   run is interrupted before the reference.
2. State: `refs/` of `H` is empty. Outside: no reference, so evidence
   X is looked up, retrieval `service-cache` or `network`.
3. `borax reconcile` in `R1`:
   - the identity is `Present(L)`, minted by step 1's write;
   - `references_owed` gives `(H, A, I)`;
   - the key's file is absent, and there is no stale one;
   - `(L, R1, A, I)` is written, with one `content-index-write`
     `reference` `written` before the totals.
4. Outside: the reference is reachable and answers with `I`'s record.
   - `record_retrieval` is `library-reference`, root `R1`.
   - `content_index.read` is `hit`, `references` `[answered]`, with no
     `evidence`.
   - No lookup is made.

The uninterrupted run ends with the same reference: **equal**. A later
applying run that admits `F` also writes it. `adopt` does not, since `F`
is recorded. An operator admission (supplied Y) gets its `operator.json`
from the same reconcile (13a's D5), and the reference.

**(b) `rsync` copy, reconcile in the copy, outside, delete one.**

1. `L` at `R1` holds `A` (bytes `H`, item `I`, `doi:X`). `refs/` holds
   `(L, R1, A, I)`.
2. `rsync -a R1/ R2/` on the same machine. Copying registers nothing.
3. `borax reconcile` in `R2`:
   - the identity is `Present(L)` (copied);
   - the key `(L, R2, A)` has no file;
   - `(L, R1, A)` is at a root holding `L`, so it is not stale;
   - `(L, R2, A, I)` is written.

   Two references.
4. Outside: both are reachable and carry `doi:X`. They agree. In entry
   order `R1` sorts first: it is `answered`, and `R2` `agreed`.
5. `rm -r R2`. Outside: `R1` answers. `R2` is `unavailable` `missing`,
   and the line ends `; a library reference could not answer: R2 is
   missing`.
6. `borax reconcile` in `R1`: `(L, R1, A)` holds `I`, so nothing is
   written. `(L, R2, A)` is kept: there is no pruning. Every outside run
   reaching `H` reports it (Deferred).

Branch, from step 2: `rm -r R1` before step 3. Then the reconcile in
`R2` finds `(L, R1, A)` stale (`missing`). It writes `(L, R2, A, I)`,
then removes `R1`'s, with `repaired_from` `[R1]`. One reference. A
deleted copy cannot take part in a disagreement.

**(c) Disagreement: one copy re-linked, both present.** This starts from
(b)'s step 4.

1. In `R2`, an interactive rename of `F` where the operator supplies
   `doi:Y`:
   - the re-identification links `A` (in `R2`) to item `J` and appends
     `(H, J, s(Y))`;
   - `keep` finds `(L, R2, A)` holding `I`, so it is rewritten to `J`;
   - `remember` writes `operator.json` = `s(Y)`.
2. Outside:
   - `(L, R1, A, I)` names `doi:X` and `(L, R2, A, J)` names `doi:Y`,
     so both are `disagreeing`;
   - the entry's evidence answers: `s(Y)` is looked up;
   - the line ends `; library references disagree: the library at R1
     names doi:X, the library at R2 names doi:Y`.

   With no evidence it would be `miss`, then extraction.
3. Variants:
   - `A`'s link edited to `J` by hand in `R2`: the reference still says
     `I`, so the two agree until `borax reconcile` in `R2` rewrites it
     (D3, step 1);
   - the DOI edited inside `I`'s file in `R2`: visible at once, since
     the item is read.

**(d) A moved root, then a run inside, then outside.**

1. `L` at `R1`. Its record `A` holds `H0` (accepted under `I0`) and
   `H` (link `I`). The references are `(L, R1, A, I0)` for `H0` and
   `(L, R1, A, I)` for `H`.
2. `mv R1 R2`. Outside over `H`: `R1` is `missing`, so the evidence is
   looked up, or the file extracted.
3. `borax resolve F` in `R2`: `F` is tracked, so the library answers and
   nothing is written. Outside is unchanged.
4. `borax reconcile` in `R2` repairs both:
   - for `H0`, `(L, R2, A)` has no file and `(L, R1, A)` is stale, so
     `(L, R2, A, I0)` is written and `R1`'s removed, with
     `repaired_from` `[R1]`;
   - `H` the same.
5. Outside: `R2` answers.

Branch, from step 3: an applying rename admitting `F` repairs `H`'s
reference alone. `H0`'s stays at `R1` until a reconcile.

**(e) A faulty or absent identity receiving an admission.**

- **Absent, with a store write.** The `StoreWriter` mints, the identity
  is `Present` when the reference is written, and it is written.
- **Absent, and an admission that writes nothing** (already named, the
  record unchanged): nothing is minted, no reference is written, and
  nothing is reported. A reconcile over the untouched library writes
  nothing, mints nothing and writes no reference. The first store write
  changes that.
- **Faulty.** The admission writes, and change 12 warns once. No
  reference is written, and nothing else is reported.
- **The mint fails.** The admission is `unwritten`, so no reference.

**(f) A reference whose item file is gone.**

1. `(L, R1, A, I)` exists, and `I`'s file is deleted in `R1`.
2. Outside: `item-unavailable` `dangling`, so the evidence is looked up.
   The line ends `; a library reference could not answer: the library
   at R1 holds no item I`.
3. Inside `R1`, `F` gets `dangling-item` from its library. It then
   reads its entry: the same reference reads `item-unavailable`, and
   the evidence answers.
4. `borax reconcile` in `R1`: `item_for(H)` is still `I`, the key holds
   `I`, and nothing is written. The reference is kept, and `borax
   validate` reports the dangling link.

**(g) Finding 5: a reference never written, then newer bytes.**

1. (a)'s step 1, interrupted. Then `F` is edited, and an applying
   rename accepts `H2` (`changed`).
2. `keep` writes `(L, R1, A, I)` for `H2` only.
3. `borax reconcile` writes `H`'s too: `references_owed` gives both.

**(h) Finding 3: a record that links no item.**

1. `A` holds `H` and links no item, and its entry names none.
2. `borax reconcile` writes nothing for `H` and reports nothing for it.
3. `status` and `validate` count `A` `unlinked`. A reference written
   while `A` linked `I` is left as it is.

## D11. Group boundaries and green points

Each type change lands with every consumer of that type, in one
red/green pair. Every group ends with the workspace compiling and the
whole suite green.

| Group | Lands | Why it is green at its end |
|---|---|---|
| 1 | `LibraryReference` and its JSON and name; `IndexStore::list_facts` and `remove_fact`; `ContentIndex`'s reference API | Additive except the trait's two methods. Every implementor changes in the pair: `FileCache`, `MemoryCache`, `IndexHome` and the six test stores (D13) |
| 2 | `Admitted::answers_with`, `ItemStore::answer`, `OwedReference`, `references_owed` | `Admitted`'s four `src` literals and patterns, and its three test literals, change in the pair. The rest is additive |
| 3 | `borax::references`: `Libraries`, `settle`, `read`, `keep` | Additive: nothing calls it yet |
| 4 | Resolution reads references: `IndexRead::Referenced`, `IndexEvidence::references`, `RecordRetrieval::LibraryReference`, the event shapes, `standing_in`, the shared `Libraries`, the human clauses, `describe.rs` | Every consumer of `IndexEvidence`, `IndexRead`, `IndexReadStep`, `RecordRetrieval` and `RetrievedFrom` changes here, with the test sites D13 lists. Nothing writes references yet, so every existing run behaves as before |
| 5 | Writing: `WrittenEntry::Reference`, the rename driver's write, `reconcile`'s restoration, the failed-write line, and the binary tests | Additive behaviour through the event shapes group 4 fixed. D13's sweep found no existing test whose library both has an identity and links an item while asserting the event sequence |
| 6 | Adoption: `Adopted`, `Unadopted::Unanswered`, `Adoption::Unanswered`, `Adoptions::owed`, `held_answer`'s reference source, `adopt_events`' writes | Their last consumers, `held_answer`, `adopt`, `what_was_adopted` and the adoption tests, change in the pair |

There is no interim state worth stating. Groups 1 to 3 add code nothing
calls. After group 4, references are read and nothing writes them.
After group 5, everything but adoption writes them.

## D12. How the specifications change

ADDED:

- `resolution`:
  - "The content index keeps references to libraries, one file each":
    D1, and D2's summary;
  - "A library reference answers for a file before the entry's
    evidence": D4, D5;
  - "What became of a library reference write is reported": D7's
    `content-index-write` and its order.
- `library`: "A library's references are written after its records, and
  reconcile restores and repairs them": D2, D3.

MODIFIED, each restated whole with every living paragraph and scenario
name kept, so no `<!-- drops: -->` marker is used.

In `resolution`:

- "Responses are cached locally";
- "A resolution reports the evidence it was checked against";
- "An operator's answer about a file is remembered";
- "A file its library tracks resolves from its library item";
- "A resolution says where its record came from";
- "A resolution says what the content index did";
- "A resolution says why a step was not taken";
- "Resolution events carry their evidence in sections";
- "A resolution reports what the content index did and every cache
  write";
- "A resolution reports where its record was retrieved";
- "What became of remembering an operator's answer is reported";
- "Human resolution lines name the work, where it came from, and why a
  file was skipped";
- "The content index keeps identification evidence, one fact per file";
- "A content-index entry answers with its identifier".

In `library`:

- "The library's text is authoritative and is read directly";
- "borax adopt records what the library already holds".

In `rename`: "An interactive question shows what the answer rests on".

The proposal lists what was checked and left unchanged.

## D13. Existing tests: what changes and how

The rules of the earlier changes apply:

- **(a)** a shape-only edit must not change what a test expects;
- **(b)** a deliberate rewrite states the new form of the same fact;
- **(c)** a deletion must be listed. None is.

**How the sweep was made.** A script split every file under
`crates/*/tests/` into its `fn`, `struct` and `impl` items and read each
body. Each body flagged was then read whole. It flagged:

1. **A type this change reshapes:**
   - `impl … IndexStore for`;
   - `IndexEvidence {`, `IndexRead::`, `IndexReadStep::Hit`,
     `IndexReadStep::Miss`;
   - `RetrievedFrom::`, `WrittenEntry::`;
   - `Admitted {`, `Adopted {`, `Unadopted::Unanswered`,
     `Adoption::Unanswered`;
   - an exhaustive `match` over any of these.
2. **A body that runs a reference-writing command through
   `events_for`, `dispatch`, `invoke`, `adopt_over` or the binary**: an
   applying or interactive rename over `RealFilesystem`, `adopt`, or
   `reconcile`. Each was read for:
   - whether its library can have a `Present` identity by the time the
     reference would be written, which needs a store write or a seeded
     identity file;
   - whether its record links an item;
   - whether it asserts the whole event sequence, a count or position
     of events, a `content-index-write` count or absence, or exact
     stdout.
3. **A body with two runs sharing one index** in a library, which could
   meet the first run's references.

It flagged 101 items in 9 files for (1), 119 in 4 files for (2), and
16 in 2 files for (3).
No item of (2) or (3) changes what it expects, and one is renamed (b,
below). The reasons:

- Every reconcile test whose run could write references has records
  linking no item (`item: None`, through `hash_entry_for` and
  `write_lib_artifact_record`), or a library with no identity.
- The adopt tests that record an orphan, which writes its reference
  from group 6, read events in three ways:
  - by kind, through `adoptions`, `adopted_totals` and
    `assert_every_orphan_is_accounted_for`;
  - by their last event, which stays the totals;
  - as human lines, where a written reference renders nothing.

  The JSON adopt tests record nothing.
- The applying-rename tests over `RealFilesystem` assert with `any`, by
  `Event` kind, or on library files and `index.entry`, which reads no
  reference.
- The stdout tests read stderr, or human output, where a written
  reference renders nothing.

Line numbers drift as earlier groups edit a file. A site the test
stage cannot find, or a break it finds that is not here, is reported
back.

**Shape-only (a):**

| Group | File | Site | Edit |
|---|---|---|---|
| 1 | `crates/borax-sources/tests/store.rs` | `WriteFailingCache` (near 40) | `list_facts` gives `Ok(vec![])`, `remove_fact` fails |
| 1 | `crates/borax/tests/runlog.rs` | `WriteFailingCache` (near 809) | the same |
| 1 | `crates/borax/tests/pipeline.rs` | `WriteFailingCache` (near 7163) | the same |
| 1 | | `CountingCache` (near 7178) | `list_facts` counts into `gets`, `remove_fact` into `puts`, so `a_tracked_file_resolves_from_its_item_with_no_extraction_no_source_and_no_index_traffic` keeps its zero |
| 1 | `crates/borax/tests/dispatch.rs` | `RecordOrderStore` (near 5711) | both forward to `inner`. Its `/operator` check is unchanged |
| 1 | | `WriteFailingCache` (near 6577) | as in `store.rs` |
| 2 | `crates/borax/tests/library.rs` | the `Admitted` literals at 5849, 6890 and 6907 | `answers_with` set to the literal's `item` |
| 4 | `crates/borax/tests/event.rs` | `IndexEvidence` literals at 34 and 70 | `references: Vec::new()` |
| 4 | `crates/borax/tests/library.rs` | the `IndexEvidence` literal at 8475 | the same |
| 4 | `crates/borax/tests/pipeline.rs` | `default_content_index` (1188), and the `IndexEvidence` literals at 1327, 1362, 1689, 1759, 1904, 4359, 5177, 5209, 5254, 5287, 6882, 6928, 6966 and 6993 | the same |
| 4 | `crates/borax/tests/describe.rs` | `IndexReadStep::Hit` at 89 | `evidence: Some(…)`, `references: Vec::new()` |
| 4 | `crates/borax/tests/renaming.rs` | `IndexReadStep::Hit` at 187 | the same |
| 4 | `crates/borax/tests/event.rs` | `IndexReadStep::Hit` at 152, 3337 and 3763 | the same |
| 4 | `crates/borax/tests/pipeline.rs` | `IndexReadStep::Hit` at 1069, 1300, 6338 and 6980 | the same |
| 4 | `crates/borax/tests/dispatch.rs` | the pattern `IndexReadStep::Hit { evidence }` at 17343, in `index_held_retry_runs_no_title_check_writes_nothing_and_keeps_its_origin` | `Hit { evidence: Some(evidence), .. }` |
| 4 | | `sections_for`'s `IndexReadStep::Miss` at 507 and 574 | `Miss { references: Vec::new() }` |
| 4 | `crates/borax/tests/per_file.rs` | `IndexReadStep::Miss` at 277 and 329 | the same |
| 4 | `crates/borax/tests/describe.rs` | `IndexReadStep::Miss` at 178, 303, 347, 1378, 2245, 2311 and 2374 | the same |
| 4 | `crates/borax/tests/event.rs` | `IndexReadStep::Miss` at 108, 1414, 1446 and 1493 | the same |
| 4 | `crates/borax/tests/pipeline.rs` | `IndexReadStep::Miss` at 1307 | the same |
| 6 | `crates/borax/tests/library.rs` | the four `Adopted` literals in the `lookup` closures at 10332, 10389, 10445 and 10496 | `identifier: Some(…)`, `retrieval: RetrievedFrom::ServiceCache { service: "crossref" }` |
| 6 | | `Unadopted::Unanswered` at 10548, `Adoption::Unanswered` at 10567 | `identifier: Some(…)`, `references: Vec::new()` |
| 6 | `crates/borax/tests/dispatch.rs` | `Adoption::Unanswered` at 15960 | the same |
| 6 | `crates/borax/tests/event.rs` | `Adoption::Unanswered` at 706, 4283, 4340, 4413, 4488 and 4565 | the same. Every JSON and human expectation of these is unchanged, since the empty `references` is not serialized |

**Deliberate rewrites (b):**

| Group | File | Test | New form |
|---|---|---|---|
| 6 | `crates/borax/tests/dispatch.rs` | `adopt_writes_nothing_to_the_content_index` (15915) | renamed `adopt_writes_no_evidence_to_the_content_index`. Its assertion, `index.entry(&hash)` unchanged, still holds, since `entry` reads no reference. The old name became false: adoption writes references |

**Fixture helpers re-read and unchanged:**

- `adopt_over`, `adoptions`, `adopted_totals` and
  `assert_every_orphan_is_accounted_for`: reference events are not
  `library-adoption`, and the totals stay last.
- `reconcile_adapters`, `seed_tracked_file`, `seed_artifact_record`,
  `write_lib_artifact_record` and `hash_entry_for`: unchanged.
  `hash_entry_for` gives `item: None`, which is why the 13a reconcile
  tests write no reference.
- `stderr_of_rename`: no warning is added (D2), so every stderr count
  holds.
- `sections_for`: only the shape edit above.

Flagged by (1) and **unchanged**:

- every `IndexRead::Hit(…)` and `IndexRead::Miss` (`event.rs`,
  `pipeline.rs`): the variants keep their shape, and `Referenced` is
  added;
- the `match` at `pipeline.rs` 4495: it has a wildcard arm;
- every `RetrievedFrom::Library`, `ServiceCache` and `Network`
  constructor: the enum only gains a variant, and no test matches it
  exhaustively;
- every `WrittenEntry::Evidence` constructor, for the same reason.

## Not changed, deliberately

- What a library answers for its own tracked files, and that it is
  asked first.
- 13a's evidence: its files, writers, precedence and restoration.
  `IndexEntry::answer` is unchanged.
- Duplicate checks, the acceptance gate, minting and the append rule.
- `status` and `validate` write nothing. `resolve` and `bib` write no
  reference.
- `borax cache`, the response cache and `run-started`.

## Risks / Trade-offs

- **A referenced library's whole item store is read** the first time a
  run reaches a reference to it. That is once per root per run, but for
  one outside file it is every item file of that library.
- **Unpruned references are reported on every run** that reaches their
  bytes. A deleted copy's `missing` reference stays until an explicit
  operation exists (Deferred).
- **An unreadable reference file, or an unreadable `refs/`, is
  ignored.** A damaged file that would have disagreed cannot then stop
  the others answering. Withholding every reference for one damaged file
  would let it silence every reachable library, and the evidence
  remains the fallback. The next reconcile in the file's library
  rewrites it (D3, step 1).
- **A failed stale removal** leaves both files. D3's step 1 then finds
  the key held, so no later run removes the stale one.
- **A hand re-link** reaches the reference only at the next
  `reconcile` in that library, or the next admission of the file. A
  hand edit inside the item is visible at once.
- **A damaged link.** For a record linking no item, older bytes keep
  `item_for`'s item outside, while the library answers `no-item` for
  them inside (D2).
- **A root that is not UTF-8** can never be referenced, and every
  attempt is reported `failed`.
- **The data directory is per machine.** A copy on another machine
  writes its references into that machine's index.

## Illustrative schema-5 lines

A reference answer outside any library:

```json
{"schema":5,"event":"resolved","path":"/home/u/Downloads/paper.pdf","identifier":"doi:10.1039/c5ay00042d","record":{…},"library":{"status":"not-attempted","reason":"no-library"},"content_index":{"read":{"status":"hit","references":[{"library":"0190c3a2-7f21-7abc-8def-0123456789aa","root":"/home/u/papers","artifact":"0190c3a2-7f21-7abc-8def-0123456789ab","item":"0190c3a2-7f21-7abc-8def-0123456789ac","status":"answered"}]},"write":{"status":"not-attempted","reason":"content-index-hit"}},"extraction":{"result":{"status":"not-attempted","reason":"content-index-hit"},"titles":{"status":"not-attempted","reason":"content-index-hit"}},"identifier_input":{"status":"not-attempted","reason":"not-asked"},"lookup":{"status":"not-attempted","reason":"content-index-hit"},"record_retrieval":{"kind":"library-reference","library":"0190c3a2-7f21-7abc-8def-0123456789aa","root":"/home/u/papers","artifact":"0190c3a2-7f21-7abc-8def-0123456789ab","item":"0190c3a2-7f21-7abc-8def-0123456789ac"},"match_check":{"status":"not-attempted","reason":"content-index-hit"},"acceptance":{"status":"automatic"}}
```

An evidence answer after two references could not answer:

```json
"content_index":{"read":{"status":"hit","evidence":{"identifier":"doi:10.1000/y","origin":"supplied"},"references":[{"library":"0190…aa","root":"/media/old","artifact":"0190…ab","item":"0190…ac","status":"unavailable","cause":"missing"},{"library":"0190…aa","root":"/home/u/papers","artifact":"0190…ab","item":"0190…ad","status":"item-unavailable","cause":"dangling"}]},"write":{"status":"not-attempted","reason":"content-index-hit"}}
```

A disagreement:

```json
"references":[{"library":"0190…aa","root":"/home/u/a","artifact":"0190…ab","item":"0190…ac","status":"disagreeing","identifier":"doi:10.1000/x"},{"library":"0190…aa","root":"/home/u/b","artifact":"0190…ab","item":"0190…ae","status":"disagreeing","identifier":"doi:10.1000/y"}]
```

Writes:

```json
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"reference","library":"0190…aa","root":"/home/u/papers","artifact":"0190…ab","item":"0190…ac"},"write":{"status":"written"}}
{"schema":5,"event":"content-index-write","path":"/home/u/papers/Smith2015.pdf","hash":"sha256-…","entry":{"kind":"reference","library":"0190…aa","root":"/home/u/papers","artifact":"0190…ab","item":"0190…ac","repaired_from":["/media/old"]},"write":{"status":"written"}}
```

Adoption:

```json
{"schema":5,"event":"library-adoption","path":"new.pdf","adoption":{"kind":"recorded","id":"0190…","item":"0190…","retrieval":{"kind":"library-reference","library":"0190…aa","root":"/home/u/papers","artifact":"0190…ab","item":"0190…ac"}}}
{"schema":5,"event":"library-adoption","path":"scan.pdf","adoption":{"kind":"unanswered","references":[{"library":"0190…aa","root":"/media/old","artifact":"0190…ab","item":"0190…ac","status":"unavailable","cause":"missing"}]}}
```

Human lines:

```text
/home/u/Downloads/paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the library at /home/u/papers
/home/u/Downloads/paper.pdf: resolved doi:10.1000/y to "Another" (Jones, 2020) via crossref, from the response cache; a library reference could not answer: /media/old is missing; a library reference could not answer: the library at /home/u/papers holds no item 0190…ad
/home/u/Downloads/paper.pdf: resolved doi:10.1000/y to "Another" (Jones, 2020) via crossref, from the network; library references disagree: the library at /home/u/a names doi:10.1000/x, the library at /home/u/b names doi:10.1000/y
/home/u/papers/Smith2015.pdf: the content index could not keep its reference to the library at /home/u/papers (cannot write …: No space left on device)
scan.pdf: no library reference the content index holds for its bytes answered, and it holds no identifier for them, so it is still an orphan; a library reference could not answer: /media/old is missing
```

On disk:

```text
~/.local/share/borax/v2/content/sha256-3f…/extracted.json                {"identifier":"doi:10.1000/y"}
~/.local/share/borax/v2/content/sha256-3f…/refs/sha256-9c41….json       {"library":"0190…aa","root":"/home/u/papers","artifact":"0190…ab","item":"0190…ac"}
```
