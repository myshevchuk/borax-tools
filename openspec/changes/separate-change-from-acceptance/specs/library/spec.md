## ADDED Requirements

### Requirement: Changed bytes become an artifact's history only once accepted
Bytes at an artifact record's last-known path that no record's history holds SHALL become that record's history only once accepted as its artifact, whether `borax reconcile` or an applying `rename` meets them.

Both SHALL observe such bytes and report them every time they meet
them. Observing them SHALL NOT accept them. A record whose item
answered for bytes it never accepted would let one work's item answer
for another work's file, and nothing afterwards could tell.

The bytes SHALL be accepted automatically only when extraction of the
file finds an identifier that the record's linked item carries. The
operator accepts them otherwise: in a rename, by supplying for the file
an identifier the linked item carries; in reconciliation, by naming the
path with `--accept`. Re-identifying the file as a work the linked item
does not carry, and answering a rename question, SHALL NOT accept them:
each says what work the file is or what it is to be called, and neither
says that the file is the record's artifact.

The linked item carries an identifier when exactly one item file of the
library holds the record's linked identity and that item's record holds
an identifier of the same kind equal to it. DOI, arXiv, PMID and ISBN
SHALL each be compared, in the normalised form the `extraction`
capability defines, only with the item's identifier of its own kind. An
arXiv identifier SHALL be compared without its version, because the
item holds the version that was newest when it was looked up and the
file holds its own. An ISBN-10 and an ISBN-13 SHALL NOT be taken for
each other. A record linking no item, or an item no single readable
item file carries, carries nothing.

Bytes not accepted are a pending change for as long as they stay at the
path. A pending change SHALL write nothing to the record: no hash, no
size and no modification time. It SHALL be reported on every reconcile
that reaches it, and by every applying run that keeps a record and
settles the file there, until it is accepted, the way a missing record
is reported until it is found. It is not a finding and not a skip, and
no exit status depends on it. Meanwhile every run that resolves the
file, `--no-record` and previews included, reports its library answer
as unrecognised content, as the `resolution` capability states.

`borax status` and `borax validate` SHALL NOT report pending changes.
The first opens and hashes nothing, the second hashes nothing, and a
pending change is known only once a file has been hashed and read.

Deciding acceptance SHALL open only a file whose bytes changed at a
recorded path. It runs the extraction passes `resolve` runs, under the
run's extraction settings, and queries no service.

#### Scenario: An unrelated replacement
- **WHEN** a buffer guide whose first page prints a DOI the paper's item
  does not carry has replaced a paper's PDF at its recorded path, and
  `borax reconcile` runs
- **THEN** the paper's record gains no hash, the paper's item does not
  answer for the guide's bytes, and the change is reported as pending,
  naming the DOI found and the paper's item

#### Scenario: A trailing comment is an edit
- **WHEN** the paper's PDF instead gains a trailing comment and its DOI
  can still be extracted, and `borax reconcile` runs
- **THEN** the new bytes are accepted on that DOI, which the paper's
  item carries, and appended naming that item

#### Scenario: An annotated paper renamed
- **WHEN** an applying run renames a file at its recorded path whose
  bytes the record's history does not hold, and from which extraction
  reads the DOI its item carries
- **THEN** the hash is appended naming that item, the record follows
  the file, and the run reports the acceptance on that DOI

#### Scenario: A later arXiv version
- **WHEN** an annotated preprint whose stamp reads `arXiv:2401.12345v1`
  sits at its recorded path, its item carries `arXiv:2401.12345v2`, and
  `borax reconcile` runs
- **THEN** the change is accepted on that arXiv identifier

#### Scenario: An ISBN of the other length
- **WHEN** an interactive run reaches a changed scan of a book at its
  recorded path, from which nothing can be extracted, the operator
  supplies the book's ISBN-10 while its item carries only its ISBN-13,
  and the file keeps its name
- **THEN** the change is not accepted, nothing is written, and it is
  reported as pending

#### Scenario: A record that links no item
- **WHEN** the record whose path holds the changed bytes links no item
- **THEN** the change is pending, and is reported naming no item

