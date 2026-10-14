## ADDED Requirements

### Requirement: A library decision is referenced from the content index after it is written
borax SHALL write a library reference to the content index only for bytes an artifact record already holds on disk, after that record is written, and only while the library's identity is present.

A reference names the library's identity, the library's root,
absolute and normalised as the library compares paths, the artifact
record's identity and the item the record answers with for those bytes.
It is written under those bytes' content hash, beside whatever the
entry already holds, as the `resolution` capability's "The content
index keeps identification evidence and library references" states.

A reference SHALL be written:

- after an applying run's admission of a file returns its record, for
  the file's hash. This covers an admission that wrote the record and
  one that found it already describing the file, so an applying run
  over a recorded file puts back a reference that is missing. A pending
  change, a file outside the library, and an admission whose write
  failed write no reference;
- after `borax adopt` records an orphan, for the orphan's hash;
- by `borax reconcile`, for the newest hash of every artifact record the
  library holds, after its own writes, whatever became of each record.

Nothing else SHALL write one. A preview, a run with duplicate detection
off, `borax status`, `borax validate`, `borax resolve` and `borax bib`
write no reference.

Immediately before writing references, borax SHALL read the library's
identity, and SHALL NOT assume one from a store write having succeeded.
With no identity file, it SHALL write no reference, and the run SHALL
warn once on standard error, naming the library and how many records
went unreferenced, and saying that its next store write gives it an
identity. Writing a reference SHALL NOT mint an identity: "A library's
first store write mints its identity" keeps minting to items and
artifact records. With an identity file that cannot be read, it SHALL
write no reference, and the run SHALL warn as that requirement warns,
once.

The order SHALL be the item, then the artifact record with its history
entry's identification evidence, then the reference, written by
replacing the content-index entry whole and atomically. So an
interruption can leave a decision in the library with no reference to
it, and SHALL NOT leave a reference as the only record of a decision.
The next `borax reconcile`, or the next applying run or adoption that
reaches the file, writes the missing reference.

Every step SHALL be idempotent. A reference SHALL be identified by its
library, its root and its artifact, and writing one the entry already
holds SHALL change only its item, and only when the library now answers
with another. When the entry holds no reference of that library, root
and artifact, but holds one of that library and artifact at another
root that no longer holds that library — the root is gone, or holds no
library identity or another one — that reference SHALL be repaired to
the new root rather than a second one added. A root that cannot be read
is not taken to be stale. No reference SHALL be removed from an entry,
whatever its library's state: two roots of one library are two
references, and a reference that can no longer answer stays until an
explicit operation removes it.

When the entry holds no identification evidence, the reference write
SHALL also give it the evidence the artifact record's history holds for
those bytes, if any.

A reference write that fails SHALL be reported, as the `resolution`
capability's "What became of remembering an operator's answer is
reported" states, and SHALL cost nothing else: the admission, adoption
or repair it followed stands, and the run's exit status does not depend
on it.

#### Scenario: A recorded file is referenced
- **WHEN** `borax rename --apply` renames and records a file in a
  library whose identity is present
- **THEN** after its artifact record is written, the content index
  entry for the file's hash holds a reference naming the library's
  identity, its root, the artifact and its item

#### Scenario: An interrupted admission is completed later
- **WHEN** an applying run wrote a file's item and artifact record and
  was stopped before writing the reference, and `borax reconcile` runs
- **THEN** the reference is written, and the record is byte-identical

#### Scenario: A reconcile over referenced records writes nothing to the index
- **WHEN** `borax reconcile` runs twice over a library nothing has
  touched, whose identity is present
- **THEN** the second run writes no content-index entry and reports no
  `content-index-write` event

#### Scenario: A moved library is found again
- **WHEN** a library whose records are referenced from `/old` is moved
  to `/new`, and `borax reconcile` runs at `/new`
- **THEN** each of those references names `/new` afterwards, and none
  is added beside it

#### Scenario: A copy is a second location
- **WHEN** a library at `/a` is copied to `/b`, and `borax reconcile`
  runs at `/b`
