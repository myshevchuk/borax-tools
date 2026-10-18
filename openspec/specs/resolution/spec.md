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
source file's content hash, through the identification evidence "The
content index keeps identification evidence, one fact per file" states,
and through the library references "The content index keeps references
to libraries, one file each" states. A cache hit SHALL be used without any network request. A bypass
flag (`--no-cache`) SHALL force live queries for every file the run's
library does not answer for, and SHALL skip reading the content index,
and a cache subcommand SHALL be able to clear the cache.

Only the response cache is a cache. The cache subcommand clears it and
nothing else: the content index lives outside the cache directory and
survives clearing it. Neither is asked about a file its library tracks:
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
- **THEN** its record is served through the content-hash index without
  re-extraction, and from the response cache without network access —
  or, where the run's library tracks the file under its new name, from
  its library item instead, and where a library reference for its bytes
  answers, from the item that reference names

#### Scenario: Clearing the cache spares the index
- **WHEN** `borax cache --clear` runs after a file was resolved, and the
  file is resolved again
- **THEN** it is not extracted, and the content index answered for it:
  where no library reference for its bytes answers, its identifier is
  looked up again over the network; where one answers, nothing is
  looked up and the record is the item's that reference names

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
A `resolved` event SHALL carry every title the file claims for itself, each with where it was read — the XMP packet or the document information dictionary — in the order they were read, including claims the conflict check did not count as evidence, as the `claims` of its `extraction` section's `titles`. A file that was not opened, because the content index or the file's library item answered, SHALL carry its titles as not attempted, with that reason, and not as a file that claims none.

A `resolved` event SHALL carry the identifier the run looked up, in its
`lookup` section, alongside the identifiers the record itself holds,
wherever the run looked one up. A record the library or a library
reference answered with was reached by no lookup, and its `lookup`
section says so. The
two are not always the same — a record resolved from an arXiv identifier
may carry a DOI, which is what the event reports as the record's
`identifier` — and only the former is evidence about the file.

Which services supplied the record's fields is the record's own per-field
provenance, which the event carries in `record`, and it is a separate
fact from where the record was retrieved. Where a rendering names the
services that supplied the record, they SHALL be the service that
answered the lookup, or, when the library answered or a library
reference answered for the file's bytes, the sources the record's per-field provenance names other than
extraction itself, in a fixed order that does not depend on the
identifier type: Crossref, OpenAlex, arXiv, DataCite, PubMed, then a
sidecar. A record whose provenance names no such source SHALL be
rendered with no service named, never with the name of the store it was
kept in. Whether the content index answered SHALL continue to be
reported separately, and so SHALL whether the library did.

#### Scenario: Claims reach the stream
- **WHEN** a file whose XMP carries a title and whose document
  information carries a producer's placeholder is resolved
- **THEN** its `resolved` event's `extraction.titles` is `read` with
  both titles, marked XMP and document information respectively

#### Scenario: A content-index answer names its service
- **WHEN** a file is resolved from the content index, with no library
  reference for its bytes answering, by looking up the DOI it holds,
  which Crossref's response cache answers
- **THEN** its human line names Crossref as the service, its
  `resolved` event's `record_retrieval` is `service-cache` naming
  Crossref, and its titles are not attempted because the content index
  answered

#### Scenario: A library answer names its service
- **WHEN** a file is resolved from its library item and the item's
  provenance attributes its fields to Crossref
- **THEN** its human line names Crossref as the service, its `resolved`
  event's `record_retrieval` is `library`, its `content_index.read` is
  not attempted because the library answered, and its titles are not
  attempted for the same reason

#### Scenario: A reference answer names its item's services
- **WHEN** a file outside any library is answered by a library
  reference whose item's provenance attributes its fields to Crossref
- **THEN** its human line names Crossref as the service, its
  `record_retrieval` is `library-reference`, and its titles are not
  attempted because the content index answered

#### Scenario: The looked-up identifier is carried
- **WHEN** a file resolves from an arXiv identifier to a record that
  also carries a DOI
- **THEN** its `resolved` event's `lookup.identifier` is the arXiv
  identifier, and its `identifier` is the record's DOI

#### Scenario: A record built from two services
- **WHEN** a record whose provenance names OpenAlex for some fields and
  Crossref for others is served by the library
- **THEN** its human line names Crossref before OpenAlex

### Requirement: An operator can supply an identifier
In an interactive rename run, the operator SHALL be able to supply an identifier for a file, and resolution SHALL treat a supplied identifier as it treats an extracted one: parsed and normalised by the same rules, dispatched to the same services in the same priority order, and checked against the titles the file claims.

A supplied identifier SHALL be accepted as a DOI in any form extraction
accepts, as an arXiv identifier, or, with a `pmid:` or `isbn:` prefix,
as a PMID or ISBN. Input that is none of these SHALL be refused with the
forms accepted and SHALL NOT be sent to any service. A refused input is
still a submission, and SHALL be reported with why it was refused, as
the requirement "Supplied text that names no identifier says why"
states.

A supplied identifier SHALL be looked up exactly as it was parsed and
normalised. Nothing SHALL complete, extend or otherwise repair it. A
DOI cut short in copying is still a well-formed DOI: it is looked up as
typed and reported with whatever the services answer about it.

A record resolved from a supplied identifier SHALL be described to the
operator and used only on the operator's further answer. A disagreement
between it and the file's claimed titles SHALL be shown and SHALL NOT
by itself prevent that answer. Agreement SHALL NOT stand in for the
answer either: until the operator answers, the record is pending,
whatever its title check concluded.

A `resolved` event for a record found from a supplied identifier SHALL
report the operator, and not an extraction pass, as the origin of the
identifier its `lookup` section names.

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

#### Scenario: A truncated DOI is looked up as typed
- **WHEN** the operator supplies `10.1039/c9cc02492`, which is the DOI
  `10.1039/c9cc02492a` with its last character lost, and no service
  holds it
- **THEN** the identifier looked up is `doi:10.1039/c9cc02492`, each
  service's answer about that DOI is reported, and no other DOI is
  looked up or proposed

#### Scenario: Agreeing titles do not accept a supplied record
- **WHEN** the operator supplies a DOI whose record's title matches the
  title the file claims
- **THEN** the record is described as pending, and nothing moves and
  nothing is written to the content index until the operator answers

### Requirement: An operator's answer about a file is remembered
When an interactive run renames a file from a record found from a supplied identifier, or from a record accepted over a conflict, it SHALL keep the operator's answer durably, in the library it records the file in and in the content index under the file's content hash, so that a later run resolves the file from it without asking again.

The library is the durable home of an accepted association. Inside a
library the run records in, the rename also links the file's artifact
record to the item for that record, and the history entry for the
file's bytes names that item, the identifier and its origin, `supplied`
or `overridden`, as the `library` capability states. That entry is the
decision, and a later run SHALL resolve the file from that item: the
library tracks the file, and its item answers before the content index
is asked. The content index entry is written all the same, after the
artifact record: the identifier with its origin, in the entry's
operator evidence. A reference to the library is written between the
artifact record and that evidence, as the `library` capability's
requirement "A library's references are
written after its records, and reconcile restores and repairs them"
states. Where the library does not answer for the file, a copy of the
same bytes elsewhere among them, that reference answers with the item
first, as "A library reference answers for a file before the entry's
evidence" states. The operator's identifier is looked up where no
reference answers. An interruption between the writes leaves the
decision in the library alone, and the next `borax reconcile` writes
both to the content index, as the `library` capability states.

Outside a library the content index is the one place the answer is
kept, and it keeps it durably: the content index is not a cache,
`borax cache --clear` leaves it, and an identifier extraction finds
never takes the place of an operator's, as "The content index keeps
identification evidence, one fact per file" states. A later run
looks the operator's identifier up wherever no library reference for
the bytes answers, so the record itself comes from the response cache,
or from the services when the response cache does not hold it. Where a
reference answers, nothing is looked up and the record is the item's
that reference names.

No record SHALL be written for a file the operator did not rename: a
record resolved from a supplied identifier and then skipped, abandoned
for another identifier, or left by quitting SHALL NOT be stored, in the
content index or in a library. What the index already held for that
file SHALL be left as it is — the rejected candidate is not kept, and
nothing the file was identified as before is removed.

Remembering in the content index is best-effort, as every write to the
response cache is: outside a library, an entry that could not be
written means the file is asked about again, which is how losing an
answer fails safely, and inside one the library still holds the
decision and its next reconcile writes it to the content index. A run SHALL NOT
report a write it could not make as a failure of the rename it
followed.

#### Scenario: Asked once
- **WHEN** a file is renamed in an interactive run from a supplied
  identifier, and a batch run later reaches it under its new name
- **THEN** the batch run resolves it without extraction and without a
  question — by looking up the identifier the content index holds, with
  no network request while the response cache holds it, or from its
  library item where its library tracks it, or, with no lookup, from the
  item a library reference names where one answers for its bytes — and
  reports it already named

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

#### Scenario: The library keeps the answer
- **WHEN** a file is renamed in a library from a supplied identifier,
  and the content index is deleted afterwards
- **THEN** its artifact record's newest history entry names the item,
  the supplied identifier and the origin `supplied`, a later run
  resolves the file from that item, and the next `borax reconcile`
  writes the supplied identifier to the content index again

#### Scenario: Clearing the cache forgets no answer
- **WHEN** a file outside any library was renamed from a supplied
  identifier, `borax cache --clear` runs, and a batch run reaches the
  file with the services reachable, no library reference for its bytes
  answering
- **THEN** the file is not asked about and not extracted, and the
  supplied identifier is looked up over the network

### Requirement: A file its library tracks resolves from its library item
A file the run's library tracks SHALL resolve to the record of its library item, and resolution SHALL ask the library before the content index and SHALL then ask neither the content index, extraction, nor any service for that file.

A file is tracked when, among the artifact records whose last-known
path is the file's own path, exactly one holds the file's content hash
anywhere in its hash history, that record links to an item that
exactly one item file of the library carries, and the item it answers
with for those bytes is carried by exactly one item file too.

Paths SHALL be compared as the `ledger` capability's duplicate checks
compare them: the library root and the file's path are both made
absolute against the working directory and normalised lexically, with
no symlink resolved, before containment in the library, exclusion of a
subtree it does not own, and a record's path are judged. A file named
by a relative path, or by a spelling with `.` or `..` in it, SHALL be
consulted exactly as its absolute spelling is.

Any entry of the history SHALL count, not only the newest, so that an
artifact restored to an earlier version still resolves from an item.
The item a record answers with for given bytes SHALL be the record's
link when those bytes are the history's newest entry, and otherwise
the item named by the newest entry holding them: the item those bytes
were accepted under, which each history entry records as the `library`
capability states. So bytes an artifact held before its record was
re-linked to another item resolve from the item they were accepted
under, and not from the item it links to now, while a link a person
corrected in a text editor decides for the artifact's newest bytes.
The `library` capability ties every change of a link to the bytes it
applied to, so a link corrected while bytes were newest still answers
for them once later bytes are accepted.

The record's link SHALL still be checked first, and a problem with it
reported as it always was. An answer through an earlier entry's item
SHALL report the link as the answer's `item`, and the item answered
with beside it, as "A resolution says whether its library answered"
states.

An entry that does not record which item the artifact was linked to
when it was written, as no entry written before entries named items
does, answers with the item the record links to now. That is the
interim rule this requirement once stated for every entry, kept only
where the history holds nothing better: it is not a statement that the
entry was accepted as the work the record now links to.

Both the path and the hash SHALL be required. A record naming the
file's path whose history does not hold the file's bytes SHALL NOT
answer for it, since the library has never accepted those bytes as that
artifact; the requirement "A library that cannot answer for a file says
so" states what happens instead. A file whose bytes a record holds at
another path — a copy, or a recorded artifact moved out of band before
`borax reconcile` repaired its record — SHALL be untracked, as `borax
status` counts it an orphan, and SHALL resolve through the content
index, extraction and the services as it did before this requirement.
The content index can answer for such a file through a reference to
this same library, naming the item its bytes' record answers with, as
"A library reference answers for a file before the entry's evidence"
states. The file is still untracked: that answer is the reference's,
not the library's, and is reported as such.

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
  `library` as its `record_retrieval`, the content index's read as not
  attempted because the library answered, and its titles as not
  attempted for the same reason, no service is queried, and the content
  index's entry for the file is unchanged

#### Scenario: Bypassing the cache does not bypass the library
- **WHEN** the same run is made with `--no-cache`
- **THEN** the file resolves from its library item with the corrected
  title, no service is queried, and nothing is written to the content
  index for it

#### Scenario: An untracked file resolves as before
- **WHEN** a PDF inside a library that no artifact record names is
  resolved and the content index holds its hash
- **THEN** it resolves from the content index exactly as before — by
  the item a library reference names where one answers, with no lookup,
  and otherwise by looking up the identifier the entry holds — and its
  `resolved` event reports the library's answer as untracked

#### Scenario: A copy is not the artifact
- **WHEN** a byte-identical copy of a tracked artifact sits at another
  path in the library with no record naming that path, and `borax
  resolve` runs on the copy
- **THEN** the copy is not tracked by the artifact's record: it is
  reported untracked, it does not resolve from the library's answer,
  and it resolves through the content index, extraction and the
  services as before. Where the content index holds a reference to this
  library for its bytes, that reference answers with the artifact's
  item, and the copy's `record_retrieval` is `library-reference`, not
  `library`

