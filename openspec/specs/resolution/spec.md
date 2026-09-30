# resolution Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
### Requirement: Sources are queried by identifier type and priority
Resolution SHALL dispatch on identifier type: DOIs query Crossref first,
falling back to OpenAlex; arXiv identifiers query the arXiv API first. A
source failure (network error, HTTP error, record not found) SHALL fall
through to the next source in priority order; only when all applicable
sources fail is the file diverted to the skip queue.

#### Scenario: Crossref outage falls back to OpenAlex
- **WHEN** resolving a DOI while Crossref returns HTTP 503 and OpenAlex
  holds the record
- **THEN** resolution succeeds with a record whose provenance names
  OpenAlex

#### Scenario: Identifier unknown everywhere
- **WHEN** a syntactically valid DOI is found in no configured source
- **THEN** the file is diverted to the skip queue with reason
  "identifier not resolvable" and the batch continues

### Requirement: Responses are cached locally
Successful source responses SHALL be cached on disk keyed by normalized
identifier, and resolved records SHALL additionally be indexed by the
source file's content hash. A cache hit SHALL be used without any network
request. A bypass flag (`--no-cache`) SHALL force live queries for every
file the run's library does not answer for, and a cache subcommand SHALL
be able to clear the cache.

Both are caches, and neither is asked about a file its library tracks:
the run's library answers first, as the requirement "A file its library
tracks resolves from its library item" states. The library is not a
cache, so `--no-cache` SHALL NOT bypass it, and clearing the cache SHALL
change no answer the library gives.

#### Scenario: Re-run over the same directory is offline
- **WHEN** a directory is processed a second time with an intact cache
- **THEN** no network requests are made and results are identical to the
  first run

#### Scenario: Renamed file, same content
- **WHEN** a previously resolved file is encountered again under a
  different name with identical content
- **THEN** its record is served from the content-hash index without
  re-extraction or network access — or, where the run's library tracks
  the file under its new name, from its library item instead

### Requirement: Network use is polite and bounded
Every request SHALL carry a User-Agent identifying borax and its version
and, when configured, a contact mailto (Crossref/OpenAlex polite pools).
Requests SHALL be rate-limited per source, and concurrent resolution
across files SHALL be bounded by a configurable limit.

Concurrency SHALL NOT change what a run reports: results are ordered by
their input position, so the event stream of a concurrent run is
identical to a sequential one's over the same inputs. Rate limiting is
per service and independent of the number of threads in flight.

#### Scenario: Concurrency does not reorder the stream
- **WHEN** the same batch is resolved with a concurrency of one and with
  a concurrency of eight
- **THEN** both runs emit the same events in the same order

#### Scenario: Polite-pool identification
- **WHEN** a contact address is set in configuration and a Crossref
  request is issued
- **THEN** the request carries the configured mailto and the borax
  User-Agent

### Requirement: Ambiguity is skipped, never guessed
Resolution SHALL divert files with conflicting or low-confidence results
(e.g. embedded metadata and the resolved record disagreeing on title
beyond a normalization tolerance) untouched to the skip queue with a
stated reason. Batch mode SHALL NOT auto-accept a low-confidence match.

The tolerance SHALL be a similarity threshold, not equality. Titles SHALL
be compared as content words after folding both sides toward the lossiest
encoding either could carry — Latin letters transliterated to ASCII,
every other non-alphanumeric character (punctuation, dashes, and letters
with no transliteration) treated as a separator, function words dropped —
and SHALL be taken to name the same work when either is a prefix of the
other, when they differ only in where words were split, or when their
similarity reaches the threshold. The skip event SHALL report the
similarity, so a near miss is distinguishable from two unrelated works.

A document may claim several titles (an XMP `dc:title` and an Info
dictionary title, which need not agree). Every claim SHALL be considered,
and agreement by any one of them SHALL clear the file. A claim that does
not plausibly name a work — a placeholder, a filename, a bare identifier,
or a short value sharing nothing with the record — SHALL NOT count as
evidence of disagreement, since a producer's leftover contradicts every
record and would otherwise make a resolvable file permanently unskippable.

