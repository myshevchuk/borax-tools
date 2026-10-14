## ADDED Requirements

### Requirement: The content index keeps identification evidence and library references
The content index SHALL keep, under a file's content hash, the identification evidence borax accepted for those bytes and a reference to each library that recorded them, and SHALL keep no record.

Identification evidence is an identifier and its origin. The origin
SHALL be one of:

- `extracted`: an extraction pass read the identifier from the bytes;
- `supplied`: an operator supplied it for a file with those bytes;
- `overridden`: an operator accepted the record it reaches over a
  conflict the title check concluded;
- `adopted`: `borax adopt` recorded the bytes on it.

`supplied` and `overridden` are an operator's origins. An entry holds
at most one piece of evidence. A library reference names a library's
identity, the library's root as borax last wrote it, an artifact
identity and an item identity, and an entry holds zero or more of them.
Two roots of one library are two references. The `library` capability
states when a reference is written.

The content index SHALL live in the user's data directory, not in the
cache directory: on Unix under `$XDG_DATA_HOME`, else `$HOME/.local/share`,
and on Windows under `%APPDATA%`, else `$XDG_DATA_HOME`, in
`borax/v2/content/`, one JSON file per hash. A variable that is unset,
empty or relative SHALL be skipped. A system naming no data directory
SHALL keep the index in memory for the run. Nothing SHALL read, import
or migrate the entries an earlier release kept in the cache directory.

The content index is not a cache. `borax cache --clear` SHALL NOT touch
it, and it SHALL survive every operation on the response cache. It is
not library state either: deleting it changes no answer a library gives
for its own files and no count borax reports, and costs only the
answers it gave for files outside any library and the identifiers of
operators' answers kept there.

Evidence SHALL be written under a file's hash in exactly these places:

- when a resolution reaches a record by extraction and a lookup and the
  title check does not refuse it: the identifier looked up, `extracted`;
- when an interactive run moves a file renamed from an operator's
  record, as "An operator's answer about a file is remembered" states:
  the identifier, `supplied` or `overridden`;
- beside a library reference, when the entry holds no evidence: the
  evidence the referenced artifact record's history holds for those
  bytes.

Evidence of an operator's origin SHALL be replaced only by evidence of
an operator's origin. A resolution's write of `extracted` evidence over
it SHALL NOT be made, and SHALL be reported not attempted with reason
`operator-evidence`. Every write SHALL keep the entry's other half: an
evidence write keeps the references, and a reference write keeps the
evidence.

An entry that cannot be read or does not parse SHALL be read as no
entry, and a later write SHALL replace it. A reader SHALL ignore a key
it does not know.

Where a requirement speaks of a record written to, held by, served from
or remembered in the content index, it means the identification
evidence the content index keeps for that record under the file's hash,
and the record reached through that evidence or through a library
reference, as "A content-index entry answers through its references,
then its identifier" states.

#### Scenario: Clearing the cache keeps the index
- **WHEN** a file was resolved and `borax cache --clear` runs
- **THEN** the response cache is empty, the content index still holds
  the file's evidence, and a later run over the file extracts nothing

#### Scenario: An entry holds no record
- **WHEN** `borax resolve` resolves a file from Crossref outside any
  library
- **THEN** the entry under its hash holds the DOI looked up with origin
  `extracted`, and no title, author or other field of the record

#### Scenario: No data directory
- **WHEN** neither `XDG_DATA_HOME` nor `HOME` is set on Unix, and
  `borax resolve` resolves a file
- **THEN** the run resolves it as it would over an empty index, and no
  content-index entry is written to disk

#### Scenario: An operator's identifier outlasts a bypass
- **WHEN** an operator supplied `doi:10.1000/x` for a file in an earlier
  run, and `borax resolve --no-cache` extracts `doi:10.1000/y` from it
  and resolves that
- **THEN** its `content_index.write` is not attempted with reason
  `operator-evidence`, and the entry still holds `doi:10.1000/x` with
  origin `supplied`

#### Scenario: An earlier release's entries are not read
- **WHEN** the cache directory holds a `v1` content-index entry carrying
  a whole record for a file's hash, and the data directory holds
  nothing for it
