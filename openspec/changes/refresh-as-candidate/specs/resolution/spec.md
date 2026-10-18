## ADDED Requirements

### Requirement: A refresh looks a file's library item up again and never replaces it
A run given `--refresh` SHALL look up again the preferred identifier of the library item that answers for each file it resolves from an item, and SHALL report the record it finds as a candidate beside the item's record, never in its place.

`borax resolve`, `borax rename` and `borax bib` accept `--refresh`, as
the `cli` capability states. Every lookup a run given it makes SHALL
ask the service. No answer is read from the response cache, and each
successful answer is stored in it over any it held, as the requirement
"Responses are cached locally" states. That holds for every lookup of
the run: a candidate's, a file's own, and one an operator asks for
again or supplies.

A file gets a refresh when its verdict is `resolved` and the record it
resolved to is a library item's: the run's own library's, reported with
`record_retrieval` `library`, or another library's that a content-index
reference names, reported with `library-reference`. The item refreshed
SHALL be the one the verdict's record came from, which for bytes
accepted under an earlier item is that item. A verdict that is not
`resolved` gets no refresh, and neither does a work duplicate an
operator then files as another artifact. Nor does a file answered by the content
index's evidence or by extraction: its own lookup is made fresh, and
what it reaches is its ordinary resolution, with its title check and
its evidence written as ever.

The identifier looked up SHALL be the item's preferred identifier: its
DOI, else its arXiv identifier, its PMID, or its ISBN, the identifier
the `resolved` event reports. The services SHALL be asked in the order
the requirement "Sources are queried by identifier type and priority"
gives. An item whose record carries none of the four SHALL be refreshed
by no lookup and reported as having no identifier to ask about.

The file SHALL still resolve to the item's record. Its sections, its
name, its citation and its admission are what they would be without
`--refresh`. The one exception is an item record the operator replaced
in an interactive rename, as the `rename` capability's requirement "An
interactive run asks whether a differing refresh candidate replaces the
library's record" states. A refresh SHALL NOT open the file, read or
write the content index, or check the candidate's title against the
file's.

The record found SHALL be compared with the item's record in these
fields, in this order, each named by the key the record serializes it
under: `type`, `title`, `author`, `issued`, `container-title`, `volume`,
`issue`, `page`, `publisher`, `DOI`, `PMID`, `ISBN`, and the arXiv
identifier, `arxiv`. Two values SHALL be equal exactly when the field
serializes to the same JSON in both records, a field absent from both
included. No value is normalised: authors are compared in order, a date
of issue to its precision, and text as the source and the operator left
it. The record's provenance, its confidence and its source fields SHALL
NOT be compared, since they say where values came from and how a
service files the work, not what the work is.

A candidate equal to the item in every compared field is current, and
nothing is offered or written for it. A candidate that differs SHALL be
offered for acceptance only where the `rename` capability offers it.
Everywhere else it SHALL be reported and SHALL NOT be written into any
library: in `borax resolve`, `borax bib`, a batch `rename` previewing or
applying, a rename that records nothing, and for an item of a library
other than the run's own. No run writes into a library other than its
own.

An item SHALL be the run's own library's when the run's library
answered for the file, or when a content-index reference answered for
it whose library identity is the identity the run's library holds and
whose root, normalised lexically, is the run's library root, normalised
lexically. Such a self-reference answers for an untracked copy inside
the run's library, and its item is treated exactly as an item the run's
library answered with. Every other reference answer SHALL be another
library's: a reference to another identity, or to the run's library's
identity at another root, which is a copy of the library elsewhere. A
run whose library has no readable identity has no self-reference, since
no reference can name it.

A refresh whose lookup no service answered SHALL leave the item as it
is and report what each service said. Whether the services failed or
said they do not hold the identifier, nothing is offered, and asking
again is a later run's.

#### Scenario: A hand correction survives a refresh
- **WHEN** a tracked PDF's item has had its title corrected in a text
  editor, Crossref still holds the old title for its DOI, and `borax
  resolve --refresh --json` runs on the PDF