A record an operator accepted over a conflict SHALL NOT be judged
again. It is written to the content index under the file's content
hash, and a later run resolving that file from the index SHALL use it
as it uses any record the index holds, without re-reading the file or
re-checking its titles: the decision was made once, by the only party
entitled to make it. Where the run that accepted it also recorded the
file in a library, a later run resolves the file from the item its
artifact record was linked to, and that item is not judged either.

Only a person SHALL be able to accept a conflicting record. In an
interactive rename run, a file with a conflict SHALL be shown to the
operator with the extracted title, the record's title and their
similarity, and the operator MAY accept the record by choosing to rename
the file to the target it names. No setting, flag or default SHALL
accept one, and a batch run SHALL skip every conflict as this
requirement describes.

#### Scenario: Metadata conflict
- **WHEN** a file's embedded title and the Crossref record's title
  disagree materially
- **THEN** the file is skipped with reason "metadata conflict", the
  reported similarity is below the threshold, and no rename is planned
  for it

#### Scenario: A character the producer could not encode
- **WHEN** a file's embedded title is the record's title with a Greek
  letter and its hyphens dropped, as a typesetter that cannot encode them
  writes it
- **THEN** the titles agree and the file resolves

#### Scenario: Placeholder alongside a real title
- **WHEN** a file's XMP `dc:title` is a producer's placeholder and its
  Info dictionary carries the real title
- **THEN** the placeholder is not treated as evidence, the real title
  agrees, and the file resolves

#### Scenario: Operator accepts a conflict
- **WHEN** an interactive run shows a file whose manuscript title differs
  from its published record's, and the operator chooses to rename it
  anyway
- **THEN** the file is renamed from that record, and its `resolved` event
  carries the conflict it overrode with the reported similarity

#### Scenario: An accepted conflict is not re-judged
- **WHEN** a file whose record an operator accepted over a conflict is
  reached again by a batch run
- **THEN** it resolves from the content index — or from its library item,
  where its library tracks it — and is not skipped as a conflict

#### Scenario: Batch still skips
- **WHEN** the same file is reached by a batch run with `--apply`
- **THEN** it is skipped with reason "metadata conflict" and not renamed

### Requirement: The run summary reports the skip queue
Every run that can skip a file SHALL report each skipped file with its reason, in both human and JSON output modes, and SHALL end with a summary counting the skips.

The listing is the skip queue as it happened: a `skipped` event for
each reason a file was skipped, written as a line of its own in the
human output and as a JSON object in the JSON output. A file is
usually skipped once, but not always — a `bib` run whose sidecar write
fails and whose master merge then fails too skips that file twice, once
for each. The count is the `skipped` total of `run-finished`, which
counts those events rather than distinct files, and which the human
output states in the
closing summary line of each command that can skip — `resolve`,
`rename` and `bib`. A command that cannot skip a file, such as
`status`, owes no skip queue and no summary line for one; what its
human output closes with is the `cli` requirement "A run's human
summary fits its command".

#### Scenario: Summary after a mixed batch
- **WHEN** a batch resolves 8 files and skips 2
- **THEN** the summary lists the 2 skipped files with their reasons, and
  the JSON stream contains one skip event per skipped file, and the
  closing summary line of the human output counts 2 skipped

### Requirement: A resolution reports the evidence it was checked against
A `resolved` event SHALL carry every title the file claims for itself, each with where it was read — the XMP packet or the document information dictionary — in the order they were read, including claims the conflict check did not count as evidence. A file that was not opened, because the content index or the file's library item answered, SHALL carry no claims.

A `resolved` event SHALL carry the identifier the run looked up,
alongside the identifiers the record itself holds. The two are not
always the same — a record resolved from an arXiv identifier may carry
a DOI, which is what the event reports as the record's — and only the
former is evidence about the file.

A `resolved` event SHALL name as its source the services that supplied
the record. When the content index or the library answered, those are
the sources the record's per-field provenance names other than
extraction itself, in a fixed order that does not depend on the
identifier type: Crossref, OpenAlex, arXiv, DataCite, PubMed, then a
sidecar. A record whose provenance names no such source SHALL report
where it was kept: the content index itself as `cache`, and a library
item as `library`. Whether the content index answered SHALL continue to
be reported separately, and so SHALL whether the library did.

