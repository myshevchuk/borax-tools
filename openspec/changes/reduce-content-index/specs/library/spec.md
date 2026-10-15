## ADDED Requirements

### Requirement: A library's operator decisions are restored to the content index
`borax reconcile` SHALL write to the content index, for every hash an artifact record's history holds, the operator's identification evidence that the newest entry for that hash with an operator's origin names, wherever the content index holds no operator evidence for that hash or holds different evidence.

An operator's decision about a file in a library is written in two
places, in this order: the artifact record's history entry, by the
admission, and then the content index's operator evidence for the
file's bytes, when the run remembers the answer at the move. The
library is authoritative for the bytes its records hold. An
interruption between the two writes leaves the decision in the library
alone, and an outside answer for those bytes then looks up whatever
the content index held before, which may be the extracted identifier
the operator corrected.

So, after its own writes, `borax reconcile` SHALL read every artifact
record of the library and, for each distinct hash its history holds,
take the newest entry holding that hash whose origin is `supplied` or
`overridden`. Where there is one, and the content index's operator
evidence for that hash is absent, cannot be read, or names another
identifier or origin, it SHALL write that entry's identifier and origin
as the hash's operator evidence. Where records disagree about the same
hash, the record read last, in the order of their files' paths, is the
one written. Where there is none, it SHALL write nothing for that hash, and SHALL leave any operator evidence the
content index holds for it as it is: those bytes may have been answered
for outside the library.

The library's decision wins over a different operator decision the
content index holds for the same bytes, so a later reconcile replaces
an identifier an operator supplied for a copy outside the library with
the one the library recorded. Between the two, the content index
answers with the latest operator decision written.

Restoration SHALL write nothing else: no extracted evidence, and no
library state, so a reconcile over a library nothing has touched still
leaves every file of the item store and of `.borax/artifacts/`
byte-identical. It does not depend on the library's identity. Each
restoration is reported as a `content-index-write` event, and one that
fails costs nothing else: the reconcile's repairs stand, and its exit
status does not depend on it. A run that is not a reconcile restores
nothing: `borax status` and `borax validate` read only the library, and
an applying run writes an operator's evidence only for a decision it
made itself.

#### Scenario: An interrupted operator admission is completed
- **WHEN** a file's bytes have extracted evidence `doi:10.1000/x` in the
  content index, an interactive run in a library renames it from the
  operator's `doi:10.1000/y`, writes its artifact record, and stops
  before remembering the answer, and `borax reconcile` then runs
- **THEN** the content index's operator evidence for those bytes is
  `doi:10.1000/y` with origin `supplied`, its extracted evidence is
  still `doi:10.1000/x`, and an outside run over a copy of the file
  looks up `doi:10.1000/y`, as it would had the run not stopped

#### Scenario: A restored index writes nothing the second time
- **WHEN** `borax reconcile` runs twice over a library nothing has
  touched
- **THEN** the second run writes no content-index file and reports no
  `content-index-write` event

#### Scenario: The index deleted
- **WHEN** the content index is deleted and `borax reconcile` runs over
  a library one of whose records holds bytes an operator identified
- **THEN** the operator's identifier for those bytes is in the content
  index again, and nothing is written for bytes whose entries name no
  operator's origin

#### Scenario: The library's decision wins
- **WHEN** the content index's operator evidence for a recorded file's
  bytes names `doi:10.1000/z`, supplied for a copy outside the library,
  while the record's newest operator entry for them names
  `doi:10.1000/y`, and `borax reconcile` runs
