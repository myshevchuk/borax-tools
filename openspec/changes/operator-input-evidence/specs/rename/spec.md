## ADDED Requirements

### Requirement: A file's verdict keeps every identifier its operator supplied
An interactive run SHALL report, in the resolution event of each file it settles, every identifier its operator supplied for that file and what each came to, so that the file's final verdict is told apart from the outcomes of the candidates before it.

A file skipped after a candidate SHALL be reported with its own verdict:
its own reason and its own sections. The candidate SHALL appear in its
submission, as the operator's rejection, and never as the file's
record.

A file renamed from a record a supplied identifier reached SHALL be
reported with that record and its submission named as `used`. Its
`acceptance` SHALL be `accepted`, or `overridden` where the rename went
over the record's title conflict.

A record a supplied identifier reached SHALL stay on offer, awaiting the
operator's answer, until one of these:

- the operator renames from it;
- the operator skips the file;
- a later supplied identifier reaches a record that takes its place on
  offer;
- the operator quits.

A later identifier that reaches no record, or a record that leads to no
move, SHALL leave it on offer. So SHALL a rename the run answers by
saying the library already holds a file of the record's work, as the
requirement "An interactive run asks about files it could not settle"
requires. The answer given after that notice is the one that settles
the record.

A file renamed from its own record SHALL report its submissions, if
any, beside that record's evidence.

Nothing the operator typed SHALL be reported for a file the run quit
at, since nothing at all is reported for it.

#### Scenario: A correction and a skip
- **WHEN** an interactive run finds no identifier in a file whose pages
  hold text, and the operator:
  - supplies `not-an-identifier`, which is refused;
  - supplies `10.1039/c9cc02492`, which no service holds;
  - supplies `10.1039/c9cc02492a`, for which Crossref returns a record;
  - answers skip
- **THEN** the file is reported `skipped` with reason
  `text-without-identifier` and its own sections;
- **AND** its `identifier_input` lists three submissions in order:
  - the first, `syntax` `rejected` as `unrecognised`, with nothing
    looked up;
  - the second, looked up as `doi:10.1039/c9cc02492` with each service
    answering `not-found`;
  - the third, with Crossref's record and `acceptance` `rejected`;
- **AND** it names none as `used`, and its human line ends
  `; candidate rejected: doi:10.1039/c9cc02492a`

#### Scenario: Accepting a supplied candidate
- **WHEN** the operator supplies a DOI whose record's title agrees with
  the file's, is shown that record as pending, and answers rename
- **THEN** the file's `resolved` event reports that record with
  `acceptance` `accepted` and the DOI's submission as `used`, followed by
  its `renamed` and `content-index-write` events

#### Scenario: Accepting a supplied candidate over its conflict
- **WHEN** the operator supplies a DOI whose record's title conflicts
  with the file's, and renames the file to that record's name anyway
- **THEN** the file's `resolved` event carries `match_check` `conflict`,
  `acceptance` `overridden`, and the DOI's submission as `used`

#### Scenario: Overriding the file's own conflict
- **WHEN** the file's own record conflicts with its title, and the
  operator renames the file anyway without supplying anything
- **THEN** the file's `resolved` event carries `acceptance` `overridden`
  and `identifier_input` not attempted with reason `not-supplied`

#### Scenario: A candidate replaced by another
- **WHEN** the operator supplies a DOI, sees its record, supplies a
  second DOI, and renames the file from the second record
- **THEN** the first submission carries its record with `acceptance`
  `rejected`, and the second is `used`

#### Scenario: A supply nobody holds keeps the candidate on offer
- **WHEN** the operator supplies a DOI whose record is offered as a
  move, then supplies a second DOI that no service holds
- **THEN** the services' answers about the second DOI are shown, the
  question put again still offers the move to the first record's name,
  and that record is still pending; answering rename then reports it
  with `acceptance` `accepted` and the first DOI's submission as `used`,
  and the second submission with `acceptance` not attempted for
  `no-record`

#### Scenario: A second candidate takes the first one's place
- **WHEN** the operator supplies a DOI whose record is offered as a
  move, then supplies a second DOI whose record is also offered as a
  move
- **THEN** the first submission is `rejected` with its record, and the
  question put next describes the second record as pending

#### Scenario: A collision notice keeps the candidate pending
- **WHEN** the operator supplies a DOI whose work the library already
  holds a file of, and answers rename