- **THEN** its `resolved` event carries the corrected title and
  `record_retrieval` `library`, its `refresh` reports a candidate
  differing in `title` with Crossref's title, and the item file is
  byte-identical afterwards

#### Scenario: An applying batch run replaces nothing
- **WHEN** `borax rename --refresh --apply` runs over the same file
- **THEN** the file is named from the item's record, the candidate is
  reported with its acceptance not attempted because no question was
  put, and no item file is written

#### Scenario: An item the services agree with
- **WHEN** a tracked file's item equals what Crossref returns for its
  DOI in every compared field, its provenance aside
- **THEN** its refresh is reported current, and nothing is offered

#### Scenario: A refresh during an outage
- **WHEN** a run given `--refresh` reaches a tracked file while every
  service it asks is unreachable
- **THEN** the file resolves from its item, its refresh reports each
  service's failure in order, and the item is unchanged

#### Scenario: Another library's item
- **WHEN** `borax resolve --refresh` runs on a file outside any library
  that a reference to the library at `/home/u/papers` answers for, and
  the candidate differs
- **THEN** the candidate is reported with its acceptance not attempted
  because the item is another library's, and nothing is written in that
  library

#### Scenario: A copy inside the library answered through its own library
- **WHEN** a run in the library at `/home/u/papers` given `--refresh`
  reaches an untracked copy of a tracked artifact, which a reference to
  that library at `/home/u/papers` answers for, and the candidate
  differs
- **THEN** the item is the run's own library's, and in a batch run the
  candidate's acceptance is not attempted because no question was put,
  not because the item is another library's

#### Scenario: A copy of the library is another library
- **WHEN** the same run reaches a file answered by a reference to the
  same library identity at `/media/backup/papers`
- **THEN** the candidate is reported with its acceptance not attempted
  because the item is another library's, and nothing is written at
  either root

#### Scenario: A file the content index answers for
- **WHEN** a run given `--refresh` resolves an untracked file from the
  identifier the content index holds, while the response cache holds an
  answer for it
- **THEN** the service is asked, its answer is the file's record and is
  stored in the response cache in place of the one held, and the
  `resolved` event carries no `refresh`

#### Scenario: An item with no identifier
- **WHEN** a run given `--refresh` reaches a tracked file whose item
  carries no DOI, arXiv identifier, PMID or ISBN
- **THEN** no service is asked about it, and its refresh is reported
  not attempted because the item carries no identifier

#### Scenario: Provenance is not a difference
- **WHEN** a candidate equals the item in every compared field and its
  source fields name subjects the item's do not
- **THEN** the refresh is reported current

### Requirement: A refresh is reported beside the verdict
A `resolved` event of a run given `--refresh` SHALL carry a `refresh` key, after `acceptance`, exactly when the file got a refresh, and SHALL carry none otherwise.

Which files get one is the requirement "A refresh looks a file's
library item up again and never replaces it"'s to say: a file whose
own resolution reached a library item's record, reported with
`record_retrieval` `library` or `library-reference`. Where an
operator's submission later displaced the file's own steps, the verdict
reached from the submission SHALL still carry the file's refresh as it
stood, and `identifier_input.displaced.record_retrieval` names the item.
A resolution `skipped` event SHALL NOT carry `refresh`.

`refresh` SHALL be an object tagged by `status`:

- `not-attempted`, with `reason` `no-identifier`, for an item carrying
  no identifier to look up;
- `no-eligible-service`, with the `identifier`, when no configured
  service supports its kind;
- `unresolved`, with the `identifier` and its `attempts`, when no
  service supplied a record;
- `current`, with the `identifier` and its `attempts`, for a candidate
  equal to the item in every compared field;
- `differs`, with the `identifier`, its `attempts`, the `differences`,
  the `candidate` and an `acceptance`, otherwise.