#### Scenario: Claims reach the stream
- **WHEN** a file whose XMP carries a title and whose document
  information carries a producer's placeholder is resolved
- **THEN** its `resolved` event carries both titles, marked XMP and
  document information respectively

#### Scenario: A content-index answer names its service
- **WHEN** a file is resolved from the content index and its record's
  provenance attributes its fields to Crossref
- **THEN** its `resolved` event names Crossref as the source, reports
  that the content index answered, and carries no claims

#### Scenario: A library answer names its service
- **WHEN** a file is resolved from its library item and the item's
  provenance attributes its fields to Crossref
- **THEN** its `resolved` event names Crossref as the source, reports
  that the library answered and the content index did not, and carries
  no claims

#### Scenario: The looked-up identifier is carried
- **WHEN** a file resolves from an arXiv identifier to a record that
  also carries a DOI
- **THEN** its `resolved` event reports the arXiv identifier as the one
  found, and the record's DOI as the record's

#### Scenario: A record built from two services
- **WHEN** a record whose provenance names OpenAlex for some fields and
  Crossref for others is served by the content index
- **THEN** its `resolved` event names Crossref before OpenAlex

### Requirement: An operator can supply an identifier
In an interactive rename run, the operator SHALL be able to supply an identifier for a file, and resolution SHALL treat a supplied identifier as it treats an extracted one: parsed and normalised by the same rules, dispatched to the same services in the same priority order, and checked against the titles the file claims.

A supplied identifier SHALL be accepted as a DOI in any form extraction
accepts, as an arXiv identifier, or, with a `pmid:` or `isbn:` prefix,
as a PMID or ISBN. Input that is none of these SHALL be refused with the
forms accepted and SHALL NOT be sent to any service.

A record resolved from a supplied identifier SHALL be described to the
operator and used only on the operator's further answer. A disagreement
between it and the file's claimed titles SHALL be shown and SHALL NOT
by itself prevent that answer.

A `resolved` event for a record found from a supplied identifier SHALL
report `supplied` as the pass that found the identifier.

#### Scenario: A preprint without a DOI
- **WHEN** an interactive run finds no identifier in a file and the
  operator supplies `arXiv:2401.12345`
- **THEN** the arXiv record is resolved, described, and proposed as the
  file's name, and nothing moves until the operator answers

#### Scenario: A pasted DOI link
- **WHEN** the operator supplies `https://doi.org/10.1021/JACS.4C01234`
- **THEN** the identifier resolved is the DOI `10.1021/jacs.4c01234`

#### Scenario: Input that names nothing
- **WHEN** the operator supplies `see email from Anna`
- **THEN** it is refused with the accepted forms, no service is queried,
  and the operator is asked again

#### Scenario: An unknown supplied identifier
- **WHEN** the operator supplies a well-formed DOI that no service holds
- **THEN** the services' answers are shown and the file's question is put
  again

### Requirement: An operator's answer about a file is remembered
When an interactive run renames a file from a record found from a supplied identifier, or from a record accepted over a conflict, it SHALL write that record to the content index under the file's content hash, so that a later run resolves the file from it as from any record the index holds.

Inside a library the run records in, the rename also links the file's
artifact record to the item for that record, as the `library` capability
states, and a later run SHALL resolve the file from that item: the
library tracks the file, and its item answers before the content index
is asked. The content index entry is written all the same, and is what
answers for the file where the library does not.

No record SHALL be written for a file the operator did not rename: a
record resolved from a supplied identifier and then skipped, abandoned
for another identifier, or left by quitting SHALL NOT be stored. What
the index already held for that file SHALL be left as it is — the
rejected candidate is not kept, and nothing the file was identified as
before is removed.

Remembering is best-effort, as every write to the response cache is: an
entry that could not be written means the file is asked about again,
which is how losing an answer fails safely. A run SHALL NOT report a
write it could not make as a failure of the rename it followed.

#### Scenario: Asked once
- **WHEN** a file is renamed in an interactive run from a supplied
  identifier, and a batch run later reaches it under its new name
