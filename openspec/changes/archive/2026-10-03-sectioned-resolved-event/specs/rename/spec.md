## ADDED Requirements

### Requirement: An interactive description says whether the file's titles were read
The description an interactive run shows before a question SHALL state which state the file's titles are in: the titles read, each with where it was read; that the file was read and claims no title; that the file could not be opened, with the reason; or that the file was not read, with why.

The description of a file skipped because extraction found no identifier
in it, or because no service held its identifier, SHALL show the file's
titles in the same way, since they are what an operator supplying an
identifier has to go on, and they are carried by the file's `skipped`
event.

The description of a file extraction found no identifier in SHALL say
which way extraction failed: the pages read held no text, or held text
and no identifier. An encrypted file SHALL be described as encrypted,
not as unreadable.

The description SHALL remain a rendering of the file's resolution event,
as the requirement "An interactive question shows what the answer rests
on" states: each of these is read from the event's `extraction`
section, and none is worked out anew.

#### Scenario: A titled blank scan
- **WHEN** an interactive run offers to supply an identifier for a PDF
  whose single page is blank and whose document information carries the
  title `A Title`
- **THEN** the description says no text was found on the pages read,
  and shows `A Title` as the title the file claims, read from the
  document information

#### Scenario: A file that claims no title
- **WHEN** an interactive run asks about a file that was opened, claims
  no title, and resolved from its DOI
- **THEN** the description says the file claims no title in its
  metadata, rather than that nothing was read

#### Scenario: A record from an earlier run
- **WHEN** an interactive run asks about a file the content index
  answered for
- **THEN** the description says the file's titles were not read because
  an earlier run answered

#### Scenario: An encrypted file
- **WHEN** an interactive run offers to supply an identifier for a PDF
  encrypted under a user password
- **THEN** the description says the file is encrypted, and that its
  titles could not be read, with the reason

## MODIFIED Requirements

### Requirement: An interactive run asks about files it could not settle
An interactive rename run SHALL put a question to the operator, in addition to the questions for proposed moves, for each file whose content hash is known and which: had no identifier found in it; had an identifier no service holds; could not be read as a PDF or is encrypted; resolved to a record its claimed titles conflict with; or resolved to an identifier an item already in the library carries.

A file with no identifier, an unreadable body, or an identifier no
service holds SHALL be offered: supply an identifier, skip, or quit. A
file whose resolution failed without settling whether any service holds
its identifier — a service unreachable, rate-limited, or answering
unusably — SHALL additionally be offered a retry, first, and its
question SHALL say that the services did not answer rather than that
the identifier is unknown. A file with a
conflict whose proposal is a move SHALL additionally be offered renaming
to the proposed target despite the conflict, and that choice SHALL name
the target and SHALL NOT be the default. Every question about a proposed
move SHALL additionally offer supplying a different identifier.

A file whose own resolution lands on an identifier an item already
carries SHALL be
offered: file it as another artifact of that item, skip, or quit. The
question SHALL name the item and a path already recorded against it
that still holds a file, so that the operator is deciding about a work
they can see they hold, and filing SHALL NOT be the default. An item
with no such path is not a work duplicate at all and SHALL NOT produce
this question: there is no file to compare against, and the incoming
one is that item's first artifact. Accepting SHALL return the file to
planning, where the ordinary question about its move is put; only then
is it moved and recorded. Declining SHALL report it with the
work-duplicate reason a batch run would have given. This is the one
question borax asks about a duplicate, and only an operator can answer
it: a second artifact of a work and an unwanted second copy of it are
one thing in the bytes and two in the intention.

A record the operator reached rather than one the run resolved — an
identifier they supplied, a lookup they retried, a record they accepted
over a conflict — SHALL be checked against the library the same way and
SHALL NOT produce that question. Where it lands on a work the library
already holds a file of, the run SHALL say so, naming the same item and
recorded path the question would have named, and SHALL put the ordinary
question about the file's move again; the answer given after it stands,
and a move carried out then admits the file as another artifact of that
item. The collision SHALL be stated once, so that a second answer is
never asked for twice and the move is never unreachable. Supplying an
identifier is the operator saying what the file is, which is the
statement the filing question exists to obtain, so what is left is not
a decision to ask for but a fact they have not been told.

#### Scenario: A supplied identifier lands on a work already held
- **WHEN** an interactive run's operator supplies the DOI of a work the
  library already holds a file of, and answers rename
- **THEN** the run reports the item and the recorded path it collided
  with, puts the question about the move again, and a second rename
  answer moves the file and records it as another artifact of that
  item, with no second item minted

When `rename.skip-named` is off, an already-named file SHALL be put to
the operator with the choices keep its name, supply a different
identifier, or quit. This is the one question an already-named file is
asked, and it is asked only when that setting is off; a run passing
over named files asks nothing about them.