The `identifier` SHALL be written as the stream writes one (`doi:…`).
`attempts` SHALL be every service asked, in order, each as the `lookup`
section reports an attempt, never empty, and with a found attempt last.
`differences` SHALL be an array in the compared fields' order, one
object per field that differs, carrying `field`, the field's key, and
`item` and `candidate`, its value in each record as that record
serializes it, each left out where that record has no value.
`candidate` SHALL be the whole record found.

`acceptance` SHALL be tagged by `status`:

- `pending` while the operator is being asked, which only the event a
  question's description is rendered from carries, and never a reported
  verdict;
- `accepted` when the operator replaced the item's record and the write
  landed;
- `unwritten`, with a `message`, when the operator chose to replace it
  and the write failed or was refused, so the item stands;
- `declined` when the operator kept the item's record;
- `not-attempted`, with the first `reason` that applies:
  `another-library` for an item of a library other than the run's own,
  as the requirement "A refresh looks a file's library item up again and
  never replaces it" defines it,
  `not-asked` for a run with nobody to ask, `not-recording` for an
  interactive rename that records nothing, and `already-asked` for an
  item the operator already answered for at an earlier file of the run.

`run-started` SHALL carry `refresh` with the value `true` on a run given
`--refresh`, and SHALL leave it out otherwise.

What the operator decided about a candidate SHALL be reported under
`refresh`, never in the verdict's own `acceptance`, which stays about
the record the file resolved to.

#### Scenario: A differing candidate in the stream
- **WHEN** a batch run given `--refresh` resolves a tracked file whose
  item's title differs from Crossref's
- **THEN** its `resolved` event's last key is `refresh`, with `status`
  `differs`, the DOI as `identifier`, Crossref's found attempt, one
  difference with `field` `title` and both titles, the candidate record,
  and `acceptance` not attempted with reason `not-asked`

#### Scenario: A value only one record has
- **WHEN** the candidate names a volume and the item names none
- **THEN** that difference carries `field` `volume` and the candidate's
  value, and no `item` key

#### Scenario: A run without the flag
- **WHEN** a run not given `--refresh` resolves the same file
- **THEN** its `resolved` event and its `run-started` event carry no
  `refresh` key, and are written exactly as before this change

#### Scenario: The run says it refreshed
- **WHEN** `borax rename --refresh --json` runs
- **THEN** its `run-started` event carries `refresh: true`

#### Scenario: Pending is never reported
- **WHEN** an operator answers the question about a differing candidate
- **THEN** the file's reported `resolved` event carries `accepted`,
  `unwritten` or `declined`, and never `pending`

## MODIFIED Requirements

### Requirement: Responses are cached locally
Successful source responses SHALL be cached on disk keyed by normalized
identifier, and resolved records SHALL additionally be indexed by the
source file's content hash, through the identification evidence "The
content index keeps identification evidence, one fact per file" states,
and through the library references "The content index keeps references
to libraries, one file each" states. A cache hit SHALL be used without any network request, except in a run
given `--refresh`. A bypass
flag (`--no-cache`) SHALL force live queries for every file the run's
library does not answer for, and SHALL skip reading the content index,
and a cache subcommand SHALL be able to clear the cache.

A refresh flag (`--refresh`) SHALL make every lookup of the run ask
the service: no answer is read from the response cache, and each
successful answer is stored in it over any answer it held, as the
requirement "A refresh looks a file's library item up again and never
replaces it" states. It SHALL NOT bypass the content index, which is
read as it is without the flag, and SHALL NOT bypass the library. The
two flags compose: `--refresh --no-cache` asks every service with no
response cache in front, and still reads no content index. Where a
requirement or a scenario says that a lookup is answered from the
response cache with no network request, it describes a run not given
`--refresh`.

Only the response cache is a cache. The cache subcommand clears it and
nothing else: the content index lives outside the cache directory and
survives clearing it. Neither is asked about a file its library tracks:
the run's library answers first, as the requirement "A file its library
tracks resolves from its library item" states. The library is not a
cache, so `--no-cache` SHALL NOT bypass it, and clearing the cache SHALL
change no answer the library gives. The lookup a run given `--refresh`
makes of a tracked file's item asks a service about the item, and
answers nothing about the file.

