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
A `resolved` event SHALL carry every title the file claims for itself, each with where it was read — the XMP packet or the document information dictionary — in the order they were read, including claims the conflict check did not count as evidence, as the `claims` of its `extraction` section's `titles`. A file that was not opened, because the content index or the file's library item answered, SHALL carry its titles as not attempted, with that reason, and not as a file that claims none.

A `resolved` event SHALL carry the identifier the run looked up, in its
`lookup` section, alongside the identifiers the record itself holds. The
two are not always the same — a record resolved from an arXiv identifier
may carry a DOI, which is what the event reports as the record's
`identifier` — and only the former is evidence about the file.

Which services supplied the record's fields is the record's own per-field
provenance, which the event carries in `record`, and it is a separate
fact from where the record was retrieved. Where a rendering names the
services that supplied the record, they SHALL be the service that
answered the lookup, or, when the content index or the library answered,
the sources the record's per-field provenance names other than
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
- **WHEN** a file is resolved from the content index and its record's
  provenance attributes its fields to Crossref
- **THEN** its human line names Crossref as the service, its
  `resolved` event's `record_retrieval` is `content-index`, and its
  titles are not attempted because the content index answered

#### Scenario: A library answer names its service
- **WHEN** a file is resolved from its library item and the item's
  provenance attributes its fields to Crossref
- **THEN** its human line names Crossref as the service, its `resolved`
  event's `record_retrieval` is `library`, its `content_index.read` is
  not attempted because the library answered, and its titles are not
  attempted for the same reason

#### Scenario: The looked-up identifier is carried
- **WHEN** a file resolves from an arXiv identifier to a record that
  also carries a DOI
- **THEN** its `resolved` event's `lookup.identifier` is the arXiv
  identifier, and its `identifier` is the record's DOI

#### Scenario: A record built from two services
- **WHEN** a record whose provenance names OpenAlex for some fields and
  Crossref for others is served by the content index
- **THEN** its human line names Crossref before OpenAlex

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
- **THEN** its `resolved` event carries schema version 4,
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
- **WHEN** `paper.pdf` resolves from the content index because its
  record links to an item the library does not hold
- **THEN** its line is its ordinary resolution line ending `from the
  content index`, followed by `; the library could not answer: ` and a
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
from the file, or an operator who supplied it. A lookup of an
identifier that no configured service supports SHALL be retained with
the identifier and no attempts. That keeps a lookup no service could
take apart from one that every service declined.

This applies to every lookup made for a file: the one made from the
file's own identifier, one an operator asks for again after an outage,
which keeps the extraction pass as its origin, and one made from an
identifier an operator supplied.

A lookup the operator asks for again is the file's own lookup, and it
SHALL replace the lookup retained for the file, whether it finds a
record or not. A lookup made from a supplied identifier belongs to the
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

#### Scenario: Asking again replaces the file's own lookup
- **WHEN** a file's DOI meets an outage, the operator asks the services
  again, and every service now says it does not hold the DOI
- **THEN** the file's retained lookup is the second one, with every
  attempt as not found, so the lookup is conclusive

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

### Requirement: A resolution says where its record came from
Every record a resolution reaches SHALL be retained with where it was retrieved from: the file's library item, naming the artifact and the item; the content index; a service's response cache, naming the service; or the network, naming the service.

A service's answer served from the response cache SHALL be
distinguishable from one the service sent over the network. This
holds for the attempt that found the record as well as for the record.
A record the library or the content index answered with involves no
service attempt.

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
- **WHEN** a file is resolved from the content index
- **THEN** its record is retained as retrieved from the content index,
  with no service attempt

#### Scenario: A library answer
- **WHEN** a file its library tracks is resolved from its item
- **THEN** its record is retained as retrieved from the library, naming
  the artifact record and the item, with no service attempt

#### Scenario: A record an operator reached comes from the service
- **WHEN** the content index answered for a file, or its library
  tracks it, and an operator supplies an identifier that Crossref holds
- **THEN** the record the operator reached is retained as retrieved
  from Crossref, not from the content index or the library, while what
  the content index and the library said is retained as they said it

### Requirement: A resolution says what the content index did
Every resolution of a file SHALL retain what the content index did for it: it answered; it held nothing for the file; it was bypassed because the run turned the cache off; it could not be asked because the file could not be hashed, with the reason hashing gave; or it was not asked because the resolution was already decided, with the reason.

A file that could not be hashed SHALL be retained as such even when the
run bypassed the cache. The missing hash also decides that nothing can
be remembered for the file.