- **THEN** the entries hold references to both `/a` and `/b`

#### Scenario: A deleted copy is kept
- **WHEN** `/a` is then deleted and `borax reconcile` runs at `/b`
- **THEN** the references to `/a` are still in the entries, and nothing
  is written for them

#### Scenario: A library with no identity is not referenced
- **WHEN** `borax rename --apply` re-records an already-recorded file
  in a library an earlier release wrote, which has no identity file
- **THEN** no identity is minted, no reference is written, and the run
  warns once that the library has no identity yet

#### Scenario: A reference that cannot be written
- **WHEN** the content index cannot be written and an applying run
  records a file
- **THEN** the file is renamed and recorded, the failure is reported,
  and the run's exit status is what it would have been

#### Scenario: Reading writes no reference
- **WHEN** `borax status`, `borax validate`, `borax resolve` and a
  preview run over a library whose records no reference names
- **THEN** none of them writes a reference

## MODIFIED Requirements

### Requirement: An artifact record is authoritative state keyed by an artifact identity
An artifact record SHALL be authoritative library state stored flat
under `.borax/artifacts/`, one file per artifact, keyed by a minted
artifact UUID. It SHALL hold the identity of the item it is linked to
when it has one, the artifact's hash history oldest first, the
artifact's last-known library-relative path, and the size and
modification time reconciliation's fast path compares.

Each entry of the hash history SHALL carry, beside the hash, the run
identifier, the timestamp and the borax version that recorded it. That
is the provenance the retired ledger carried a row per admission for —
which run admitted this artifact, when, and under which version —
folded into the record so that one file answers it. Neither the file's
modification time nor the time inside a UUID substitutes for it: the
first is the artifact's and the second is the identity's.

Each entry borax writes SHALL also name the item its bytes were
accepted under, as "Changed bytes become an artifact's history only
once accepted" accepts them: the item a minted record links, the item a
record links once an acceptance of changed bytes is written, or the
item a re-identification links. An entry recorded before entries named
items names none, and SHALL remain valid and readable exactly as it was
written: nothing rewrites it, and where it decides which item answers
for its bytes, the record's link answers, as the `resolution`
capability states.

Each entry borax writes SHALL also carry the identification evidence of
its bytes, where the bytes were identified: the identifier they were
identified by and its origin — `extracted`, `supplied`, `overridden` or
`adopted`, as the `resolution` capability's "The content index keeps
identification evidence and library references" defines them. That is
the identifier an applying run's record was reached by, with the origin
of how it was reached (an identifier the content index held keeps the
origin it was written under); the identifier reconciliation accepted
changed bytes on, `extracted`; and the identifier an adoption's record
was found by, `adopted`. An entry for bytes no identifier established
carries none: an acceptance by `borax reconcile --accept`, a
restatement, an admission whose record a library item supplied with no
evidence held for those bytes, and an adoption whose record carries no
identifier. The
identifier and the origin are kept together or not at all. An entry
holding one without the other, an identifier that does not parse, or
an origin outside those four makes its record a file that does not
parse. An entry recorded before entries carried evidence carries none,
and SHALL remain valid and readable exactly as it was written. This is
where an operator's decision about a file lives inside a library: the
content index refers to it and keeps nothing it alone knows.

Every decision about which item a record's bytes belong to SHALL be
tied to those bytes. The bytes a record holds answer as follows:

- the newest entry's bytes answer with the record's link;
- other bytes answer with the item named by the newest entry holding
  them, or with the link when that entry names none;
- a record that links no item answers for no bytes, as the `resolution`
  capability states.

Two rules keep this true, and both keep the history append-only:

- **A decision is recorded.** Whenever an admission decides that bytes
  belong to an item, it SHALL append an entry for those bytes under
  that item. That covers accepting changed bytes, re-identifying a file,
  re-linking a record, and giving an item to a record that linked none.
  The one exception is bytes that are already the newest entry's and
  already answer with that item once the admission is made.
