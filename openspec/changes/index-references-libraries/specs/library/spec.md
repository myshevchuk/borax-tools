## ADDED Requirements

### Requirement: A library's references are written after its records, and reconcile restores and repairs them
borax SHALL write a library reference to the content index only after the artifact record it names is on disk, and only when the library's identity, read at that moment, is present; it SHALL write one for every hash an artifact record answers for with an item, as the runs below reach them.

A reference names the library's identity, its root, an artifact record
and the item that record answers with for the bytes, as the
`resolution` capability's "The content index keeps references to
libraries, one file each" states. Its item SHALL be the item the record
answers with for those bytes once the write the reference follows is
made: the record's link for its newest bytes, and otherwise the item
the newest entry holding them names, or the link where that entry names
none.

The writers are three, and nothing else writes a reference:

- **An applying rename**, after each admission that records the file:
  one that mints a record, appends to one, re-links one, separates the
  file into a new artifact, or leaves the record as it was. It writes
  the reference for the admitted bytes and the artifact admitted. A
  separated file's reference names its new artifact, and the record it
  left keeps its references. An admission reported `pending`,
  `outside` or `unwritten`, a preview, and a run with `--no-record`
  write none.
- **`borax adopt`**, after its pass, for each orphan it recorded.
- **`borax reconcile`**, after its repairs and its restorations of
  operator decisions. It writes for every distinct hash every artifact
  record's history holds, each history read afresh, not only the newest
  hash.

`borax status`, `borax validate`, `borax resolve`, `borax bib` and
`borax cache` SHALL write no reference.

The identity SHALL be read immediately before an applying rename writes
a reference, and once by `adopt` and by `reconcile` after their own
store writes. A store write earlier in the same run may have minted it.
A reference SHALL name the identity read, and SHALL be written only when
it is present. A write of a reference SHALL NOT mint an identity.
Where the identity file is absent, or cannot be read, no reference is
written, and nothing more is reported than the requirement "A library's
first store write mints its identity" reports: a library with no
identity is well formed, as "A library carries an identity that does
not depend on where it is" states.

Bytes for which the record answers with no item, because the record
links none and no entry for them names one, SHALL have no reference
written, and nothing is reported about them: `borax status` and `borax
validate` count the record as unlinked. A reference written before the
record lost its link is left as it is.

Writing a reference SHALL be an upsert:

- where the content index already holds a reference for the same
  library, root and artifact naming the same item, nothing is written
  and nothing is reported;
- where it holds one naming another item, or one that cannot be read,
  that reference is replaced, which is how a link corrected by hand
  reaches the content index;
- where it holds none, the reference is written, and then every other
  reference to the same library and artifact whose root is stale is
  removed. A root is stale when it does not exist, is not a directory,
  holds no library identity, or holds another library's identity.

A root holding the same library's identity is a copy, and its reference
is not stale. Neither is one whose root or identity file cannot be
read. A reference whose own key the content index already held SHALL
NOT be removed, nor one for another artifact or another library, nor
one that cannot be read. A write that fails removes nothing. A removal
that fails leaves that reference, and is reported.

So `borax reconcile` is the command that restores a library's
references. After it, the content index holds a reference for every
hash its records answer for with an item, at the root it ran at, naming
that item. That covers a reference an interrupted admission never
wrote, and every hash of a library that has moved. An applying rename
writes and repairs only the reference of the bytes it admits.
`borax adopt` writes only those of the orphans it records: it never
reaches a file a record names, so it restores nothing for one.

Reference writes go to the content index alone. They SHALL write no
library state and no identification evidence, so a reconcile over a
library nothing has touched still leaves every file of the item store
and of `.borax/artifacts/` byte-identical.

#### Scenario: An interrupted admission is completed by reconcile
- **WHEN** an applying rename records a file in a library that has an
  identity and stops before writing its reference, and `borax
  reconcile` then runs over the library
- **THEN** the content index holds the reference the rename would have
  written, and a later run over a copy of the file outside the library
  is answered from its item

#### Scenario: Every hash of every history
- **WHEN** a record holds earlier bytes accepted under one item and its
  newest bytes linked to another, and `borax reconcile` runs over a
  content index holding neither reference