#### Scenario: Re-run over the same directory is offline
- **WHEN** a directory is processed a second time with an intact cache,
  by a run not given `--refresh`
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

#### Scenario: A refresh asks again and keeps what it hears
- **WHEN** a run given `--refresh` looks up a DOI whose answer the
  response cache holds
- **THEN** the service is asked, the run uses its answer, and the
  response cache holds that answer afterwards in place of the old one

#### Scenario: A refresh still reads the content index
- **WHEN** a run given `--refresh` reaches an untracked file whose
  bytes the content index holds an identifier for
- **THEN** the file is not extracted, and that identifier is looked up
  over the network

### Requirement: A file its library tracks resolves from its library item
A file the run's library tracks SHALL resolve to the record of its library item, and resolution SHALL ask the library before the content index and SHALL then ask neither the content index, extraction, nor any service for that file's record: a run given `--refresh` asks a service about the item, for a candidate that does not change what the file resolves to.

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
The one exception is an item whose record the operator replaced in
this run by accepting a refresh candidate, as the `rename` capability
states: that replacement is the library's own statement, written into
its own text, and every later file of the run is consulted against
the item as replaced.

`borax resolve`, `borax rename` — batch and interactive, previewing and
applying — and `borax bib` SHALL consult the library. `--no-cache`
SHALL NOT bypass it: the library is not a cache. `--no-record` SHALL NOT
bypass it either: that setting withholds the account of admissions and
the writes behind it, and asking the library what a file is does
neither. `--refresh` SHALL NOT bypass it either: the file still
resolves from its item, and the record a fresh lookup of the item's
identifier finds is a candidate beside it, as the requirement "A
refresh looks a file's library item up again and never replaces it"
states.

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
capability states. In a run given `--refresh`, a file whose item's
candidate differs is first asked whether the candidate replaces the
item's record, as the `rename` capability's requirement "An
interactive run asks whether a differing refresh candidate replaces
the library's record" states.

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

#### Scenario: A refresh does not change what a tracked file resolves to
- **WHEN** the run in "A correction in the library wins over the content
  index" is made with `--refresh`, and Crossref holds the old title
- **THEN** the file resolves from its library item with the corrected
  title, its `lookup` is not attempted because the library answered,
  Crossref is asked once about the item's DOI for its `refresh`, which
  reports the old title as a differing candidate, and the item file and
  the content index's entry are unchanged

#### Scenario: A replacement answers the item's next file
- **WHEN** an interactive run given `--refresh` replaces an item's
  record at the first of two files linked to that item
- **THEN** the second file resolves to the item's record as replaced

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
supplied. It applies too to the lookup a run given `--refresh` makes
of a library item's preferred identifier, whose identifier comes from
the item, and which is retained with the refresh rather than as the
file's lookup.

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

#### Scenario: A refresh keeps its attempts
- **WHEN** a refresh of a tracked file's item asks Crossref, which is
  unreachable, and then OpenAlex, which supplies the record
- **THEN** both attempts are retained with the refresh, Crossref's
  first, and the file's own lookup is still not attempted

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
- in a run given `--refresh`, the refresh of the library item that
  answered for the file, and the operator's acceptance of the candidate
  it found;
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
- for a refresh alone, the library item's record carries no identifier
  to look up;
- and, for a refresh candidate's acceptance alone: the item is another
  library's, reached through a reference; the run records nothing into
  its library; the operator already answered for the item at an
  earlier file of the run; or, as for any step, no question was put;
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

#### Scenario: A candidate nobody could accept
- **WHEN** a batch run given `--refresh` reaches a tracked file whose
  candidate differs from its item
- **THEN** the candidate's acceptance is retained as not taken because
  no question was put

#### Scenario: An item with nothing to look up
- **WHEN** a run given `--refresh` reaches a tracked file whose item
  carries no identifier
