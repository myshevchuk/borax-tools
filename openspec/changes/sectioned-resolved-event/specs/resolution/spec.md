## ADDED Requirements

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

The event schema version SHALL be 4. The schema-3 fields `found`,
`cached`, `source`, `tier`, `claims` and `overrode` SHALL NOT be
emitted on any event, under those names or any other, and no event SHALL
carry a schema-3 rendering of a fact beside its schema-4 one. A
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
- **WHEN** any run emits a `resolved` event or a `skipped` event
- **THEN** the event carries none of the keys `found`, `cached`,
  `source`, `tier`, `claims` or `overrode`

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

Every value the human rendering of a `resolved` or `skipped` event, or
of a `content-index-write` event, takes from a record, a file, a library
store or a service SHALL be escaped as the interactive description
escapes it, so that a control character is shown rather than acted on.
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

## MODIFIED Requirements

<!-- drops: schema 3's `source` field and its `cache` and `library` stand-ins; schema 4 reports where a record was kept in `record_retrieval`, and the services move to the human line -->
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

<!-- drops: the `tier` and `cached` reporting of a library answer, and the statement that the schema version does not change, which schema 4 replaces -->
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