#### Scenario: Outside any library nothing changes
- **WHEN** `borax resolve` runs on a file with no `.borax.toml` above it
  and no `library-root` set
- **THEN** it resolves as before, and its `resolved` event reports the
  library as not attempted with reason `no-library`

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
- **WHEN** an artifact whose history holds the hash of work A's bytes,
  in an entry naming work A's item, was re-linked to work B's item when
  the operator re-identified newer bytes as work B, and work A's bytes
  are restored at its path
- **THEN** the file resolves from work A's item, the item those bytes
  were accepted under

#### Scenario: Bytes recorded before entries named items
- **WHEN** the same happens to an artifact whose entry for work A's
  bytes was written by an earlier release and names no item
- **THEN** the file resolves from work B's item, the item the record
  links to now, which is all its history can say

#### Scenario: A link corrected by hand
- **WHEN** a tracked file holds its record's newest bytes, whose entry
  names one item, and the record's link has since been edited in a
  text editor to another item the library holds
- **THEN** the file resolves from the item the link now names

#### Scenario: A correction holds for the bytes it corrected
- **WHEN** an operator re-identified a tracked file as work B in an
  applying run, its record had earlier linked work A, and later bytes
  have since been accepted, and the corrected bytes are restored at its
  path
- **THEN** the file resolves from work B's item

#### Scenario: A hand correction holds for the bytes it corrected
- **WHEN** a record's link was corrected in a text editor from work A's
  item to work B's while bytes A were its newest, later bytes have
  since been accepted under work B's item, and bytes A are restored at
  its path
- **THEN** the file resolves from work B's item

#### Scenario: Re-identifying restored bytes leaves newer bytes alone
- **WHEN** a record holds bytes A accepted under work X and newer bytes
  B accepted under work Y, bytes A are restored and an operator
  re-identifies them as work X in an applying run, and bytes B are then
  restored
- **THEN** the file resolves from work Y's item

#### Scenario: An answer through an earlier entry names the link too
- **WHEN** a file resolves from the item its bytes were accepted under,
  which is not the record's link
- **THEN** its `library` answer is `tracked` with the link as `item`
  and the item answered with as `accepted_under`, and its
  `record_retrieval` names the item answered with as the item its
  record was retrieved from

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
  library recorded it, and no change to it has been accepted, which
  includes a change pending acceptance;
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
- its link is carried by exactly one item file, but the file's bytes
  are an earlier entry of its history, accepted under another item, and
  that item is not carried by exactly one readable item file
  (`accepted-item-unavailable`), reported with the link, the item the
  bytes were accepted under, and whether that item is missing,
  unreadable (with the file or directory and the reason) or carried by
  several files (with those files);
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

#### Scenario: Restored bytes whose item is gone
- **WHEN** a file's bytes are an earlier entry of its record's history,
  naming an item whose file has since been deleted, while the record
  links an item the library holds
- **THEN** its event reports `accepted-item-unavailable` naming the
  artifact, the record's link as `item`, and the deleted item as the
  item the bytes were accepted under, with the cause `dangling`, and
  the file resolves as untracked

#### Scenario: A damaged link is reported as it always was
- **WHEN** a file's bytes are an earlier entry naming an item the
  library holds, while the record's link names an item whose file has
  been deleted
- **THEN** its event reports a dangling item naming the artifact and the
  link, exactly as for the record's newest bytes

#### Scenario: A pending change is unrecognised content
- **WHEN** `borax reconcile` reported the change at a tracked file's
  path as pending, and `borax resolve` runs on the file
- **THEN** its event reports unrecognised content naming the artifact
  recorded at that path, and the file resolves as untracked

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
Every `resolved` event, and every `skipped` event that is a file's resolution verdict, SHALL carry a `library` section stating what the run's library said about the file or why it was not asked, and a record the library answered with SHALL be reported with `library` as its `record_retrieval`.

`library` SHALL be not attempted when the library was not asked: with
reason `no-library` when the run has no library, `outside-library`
when the file lies outside it or in a subtree it excludes, and
`content-duplicate` when the resolution ended before the library was
asked, as a content duplicate does. A `skipped` event that is not a
resolution verdict — a taken target, a declined move, a failed rename,
a refused move, an unciteable record, a bibliography that could not be
written — SHALL carry no `library` section, since where the file resolved its `resolved` event
already carries what the library said.

Otherwise `library` SHALL have `status` `consulted` and an `answer`
object tagged by `kind`: `tracked` with the `artifact` and `item`
identities; `untracked`; or one of the kinds the requirement "A library
that cannot answer for a file says so" names — `unrecognised-content`,
`ambiguous` and `unhashable` with the `artifacts` recorded at the path,
`no-item` with the `artifact`, `dangling-item` with the `artifact` and
`item`, `unreadable-item` with the `artifact`, the `item`, the `path` of
the item file or item store that could not be read and the `message`
reading it produced, `ambiguous-item` with the `artifact`, the `item`
and the item `files` carrying it, and `unreadable-records` with whether
the artifact store could be `listed` and how many of its files were
`unreadable`. Identities SHALL be written as the canonical UUID text a
record stores.

Every `item` above SHALL name the record's link, as it always has. When
the item answered with is another item, because the file's bytes are an
earlier entry accepted under it, `tracked` SHALL also carry that item
as `accepted_under`. `accepted_under` is absent whenever the item
answered with is the link, so an answer that release 0.9.0 could have
given is written exactly as 0.9.0 wrote it. The new kind
`accepted-item-unavailable` SHALL carry the `artifact`, the link as
`item`, the item the bytes were accepted under as `accepted_under`, and
a `cause`:
- `dangling`, when no item file carries it;
- `unreadable`, with the `path` and `message`, as for `unreadable-item`;
- `ambiguous`, with the item `files` carrying it.

The schema version did not change for these, because every key and kind
here is an addition and no existing one changes meaning. Schema 5 is
"Resolution events carry their evidence in sections"'s, for the content
index.

What the library said SHALL reach every event that reports the file's
resolution verdict, whichever way the run settled the file: a batch
rename's skip, an interactive run's report after its operator's answer,
and a verdict reached by asking the services again after an outage.

`library` reports what the library said, `record_retrieval` where the
record came from, and `lookup` where its identifier came from, and the
three are separate facts: an interactive run whose operator supplied an
identifier for a tracked file reports the operator as its lookup's
origin, the answering service as its `record_retrieval`, and `tracked`
as its library's answer.

A `resolved` event answered by the library SHALL report the content
index's read and write, extraction's result, the titles, the lookup and
the title check as not attempted with reason `library-answered`, since
the file was neither opened nor looked up, and SHALL report the record's
own identifier as its `identifier`.

The human rendering of a `resolved` event whose `record_retrieval` is
`library` SHALL end its retrieval clause with `from the library`, as the
requirement "Human resolution lines name the work, where it came from,
and why a file was skipped" states. The human rendering of a resolution
event whose library answer is one of the kinds the library could not
answer with SHALL be the line it would otherwise have, followed by `;
the library could not answer: ` and a statement of the problem, naming
the artifacts, item and files involved where it has them. A library
answer of `tracked` on a record the operator supplied, `untracked`, and
a library not asked SHALL leave the line as it was.

#### Scenario: A library answer in the stream
- **WHEN** `borax resolve --json` resolves a tracked file whose item's
  provenance attributes its fields to Crossref
- **THEN** its `resolved` event carries schema version 5,
  `record_retrieval` `library` naming the artifact and the item, the
  titles not attempted because the library answered, and a `library`
  section `consulted` whose answer is of kind `tracked` naming the
  artifact and the item

#### Scenario: An item that names no service
- **WHEN** a tracked file's item was written by hand with no per-field
  provenance
- **THEN** its human line names no service and says the record is from
  the library