- **THEN** the file is extracted and looked up as one the index does not
  hold, and the `v1` entry is left where it is

### Requirement: A content-index entry answers through its references, then its identifier
A file whose content hash the content index holds an entry for SHALL be answered through the entry's library references when they agree, otherwise by looking up the entry's identifier, and SHALL be opened for extraction only when neither answers, wherever the run's library does not answer for it first.

This applies after the run's library has been asked and has not
answered, and only where the run reads the content index: the cache is
on and the file has a hash. A file the run's library tracks is answered
by the library and never reaches its entry. So a run inside a library
is never ambiguous about its own tracked files, and an untracked file
inside it reads its entry as any file does.

Each reference SHALL be given exactly one outcome, in the entry's
order:

- **unavailable**, with its cause, when the reference's library cannot
  be reached at its root:
  - `missing`: the root does not exist;
  - `other-library`: the root holds no library identity, or holds
    another library's, which is named;
  - `unreadable`: the root, or the library identity file there, cannot
    be read, named with the reason;
- **item-unavailable**, with its cause, when the library is there and
  the item the reference names is not carried by exactly one readable
  item file of it: `dangling`, `unreadable` with the file or directory
  and the reason, or `ambiguous` with the files, as for an earlier
  entry's item ("A library that cannot answer for a file says so");
- otherwise **reachable**, with that item's record.

Reachable references SHALL agree when every one's item carries the same
preferred identifier: the first of DOI, arXiv identifier, PMID and ISBN
its record holds, as the event's `identifier` is chosen. Items that
carry none SHALL agree only when they are one item, the same item
identity in the same library. When exactly one reference is reachable,
or several are and they agree, the first reachable one in the entry's
order SHALL answer and the others SHALL be reported as agreeing. Its
item's record is the file's record. The file is not opened, nothing is
looked up, its titles are not checked, and nothing is written to the
content index for it. When several are reachable and do not agree, none
SHALL answer, and each SHALL be reported as disagreeing with the
preferred identifier its item carries.

An unavailable reference is not a deletion. It SHALL be kept in the
entry, SHALL NOT be pruned by reading it, and SHALL be reported with
its cause on every resolution that reads it, so that a resolution
answered by the evidence is always distinguishable from one a library
answered, and an unreachable library from a disagreeing one. Reading a
reference SHALL write nothing, to the entry or to the library it names.

When no reference answers and the entry holds evidence, the evidence's
identifier SHALL be looked up through the run's services, the response
cache first where one stands in front of a service, and the lookup's
origin SHALL be the content index. The file SHALL NOT be opened: its
extraction and its titles are not attempted because the content index
answered, and the title check SHALL NOT be made, whatever the
evidence's origin. Evidence of origin `overridden` is an operator's
acceptance, which "Ambiguity is skipped, never guessed" forbids judging
again. Evidence of any other origin was written only for a record the
title check did not refuse, for these same bytes. A record found is the
file's record, retrieved from the service's response cache or the
network. An identifier no service answers for SHALL make the file a
skip of kind `unresolvable`, with the lookup's attempts. Outside a
library, offline resolution therefore depends on the response cache.

When no reference answers and the entry holds no evidence, the file
SHALL be resolved as one the index holds nothing for: extraction, the
lookup and the title check, and the evidence written on success beside
the entry's references.

A record reached through a reference or through the entry's evidence
is a record like any other: the work check applies to it, and an
interactive run puts it the question it calls for.

#### Scenario: One library answers
- **WHEN** a file outside any library has the bytes of an artifact a
  library at `/home/u/papers` recorded, and `borax resolve` runs on it
- **THEN** it resolves from that library's item, its `record_retrieval`
  is `library-reference` naming the library, its root, the artifact and
  the item, no service is asked, and the file is not opened

#### Scenario: Two locations that agree
- **WHEN** the entry holds references to one library at `/a` and at
  `/b`, a copy of it, whose items carry the same DOI
- **THEN** the reference to `/a` answers, the one to `/b` is reported as
  agreeing, and the file resolves from `/a`'s item