- **THEN** it writes both: the earlier bytes' naming the item they were
  accepted under, the newest bytes' naming the link

#### Scenario: A record that links no item
- **WHEN** `borax reconcile` runs over a library one of whose records
  links no item and names none for its bytes
- **THEN** no reference is written for those bytes, nothing is reported
  about them, and the record is counted unlinked by `borax validate`

#### Scenario: A library with no identity
- **WHEN** an applying rename records nothing new in a library with no
  `.borax/library.toml`, and leaves its record as it was
- **THEN** no identity is created, no reference is written, and nothing
  is reported about either

#### Scenario: A library whose identity cannot be read
- **WHEN** an applying rename records a file in a library whose
  `.borax/library.toml` does not parse
- **THEN** the file is recorded, no reference is written, and the run
  warns once, as for any store write into such a library

#### Scenario: A moved library is repaired
- **WHEN** a library at `/media/old` is moved to `/home/u/papers`, and
  `borax reconcile` runs at its new root
- **THEN** each of its references is written for `/home/u/papers`, and
  the one for `/media/old` is removed after its replacement is written

#### Scenario: A copy adds a reference
- **WHEN** a library is copied to another root while the original stays,
  and `borax reconcile` runs in the copy
- **THEN** the content index holds a reference for each root, and
  neither is removed

#### Scenario: A deleted copy is not pruned
- **WHEN** one of two referenced copies of a library is deleted, and
  `borax reconcile` runs in the other
- **THEN** the deleted copy's reference is kept, and reported unavailable
  by any run its bytes reach

#### Scenario: A hand re-link reaches the index
- **WHEN** an artifact record's link is changed in a text editor, and
  `borax reconcile` runs
- **THEN** the reference for its newest bytes names the new item

#### Scenario: Adoption restores nothing for a recorded file
- **WHEN** an applying rename recorded a file and stopped before its
  reference, and `borax adopt` runs over the library
- **THEN** the file is not an orphan, and no reference is written for it

## MODIFIED Requirements

### Requirement: The library's text is authoritative and is read directly
The item store, the artifact records and the library's identity file SHALL
be the authoritative state of a library, and every answer borax gives
about a library SHALL be read from those files directly. There SHALL be
no index, no database and no cached listing in this change: a count, a
listing and a validation are each a walk of the tree and a read of the
store.

Deleting any derived state SHALL change no answer about a library. The
response cache is a cache, so clearing it costs round trips and loses
no library content. The content index is not a cache, but what it
holds about a library's files is derived from the library: the
operator identifiers it keeps for them are in their history entries
too, and its references to the library name the library's own
identity, artifact records and items. So deleting the content index
loses no library content either, and costs answers outside the library
until `borax reconcile` writes those identifiers and references again
and runs over the files extract theirs again.
A citation sidecar is derived
output, so deleting every sidecar in a library changes no item, no
artifact record and no count. The identity file is not derived state:
deleting it loses the library's identity, which nothing reconstructs
and the next store write replaces with another, as "A library carries
an identity that does not depend on where it is" states.

The direct implementation is deliberate. It is what demonstrates that
the files rather than a program's index are the library, and it is the
baseline any later measurement that argues for an index is taken
against.

#### Scenario: Derived state is disposable
- **WHEN** the response cache, the content index and every citation
  sidecar in a library are deleted and `borax status` runs
- **THEN** the artifact, item, record and orphan counts are what they
  were, and so is the library's identity

#### Scenario: The index is rebuilt from the library
- **WHEN** the content index is deleted and `borax reconcile` runs over
  a library whose records hold bytes operators identified
- **THEN** the operators' identifiers for those bytes are in the
  content index again, and so are the library's references for every
  hash its records answer for with an item, where the library has an
  identity