#### Scenario: A library answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from its library item,
  whose DOI is `10.1039/c5ay00042d`, whose title is `Determination of
  things`, whose one author's family name is `Smith`, which was issued in
  2015, and whose provenance names Crossref
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the library`

#### Scenario: A library problem read by a person
- **WHEN** `paper.pdf` resolves from the content index, by looking up
  the identifier it holds in the response cache, because its record
  links to an item the library does not hold
- **THEN** its line is its ordinary resolution line ending `from the
  response cache`, followed by `; the library could not answer: ` and a
  statement naming the artifact and the missing item

#### Scenario: A rename's skip keeps what the library said
- **WHEN** a batch `borax rename` reaches a file whose record links an
  item the library does not hold, and whose pages hold readable text and
  no identifier
- **THEN** its `skipped` event has reason `text-without-identifier` and
  carries the dangling item in its `library` section

#### Scenario: Asking the services again keeps what the library said
- **WHEN** an interactive run reaches a file whose record links an item
  the library does not hold, whose services were unreachable, and the
  operator asks them again and they answer
- **THEN** the file's `resolved` event carries the dangling item in its
  `library` section

#### Scenario: An operator re-identifies a tracked file
- **WHEN** an interactive run reaches a tracked file and the operator
  supplies a different identifier and renames the file
- **THEN** the file's `resolved` event reports `operator` as its
  lookup's origin and `tracked` as its library's answer

#### Scenario: An answer through the link is written as before
- **WHEN** a tracked file holds its record's newest bytes
- **THEN** its `tracked` answer carries the `artifact` and the link as
  `item`, with no `accepted_under`, and its `record_retrieval` names
  the same artifact and item

#### Scenario: An answer through an earlier entry's item
- **WHEN** a tracked file's bytes are an earlier entry accepted under
  work X's item, while the record links work Y's
- **THEN** its `tracked` answer carries Y's identity as `item` and X's
  as `accepted_under`, and its record is X's item's record

#### Scenario: An earlier entry's item that cannot answer
- **WHEN** that entry's item has since been deleted
- **THEN** its answer is `accepted-item-unavailable` with Y as `item`, X
  as `accepted_under` and the cause `dangling`, and its human line says
  the library could not answer, naming the artifact and both items

### Requirement: A resolution keeps every service it asked, in order
Resolution SHALL retain, for each identifier it looks up, every service it asked about that identifier, in the order it asked them, each with the outcome that service gave.

The outcome SHALL be exactly one of: found; not found; unavailable,
with the message the failure carried; rate limited; or malformed, with
the message the failure carried. No two of these SHALL be retained as
the same outcome, since only "not found" is an answer about the
identifier and the others say the service could not give one.

A service that failed before another service supplied the record SHALL
be retained ahead of the service that supplied it. A record found on a
second attempt is not evidence that the first service was never asked.

The identifier looked up SHALL be retained with its attempts, together
with where that identifier came from: the extraction pass that read it
from the file, an operator who supplied it, or the content index, which
held it for the file's bytes with the origin it was written under. A
lookup of an
identifier that no configured service supports SHALL be retained with
the identifier and no attempts. That keeps a lookup no service could
take apart from one that every service declined.

This applies to every lookup made for a file: the one made from the
file's own identifier, extracted from it or held for it by the content
index; one an operator asks for again after an outage, which keeps the
extraction pass, or the content index, as its origin, whichever the
lookup it repeats had; and one made from an identifier an operator
supplied.

A lookup the operator asks for again is the file's own lookup, and it
SHALL become the file's current lookup, whether it finds a record or
not. Every lookup of the file's own identifier before it SHALL be
retained with it, in the order made, each with its attempts, so that an
outage the operator asked past is not lost. Whether the file's lookup
is conclusive, which decides whether asking again is offered, SHALL be
judged on the current lookup alone. A lookup made from a supplied identifier belongs to the
candidate it reached. It SHALL NOT replace or alter the file's own
lookup, even when no service holds the supplied identifier.

Retaining attempts SHALL NOT change which services are asked or in
what order. The requirement "Sources are queried by identifier type
and priority" fixes both.

#### Scenario: A failure before a success
- **WHEN** a file's DOI is looked up while Crossref is unavailable and
  OpenAlex holds the record
- **THEN** the file resolves from OpenAlex, and its resolution retains
  two attempts in order: Crossref as unavailable with its message,
  then OpenAlex as found

#### Scenario: Ways of failing are told apart
- **WHEN** a file's DOI is looked up while Crossref rate-limits the
  request and OpenAlex answers with a body that cannot be read as a
  record
- **THEN** the file is not resolved, and its resolution retains
  Crossref as rate limited and OpenAlex as malformed with its message,
  and retains neither as not found

#### Scenario: Not found everywhere is conclusive
- **WHEN** a file's DOI is looked up and every service asked says it
  does not hold it
- **THEN** each attempt is retained as not found, and the lookup is
  distinguishable from one in which any service failed to answer

#### Scenario: No service could be asked
- **WHEN** a run whose services are Crossref and OpenAlex looks up an
  arXiv identifier, which neither of them supports
- **THEN** no service is asked, and the resolution retains the arXiv
  identifier and its extraction pass with no attempts

#### Scenario: A supplied identifier's lookup names its origin
- **WHEN** an operator supplies a DOI for a file in an interactive run
  and Crossref holds it
- **THEN** the lookup retained for that record names the operator as
  the origin of the DOI and Crossref as found

#### Scenario: Asking again keeps every round of the file's own lookup
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and every service now says it does not hold the DOI
- **THEN** the file's current lookup is the second one, with every
  attempt as not found, so the lookup is conclusive and asking again is
  not offered; the first lookup, with the outage, is retained before it

#### Scenario: A supplied identifier nobody holds leaves the file's lookup alone
- **WHEN** a file's DOI meets an outage, and the operator then supplies
  another DOI that no service holds
- **THEN** the file's retained lookup is still the one that met the
  outage, and the supplied DOI's attempts are not part of it

#### Scenario: Asking again keeps the file's own origin
- **WHEN** an operator asks the services again about a file's own
  extracted DOI after an outage, and the services now answer
- **THEN** the lookup retained for the record names the extraction pass
  that read the DOI as its origin, not the operator

#### Scenario: Asking again until the services answer
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and Crossref now holds the DOI
- **THEN** the record's current lookup names Crossref as found, and the
  lookup that met the outage is retained before it with its attempts

#### Scenario: Asking again about the content index's identifier
- **WHEN** the DOI the content index holds for a file meets an outage,
  and the operator asks the services again and Crossref answers
- **THEN** the record's current lookup names the content index as the
  DOI's origin, and the lookup that met the outage is retained before it

### Requirement: A resolution says where its record came from
Every record a resolution reaches SHALL be retained with where it was retrieved from: the file's library item, naming the artifact and the item; the item a library reference names, naming the library, its root, the artifact and the item; a service's response cache, naming the service; or the network, naming the service.

A service's answer served from the response cache SHALL be
distinguishable from one the service sent over the network. This
holds for the attempt that found the record as well as for the record.
A record the library answered with involves no service attempt. A
record reached by looking up the identifier the
content index holds was retrieved from the service that answered that
lookup, from its response cache or the network. The content index holds
no record, and so is never where one was retrieved from.

A record a library reference answered with was retrieved from the item
in the library that reference names, and involves no service attempt.
It is distinguishable from a record the run's own library answered
with, which names no root, even where the reference names the run's own
library.

Where a record was retrieved from is a separate fact from which
services its fields came from, which is the record's per-field
provenance. A record retrieved from a cache or from the library keeps
the provenance it was stored with.

#### Scenario: A network answer
- **WHEN** a file's DOI is resolved and Crossref's answer is not in the
  response cache
- **THEN** the record is retained as retrieved from the network from
  Crossref, and so is the attempt that found it

#### Scenario: A response-cache answer
- **WHEN** the same DOI is resolved again for another file, and
  Crossref's earlier answer is held in the response cache
- **THEN** no request is sent, and both the record and the attempt that
  found it are retained as served by Crossref's response cache

#### Scenario: A content-index answer
- **WHEN** a file is resolved from the content index, with no library
  reference for its bytes answering, by looking up the identifier it
  holds, and Crossref's response cache answers
- **THEN** its record is retained as served by Crossref's response
  cache, not as retrieved from the content index, and so is the attempt
  that found it

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** its record is retained as retrieved from the library, naming
  the artifact record and the item, with no service attempt

#### Scenario: A reference answer
- **WHEN** a file outside any library is answered by a library
  reference
- **THEN** its record is retained as retrieved from the item that
  reference names, naming the library, its root, the artifact and the
  item, with no service attempt

#### Scenario: A record an operator reached comes from the service
- **WHEN** the content index answered for a file, or its library
  tracks it, and an operator supplies an identifier that Crossref holds
- **THEN** the record the operator reached is retained as retrieved
  from Crossref, not from the content index or the library, while what
  the content index and the library said is retained as they said it

### Requirement: A resolution says what the content index did
Every resolution of a file SHALL retain what the content index did for it: it answered, through a library reference or with the identification evidence it answered with; it held nothing for the file that answered; it was bypassed because the run turned the cache off; it could not be asked because the file could not be hashed, with the reason hashing gave; or it was not asked because the resolution was already decided, with the reason.

The content index answers through a library reference first, with the
item it names, as "A library reference answers for a file before the
entry's evidence" states. Where no reference answers, it answers by the
identifier it holds, which the resolution then looks up, as "A
content-index entry answers with its identifier" states. It answers
with no record of its own.

Wherever the content index was read and its entry held references, the
resolution SHALL retain every reference read, with what became of it:
answered, agreed, disagreeing, unavailable with the cause, or
item-unavailable with the cause. It retains them whether the entry then
answered through a reference, through its evidence, or not at all.

A file that could not be hashed SHALL be retained as such even when the
run bypassed the cache. The missing hash also decides that nothing can
be remembered for the file.

A resolution SHALL also retain what became of writing the record it
reached to the content index under the file's hash, which is writing
its identification evidence. The write was made; or it failed, with
the store's message; or it was not attempted, with the reason. The
reason is the earliest one in the order the resolution runs:

- the library, the content index, or a content duplicate settled the
  file first;
- extraction found no identifier;
- no service held the identifier;
- the title check refused the record;
- the record waits for an operator to accept it, and is written when
  the file is moved;
- the file has no content hash.

#### Scenario: A hit and a miss
- **WHEN** one file is resolved from the content index's evidence, no
  library reference for its bytes answering, and another, whose hash
  the index does not hold, is extracted and resolved from a service
- **THEN** the first retains the content index as having answered, with
  the identifier it held, and the second retains it as having held
  nothing and retains the write of its record as made

#### Scenario: References that did not answer are retained
- **WHEN** a file's entry holds a reference to a library whose root is
  missing, and an extracted DOI that the lookup resolves
- **THEN** its resolution retains the content index as having answered
  with that DOI, and retains the reference as unavailable because its
  root is missing

#### Scenario: An operator's identifier is not overwritten
- **WHEN** the content index holds an identifier an operator supplied
  for a file, and a run with `--no-cache` extracts and resolves another
- **THEN** its resolution retains the content index as bypassed and the
  write of the extracted identifier as made, and the content index's
  evidence still answers with the operator's identifier

#### Scenario: An unhashable file
- **WHEN** a file cannot be hashed, with the cache on, and its DOI
  resolves from Crossref
- **THEN** its resolution retains the content index as unavailable with
  the hashing error, extraction and the lookup run, and the write is
  retained as not attempted because the file has no content hash

#### Scenario: Unhashable outranks bypassed
- **WHEN** the same file is resolved with `--no-cache`
- **THEN** its resolution still retains the content index as
  unavailable with the hashing error

#### Scenario: The cache bypassed
- **WHEN** a hashable file is resolved with `--no-cache` and its DOI
  resolves
- **THEN** its resolution retains the content index as bypassed, and
  the write of its record as made

### Requirement: A failed cache write is evidence, not a failure
A write to the response cache or to the content index that fails SHALL be retained, with the store's message, as evidence about the resolution or the move it belongs to, and SHALL NOT fail the resolution, change its verdict, fail the rename it followed, or end the run.

Every write borax makes to either store SHALL report whether it was
made. A record a service sent over the network, with a response cache
in front of that service, SHALL be retained with whether it was written
to the response cache. A record sent with no response cache in front,
because the run turned the cache off, SHALL be retained as not written
there.

Remembering a record an operator accepted, when the file is moved,
SHALL yield what became of that write: made; failed, with the store's
message; or not attempted because the file has no content hash. That
result belongs to the move. It is not part of the verdict reported
before the move, which retains the write as waiting for acceptance.

#### Scenario: The response cache cannot be written
- **WHEN** a file's DOI is resolved from Crossref over the network, and
  the response cache's directory cannot be created
- **THEN** the file resolves from Crossref, and the attempt that found
  the record retains the failed write with its message

#### Scenario: The content index cannot be written
- **WHEN** a file is resolved from a service and the content index
  cannot be written
- **THEN** the file resolves exactly as it would otherwise have, and
  its resolution retains the content-index write as failed with the
  store's message

#### Scenario: An accepted answer that could not be kept
- **WHEN** a record an operator accepted is remembered when the file is
  moved, and the content index cannot be written
- **THEN** remembering yields a failed write with the store's message,
  and the move stands

#### Scenario: No response cache in front
- **WHEN** a file's DOI is resolved from Crossref with `--no-cache`
- **THEN** the attempt that found the record retains that the response
  cache was not written

### Requirement: A refused record is kept as evidence and never remembered
When the title check refuses a record, resolution SHALL retain that record as the file's candidate, together with the evidence it was reached by and the conflict that refused it.

The candidate's evidence is everything a resolved record's would be:

- the identifier looked up and where it came from;
- every service attempt, in order;
- where the record was retrieved from;
- what extraction found;
- the titles the file claims.

A refused record SHALL NOT be written to the content index under the
file's content hash, and nothing the content index already held for
that hash SHALL change. A later run therefore extracts and checks the
file again rather than being answered for it from the index. The
response cache keeps the service's answer under the identifier, as it
keeps every answer. That entry says what the service holds and nothing
about the file.

When an operator accepts the refused record, the record's
content-index write SHALL be retained as waiting for the move, no
longer as refused. The title check's conflict SHALL still be retained
beside the acceptance. Accepting the record is the decision the
conflict was put to the operator for, and it does not change what the
check concluded.

Retaining a candidate SHALL NOT offer the candidate to anyone, search
for other records, or accept it. Accepting a record over a conflict
remains an operator's decision, as the requirement "Ambiguity is
skipped, never guessed" states.

#### Scenario: A conflict keeps its candidate
- **WHEN** a batch run resolves a file whose DOI is read from its text
  layer and answered by Crossref over the network, and whose embedded
  title disagrees with the record's
- **THEN** the file is skipped as a conflict, and the refused record is
  retained with the DOI and its text-layer origin, Crossref's attempt
  as found over the network, the extraction result, the file's titles,
  and the conflict with its similarity

#### Scenario: A refused record is not remembered
- **WHEN** the same file is skipped as a conflict
- **THEN** the content index holds no record for the file's hash, the
  refused record retains the content-index write as not attempted
  because it was refused, and resolving the file again checks its
  titles again and skips it again

#### Scenario: An accepted conflict waits for the move
- **WHEN** an operator renames a file over its own title conflict
- **THEN** the accepted record retains the conflict, retains the
  content-index write as waiting for the move rather than as refused,
  and the record is written to the content index only once the file
  has moved

#### Scenario: An earlier entry survives a refusal
- **WHEN** the content index holds a record for a file's hash, and a
  run with `--no-cache` resolves the file to a different record that
  the title check refuses
- **THEN** the content index still holds the earlier record for that
  hash

### Requirement: A resolution says what the title check concluded
Every resolution SHALL retain what the title check concluded about the record it reached: the file's titles agreed with it; they conflicted, with the field, both titles and their similarity; there was not enough evidence to judge, with why; or the check was not made, with the reason.

There SHALL be three reasons for too little evidence, kept apart: the
file claims no title; none of the titles it claims counts as evidence;
or the record has no title to compare. Agreement and too little
evidence SHALL NOT be retained as the same conclusion. Neither refuses
a record, but only agreement is evidence that the record is the
file's.

The rules for which titles count as evidence and for when two titles
agree are those of the requirement "Ambiguity is skipped, never
guessed", and they are unchanged.

#### Scenario: Agreement
- **WHEN** a file's XMP title matches the title of the record its DOI
  resolves to
- **THEN** the resolution retains the title check as agreed

#### Scenario: A placeholder is not agreement
- **WHEN** the only title a file claims is a producer's placeholder,
  and its DOI resolves to a titled record
- **THEN** the file resolves, and the resolution retains the title
  check as having too little evidence because none of the file's titles
  counts, not as agreed

#### Scenario: A file that claims no title
- **WHEN** a file whose metadata carries no title resolves from its DOI
- **THEN** the resolution retains the title check as having too little
  evidence because the file claims no title

#### Scenario: A record with no title
- **WHEN** a file claiming a title resolves to a record that carries no
  title
- **THEN** the resolution retains the title check as having too little
  evidence because the record has no title

#### Scenario: The check was not made
- **WHEN** a file is resolved from the content index
- **THEN** the resolution retains the title check as not made because
  the content index answered

### Requirement: A resolution says why a step was not taken
For every step a resolution did not take, the resolution SHALL retain that the step was not taken and why, and SHALL NOT retain it as a step that ran and found nothing.

The steps are:

- asking the run's library;
- asking the content index;
- reading the file's titles;
- extraction;
- taking an operator's identifier input;
- the lookup;
- the title check;
- writing the record to the content index;
- and, for each identifier an operator submitted, its lookup, its title
  check, and the operator's acceptance of what it reached.

The reasons are:

- the run has no library;
- the file lies outside the run's library or in a subtree it excludes;
- the file's bytes duplicate an artifact the library holds;
- the library answered;
- the content index answered, through a library reference or with the
  identifier it holds;
- extraction found no identifier;
- no service held the identifier, or none could be asked;
- no question was put to an operator about the file;
- a question was put and no identifier was submitted;
- the text submitted named no identifier;
- the record a submission reached led to no move, so it was never put
  to the operator;
- and, for the content-index write alone, those the requirement "A
  resolution says what the content index did" names.

A lookup that was never made is not one that found nothing. Titles
that were never read are not a file claiming none.

Evidence SHALL be retained for every verdict a resolution reaches, a
resolved record and a skip alike. A skip that is a file's resolution
verdict retains the evidence of every step that ran before it.

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** the content index, the titles, extraction, the lookup, the
  title check and the content-index write are each retained as not
  taken because the library answered

#### Scenario: A content-index answer
- **WHEN** a file is resolved from the content index's evidence, no
  library reference for its bytes answering
- **THEN** the titles, extraction, the title check and the
  content-index write are each retained as not taken because the
  content index answered, and the lookup of the identifier it held is
  retained as made

#### Scenario: A reference answer
- **WHEN** a file is answered by a library reference
- **THEN** the titles, extraction, the lookup, the title check and the
  content-index write are each retained as not taken because the
  content index answered, and the operator's identifier input as not
  taken because no question was put

#### Scenario: Extraction found no identifier
- **WHEN** a file carrying a title yields no identifier
- **THEN** it is skipped, its titles are retained as read, and the
  lookup, the title check and the content-index write are retained as
  not taken because extraction found no identifier

#### Scenario: No service held the identifier
- **WHEN** a file's DOI is held by no service
- **THEN** it is skipped with every attempt retained, and the title
  check and the content-index write are retained as not taken because
  no service held the identifier

#### Scenario: No library, and a file outside it
- **WHEN** one file is resolved by a run with no library, and another by
  a run whose library does not contain it
- **THEN** the first retains the library as not asked because the run
  has none, and the second as not asked because the file lies outside
  it

#### Scenario: A content duplicate
- **WHEN** a rename run reaches a file whose bytes an artifact of its
  library already holds
- **THEN** it is skipped as a content duplicate, and the library, the
  content index, the titles, extraction, the operator's identifier
  input, the lookup, the title check and the content-index write are
  each retained as not taken because the file duplicates content the
  library holds

#### Scenario: A submission that names nothing
- **WHEN** the text an operator submits names no identifier
- **THEN** that submission's lookup, title check and acceptance are
  retained as not taken because the text named no identifier

### Requirement: Resolution events carry their evidence in sections
Every `resolved` event, and every `skipped` event that is a file's resolution verdict, SHALL carry the evidence of the file's resolution as eight sections, in the order the resolution runs: `library`, `content_index`, `extraction`, `identifier_input`, `lookup`, `record_retrieval`, `match_check` and `acceptance`.

Each section that is one step, and each of the two steps `content_index`
and `extraction` hold under their own keys (`read` and `write`, `result`
and `titles`), SHALL be an object tagged by `status`, naming what the
step concluded. `record_retrieval` names a place rather than a step: it
SHALL be tagged by `kind`, and SHALL be `null` on a verdict that reached
no record.

A step the resolution did not take SHALL be reported as exactly
`{"status": "not-attempted", "reason": <reason>}`, where the reason is
the kebab-case name of why it was not taken: `no-library`,
`outside-library`, `content-duplicate`, `library-answered`,
`content-index-hit`, `extraction-failed`, `no-record`, `refused`,
`unhashable`, `awaiting-acceptance`, `cache-bypassed`, `not-asked`,
`not-supplied`, `unparsed`, or `no-move`. A step not taken SHALL NOT be
left out, and SHALL NOT be reported as a step that ran and found
nothing.

The event schema version SHALL be 5. A `resolved` event SHALL NOT carry
the schema-3 top-level fields `found`, `cached`, `source`, `tier`,
`claims` or `overrode`, and the `reason` of a resolution `skipped` event
SHALL NOT carry the schema-3 reason fields `found`, `tier`, `attempts`,
`field`, `extracted`, `resolved` or `similarity`, whether under those
names or under other names at those places. A name schema 4 uses inside
a section — `found` as a status, `tier` in extraction's result, `claims`
in the titles — is schema 4's own and is not one of these. No event
SHALL carry a schema-3 rendering of a fact beside its schema-4 one. A
`resolved` event keeps `path`, `identifier`, the record's preferred
identifier, and `record`, the whole record.

The `identifier_input` section, the `earlier` rounds of a lookup, and
the `acceptance` value `pending` are additions to schema 4. A consumer
that ignores what it does not know reads them unchanged.

One change is not an addition. A record reached from a supplied
identifier and renamed from with no conflict is `accepted`, so
`automatic` no longer covers it, which narrows that value's meaning.
No release had carried schema 4 when it was made. The requirement "JSON
Lines output is first-class" lets a schema that no release has carried
change without a further bump. So none of these changed the version,
and it remained 4.

Schema 4 was then released, in 0.9.0, and the content index stopped
holding records, which changes what released values mean:
`content_index.read` `hit` is an identifier the index answered with,
which the resolution then looks up, rather than a record served — where
no library reference answers, as the paragraph on references below
states;
`content_index.write` writes identification evidence rather than a
record; `record_retrieval` `content-index` is no longer emitted;
`content-index-write` reports every evidence write made after a
verdict — the move's, for an operator's record or a retried lookup of
the file's own identifier, and `borax reconcile`'s restorations;
and `library-adoption` `unindexed` means no entry rather than
no record. A consumer reading schema 4 correctly could read these
wrongly, so the version SHALL be 5. No release has carried schema 5, so
a later change SHALL be free to change it without a further bump until
one does.

Library references were then added to schema 5, before any release
carried it. These are additions:

- `references` on `content_index.read` `hit` and `miss`;
- `record_retrieval` `library-reference`;
- `content-index-write`'s entry kind `reference`, with `repaired_from`;
- `references` on `library-adoption` `unanswered`.

These meanings schema 5 had given changed with them:

- a `hit` answered by a reference carries no `evidence`, and no lookup
  follows it, so its `lookup` is not attempted with reason
  `content-index-hit`;
- `content-index-write` reports reference writes too, by an applying
  run, batch runs included, by `borax adopt`, and by `borax reconcile`;
- `library-adoption` `unanswered` carries no `identifier` where the
  entry held no evidence;
- `library-adoption` `unindexed` narrows: it means the entry holds
  neither evidence that answers nor a reference that can be read, since
  an entry holding only references is `recorded` through one that
  answers, or `unanswered`;
- `library-adoption` `recorded` may carry `retrieval`
  `library-reference`, where it was always a service's response cache;
- a `miss` means that neither a reference nor the evidence answered,
  and may carry the `references` that did not;
- `record_retrieval` after a `hit` may be `library-reference`, where it
  was always the lookup's service;
- `borax adopt` writes to the content index.

A line about an entry that holds no reference is written exactly as
before, since `references` is left out when there are none. The
version SHALL stay 5, as the requirement "JSON Lines output is
first-class" allows for a schema no release has carried.

#### Scenario: A content-index answer in sections
- **WHEN** a run with no library resolves a file from the content
  index's evidence, no library reference for its bytes answering, and
  Crossref's response cache answers for its identifier
- **THEN** its `resolved` event carries schema version 5; `library` not
  attempted with reason `no-library`; `content_index` with `read` `hit`
  carrying that identifier and its origin as `evidence`, and `write`
  not attempted with reason `content-index-hit`; `extraction`'s
  `result` and `titles` and `match_check` each not attempted with
  reason `content-index-hit`; `identifier_input` not attempted with
  reason `not-asked`; `lookup` attempted with `origin`
  `content-index`; `record_retrieval` of kind `service-cache` naming
  Crossref; and `acceptance` `automatic`

#### Scenario: A reference answer in sections
- **WHEN** a run with no library resolves a file through a library
  reference
- **THEN** its `resolved` event carries `content_index` with `read`
  `hit` carrying `references` and no `evidence`, and `write` not
  attempted with reason `content-index-hit`; `extraction`'s `result`
  and `titles`, `lookup` and `match_check` each not attempted with
  reason `content-index-hit`; `record_retrieval` of kind
  `library-reference`; and `acceptance` `automatic`

#### Scenario: The version is five
- **WHEN** any run emits an event
- **THEN** it carries schema version 5

#### Scenario: No schema-3 field survives
- **WHEN** any run emits a `resolved` event or a resolution `skipped`
  event
- **THEN** the `resolved` event carries none of the keys `found`,
  `cached`, `source`, `tier`, `claims` or `overrode` at its top level,
  and the `skipped` event's `reason` carries none of the keys `found`,
  `tier`, `attempts`, `field`, `extracted`, `resolved` or `similarity`

#### Scenario: A resolution skip carries every section
- **WHEN** a file is skipped because no service holds its identifier
- **THEN** its `skipped` event carries all eight sections, with
  `record_retrieval` `null`

#### Scenario: The sections in pipeline order
- **WHEN** any run emits a `resolved` event
- **THEN** its line carries `identifier_input` after `extraction` and
  before `lookup`

### Requirement: A resolution reports what the content index did and every cache write
The `content_index` section SHALL report the content index's `read` as `hit`, `miss`, `bypassed`, `unavailable` with the hashing error's `message`, or not attempted, and its `write` of the record reached as `written`, `failed` with the store's `message`, or not attempted with the earliest reason the requirement "A resolution says what the content index did" orders.

A `hit` answered by the entry's evidence SHALL carry `evidence`: the
`identifier` the entry answered with and its `origin`, `extracted`,
`supplied` or `overridden`. A `hit` answered by a library reference
SHALL carry no `evidence`. An entry holding nothing that answers is
reported `miss`, as "The content index keeps identification evidence,
one fact per file" states.

A `hit` or a `miss` whose entry held at least one reference that could
be read SHALL carry `references`, an array in entry order. Each element
has the reference's `library`, `root`, `artifact` and `item`, and a
`status`:

- `answered`;
- `agreed`;
- `disagreeing`, with the `identifier` its item carries, left out when
  it carries none;
- `unavailable`, with a `cause`:
  - `missing`;
  - `other-library`, with the identity `found`, left out when the root
    holds none;
  - `unreadable`, with `path` and `message`;
- `item-unavailable`, with a `cause`:
  - `dangling`;
  - `unreadable`, with `path` and `message`;
  - `ambiguous`, with the item `files`.

`references` SHALL be left out where the entry held none, so such a
line is written as it was before references existed.

The `write` is of the extracted identifier the record was reached by,
into the entry's extracted evidence. It is made whatever operator
evidence the entry holds, which it never touches.

A file that could not be hashed SHALL be reported `unavailable` even
when the run bypassed the cache.

A service attempt that found the record over the network SHALL carry
`stored`, the result of writing that answer to the response cache:
`written`, `failed` with the store's `message`, or not attempted with
reason `cache-bypassed` when the run turned the cache off and no
response cache stood in front of the service. An attempt answered from
the response cache SHALL carry no `stored`, since nothing was fetched to
be written.

A write that failed SHALL be reported only in the section of the store
written. It SHALL NOT be reported as a skip or a finding, SHALL NOT
change the verdict or the run's counts, and SHALL NOT change the exit
status.

#### Scenario: A miss, and the record written
- **WHEN** a file whose hash the content index does not hold resolves
  from Crossref
- **THEN** its `content_index` reports `read` `miss` and `write`
  `written`

#### Scenario: An unhashable file
- **WHEN** a file that cannot be hashed resolves from Crossref, with the
  cache on and again with `--no-cache`
- **THEN** both events report `read` `unavailable` with the hashing
  error, and `write` not attempted with reason `unhashable`

#### Scenario: The content index cannot be written
- **WHEN** a file resolves from a service and the content index cannot
  be written
- **THEN** the file is reported resolved, its `write` is `failed` with
  the store's message, and the run's counts and exit status are what
  they would have been had the write succeeded

#### Scenario: The response cache cannot be written
- **WHEN** a file's DOI is answered by Crossref over the network and
  the response cache cannot be written
- **THEN** Crossref's attempt is `found` with `retrieval` `network` and
  `stored` `failed` with the store's message

#### Scenario: No response cache in front
- **WHEN** a file's DOI is answered by Crossref under `--no-cache`
- **THEN** `content_index.read` is `bypassed`, and Crossref's attempt
  carries `stored` not attempted with reason `cache-bypassed`

#### Scenario: A hit names the evidence it answered with
- **WHEN** a file's entry holds an extracted DOI and a DOI of origin
  `supplied`, and no reference that answers
- **THEN** `content_index.read` is `hit` with `evidence` naming the
  supplied DOI and `supplied`

#### Scenario: A hit through a reference
- **WHEN** a file's entry holds a reachable reference and an extracted
  DOI
- **THEN** `content_index.read` is `hit` with `references` naming that
  reference `answered`, and carries no `evidence`

#### Scenario: A miss beside a reference that could not answer
- **WHEN** a file's entry holds only a reference whose root is missing
- **THEN** `content_index.read` is `miss` with `references` naming it
  `unavailable` with cause `missing`, and the file is extracted

### Requirement: Each service asked is reported with its outcome
The `lookup` section SHALL report the identifier the resolution looked up, its `origin` as `extracted`, `operator` or `content-index`, and every service asked about it, in the order asked, each as an object with the `service` and an `outcome` tagged `found`, `not-found`, `unavailable` with its `message`, `rate-limited`, or `malformed` with its `message`.

A `found` outcome SHALL carry `retrieval`: `service-cache` when the
service's response cache answered and no request was sent, or `network`
when the service answered over the network. A service that failed
before another found the record SHALL be reported ahead of it, and the
found attempt SHALL be the last.

A lookup in which at least one service was asked SHALL have `status`
`attempted`. A lookup of an identifier no configured service supports
SHALL have `status` `no-eligible-service`, with the identifier and its
origin and no attempts. A resolution that looked nothing up SHALL report
`lookup` not attempted, with the reason.

`origin` `extracted` SHALL be reported for the file's own identifier,
including when an operator asked for its lookup to be made again;
`operator` for an identifier an operator supplied in this run; and
`content-index` for the identifier the content index held for the
file's bytes, including when an operator asked for its lookup to be
made again. The pass that read an extracted identifier is
`extraction`'s result's `tier`, and SHALL NOT be repeated in `lookup`.
The origin the content index's identifier was written under is the
`content_index` section's `evidence`, and SHALL NOT be repeated in
`lookup` either.

A lookup of the file's own identifier that an operator asked for again
SHALL report the lookups before it as `earlier`. That is an ordered
list, oldest first, of rounds, each `attempted` with its `attempts` or
`no-eligible-service`. A round SHALL NOT repeat the identifier or the
origin, which are the lookup's own. The lookup's `status`, its
`attempts` and its outcome are the current round's. `earlier` SHALL be
absent from a lookup made once, and from every lookup of an identifier
an operator supplied, and its absence is not a step that did not run.
The human lines and the interactive description SHALL continue to show
the current round only.

#### Scenario: A failure before a success
- **WHEN** a file's DOI is looked up while Crossref is unavailable and
  OpenAlex answers over the network
- **THEN** `lookup.attempts` holds Crossref as `unavailable` with its
  message, then OpenAlex as `found` with `retrieval` `network`, and
  `record_retrieval` is `network` naming OpenAlex

#### Scenario: Not found everywhere
- **WHEN** every service asked about a file's DOI says it does not hold
  it
- **THEN** the file's `skipped` event has reason `unresolvable` and
  every attempt's outcome is `not-found`

#### Scenario: Ways of failing are told apart
- **WHEN** Crossref rate-limits the request and OpenAlex answers with a
  body that is not a record
- **THEN** the attempts are `rate-limited` and `malformed` with its
  message, in that order, and neither is `not-found`

#### Scenario: No service could be asked
- **WHEN** a run whose services are Crossref and OpenAlex reaches a file
  carrying only an arXiv identifier
- **THEN** its `lookup` has `status` `no-eligible-service`, the arXiv
  identifier and `origin` `extracted`, and no attempts

#### Scenario: A supplied identifier's origin
- **WHEN** an operator supplies a DOI for a file and renames it from the
  record Crossref holds
- **THEN** the file's `resolved` event reports `lookup.origin`
  `operator`

#### Scenario: An outage, a retry, and an answer
- **WHEN** Crossref and OpenAlex are both unavailable for a file's DOI,
  the operator asks the services again, and OpenAlex now answers over
  the network
- **THEN** the file's `resolved` event's `lookup.attempts` holds the
  second round, ending with OpenAlex as `found`, and `lookup.earlier`
  holds one `attempted` round with both services as `unavailable` and
  their messages

#### Scenario: An outage, a retry, and not found everywhere
- **WHEN** Crossref is unavailable for a file's DOI, the operator asks
  the services again, and every service now says it does not hold it
- **THEN** the file's `skipped` event has reason `unresolvable`,
  `lookup.attempts` every outcome `not-found`, and `lookup.earlier` one
  round with Crossref as `unavailable`; the question put again offers
  no retry, and the human line names only the second round's answers

#### Scenario: A lookup made once has no history
- **WHEN** a batch run looks up a file's DOI
- **THEN** its `lookup` carries no `earlier`

#### Scenario: The content index's identifier
- **WHEN** a batch run looks up the DOI the content index holds for a
  file, which an operator supplied in an earlier run
- **THEN** its `lookup.origin` is `content-index`, not `operator`, its
  `identifier_input` is not attempted with reason `not-asked`, and its
  `content_index.read.evidence.origin` is `supplied`

### Requirement: A resolution reports where its record was retrieved
The `record_retrieval` section SHALL name where the record a resolution reached was retrieved from: `library` with the `artifact` and `item` identities, `library-reference` with the `library`, `root`, `artifact` and `item` the reference names, `service-cache` with the `service`, or `network` with the `service`.

The `library` retrieval's `item` SHALL name the item whose record was
returned, which is what this section has always attributed the record
to. For a file whose bytes are an earlier entry accepted under another
item, that is the item it was accepted under, not the record's link.
The link is reported in the `library` section. No key was added for
that, and the field kept its meaning, so the schema version did not
change for it.

The content index holds no record, so `content-index` SHALL NOT be
reported as a retrieval. A record reached
by looking up the identifier the content index holds SHALL be reported
as retrieved from the service that answered, from its response cache
or the network.

A record a library reference answered with SHALL be reported as
`library-reference`, never as `library`, which names the run's own
library's answer for a file it tracks. That holds when the reference
names the run's own library.

On a conflict skip it SHALL name where the refused candidate was
retrieved from. A record an operator reached by a lookup SHALL be
reported as retrieved from the service, even where the library or the
content index had answered for the file first.

Where the record was retrieved from SHALL be reported apart from which
services its fields came from, which is the record's own per-field
provenance, carried in `record`. `record_retrieval` SHALL NOT name the
services the fields came from, and nothing SHALL report a store as
though it were a service.

#### Scenario: A network answer and a response-cache answer
- **WHEN** one file's DOI is answered by Crossref over the network, and
  a second file carrying the same DOI is resolved later in the run
- **THEN** the first event's `record_retrieval` is `network` naming
  Crossref and the second's is `service-cache` naming Crossref, and no
  request is sent for the second

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** `record_retrieval` is `library` naming the artifact and the
  item

#### Scenario: A content-index answer
- **WHEN** a file outside any library is resolved by looking up the DOI
  the content index holds for it, no library reference for its bytes
  answering, and Crossref answers over the network
- **THEN** `record_retrieval` is `network` naming Crossref

#### Scenario: A reference answer
- **WHEN** a file outside any library is answered by a reference to a
  library at `/home/u/papers`
- **THEN** `record_retrieval` is `library-reference` naming that
  library's identity, `/home/u/papers`, the artifact and the item

#### Scenario: An operator's lookup over a tracked file
- **WHEN** an operator supplies an identifier for a tracked file and
  renames it from the record Crossref holds
- **THEN** its `record_retrieval` names Crossref, and its `library`
  section still reports the library's answer as `tracked`

#### Scenario: A record retrieved through an earlier entry
- **WHEN** a tracked file's bytes are an earlier entry accepted under
  work X's item, while the record links work Y's
- **THEN** `record_retrieval` is `library` naming the artifact and X as
  `item`, while the `library` section's `tracked` answer names Y as
  `item` and X as `accepted_under`

### Requirement: A resolution reports its title check and whether its record was accepted
The `match_check` section SHALL report the title check's conclusion as `agreed`, `conflict` with the `field`, the `extracted` and `resolved` values and their `similarity`, `insufficient-evidence` with a `reason` of `record-untitled`, `no-titles` or `no-evidence`, or not attempted, and the `acceptance` section SHALL report `automatic`, `overridden`, `accepted`, `pending` or `not-applicable`.

`acceptance` SHALL be `overridden` exactly when an operator accepted a
record over a conflict the title check concluded, whether the record
was the file's own or one reached from an identifier they supplied. The
conflict stays in `match_check` on the same event, and `acceptance`
SHALL carry nothing beside its status. It SHALL be:

- `accepted` when an operator accepted a record reached from an
  identifier they supplied, and no conflict was overridden to use it;
- `pending` while such a record is on offer and the operator has not
  answered, whatever its title check concluded;
- `automatic` when any other record was used and no conflict was
  overridden to use it;
- `not-applicable` on every resolution skip, a conflict skip included.

A file's reported verdict SHALL NOT carry `pending`. An interactive run
reports a file only once an answer has settled the record on offer.
A rename the run meets with a notice that the library already holds a
file of the record's work settles nothing, and the record stays
`pending`. `pending` is carried by the event a
question's description is rendered from. What the operator decided
about a record that did not become the file's verdict is reported on
the submission that reached it, and never in this section.

Agreement and too little evidence SHALL NOT be reported alike.

#### Scenario: A placeholder is not agreement
- **WHEN** the only title a file claims is a producer's placeholder and
  its DOI resolves to a titled record
- **THEN** the file resolves, and its `match_check` is
  `insufficient-evidence` with reason `no-evidence`

#### Scenario: An operator overrides a conflict
- **WHEN** an operator renames a file over its own title conflict
- **THEN** its `resolved` event carries `match_check` `conflict` with
  the field, both titles and the similarity, and `acceptance`
  `overridden`

#### Scenario: A batch run's conflict
- **WHEN** a batch run skips a file as a conflict
- **THEN** its `skipped` event carries `match_check` `conflict` and
  `acceptance` `not-applicable`

#### Scenario: A supplied record renamed
- **WHEN** an operator supplies a DOI whose record's title agrees with
  the file's, and renames the file from that record
- **THEN** its `resolved` event carries `match_check` `agreed` and
  `acceptance` `accepted`

#### Scenario: A supplied record renamed over its conflict
- **WHEN** an operator supplies a DOI whose record's title conflicts
  with the file's, and renames the file from that record anyway
- **THEN** its `resolved` event carries `match_check` `conflict` and
  `acceptance` `overridden`

#### Scenario: A supplied record awaiting an answer
- **WHEN** a record reached from a supplied DOI, whose title agrees with
  the file's, is described before the operator answers
- **THEN** the event the description is rendered from carries
  `acceptance` `pending`

#### Scenario: A retried record renamed
- **WHEN** an operator asks the services again after an outage, and
  renames the file from the record its own DOI now reaches, whose title
  agrees with the file's
- **THEN** its `resolved` event carries `acceptance` `automatic`

#### Scenario: A retried record renamed over its conflict
- **WHEN** an operator asks the services again after an outage, the
  record its own DOI now reaches has a title that conflicts with the
  file's, and the operator renames the file anyway
- **THEN** its `resolved` event carries `match_check` `conflict`,
  `acceptance` `overridden`, and `lookup.origin` `extracted`

#### Scenario: A collision notice keeps a supplied record pending
- **WHEN** an operator answers rename on a record a supplied DOI
  reached, and the run says the library already holds a file of its
  work
- **THEN** the event the next question's description is rendered from
  still carries `acceptance` `pending`

### Requirement: A resolution skip names its cause and states each fact once
A `skipped` event that is a file's resolution verdict SHALL carry a `reason` whose `kind` is one of `no-text-layer`, `text-without-identifier`, `encrypted`, `unreadable`, `unresolvable`, `conflict` or `duplicate`, and SHALL carry no field in its reason other than the reader's `message` on `unreadable` and, on `duplicate`, the duplicate's `reason` and `existing_path` as before.

What the schema-3 reasons carried SHALL be reported in the sections
instead: the identifier looked up, its origin and the services' answers
in `lookup`; the pass that read the identifier in `extraction`; the
conflict's field, both values and similarity in `match_check`.

A skip of kind `conflict` SHALL carry the record the title check refused
as `candidate`, in full, as a `resolved` event carries its `record`. No
other skip SHALL carry a `candidate`.

A duplicate, by content or by work, is a resolution verdict: the
library's duplicate checks reach it while the file is being resolved,
and no later step produces one. A content duplicate is reached before
any step runs, and SHALL report every section not attempted with
reason `content-duplicate`, with `record_retrieval` `null`. A work
duplicate is reached once the file has resolved to a record, and SHALL
report the evidence of the resolution that reached that record —
`library` included — with `record_retrieval` naming where that record
came from. Both SHALL report `acceptance` `not-applicable`, and neither
SHALL carry a `candidate`.

Every other skip is not a resolution verdict: a taken target, a name
that renders empty, a declined move, a failed rename, a bibliography
that could not be written, a record that renders no citation key, a
taken sidecar, an unrecordable move, and a stranding move. Such a skip
SHALL carry its `path` and its `reason`, spelled as before, and no
section.

#### Scenario: An unresolvable skip
- **WHEN** a batch run skips a file because no service holds the DOI in
  its text layer
- **THEN** the `skipped` event's reason is `{"kind": "unresolvable"}`,
  and its `lookup` carries the DOI, `origin` `extracted` and each
  service's answer, and its `extraction` result carries `text-layer` as
  the tier

#### Scenario: A conflict skip keeps its candidate
- **WHEN** a batch run skips a file whose embedded title disagrees with
  the record Crossref holds for its DOI
- **THEN** the `skipped` event's reason is `{"kind": "conflict"}`, its
  `candidate` is that record, its `match_check` carries the conflict,
  its `content_index.write` is not attempted with reason `refused`, and
  its `record_retrieval` names Crossref

#### Scenario: A taken target carries no sections
- **WHEN** a batch rename skips a resolved file because its target is
  taken
- **THEN** the `skipped` event carries its path and the reason
  `target-taken` with the target, and none of the sections

#### Scenario: A content duplicate carries its sections
- **WHEN** a rename run skips a file whose bytes an artifact of its
  library already holds
- **THEN** the `skipped` event carries the `duplicate` reason of kind
  `content` with the existing path, every section not attempted with
  reason `content-duplicate`, `record_retrieval` `null` and
  `acceptance` `not-applicable`

#### Scenario: A work duplicate carries its record's evidence
- **WHEN** a batch rename skips a file whose DOI, answered by Crossref,
  names a work the library already holds a file of, and the library has
  no record of this file
- **THEN** the `skipped` event carries the `duplicate` reason of kind
  `work` with the existing path, a `library` section whose answer is
  `untracked`, a `lookup` with Crossref's attempt as `found`,
  `record_retrieval` naming Crossref, and `acceptance`
  `not-applicable`, and no `candidate`

### Requirement: What became of remembering an operator's answer is reported
When an interactive run writes a record to the content index on the move it followed, or `borax reconcile` writes an operator's identifier to the content index from a library's history, it SHALL report what became of each write in a `content-index-write` event carrying the file's `path`, its content `hash`, the `entry` written, and a `write` of `written`, or `failed` with the store's `message`.

The `entry` SHALL be tagged by `kind`: `evidence`, with the `identifier`
and its `origin`. At the move that is `supplied` or `overridden`, put
in the entry's operator evidence, or `extracted`, put in its extracted
evidence, for the file's own identifier found again by a lookup the
operator asked for after an outage. A lookup of the identifier the
content index already held writes nothing at the move. A restoration is written as the
`library` capability's requirement "A library's operator decisions are
restored to the content index" states.

The event SHALL follow the file's `renamed` event, its
`library-admission` event where there is one, and the event reporting
its library reference's write where there is one, and SHALL precede
anything else reported about the file. In `borax reconcile`, which reports
after its pass, the events SHALL follow every per-object event of the
run and precede its totals. The `path` is the file after the move, or
the full path the reconciled record names, whether or not a file is
there. It SHALL be written to the stream and the
run log alike, as every event after a move is, and SHALL NOT be recorded
before the move.

The `resolved` event before the move SHALL report the write as not
attempted with reason `awaiting-acceptance`. Where no write is made — a
batch run, whose resolution wrote its record's evidence as it resolved;
a move declined, skipped or never reached; a reconcile that finds the
content index already holding the library's operator identifier — no
`content-index-write` event of kind `evidence` SHALL be emitted for it.
A reference written for the same file is reported in an event of its
own, as "What became of a library reference write is reported" states.

The event SHALL NOT be counted in `run-finished`, and a `failed` write
SHALL NOT change the exit status or be reported as a failure of the
rename or the reconcile.

#### Scenario: A supplied identifier remembered
- **WHEN** an operator supplies a DOI for a file outside any library
  and renames it from the record it reaches
- **THEN** the stream carries the file's `resolved` event with
  `content_index.write` not attempted for `awaiting-acceptance`, its
  `renamed` event, and then a `content-index-write` event naming the
  file's new path and its hash, with an `entry` of kind `evidence`
  naming the DOI and `supplied`, and `write` `written`, and the run log
  carries the same three

#### Scenario: The answer could not be kept
- **WHEN** the same rename is made and the content index cannot be
  written
- **THEN** the `content-index-write` event's `write` is `failed` with
  the store's message, the rename is reported as made, and the run's
  counts and exit status are unchanged by the failure

#### Scenario: A batch run writes as it resolves
- **WHEN** `borax rename --apply` renames a file it resolved from a
  service
- **THEN** no `content-index-write` event of kind `evidence` is emitted,
  and the file's `resolved` event reports `content_index.write`
  `written`; inside a library with an identity, the reference the move
  recorded is reported in a `content-index-write` event of kind
  `reference`

#### Scenario: A retried lookup remembered
- **WHEN** a file's own DOI meets an outage, the operator asks the
  services again, Crossref answers, and the operator renames the file
- **THEN** the `renamed` event is followed by a `content-index-write`
  event whose `entry` is `evidence` naming the DOI and `extracted`, the
  entry's extracted evidence holds the DOI, and its operator evidence is
  absent

#### Scenario: A restoration reported
- **WHEN** `borax reconcile` writes to the content index the identifier
  an operator supplied for a record's bytes, which the index lacked
- **THEN** a `content-index-write` event names the path the record
  names, the hash, an `entry` of kind `evidence` naming the identifier
  and `supplied`, and `write` `written`, before the reconcile's totals

### Requirement: Human resolution lines name the work, where it came from, and why a file was skipped
The human rendering of a `resolved` event SHALL be `<path>: resolved <identifier>`, then ` to ` and the record's title in double quotes followed by its authors and year of issue in parentheses, then ` via ` and the services that supplied the record, then `, from ` and where the record was retrieved, with each part the record does not hold left out.

The authors SHALL be named by family name: one alone, two joined by
` and `, and three or more as the first followed by ` et al.`. The
parenthesis SHALL hold whichever of the authors and the year the record
holds, joined by `, `, and SHALL be left out when it holds neither. The
services SHALL be those the requirement "A resolution reports the
evidence it was checked against" names; ` via ` and the services SHALL
be left out when there are none. Where the record was retrieved SHALL be
written `the library`, `the library at <root>` for a library
reference's item, `the response cache` or `the network`. No record
is retrieved from the content index, so `the content index` is never
written there: a record reached through the identifier it holds names
the service's response cache or the network, and one reached through a
reference names the library at the reference's root.

Where the file's entry held references, a `resolved` line or a
resolution skip line SHALL carry clauses about them, after any clause
saying the library could not answer and before any rejected candidate,
in entry order:

- for each reference that is `unavailable` or `item-unavailable`, `; a
  library reference could not answer: ` followed by:
  - `<root> is missing`, for `missing`;
  - `<root> holds no library`, for `other-library` with no identity
    found;
  - `<root> holds another library (<found>)`, for `other-library` with
    one;
  - `<path> could not be read (<message>)`, for `unreadable` under
    either status;
  - `the library at <root> holds no item <item>`, for `dangling`;
  - `the library at <root> holds item <item> in several files`, for
    `ambiguous`;
- where references disagreed, one clause: `; library references
  disagree: ` and, for each `disagreeing` reference, `the library at
  <root> names <identifier>`, or `the library at <root> names no
  identifier`, joined by `, `.

A reference `answered` or `agreed` adds no clause.

The human rendering of a resolution skip SHALL be `<path>: skipped, `
followed by, for each kind:

- `no-text-layer`: `no identifier found; the pages read hold no text`;
- `text-without-identifier`: `no identifier found in its metadata or
  the pages read`;
- `encrypted`: `encrypted, so no identifier could be read`;
- `unreadable`: `unreadable (<message>)`;
- `unresolvable`: `no source had a record for <identifier>` followed
  by each service's answer in parentheses, `; `-separated, or `no
  configured service could be asked about <identifier>` when no service
  could be asked;
- `conflict`: `<field> disagrees <N>% (file says <extracted>, record
  says <resolved>)`.

A `resolved` line or a resolution skip line whose `identifier_input`
holds a submission the operator rejected SHALL end with one more
clause, after everything above and after any clause saying the library
could not answer:

- one such submission: `; candidate rejected: ` and its identifier;
- more than one: `; candidates rejected: ` and every such identifier, in
  submission order, joined by `, `.

A line with no rejected submission SHALL carry no such clause. A
submission that reached no record, or whose record led to no move,
SHALL NOT be named in it.

In the human rendering of a `resolved`, `skipped` or
`content-index-write` event, the text after the leading `<path>: ` SHALL
be escaped as the interactive description escapes it, so that a control
character in any value that text takes from a record, a file's contents,
a library store or a service is shown rather than acted on. The leading
path SHALL be printed as every other human line prints it; escaping file
names is outside this requirement.
A `content-index-write` event SHALL render no line when its write was
made, and a line saying the content index could not keep the answer,
with the store's message, when it failed. For an `entry` of kind
`reference`, that line says the content index could not keep its
reference to the library at the reference's root.

#### Scenario: A network answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from Crossref over the
  network to a record with DOI `10.1039/c5ay00042d`, title
  `Determination of things`, one author whose family name is `Smith`,
  and issued in 2015
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network`