#### Scenario: One location gone
- **WHEN** `/a` has since been deleted
- **THEN** the reference to `/a` is reported unavailable with cause
  `missing`, the reference to `/b` answers, and the entry still holds
  both

#### Scenario: Locations that disagree
- **WHEN** the item at `/b` has been corrected by hand to another DOI,
  and the entry holds evidence of origin `extracted`
- **THEN** neither reference answers, both are reported as disagreeing
  with the DOI each item carries, and the entry's DOI is looked up with
  origin `content-index`

#### Scenario: Another library at the root
- **WHEN** the directory a reference names holds a library whose
  identity is not the one the reference names
- **THEN** the reference is reported unavailable with cause
  `other-library`, naming the identity found there, and the evidence is
  looked up instead

#### Scenario: An accepted conflict is looked up and not judged
- **WHEN** the entry holds no reference and evidence of origin
  `overridden`
- **THEN** its identifier is looked up, the file is not opened, its
  `match_check` is not attempted with reason `content-index-hit`, and it
  resolves with the record the lookup reaches

#### Scenario: Offline with no response cache
- **WHEN** the entry holds evidence alone, the response cache does not
  hold its identifier, and no service can be reached
- **THEN** the file is skipped as `unresolvable` with each service
  reported unavailable, and the entry is unchanged

#### Scenario: An entry with nothing that answers
- **WHEN** the entry's only reference is unavailable and it holds no
  evidence
- **THEN** the file is extracted, looked up and checked, and the entry
  afterwards holds the evidence and still the reference

#### Scenario: A copy inside the library
- **WHEN** a byte-identical copy of a tracked artifact sits untracked
  inside its library, whose reference the entry holds, and `borax
  resolve` runs on the copy
- **THEN** its `library` answer is `untracked`, and it resolves from the
  item through the reference

## MODIFIED Requirements

### Requirement: Responses are cached locally
Successful source responses SHALL be cached on disk keyed by normalized
identifier, and resolved records SHALL additionally be indexed by the
source file's content hash, through the identification evidence "The
content index keeps identification evidence and library references"
states. A cache hit SHALL be used without any network request. A bypass
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
answered the lookup, or, when the library or a library reference the
content index holds answered, the sources the record's per-field provenance names other than
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
- **WHEN** a file is resolved from the content index, through a
  reference to a library whose item's provenance attributes its fields
  to Crossref
- **THEN** its human line names Crossref as the service, its
  `resolved` event's `record_retrieval` is `library-reference`, and its
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
  Crossref for others is served by the content index, through a library
  reference
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
is asked. The content index entry is written all the same: the
identifier with its origin, and a reference to the library once the
artifact record is written. It is what answers for the file where the
library does not, through the reference or by looking the identifier
up.

Outside a library the content index is the one place the answer is
kept, and it keeps it durably: the content index is not a cache,
`borax cache --clear` leaves it, and an identifier extraction finds
never replaces an operator's, as "The content index keeps
identification evidence and library references" states. A later run
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
decision and its next reconcile writes the reference. A run SHALL NOT
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
  writes its reference to the content index again

#### Scenario: Clearing the cache forgets no answer
- **WHEN** a file outside any library was renamed from a supplied
  identifier, `borax cache --clear` runs, and a batch run reaches the
  file with the services reachable
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
Through the content index it can still reach the artifact's item:
where the library wrote a reference for those bytes, the reference
answers as "A content-index entry answers through its references, then
its identifier" states, while the library's own answer stays
`untracked`.

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
- **THEN** it resolves from the content index exactly as it would with
  no library, and its `resolved` event reports the library's answer as
  untracked

#### Scenario: A copy is not the artifact
- **WHEN** a byte-identical copy of a tracked artifact sits at another
  path in the library with no record naming that path, and `borax
  resolve` runs on the copy
- **THEN** the copy is not answered as the library's artifact: its
  `library` answer is `untracked`, and it resolves through the content
  index — by the library's reference to the artifact's item where the
  index holds one, its `record_retrieval` then `library-reference` and
  not `library` — or through extraction and the services as before

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
Every record a resolution reaches SHALL be retained with where it was retrieved from: the file's library item, naming the artifact and the item; another library's item, through a reference the content index holds, naming that library, its root, the artifact and the item; a service's response cache, naming the service; or the network, naming the service.