- **A hand correction is restated.** This is judged on the record as it
  was read, before the admission changes its link. When that record
  links an item, its newest entry names another item or none, and the
  admission is about to append an entry for other bytes, the admission
  SHALL first append a restatement of the newest entry's bytes under
  the link as read. A restatement is an ordinary entry, stamped with
  the run that writes it. A link corrected in a text editor applies to
  the bytes that were newest when it was corrected, and the
  restatement is what keeps that correction for those bytes once later
  bytes are accepted.

So the history grows by at most one entry for each explicit operator
re-identification, plus at most one restatement, and by nothing on an
ordinary re-run.

The key SHALL NOT be a content hash. Annotating or otherwise editing a
file changes its bytes, so a hash-keyed record would have to be renamed
when its artifact was edited, and an identity that moves when the
content moves is not an identity. The artifact UUID SHALL remain stable
across both a move of the file and a change to its bytes.

Hash history SHALL be retained rather than replaced, so that borax can
recognise that an artifact has changed and can still recognise the
content identities it had before. An artifact record SHALL be able to
outlive its artifact, keeping the fact that the library once held the
file; a record stored beside its artifact would be destroyed by an
ordinary deletion and lose that.

Every write borax makes to a record SHALL replace the whole file
atomically — the bytes written to a temporary in the same directory and
renamed over the destination, as `write_atomically` in
`crates/borax-sources/src/store.rs` already does. So a write that fails
or is interrupted SHALL leave the record exactly as it was, with its
artifact identity, its whole hash history and its item link intact, and
a concurrent reader SHALL see either the previous record or the new one
and never a partial file. borax SHALL NOT append to, truncate, or
rewrite a record in place. Nothing that an interrupted borax write can
produce is recoverable by resolving the file again: a hash history is
evidence, not a derivation.

Artifact-specific state SHALL be recorded in the artifact record rather
than written into the artifact, because borax never modifies an
artifact's bytes. This is the first authoritative content `.borax/`
holds: the run logs there are evidence of what a run did, and deleting
an artifact record loses state nothing can reconstruct.

An artifact record SHALL NOT be confused with a citation sidecar, and
neither SHALL substitute for the other. An artifact record is borax's
authoritative binding state, named by artifact identity, living flat
under `.borax/artifacts/`, always kept. A `.bib` sidecar is optional
derived citation output for other tools, named for the file it sits
beside, off by default, and load-bearing for nothing — the `bib-output`
capability states it. A library with every sidecar deleted is complete;
a library with its artifact records deleted has lost its account of
itself.

#### Scenario: An annotated artifact keeps its identity
- **WHEN** an artifact with a record is annotated in a PDF reader so
  that its size or modification time changed, its text still carrying
  the DOI its item carries, and `borax reconcile` runs
- **THEN** its record is the same file under the same artifact UUID, its
  new hash is appended with the run that recorded it, the item it was
  accepted under, and the DOI with origin `extracted`, and its earlier
  hashes and their provenance are still there

#### Scenario: An operator's identifier is recorded with its bytes
- **WHEN** an interactive run inside a library renames a file from an
  identifier the operator supplied
- **THEN** the history entry for the file's bytes names the item, the
  supplied identifier, and the origin `supplied`

#### Scenario: An accepted change names no identifier
- **WHEN** `borax reconcile --accept` accepts a pending change at a
  scan from which nothing was extracted
- **THEN** its history entry names the record's item and carries no
  identifier and no origin

#### Scenario: A record written before entries named items
- **WHEN** an artifact record whose every history entry was written by
  an earlier release, naming no item and no identifier, is read,
  validated and reconciled over a library in which nothing has changed
- **THEN** it parses, `borax validate` reports no finding about it,
  `borax reconcile` writes nothing to it, and its file is
  byte-identical afterwards

#### Scenario: A hand correction is kept for the bytes it corrected
- **WHEN** a record holds bytes A in an entry naming item X, its link
  has been corrected in a text editor to item Y, and `borax reconcile`
  accepts edited bytes B under Y