#### Scenario: A content-index answer read by a person
- **WHEN** the same file is resolved from the content index, with no
  library reference for its bytes answering, by looking up the DOI it
  holds, which Crossref's response cache answers
- **THEN** the line ends `via crossref, from the response cache`

#### Scenario: A reference answer read by a person
- **WHEN** the same file is answered by a reference to the library at
  `/home/u/papers`, whose item's provenance names Crossref
- **THEN** the line ends `via crossref, from the library at
  /home/u/papers`

#### Scenario: A missing library read by a person
- **WHEN** a file's entry holds a reference whose root `/media/old` no
  longer exists, and its extracted DOI is answered by Crossref's
  response cache
- **THEN** the line ends `from the response cache; a library reference
  could not answer: /media/old is missing`

#### Scenario: A disagreement read by a person
- **WHEN** references to libraries at `/home/u/a` and `/home/u/b`
  disagree, naming `doi:10.1000/x` and `doi:10.1000/y`
- **THEN** the line carries `; library references disagree: the library
  at /home/u/a names doi:10.1000/x, the library at /home/u/b names
  doi:10.1000/y`

#### Scenario: A title carrying terminal escapes
- **WHEN** a file is skipped as a conflict and its embedded title holds
  an escape sequence that would clear the screen
- **THEN** its skip line shows the sequence as text

