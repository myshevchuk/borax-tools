## ADDED Requirements

### Requirement: Library conditions are named where they are counted
`borax status` and `borax validate` SHALL report each library condition
they count as a `library-condition` event naming the object it is
about, so that every such count can be traced to the objects behind it.

There are three conditions, each carried as the `kind` of the event's
`condition`:

- `orphan`: an artifact no artifact record names. The event's `path` is
  the artifact's library-relative, `/`-separated path.
- `missing`: an artifact record whose last-known path holds no artifact
  of this library, because no file is there or because the file there
  lies in a subtree the library does not own. The event's `path` is
  that last-known path exactly as the record holds it, its `id` is the
  record's artifact identity, and its `record` is the library-relative,
  `/`-separated path of the record's own file under `.borax/artifacts/`.
  The record file is what tells two records apart when they carry one
  identity, which validation tolerates and reports as a finding, so
  every `missing` event names exactly one record file.
- `unlinked`: an item no artifact record links to. The event's `path`
  is the library-relative, `/`-separated path of its item file, and its
  `id` is the item's identity as that file records it.

`borax status` counts orphans and SHALL name orphans alone; `borax
validate` counts all three and SHALL name all three. A command's count
of a condition SHALL equal the number of that run's `library-condition`
events of the same kind, and naming conditions SHALL NOT widen what the
command counts: the objects named are exactly the objects counted.
`borax adopt` and `borax reconcile` trace their own counts through their
own per-object events and write no `library-condition` event.

Every `library-condition` event of a run SHALL precede that run's
totals event, which remains the last event before `run-finished`. In
`borax validate` they follow the findings: the orphans first, then the
missing records, then the unlinked items. The event is an addition and
does not change the event schema version.

The human rendering SHALL write one line per condition, naming its
path and opening its clause with the word the totals line counts it
under — `orphan`, `missing` or `unlinked` — followed by the record's or
the item's identity where the condition carries one, and by the record
file for a missing record. Every path on that line, the record file's
included, SHALL have its control characters written out rather than
sent to the terminal. The totals lines keep their wording.

A condition SHALL NOT be a finding or a skip. `run-finished` counts
nothing for it, no summary line is written for it, and no command's
exit status depends on the conditions it reports: an orphan is work to
do, a missing artifact is history the library deliberately keeps, and
an unlinked item is an ordinary item for a work with no file. A caller
that has to act on a condition detects it from the event stream — from
the `library-condition` events or from the count on the totals event —
and not from the exit status.

#### Scenario: Validation names each condition it counts
- **WHEN** `borax validate` runs over a library holding `new.pdf`, which
  no artifact record names, an artifact record naming `gone.pdf` where
  no file is, and an item file `items/milner1978.<uuid>.toml` that no
  artifact record links to
- **THEN** one `library-condition` event names `new.pdf` as `orphan`,
  one names `gone.pdf` as `missing` with the record's identity and its
  record file, one names the item file as `unlinked` with the item's
  identity, and all
  three precede the totals, which count 1 orphan, 1 missing and 1
  unlinked

#### Scenario: A condition does not fail a run
- **WHEN** `borax validate` runs over the library of the previous
  scenario, which holds no finding
- **THEN** no `library-finding` event is written, `run-finished` counts
  no finding, and the run exits 0

#### Scenario: A record inside a nested library is missing, not an orphan
- **WHEN** an artifact record names `nested/kept.pdf`, where a file
  stands inside a nested library, and `borax validate` runs
- **THEN** that record is named as `missing`, no orphan is named for the
  file, and no finding is reported

#### Scenario: Two records of one identity are both named missing
- **WHEN** two artifact record files under `.borax/artifacts/` carry one
  artifact identity, both name `gone.pdf`, where no file is, and
  `borax validate` runs
- **THEN** the shared identity is reported as a finding, two
  `library-condition` events of kind `missing` are written, each naming
  `gone.pdf` and that identity and each naming a different record
  file, and the totals count 2 missing

#### Scenario: A path cannot drive the terminal
- **WHEN** `borax status` runs in human mode over an orphan whose file
  name holds an escape character
- **THEN** the orphan's line writes that character as `\x1b`, and the
  `library-condition` event in `--json` output carries the path
  unchanged

## MODIFIED Requirements