- **THEN** the batch run resolves it without extraction or any source
  being queried — from the content index, or from its library item where
  its library tracks it — and reports it already named

#### Scenario: A wrong identifier abandoned
- **WHEN** the operator supplies an identifier, sees a record for a
  different paper, and skips the file
- **THEN** the content index holds no record for the file, and the next
  interactive run asks about it again

#### Scenario: The answer could not be kept
- **WHEN** a rename from a supplied identifier is made and the content
  index cannot be written
- **THEN** the rename stands and is reported as any rename is, and the
  next run over that file asks about it again, unless its library
  records the file and so answers for it

### Requirement: A file its library tracks resolves from its library item
A file the run's library tracks SHALL resolve to the record of its library item, and resolution SHALL ask the library before the content index and SHALL then ask neither the content index, extraction, nor any service for that file.

A file is tracked when, among the artifact records whose last-known
path is the file's own path, exactly one holds the file's content hash
anywhere in its hash history, and that record links to an item that
exactly one item file of the library carries.

Paths SHALL be compared as the `ledger` capability's duplicate checks
compare them: the library root and the file's path are both made
absolute against the working directory and normalised lexically, with
no symlink resolved, before containment in the library, exclusion of a
subtree it does not own, and a record's path are judged. A file named
by a relative path, or by a spelling with `.` or `..` in it, SHALL be
consulted exactly as its absolute spelling is.

Any entry of the history SHALL count, not only the newest, so that an
artifact restored to an earlier version still resolves from its item.
This is an interim rule and not a statement that every entry was
accepted as the work the record now links to: a history entry does not
record which item the artifact was linked to when it was written, so
bytes an artifact held before its record was re-linked to another item
resolve from the item it links to now.

Both the path and the hash SHALL be required. A record naming the
file's path whose history does not hold the file's bytes SHALL NOT
answer for it, since the library has never accepted those bytes as that
artifact; the requirement "A library that cannot answer for a file says
so" states what happens instead. A file whose bytes a record holds at
another path — a copy, or a recorded artifact moved out of band before
`borax reconcile` repaired its record — SHALL be untracked, as `borax
status` counts it an orphan, and SHALL resolve through the content
index, extraction and the services as it did before this requirement.

The run's library SHALL be the one the run already discovers: the
directory holding the nearest `.borax.toml` above the run's start
directory, or the `library-root` setting. A file outside it, or inside
a subtree it excludes, SHALL NOT be consulted, and a run with no library
resolves every file as it did before this requirement. The library's
stores SHALL be read once per run, before the first file is resolved,
and reading them SHALL NOT refuse a run. Every file of the run SHALL be
consulted against the library as it was read then, and not as the
run's own admissions have since left it: what a run learns from
recording one file is not the library's statement about another.

`borax resolve`, `borax rename` — batch and interactive, previewing and
applying — and `borax bib` SHALL consult the library. `--no-cache`
SHALL NOT bypass it: the library is not a cache. `--no-record` SHALL NOT
bypass it either: that setting withholds the account of admissions and
the writes behind it, and asking the library what a file is does
neither.

A library answer SHALL NOT open the file and SHALL NOT compare its
titles against the item: the item is the library's accepted statement
of the work, and comparing records can reveal staleness but cannot
establish authority. A library answer SHALL neither read nor write the
content index, whose entry for the file's hash is left exactly as it
was.

The library's two duplicate checks keep their places. A rename run's
content-duplicate check SHALL run before the library is asked, as it
runs before the content index, and the work check SHALL apply to a
library answer as to any other record.

In an interactive run, a file the library answered for SHALL be put the
question its record calls for, exactly as any resolved file is, and
supplying an identifier for it re-identifies the file as the `library`
capability states.

#### Scenario: A correction in the library wins over the content index
- **WHEN** a tracked PDF's linked item has had its title corrected in a
  text editor since an earlier run left the old record in the content
  index, and `borax resolve --json` runs on the PDF
- **THEN** its `resolved` event carries the corrected title, reports
  `library` as its `tier`, `cached` false and no claims, no service is
  queried, and the content index's entry for the file is unchanged