- **THEN** the run names the item and the recorded path, puts the
  question again, and the record is still described as pending; a
  second rename moves the file and reports `acceptance` `accepted`, a
  skip reports the submission `rejected`, a further supply that reaches
  a record on offer rejects it, and a quit reports nothing about the
  file

#### Scenario: Quitting at a candidate
- **WHEN** the operator supplies a DOI, sees its record, and quits
- **THEN** no event reports the file, and the content index holds
  nothing new for it

### Requirement: An interactive description says what became of the operator's candidates
The description an interactive run shows before a question SHALL say when the record it describes is a candidate pending the operator's answer, and SHALL name each candidate the operator has rejected for the file.

A record whose `acceptance` is `pending` SHALL be described with a line
labelled `candidate` reading `pending; skipping leaves the file as it
was`. Its title conflict, where it has one, SHALL be shown on the
`conflict` line before the operator answers, as it is shown for a
record accepted over one.

Each submission whose `acceptance` is `rejected` SHALL be named on a
line labelled `rejected`, in submission order. The line carries the
submission's identifier whole, as the `identifier` line carries one.

Both SHALL be read from the event the description renders, and every
value SHALL be escaped as every description value is. A file with no
rejected submission and no pending record SHALL be described as it is
without them.

#### Scenario: A candidate on offer
- **WHEN** the operator supplies a DOI and the record it reaches is
  described
- **THEN** the description carries a `candidate` line saying the record
  is pending and that skipping leaves the file as it was

#### Scenario: A pending candidate's conflict
- **WHEN** the record a supplied DOI reaches has a title the file's own
  title conflicts with
- **THEN** its description shows the `conflict` line before the
  operator answers

#### Scenario: A rejected candidate named in the next question
- **WHEN** a supplied DOI's record is on offer, and the operator
  supplies a second DOI whose record is offered in its place
- **THEN** the question about the second record carries a `rejected`
  line naming the first DOI

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
declined. Either way, the file's resolution event — its
`skipped` verdict, or the `resolved` event reported before a `declined`
skip — SHALL carry every identifier the operator supplied for it, as the
requirement "A file's verdict keeps every identifier its operator
supplied" states.

A supplied identifier that resolves SHALL leave the file in whatever
situation its new record puts it: a move to offer, a target taken, a
name that renders empty, or a file already named. A supplied
identifier that does not resolve, and an input the operator abandons,
SHALL leave the file exactly as it was, with the record and the choices
it already had. Where the record on offer was one an earlier supplied
identifier reached, that record stays on offer, still awaiting the
operator's answer. The one thing that does change: the identifier that
did not resolve is kept among the file's submissions, with what its
lookup came to. An abandoned input submitted nothing and adds none.

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

#### Scenario: Skipping after a supplied identifier led nowhere
- **WHEN** the operator supplies a DOI for a file with no identifier, no
  service holds it, and the operator then skips the file
- **THEN** the file is reported skipped with the reason its extraction
  gave, and its `skipped` event carries the DOI as a submission with
  each service's answer

### Requirement: A candidate the operator abandoned leaves nothing behind
A record resolved from a supplied identifier and then abandoned — another identifier supplied, the file skipped, or the run quit — SHALL NOT be reported as the file's resolution, written to the content index, or cited. No sidecar SHALL be written from it and no entry SHALL be merged into the master bibliography from it.

Only the record a file's decision settled on SHALL reach any of those.
A file whose own resolution stands is cited from that record as it
would be in a batch run, whatever candidates were shown along the way.

Where an abandoned record is reported at all, it SHALL be reported only
as evidence: in full, as the outcome of the submission that reached it,
inside the resolution event of the file it was offered for, with that
submission's `acceptance` `rejected`. A run that quits reports nothing
about the file it quit at, and so nothing about that file's candidates.

#### Scenario: A wrong identifier is not written beside the file
- **WHEN** the operator supplies an identifier, sees a record for
  another paper, and supplies a different one instead
- **THEN** no sidecar and no bibliography entry is written from the
  abandoned record, and the file's own sidecar, when it is written,
  carries the record the operator accepted

#### Scenario: A skipped candidate is evidence, not a resolution
- **WHEN** the operator supplies an identifier, sees its record, and
  skips the file
- **THEN** the record appears only in that identifier's submission,
  with `acceptance` `rejected`; no `resolved` event reports it, the
  content index holds nothing new for the file, and no sidecar or
  bibliography entry is written from it