#### Scenario: Status knows nothing of a pending change
- **WHEN** `borax status` runs over a library holding a pending change
- **THEN** no document is opened, no file is hashed, and nothing names
  the change

### Requirement: An operator accepts a pending change by naming its path
`borax reconcile --accept <path>` SHALL accept the pending change at `path` as that record's artifact, appending its hash after the hashes already recorded, with the record's linked item, and refreshing the record's size and modification time.

A record linking no item gains an entry naming none.

The flag SHALL be repeatable. Each path SHALL be taken as every path
borax is given is taken: made absolute against the working directory
and normalised lexically before it is compared with a record's
library-relative path. A path named twice counts once. A record whose
last-known path is named SHALL be hashed whatever its recorded size and
modification time say, so that a change the fast path cannot see is
accepted without `--rehash`.

Acceptance happens in reconciliation's step 3, after steps 1 and 2 have
claimed what they claim, and only for a change step 3 left pending. A
named path whose change the identifier accepted in the same run is
reported as accepted on that identifier, and a named path whose
record's history already holds the bytes there needs nothing. Neither
is reported further. Every other named path SHALL be reported as a
`skipped` event naming the path as given, with the reason
`not-pending`, and the run SHALL then exit with the partial-success
code, so that a mistyped path is never taken for a success. That covers
a path no record names, one outside the library or in a subtree it
excludes, one whose record was repaired elsewhere or is ambiguous or
missing, and one whose file belongs to another record's history.

The acceptance SHALL be reported as the `changed` repair, with
`accepted` `{"by":"operator"}`, and counted as changed. It is the
operator's decision, made where the change was observed. There is no
prompt, and no form that accepts every pending change at once.

#### Scenario: The operator accepts a pending change
- **WHEN** `borax reconcile` left `scan.pdf` pending, and `borax
  reconcile --accept scan.pdf` runs from the library root
- **THEN** the file's hash is appended naming the record's item, the
  record is reported `changed`, accepted by the operator, and a further
  `borax reconcile` settles it on the fast path

#### Scenario: A change the fast path hides
- **WHEN** a recorded artifact was edited to the same length with its
  modification time preserved, and `borax reconcile --accept` names it
  without `--rehash`
- **THEN** the file is hashed and its change accepted

#### Scenario: Two paths in one run
- **WHEN** `borax reconcile --accept a.pdf --accept b.pdf` runs over a
  library where both changes are pending
- **THEN** both are accepted and both are reported

#### Scenario: A path with nothing pending
- **WHEN** `borax reconcile --accept typo.pdf` names a path no artifact
  record names
- **THEN** a `skipped` event names `typo.pdf` with the reason
  `not-pending`, nothing is written, and the run exits with the
  partial-success code

#### Scenario: Accepting what is already recorded
- **WHEN** `borax reconcile --accept kept.pdf` names a path whose
  record's history holds the bytes there
- **THEN** nothing is written and nothing is reported for it, and the
  run exits 0

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
  new hash is appended with the run that recorded it and the item it
  was accepted under, and its earlier hashes and their provenance are
  still there

#### Scenario: A record written before entries named items
- **WHEN** an artifact record whose every history entry was written by
  an earlier release, naming no item, is read, validated and reconciled
  over a library in which nothing has changed
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

### Requirement: Reconciliation repairs a stale path by a bounded walk
`borax reconcile` SHALL bring each artifact record's last-known path back
into agreement with the library, and SHALL compare the recorded size and
modification time before hashing anything: an artifact that matches that
fast path SHALL NOT be hashed. `--rehash` SHALL skip the fast path and
hash regardless.

A reconcile SHALL resolve records against artifacts in this order, and
SHALL complete each step for every record before beginning the next:

1. **Settled records.** A record whose last-known path holds a file that
   matches the fast path, or whose hash is in that record's history, is
   confirmed, and that artifact is *claimed* by it.
2. **Repair by content.** Each remaining record is matched against the
   artifacts no record claimed, by any hash in its history. A match on
   the record's current hash SHALL be preferred over a match on an
   earlier one. A record with exactly one candidate, where that
   candidate is a candidate for no other record, has its last-known
   path repaired and claims that artifact.