#### Scenario: Bypassing the cache does not bypass the library
- **WHEN** the same run is made with `--no-cache`
- **THEN** the file resolves from its library item with the corrected
  title, no service is queried, and nothing is written to the content
  index for it

#### Scenario: An untracked file resolves as before
- **WHEN** a PDF inside a library that no artifact record names is
  resolved and the content index holds its hash
- **THEN** it resolves from the content index exactly as before, and its
  `resolved` event reports the library's answer as untracked

#### Scenario: A copy is not the artifact
- **WHEN** a byte-identical copy of a tracked artifact sits at another
  path in the library with no record naming that path, and `borax
  resolve` runs on the copy
- **THEN** the copy does not resolve from the artifact's item, is
  reported untracked, and resolves through the content index,
  extraction and the services as before

#### Scenario: Outside any library nothing changes
- **WHEN** `borax resolve` runs on a file with no `.borax.toml` above it
  and no `library-root` set
- **THEN** it resolves as before, and its `resolved` event reports no
  library answer

#### Scenario: A relative path names the same file
- **WHEN** `borax resolve paper.pdf` is run from the library root on a
  tracked file, and again as `borax resolve ./sub/../paper.pdf` from
  the same directory
- **THEN** both runs resolve it from its library item, exactly as its
  absolute path does

#### Scenario: An earlier version of an artifact
- **WHEN** a tracked artifact's bytes are restored to an earlier version
  whose hash is an older entry of its record's hash history
- **THEN** it resolves from its library item

#### Scenario: Bytes from before a re-link
- **WHEN** an artifact whose history holds the hash of work A's bytes
  was re-linked to work B's item when the operator re-identified it,
  and work A's bytes are restored at its path
- **THEN** the file resolves from work B's item, which is the interim
  rule this requirement states

#### Scenario: A rename names the file from its item
- **WHEN** `borax rename --apply` runs over a tracked file whose
  item's record has been corrected so that the filename template renders
  a different name
- **THEN** the file is renamed to the name rendered from the item's
  record, and no service is queried for it

#### Scenario: A rename that keeps no record still asks the library
- **WHEN** the same run is made with `--no-record`
- **THEN** the file resolves from its library item as before, no
  duplicate check is made, and no artifact record and no item is written

#### Scenario: What a run records does not answer for another file
- **WHEN** an applying rename run reaches two files whose records both
  link to one item the library does not hold, and records the first
  from the record it resolved by fallback
- **THEN** the second is still reported as linking to an item the
  library does not hold, and is not answered with the first file's
  record

#### Scenario: A bibliography cites the item
- **WHEN** `borax bib` runs over a tracked file whose item's title has
  been corrected
- **THEN** the bibliography entry written for it carries the corrected
  title

### Requirement: A library that cannot answer for a file says so
When the run's library cannot answer for a file it records or may record, resolution SHALL report why on the file's resolution event and SHALL then resolve the file as it resolves a file the library does not track.

The library cannot answer when one of the following holds, each reported
as its own kind:

- no record naming the path holds the file's content hash
  (`unrecognised-content`): the file was edited or replaced since the
  library recorded it;
- more than one record naming the path holds that hash (`ambiguous`);
- the one record that holds it links no item (`no-item`);
- it links an item the library does not hold (`dangling-item`);
- it links an item the library does not hold, and either the item
  store could not be listed or an item file whose name claims that
  item's identity could not be read or parsed (`unreadable-item`),
  reported with that directory or file and the reason — an item store
  that could not be read SHALL NOT be taken to lack the item;
- it links an identity that more than one item file carries
  (`ambiguous-item`), reported with those files;
- the file could not be hashed, so no record naming its path can be
  confirmed (`unhashable`);
- no record the library could read names the file's path, but the
  artifact store could not be listed or holds a file that could not be
  read or parsed (`unreadable-records`), so a record the library could
  not read may name it.

A file whose path no record names, in an artifact store read whole, is
untracked, whether or not it could be hashed, and is not a problem the
library reports.

A store directory that is absent SHALL be an empty store. A store
directory that exists but cannot be listed, and an entry of one that
cannot be read, SHALL be a fault of that store and SHALL NOT be taken
for an empty store.

