## ADDED Requirements

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

## MODIFIED Requirements

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