#### Scenario: A blank scan read by a person
- **WHEN** `borax resolve` skips `scan.pdf`, whose pages hold no text
- **THEN** the line is
  `scan.pdf: skipped, no identifier found; the pages read hold no text`

#### Scenario: A skip after a rejected candidate read by a person
- **WHEN** an interactive run skips `paper.pdf`, in which no identifier
  was found, after its operator saw and skipped the record
  `doi:10.1039/c9cc02492a` reached
- **THEN** the line is
  `paper.pdf: skipped, no identifier found in its metadata or the pages read; candidate rejected: doi:10.1039/c9cc02492a`

### Requirement: A resolution reports the identifiers an operator supplied
Every `resolved` event and every resolution `skipped` event SHALL carry an `identifier_input` section, between `extraction` and `lookup`, reporting every text an operator submitted at the identifier prompt for the file, or why none was submitted.

When at least one text was submitted, the section SHALL be `supplied`,
with `submissions`, `used` and `displaced`. `submissions` SHALL list
every text the prompt returned for the file, in the order typed,
whether it parsed or not. Each entry SHALL carry:

- `submission`: its number, counting from 1 within the file, with no
  gaps;
- `raw`: its text, as the prompt returned it;
- `syntax`: `parsed` with the normalised identifier, or `rejected` with
  why, as the requirement "Supplied text that names no identifier says
  why" states.

