## MODIFIED Requirements

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

The schema version does not change, because every key and kind here is
an addition and no existing one changes meaning.

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

### Requirement: A resolution reports where its record was retrieved
The `record_retrieval` section SHALL name where the record a resolution reached was retrieved from: `library` with the `artifact` and `item` identities, `content-index`, `service-cache` with the `service`, or `network` with the `service`.

The `library` retrieval's `item` SHALL name the item whose record was
returned, which is what this section has always attributed the record
to. For a file whose bytes are an earlier entry accepted under another
item, that is the item it was accepted under, not the record's link.
The link is reported in the `library` section. No key is added here,
and the field keeps its meaning, so the schema version does not
change.

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

#### Scenario: A record retrieved through an earlier entry
- **WHEN** a tracked file's bytes are an earlier entry accepted under
  work X's item, while the record links work Y's
- **THEN** `record_retrieval` is `library` naming the artifact and X as
  `item`, while the `library` section's `tracked` answer names Y as
  `item` and X as `accepted_under`