- **THEN** the operator evidence names `doi:10.1000/y` afterwards

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
identification evidence, one fact per file" defines the first three.
That is
the identifier an applying run's record was reached by, with the origin
of how it was reached (an identifier the content index held keeps the
origin it was written under); the identifier reconciliation accepted
changed bytes on, `extracted`; and the identifier an adoption's record
was found by, `adopted`. An entry for bytes no identifier established
carries none: an acceptance by `borax reconcile --accept`, a
restatement, and an admission whose record the library's own item
supplied. The
identifier and the origin are kept together or not at all. An entry
holding one without the other, an identifier that does not parse, or
an origin outside those four makes its record a file that does not
parse. An entry recorded before entries carried evidence carries none,
and SHALL remain valid and readable exactly as it was written. This is
where an operator's decision about a file lives inside a library: the
content index keeps a copy of it, which "A library's operator decisions
are restored to the content index" puts back from here.

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
  The one exception is bytes that are already the newest entry's,
  already answer with that item once the admission is made, and whose
  newest entry already carries the identification evidence the
  decision carries, where it carries any. A decision carrying an
  operator's evidence that the newest entry does not carry is recorded,
  so that the operator's identifier reaches the library even when the
  link alone would have said the rest.
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

#### Scenario: Re-identifying the newest bytes records the identifier
- **WHEN** a record's newest bytes, accepted under item X with an
  extracted DOI, are re-identified in an applying run from a DOI the
  operator supplied, which item Y carries
- **THEN** the link moves to Y and the history gains one entry for
  those bytes naming Y, the supplied DOI and `supplied`

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

### Requirement: An applying run records what it admits
An applying `rename` run SHALL write an artifact record for each file in
the library whose fate it settled with a record in hand — a file it
moved, and a file it found already carrying the name its record implies
— minting an artifact identity for a file that has no record yet and
updating the record of a file that has one, except where that record
has not accepted the file's bytes, which the paragraphs on changed
bytes below govern. A preview SHALL write no
artifact record and no item, and a run with duplicate detection off
(`--no-record` or config) SHALL write neither either.

Updating a record SHALL write the file's content hash into that
record's hash history when the history does not already hold it,
together with the file's library-relative path and its size and
modification time as they are after the run's work on it. So a record
an applying run touched describes the file it names: its history holds
that file's hash, and the fields reconciliation's fast path compares
are that file's too. A hash the history already holds, such as an
earlier version restored, SHALL NOT be written again by an ordinary
run, only when the operator decides what those bytes are, so that they
keep answering with the item they were accepted under rather than
becoming the newest and answering with the record's link.

This is stated rather than left to follow from what a record holds,
because the evidence that identifies a record is not evidence about
content. A record is identified by its recorded path holding the file,
and it is confirmed by the fast path *or* by a recorded hash — and the
fast path compares size and modification time, which a same-length
in-place edit leaves alone. An update that wrote only the new path
would therefore leave a history that does not hold the file's hash, and
nothing afterwards would say so: the fast path would settle that record
on every later reconcile, and the file's actual bytes would appear in no
record at all.

A file at the path its record names, whose bytes that record's history
does not hold, SHALL be written into that record only once accepted,
as "Changed bytes become an artifact's history only once accepted"
accepts it: by an identifier extracted from the file, or supplied for
it by the operator in this run, that the record's linked item carries.
An accepted file's hash is written with the item the record links once
the admission is made, and the run SHALL report the acceptance, naming
the artifact, the hash and what it was accepted on.

A file whose changed bytes are not accepted SHALL NOT be written into
that record, whether or not the operator re-identified it. Where the
run moved the file, it SHALL be recorded as a new artifact: a minted
identity at the path it moved to, linked to the item a mint selects for
the record the file resolved to. The record that did not accept it
SHALL be left exactly as it was, still naming the path the file left,
and the run SHALL report the separation, naming both artifacts and the
new artifact's item. Where the file stays at the path its record names,
already named or kept, the run SHALL write nothing for it and SHALL
report the change as pending, naming the record and the hash, whatever
an interactive run's hold would otherwise withhold. A preview foresees
the same outcome and writes nothing, treating the path a separated
file left as holding nothing, as the `ledger` capability requires of a
preview's account.