- **THEN** its refresh is retained as not taken because the item's
  record carries no identifier

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
`not-supplied`, `unparsed`, `no-move`, `no-identifier`,
`another-library`, `not-recording`, or `already-asked`. The last four
occur only within a `resolved` event's `refresh`. A step not taken SHALL NOT be
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

Refreshing was then added to schema 5, before any release carried it.
These are additions, each left out of every line a run not given
`--refresh` writes:

- `refresh` on `run-started`, `true` on a run given `--refresh`;
- `refresh` on `resolved`, after `acceptance`, as the requirement "A
  refresh is reported beside the verdict" states;
- the event `library-replacement`, as the `library` capability's
  requirement "An item's record is replaced only when an operator
  accepts a refresh candidate" states;
- the reasons `no-identifier`, `another-library`, `not-recording` and
  `already-asked`.

No meaning schema 5 had given changes with them. `lookup` is still the
lookup the verdict's record came from, and never a refresh's.
`record_retrieval` `library` still names the item whose record the
verdict carries, which after a replacement the operator accepted in the
run is the item's record as replaced. The verdict's `acceptance` is
still about that record, and stays `automatic` for an item's. A line
of a run not given `--refresh` is written exactly as before. The
version SHALL stay 5.

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

#### Scenario: A refresh after the sections
- **WHEN** a run given `--refresh` resolves a tracked file
- **THEN** its `resolved` event carries the eight sections as a library
  answer does, followed by `refresh`, and schema version 5

#### Scenario: A run without the flag is unchanged
- **WHEN** a run not given `--refresh` resolves a tracked file
- **THEN** its `resolved` event carries no `refresh` key, and its
  `run-started` event no `refresh` key

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

Where a `resolved` event carries `refresh`, its line SHALL carry one
clause about it, after the reference clauses and before any rejected
candidate:

- `not-attempted`: `; no fresh record: the library's record carries no
  identifier to ask about`;
- `no-eligible-service`: `; no fresh record: no configured service
  could be asked about <identifier>`;
- `unresolved`: `; no fresh record: no source had a record for
  <identifier>` followed by each service's answer in parentheses,
  `; `-separated, as the `unresolvable` skip line writes them;
- `current`: `; a fresh record from <service> does not differ`;
- `differs`: `; a fresh record from <service> differs in <fields>; `
  followed by what became of it.

`<service>` is the service whose attempt found the record, named as
after ` via `, and `<fields>` the differing fields' keys joined by `, `.
What became of a differing candidate is written:

- `not-asked`: `the library's record is unchanged`;
- `another-library`: `only a rename in the library at <root> can take
  it`, with the reference's root;
- `not-recording`: `this run records nothing, so the library's record
  is unchanged`;
- `already-asked`: `answered at an earlier file of the same item`;
- `declined`: `the library's record was kept`;
- `accepted`: `the library's record was replaced`;
- `unwritten`: `the library's record could not be replaced
  (<message>)`.

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
`content-index-write` or `library-replacement` event, the text after
the leading `<path>: ` SHALL
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
A `library-replacement` event SHALL render `<path>: replaced <fields>
in <file>` when its write was made, and `<path>: could not replace
<fields> in <file> (<message>)` when it was not, with `<fields>` its
fields joined by `, ` and `<file>` the item file.

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

#### Scenario: A differing candidate read by a person
- **WHEN** `borax resolve --refresh` reports `paper.pdf`, tracked, whose
  item's title differs from the record Crossref returned over the
  network
- **THEN** the line ends `from the library; a fresh record from crossref
  differs in title; the library's record is unchanged`

#### Scenario: A current item read by a person
- **WHEN** the record Crossref returned equals the item in every
  compared field
- **THEN** the line ends `from the library; a fresh record from crossref
  does not differ`

#### Scenario: A replacement read by a person
- **WHEN** an interactive run replaces the title in the item file
  `/home/u/papers/items/klykov2015.<uuid>.toml` for `Klykov2015.pdf`
- **THEN** the line is `Klykov2015.pdf: replaced title in
  /home/u/papers/items/klykov2015.<uuid>.toml`