- **THEN** the history gains an entry restating A under Y and then the
  entry for B under Y, with every earlier entry unchanged, and A, if
  restored, answers with Y

#### Scenario: Re-identifying restored bytes ties the link to them
- **WHEN** a record holds A under X and newer B under Y, links Y, and an
  applying run re-identifies restored A as X
- **THEN** the history gains an entry for A under X and the link moves
  to X, and B, if restored, still answers with Y

#### Scenario: Re-identifying restored bytes as the link's own item
- **WHEN** a record holds A under X and newer B under Y, links Y, and an
  applying run re-identifies restored A as Y
- **THEN** the link stays Y, the history gains an entry for A under Y,
  and A answers with Y from then on

#### Scenario: Ordinary re-runs add nothing
- **WHEN** ten ordinary applying runs reach a restored earlier version
  of a recorded artifact in turn
- **THEN** its history is exactly what it was before the first, and the
  version still answers with the item it was accepted under

#### Scenario: The first acceptance in a record written before entries named items
- **WHEN** a record whose one entry, for bytes A, names no item, and
  which links item X, has edited bytes B accepted
- **THEN** the history gains an entry restating A under X and then the
  entry for B, and the earlier entry is unchanged

#### Scenario: A record outlives its artifact
- **WHEN** an artifact with a record is deleted from the library
- **THEN** the record is still there, still naming its last-known path
  and its item, and the library reports that it cannot find that
  artifact

#### Scenario: A write cut off mid-way loses nothing
- **WHEN** borax is killed while updating an artifact record
- **THEN** the record on disk is the one that was there before, with its
  identity, hash history and item link unchanged, and no temporary file
  is left where a reader would take it for a record

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
holds about a library is derived from the library: its references name
artifact records and items the library keeps, and the identifiers it
keeps for the library's files are in their history entries too. So
deleting the content index loses no library content either, and costs
answers outside the library until `borax reconcile` writes its
references again. A citation sidecar is derived
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
  a library whose identity is present
- **THEN** every artifact record's newest bytes are referenced again,
  with the identification evidence their history entry holds