A pending change is a state of a path. Once a move takes the bytes
away, the record names a path holding nothing, which is the missing
artifact it has become, with its whole history kept and repairable by
hash if those bytes appear again. The bytes are then a file no record
names, and recording such a file is what an applying run does. No
unaccepted bytes enter any history, and no record is left linked to a
work its file is not.

An artifact that already has a record SHALL keep the item link that
record carries, unless the operator has re-identified the file in that
run. An ordinary re-run SHALL NOT change a link: selection happens when
a record is *minted*, so re-running over a recorded file cannot move its
artifact to a different item whether or not its record resolves to an
identifier, which is what keeps a file carrying no identifier from
accumulating a fresh item behind it on every run.

An identifier the operator supplied for the file, or a record the
operator accepted over a conflict, SHALL re-link that file's artifact
record to the item for the record so identified — the item the library
already holds for one of its identifiers, or one minted from it. This
is the deliberate re-identification the rule above excepts, and the
operator's answer is what distinguishes it from a re-run: `resolution`
already keeps an operator's answer distinguishable from an extraction's
find, and a run that corrects a file's name from a supplied identifier
has corrected what the file *is*. Leaving the link alone would file the
corrected name against the wrong work, so the name and the item move
together or neither does. The run SHALL report the re-link, naming both
items; an item nothing links to afterwards is an ordinary library state
that `borax validate` counts.

A re-identification SHALL write the file's hash into the history under
the item it selected, with the operator's identifier and its origin,
`supplied` or `overridden`, unless those bytes are already the newest
entry's, answer with that item once the link has moved, and that newest
entry already carries the same identifier and origin. So re-identifying
the newest bytes appends an entry even where the link alone would have
moved, because without it the operator's identifier would reach the
library nowhere: the entry is where the library keeps it. "An artifact record
is authoritative state keyed by an artifact identity" requires this of
every decision about an item. A restored version re-identified as the
very item the link already names is therefore still written, because
until then it answered with the item it was accepted under. The
correction then holds for those bytes once later bytes are accepted,
and bytes newer than them keep the item they were accepted under. A
re-identification moves
the link of a record that holds or
accepts the file's bytes. A file whose changed bytes its record does
not accept is separated or left pending, as the paragraphs on changed
bytes state.

When a record is minted, the run SHALL link it to the item the library
already holds carrying one of the resolved record's identifiers, or to
one it mints from that record when the library holds none. A record with
no identifier at all SHALL still get an item, since an item's identity
is its minted UUID and identifiers are optional — so two *different*
files carrying no identifier become two items, which states what borax
knows rather than guessing a match. The minted record's one history
entry SHALL name the item it was linked to.

An item the library holds with no artifact recorded against it, or none
whose recorded path still holds a file, SHALL take the incoming file as
its first artifact rather than treating it as a duplicate of itself. An
item without an artifact is the case the model exists for — a work cited
before its PDF was ever had — so the file that finally supplies it is
admitted, its minted record links to that item, and no second item is
minted for the work. The `ledger` capability states the matching rule
from the other side: a work duplicate requires an artifact that is
actually there to be a duplicate of.

An already-named file SHALL be recorded for the same reason a moved one
is: the run has the record, agrees with the name, and a library already
in good order would otherwise never be recorded at all.

The item SHALL be written before the artifact record that links to it.
No cross-file atomicity is available where the writers are peers and no
lock can be a precondition, so the order is chosen for what an
interruption leaves: an item nothing links to, which is an ordinary
library state and not a finding. The reverse order would leave an
artifact record naming an item that does not exist, which is a finding
and a dangling link.

`--no-record` SHALL suppress every write this requirement makes and
SHALL NOT suppress the check a run makes against the record of a file it
is about to move. Before moving an artifact, an applying run SHALL
identify that artifact's record by the test reconciliation settles a
record with — the recorded path holding the file, and the recorded size
and modification time or one of the recorded hashes confirming it — and
under `--no-record` it SHALL make that identification and SHALL NOT
write its result.

