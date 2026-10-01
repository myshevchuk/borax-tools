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
synchronised or versioned as a unit.

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
record and no item. Reporting takes the directory it was given as the
root of what it reports, so borax stays useful on a directory nobody has
marked, and the marker buys writing rather than seeing.

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

#### Scenario: Two hundred files borax has never seen
- **WHEN** `borax status` runs over a marked directory holding 200 PDFs,
  no items and no artifact records
- **THEN** it reports 200 artifacts, 0 items, 0 artifact records and 200
  orphans, having been given no command before it

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
  that its size or modification time changed, and `borax reconcile`
  runs
- **THEN** its record is the same file under the same artifact UUID, its
  new hash is appended with the run that recorded it, and its earlier
  hashes and their provenance are still there

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
The item store and the artifact records SHALL be the authoritative state
of a library, and every answer borax gives about a library SHALL be read
from those files directly. There SHALL be no index, no database and no
cached listing in this change: a count, a listing and a validation are
each a walk of the tree and a read of the store.

Deleting any derived state SHALL change no answer about a library. The
response cache and the content index are caches, so clearing them costs
round trips and loses no library content; a citation sidecar is derived
output, so deleting every sidecar in a library changes no item, no
artifact record and no count.

The direct implementation is deliberate. It is what demonstrates that
the files rather than a program's index are the library, and it is the
baseline any later measurement that argues for an index is taken
against.

#### Scenario: Derived state is disposable
- **WHEN** the response cache, the content index and every citation
  sidecar in a library are deleted and `borax status` runs
- **THEN** the artifact, item, record and orphan counts are what they
  were

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
3. **Edited in place.** A record still unresolved whose last-known path
   holds an unclaimed artifact matching no record's history is that
   artifact edited since borax last saw it: its new hash is appended
   after the hashes already recorded, and it claims that artifact.
4. **Unresolved.** Any record still unresolved is left exactly as it is.

The order is what makes two records whose files swapped paths both
repair: each file matches the other record's history in step 2, so
neither is mistaken in step 3 for the other's artifact edited in place.

A reconcile SHALL refresh a record's recorded size and modification
time whenever it confirms or repairs that record, so that those fields
describe the file as borax last saw it rather than as it once was. This
includes a record confirmed by a hash match after the fast path missed:
the bytes are the same and the record is right about them, and the
fields the fast path compares are the ones that were wrong. A record
whose recorded fields already describe the file is confirmed by the
fast path itself and needs no refresh.

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
run to make.

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
  different size or modification time, and `borax reconcile` runs
- **THEN** its new hash is appended after the hashes already recorded,
  with the run that recorded it, and it is reported as changed since
  borax last saw it

#### Scenario: A change the fast path cannot see
- **WHEN** an artifact's bytes changed while its size and modification
  time did not, and `borax reconcile` runs
- **THEN** nothing is reported for it and nothing is written, and
  `borax reconcile --rehash` reports the change and appends the hash

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

`borax validate` SHALL repair nothing and SHALL refuse nothing. A
finding is a report about a library, not a rejected write: a writer that
would produce one is not stopped, because a tool that refuses is the
enforcing tool this design exists not to be. `borax validate` SHALL
exit with the partial-success code when it reports any finding and 0
when it reports none.

An orphan, an artifact borax cannot find, and an item nothing links to
SHALL be reported as counts rather than as findings. None is a malformed
library: the first is work to do, the second is history the library
deliberately keeps, and the third is an ordinary item for a work with no
file.

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
updating the record of a file that has one. A preview SHALL write no
artifact record and no item, and a run with duplicate detection off
(`--no-record` or config) SHALL write neither either.

Updating a record SHALL write the file's content hash into that
record's hash history when it is not already the history's current
hash, together with the file's library-relative path and its size and
modification time as they are after the run's work on it. So a record
an applying run touched describes the file it names: its current hash
is that file's, and the fields reconciliation's fast path compares are
that file's too.

This is stated rather than left to follow from what a record holds,
because the evidence that identifies a record is not evidence about
content. A record is identified by its recorded path holding the file,
and it is confirmed by the fast path *or* by a recorded hash — and the
fast path compares size and modification time, which a same-length
in-place edit leaves alone. An update that wrote only the new path
would therefore leave a history whose newest hash is not the file's, and
nothing afterwards would say so: the fast path would settle that record
on every later reconcile, and the file's actual bytes would appear in no
record at all.

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

When a record is minted, the run SHALL link it to the item the library
already holds carrying one of the resolved record's identifiers, or to
one it mints from that record when the library holds none. A record with
no identifier at all SHALL still get an item, since an item's identity
is its minted UUID and identifiers are optional — so two *different*
files carrying no identifier become two items, which states what borax
knows rather than guessing a match.

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
edited-in-place step needs a file at the recorded path, so neither route
could find the artifact again.

The reason SHALL name `borax reconcile --rehash` as the remedy, and
SHALL NOT tell the operator to re-run without the flag. Reconciling
with `--rehash` reaches this file by its recorded path, finds bytes no
record's history holds, and appends them as an edit in place; the move
then proceeds under either setting, because the record's history holds
the file's hash. Re-running without the flag is *not* a remedy that
always works: where the file's size or modification time changed along
with its bytes, the record is settled by neither clause of the pre-move
test, and a run without the flag refuses the move for that reason
instead. A diagnostic may not name a retry that cannot succeed, so this
one names the operation that repairs the record rather than the
invocation that sometimes skirts it.

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
  re-linked to the item for the supplied record, the run reports both
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
  the record and `borax reconcile --rehash`, which records the file's
  current bytes and lets the move proceed afterwards

#### Scenario: An edit the fast path cannot see does not license a move
- **WHEN** `borax rename --apply --no-record` reaches an artifact edited
  in place to the same length with its modification time preserved, so
  that the record's recorded size and modification time still match the
  file while no recorded hash is its
- **THEN** the move is refused, because the confirmation the fast path
  gives would not survive it: the record would name a path holding
  nothing and hold no hash matching the file

#### Scenario: A recorded move leaves the record describing the file
- **WHEN** an applying run without the flag moves that same artifact
- **THEN** its record afterwards names the new path, carries the file's
  current hash as the newest in its history with the earlier hashes
  still there, and carries the file's size and modification time — so
  the next reconcile settles it on the fast path

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
an orphan.

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
- **THEN** the first is still an orphan and is counted as one, the
  second is byte-identical including its item link, and a second run of
  the command writes no library state

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
  orphan, rather than reporting a failure