A service's answer served from the response cache SHALL be
distinguishable from one the service sent over the network. This
holds for the attempt that found the record as well as for the record.
A record the library or a library reference answered with involves no
service attempt. A record reached by looking up the identifier the
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
- **WHEN** a file is resolved from the content index, through a
  reference to another library
- **THEN** its record is retained as retrieved through that reference,
  naming the library, its root, the artifact and the item, with no
  service attempt

#### Scenario: An identifier the content index held
- **WHEN** a file is resolved by looking up the identifier the content
  index holds for it, and Crossref's response cache answers
- **THEN** its record is retained as served by Crossref's response
  cache, and so is the attempt that found it

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
Every resolution of a file SHALL retain what the content index did for it: it held an entry for the file, with the identification evidence the entry held and what each of its library references came to; it held nothing for the file; it was bypassed because the run turned the cache off; it could not be asked because the file could not be hashed, with the reason hashing gave; or it was not asked because the resolution was already decided, with the reason.

An entry held is not by itself an answer. The content index answered
when a reference answered or the entry's identifier was looked up, as
"A content-index entry answers through its references, then its
identifier" states, and what was retained of the entry is what shows
which.

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
- the file has no content hash;
- the content index holds an identifier an operator gave for those
  bytes, which extraction does not replace.

#### Scenario: A hit and a miss
- **WHEN** one file is resolved from the content index, and another,
  whose hash the index does not hold, is extracted and resolved from a
  service
- **THEN** the first retains the content index as having held an entry
  that answered, and the second retains it as having held nothing and
  retains the write of its record as made

#### Scenario: An operator's identifier is not overwritten
- **WHEN** the content index holds an identifier an operator supplied
  for a file, and a run with `--no-cache` extracts and resolves another
- **THEN** its resolution retains the content index as bypassed and the
  write as not attempted because an operator's identifier is held

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
- **WHEN** a file is resolved from the content index, through a library
  reference
- **THEN** the titles, extraction, the lookup, the title check and the
  content-index write are each retained as not taken because the
  content index answered

#### Scenario: An identifier the content index held
- **WHEN** a file is resolved by looking up the identifier the content
  index holds for it
- **THEN** the titles, extraction, the title check and the
  content-index write are each retained as not taken because the
  content index answered, and the lookup is retained as made

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
`not-supplied`, `unparsed`, `no-move`, or `operator-evidence`. A step
not taken SHALL NOT be left out, and SHALL NOT be reported as a step
that ran and found nothing.

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
`content_index.read` `hit` is an entry held rather than a record
served, `content_index.write` writes identification evidence rather
than a record, `record_retrieval` `content-index` is no longer emitted,
`content-index-write` reports every write of a file's entry after its
verdict, and `library-adoption` `unindexed` means no entry rather than
no record. A consumer reading schema 4 correctly could read these
wrongly, so the version SHALL be 5. No release has carried schema 5, so
a later change SHALL be free to change it without a further bump until
one does.

#### Scenario: A content-index answer in sections
- **WHEN** a run with no library resolves a file from the content index,
  through a library reference
- **THEN** its `resolved` event carries schema version 5; `library` not
  attempted with reason `no-library`; `content_index` with `read` `hit`
  carrying that reference as `answered`, and `write` not attempted with
  reason `content-index-hit`; `extraction`'s `result` and `titles`,
  `lookup` and `match_check` each not attempted with reason
  `content-index-hit`; `identifier_input` not attempted with reason
  `not-asked`; `record_retrieval` of kind `library-reference`; and
  `acceptance` `automatic`

#### Scenario: An evidence answer in sections
- **WHEN** a run with no library resolves a file by looking up the
  identifier the content index holds, answered by Crossref's response
  cache
- **THEN** its `content_index` `read` is `hit` with that identifier and
  its origin as `evidence`; `extraction`'s `result` and `titles` and
  `match_check` are not attempted with reason `content-index-hit`;
  `lookup` is attempted with `origin` `content-index`; and
  `record_retrieval` is `service-cache` naming Crossref

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