A resolution SHALL also retain what became of writing the record it
reached to the content index under the file's hash. The write was
made; or it failed, with the store's message; or it was not attempted,
with the reason. The reason is the earliest one in the order the
resolution runs:

- the library, the content index, or a content duplicate settled the
  file first;
- extraction found no identifier;
- no service held the identifier;
- the title check refused the record;
- the record waits for an operator to accept it, and is written when
  the file is moved;
- the file has no content hash.

#### Scenario: A hit and a miss
- **WHEN** one file is resolved from the content index, and another,
  whose hash the index does not hold, is extracted and resolved from a
  service
- **THEN** the first retains the content index as having answered, and
  the second retains it as having held nothing and retains the write of
  its record as made

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
- the lookup;
- the title check;
- writing the record to the content index.

The reasons are:

- the run has no library;
- the file lies outside the run's library or in a subtree it excludes;
- the file's bytes duplicate an artifact the library holds;
- the library answered;
- the content index answered;
- extraction found no identifier;
- no service held the identifier, or none could be asked;
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
- **WHEN** a file is resolved from the content index
- **THEN** the titles, extraction, the lookup, the title check and the
  content-index write are each retained as not taken because the
  content index answered

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
  content index, the titles, extraction, the lookup, the title check
  and the content-index write are each retained as not taken because
  the file duplicates content the library holds

### Requirement: Resolution events carry their evidence in sections
Every `resolved` event, and every `skipped` event that is a file's resolution verdict, SHALL carry the evidence of the file's resolution as seven sections, in the order the resolution runs: `library`, `content_index`, `extraction`, `lookup`, `record_retrieval`, `match_check` and `acceptance`.

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
`unhashable`, `awaiting-acceptance`, or `cache-bypassed`. A step not
taken SHALL NOT be left out, and SHALL NOT be reported as a step that
ran and found nothing.

The event schema version SHALL be 4. A `resolved` event SHALL NOT carry
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

#### Scenario: A content-index answer in sections
- **WHEN** a run with no library resolves a file from the content index
- **THEN** its `resolved` event carries schema version 4; `library` not
  attempted with reason `no-library`; `content_index` with `read` `hit`
  and `write` not attempted with reason `content-index-hit`;
  `extraction`'s `result` and `titles`, `lookup` and `match_check` each
  not attempted with reason `content-index-hit`; `record_retrieval` of
  kind `content-index`; and `acceptance` `automatic`

#### Scenario: No schema-3 field survives
- **WHEN** any run emits a `resolved` event or a resolution `skipped`
  event
- **THEN** the `resolved` event carries none of the keys `found`,
  `cached`, `source`, `tier`, `claims` or `overrode` at its top level,
  and the `skipped` event's `reason` carries none of the keys `found`,
  `tier`, `attempts`, `field`, `extracted`, `resolved` or `similarity`

#### Scenario: A resolution skip carries every section
- **WHEN** a file is skipped because no service holds its identifier
- **THEN** its `skipped` event carries all seven sections, with
  `record_retrieval` `null`

### Requirement: A resolution reports what the content index did and every cache write
The `content_index` section SHALL report the content index's `read` as `hit`, `miss`, `bypassed`, `unavailable` with the hashing error's `message`, or not attempted, and its `write` of the record reached as `written`, `failed` with the store's `message`, or not attempted with the earliest reason the requirement "A resolution says what the content index did" orders.

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

### Requirement: Each service asked is reported with its outcome
The `lookup` section SHALL report the identifier the resolution looked up, its `origin` as `extracted` or `operator`, and every service asked about it, in the order asked, each as an object with the `service` and an `outcome` tagged `found`, `not-found`, `unavailable` with its `message`, `rate-limited`, or `malformed` with its `message`.

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
`operator` for an identifier an operator supplied. The pass that read an
extracted identifier is `extraction`'s result's `tier`, and SHALL NOT be
repeated in `lookup`.

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

### Requirement: A resolution reports where its record was retrieved
The `record_retrieval` section SHALL name where the record a resolution reached was retrieved from: `library` with the `artifact` and `item` identities, `content-index`, `service-cache` with the `service`, or `network` with the `service`.

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

#### Scenario: An operator's lookup over a tracked file
- **WHEN** an operator supplies an identifier for a tracked file and
  renames it from the record Crossref holds
- **THEN** its `record_retrieval` names Crossref, and its `library`
  section still reports the library's answer as `tracked`