#### Scenario: A text editor is a peer
- **WHEN** an item's title is corrected in a text editor and `borax
  status` runs
- **THEN** the correction is what borax reports, with nothing to
  reindex or refresh

### Requirement: Validation defines a well-formed library
`borax validate` SHALL define what a well-formed library is, and every
writer SHALL be judged by it rather than gated on it. The invariants
live in the validator and not in an application-service layer: borax's
own CLI is one convenient writer, and a file-manager move or a
text-editor edit is checked by exactly the same rules.

`borax validate` SHALL report at minimum these findings:

- an artifact record naming an item the library does not hold;
- two artifact records carrying one artifact identity, or two item files
  carrying one item identity;
- a record or an item file whose identity disagrees with the UUID in its
  file name;
- an artifact record whose last-known path is not a library-relative
  path under the root;
- an artifact record with an empty hash history, or one holding a
  malformed hash, or a history entry naming no run;
- a file in the item store that does not parse as an item record,
  including one whose verbatim source fields do not parse as JSON;
- a library identity file, `.borax/library.toml`, that cannot be read
  or does not parse as a library identity, including one whose `id` is
  not the canonical spelling of a UUID.

The item a history entry names SHALL NOT be a finding, whether the
entry names none, as every entry recorded before entries named items
does, or names an item the library does not hold, which is the history
of an item deleted since: deleting an item nothing links to is an
ordinary operation, and the entry still records what those bytes were
accepted as. An entry whose item is not a canonical UUID makes its
record a file that does not parse, and is reported as such a file is.

The identification evidence a history entry carries SHALL NOT be a
finding either, nor its absence: an entry with no identifier and no
origin is valid, as every entry recorded before entries carried
evidence is. An entry holding an identifier without an origin, or an
origin without an identifier, or an identifier that does not parse, or
an origin other than `extracted`, `supplied`, `overridden` and
`adopted`, makes its record a file that does not parse, and is reported
as such a file is. Whether the content index refers to a record is not
validated: the content index is not the library's.

A library with no identity file SHALL NOT be a finding or a condition:
it is the healthy state of a library no borax store write has reached
since identities were introduced. An identity file that cannot be read
or does not parse SHALL be reported as such a file, before every
finding about the item store and the artifact store. `borax validate`
SHALL mint no identity, and SHALL carry the library's identity as the
`id` of `library-validated`, and as a closing `; library <uuid>` clause
on its totals line, when the identity file holds one; with none, or
with one that cannot be read, it carries neither.

`borax validate` SHALL repair nothing and SHALL refuse nothing. A
finding is a report about a library, not a rejected write: a writer that
would produce one is not stopped, because a tool that refuses is the
enforcing tool this design exists not to be. `borax validate` SHALL
exit with the partial-success code when it reports any finding and 0
when it reports none.

An orphan, an artifact borax cannot find, and an item nothing links to
SHALL be reported as conditions rather than as findings: each one named
by a `library-condition` event of its own, as "Library conditions are
named where they are counted" requires, and each kind reported as a
count. None is a malformed library: the first is work to do, the second
is history the library deliberately keeps, and the third is an ordinary
item for a work with no file. Naming them SHALL leave the findings
exactly as they are: no condition is a finding, and no finding is
reported or withheld because a condition is named beside it.

No invariant SHALL depend on holding continuously, and no advisory lock
SHALL be a precondition for correctness. A library has no single
consistent state at any given moment, because its writers are peers: a
reader may observe an out-of-band edit or a multi-file operation in
progress. A lock MAY coordinate borax's own writes, and a file manager
will not take it, so validation SHALL be correct on a library observed
mid-edit — a file that does not parse is a finding about that file and
about nothing else. Since every borax write replaces a whole file
atomically, such a file came from a peer writer rather than from a borax
run cut short. The one exception is a library identity file: on a
filesystem that cannot give one file two names, a mint interrupted
mid-write can leave it short, and it is then a finding about that file
like any other.

#### Scenario: A dangling item link
- **WHEN** an item file is deleted and an artifact record still names it
- **THEN** `borax validate` reports that record as naming an item the
  library does not hold, and exits with the partial-success code

#### Scenario: Validation refuses nothing
- **WHEN** a library holds a finding and an applying `rename` run is
  made over it
- **THEN** the run proceeds and records what it admits, and the finding
  is still reported by `borax validate` afterwards

#### Scenario: A library observed mid-edit
- **WHEN** `borax validate` runs while a text editor holds one artifact
  record half-written
- **THEN** it reports a finding about that file, reports the rest of the
  library as it is, and holds no lock over either

#### Scenario: A clean library exits 0
- **WHEN** `borax validate` runs over a library whose records and items
  agree
- **THEN** it reports no finding and exits 0, whatever the number of
  orphans and whatever the number of items nothing links to

#### Scenario: Conditions are named beside the findings
- **WHEN** `borax validate` runs over a library holding an artifact
  record whose file is at its last-known path and which names an item
  the library does not hold, an orphan, and an item nothing links to
- **THEN** the dangling link is reported as a finding, the orphan and
  the item are each named by a `library-condition` event after it, the
  totals count 1 finding, 1 orphan and 1 unlinked, and the exit code is
  the partial-success code because of the finding alone

#### Scenario: A missing artifact alone exits 0
- **WHEN** `borax validate` runs over a library whose records and items
  agree except that one record's last-known path holds no file
- **THEN** it names that record as `missing`, reports no finding, and
  exits 0

#### Scenario: Every check still reports
- **WHEN** `borax validate` runs over a library holding one instance of
  every finding listed above beside an orphan, a missing artifact and
  an unlinked item
- **THEN** each of those findings is reported as a finding and counted
  in the totals, and none of the three conditions is reported as a
  finding

#### Scenario: History entries name no item, or a deleted one
- **WHEN** `borax validate` runs over a library holding an artifact
  record that links an item the library holds, whose first history
  entry names no item and whose second names an item no item file
  carries
- **THEN** no finding is reported about either entry, and the run exits
  0

#### Scenario: Evidence on history entries
- **WHEN** `borax validate` runs over a library holding an artifact
  record whose first entry carries no identifier and whose second
  carries a DOI with origin `supplied`, and another record whose entry
  carries an origin of `guessed`
- **THEN** no finding is reported about the first record, and the second
  is reported as a file that does not parse

#### Scenario: An identity file that does not parse
- **WHEN** `borax validate` runs over a library whose records and items
  agree and whose `.borax/library.toml` holds `id = "not-a-uuid"`
- **THEN** it reports one finding, about that file, as unreadable,
  `library-validated` carries no `id`, the file is byte-identical
  afterwards, and the run exits with the partial-success code

#### Scenario: A library with no identity validates clean
- **WHEN** `borax validate` runs over a library whose records and items
  agree and which has no `.borax/library.toml`
- **THEN** it reports no finding, names no condition about an identity,
  creates no identity file, and exits 0

#### Scenario: Validation reports the identity
- **WHEN** `borax validate` runs over a library whose
  `.borax/library.toml` holds an identity
- **THEN** `library-validated` carries that identity as `id`, and the
  totals line ends with `; library ` followed by it

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
other.

For each orphan in the library, `borax adopt` SHALL hash the artifact
and consult the content index for that hash. Where the index holds the
record borax resolved for those bytes, the run SHALL write an artifact
record for the artifact and link it to the item the library already
holds for one of that record's identifiers, or to one it mints from
that record. The record is found, in this order, without the network:

1. through the entry's library references, answering as the
   `resolution` capability's "A content-index entry answers through its
   references, then its identifier" lets them answer — one reachable
   reference, or several that agree — with the referenced item's
   record;
2. through the entry's identifier, from the response cache: the answer
   a service gave for that identifier, read for each service in the
   priority order resolution asks them in for that kind of identifier,
   whatever services the run's configuration names, the first one held
   being taken.

The adopted record's history entry SHALL name the identifier it was
found by, with the origin `adopted`, or none when it carries none. The
run SHALL report where the record came from: the library reference
that answered, or the service whose response cache held it. After the
pass, each adopted record is referenced from the content index, as "A
library decision is referenced from the content index after it is
written" states.

Where the index holds nothing for the bytes, the artifact SHALL remain
an orphan, and the run SHALL report it, naming the artifact, as an
orphan the content index holds no record for. Where the index holds an
entry and neither a reference nor the response cache supplies a
record, the artifact SHALL remain an orphan too, reported as one the
content index could not answer for, with the identifier the entry
holds, when it holds one, and what each reference came to. `borax
resolve` over such a file fetches the record its identifier names, and
an adoption afterwards records it.

Every orphan the run reaches SHALL be reported by exactly one
`library-adoption` event, whose kind says what became of it: recorded,
held by an existing record, unreadable, unwritten, `unindexed` when
the content index holds nothing for its bytes, or `unanswered` when it
holds an entry that could not supply a record. No orphan is left an
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
records only what a library reference answers for. The content index
is not a cache, and survives it. What `borax adopt` offers is not a
guarantee of recovery but the one path that records a library without
querying a service and without moving a file, which `borax rename
--apply` cannot promise.

#### Scenario: A library borax has seen before
- **WHEN** `borax adopt` runs over a library whose files borax resolved
  on an earlier run, so the content index answers for their hashes and
  the response cache holds their records
- **THEN** each of those artifacts gains an artifact record and an item
  holding the cached record, no service is queried, and no file is
  renamed, moved or deleted

#### Scenario: Adopting what another library recorded
- **WHEN** `borax adopt` runs over a library holding an orphan whose
  bytes another library, reachable at its root, recorded and referenced
- **THEN** the orphan is recorded from that library's item, its event
  names the reference it came from, its history entry carries the
  item's DOI with origin `adopted`, and the content index entry for its
  bytes then references both libraries

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
  library whose orphans' entries hold identifiers and no references
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