Where no rewrite will follow, the evidence SHALL be evidence that
survives the move. A move under `--no-record` SHALL proceed only when
the record naming the file's current path holds that file's content
hash in its history, or when no record names that path at all. The
fast path SHALL NOT be sufficient on its own here: it compares size and
modification time, which a same-length in-place edit preserves, so it
can confirm a record whose every recorded hash is stale — and a move
made on that confirmation takes the file away from the one path the
record names while leaving the record unable to name the file's bytes.

The hash this decision needs SHALL NOT cost a read of its own. The
`rename` capability already requires every applied rename to be recorded
in the run log with the file's content hash, and requires that event to
be written and flushed *before* the move it records; so the hash is
computed and in hand strictly before the moment this decision is made.

A move SHALL be refused under `--no-record` otherwise — when a record
names the file's current path and its history does not hold that file's
hash — reported with a reason naming the record, leaving the file where
it is. Such a record cannot survive the move: its path would hold
nothing, and reconciliation's walk matches by hash while its
changed-in-place step needs a file at the recorded path, so neither route
could find the artifact again. That holds as much for a change the
record has not accepted as for one it would: the move would take away
the one place the change can be accepted at.

The reason SHALL name `borax reconcile --rehash` as the remedy, and
`borax reconcile --accept` for a change that leaves pending, and SHALL
NOT tell the operator to re-run without the flag. Reconciling with
`--rehash` reaches this file by its recorded path, finds bytes no
record's history holds, and appends them when they carry an identifier
the record's item carries, or reports them pending for `--accept` when
they do not; once they are appended, the move proceeds under either
setting, because the record's history holds the file's hash.

Re-running without the flag is *not* a remedy that always works. A
recording run identifies the record by path alone and refuses nothing.
It writes the record afterwards, which keeps the record with its
artifact only when the record accepts the file's bytes, by an
identifier its item carries. Otherwise the recording run moves the file
and records it as a new artifact, separated from the record, which is
left naming a path that holds nothing. That is the right outcome for a
replacement, and the wrong one for an edit nobody has accepted yet. The
flag's refusal exists because a run that writes nothing cannot separate
the file either: the move would leave the record unable to name its
artifact, and leave the moved file recorded nowhere. A diagnostic may
not name a retry that cannot succeed, so this one names the operation
that repairs the record rather than the invocation that sometimes
skirts it.

A move whose artifact has no record strands nothing and SHALL proceed.
A record elsewhere in the store whose history already holds this file's
hash SHALL NOT refuse anything: the move takes away a path and not
bytes, so that record's hash evidence is exactly what it was before,
and the file still answers to it.

What the move can disturb is which record reconciliation *matches* that
evidence to, and this requirement does not claim otherwise. A record
whose recorded size and modification time happen to match the moved
file can claim the destination path through the fast path once the file
arrives there — step 1 settles it and claims the artifact, and a
claimed artifact leaves the pool step 2 searches by content. A third
record that would have been repaired to that file by hash then finds
nothing to match, and stays unrepaired while the file is associated
with the record that claimed it. The move made a coincidence
reachable that was not reachable before.

That limitation is in reconciliation's matching rather than in this
rule, and this change does not close it: the fast path settles a record
on metadata alone, which is the cost that buys not rehashing a library
on every pass. It is bounded and it is visible — the hash evidence is
never lost, so a later pass over a library where the coincidence has
gone can still match by content, and meanwhile the unrepaired record is
reported as an artifact borax has a record of and cannot find rather
than silently dropped.

The asymmetry deciding this is the one this change uses throughout. A
stale path is repaired by the next pass and costs nothing meanwhile; a
record that matches no file and names no file is lost, and nothing
afterwards can tell which artifact it belonged to. So the gate declines
to write, declines to check for duplicates, and still refuses the one
move that would destroy something.

