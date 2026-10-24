# library Specification

## Purpose
TBD - created by archiving change add-library-store. Update Purpose after archive.
## Requirements
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

### Requirement: An artifact is a document in the tree, and one without a record is a worklist item
An artifact SHALL be a file inside the library tree whose extension
borax recognises as a document — today `.pdf`, ignoring case, as the
input walk already recognises it. A file under `.borax/`, any file under
the item store whatever its extension, a citation sidecar, a symlink,
anything beneath a nested `.borax.toml`, and any other file the library
happens to hold SHALL NOT be an artifact.

An orphan SHALL be an artifact with no artifact record naming it. An
orphan SHALL be reported and counted, and SHALL NOT be a validation
finding: it is a worklist item, and a library describing its own
incompleteness is well formed. A library of nothing but orphans is
therefore valid.

#### Scenario: A library of orphans validates
- **WHEN** `borax validate` runs over a library holding 200 artifacts
  and no artifact records
- **THEN** it reports no finding, reports 200 orphans, and exits 0

#### Scenario: What is not an artifact
- **WHEN** a library holds a `README.md`, an item file, a PDF inside the
  item store, a `paper.pdf.bib` sidecar and `paper.pdf`
- **THEN** only `paper.pdf` is counted as an artifact, and the other
  four are neither artifacts nor orphans

### Requirement: An item is a logical work with a minted identity
An item SHALL be a logical, independently citable bibliographic entity,
recorded as a text file in the library's item store and carrying a
minted UUID as a field inside it. An item SHALL be able to exist with no
artifact at all, so that a library can hold and name a work it has no
file for; and one item MAY have several artifacts, such as a preprint, a
published PDF, supplementary information and a scan.

Independently citable is a statement about the model: the item is the
thing a citation refers to, the artifact is a file that represents it,
and two bibliographic manifestations of one work are two items rather
than one item with a variant. Nothing about that is weakened by the
paragraph below.

This change SHALL NOT add a way to emit an item into a bibliography. An
item carries everything a citation needs, since it holds the canonical
record, and `borax bib` takes files and emits from what they resolve to
— so an item with no artifact has no file to name it by and no route
out. That gap is stated rather than implied: what the item store gives
this change is a place for such a work to exist, be counted, be read
and be edited, and what it does not give is `bib` over the store.

An item's UUID SHALL be what every reference to that item names. A path,
a citation key, a DOI, an arXiv identifier and a node name SHALL NOT
substitute for it: each of them can change while the item stays the
same work, which is what a minted identity is for. A content hash SHALL
NOT identify an item either — re-downloading one work can produce
different bytes, and many works have no strong identifier at all.

An item's record SHALL be the canonical record borax already resolves,
stored as text the operator can read, diff and edit. An item SHALL be
deletable by deleting its file and readable by reading it, with no borax
command in the way.

#### Scenario: An item with no artifact
- **WHEN** an item file names a work the library holds no file for
- **THEN** the item is counted and reported, holds the whole record a
  citation of that work would be made from, and nothing reports it as
  incomplete

#### Scenario: No route from an artifactless item to a bibliography
- **WHEN** `borax bib` is run over a library holding an item with no
  artifact
- **THEN** no entry is emitted for that item, because the command takes
  files, and nothing in this change claims otherwise

#### Scenario: One work, several files
- **WHEN** a preprint PDF and the published PDF of one paper both have
  artifact records naming one item
- **THEN** the library reports one item with two artifacts, and neither
  file is privileged over the other

#### Scenario: An item is deleted with rm
- **WHEN** an item file is deleted outside borax
- **THEN** the next `borax status` reports one fewer item, and the
  artifact records that named it are reported by `borax validate` as
  naming an item that does not exist

### Requirement: An item file's name carries a legible key and its identity
An item file SHALL be stored at `items/<key>.<uuid>.toml` relative to the
library root, where `<key>` is the citation key the `bib-output`
capability renders for the item's record through the `citation-keys`
templates in force **when the file is created**, folded by the public
`slug` fold, and `<uuid>` is the item's full UUID. An item whose key
renders empty at that moment SHALL be named by its UUID alone.