A file reported as a content duplicate, or whose content hash is
unknown, SHALL NOT be asked about. The same bytes are already in the
library, so there is nothing an operator could add: the question above
is asked about a work the library holds, never about a file it holds.

Skipping any of these files SHALL leave it untouched and report it with
the reason a batch run would have given, not as `declined`. A file that
had a move on offer and was skipped after a supplied identifier led
nowhere SHALL be reported `declined`, since a move is what was
declined.

A supplied identifier that resolves SHALL leave the file in whatever
situation its new record puts it: a move to offer, a target taken, a
name that renders empty, or a file already named. A supplied
identifier that does not resolve, and an input the operator abandons,
SHALL leave the file exactly as it was, with the record and the choices
it already had.

#### Scenario: Supplying an identifier for an unidentified file
- **WHEN** an interactive run finds no identifier in an author manuscript
  and the operator supplies the DOI of its published version and then
  answers rename
- **THEN** the file is renamed from the published record, and the run
  log records a `resolved` event whose `lookup` names the operator as
  the identifier's origin and a `renamed` event for it

#### Scenario: Skipping an unidentified file
- **WHEN** the operator skips a file with no identifier
- **THEN** it keeps its name and is reported skipped with the reason
  its extraction gave: `text-without-identifier` where its pages hold
  text, or `no-text-layer` where they hold none

#### Scenario: A reference's DOI caught
- **WHEN** a proposal comes from a DOI that belongs to a paper the file
  cites, and the operator supplies the file's own DOI instead
- **THEN** a new proposal is made from the file's own record, and the
  name first proposed is not claimed

#### Scenario: Re-identifying a named file
- **WHEN** an interactive run with `--no-skip-named` reaches a file named
  from the wrong record, and the operator supplies the right identifier
  and answers rename
- **THEN** the file is renamed from the right record, and if it has an
  artifact record that record is re-linked to the item for the supplied
  record, so its name and what the library says it is are corrected
  together

#### Scenario: Filing a second artifact of a work already held
- **WHEN** an interactive run reaches a scan of a paper whose published
  PDF the library already holds under the same DOI, and the operator
  answers that it is another artifact of that item
- **THEN** the question names the item and the recorded path of the
  artifact already linked to it, the ordinary move question follows, and
  answering rename files the scan and records it against that same item

#### Scenario: Declining a second copy
- **WHEN** the operator answers skip to that question
- **THEN** the file keeps its name and is reported with the
  work-duplicate reason a batch run would have given, not as `declined`

#### Scenario: A content duplicate is still never asked about
- **WHEN** an interactive run reaches a file whose hash is already in an
  artifact record's history
- **THEN** it is reported a content duplicate and no question is put

### Requirement: An interactive question says whether the library answered
The description an interactive run shows before a question SHALL say when the file's record is its library item's, and SHALL state what the library could not answer for when it could not.

Where the file's `resolved` event reports `library` as its
`record_retrieval`, the description SHALL name the record's own
identifier without claiming
where it was found, since nothing was looked up; SHALL name on its
`record` line the services the item's provenance records followed by
`from the library`, or the library alone where the provenance names no
service; and SHALL show that no title was read from the file. It SHALL
NOT describe a library answer as coming from the file or from an
earlier run.

Where the file's resolution event reports a library answer of a kind the
library could not answer with, the description SHALL carry a line
labelled `library` stating the problem as the file's human output line
states it — after the `record` line for a resolved file, and after the
reason for a file that was skipped.

The description SHALL remain a rendering of the file's resolution
event: it shows what that event's `library` and `record_retrieval`
carry, and nothing the event does not. What the questions offer and what their
answers do SHALL NOT change: a file the library answered for is asked
about its move as any resolved file is, and one already named is passed
over under `rename.skip-named` as any already-named file is.

#### Scenario: Deciding on a library answer
- **WHEN** an interactive run is about to ask whether to rename a
  tracked file whose item's provenance names Crossref
- **THEN** the description names the item's DOI without saying where it
  was found, its `record` line reads `Crossref, from the library`, and
  it says nothing was read from the file

#### Scenario: A library that could not answer
- **WHEN** an interactive run is about to ask about a file whose record
  links to an item the library does not hold, and which resolved from
  the content index instead
- **THEN** the description carries a `library` line naming the artifact
  and the missing item, and its `record` line says the record comes from
  an earlier run

#### Scenario: A file nothing identified, whose library could not answer
- **WHEN** an interactive run offers to supply an identifier for a file
  that carries none and whose record links an item the library does not
  hold
- **THEN** the description carries a `library` line naming the artifact
  and the missing item after the reason the file was not identified

#### Scenario: A tracked file already named is passed over
- **WHEN** an interactive run with `rename.skip-named` on reaches a
  tracked file that already carries the name its item's record implies
- **THEN** nothing is shown or asked about it, and its `resolved` and
  `already-named` events are in the run log