### Requirement: A resolution reports its title check and whether its record was accepted
The `match_check` section SHALL report the title check's conclusion as `agreed`, `conflict` with the `field`, the `extracted` and `resolved` values and their `similarity`, `insufficient-evidence` with a `reason` of `record-untitled`, `no-titles` or `no-evidence`, or not attempted, and the `acceptance` section SHALL report `automatic`, `overridden` or `not-applicable`.

`acceptance` SHALL be `overridden` exactly when an operator accepted a
record over a conflict the title check concluded. The conflict stays in
`match_check` on the same event, and `acceptance` SHALL carry nothing
beside its status. It SHALL be `automatic` when a record was used and no
conflict was overridden to use it, and `not-applicable` on every
resolution skip, a conflict skip included.

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
  `target-taken` with the target, and none of the seven sections

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
When an interactive run writes a record to the content index on the move it followed, it SHALL report what became of the write in a `content-index-write` event carrying the file's `path` after the move and a `write` of `written`, or `failed` with the store's `message`.

The event SHALL follow the file's `renamed` event, and its
`library-admission` event where there is one, and SHALL precede anything
else reported about the file. It SHALL be written to the stream and the
run log alike, as every event after a move is, and SHALL NOT be recorded
before the move.

The `resolved` event before the move SHALL report the write as not
attempted with reason `awaiting-acceptance`. Where no write is made — a
batch run, whose resolution wrote its record as it resolved; a move
declined, skipped or never reached; a file already named — no
`content-index-write` event SHALL be emitted.

The event SHALL NOT be counted in `run-finished`, and a `failed` write
SHALL NOT change the exit status or be reported as a failure of the
rename.

#### Scenario: A supplied identifier remembered
- **WHEN** an operator supplies a DOI for a file and renames it from the
  record it reaches
- **THEN** the stream carries the file's `resolved` event with
  `content_index.write` not attempted for `awaiting-acceptance`, its
  `renamed` event, and then a `content-index-write` event naming the
  file's new path with `write` `written`, and the run log carries the
  same three

#### Scenario: The answer could not be kept
- **WHEN** the same rename is made and the content index cannot be
  written
- **THEN** the `content-index-write` event's `write` is `failed` with
  the store's message, the rename is reported as made, and the run's
  counts and exit status are unchanged by the failure

#### Scenario: A batch run writes as it resolves
- **WHEN** `borax rename --apply` renames a file it resolved from a
  service
- **THEN** no `content-index-write` event is emitted, and the file's
  `resolved` event reports `content_index.write` `written`

### Requirement: Human resolution lines name the work, where it came from, and why a file was skipped
The human rendering of a `resolved` event SHALL be `<path>: resolved <identifier>`, then ` to ` and the record's title in double quotes followed by its authors and year of issue in parentheses, then ` via ` and the services that supplied the record, then `, from ` and where the record was retrieved, with each part the record does not hold left out.

The authors SHALL be named by family name: one alone, two joined by
` and `, and three or more as the first followed by ` et al.`. The
parenthesis SHALL hold whichever of the authors and the year the record
holds, joined by `, `, and SHALL be left out when it holds neither. The
services SHALL be those the requirement "A resolution reports the
evidence it was checked against" names; ` via ` and the services SHALL
be left out when there are none. Where the record was retrieved SHALL be
written `the library`, `the content index`, `the response cache` or `the
network`.

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

In the human rendering of a `resolved`, `skipped` or
`content-index-write` event, the text after the leading `<path>: ` SHALL
be escaped as the interactive description escapes it, so that a control
character in any value that text takes from a record, a file's contents,
a library store or a service is shown rather than acted on. The leading
path SHALL be printed as every other human line prints it; escaping file
names is outside this requirement.
A `content-index-write` event SHALL render no line when its write was
made, and a line saying the content index could not keep the answer,
with the store's message, when it failed.

#### Scenario: A network answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from Crossref over the
  network to a record with DOI `10.1039/c5ay00042d`, title
  `Determination of things`, one author whose family name is `Smith`,
  and issued in 2015
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network`

#### Scenario: A content-index answer read by a person
- **WHEN** the same file is resolved from the content index, its
  record's provenance naming Crossref
- **THEN** the line ends `via crossref, from the content index`

#### Scenario: A title carrying terminal escapes
- **WHEN** a file is skipped as a conflict and its embedded title holds
  an escape sequence that would clear the screen
- **THEN** its skip line shows the sequence as text

#### Scenario: A blank scan read by a person
- **WHEN** `borax resolve` skips `scan.pdf`, whose pages hold no text
- **THEN** the line is
  `scan.pdf: skipped, no identifier found; the pages read hold no text`