3. **Changed in place.** A record still unresolved whose last-known path
   holds an unclaimed artifact matching no record's history has changed
   since borax last saw it, and it claims that artifact whatever is
   decided about the change. The change is accepted, and its new hash
   appended after the hashes already recorded with the item the record
   links, when "Changed bytes become an artifact's history only once
   accepted" accepts it; otherwise the change is pending, and nothing is
   written for that record.
4. **Unresolved.** Any record still unresolved is left exactly as it is.

The order is what makes two records whose files swapped paths both
repair: each file matches the other record's history in step 2, so
neither is mistaken in step 3 for the other's artifact changed in
place, and neither file is opened to be read.

A pending change SHALL be reported on every reconcile that reaches it,
as a `library-repair` event of kind `pending` carrying the hash, the
record's linked item and what extraction made of the file, and SHALL
be counted as pending on the totals. It is not a finding and not a
skip, and the run's exit status does not depend on it. A path named by
`--accept` SHALL be hashed whatever the fast path says, and a pending
change at it SHALL be accepted as "An operator accepts a pending change
by naming its path" states. An accepted change SHALL be reported as the
`changed` repair, carrying how it was accepted.

A reconcile SHALL refresh a record's recorded size and modification
time whenever it confirms or repairs that record, or accepts its
change, so that those fields
describe the file as borax last saw it rather than as it once was. This
includes a record confirmed by a hash match after the fast path missed:
the bytes are the same and the record is right about them, and the
fields the fast path compares are the ones that were wrong. A record
whose recorded fields already describe the file is confirmed by the
fast path itself and needs no refresh.

A reconcile SHALL NOT refresh those fields for a record whose change is
pending. They are what makes the next reconcile hash the file again
and report the change again: a refreshed record would be settled by the
fast path, and the change would never be reported again.

Nothing else in the library repairs those fields, which is why this is
stated rather than left to follow. `touch` on an artifact changes its
modification time and not its bytes; without the refresh, the record
would be confirmed by its hash and keep a modification time that can
never again match the file, and every later operation that compares the
two — including a view derived from artifacts, which the hierarchical
views build on this store — would treat that artifact as unconfirmed
for as long as the library lasts.

A record SHALL be preserved, not reassigned, wherever the match is
ambiguous. A record with more than one candidate, and a record whose
only candidate is also a candidate for another record, SHALL keep its
last-known path, its hash history and its item link exactly as they
were, and SHALL be reported as ambiguous naming the candidates.
Reconciliation SHALL NOT give one artifact's identity or item link to
another artifact: a wrong link is worse than a stale path, because a
stale path is repairable by the next pass and a wrong link is not
detectable at all.

A record whose last-known path holds nothing and that matched no
artifact SHALL keep its last-known path and be reported as an artifact
borax has a record of and cannot find.

`borax reconcile` SHALL create no artifact record and SHALL delete none.
An orphan is not adopted by a reconcile — a library whose orphans were
adopted automatically would have no worklist, and `borax adopt` is the
command that records what is on disk.

A reconcile over a library nothing has touched SHALL write no library
state, leaving every file of the item store and of `.borax/artifacts/`
byte-identical, so that a routine pass produces no diff. Its run log is
written as any run's is.

"Nothing has touched" means every record's path, hashes, size and
modification time already agree with the file it names: a file whose
modification time changed has been touched, its record does not yet
describe it, and refreshing that record is a change the reconcile was
run to make. A record whose change is pending is the exception: it is
written nothing, so a reconcile that finds nothing but pending changes
writes no library state either.

Reconciliation is a routine library operation and SHALL NOT be an error
path. Between an out-of-band move and the next reconcile, a recorded
path may be wrong; that interval is the accepted consequence of peer
writers, and the library is correct after reconciliation rather than
correct by construction.

#### Scenario: A file manager move is repaired
- **WHEN** an artifact is moved to another subdirectory outside borax and
  `borax reconcile` runs
- **THEN** its record's last-known path names the new location, its
  artifact UUID and item link are unchanged, and the repair is reported

#### Scenario: Two artifacts swap paths
- **WHEN** two recorded artifacts are moved onto each other's paths
  outside borax and `borax reconcile` runs
