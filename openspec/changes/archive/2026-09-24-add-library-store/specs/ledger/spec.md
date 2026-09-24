## ADDED Requirements

### Requirement: The record of admissions is optional and degrades loudly, never blocking
The pipeline SHALL treat the library's record of what it has admitted as
optional: when it is disabled (`--no-record` or config) or there is no
library root, the run proceeds with duplicate detection off and says
nothing about it, having been told not to check or having nowhere to
check against. An empty artifact store is not a degradation — it is a
library that has admitted nothing borax has recorded — and SHALL produce
no warning.

`--no-record` SHALL suppress every write to the store and both
duplicate checks: the run writes no artifact record and no item, makes
no content check and no work check, and nothing persistent survives it
but the moves it made and its run log. One setting governs both,
because a run that keeps no account of what it admitted has nothing to
check a later file against either.

What `--no-record` SHALL NOT suppress is the check an applying run makes
against the record of the very file it is about to move. That check is
not part of the account and not a policy about admissions: it is what
keeps a move from stranding a record beyond repair, and the `library`
capability states it and the one refusal it produces. The gate governs
bookkeeping, not safety — an operator turning off the account is asking
borax to keep no record of this run, not asking it to damage the records
it already has.

An artifact record that cannot be read or does not parse SHALL cost its
own artifact and nothing else: it contributes to neither check, the run
warns once naming the number of unreadable records, and the rest of the
store answers normally. One unreadable file SHALL NOT turn duplicate
detection off across the library, which is what a single-file account
had to do.

#### Scenario: An unreadable artifact record
- **WHEN** one file under `.borax/artifacts/` does not parse
- **THEN** the run warns naming it, duplicate detection still runs
  against every other record, and `borax validate` reports it as a
  finding

#### Scenario: No library root
- **WHEN** files are processed in a directory tree with no
  `.borax.toml` above it
- **THEN** no artifact record is read or written and no duplicate
  warning is possible for that run

#### Scenario: The account is turned off for a run
- **WHEN** `borax rename --apply --no-record` runs over a library
- **THEN** no duplicate check is made and no artifact record and no item
  is written, and the files are renamed and reported as they otherwise
  would be

## MODIFIED Requirements

<!-- drops: the ledger entry as the unit a lookup answers with. Both
     checks, both reasons and the divert survive; what answers them is
     the artifact store, where a hash history rather than one hash per
     entry is what a content check matches against, and an item rather
     than an entry's restated identifiers is what a work check matches
     against. -->

### Requirement: Duplicate detection operates at two levels with distinct reasons
The pipeline SHALL check each incoming file against the library's
artifact store twice and report the two outcomes distinctly: a content
duplicate (the file's hash appears in the hash history of some artifact
record other than the incoming file's own) is reported as "same bytes
already archived" naming the recorded path; a work duplicate (no record
holds that hash, but a resolved identifier is carried by an item the
library holds *and* that item has an artifact whose recorded path still
holds a file) is reported as "same work already archived (different
file)" naming that artifact's path. The content check SHALL run after
hashing and before any resolution, so byte-identical re-downloads cost
no network access.

An item with no artifact recorded against it, and one whose every
recorded artifact names a path holding no file, SHALL NOT make an
incoming file a work duplicate. A duplicate report names the file the
incoming one duplicates, so a work with no file present is a work there
is nothing to duplicate: the incoming file is that item's first
artifact and is admitted as one, which is the case an item existing
without an artifact is for. The `library` capability states the
admission side.

A work duplicate SHALL divert to the skip queue in a batch run, and
SHALL be put to the operator in an interactive one, where accepting it
files the file as another artifact of that item. borax cannot tell a
second manifestation of a work — a scan beside the published PDF,
supplementary information beside the paper — from an unwanted second
copy of it, because the two differ in the operator's intention and not
in the bytes. So the run that has an operator asks, and the run that
has none takes the conservative branch: the file is left exactly where
it is, named for what it matched. The `rename` capability specifies the
question.

Matching a hash anywhere in a record's history, and not only its current
hash, SHALL be what the content check means. A file annotated since it
was admitted is recognised as the artifact it is rather than as a new
one, and a byte-identical copy of what an artifact used to be is
recognised as a copy of it.

An artifact record whose recorded path is the incoming file's own path
SHALL NOT make that file a duplicate at either level, and an item that
the incoming file's own artifact record already links to SHALL NOT make
it a work duplicate. Such a record records the file itself, not another
copy of it, and such an item is the file's own item, not one it is
being added to. The second rule is load-bearing rather than tidy:
without it every artifact of a multi-artifact item would meet its
siblings as a work duplicate on every run, and a batch run would skip
each of them for belonging to the item it belongs to.