A write to the store that fails SHALL cost the file its record and not
its rename, and SHALL be reported. The file has moved or is correctly
named either way. borax SHALL retry nothing within a run: the next
applying run over that file records it again, and the cost of an
interrupted admission of a file carrying no identifier is an item
nothing links to, which `borax validate` counts and no run mistakes for
damage.

#### Scenario: An applied rename records the artifact
- **WHEN** an applying run renames a resolved file inside a library
- **THEN** an artifact record names the file's new library-relative
  path, carries its hash with the run that recorded it, and names an
  item holding its record

#### Scenario: A recorded file keeps its item
- **WHEN** an applying run is made again over a recorded file whose
  record resolves to no identifier
- **THEN** its artifact record still names the item it named before, no
  second item is minted, and the item count is unchanged

#### Scenario: The PDF for a work already cited
- **WHEN** an applying run admits a file whose resolved DOI is carried
  by an item the library holds and which has no artifact recorded
  against it
- **THEN** the file is not reported a duplicate, an artifact record is
  minted for it naming that same item, and the item count is unchanged

#### Scenario: An item whose only artifact is gone
- **WHEN** the same happens for an item whose one artifact record names
  a path that no longer holds a file
- **THEN** the incoming file is admitted and recorded against that item,
  and the record of the absent artifact is left exactly as it is

#### Scenario: An operator corrects what a recorded file is
- **WHEN** an interactive run with `--no-skip-named` reaches a recorded
  file named from the wrong record, the operator supplies the right
  identifier, and the file is renamed
- **THEN** its artifact record keeps its artifact identity and is
  re-linked to the item for the supplied record, the file's bytes
  answer with that item from then on, the run reports both
  items, and the item it left is counted by `borax validate` as an item
  nothing links to; the history gains one entry for the file's bytes,
  naming the new item, the supplied identifier and `supplied`

#### Scenario: The same correction made twice
- **WHEN** the same identifier is supplied for the same file in a
  second interactive run
- **THEN** no entry is appended the second time

#### Scenario: A second artifact of one item
- **WHEN** an operator accepts, in an interactive run, a second PDF of a
  work whose item the library already holds under the same DOI, and
  answers rename to the move question that follows
- **THEN** a second artifact record is written naming that same item,
  and no second item is minted

#### Scenario: A batch run admits no second artifact unasked
- **WHEN** `borax rename --apply` runs over a directory holding a second
  PDF of a work the library already holds
- **THEN** that file is reported a work duplicate and left where it is,
  no artifact record is written for it, and the run does not move it

#### Scenario: A preview records nothing
- **WHEN** the same run is made without `--apply`
- **THEN** no artifact record and no item is written

#### Scenario: Duplicate detection off records nothing
- **WHEN** `borax rename --apply --no-record` runs over a library whose
  artifacts either have no record or have one the pre-move check settles
- **THEN** the files are renamed and reported, no duplicate check is
  made, and no artifact record and no item is written

#### Scenario: A stale path left by a run that kept no record
- **WHEN** `borax rename --apply --no-record` moves an artifact whose
  record the pre-move check settled, and `borax reconcile` runs
  afterwards
- **THEN** the record's last-known path is repaired to the new location
  by the bounded walk, matching the hash it already recorded, and the
  artifact's identity and item link are unchanged throughout

#### Scenario: A move that would strand a record is refused
- **WHEN** `borax rename --apply --no-record` reaches an artifact whose
  record names its current path but whose bytes changed out of band, so
  that no recorded hash is the file's
- **THEN** the file is not moved and is reported with a reason naming
  the record, `borax reconcile --rehash` and `borax reconcile --accept`,
  which between them record the file's current bytes and let the move
  proceed afterwards

#### Scenario: An edit the fast path cannot see does not license a move
- **WHEN** `borax rename --apply --no-record` reaches an artifact edited
  in place to the same length with its modification time preserved, so
  that the record's recorded size and modification time still match the
  file while no recorded hash is its