### Requirement: borax reports a library it has never seen
`borax status` SHALL report a library without any preceding
initialization, import or ingestion step, naming at minimum the number
of artifacts in the tree, the number of items in the item store, the
number of artifact records, and the number of orphans. A library borax
has never seen SHALL be reported exactly as one it wrote itself:
nothing in the count depends on the files having entered through borax.

`borax status` SHALL open no document to produce those counts. They come
from the tree and from the text store, so the cost of the command is a
directory walk and a read of the store rather than a pass over the
artifacts.

`borax status --identify` SHALL additionally report how many artifacts
an identifier can be extracted from, running the same extraction passes
`resolve` runs and querying no service. It is the same command asked for
more rather than a separate operation, and the extra cost is the pass
over the files that the count requires.

`borax status --identify` SHALL also report, for each artifact it
counts, what extraction made of that artifact: the result the
`extraction` capability defines, which is the identifier and the pass
that found it, or the way extraction failed. The artifacts the count is
taken over are the selection, and reporting on them SHALL NOT widen it.
A file the count leaves out, such as one beneath a nested library, a
symlink, a citation sidecar, or a file in the item store or under
`.borax/`, is neither opened nor reported.

Each artifact's result SHALL be a `library-extraction` event carrying
the artifact's library-relative, `/`-separated path and its result,
written when that artifact's extraction is done rather than once the
pass over the library is over. Every `library-extraction` event of a
run SHALL precede its `library-status` event, which remains the last
event before `run-finished`. The identifiable count SHALL equal the
number of that run's `library-extraction` events reporting an
identifier, so the totals and the per-file results cannot disagree.
The event is an addition and does not change the event schema version.

The human rendering SHALL write one line per artifact, naming its path
and either the identifier and the pass that found it or the failure.
Every value on that line that comes from the file, its name or the PDF
reader SHALL have its control characters written out rather than sent
to the terminal. The report line keeps its wording and remains the
last line of human output.

An extraction result SHALL NOT be a skip or a finding. `run-finished`
counts nothing for it, `status` gains no summary line, and the run's
exit status does not depend on what extraction found.

`borax status` SHALL also name each orphan it counts, with or without
`--identify`, as the `library-condition` event of kind `orphan` that
"Library conditions are named where they are counted" defines. The
orphans are known once the tree is walked and the store is read, so
their events SHALL be written before any document is opened, and
therefore before every `library-extraction` event of the run. The
orphan count SHALL equal the number of those events. Naming an orphan
opens nothing, and an artifact an artifact record names by its path is
never named an orphan.

#### Scenario: Two hundred files borax has never seen
- **WHEN** `borax status` runs over a marked directory holding 200 PDFs,
  no items and no artifact records
- **THEN** it reports 200 artifacts, 0 items, 0 artifact records and 200
  orphans, naming each of the 200 orphans ahead of the report, having
  been given no command before it

#### Scenario: What is identifiable is asked for
- **WHEN** `borax status --identify` runs over the same directory
- **THEN** it additionally reports how many of the 200 artifacts yield
  an identifier, and no service is queried; each of the 200 artifacts
  is reported once with its own result, before the report

#### Scenario: Status opens nothing by default
- **WHEN** `borax status` runs over a library whose artifacts cannot be
  read
- **THEN** the counts are reported and no artifact is opened, so an
  unreadable document costs the command nothing

#### Scenario: Each artifact is reported with what extraction found
- **WHEN** `borax status --identify` runs over a library holding
  `a.pdf`, whose XMP packet carries a DOI, and `sub/b.pdf`, whose
  metadata carries no identifier and whose first page prints an arXiv
  identifier
- **THEN** a `library-extraction` event reports `a.pdf` with the DOI
  and the `embedded-metadata` pass, another reports `sub/b.pdf` with
  the arXiv identifier and the `text-layer` pass, both precede the
  `library-status` event, and the report counts 2 identifiable

#### Scenario: A blank page with an embedded title has no text layer
- **WHEN** `borax status --identify` runs over a library holding a PDF
  whose only page is blank and whose metadata carries a title, with no
  identifier in the title or anywhere else in the metadata
- **THEN** the PDF is reported as `no-text-layer`, it does not count as
  identifiable, and the run exits 0

#### Scenario: Readable text without an identifier is told apart
- **WHEN** `borax status --identify` runs over a library holding a PDF
  whose page holds readable prose and no identifier, and whose metadata
  carries a title, with no identifier in the title or anywhere else in
  the metadata, beside the blank-page PDF of the previous scenario
