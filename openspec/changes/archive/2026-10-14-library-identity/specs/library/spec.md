## ADDED Requirements

### Requirement: A library carries an identity that does not depend on where it is
A library's identity SHALL be a UUID kept as the `id` of `.borax/library.toml` under the library root, written as its canonical lowercase hyphenated text, and it SHALL name the library wherever the library's tree stands.

The identity file SHALL hold `id` and nothing else borax relies on: no
creation time, no borax version and no format version. A reader SHALL
ignore a key it does not know, as it does in an item file and an
artifact record. A reader SHALL take a file as one that does not parse
when its `id` is absent, is not a string, or is not the canonical
spelling of a UUID. An identity is minted as a version 7 UUID, and its
version is not checked when it is read, as an item's and an artifact's
are not.

The identity SHALL NOT be kept in `.borax.toml`. That file is the
operator's configuration, edited by hand and merged with the layers
around it, and a library's identity is state borax writes once and
never merges.

A library copied, synchronised or cloned together with its `.borax/`
directory SHALL be the same library at another location: the copy
carries the identity file and answers with the same identity. Which
files a library answers for SHALL NOT depend on its identity. A library
is still found by its marker, as "A library is one directory tree"
states, and it answers for its own files whether it has an identity or
not. An identity is what another location names a library by when it
refers to it, and within the library it decides nothing.

A library with no identity file SHALL be well formed. That is the state
of a library no borax store write has reached since identities were
introduced, and of a library borax has never written. It is not a
validation finding and not a library condition, and nothing reports it
as incomplete.

The identity file is authoritative state, as the item store and the
artifact records are. Deleting it SHALL lose the identity: nothing
reconstructs it, and the next store write mints another. To every
location that referred to the old identity, the library then has a
new one.

#### Scenario: A copy is the same library
- **WHEN** a library whose `.borax/library.toml` holds an identity is
  copied with `rsync` to another machine, and `borax status` runs over
  each copy
- **THEN** both report the same identity, and an applying run over the
  copy leaves its identity file byte-identical

#### Scenario: The identity decides nothing about the library's own files
- **WHEN** a library holding artifact records and items has no identity
  file, and `borax resolve` runs over a file one of its records tracks
- **THEN** the file resolves from its library item as it would in a
  library with an identity, and no identity file is created

#### Scenario: An unknown key is ignored
- **WHEN** `.borax/library.toml` holds `id` and a key borax does not know
- **THEN** the library's identity is that `id`, and `borax validate`
  reports no finding about the file

#### Scenario: A library nobody has written has no identity
- **WHEN** `borax validate` runs over a marked directory holding PDFs
  and no `.borax/` directory
- **THEN** no finding and no condition concerns an identity, no
  identity file is created, and the run exits 0

#### Scenario: A deleted identity is not reconstructed
- **WHEN** `.borax/library.toml` is deleted, and an applying run then
  records a file in that library
- **THEN** the library carries an identity again, and it is not the one
  that was deleted

### Requirement: A library's first store write mints its identity
borax SHALL NOT write an item or an artifact record into a library whose `.borax/library.toml` is absent: immediately before such a write, it SHALL mint the library's identity and create that file.

That covers every store write: an applying run's admission, an
adoption, and reconciliation's writes to the records it repairs,
refreshes or accepts. The identity file is created before the item and
the artifact record of the write it precedes, so an interruption leaves
an identity alone, or an identity beside an item nothing links to,
which are both ordinary library states. A library that already holds
items and artifact records but no identity file, as every library an
earlier release wrote does, SHALL be given one by the next of those
writes.

A run that writes no item and no artifact record SHALL mint nothing.
That covers:

- `borax status`, `borax validate`, `borax resolve` and `borax bib`;
- a preview, and a run with duplicate detection off;
- a reconcile whose confirmations write nothing, or that finds only
  pending changes and missing records. A record confirmed by its hash
  after a touch has its size and modification time rewritten, and that
  is a store write;
- an adoption that records nothing;
- an admission that leaves its record exactly as it was.

So every item and artifact record borax writes is written into a
library that has an identity file at that moment, and nothing else
creates one.

borax SHALL never replace or rewrite an identity file that exists, and
a run that goes on with an identity SHALL go on with the one on disk.
A library minted into by several runs at once SHALL end with at most
one identity.

On a filesystem that can give one file two names, a reader SHALL never
see a partially written identity file. Every run minting there at once
SHALL end with the identity of the run whose file was created first.

