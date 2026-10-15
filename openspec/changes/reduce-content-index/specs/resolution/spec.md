## ADDED Requirements

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
- `refs/`, reserved for references to libraries, which nothing yet
  writes or reads.

No other file in an entry SHALL be read as a fact, so the temporary an
interrupted write leaves is none.

Each fact SHALL have exactly one writer, which replaces the whole file
atomically and reads no other fact to decide what to write. No writer
SHALL read, merge and rewrite a file another writer writes:

- `extracted.json` is written only when a resolution reaches a record
  by extraction and a lookup and the title check does not refuse it;
- `operator.json` is written only for an operator's decision: when an
  interactive run moves a file renamed from an operator's record, as
  "An operator's answer about a file is remembered" states, or when
  `borax reconcile` restores the decision a library holds, as the
  `library` capability states. The latest operator decision written
  is the one it holds.

So an extraction and an operator's decision about the same bytes,
written in either order or at once, both survive.

The evidence an entry answers with SHALL be the operator's when
`operator.json` holds evidence, and the extracted evidence otherwise.
An entry whose directory is absent, or holds no fact that can be read,
is no entry. An `operator.json` that cannot be read or does not parse
SHALL stop the entry from answering at all, so the file is resolved as
one the index holds nothing for: the extracted identifier beside it is
the one an operator may have corrected. An `extracted.json` that cannot
be read or does not parse is ignored. Either is reported as the index
holding nothing for the file, as an unreadable entry is reported today,
and the next write of that fact replaces it. A reader SHALL ignore a key
it does not know.

The content index is not a cache. `borax cache --clear` SHALL NOT touch
it, and it SHALL survive every operation on the response cache. It is
not library state either: deleting it changes no answer a library gives
for its own files and no count borax reports. It costs the answers the
index gave for files outside any library, including the identifiers
operators supplied for them, and, until the next `borax reconcile`
restores them, the operator identifiers of a library's files.

Where a requirement speaks of a record written to, held by, served from
or remembered in the content index, it means the identification
evidence the content index keeps for that record under the file's hash,
and the record a lookup of that evidence reaches, as "A content-index
entry answers with its identifier" states.

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
  resolves that, and a later run without `--no-cache` reaches the file
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
A file whose content hash the content index holds evidence for SHALL be resolved by looking up that evidence's identifier, and SHALL NOT be opened for extraction or have its titles checked, wherever the run's library does not answer for it first.

This applies after the run's library has been asked and has not
answered, and only where the run reads the content index: the cache is
on and the file has a hash. A file the run's library tracks is answered
by the library and never reaches its entry.

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
for.

#### Scenario: An evidence answer
- **WHEN** a file outside any library, whose entry holds the DOI an
  earlier run extracted, is resolved, and Crossref's response cache
  holds that DOI
- **THEN** it resolves from Crossref's response cache with no request
  sent, its `lookup.origin` is `content-index`, and the file is not
  opened

#### Scenario: The operator's identifier answers
- **WHEN** the entry holds an extracted DOI and a DOI an operator
  supplied
- **THEN** the supplied DOI is the one looked up

#### Scenario: An accepted conflict is looked up and not judged
- **WHEN** the entry holds evidence of origin `overridden`, and the
  file's titles conflict with the record its identifier reaches
- **THEN** the file is not opened, its `match_check` is not attempted
  with reason `content-index-hit`, and it resolves with that record

#### Scenario: Offline with no response cache
- **WHEN** the entry holds evidence, the response cache does not hold
  its identifier, and no service can be reached
- **THEN** the file is skipped as `unresolvable` with each service
  reported unavailable, it is not extracted, and the entry is unchanged

## MODIFIED Requirements

### Requirement: Responses are cached locally
Successful source responses SHALL be cached on disk keyed by normalized
identifier, and resolved records SHALL additionally be indexed by the
source file's content hash, through the identification evidence "The
content index keeps identification evidence, one fact per file" states. A cache hit SHALL be used without any network request. A bypass
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
  its library item instead

#### Scenario: Clearing the cache spares the index
- **WHEN** `borax cache --clear` runs after a file was resolved, and the
  file is resolved again
- **THEN** it is not extracted, its identifier is looked up again over
  the network, and the content index answered for it

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
answered the lookup, or, when the library answered, the sources the record's per-field provenance names other than
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
- **WHEN** a file is resolved from the content index, by looking up the
  DOI it holds, which Crossref's response cache answers
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

#### Scenario: The looked-up identifier is carried
- **WHEN** a file resolves from an arXiv identifier to a record that
  also carries a DOI
- **THEN** its `resolved` event's `lookup.identifier` is the arXiv
  identifier, and its `identifier` is the record's DOI

#### Scenario: A record built from two services
- **WHEN** a record whose provenance names OpenAlex for some fields and
  Crossref for others is served by the library
- **THEN** its human line names Crossref before OpenAlex

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
operator evidence. It is what answers for the file where the library
does not, a copy of the same bytes elsewhere among them, by looking the
identifier up. An interruption between the two leaves the decision in
the library alone, and the next `borax reconcile` writes it to the
content index, as the `library` capability states.

Outside a library the content index is the one place the answer is
kept, and it keeps it durably: the content index is not a cache,
`borax cache --clear` leaves it, and an identifier extraction finds
never takes the place of an operator's, as "The content index keeps
identification evidence, one fact per file" states. A later run
looks the operator's identifier up, so the record itself comes from
the response cache, or from the services when the response cache does
not hold it.

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
  library item where its library tracks it — and reports it already
  named

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
  file with the services reachable
- **THEN** the file is not asked about and not extracted, and the
  supplied identifier is looked up over the network

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
Every record a resolution reaches SHALL be retained with where it was retrieved from: the file's library item, naming the artifact and the item; a service's response cache, naming the service; or the network, naming the service.