The lookup SHALL pass over that record and go on, rather than the
result being discarded once found: a file whose own record is the first
answer would otherwise hide a second copy recorded elsewhere in the
library.

Paths SHALL be compared as the library builds them — the record's
library-relative path under the library root against the incoming path
made absolute — normalised lexically and matched the way the platform
matches file names. Symlinks SHALL NOT be resolved.

The library the checks answer from SHALL be the library as the run has
left it so far, not as it stood when the run began. A file the run
admits SHALL be, for every later file of the same run, an artifact
record carrying that file's hash and path and linked to its item; and a
recorded artifact the run moves SHALL be found at the path it moved to.
A byte-identical pair reached in one run is therefore one artifact and
one content duplicate, whether or not either was recorded before, and a
record whose artifact this run moved is not a stale path for having
moved. A preview SHALL learn what it would admit in the same way,
without writing it, so that it reports what the same run with `--apply`
would do; a run whose checks are off learns nothing, having nothing to
check against.

#### Scenario: Re-downloaded identical file
- **WHEN** an incoming file's hash appears in another artifact record's
  hash history
- **THEN** it is reported as a content duplicate of that record's path
  and no source is queried for it

#### Scenario: Second PDF of an archived paper
- **WHEN** an incoming file's hash is in no record's history but its
  resolved DOI is carried by an item the library holds
- **THEN** a batch run reports it a work duplicate naming the path of an
  artifact linked to that item, and an interactive run puts the question
  that offers to file it as another artifact of that item

#### Scenario: The first PDF of a work the library only cited
- **WHEN** an incoming file resolves to an identifier carried by an item
  that has no artifact recorded against it
- **THEN** it is not reported a duplicate at either level, no question
  is put about it, and an applying run records it as that item's first
  artifact

#### Scenario: An item whose artifacts are all absent
- **WHEN** an incoming file resolves to an identifier carried by an item
  whose every artifact record names a path that no longer holds a file
- **THEN** it is not reported a work duplicate, since there is no file
  for it to duplicate, and the run reports that the library has paths to
  reconcile

#### Scenario: Re-running over an admitted file
- **WHEN** a file an applied run renamed and recorded is reached again
  at the path its artifact record names
- **THEN** it is not reported as a duplicate, and with its record in the
  content index it is reported already named without any source being
  queried

#### Scenario: An admitted file annotated afterwards
- **WHEN** a file at its recorded path has changed bytes and resolves to
  an identifier its own item carries
- **THEN** it is not reported a work duplicate of itself, no question is
  put about it, and its earlier hash is still in its record's history

#### Scenario: A sibling artifact is not a duplicate of its own item
- **WHEN** a batch run reaches each of the three recorded artifacts of
  one item in turn
- **THEN** none of them is reported a work duplicate, and none is
  skipped for belonging to the item all three already belong to

#### Scenario: A byte-identical pair in one run
- **WHEN** an applying run reaches two byte-identical files, neither of
  them recorded before the run
- **THEN** the first is recorded and the second is reported a content
  duplicate naming the path the first now has, and a preview of the
  same run reports the same

#### Scenario: A copy of an artifact the run has just moved
- **WHEN** an applying run moves a recorded artifact and then reaches a
  byte-identical copy of it
- **THEN** the copy is reported a content duplicate naming the moved
  artifact's new path, and the run does not report that the library has
  paths to reconcile

#### Scenario: Two files of one work in one batch
- **WHEN** a batch run reaches two different files resolving to one DOI
  that no item carried before the run
- **THEN** the first is recorded against a new item and the second is
  reported a work duplicate naming the first's path

#### Scenario: A file's own entry does not hide a copy
- **WHEN** a file at its recorded path is reached again and a
  byte-identical copy of it sits elsewhere in the library under its own
  artifact record
- **THEN** the incoming file is reported a content duplicate of that
  other copy, not passed as new

### Requirement: Duplicates are skipped, never destroyed
Detected duplicates SHALL divert to the skip queue with their duplicate
reason and the existing path; the incoming file is left untouched and
nothing in the library is deleted, overwritten, or replaced.

A work duplicate an operator accepts in an interactive run SHALL leave
that guarantee exactly as it is. It is filed as another artifact of the
item it matched: the incoming file is moved and recorded, and nothing
already in the library is deleted, overwritten or replaced. What the
operator's answer changes is whether this file is admitted, never what
becomes of the file it matched.

#### Scenario: Duplicate in a batch
- **WHEN** a batch contains one duplicate among new files
- **THEN** the duplicate's source file still exists unmodified after the
  run and every other file is processed normally