Resolving the file as untracked means the content index (unless
`--no-cache`), then extraction, the services and the conflict check,
with the content index written on success as for any fresh resolution.
The report SHALL travel with whatever that produced — the `resolved`
event, or the `skipped` event that is the file's resolution verdict — so
that a record reached this way is distinguishable both from a library
answer and from an ordinary resolution of an untracked file.

A library that cannot answer SHALL NOT make the file a skip or a
finding, and SHALL NOT by itself change the run's exit status. Nothing
is repaired or written by asking: `borax validate` and `borax reconcile`
remain the commands that examine and repair a library.

An artifact record file that cannot be read or does not parse SHALL
cost its own artifact and nothing else, as the `ledger` capability
states for duplicate detection: a file whose own record reads cleanly
is still tracked. A run that consults a library whose artifact store
has faults SHALL warn once, on standard error, saying that the records
could not be listed or naming how many could not be read, and that
`borax validate` reports them.

#### Scenario: A dangling item link
- **WHEN** a file's artifact record at its path holds its hash and links
  to an item whose file has been deleted, and the content index holds
  the file's hash
- **THEN** the file resolves from the content index, and its `resolved`
  event reports a dangling item naming the artifact and the missing
  item, and its human line says the library could not answer and why

#### Scenario: Bytes the library has not recorded at a recorded path
- **WHEN** another PDF has been copied over a tracked artifact at its
  recorded path, and `borax resolve --no-cache` runs on it
- **THEN** the linked item does not answer for it, its event reports
  unrecognised content naming the artifact recorded at that path, and it
  resolves from its own identifier through the services

#### Scenario: An item file that will not parse
- **WHEN** a tracked file's linked item file has been damaged so that it
  no longer parses
- **THEN** the file's event reports an unreadable item naming the item
  file and the parser's message, and the file resolves as untracked

#### Scenario: Two records claim one file
- **WHEN** two artifact records both name a file's path and both hold
  its hash
- **THEN** neither answers, the file's event reports the ambiguity
  naming both artifacts, and the file resolves as untracked

#### Scenario: A file that cannot be hashed
- **WHEN** a file at a recorded path cannot be hashed
- **THEN** its resolution event reports that the file could not be
  hashed, naming the artifact recorded at that path, and it resolves or
  is skipped as it would have been before

#### Scenario: One item identity in two files
- **WHEN** a tracked file's record links an item whose identity two item
  files carry
- **THEN** neither item answers, the file's event reports the ambiguous
  item naming both files, and the file resolves as untracked

#### Scenario: An artifact store that cannot be listed
- **WHEN** `.borax/artifacts/` exists but cannot be listed, and `borax
  resolve` runs on a file in the library
- **THEN** its event reports that the library's records could not be
  read rather than that the file is untracked, the file resolves as
  untracked, and the run warns once that the records could not be
  listed

#### Scenario: An item store that cannot be listed
- **WHEN** `items/` exists but cannot be listed, and a file's record
  links an item
- **THEN** its event reports an unreadable item naming the `items/`
  directory, not a dangling item

#### Scenario: A fallback that finds nothing still reports the library
- **WHEN** a file's record links a missing item, and the file carries no
  identifier
- **THEN** it is skipped as having no identifier, and that `skipped`
  event reports the dangling item

#### Scenario: One unreadable artifact record
- **WHEN** one file under `.borax/artifacts/` does not parse and `borax
  resolve` runs over the library
- **THEN** every tracked file whose own record parses resolves from its
  item, a file no readable record names is reported as one the library
  could not read the records for, the run warns once that one artifact
  record could not be read, and the exit code is what it would have been
  without the warning

### Requirement: A resolution says whether its library answered
Every `resolved` event, and every `skipped` event that is a file's resolution verdict, SHALL carry a `library` field stating what the run's library said about the file, and a record the library answered with SHALL be reported with `library` as its `tier`.