On a filesystem that cannot, borax still never replaces an identity
file, but a run can meet the file in the moment another run is writing
it. Such a run finds the file unreadable, as it finds any file a peer
writer has half-written, and proceeds as the paragraph on an unreadable
identity file below states: it writes without an identity, and warns.
The library then holds the other run's identity once that write
completes. If that write fails, the library holds none, and its next
store write mints one.

When the identity file is absent and creating it fails, the store write
it precedes SHALL NOT be made. It SHALL be reported as that write's
failure already is (an admission, an adoption or a repair reported
`unwritten`), with a message naming the identity file. Like any failed
store write, a failed mint costs a renamed file its record and not its
rename.

When the identity file is present but cannot be read or does not parse,
borax SHALL NOT mint over it, because a new identity would silently
rename the library for every location that refers to it. borax SHALL
NOT withhold the store write either: the write is made, and `borax
validate` reports the file. A run that made a store write into such a
library SHALL warn once on stderr, naming the file and `borax validate`.

No event reports a mint, and none reports that warning.

#### Scenario: An applying run gives a library its identity
- **WHEN** `borax rename --apply` records a file in a library with no
  `.borax/library.toml`
- **THEN** the identity file exists afterwards holding a version 7
  UUID, and it was created before the item and the artifact record the
  run wrote

#### Scenario: A library an earlier release wrote gains an identity
- **WHEN** `borax reconcile` repairs a moved artifact in a library whose
  records an earlier release wrote and which has no identity file
- **THEN** the repair is written, the library has an identity file, and
  the records it did not repair are byte-identical

#### Scenario: A later write keeps the identity
- **WHEN** a second applying run records another file in a library
  that has an identity
- **THEN** `.borax/library.toml` is byte-identical afterwards

#### Scenario: Nothing is minted without a store write
- **WHEN** `borax status`, `borax validate`, a preview, `borax rename
  --apply --no-record`, and `borax reconcile` over a library nothing
  has touched each run over a library holding records and no identity
  file
- **THEN** none of them creates `.borax/library.toml`

#### Scenario: Two runs mint one identity
- **WHEN** two runs each make their first store write into one library
  with no identity file at the same moment, on a filesystem that can
  give one file two names
- **THEN** one identity file exists afterwards, neither run replaced
  it, and both went on with the identity it holds

#### Scenario: A run meets the identity file mid-write
- **WHEN** on a filesystem that cannot give one file two names, a run
  makes a store write while another run has created the identity file
  and not finished writing it
- **THEN** the first run writes without minting and warns, and the
  library afterwards holds the other run's identity, or none if the
  other run's write failed

#### Scenario: A failed mint writes no record
- **WHEN** `borax rename --apply` renames a file in a library whose
  `.borax/` directory refuses a new file, while its item store and its
  artifact store accept one
- **THEN** the file is renamed, no item and no artifact record is
  written, and the admission is reported `unwritten`, naming
  `.borax/library.toml`

#### Scenario: An unreadable identity is left alone
- **WHEN** `.borax/library.toml` holds text that does not parse, and an
  applying run records two files in that library
- **THEN** both files are recorded, the identity file is byte-identical,
  stderr carries exactly one warning naming the file and `borax
  validate`, and `borax validate` reports the file as unreadable

## MODIFIED Requirements

### Requirement: A library is one directory tree
A library SHALL be one directory tree, and every file the library
consists of — physical artifacts, item records, artifact records, and
the accounting beside them — SHALL live inside it. Artifacts MAY be
placed freely in its subdirectories. There is no distinction between an
artifact borax manages and one it merely links to: a file is in the
library when it is in the tree.

The library root SHALL be the directory holding the nearest `.borax.toml`
at or above an input, which is the directory that anchors `.borax/`
today, and the `library-root` configuration key SHALL override the
search for an unusual layout. One marker, one root: a library is a
collection that has grown items and artifact records, not a second scope
discovered separately, and no operation SHALL require a library to be
initialized, imported into, or ingested before it can be reported on.

A library's tree SHALL stop at a nested `.borax.toml`. A directory
holding its own marker is a library of its own, and the enclosing
library SHALL exclude that directory and everything beneath it from its
artifacts, its orphans, its admissions and its reconciliation alike —
all four, by the one rule, so that a library never reports work it
cannot do. Nearest-marker ownership is how configuration and the
accounting root are already discovered, and an overlapping walk would
let a parent count files whose records another library keeps and list
orphans no command of the parent's will adopt.

Every path the library records SHALL be relative to the library root and
`/`-separated, whatever separator the platform writes, so that a library
means the same thing on the machine that wrote it and the one that
reads it back — and so that the whole library can be moved, copied,
synchronised or versioned as a unit. The library's identity, kept in
`.borax/library.toml`, is part of that unit: a copy carries it, and is
the same library at another location, as "A library carries an
identity that does not depend on where it is" states.