A `hit` SHALL carry what the entry held: `evidence`, with its
`identifier` and its `origin` (`extracted`, `supplied`, `overridden` or
`adopted`), absent when the entry holds none; and `references`, every
library reference of the entry in its order, possibly none. Each
reference SHALL carry its `library`, `root`, `artifact` and `item`, and
a `status` naming what it came to:

- `answered`: its item's record is the file's record;
- `agreed`: reachable, and agreeing with the one that answered;
- `disagreeing`: reachable, and not agreeing, with the `identifier` its
  item carries, absent when it carries none;
- `unavailable`, with a `cause`: `missing`; `other-library`, with
  `found` naming the identity there when there is one; or `unreadable`,
  with the `path` and `message`;
- `item-unavailable`, with a `cause`: `dangling`; `unreadable`, with
  the `path` and `message`; or `ambiguous`, with the item `files`.

The `write` is of the identification evidence the record was reached
by, and a write withheld because the entry holds an operator's
identifier is not attempted with reason `operator-evidence`.

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

#### Scenario: A hit names what the entry held
- **WHEN** a file's entry holds a DOI of origin `supplied` and a
  reference to a library whose root is missing
- **THEN** `content_index.read` is `hit` with `evidence` naming the DOI
  and `supplied`, and `references` holding that reference with `status`
  `unavailable` and `cause` `missing`

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
The `record_retrieval` section SHALL name where the record a resolution reached was retrieved from: `library` with the `artifact` and `item` identities, `library-reference` with the `library`, `root`, `artifact` and `item` of the reference that answered, `service-cache` with the `service`, or `network` with the `service`.

The `library` retrieval's `item` SHALL name the item whose record was
returned, which is what this section has always attributed the record
to. For a file whose bytes are an earlier entry accepted under another
item, that is the item it was accepted under, not the record's link.
The link is reported in the `library` section. No key was added for
that, and the field kept its meaning, so the schema version did not
change for it.

The `library-reference` retrieval names the library identity, the root
it was read at, the artifact and the item of the one reference that
answered, as "A content-index entry answers through its references,
then its identifier" chooses it. The content index holds no record, so
`content-index` SHALL NOT be reported as a retrieval. A record reached
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

### Requirement: What became of remembering an operator's answer is reported
When an interactive run writes a record to the content index on the move it followed, or any run writes a library reference to the content index after a library recorded a file, it SHALL report what became of each write in a `content-index-write` event carrying the file's `path`, its content `hash`, the `entry` written, and a `write` of `written`, or `failed` with the store's `message`.

The `entry` SHALL be tagged by `kind`: `evidence`, with the `identifier`
and its `origin`, for the identifier an operator's answer was
remembered by; or `reference`, with the `library`, `root`, `artifact`
and `item` written, `repaired_from` naming the stale root it replaced
when the write repaired one, and `evidence` when the write also gave the
entry the identification evidence it lacked. A reference is written as
the `library` capability's requirement "A library decision is
referenced from the content index after it is written" states.

The event SHALL follow the file's `renamed` event, and its
`library-admission` event where there is one, and SHALL precede anything
else reported about the file. Where a run writes both for one file, the
reference's event SHALL come first and the evidence's second. In `borax
adopt` and `borax reconcile`, which report after their pass, the events
SHALL follow every per-object event of the run and precede its totals.
The `path` is the file after the move, the orphan adopted, or the path
the reconciled record names. It SHALL be written to the stream and the
run log alike, as every event after a move is, and SHALL NOT be recorded
before the move.

The `resolved` event before the move SHALL report the write as not
attempted with reason `awaiting-acceptance`. Where no write is made — a
batch run, whose resolution wrote its record's evidence as it resolved;
a move declined, skipped or never reached; a file already named whose
entry already held its reference; a reference withheld because the
library has no readable identity — no `content-index-write` event SHALL
be emitted for it.

The event SHALL NOT be counted in `run-finished`, and a `failed` write
SHALL NOT change the exit status or be reported as a failure of the
rename, the adoption or the reconcile.

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
  service, with no library to record it in
- **THEN** no `content-index-write` event is emitted, and the file's
  `resolved` event reports `content_index.write` `written`