- **THEN** the prose PDF is reported as `text-without-identifier` and
  the blank-page PDF as `no-text-layer`, so the two results differ, and
  neither counts as identifiable

#### Scenario: Encrypted and unreadable artifacts are told apart
- **WHEN** `borax status --identify` runs over a library holding a PDF
  encrypted under a user password and a PDF truncated so that it
  cannot be parsed
- **THEN** the first is reported as `encrypted` and the second as
  `unreadable` carrying the reader's message, and neither is reported
  as a file without an identifier

#### Scenario: Only the counted artifacts are inspected
- **WHEN** `borax status --identify` runs over a library holding
  `paper.pdf`, its `paper.pdf.bib` sidecar, a symlink to a PDF, and a
  nested library holding PDFs
- **THEN** exactly one `library-extraction` event is written, for
  `paper.pdf`, and no other file is opened

#### Scenario: Failed extraction is not a partial run
- **WHEN** `borax status --identify` runs in human mode over a library
  in which no artifact yields an identifier
- **THEN** each artifact's failure is on a line of its own, the last
  line is the report counting 0 identifiable, `run-finished` counts no
  skip and no finding, and the run exits 0

#### Scenario: A file's text cannot drive the terminal
- **WHEN** `borax status --identify` runs in human mode over an
  artifact that cannot be parsed and whose reader message carries an
  escape character
- **THEN** the human line writes that character as `\x1b`, and the
  `library-extraction` event in `--json` output carries the message
  unchanged

#### Scenario: Each orphan is named before anything is opened
- **WHEN** `borax status --identify` runs over a library holding
  `a.pdf`, `b.pdf` and `c.pdf`, none of which an artifact record names
- **THEN** a `library-condition` event of kind `orphan` names each of
  the three, all three are written before the first document is
  opened, and the report counts 3 orphans

#### Scenario: A recorded artifact is not named as an orphan
- **WHEN** `borax status` runs over a library holding `kept.pdf`, which
  an artifact record names, and `new.pdf`, which none does
- **THEN** exactly one `library-condition` event is written, naming
  `new.pdf` as an orphan, the report counts 1 orphan, and no document
  is opened

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

### Requirement: borax adopt records what the library already holds
`borax adopt` SHALL give recorded state to the artifacts already on disk,
from what borax already knows locally, and SHALL write no library state
but the artifact records under `.borax/artifacts/` and the items it
links them to. It SHALL move, rename and delete nothing at all. It
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
that record. Where the index holds nothing, the artifact SHALL remain
an orphan, and the run SHALL report it, naming the artifact, as an
orphan the content index holds no record for.

Every orphan the run reaches SHALL be reported by exactly one
`library-adoption` event, whose kind says what became of it: recorded,
held by an existing record, unreadable, unwritten, or `unindexed` when
the content index holds nothing for its bytes. No orphan is left an
orphan silently, and the number of orphans the run reports as left
SHALL equal the number of its `library-adoption` events that did not
record the artifact. The human line for an unindexed orphan names its
path, with control characters written out, and says that it is still
an orphan. An orphan is reported `unindexed` only when it was readable
and no artifact record holds its bytes, since only then is the content
index asked about it. Such an orphan is not a skip or a finding, and
the run's exit status does not depend on it.

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

The content index is a cache, and this requirement does not pretend
otherwise: `borax cache --clear` empties it, and adoption after that
records nothing. What `borax adopt` offers is not a guarantee of
recovery but the one path that records a library without querying a
service and without moving a file, which `borax rename --apply` cannot
promise.

#### Scenario: A library borax has seen before
- **WHEN** `borax adopt` runs over a library whose files borax resolved
  on an earlier run, so the content index answers for their hashes
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
- **WHEN** `borax cache --clear` runs and then `borax adopt`
- **THEN** nothing is adopted and the run reports every artifact as an
  orphan, rather than reporting a failure; each orphan that is readable
  and whose bytes no artifact record holds is reported by its own event
  as one the content index holds no record for, and the run exits 0

#### Scenario: Every orphan is accounted for
- **WHEN** `borax adopt` runs over a library holding one orphan the
  content index answers for, one whose bytes an existing artifact
  record holds, and one the content index holds nothing for
- **THEN** each of the three is reported by exactly one
  `library-adoption` event — recorded, held and unindexed in turn — and
  the totals count 1 adopted and 2 orphans left