`library` SHALL be `null` when the library was not asked: the run has
no library, the file lies outside it or in a subtree it excludes, or
the resolution ended before the library was asked, as a content
duplicate does. It SHALL also be `null` on a `skipped` event that is
not a resolution verdict — a taken target, a declined move, a failed
rename, a refused move, an unciteable record or a bibliography that
could not be written — since the file's `resolved` event already
carries what the library said.

Otherwise `library` SHALL be an object tagged by `kind`: `tracked`
with the `artifact` and `item` identities; `untracked`; or one of the
kinds the requirement "A library that cannot answer for a file says so"
names — `unrecognised-content`, `ambiguous` and `unhashable` with the
`artifacts` recorded at the path, `no-item` with the `artifact`,
`dangling-item` with the `artifact` and `item`, `unreadable-item` with
the `artifact`, the `item`, the `path` of the item file or item store
that could not be read and the `message` reading it produced,
`ambiguous-item` with the `artifact`, the `item` and the item `files`
carrying it, and `unreadable-records` with whether the artifact store
could be `listed` and how many of its files were `unreadable`.
Identities SHALL be written as the canonical UUID text a record stores.

What the library said SHALL reach every event that reports the file's
resolution verdict, whichever way the run settled the file: a batch
rename's skip, an interactive run's report after its operator's answer,
and a verdict reached by asking the services again after an outage.

`library` reports what the library said, and `tier` where the record's
identifier came from, and the two are separate facts: an interactive
run whose operator supplied an identifier for a tracked file reports
`supplied` as its `tier` and `tracked` as its library's answer.

A `resolved` event answered by the library SHALL report `cached` as
false, since the content index did not answer; SHALL carry no claims,
since the file was not opened; SHALL report as found the record's own
identifier, since nothing was looked up; and SHALL name its source as
the requirement "A resolution reports the evidence it was checked
against" states. `tier` SHALL NOT be `null` on a library answer, so
that `null` continues to mean exactly that the content index answered.

These are additions to the event schema, and its version SHALL NOT
change for them: no event, field, reason or value is removed or
renamed, and no existing field's meaning changes.

The human rendering of a `resolved` event whose `tier` is `library`
SHALL be `<path>: resolved <identifier> via <source> (from the
library)`. The human rendering of a resolution event whose library
answer is one of the kinds the library could not answer with SHALL be
the line it would otherwise have, followed by `; the library could not
answer: ` and a statement of the problem, naming the artifacts, item
and files involved where it has them. A library answer of `tracked` on a record the operator
supplied, `untracked`, and `null` SHALL leave the line as it was.

#### Scenario: A library answer in the stream
- **WHEN** `borax resolve --json` resolves a tracked file whose item's
  provenance attributes its fields to Crossref
- **THEN** its `resolved` event carries schema version 3, `tier`
  `library`, `cached` false, empty `claims`, `source` `crossref`, and a
  `library` object of kind `tracked` naming the artifact and the item

#### Scenario: An item that names no service
- **WHEN** a tracked file's item was written by hand with no per-field
  provenance
- **THEN** its `resolved` event reports `library` as its source

#### Scenario: A library answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from its library item,
  whose DOI is `10.1039/c5ay00042d` and whose provenance names Crossref
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d via crossref (from the library)`

#### Scenario: A library problem read by a person
- **WHEN** `paper.pdf` resolves from the content index because its
  record links to an item the library does not hold
- **THEN** its line is its ordinary `(cached)` resolution line followed
  by `; the library could not answer: ` and a statement naming the
  artifact and the missing item

#### Scenario: A rename's skip keeps what the library said
- **WHEN** a batch `borax rename` reaches a file whose record links an
  item the library does not hold, and the file carries no identifier
- **THEN** its `skipped` event reports no identifier found and carries
  the dangling item

#### Scenario: Asking the services again keeps what the library said
- **WHEN** an interactive run reaches a file whose record links an item
  the library does not hold, whose services were unreachable, and the
  operator asks them again and they answer
- **THEN** the file's `resolved` event carries the dangling item

#### Scenario: An operator re-identifies a tracked file
- **WHEN** an interactive run reaches a tracked file and the operator
  supplies a different identifier and renames the file
- **THEN** the file's `resolved` event reports `supplied` as its `tier`
  and `tracked` as its library's answer