- **THEN** the move is refused, because the confirmation the fast path
  gives would not survive it: the record would name a path holding
  nothing and hold no hash matching the file

#### Scenario: A recorded move leaves the record describing the file
- **WHEN** an applying run without the flag moves that same artifact,
  whose edited bytes still carry the DOI its item carries
- **THEN** its record afterwards names the new path, carries the file's
  current hash as the newest in its history with the earlier hashes
  still there and with its item named, and carries the file's size and
  modification time — so the next reconcile settles it on the fast path

#### Scenario: A replacement moved by a rename is a new artifact
- **WHEN** an applying run renames a file at a recorded path whose bytes
  that record's history does not hold, and which carries an identifier
  the record's item does not carry
- **THEN** a new artifact record names the path it moved to, linked to
  the item for the record the file resolved to; the old record is
  byte-identical, still naming the path the file left; and the run
  reports the separation, naming both artifacts and the new item

#### Scenario: A replacement left where it is stays pending
- **WHEN** the same file already carries the name its record implies
- **THEN** no artifact record and no item is written, and the run
  reports the change as pending, naming the record and the hash

#### Scenario: A pending file passed over is still reported
- **WHEN** an interactive run that passes over already-named files
  reaches that file
- **THEN** the pending change is shown on the terminal, though the
  file's other lines would have been withheld

#### Scenario: The operator names the linked work
- **WHEN** an interactive run reaches a scan at its recorded path whose
  bytes the history does not hold and from which nothing can be
  extracted, and the operator supplies the DOI the record's item
  carries and renames the file
- **THEN** the file's hash is appended naming that item, and the run
  reports the acceptance on the supplied DOI

#### Scenario: The operator names another work
- **WHEN** the operator instead supplies a DOI the record's item does
  not carry, and renames the file
- **THEN** the file is recorded as a new artifact linked to the item for
  the supplied record, and the old record is left exactly as it was

#### Scenario: Restored bytes keep the item they were accepted under
- **WHEN** an applying run renames a file whose bytes are an earlier
  entry of its record's history, an entry naming another item than the
  one the record now links
- **THEN** no hash is appended, the record's path, size and modification
  time describe the moved file, its link is unchanged, and the file
  still resolves from the item that entry names

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
too. So deleting the content index loses no library content either,
and costs answers outside the library until `borax reconcile` writes
those identifiers again and runs over the files extract theirs again.
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
  content index again

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
as such a file is. Whether the content index holds a record's
evidence is not validated: the content index is not the library's.

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
that record. The record is found without the network, through the
identifier the content index answers with for those bytes, the
operator's before the extracted one, from the response cache: the
answer a service gave for that identifier, read for each service in
the priority order resolution asks them in for that kind of
identifier, whatever services the run's configuration names, the first
one held being taken.

The adopted record's history entry SHALL name the identifier it was
found by, with the origin `adopted`. The run SHALL report where the
record came from: the service whose response cache held it. Adoption
writes nothing to the content index: the identifier it used is already
there, and its own origin belongs to the library.

Where the index holds nothing for the bytes, the artifact SHALL remain
an orphan, and the run SHALL report it, naming the artifact, as an
orphan the content index holds no record for. Where the index answers
with an identifier and the response cache holds no record for it, the
artifact SHALL remain an orphan too, reported as one the content index
could not answer for, with that identifier. `borax
resolve` over such a file fetches the record its identifier names, and
an adoption afterwards records it.

Every orphan the run reaches SHALL be reported by exactly one
`library-adoption` event, whose kind says what became of it: recorded,
held by an existing record, unreadable, unwritten, `unindexed` when
the content index holds nothing for its bytes, or `unanswered` when it
holds an identifier whose record the response cache does not hold. No orphan is left an
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
records nothing until the records are fetched again. The content index
is not a cache, and survives it, so the identifiers adoption needs are
kept. What `borax adopt` offers is not a
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
  library whose orphans' entries hold identifiers
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