#### Scenario: An accepted work duplicate destroys nothing
- **WHEN** an operator files an incoming file as another artifact of the
  item it matched
- **THEN** that file is moved and recorded, and the artifact it matched
  keeps its path, its record and its item link

<!-- drops: `borax ledger rebuild` as the remedy, which this change
     withdraws, and the framing of a stale record as an entry to
     distrust. A stale last-known path is now the ordinary state of a
     library between a move and the next reconcile, so the remedy is a
     reconcile and the report is not a warning about damage. -->

### Requirement: Stale entries never block re-admission
A duplicate report SHALL first verify that the recorded path still holds
a file in the library; if it does not, the record's last-known path is
stale — the incoming file is processed normally and the run reports that
the library holds paths to reconcile. Disk is the source of truth; the
artifact store alone never vetoes an admission.

A stale last-known path SHALL NOT be treated as damage. It is what an
out-of-band move leaves behind, it is repairable by `borax reconcile`
without asking anyone anything, and the library is correct after
reconciliation rather than correct by construction.

#### Scenario: Duplicate of a vanished admission
- **WHEN** an incoming file matches an artifact record whose recorded
  path no longer holds a file
- **THEN** the file is processed normally and the run reports that the
  library has paths to reconcile

## REMOVED Requirements

### Requirement: The ledger is an append-only JSONL file at the collection root
**Reason**: The artifact record replaces it. `.borax/ledger.jsonl` was a
per-file account keyed on a content hash, holding one path, one hash and
the identifiers of the record that admitted it; the artifact record is
the same account grown up — keyed on an identity that survives both a
move and a byte edit, holding a hash history whose entries carry the
run, the timestamp and the version that recorded each of them, and
naming an item rather than restating identifiers. Keeping both would
give a library two per-file stores that disagree the first time
something moves a file out of band, since only one of them has a
reconcile. An append-only log was also the wrong shape for state that is
repaired: a record whose path is corrected is one file rewritten, not a
line appended to a log that still holds the wrong answer above it.

**Migration**: None is owed. borax is before `1.0.0` and promises no
compatibility, so a library that predates the artifact store simply
starts with an empty one: `.borax/ledger.jsonl` is neither read nor
written nor deleted, and duplicate detection has nothing to answer from
until the store is populated. `borax rename --apply` over the library
records every file it settles, moved or already named, and `borax adopt`
records offline whatever the content index can still answer for. The
old file is left where it is and may be deleted by hand; it is borax's
own accounting rather than a user's document, and nothing in it is
information a user would lose.

### Requirement: The ledger is rebuildable and rebuilds deterministically
**Reason**: `borax ledger rebuild` derived the account back from the
collection's files and their citation sidecars, which is a dependence
on derived output that the authoritative artifact record exists to end.
With authoritative per-artifact state there is nothing to derive from
output: the records are the account. What a library still needs is the
two halves this requirement ran together — a local way to record what is
on disk, and a pass that brings each record back into agreement with the
tree — and both are specified in the `library` capability rather than
withdrawn. `borax adopt` is the first, offline and moving nothing;
`borax reconcile` is the second, with its bounded walk, its
`(size, mtime)` fast path, its precedence and ambiguity rules, and no
library state written when nothing has changed — its run log is written
as any run's is — which is the determinism this requirement was asking
for in the form that still means something.

**Migration**: Run `borax adopt` where `borax ledger rebuild` was run,
and `borax reconcile` to repair paths afterwards. Neither queries a
service, renames a file or moves one. An artifact the content index
cannot answer for stays an orphan, which is a worklist entry rather
than a silent omission; `borax rename --apply` over the library is what
resolves and records such a file, and it is a different operation
because it may query services and may rename under the templates in
force.

### Requirement: The ledger is optional and degrades loudly, never blocking
**Reason**: Renamed and rewritten as "The record of admissions is
optional and degrades loudly, never blocking", added by this change in
this capability. The rule survives — the account is optional, its
absence never blocks a run, and a run told not to keep one keeps none —
but every noun in it changed: there is no ledger file to be absent or
unparsable, no torn trailing line to ignore, and the setting is now
`--no-record`. It is expressed as a removal and an addition rather than
as a rename because `openspec archive` matches a MODIFIED requirement to
a living one by title, and a renamed title matches nothing.

**Migration**: None. Read the added requirement of the same shape:
`--no-ledger` is spelled `--no-record`, and what was said about an
absent or unparsable ledger file is now said about an unreadable
artifact record, which costs its own artifact alone rather than the
whole library's duplicate detection.