- **THEN** each record's last-known path is repaired to the file whose
  hash it records, neither hash history gains an entry, and neither item
  link changes

#### Scenario: One record, two candidates
- **WHEN** a recorded artifact's file is copied to a second path, the
  original is deleted, and the copy is duplicated again so that two
  unclaimed files match one record's history
- **THEN** that record keeps its last-known path, its history and its
  item link, and the run reports it as ambiguous naming both candidates

#### Scenario: One file, two records
- **WHEN** two records whose paths hold nothing both match one unclaimed
  artifact by hash
- **THEN** neither record is repaired, neither claims the file, and both
  are reported as ambiguous

#### Scenario: A file touched but not edited
- **WHEN** an artifact's modification time is changed without its bytes
  changing — `touch paper.pdf` — and `borax reconcile` runs
- **THEN** the file is hashed because the fast path missed, its record
  is confirmed by that hash, and its recorded size and modification time
  are refreshed to the file's current ones — so a second reconcile
  settles the record on the fast path and hashes nothing

#### Scenario: A repaired record describes the file it found
- **WHEN** a record's artifact was moved out of band and the move
  changed its modification time
- **THEN** the repair writes the new path and the file's current size
  and modification time together, so the next reconcile settles that
  record on the fast path

#### Scenario: An unchanged library is not rehashed
- **WHEN** `borax reconcile` runs twice over a library nothing has
  touched
- **THEN** the second run hashes no artifact, writes no library state,
  and leaves every file of the item store and of `.borax/artifacts/`
  byte-identical

#### Scenario: An artifact annotated in place
- **WHEN** an artifact at its recorded path has different bytes and a
  different size or modification time, its text still carrying the DOI
  its item carries, and `borax reconcile` runs
- **THEN** its new hash is appended after the hashes already recorded,
  with the run that recorded it and its item, and it is reported as
  changed since borax last saw it, accepted on that DOI

#### Scenario: A change the fast path cannot see
- **WHEN** an artifact's bytes changed while its size and modification
  time did not, the new bytes still carrying the DOI its item carries,
  and `borax reconcile` runs
- **THEN** nothing is reported for it and nothing is written, and
  `borax reconcile --rehash` reports the change and appends the hash

#### Scenario: A replacement is left pending
- **WHEN** another PDF, carrying an identifier the record's item does
  not carry, has replaced an artifact at its recorded path, and `borax
  reconcile` runs
- **THEN** nothing is written to the record, its size and modification
  time included, and it is reported as pending, naming the identifier
  found and the record's item, and counted as pending

#### Scenario: A pending change is reported again
- **WHEN** `borax reconcile` runs a second time over that library
- **THEN** the file is hashed again, the change is reported as pending
  again, and still nothing is written

#### Scenario: A replacement with nothing to read
- **WHEN** the file at a record's path has been replaced by a scan whose
  pages hold no text and whose metadata holds no identifier, and `borax
  reconcile` runs
- **THEN** the change is pending, reported with the result
  `no-text-layer`, and nothing is written

#### Scenario: A swap opens no document
- **WHEN** two recorded artifacts are moved onto each other's paths and
  `borax reconcile` runs
- **THEN** both records repair in step 2, and neither file is opened
  for extraction

#### Scenario: An untouched library opens no document
- **WHEN** `borax reconcile` runs over a library nothing has touched
- **THEN** no file is opened for extraction, and no change is reported

#### Scenario: An orphan stays an orphan
- **WHEN** `borax reconcile` runs over a library holding artifacts with
  no records
- **THEN** no record is created for them and `borax status` still counts
  them as orphans

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
  including one whose verbatim source fields do not parse as JSON.

The item a history entry names SHALL NOT be a finding, whether the
entry names none, as every entry recorded before entries named items
does, or names an item the library does not hold, which is the history
of an item deleted since: deleting an item nothing links to is an
ordinary operation, and the entry still records what those bytes were
accepted as. An entry whose item is not a canonical UUID makes its
record a file that does not parse, and is reported as such a file is.

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
run cut short.

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
the item it selected, unless those bytes are already the newest entry's
and answer with that item once the link has moved. "An artifact record
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
  nothing links to

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