The key SHALL be a creation-time label and SHALL NOT be maintained
against later configuration. borax SHALL NOT rename an item file
because the templates changed, because the record was corrected, or
because the key the templates now render differs from the one in the
name. An item whose bibliographic content never changed SHALL NOT be
renamed at all, and the label being stale is not a validation finding:
it was a legible handle when it was written, and that is all it was ever
for.

The path SHALL be the item's identity at the filesystem interface while
the `id` field inside the file SHALL be what references name. borax
SHALL find an item by reading item files and trusting that field, never
by parsing a file name, so an item file renamed by hand is still the
item it was. A file name whose UUID disagrees with the `id` inside it
SHALL be a validation finding even though the field decides, because the
two disagreeing is a mistake worth reporting; a *key* that disagrees
with what the templates would render is not.

The name carries the key because a bare UUID does not carry enough for a
person to act on, and carries the full UUID because a truncated one
would need a rule for two items competing over one name.

#### Scenario: An item file is named for its work
- **WHEN** an item is written for a record whose citation key is
  `milner1978`
- **THEN** its file is `items/milner1978.<uuid>.toml`, sorting beside
  the operator's other 1978 items and carrying the same UUID inside it
  as in its name

#### Scenario: The citation-key template changes
- **WHEN** `citation-keys.default` is changed and borax runs again over
  a library whose items were named under the old one
- **THEN** no item file is renamed, every item is still found, and
  `borax validate` reports no finding about any of their names

#### Scenario: An item file renamed by hand
- **WHEN** an operator renames an item file to `items/notes-on-this.toml`
- **THEN** borax still finds the item by the `id` inside it, and
  `borax validate` reports the name as disagreeing with that field

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

### Requirement: The edge points from artifact to item, and the two records are separate files
An artifact record SHALL name the item it represents, and an item SHALL
NOT name any artifact or any path. The relation points from the moving
physical thing to the stationary logical one, so that moving a file
cannot make the logical record stale, which is the repair every
reference manager that stores paths in the item has to ask a person for.

The artifact record and the item record SHALL remain separate files, for
three independent reasons, each of which is sufficient on its own:

1. The artifact-to-item edge belongs with the artifact's own state.
   Putting paths in the item would recreate the stale-attachment
   problem this direction exists to avoid.
2. An item must exist when the library holds no file for the citable
   work, so an item cannot require a file to live in.
3. Several artifacts may represent one item, and none of them may become
   the privileged item record merely because it happens to be a file.

Finding every artifact of one item SHALL therefore be a read of the
artifact records rather than a field lookup, and that cost is accepted:
the records are small, flat and greppable, so even the reverse direction
has an answer that needs no database.

#### Scenario: A file manager moves an artifact
- **WHEN** an artifact is moved to another subdirectory of the library
  outside borax
- **THEN** the item it is linked to is unchanged and still correct, and
  the only thing out of date is the artifact record's last-known path

#### Scenario: The item names no path
- **WHEN** an item's file is read
- **THEN** it carries no artifact path and no artifact identity, and
  nothing in it goes stale when a file moves

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

### Requirement: borax brings no file into the library
borax SHALL NOT copy or move a file into a library. A file named as input
from outside the library tree SHALL be resolved and renamed where it
sits, and SHALL receive no artifact record and no item — a library
records the files inside it, and a file outside is outside.

A run that renames a file outside the library SHALL report that the file
is outside it and that nothing was recorded for the file, so that an
unrecorded rename is stated rather than silent.

Relocating a file into a library is the operator's own operation, in
whatever tool they use, and a file appearing anywhere in the tree SHALL
be an artifact borax recognises without having been told about it. That
is the whole of the import mechanism: there is no import.

#### Scenario: A download renamed where it sits
- **WHEN** `borax rename --apply ~/Downloads/paper.pdf` runs under a
  configuration whose library is elsewhere