A path SHALL be inside the library when it lies under the library root
by lexical containment, with symlinks NOT resolved. A symlink inside
the tree therefore SHALL NOT bring its target into the library, and a
symlink SHALL be neither an artifact nor an orphan. This is the path
policy the `ledger` capability already depends on and
`crates/borax/src/paths.rs` already states; whether borax should instead
resolve symlinks and make them the escape hatch for material that
cannot live in the tree is an open question this requirement does not
answer, and answering it amends this paragraph and that one together.

A run with no library root — no marker above its input and no configured
root — SHALL read a library and write nothing to one: no artifact
record, no item and no library identity. Reporting takes the directory
it was given as the root of what it reports, so borax stays useful on a
directory nobody has marked, and the marker buys writing rather than
seeing.

#### Scenario: One marker establishes the library
- **WHEN** a directory holds a `.borax.toml` and artifacts sit in
  subdirectories beneath it
- **THEN** that directory is the library root, `.borax/` beneath it
  holds the library's state, and every path the library records is
  written relative to it with `/` separators

#### Scenario: A nested library is not the parent's business
- **WHEN** a library holds a subdirectory with its own `.borax.toml` and
  PDFs beneath it
- **THEN** the parent's `borax status`, `borax adopt`, `borax reconcile`
  and an applying `rename` over the parent all pass that subtree by,
  and its files are counted and recorded by the nested library alone

#### Scenario: A symlink does not widen the library
- **WHEN** a symlink inside a library points at a PDF outside the
  library tree
- **THEN** the target is not part of the library, and the link itself is
  reported neither as an artifact nor as an orphan

#### Scenario: A library nobody marked
- **WHEN** `borax status` runs over a directory with no `.borax.toml` at
  or above it
- **THEN** the directory is reported on as given, and nothing is written
  to `.borax/` anywhere

#### Scenario: A run with no library mints no identity
- **WHEN** `borax rename --apply` renames a file in a directory with no
  `.borax.toml` at or above it and no configured `library-root`
- **THEN** the file is renamed, and no `.borax/library.toml` is created
  anywhere

### Requirement: borax reports a library it has never seen
`borax status` SHALL report a library without any preceding
initialization, import or ingestion step, naming at minimum the number
of artifacts in the tree, the number of items in the item store, the
number of artifact records, and the number of orphans. A library borax
has never seen SHALL be reported exactly as one it wrote itself:
nothing in the count depends on the files having entered through borax.
The library's identity is not a count: a library borax has written
reports the identity it carries, one borax has never written reports
none, and neither is a deficiency.

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
to the terminal. The report line keeps its wording, apart from the
identity clause stated below, and remains the last line of human
output.

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

`borax status` SHALL also report the library's identity when
`.borax/library.toml` holds one: as the `id` of its `library-status`
event, and as a closing `; library <uuid>` clause on its report line,
after every count and after the nested libraries it excludes. When the
library has no identity file, or one that cannot be read or does not
parse, the event SHALL carry no `id` and the line no such clause, and
both are then exactly what they were before identities existed;
`borax validate` is what reports an unreadable one. Reporting the
identity reads that one file and SHALL write nothing: `status` never
mints an identity. The `id` is an addition and does not change the
event schema version.

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

#### Scenario: A library borax has written reports its identity
- **WHEN** `borax status` runs over a library whose `.borax/library.toml`
  holds an identity
- **THEN** `library-status` carries that identity as `id`, and the
  report line ends with `; library ` followed by it

#### Scenario: Status mints no identity
- **WHEN** `borax status` runs over a library holding artifact records
  and items and no `.borax/library.toml`
- **THEN** `library-status` carries no `id`, the report line has no
  library clause, and no `.borax/library.toml` is created

### Requirement: The library's text is authoritative and is read directly
The item store, the artifact records and the library's identity file SHALL
be the authoritative state of a library, and every answer borax gives
about a library SHALL be read from those files directly. There SHALL be
no index, no database and no cached listing in this change: a count, a
listing and a validation are each a walk of the tree and a read of the
store.

Deleting any derived state SHALL change no answer about a library. The
response cache and the content index are caches, so clearing them costs
round trips and loses no library content; a citation sidecar is derived
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
sent to the terminal. The totals lines keep their wording, apart from
the closing identity clause "Validation defines a well-formed library"
adds to `borax validate`'s when the library has an identity.

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