#### Scenario: A recorded rename reports its reference
- **WHEN** `borax rename --apply` renames and records a file in a
  library that has an identity
- **THEN** its `renamed` event is followed by a `content-index-write`
  event whose `entry` is of kind `reference`, naming the library, its
  root, the new artifact and its item, with `write` `written`

#### Scenario: Supplied in a library
- **WHEN** an operator supplies a DOI for a file in an interactive run
  inside a library that has an identity, and renames it
- **THEN** its `renamed` event is followed by the reference's
  `content-index-write` event and then the evidence's

### Requirement: Human resolution lines name the work, where it came from, and why a file was skipped
The human rendering of a `resolved` event SHALL be `<path>: resolved <identifier>`, then ` to ` and the record's title in double quotes followed by its authors and year of issue in parentheses, then ` via ` and the services that supplied the record, then `, from ` and where the record was retrieved, with each part the record does not hold left out.

The authors SHALL be named by family name: one alone, two joined by
` and `, and three or more as the first followed by ` et al.`. The
parenthesis SHALL hold whichever of the authors and the year the record
holds, joined by `, `, and SHALL be left out when it holds neither. The
services SHALL be those the requirement "A resolution reports the
evidence it was checked against" names; ` via ` and the services SHALL
be left out when there are none. Where the record was retrieved SHALL be
written `the library`, `the library at ` followed by the root of the
reference that answered, `the response cache` or `the network`. No
record is retrieved from the content index, so `the content index` is
never written there.

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

A `resolved` line or a resolution skip line whose `content_index` read
names library references that did not answer SHALL say so, after any
clause saying the library could not answer and before any rejected
candidate:

- for each unavailable reference, `; a library reference could not
  answer: ` and `<root> is missing`, `<root> holds another library
  (<identity>)`, `<root> holds no library`, `<path> could not be read
  (<message>)`, `the library at <root> holds no item <item>` or `the
  library at <root> holds item <item> in several files`, as its cause
  says;
- for references that disagree, one clause: `; library references
  disagree: ` and, joined by `, `, `the library at <root> names
  <identifier>` for each, or `names no identifier` where its item
  carries none.

A reference that answered or agreed adds no clause.

In the human rendering of a `resolved`, `skipped` or
`content-index-write` event, the text after the leading `<path>: ` SHALL
be escaped as the interactive description escapes it, so that a control
character in any value that text takes from a record, a file's contents,
a library store or a service is shown rather than acted on. The leading
path SHALL be printed as every other human line prints it; escaping file
names is outside this requirement.
A `content-index-write` event SHALL render no line when its write was
made, and a line saying the content index could not keep the answer,
with the store's message, when it failed: `the content index could not
keep this answer (<message>)` for an entry of kind `evidence`, and `the
content index could not keep its reference to the library at <root>
(<message>)` for one of kind `reference`.

#### Scenario: A network answer read by a person
- **WHEN** `borax resolve` resolves `paper.pdf` from Crossref over the
  network to a record with DOI `10.1039/c5ay00042d`, title
  `Determination of things`, one author whose family name is `Smith`,
  and issued in 2015
- **THEN** the line is
  `paper.pdf: resolved doi:10.1039/c5ay00042d to "Determination of things" (Smith, 2015) via crossref, from the network`

#### Scenario: A content-index answer read by a person
- **WHEN** the same file is resolved from the content index, through a
  reference to a library at `/home/u/papers` whose item's provenance
  names Crossref
- **THEN** the line ends `via crossref, from the library at
  /home/u/papers`

#### Scenario: A missing library read by a person
- **WHEN** the same file's entry also holds a reference to a library at
  `/mnt/old`, which no longer exists
- **THEN** the line ends `via crossref, from the library at
  /home/u/papers; a library reference could not answer: /mnt/old is
  missing`

#### Scenario: A reference that could not be kept read by a person
- **WHEN** an applying run records `paper.pdf` in a library at `/lib`
  and the content index cannot be written
- **THEN** a line reads the file's path, then `: the content index
  could not keep its reference to the library at /lib (`, the store's
  message and `)`, and the rename and the record stand

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