- **THEN** the file is renamed in `~/Downloads`, nothing is copied or
  moved into the library, and the run reports that the file is outside
  the library and was not recorded

#### Scenario: Moving a file in is the whole import
- **WHEN** an operator moves a PDF into a library subdirectory with a
  file manager and `borax status` runs
- **THEN** the file is counted as an artifact and as an orphan, with no
  import step having been performed

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
  and the operator renames the file
- **THEN** the change is not accepted: the renamed file is recorded as
  a new artifact, and the record that did not accept the bytes keeps
  its path, its history and its link

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
adoption, reconciliation's writes to the records it repairs,
refreshes or accepts, and the replacement of an item's record with a
refresh candidate an operator accepted. The identity file is created before the item and
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
- an admission that leaves its record exactly as it was;
- a run given `--refresh` that replaces no item's record and makes no
  other store write. A refresh run that admits a file, or accepts a
  replacement, mints exactly as any store write does.

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
`unwritten`, a replacement reported `failed`), with a message naming
the identity file. Like any failed
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

#### Scenario: A replacement gives a library its identity
- **WHEN** an operator accepts a refresh candidate for an item of a
  library with no `.borax/library.toml`
- **THEN** the identity file exists afterwards, created before the item
  file was replaced

#### Scenario: A refresh that writes nothing mints nothing
- **WHEN** `borax resolve --refresh`, and a previewing `borax rename
  --refresh --batch`, each run over a library with no identity file
- **THEN** neither creates `.borax/library.toml`

#### Scenario: A refresh run that records a file mints
- **WHEN** an interactive `borax rename --refresh` over a library with no
  identity file keeps every item's record and renames one file, which it
  records
- **THEN** the identity file exists afterwards, created before the
  admission's first store write

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

### Requirement: An item's record is replaced only when an operator accepts a refresh candidate
borax SHALL change the record of an item that already exists only when an operator in an interactive `rename` accepts a refresh candidate for it, and SHALL then change exactly the compared fields that differ and their provenance entries, editing the item file as a document.

Every other write borax makes leaves an existing item as it is. An
admission selects or mints an item and does not rewrite one the
library holds. `borax reconcile` and `borax adopt` write artifact
records and mint items. A person editing the file remains free to
change anything in it, as the requirement "The library's text is
authoritative and is read directly" states.

The record written SHALL be the item's record with every compared field
that differs from the candidate's taken from the candidate, set where
the candidate holds a value and removed where it holds none, and the
provenance entry under each such field's name taken from the
candidate's likewise. Every other part of the item's record SHALL be
kept: the fields that agree, the other provenance entries, the source
fields and the confidence. What is written then has no difference from
the candidate, and nothing changes that the operator was not shown,
beyond the provenance of the fields they were.

Immediately before writing, borax SHALL read the item file again and
SHALL write nothing when:

- the file cannot be read;
- it no longer parses as the item of that identity;
- its record is no longer the record the candidate was compared with,
  because a person or another run changed it after this run read it.

That protects every edit completed before the re-read, a correction a
person saved while the question was on screen or another run's
replacement included. It does not protect an edit landing between the
re-read and the write's rename: the write replaces the file
unconditionally, since no portable filesystem operation replaces a file
only if it still holds what was read, and borax takes no lock, which a
text editor would not honour. An edit saved in that window is
overwritten, and nothing reports its loss. Beyond that window nothing
is lost silently: every write is reported in a `library-replacement`
event, and the file's `resolved` event keeps each replaced field's
previous value under `refresh`.

borax SHALL NOT discard what the operator was not shown. A replacement
SHALL be refused as a whole, writing nothing, when the value the item
file holds for any key it would replace, a compared field or a
provenance entry of one, is annotated:

- **It holds a key the record model does not read**, at any depth. The
  keys the model reads inside a compared field are exactly `family` and
  `given` in each entry of `author`, and `date-parts` in `issued`; every
  other compared field and every provenance entry is a string and holds
  none. Any other key within such a value, whether the value is written
  as a table, an array of tables or an inline table, is one the model
  does not read: reading the item ignores it, so it was never part of
  what the operator was shown, and replacing the value would delete it.
- **It holds a comment**: on any line the value occupies, from its key's
  line, a comment trailing the value included, to its last line; and,
  for a value written as a table or an array of tables, from its first
  header to its last line, the comment lines directly above each of its
  headers included. A comment on the lines above a plain `key =` line
  belongs to the key, which stays where it is, and survives.

The refusal SHALL name every annotated key, provenance entries as
`provenance.<field>`. Values carrying neither are replaced as below.

The write SHALL edit the file as a document, as artifact records are
edited. Only the keys whose values change are set or removed, within
`[record]`, its `borax` table and that table's `provenance`. The item's
`id` SHALL survive, and so SHALL every key the record model does not
know and every comment, none of which can lie inside a value replaced,
and the key order and the formatting of every key left alone. The
file's name SHALL NOT change. The write SHALL go through the
same writer as every other store write of the run, which mints the
library's identity first where it has none, as the requirement "A
library's first store write mints its identity" states. The file SHALL
be replaced whole and atomically, as every record is.

A replacement SHALL write nothing else: no artifact record, no history
entry, no item link, no content-index fact and no library reference.
The item keeps its identity, so every reference to it reads the
replaced record.

Every replacement attempted SHALL be reported in a `library-replacement`
event, when the write is made, carrying:

- the `path` of the file the question was about;
- the `item` identity;
- the item `file`, as a full path;
- the `fields` replaced, by their keys, in the compared fields' order;
- a `write` of `written`, or `failed` with a `message` saying why
  nothing was written: the file could not be read, holds another item,
  changed after the run read it, holds notes in the fields named, or
  could not be written.

The event SHALL count toward no total, and a failure SHALL NOT change
the run's exit status.

#### Scenario: Only the corrected field changes
- **WHEN** an operator accepts a candidate differing from a
  hand-corrected item in its title alone
- **THEN** the item file afterwards differs from before in its `title`
  line alone, keeps its name and its `id`, and a comment and a key the
  record model does not know, both written in it by hand, are still
  there

#### Scenario: A correction made while deciding
- **WHEN** the operator edits the item's volume in a text editor while
  the question is on screen, and then accepts the candidate
- **THEN** nothing is written, the `library-replacement` event reports
  the write `failed` with a message saying the file changed after the
  run read it, and the edit stands

#### Scenario: A field the candidate adds
- **WHEN** the accepted candidate names pages the item lacks
- **THEN** the item gains `page` and the provenance entry for it, and
  every field the two records agreed on is untouched

#### Scenario: A replacement is reported
- **WHEN** a replacement is written
- **THEN** a `library-replacement` event names the file, the item, the
  item file and the fields replaced, with `write` `written`, and it is
  counted in no total

#### Scenario: An author table carrying an identifier the model does not read
- **WHEN** an operator accepts a candidate differing in `author`, and the
  item file's `[[record.author]]` table holds `orcid` beside `family`
  and `given`
- **THEN** nothing is written, and the `library-replacement` event
  reports `failed` with a message naming `author`

#### Scenario: A comment inside the date of issue
- **WHEN** the candidate differs in `issued`, and the item file's
  `issued` value carries a trailing comment
- **THEN** nothing is written, and the message names `issued`, along
  with every other annotated field the replacement would have touched

#### Scenario: A clean author list is replaced
- **WHEN** the candidate differs in `author`, and the item file's
  authors hold only `family` and `given` and no comment
- **THEN** the item's authors are the candidate's afterwards, and every
  other line of the file is as it was

#### Scenario: A comment above a replaced key survives
- **WHEN** the candidate differs in `title`, and a comment stands on the
  line above the item file's `title =` line
- **THEN** the title is replaced and the comment is still above it