A prompt the operator abandons submits nothing and SHALL add no entry.

When nothing was submitted, the section SHALL be not attempted, with
one of these reasons:

- `content-duplicate`, for a content duplicate;
- `not-asked`, where no question was put about the file. That is every
  verdict of a batch run, of `resolve` and of `bib`, and every file an
  interactive run settled or reported without a question;
- `not-supplied`, where a question was put about the file and nothing
  was submitted.

A run that takes no operator input SHALL still carry the section, not
attempted, and SHALL NOT leave it out.

#### Scenario: A batch run takes no operator input
- **WHEN** `borax resolve` resolves one file and skips another because
  no identifier was found in it
- **THEN** both events carry `identifier_input` not attempted with
  reason `not-asked`

#### Scenario: Asked, and nothing supplied
- **WHEN** an interactive run asks about a file with no identifier, and
  the operator chooses to supply one, enters an empty line, and then
  answers skip
- **THEN** the file's `skipped` event carries `identifier_input` not
  attempted with reason `not-supplied`

#### Scenario: Every text is a submission
- **WHEN** the operator types `see email from Anna` and then
  `10.1000/xyz` at the identifier prompt for one file
- **THEN** that file's `identifier_input` is `supplied`, with submission
  1 whose `raw` is `see email from Anna` and whose `syntax` is
  `rejected`, then submission 2 whose `raw` is `10.1000/xyz` and whose
  `syntax` is `parsed` with identifier `doi:10.1000/xyz`

#### Scenario: A passed-over file is reported as a batch run reports it
- **WHEN** an interactive run with `rename.skip-named` on passes over a
  file already carrying its name
- **THEN** its `resolved` event carries `identifier_input` not attempted
  with reason `not-asked`, as a batch run's would

### Requirement: Each submission reports what it came to
Each submission an operator made for a file SHALL carry its own outcome — its `lookup`, its `record_retrieval`, its `match_check`, its `acceptance`, and the `record` it reached where it reached one — except the one submission whose record the event itself reports, which `used` SHALL name and whose outcome SHALL be the event's own sections.

A submission's outcome SHALL be:

- **For a text that did not parse:** its `lookup`, its `match_check`
  and its `acceptance` not attempted with reason `unparsed`, and its
  `record_retrieval` `null`. Nothing was sent to any service.
- **For an identifier no service held, or none could be asked about:**
  its `lookup` as any lookup is reported, with origin `operator`. Its
  `match_check` and its `acceptance` not attempted with reason
  `no-record`, and its `record_retrieval` `null`.
- **For an identifier that reached a record:**
  - its `lookup`;
  - its `record_retrieval`, naming the service and whether its response
    cache or the network answered;
  - its `match_check`;
  - the `record`, in full;
  - its `acceptance`: `rejected` when the record was on offer and
    stopped being, because the operator answered skip or a later
    submission reached a record that took its place on offer; or not
    attempted with reason `no-move` when the record led to no move and
    so was never put to the operator.