#### Scenario: A text editor is a peer
- **WHEN** an item's title is corrected in a text editor and `borax
  status` runs
- **THEN** the correction is what borax reports, with nothing to
  reindex or refresh

### Requirement: borax adopt records what the library already holds
`borax adopt` SHALL give recorded state to the artifacts already on disk,
from what borax already knows locally, and SHALL write no library state
but the artifact records under `.borax/artifacts/`, the items it
links them to, and the library's identity, which its first record
mints when the library has none, as "A library's first store write
mints its identity" states. It SHALL move, rename and delete nothing at all. It
queries no service and extracts from no document: it is the offline way
to record a library, and the one operation that populates the store
without the possibility of a move.

Writing no other library state does not mean writing no other file. Like
every subcommand, `borax adopt` writes a run log, which the `cli`
capability makes operative on all of them and the `run-logs` capability
governs unchanged. That is the distinction `.borax/` has carried since
it existed: evidence about what a run did, beside authoritative state
about what the library is. A run log is the first and the library's
state is the second, and a claim about one says nothing about the
other. The references to the library it writes to the content index,
which the next paragraphs state, are not library state either.

For each orphan in the library, `borax adopt` SHALL hash the artifact
and consult the content index for that hash. Where the index holds the
record borax resolved for those bytes, the run SHALL write an artifact
record for the artifact and link it to the item the library already
holds for one of that record's identifiers, or to one it mints from
that record. The record is found without the network, in this order:

1. **A library reference.** The entry's library references are read
   and settled as the `resolution` capability's "A library reference
   answers for a file before the entry's evidence" states. One that
   answers gives the record of the item it names, from that library.
2. **The response cache.** Otherwise the identifier the content index
   answers with for those bytes, the operator's before the extracted
   one, is read from the response cache: the answer a service gave for
   that identifier, read for each service in the priority order
   resolution asks them in for that kind of identifier, whatever
   services the run's configuration names, the first one held being
   taken.

A record found through an identifier SHALL give the adopted history
entry that identifier, with the origin `adopted`. A record found
through a reference was found by no identifier, so its history entry
carries no identification evidence. The run SHALL report where the
record came from: the service whose response cache held it, or the
library, root, artifact and item the reference names. Adoption writes
no identification evidence to the content index: the identifier it used
is already there, and its own origin belongs to the library.

After its pass, for each orphan it recorded, adoption SHALL write the
reference to this library, as "A library's references are written after
its records, and reconcile restores and repairs them" states. It writes
no reference for an orphan it left, and none for a file a record
already names.

Where the index holds nothing for the bytes, the artifact SHALL remain
an orphan, and the run SHALL report it, naming the artifact, as an
orphan the content index holds no record for. Where the index answers
with an identifier and the response cache holds no record for it, the
artifact SHALL remain an orphan too, reported as one the content index
could not answer for, with that identifier. `borax
resolve` over such a file fetches the record its identifier names, and
an adoption afterwards records it. Where the entry holds references and
none answered, the orphan is reported as one the content index could
not answer for as well, naming each reference with what became of it,
and with the identifier only where the entry held one. An entry
holding no evidence that answers and no reference that can be read is
one that holds nothing, and only such an orphan is reported
`unindexed`.

Every orphan the run reaches SHALL be reported by exactly one
`library-adoption` event, whose kind says what became of it: recorded,
held by an existing record, unreadable, unwritten, `unindexed` when
the content index holds nothing for its bytes, or `unanswered` when
nothing it holds produced a record: an identifier whose record the
response cache does not hold, references none of which answered, or
both. No orphan is left an
orphan silently, and the number of orphans the run reports as left
SHALL equal the number of its `library-adoption` events that did not
record the artifact. The human line for an unindexed or unanswered
orphan names its path, with control characters written out, and says
that it is still an orphan. An orphan is reported `unindexed` or
`unanswered` only when it was readable and no artifact record holds its
bytes, since only then is the content index asked about it. Such an
orphan is not a skip or a finding, and the run's exit status does not
depend on it.

An orphan whose content hash is already held in the history of an
artifact record SHALL NOT be adopted, whatever the index holds for it,
and the run SHALL report it naming that record. Such a file is that
record's artifact moved out of band, or a byte-identical copy of it.
A second record for the first would give one artifact two identities:
the next reconcile would settle the new record at the file's path, and
the old one — with its item link and its history — could never be
repaired again. `borax reconcile` is what brings a moved artifact's
record to it, and the report says so. The same holds for an orphan
whose bytes an earlier orphan of the same run was adopted with.

Nothing SHALL be invented. `borax adopt` mints no title, no author and
no identifier, and it extracts nothing from the artifact itself: a file
whose bytes borax has never resolved is a file borax knows nothing
about, and an orphan is the honest report of that.

`borax adopt` SHALL be idempotent and SHALL leave an artifact that
already has a record exactly as it is, including its item link. Running
it twice SHALL write no library state the second time.

`borax adopt` SHALL NOT read a citation sidecar, and SHALL NOT read
`.borax/ledger.jsonl`. The first is derived output that governs no
decision; the second is the retired ledger, which this change neither
migrates nor consults. Both are left exactly where they are.

The response cache is a cache, and this requirement does not pretend
otherwise: `borax cache --clear` empties it, and adoption after that
records nothing through an identifier until the records are fetched
again. The content index is not a cache, and survives it, so the
identifiers adoption needs are kept, and so are its references, whose
records come from a library and not from the response cache. What `borax adopt` offers is not a
guarantee of recovery but the one path that records a library without
querying a service and without moving a file, which `borax rename
--apply` cannot promise.

#### Scenario: A library borax has seen before
- **WHEN** `borax adopt` runs over a library whose files borax resolved
  on an earlier run, so the content index answers for their hashes
  with identifiers, no library reference answering, and the response
  cache holds their records
- **THEN** each of those artifacts gains an artifact record and an item
  holding the cached record, no service is queried, and no file is
  renamed, moved or deleted

#### Scenario: What adoption leaves alone
- **WHEN** `borax adopt` runs over a library holding an artifact the
  content index cannot answer for, and another that already has a record
- **THEN** the first is still an orphan, is reported as one the content
  index holds no record for, and is counted as one, the second is
  byte-identical including its item link, and a second run of the
  command writes no library state

#### Scenario: A recorded artifact moved out of band is not adopted
- **WHEN** a recorded artifact is moved within the library by a file
  manager, and `borax adopt` runs with the content index answering for
  its bytes
- **THEN** it is reported as held by its existing record and left an
  orphan, no record and no item is written for it, and a
  `borax reconcile` afterwards repairs the existing record's path with
  its identity and item link unchanged

#### Scenario: Adoption reads neither the sidecars nor the old ledger
- **WHEN** `borax adopt` runs over a library whose files have sidecars
  and which holds a `.borax/ledger.jsonl` from an earlier version, with
  the content index empty
- **THEN** nothing is adopted, every artifact is still an orphan, and
  both the sidecars and the ledger file are byte-identical afterwards

#### Scenario: Adoption after the cache is cleared
- **WHEN** `borax cache --clear` runs and then `borax adopt`, over a
  library whose orphans' entries hold identifiers and no reference
- **THEN** nothing is adopted and the run reports every artifact as an
  orphan, rather than reporting a failure; each orphan that is readable
  and whose bytes no artifact record holds is reported by its own event
  as one the content index could not answer for, naming the identifier
  the index holds, and the run exits 0

#### Scenario: Every orphan is accounted for
- **WHEN** `borax adopt` runs over a library holding one orphan the
  content index answers for, one whose bytes an existing artifact
  record holds, and one the content index holds nothing for
- **THEN** each of the three is reported by exactly one
  `library-adoption` event — recorded, held and unindexed in turn — and
  the totals count 1 adopted and 2 orphans left

#### Scenario: Adoption gives a library its identity
- **WHEN** `borax adopt` records an orphan in a library with no
  `.borax/library.toml`
- **THEN** the identity file exists afterwards, and a second run of the
  command leaves it byte-identical

#### Scenario: Adoption that records nothing mints nothing
- **WHEN** `borax adopt` runs over a library with no
  `.borax/library.toml` whose orphans the content index holds nothing
  for
- **THEN** no identity file is created

#### Scenario: Adoption through another library
- **WHEN** `borax adopt` runs, after `borax cache --clear`, over a
  library holding an orphan whose bytes another reachable library holds
  and a reference names
- **THEN** the orphan is recorded with that library's item's record, its
  adoption names the reference as where the record came from, its
  history entry carries no identifier, and a reference to this library
  is written for it

#### Scenario: References that could not answer
- **WHEN** `borax adopt` reaches an orphan whose only reference names a
  root that no longer exists, and whose entry holds no evidence
- **THEN** the orphan is reported as one the content index could not
  answer for, naming that reference as unavailable because its root is
  missing and naming no identifier, and it is still an orphan