### Requirement: An interactive question shows what the answer rests on
Before an interactive run puts a question about a resolved file, it SHALL show the operator a description of that file comprising, as far as the run knows them: the file's name and its position among the run's files; the identifier used and where it was found; the services that supplied the record; the record's type, title, authors, date of issue, container title, volume, issue and pages; every title the file claims for itself, with where each was read; and the proposed name.

A field the record does not hold, or holds as text that is empty or
only spaces, SHALL be left out rather than shown empty. A field the
record does hold SHALL be shown whether or not its neighbours are: a
volume and pages are reported when the record names no container to
have them.

The identifier SHALL be named with where it was found only where the
run found it. Where the content index answered and nothing was looked
up, the description SHALL name the record's own identifier without
claiming where it came from. No title SHALL be truncated, and no identifier SHALL be broken
across lines: one that cannot be read back in one piece is no longer
evidence.

The file SHALL be named as the run names it elsewhere, so that two
files sharing a name in different directories are told apart by the
question rather than only by their place in the run. When the proposed name carries a
collision suffix, the description SHALL say which rendered name was
taken.

The description SHALL show nothing about the resolution that the file's
`resolved` event does not carry, so that a reader of the stream and the
operator at the terminal are told the same things about a file. What
belongs to the question rather than to the resolution — the file's
position in the run, the proposed name, the note saying which
rendered name a collision suffix stepped around, and that a record
reached from an identifier the operator supplied is still pending their
answer — is the question's to show and is not held to that. A pending
record is a fact about the question being put: the verdict reported
after the answer says what the answer made of it.

The identifier the description names SHALL be the one the run looked
up, with the pass that found it. A record may carry identifiers that
were not looked up, and naming one of those as found would be evidence
the run does not have.

The description SHALL be written where the question is written, so that
a run whose standard output is redirected still asks its questions with
the evidence attached.

Every value taken from a record or from a file's own metadata SHALL be
escaped before it is written, so that a control character is shown
rather than acted on. A file's metadata is written by whoever made the
file; an escape sequence in a title must not be able to redraw the
question the operator is answering.

No line SHALL exceed the width the description is rendered to, except
an identifier, which is never broken.

The description SHALL appear only in interactive runs, and SHALL NOT
change which questions are put or what their answers do.

#### Scenario: Deciding with the record in view
- **WHEN** an interactive run is about to ask whether to rename a file
  resolved through Crossref from a DOI in its text layer
- **THEN** the operator is shown the DOI, that it came from the text
  layer, that Crossref supplied the record, the record's title and
  authors, the titles the file's metadata claims, and the proposed name,
  before the question

#### Scenario: A suffixed proposal
- **WHEN** the proposed name is `smith2024a.pdf` because `smith2024.pdf`
  is taken
- **THEN** the description says that `smith2024.pdf` is taken

#### Scenario: A record from an earlier run
- **WHEN** the content index answers for the file
- **THEN** the description says the identifier comes from an earlier
  run, names the services the record's provenance records, and says no
  title was read from the file

#### Scenario: The record's identifier is not the one that was found
- **WHEN** a file's arXiv identifier is found in its text layer and the
  record resolved for it carries a DOI as well
- **THEN** the description names the arXiv identifier as the one found
  in the text layer

#### Scenario: Redirected output still asks with evidence
- **WHEN** an interactive run's standard output is redirected to a file
- **THEN** each question and its description appear at the terminal,
  and the file receives the event stream

#### Scenario: Two files of one name
- **WHEN** a run spans two directories that each hold `paper.pdf` and
  both are proposed a move
- **THEN** the two questions name their files distinguishably

#### Scenario: A title carrying terminal escapes
- **WHEN** a file's embedded title contains an escape sequence that
  would clear the screen
- **THEN** the description shows the sequence as text and the question
  above it stays on the screen

#### Scenario: A record with no container
- **WHEN** a record holds a volume and pages but names no container
- **THEN** the description reports the volume and the pages

#### Scenario: A cached answer names no origin
- **WHEN** the content index answers for a file
- **THEN** the description names the record's identifier without saying
  where it was found, and says the record comes from an earlier run

#### Scenario: A candidate is described as pending
- **WHEN** an interactive run describes the record a supplied DOI
  reached, before the operator answers
- **THEN** the description says the record is pending, although the
  file's `resolved` event, once the operator renames from it, reports
  it as accepted