A record on offer SHALL NOT be rejected by a later text that is
refused, by a later identifier that reaches no record or a record with
no move, or by the run's saying the library already holds a file of its
work. It stays on offer, as the `rename` requirement "A file's verdict
keeps every identifier its operator supplied" states.

`used` SHALL be a submission number exactly when the event's `lookup`
names the operator as the identifier's origin. When it is:

- that submission's `syntax` SHALL be `parsed`, and its identifier
  SHALL equal the event's `lookup.identifier`;
- its entry SHALL carry only `submission`, `raw` and `syntax`;
- the event's `lookup`, `record_retrieval`, `match_check`, `acceptance`
  and `record` SHALL be its outcome, stated once.

Otherwise `used` SHALL be `null`.

When `used` is a number, `displaced` SHALL carry the file's own
`lookup`, `record_retrieval` and `match_check`. Where the file's own
resolution reached a record, resolved or refused, it SHALL carry that
`record` too. These are the facts the operator's record took the place
of in the event's sections, and a correction SHALL NOT erase them.
Otherwise `displaced` SHALL be `null`.

#### Scenario: A refused text
- **WHEN** the operator types `not-an-identifier` at the identifier
  prompt
- **THEN** its submission's `syntax` is `rejected` with reason
  `unrecognised`, its `lookup`, `match_check` and `acceptance` are not
  attempted with reason `unparsed`, and its `record_retrieval` is
  `null`

#### Scenario: A supplied identifier nobody holds
- **WHEN** the operator supplies a DOI that Crossref and OpenAlex both
  say they do not hold
- **THEN** its submission's `lookup` is attempted with origin
  `operator` and both services as `not-found`, its `match_check` and
  `acceptance` are not attempted with reason `no-record`, and its
  `record_retrieval` is `null`

#### Scenario: A candidate skipped
- **WHEN** the operator supplies a DOI for which Crossref returns a
  record over the network, and then answers skip
- **THEN** its submission carries that record, `record_retrieval`
  `network` naming Crossref, the title check's conclusion, and
  `acceptance` `rejected`, and `used` is `null`

#### Scenario: A candidate set aside for another identifier
- **WHEN** a supplied DOI's record is on offer and the operator supplies
  a second DOI whose record is offered in its place
- **THEN** the first submission's `acceptance` is `rejected`

#### Scenario: A candidate outlasts a supply nobody holds
- **WHEN** a supplied DOI's record is on offer, the operator supplies a
  second DOI that no service holds, and then renames the file from the
  first record
- **THEN** `used` names the first submission, and the second follows it
  with `acceptance` not attempted for `no-record`

#### Scenario: A candidate that leads to no move
- **WHEN** the record a supplied DOI reaches renders the name the file
  already carries
- **THEN** its submission's `acceptance` is not attempted with reason
  `no-move`, and no question is put about it

#### Scenario: The accepted submission is stated once
- **WHEN** the operator supplies a DOI and renames the file from the
  record it reaches
- **THEN** `used` is that submission's number, its entry carries only
  `submission`, `raw` and `syntax`, and the event's `lookup` names that
  DOI with origin `operator`

#### Scenario: A correction keeps the file's own failed lookup
- **WHEN** a file's text layer carries a DOI that no service holds, and
  the operator supplies the published DOI and renames the file from its
  record
- **THEN** `displaced.lookup` carries the file's own DOI with origin
  `extracted` and every service as `not-found`, `displaced.record_retrieval`
  is `null`, and the event's `lookup` is the operator's

#### Scenario: A refused text after a candidate
- **WHEN** a supplied DOI's record is on offer, the operator chooses to
  supply again, types a text that is refused, abandons the prompt, and
  renames the file from the record on offer
- **THEN** `used` names the DOI's submission, and the refused text
  follows it in `submissions` with the next number

### Requirement: Supplied text that names no identifier says why
When a text an operator submits names no identifier, its `syntax` SHALL be `rejected` with a `reason` of `unrecognised`, `invalid` or `checksum`, and, for `invalid` and `checksum`, an `expected` naming the form the text named: `doi`, `arxiv`, `pmid` or `isbn`.

A text names a form by the first of these it carries:

1. a `doi:` prefix or a DOI resolver address;
2. an `arXiv:` prefix;
3. a `pmid:` prefix;
4. an `isbn:`, `isbn-10:` or `isbn-13:` prefix;
5. a body that begins with `10.`, as every DOI does.

Prefixes are compared without regard to case. The reasons mean:

- `invalid`: the text names a form and does not satisfy that form's
  syntax;
- `checksum`: an ISBN that is well formed and fails its check digit;
- `unrecognised`: the text names no form and is not a bare arXiv
  identifier. That covers prose, blank text, and a bare run of digits,
  which could be a PMID or an ISBN.

Which texts are accepted SHALL NOT change: a text accepted as an
identifier is accepted as the same identifier, and a text refused is
refused. Each reason SHALL be a distinction the identifier's own syntax
draws, and none SHALL guess what a text was meant to be.

#### Scenario: Prose
- **WHEN** the operator types `see email from Anna`
- **THEN** its `syntax` is `rejected` with reason `unrecognised` and no
  `expected`

#### Scenario: Bare digits
- **WHEN** the operator types `9781593278281` with no prefix
- **THEN** its `syntax` is `rejected` with reason `unrecognised`

#### Scenario: A malformed DOI
- **WHEN** the operator types `https://doi.org/abc`, and on another
  occasion `10.1234`
- **THEN** each `syntax` is `rejected` with reason `invalid` and
  `expected` `doi`

#### Scenario: An ISBN with a wrong check digit
- **WHEN** the operator types `isbn:9781593278282`
- **THEN** its `syntax` is `rejected` with reason `checksum` and
  `expected` `isbn`

#### Scenario: A malformed PMID
- **WHEN** the operator types `pmid:0`
- **THEN** its `syntax` is `rejected` with reason `invalid` and
  `expected` `pmid`

### Requirement: The content index keeps identification evidence, one fact per file
The content index SHALL keep, under a file's content hash, the identification evidence borax accepted for those bytes, each kind of evidence in a file of its own that only its own writer writes, and SHALL keep no record.

Identification evidence is an identifier and its origin. The content
index keeps two kinds of it:

- **extracted**, origin `extracted`: an extraction pass read the
  identifier from the bytes, and a lookup of it reached a record the
  title check did not refuse;
- **operator**, origin `supplied` or `overridden`: an operator supplied
  the identifier for a file with those bytes, or accepted the record it
  reaches over a conflict the title check concluded.

The `library` capability adds a fourth origin, `adopted`, which only a
library's history entries carry.

The content index SHALL live in the user's data directory, not in the
cache directory: on Unix under `$XDG_DATA_HOME`, else
`$HOME/.local/share`, and on Windows under `%APPDATA%`, else
`$XDG_DATA_HOME`, in `borax/v2/`. A variable that is unset, empty or
relative SHALL be skipped. A system naming no data directory SHALL keep
the index in memory for the run. Nothing SHALL read, import or migrate
the entries an earlier release kept in the cache directory.

A file's entry SHALL be the directory `content/<hash>/` there, holding
at most:

- `extracted.json`, holding the extracted identifier;
- `operator.json`, holding the operator's identifier and its origin;
- `refs/`, holding one file per reference to a library, as "The content
  index keeps references to libraries, one file each" states.

No other file in an entry SHALL be read as a fact, so the temporary an
interrupted write leaves is none.

Each fact SHALL have exactly one writer, which replaces the whole file
atomically and reads no other fact to decide what to write. No writer
SHALL read, merge and rewrite a file another writer writes:

- `extracted.json` is written only when a resolution reaches a record
  by extraction and a lookup and the title check does not refuse it:
  as it resolves, or, where that lookup was asked for again by an
  operator after an outage, when the interactive run moves the file;
- `operator.json` is written only for an operator's decision: when an
  interactive run moves a file renamed from an operator's record, as
  "An operator's answer about a file is remembered" states, or when
  `borax reconcile` restores the decision a library holds, as the
  `library` capability states. The latest operator decision written
  is the one it holds.

So an extraction and an operator's decision about the same bytes,
written in either order or at once, both survive. The references in
`refs/` have writers of their own, which never touch these two files,
and these two writers never touch a reference.

The evidence an entry answers with SHALL be the operator's when
`operator.json` holds evidence, and the extracted evidence otherwise.
An entry whose directory is absent, or holds no evidence that can be
read, answers with no evidence. An `operator.json` that cannot be read
or does not parse SHALL stop the entry's evidence from answering at all,
so the file is resolved as one the index holds no evidence for: the
extracted identifier beside it is the one an operator may have
corrected. An `extracted.json` that cannot be read or does not parse is
ignored. Either is reported as the index holding nothing for the file,
as an unreadable entry is reported today, unless a reference answers.
The next write of that fact replaces it. A reader SHALL ignore a key it
does not know. The entry's references answer before its evidence, as
"A library reference answers for a file before the entry's evidence"
states.

The content index is not a cache. `borax cache --clear` SHALL NOT touch
it, and it SHALL survive every operation on the response cache. It is
not library state either: deleting it changes no answer a library gives
for its own files and no count borax reports. It costs the answers the
index gave for files outside any library, including the identifiers
operators supplied for them. Until the next `borax reconcile` restores
them, it also costs the operator identifiers of a library's files and
the references to the library.

Where a requirement speaks of a record written to, held by, served from
or remembered in the content index, it means the identification
evidence the content index keeps for that record under the file's hash,
and the record a lookup of that evidence reaches, as "A content-index
entry answers with its identifier" states. Where the record reached is
a library item's, through a reference, it means that reference and the
item it names.

#### Scenario: Clearing the cache keeps the index
- **WHEN** a file was resolved and `borax cache --clear` runs
- **THEN** the response cache is empty, the content index still holds
  the file's evidence, and a later run over the file extracts nothing

#### Scenario: An entry holds no record
- **WHEN** `borax resolve` resolves a file from Crossref outside any
  library
- **THEN** the file's entry holds `extracted.json` with the DOI looked
  up, and no title, author or other field of the record

#### Scenario: No data directory
- **WHEN** neither `XDG_DATA_HOME` nor `HOME` is set on Unix, and
  `borax resolve` resolves a file
- **THEN** the run resolves it as it would over an empty index, and no
  content-index entry is written to disk

#### Scenario: An operator's identifier outlasts a bypass
- **WHEN** an operator supplied `doi:10.1000/x` for a file in an earlier
  run, `borax resolve --no-cache` extracts `doi:10.1000/y` from it and
  resolves that, and a later run without `--no-cache` reaches the file,
  no library reference for its bytes answering
- **THEN** the bypassing run writes `extracted.json` with
  `doi:10.1000/y`, `operator.json` still holds `doi:10.1000/x` with
  origin `supplied`, and the later run looks up `doi:10.1000/x`

#### Scenario: A late extraction write erases nothing
- **WHEN** a resolution of a file reads its entry empty, an interactive
  run then remembers an operator's identifier for the same bytes, and
  the first resolution then writes its extracted identifier
- **THEN** the entry holds both files, and answers with the operator's
  identifier

#### Scenario: An operator file that does not parse
- **WHEN** a file's entry holds a readable `extracted.json` and an
  `operator.json` that does not parse
- **THEN** the file is extracted and looked up as one the index holds
  nothing for, and its `content_index.read` is `miss`

#### Scenario: An earlier release's entries are not read
- **WHEN** the cache directory holds a `v1` content-index entry carrying
  a whole record for a file's hash, and the data directory holds
  nothing for it
- **THEN** the file is extracted and looked up as one the index does not
  hold, and the `v1` entry is left where it is

### Requirement: A content-index entry answers with its identifier
A file whose content hash the content index holds evidence for SHALL be resolved by looking up that evidence's identifier, and SHALL NOT be opened for extraction or have its titles checked, wherever neither the run's library nor a library reference in the entry answers for it first.

This applies after the run's library has been asked and has not
answered, and only where the run reads the content index: the cache is
on and the file has a hash. A file the run's library tracks is answered
by the library and never reaches its entry.

It applies after the entry's references have been read, too, and only
where none of them answered: none was reachable, or the reachable ones
disagreed, as "A library reference answers for a file before the
entry's evidence" states. The order is reference, then evidence, then
extraction. What this requirement once called an evidence-first answer
was the whole of an entry that held nothing else. The references read
are reported beside the evidence, whatever they came to.

The identifier SHALL be the evidence the entry answers with, the
operator's before the extracted one, as "The content index keeps
identification evidence, one fact per file" states. It SHALL be looked
up through the run's services, the response cache first where one
stands in front of a service, and the lookup's origin SHALL be the
content index. Extraction and the file's titles are not attempted,
because the content index answered, and the title check SHALL NOT be
made, whatever the evidence's origin:

- evidence of origin `overridden` is an operator's acceptance, which
  "Ambiguity is skipped, never guessed" forbids judging again;
- evidence of origin `supplied` is an operator's identifier, which the
  operator was shown the record for;
- evidence of origin `extracted` was written only for a record the
  title check did not refuse, for these same bytes.

A record found is the file's record, retrieved from the service's
response cache or the network, and nothing is written to the content
index for it. An identifier no service answers for SHALL make the file
a skip of kind `unresolvable`, with the lookup's attempts, and SHALL
NOT send the file to extraction: the same bytes would extract the same
identifier. Outside a library, offline resolution therefore depends on
the response cache.