A service's answer served from the response cache SHALL be
distinguishable from one the service sent over the network. This
holds for the attempt that found the record as well as for the record.
A record the library answered with involves no service attempt. A
record reached by looking up the identifier the
content index holds was retrieved from the service that answered that
lookup, from its response cache or the network. The content index holds
no record, and so is never where one was retrieved from.

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
- **WHEN** a file is resolved from the content index, by looking up the
  identifier it holds, and Crossref's response cache answers
- **THEN** its record is retained as served by Crossref's response
  cache, not as retrieved from the content index, and so is the attempt
  that found it

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
Every resolution of a file SHALL retain what the content index did for it: it answered, with the identification evidence it answered with; it held nothing for the file; it was bypassed because the run turned the cache off; it could not be asked because the file could not be hashed, with the reason hashing gave; or it was not asked because the resolution was already decided, with the reason.

The content index answers by the identifier it holds, which the
resolution then looks up, as "A content-index entry answers with its
identifier" states. It answers with no record of its own.

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
- **WHEN** one file is resolved from the content index, and another,
  whose hash the index does not hold, is extracted and resolved from a
  service
- **THEN** the first retains the content index as having answered, with
  the identifier it held, and the second retains it as having held
  nothing and retains the write of its record as made

#### Scenario: An operator's identifier is not overwritten
- **WHEN** the content index holds an identifier an operator supplied
  for a file, and a run with `--no-cache` extracts and resolves another
- **THEN** its resolution retains the content index as bypassed and the
  write of the extracted identifier as made, and the content index
  still answers with the operator's identifier

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
- the content index answered, with the identifier it holds;
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
- **WHEN** a file is resolved from the content index
- **THEN** the titles, extraction, the title check and the
  content-index write are each retained as not taken because the
  content index answered, and the lookup of the identifier it held is
  retained as made

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
which the resolution then looks up, rather than a record served;
`content_index.write` writes identification evidence rather than a
record; `record_retrieval` `content-index` is no longer emitted;
`content-index-write` reports every write of an operator's evidence,
`borax reconcile`'s restorations among them, and not only the move's;
and `library-adoption` `unindexed` means no entry rather than
no record. A consumer reading schema 4 correctly could read these
wrongly, so the version SHALL be 5. No release has carried schema 5, so
a later change SHALL be free to change it without a further bump until
one does.

#### Scenario: A content-index answer in sections
- **WHEN** a run with no library resolves a file from the content index,
  whose identifier Crossref's response cache answers for
- **THEN** its `resolved` event carries schema version 5; `library` not
  attempted with reason `no-library`; `content_index` with `read` `hit`
  carrying that identifier and its origin as `evidence`, and `write`
  not attempted with reason `content-index-hit`; `extraction`'s
  `result` and `titles` and `match_check` each not attempted with
  reason `content-index-hit`; `identifier_input` not attempted with
  reason `not-asked`; `lookup` attempted with `origin`
  `content-index`; `record_retrieval` of kind `service-cache` naming
  Crossref; and `acceptance` `automatic`

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

A `hit` SHALL carry `evidence`: the `identifier` the entry answered
with and its `origin`, `extracted`, `supplied` or `overridden`. An
entry holding no evidence that can be read is reported `miss`, as "The
content index keeps identification evidence, one fact per file" states.

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
  `supplied`
- **THEN** `content_index.read` is `hit` with `evidence` naming the
  supplied DOI and `supplied`

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
The `record_retrieval` section SHALL name where the record a resolution reached was retrieved from: `library` with the `artifact` and `item` identities, `service-cache` with the `service`, or `network` with the `service`.

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
  the content index holds for it, and Crossref answers over the network
- **THEN** `record_retrieval` is `network` naming Crossref

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

### Requirement: What became of remembering an operator's answer is reported
When an interactive run writes a record to the content index on the move it followed, or `borax reconcile` writes an operator's identifier to the content index from a library's history, it SHALL report what became of each write in a `content-index-write` event carrying the file's `path`, its content `hash`, the `entry` written, and a `write` of `written`, or `failed` with the store's `message`.

The `entry` SHALL be tagged by `kind`: `evidence`, with the `identifier`
and its `origin`, `supplied` or `overridden`, which the write put in
the entry's operator evidence. A restoration is written as the
`library` capability's requirement "A library's operator decisions are
restored to the content index" states.

The event SHALL follow the file's `renamed` event, and its
`library-admission` event where there is one, and SHALL precede anything
else reported about the file. In `borax reconcile`, which reports
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
`content-index-write` event SHALL be emitted for it.

The event SHALL NOT be counted in `run-finished`, and a `failed` write
SHALL NOT change the exit status or be reported as a failure of the
rename or the reconcile.

#### Scenario: A supplied identifier remembered
- **WHEN** an operator supplies a DOI for a file and renames it from the
  record it reaches
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
- **THEN** no `content-index-write` event is emitted, and the file's
  `resolved` event reports `content_index.write` `written`

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
written `the library`, `the response cache` or `the network`. No record
is retrieved from the content index, so `the content index` is never
written there: a record reached through the identifier it holds names
the service's response cache or the network.

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
with the store's message, when it failed.

#### Scenario: A network answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from Crossref over the
  network to a record with DOI `10.1039/c5ay00042d`, title
  `Determination of things`, one author whose family name is `Smith`,
  and issued in 2015
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network`

#### Scenario: A content-index answer read by a person
- **WHEN** the same file is resolved from the content index, by looking
  up the DOI it holds, which Crossref's response cache answers
- **THEN** the line ends `via crossref, from the response cache`

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