A record reached this way is a record like any other: the work check
applies to it, and an interactive run puts it the question it calls
for. An operator who asks the services again about the identifier the
entry answered with repeats it with the same origin, keeping the
earlier rounds; the file's titles are still not read and the title
check is still not made, and nothing is written to the content index
for it, which already holds that evidence.

#### Scenario: An evidence answer
- **WHEN** a file outside any library, whose entry holds the DOI an
  earlier run extracted and no reference that answers, is resolved, and
  Crossref's response cache holds that DOI
- **THEN** it resolves from Crossref's response cache with no request
  sent, its `lookup.origin` is `content-index`, and the file is not
  opened

#### Scenario: The operator's identifier answers
- **WHEN** the entry holds an extracted DOI and a DOI an operator
  supplied, and no reference that answers
- **THEN** the supplied DOI is the one looked up

#### Scenario: An accepted conflict is looked up and not judged
- **WHEN** the entry holds evidence of origin `overridden` and no
  reference that answers, and the file's titles conflict with the
  record its identifier reaches
- **THEN** the file is not opened, its `match_check` is not attempted
  with reason `content-index-hit`, and it resolves with that record

#### Scenario: Asking again about an operator's identifier
- **WHEN** the entry holds evidence of origin `overridden` and no
  reference that answers, its lookup meets an outage, and the operator asks the services again, Crossref
  answers, and the operator renames the file
- **THEN** the lookup's origin is still `content-index` with the outage
  kept as an earlier round, the titles are not read, the title check is
  not made, no `content-index-write` event of kind `evidence` is
  emitted, and the entry's evidence is unchanged

#### Scenario: Offline with no response cache
- **WHEN** the entry holds evidence and no reference that answers, the
  response cache does not hold its identifier, and no service can be
  reached
- **THEN** the file is skipped as `unresolvable` with each service
  reported unavailable, it is not extracted, and the entry is unchanged

### Requirement: The content index keeps references to libraries, one file each
The content index SHALL keep, under a file's content hash, a reference to each library location that recorded those bytes, each reference in a file of its own that its writer replaces whole and never merges with another.

A reference names four things: the library's identity, the root the
library was last known at, the identity of the artifact record that
holds the bytes, and the identity of the item that record answers with
for them. It holds no record and no identification evidence.

A reference SHALL be the file `refs/<name>.json` in the hash's entry
directory, holding `library`, `root`, `artifact` and `item`. `<name>`
SHALL be `sha256-` and the lowercase hexadecimal SHA-256 of the
library's identity, the root and the artifact's identity, joined by a
NUL character. So one library, root and artifact name exactly one
file, and a reference whose item changes keeps its file. The root SHALL
be the library root as text, made absolute and normalised lexically as
the library's own stores are read. Identities SHALL be canonical UUID
text. A reader SHALL ignore a key it does not know. A file that cannot
be read, does not parse, or lacks one of the four is not a reference.
Neither is a file in `refs/` whose name does not end `.json`, such as
the temporary an interrupted write leaves. Neither is reported.

Each reference file SHALL be created or replaced by atomic replacement,
by a writer that reads no other fact to decide what to write. Writing a
reference SHALL NOT touch the entry's evidence. Writing evidence SHALL
NOT touch a reference. A reference is removed only by the repair the
`library` capability's requirement "A library's references are written
after its records, and reconcile restores and repairs them" states,
which writes the reference that replaces it first. Two writers of
different references write different files. Two writers of one
reference differ at most in its item, and the one written last stands.

Who writes a reference, when, and what it names is that `library`
requirement's. A library whose root is not UTF-8 text cannot be named by
a reference, and an attempt to write one is reported as a failed write.

References SHALL be preserved: a reference whose library cannot be
reached SHALL NOT be removed because of that, and nothing removes one
automatically. A later operation that prunes references SHALL be an
explicit one of its own.

Like the entry's evidence, the references survive `borax cache --clear`
and every operation on the response cache, and are not library state.
Deleting them changes no answer a library gives for its own files. It
costs the answers they gave for copies of those files elsewhere, until
`borax reconcile` in each library writes them again.

#### Scenario: One reference per library location and artifact
- **WHEN** an applying rename records a file in a library, and
  `borax reconcile` runs over that library afterwards
- **THEN** the file's entry holds one file under `refs/`, naming the
  library's identity, its root, the artifact and the item, and the
  reconcile writes no second one

#### Scenario: A changed item keeps its file
- **WHEN** a reference's artifact record is re-linked to another item,
  and the reference is written again
- **THEN** the same file under `refs/` names the new item, and no other
  file was added

#### Scenario: Evidence and references do not touch each other
- **WHEN** a file's entry holds `extracted.json`, `operator.json` and a
  reference, and a resolution writes its extracted identifier while a
  reconcile replaces the reference with one naming another item
- **THEN** `extracted.json` holds the resolution's identifier, the
  reference names the reconcile's item, and `operator.json` is
  byte-identical

#### Scenario: Clearing the cache keeps the references
- **WHEN** a library's reconcile has written references, and `borax
  cache --clear` runs
- **THEN** every reference file is byte-identical afterwards

### Requirement: A library reference answers for a file before the entry's evidence
For a file the run's library does not answer for, a reachable library reference SHALL answer with the record of the item it names, before the entry's evidence is looked up, as long as every reachable reference agrees; the file SHALL then not be opened, looked up, or have its titles checked.

This applies where the content index is read at all: after the run's
library has been asked and has not answered, with the cache on, and for
a file that has a hash. A file the run's library tracks is answered by
its library item and never reaches its entry, references included. Any
other file inside a library reaches its entry as a file outside does:
an untracked copy, or a file whose record cannot answer. That includes
references to its own library, and its `library` section still reports
what the library said.

The order is reference, then evidence, then extraction. The requirement
"A content-index entry answers with its identifier" describes an entry
no reference of which answers. Earlier changes called an answer outside
a library evidence-first only because the entry held nothing else.

Each reference SHALL be reached through its root, the first of these
deciding:

- the root does not exist, a link to nothing included: the reference is
  `unavailable` because the root is `missing`;
- the root cannot be examined for another reason: `unavailable` because
  it is `unreadable`, with the root and the reason;
- the root is not a directory, or holds no library identity:
  `unavailable` because it holds `other-library`, with no identity
  found;
- its identity file cannot be read or does not parse: `unavailable`
  because it is `unreadable`, with that file and the reason;
- it holds another library's identity: `unavailable` because it holds
  `other-library`, with the identity found;
- no item file of that library carries the reference's item, and the
  item store could not be listed or a file whose name claims that item
  could not be read: `item-unavailable` because it is `unreadable`, with
  the directory or file and the reason;
- no item file carries it otherwise: `item-unavailable` because it is
  `dangling`;
- several item files carry it: `item-unavailable` because it is
  `ambiguous`, with those files;
- otherwise the reference is reachable, and the item it names answers.

The item SHALL be found by the identity inside an item file, never by
its name. The item SHALL be the one the reference names. The artifact
record the reference names is not read. The library's marker is not
consulted: a library is named by its identity. Each root a run's
references name SHALL be read at most once in the run, and reaching a
reference SHALL write nothing anywhere.

References are taken in entry order: by root, then library, then
artifact, compared as text. Two locations of one library are two
references. Among the reachable ones:

- **One, or several that agree**, answer: the first in entry order is
  `answered`, every other reachable one `agreed`. Reachable references
  agree when every item they name carries the same preferred
  identifier, or when none carries one and all name one item of one
  library. The preferred identifier is the record's DOI, else its arXiv
  identifier, else its PMID, else its ISBN, the identifier a `resolved`
  event reports the record under. An item carrying one and an item
  carrying none disagree.
- **Several that disagree** are each `disagreeing`, with the preferred
  identifier its item carries, and none answers. The entry's evidence
  is then looked up, as "A content-index entry answers with its
  identifier" states. With no evidence, the file is extracted as one
  the content index holds nothing for.
- **None reachable**: the entry's evidence is looked up, and failing
  that the file is extracted.

A reference that is unavailable keeps that status whatever the others
come to, so that what answered is distinguishable from the library
that could not.

A record a reference answered with SHALL be the file's record, with the
provenance it was stored with, retrieved from that library's item.
Nothing is written to the content index for it. Extraction, the file's
titles, the lookup, the title check and the content-index write are not
taken, because the content index answered. The record goes through the
work check, and an interactive run puts it the question it calls for.
No identifier established it, so a library admitting the file records
no identification evidence for it.

#### Scenario: A library's correction reaches a copy
- **WHEN** a library holds a file whose item was corrected by hand, and
  `borax resolve` runs on a copy of that file outside every library
- **THEN** the copy resolves with the corrected record, no service is
  asked, the file is not opened, and its `record_retrieval` names the
  library, its root, the artifact and the item

#### Scenario: References answer before an operator's identifier
- **WHEN** a file's entry holds a reachable reference and an
  identifier an operator supplied
- **THEN** the reference answers, and the identifier is not looked up

#### Scenario: Two copies of one library agree
- **WHEN** a library and a copy of it at another root are both
  referenced for a file's bytes, and their items carry the same DOI
- **THEN** the reference whose root sorts first answers, and the other
  is reported `agreed`

#### Scenario: Two libraries disagree
- **WHEN** two reachable references name items carrying different DOIs
- **THEN** neither answers, both are reported `disagreeing` with their
  DOIs, and the entry's evidence is looked up

#### Scenario: A missing library is reported, and the evidence answers
- **WHEN** a file's only reference names a root that no longer exists,
  and its entry holds an extracted DOI
- **THEN** the reference is reported `unavailable` because the root is
  `missing`, it is kept, and the DOI is looked up

#### Scenario: Another library at the old root
- **WHEN** a reference's root now holds a library with another identity
- **THEN** the reference is reported `unavailable` because the root
  holds `other-library`, naming the identity found

#### Scenario: The item is gone
- **WHEN** a reference's library is reachable and no item file carries
  its item
- **THEN** the reference is reported `item-unavailable` because it is
  `dangling`, and the entry's evidence is looked up

#### Scenario: A tracked file reads no reference
- **WHEN** a file its library tracks has bytes whose entry holds a
  reference to another library
- **THEN** it resolves from its own library item, and the reference is
  not read

#### Scenario: A copy inside a library
- **WHEN** an untracked copy of a tracked artifact sits at another path
  in the library, and the copy's bytes are referenced by that library
- **THEN** the copy resolves from the item the reference names, its
  `library` section reports it untracked, and its `record_retrieval` is
  the reference's, not `library`

#### Scenario: The cache bypassed
- **WHEN** a file whose entry holds a reachable reference is resolved
  with `--no-cache`
- **THEN** the content index is reported bypassed, no reference is
  read, and the file is extracted

### Requirement: What became of a library reference write is reported
Every reference a run writes to the content index, and every one it fails to write, SHALL be reported in a `content-index-write` event whose `entry` is of kind `reference`, naming the `library`, the `root`, the `artifact` and the `item`.

The event carries the file's `path`, the content `hash`, the `entry`
and a `write` of `written`, or `failed` with the store's `message`. The
`path` is:

- in a rename, the file where the run left it: the path it moved to, or
  the one it already carried;
- in `borax adopt`, the full path of the orphan it recorded;
- in `borax reconcile`, the full path the record names, whether or not a
  file is there.

A write that repaired a stale root SHALL carry `repaired_from`, the
roots whose references it removed, and SHALL be `failed` when a removal
failed, with a message naming the file left. `repaired_from` is left
out when it would be empty.

A reference held already, naming the same item, is not written, and no
event SHALL be emitted for it. So neither a re-run nor a second
reconcile over an unchanged library reports one.

The events SHALL come in this order:

- in a rename, after the file's `renamed` or `already-named` event and
  its `library-admission` events, and before the event reporting the
  operator's evidence;
- in `borax adopt`, after every `library-adoption` event, in the order
  of the orphans recorded, and before the run's totals;
- in `borax reconcile`, after every per-record event, the `not-pending`
  skips and the operator restorations, and before its totals.

They are written to the stream and the run log alike, never before the
move. A written one renders no human line. A failed one renders
`<path>: the content index could not keep its reference to the library
at <root> (<message>)`. The event SHALL NOT be counted in
`run-finished`, and a failed write SHALL NOT change the exit status or
be reported as a failure of the rename, adoption or reconcile.

#### Scenario: A rename reports its reference
- **WHEN** `borax rename --apply` moves and records a file in a library
  that has an identity
- **THEN** the stream carries its `renamed` event, then a
  `content-index-write` event whose `entry` is of kind `reference`
  naming the library, its root, the new artifact and its item, with
  `write` `written`

#### Scenario: A repair reported
- **WHEN** `borax reconcile` runs over a library moved from `/media/old`
  that a reference still names there
- **THEN** a `content-index-write` event names the new root and carries
  `repaired_from` with `/media/old`, before the reconcile's totals

#### Scenario: Nothing to report the second time
- **WHEN** `borax reconcile` runs twice over a library nothing has
  touched
- **THEN** the second run reports no `content-index-write` event

#### Scenario: A failed reference write
- **WHEN** the content index cannot be written during an applying rename
  in a library
- **THEN** the file is renamed and recorded, the reference's
  `content-index-write` event is `failed` with the store's message, its
  human line names the library's root, and the exit status is what it
  would have been had the write succeeded

