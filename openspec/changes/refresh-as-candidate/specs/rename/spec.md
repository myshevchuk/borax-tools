## ADDED Requirements

### Requirement: An interactive run asks whether a differing refresh candidate replaces the library's record
An interactive rename run given `--refresh` SHALL put a question to the operator about each file whose refresh candidate differs from the run's own library item, before any other question about that file, wherever the run records into its library and the operator has not yet answered for that item in the run.

The question SHALL be put whatever `rename.skip-named` says, and to an
already-named file as to any other. Its choices SHALL be, in this
order: keep the library's record, which is the default; replace it with
the fresh record; and quit. The default SHALL never be the answer that
writes over the library's record. The question names the file itself,
since no move is in play yet.

Its description SHALL be the file's `resolved` event with the
candidate's acceptance `pending`, and SHALL name no new name, since the
name depends on the answer. It SHALL show the refresh lines the
requirement "An interactive question shows what the answer rests on"
gives: what the fresh lookup came to, each field that differs with
both values, and that keeping leaves the library's record as it is.

Keeping SHALL write nothing and report the candidate `declined`. The
file then goes on to its own situation with the item's record.

Replacing SHALL write the replacement at once, as the `library`
capability's requirement "An item's record is replaced only when an
operator accepts a refresh candidate" states, and report it then.

- When the write lands, the candidate is `accepted`. The file goes on
  with the item's record as replaced: it is named and cited from it, and
  it is reported as its record. The library the run consults and the
  account its duplicate checks read SHALL both hold the item as
  replaced from then on.
- When the write fails or is refused, the candidate is `unwritten` with
  the reason. The file goes on with the item's record as the run read
  it, and the next question's description says why.

Either way the file is then put the question its situation calls for,
which may be a move to the name the replaced record renders. An
already-named file under `rename.skip-named` is settled with no further
question.

Quitting at this question SHALL end the run as any quit does, and SHALL
write nothing. A replacement made at this question stands if the run is
then quit at a later question about the same file.

An item SHALL be asked about once in a run. A later file answered by an
item whose question was already put SHALL NOT be asked about it again:

- after a replacement it is consulted against the item as replaced;
- after a keep, or a replacement that did not land, a candidate that
  still differs is reported with its acceptance not attempted, reason
  `already-asked`.

Retry SHALL NOT be offered for a refresh's lookup. A file the library
answers for made no lookup of its own, and a refresh that found nothing
costs the file nothing, since the item stands. Supplying an identifier
is not offered at this question. At the file's own question it
re-identifies the file as ever, and a replacement already made stands.

A differing candidate the run does not ask about is reported in the
file's verdict and described in its own question, if it is put one:

- a run that records nothing reports it with reason `not-recording`;
- another library's item reached through a reference, with reason
  `another-library`.

Nothing is written for either.

#### Scenario: Keeping the library's record
- **WHEN** an interactive `borax rename --refresh` reaches a tracked,
  already-named file whose item's title was corrected by hand and
  differs from Crossref's, and the operator answers keep
- **THEN** the question was put although `rename.skip-named` is on, the
  item file is byte-identical, the file is reported already named, and
  its `resolved` event reports the candidate `declined`

#### Scenario: Replacing it
- **WHEN** the operator answers replace instead
- **THEN** a `library-replacement` event reports the item's title
  written, the item file differs in its title alone, and the file's
  `resolved` event carries the replaced record with the candidate
  `accepted`

#### Scenario: A replaced record renders another name
- **WHEN** the filename template renders the title, and the operator
  replaces the item's record
- **THEN** the next question proposes the name the replaced record
  renders

#### Scenario: Quitting after replacing
- **WHEN** the operator replaces the item's record and then quits at the
  question about the file's move
- **THEN** the item stays replaced, the `library-replacement` event is
  in the stream and the run log, the file keeps its name, and
  `run-finished` counts it unreached

#### Scenario: One item, two files
- **WHEN** an item has two artifacts in the run and the operator
  replaces its record at the first
- **THEN** no question is put about the item at the second, which is
  consulted against the record as replaced

#### Scenario: A run that records nothing
- **WHEN** `borax rename --refresh --no-record` runs interactively over
  a file whose candidate differs
- **THEN** no question is put about the candidate, it is reported with
  reason `not-recording`, and nothing is written

#### Scenario: Another library's item
- **WHEN** an interactive run reaches a file a reference to another
  library answers for, and its candidate differs
- **THEN** no question is put about the candidate, and the file's own
  question describes it with that library's root

#### Scenario: Nothing found, nothing offered
- **WHEN** an interactive run given `--refresh` reaches a tracked,
  already-named file while every service is unreachable
- **THEN** no question is put, no retry is offered, and the file is
  shown rather than passed over

## MODIFIED Requirements

### Requirement: Quitting an interactive run leaves the rest untouched
Answering quit, or interrupting a question, SHALL end an interactive run at that file: the file and every file after it SHALL be left untouched, nothing further SHALL be resolved, and the run SHALL close with `run-finished` as a completed run does.

`run-finished` SHALL count as unreached the file the run ended at and
every file after it, since none of them was given a fate. A run that
leaves any file unreached SHALL exit with the partial-success code.

Work already done SHALL stand: files moved before the quit stay moved
and recorded, and bibliography output for the files that were visited is
written as it would have been had the run finished. So does a
replacement of a library item's record the operator accepted at an
earlier question about the file the run ended at: its
`library-replacement` event was reported when the write was made, and
the file is still counted unreached, with nothing else reported about
it.

#### Scenario: Quit midway
- **WHEN** the operator quits at the third of ten files, having renamed
  the first two
- **THEN** the first two stay renamed, the other eight keep their names,
  `run-finished` counts eight unreached, and the exit code is the
  partial-success code

#### Scenario: Interrupt at a prompt
- **WHEN** the operator presses Ctrl-C while a question is showing
- **THEN** the run ends as it would on quit, closing with `run-finished`

#### Scenario: Quit after replacing an item's record
- **WHEN** the operator replaces a file's item record at its refresh
  question and quits at the question about its move
- **THEN** the item stays replaced, the stream carries its
  `library-replacement` event and no `resolved` event for the file, and
  `run-finished` counts the file unreached

### Requirement: An interactive run passes over already-named files
An interactive rename run SHALL, when the `rename.skip-named` setting is on, render nothing to the operator for a file that is already named and has nothing to report about a refresh: no resolution line, no outcome line and no question. The setting SHALL default to on, and `--no-skip-named` SHALL turn it off for a run.

A passed-over file SHALL still be reported in the event stream and the
run log exactly as when the setting is off, and the run's closing
summary SHALL state how many files were passed over.

With the setting off, an interactive run SHALL render already-named
files as a batch run does, and SHALL put the question an already-named
file is owed — keep its name, supply a different identifier, or quit —
so that a file named from the wrong record can be put right. The
setting SHALL NOT affect batch runs.

In a run given `--refresh`, a file has nothing to report about a
refresh when it got none, or its refresh found its library item
current. A file whose refresh found a differing candidate, found
nothing, or had no identifier to ask about SHALL be rendered as the
setting off renders it, and SHALL NOT be counted among the files
passed over. It is put the refresh question where the requirement "An
interactive run asks whether a differing refresh candidate replaces
the library's record" puts it, and with the setting on it is still not
put the question about its name.

#### Scenario: New papers among renamed ones
- **WHEN** an interactive run reaches a directory holding three files
  that need renaming and two hundred already named
- **THEN** the operator sees the three questions and no line about the
  other two hundred, and the summary reports two hundred already named

#### Scenario: Showing named files
- **WHEN** the same run is started with `--no-skip-named`
- **THEN** each already-named file is shown as already named, and is
  asked about

#### Scenario: The run log keeps the full account
- **WHEN** an interactive run passes over an already-named file
- **THEN** its run log carries that file's resolution and its
  `already-named` event

#### Scenario: A refresh to report is shown
- **WHEN** an interactive run given `--refresh`, with
  `rename.skip-named` on, reaches an already-named tracked file while
  every service is unreachable
- **THEN** its lines are shown, no question is put about its name, and
  the summary does not count it among the files not shown

#### Scenario: A current item is still passed over
- **WHEN** the same run reaches an already-named tracked file whose
  refresh finds its item current
- **THEN** nothing about it is shown, and it is counted among the files
  not shown

### Requirement: An interactive question shows what the answer rests on
Before an interactive run puts a question about a resolved file, it SHALL show the operator a description of that file comprising, as far as the run knows them: the file's name and its position among the run's files; the identifier used and, where it was looked up, where it was found; the services that supplied the record; the record's type, title, authors, date of issue, container title, volume, issue and pages; every title the file claims for itself, with where each was read; and the proposed name.

A field the record does not hold, or holds as text that is empty or
only spaces, SHALL be left out rather than shown empty. A field the
record does hold SHALL be shown whether or not its neighbours are: a
volume and pages are reported when the record names no container to
have them.

The identifier SHALL be named with where it was found only where the
run found it, or where the content index held it for the file's bytes.
The description SHALL then name it as an earlier run's: found by an
earlier run, supplied in an earlier run, or accepted over a title
conflict in an earlier run, as the origin the content index kept it
under says, and SHALL NOT name a pass of this run. Where the library
answered and nothing was looked up, the description SHALL name the
record's own identifier without claiming where it came from. The same
holds where a library reference in the content index answered: nothing
was looked up, and the identifier is the record's own.

Where a library reference answered, the description's `record` line
SHALL name the services the item's provenance records followed by `from
the library at <root>`, with the reference's root, or `the library at
<root>` alone where the provenance names no service. Where the file's
entry held references that could not answer, the description SHALL
carry one line labelled `reference` for each, after any `library` line,
stating what the file's human output line states after `a library
reference could not answer: `. Where references disagreed, it SHALL
carry one line labelled `references`, stating `disagree: ` followed by
what that line states after `library references disagree: `. The
description of a file that was skipped carries the same lines, after
its reason and any `library` line. Like every other line, these render
what the file's resolution event carries. No title SHALL be truncated, and no identifier SHALL be broken
across lines: one that cannot be read back in one piece is no longer
evidence.

Where the file's `resolved` event carries a `refresh`, the description
SHALL carry, after the titles the file claims and before any conflict,
a line labelled `refresh` saying what the fresh lookup came to: the
identifier looked up, written whole, then whether the service that
answered differs from the library's record or does not, or each
service's answer where none supplied a record, or that no configured
service can be asked about it; or, for an item with no identifier, that
the library's record carries none to ask about. A candidate that
differs SHALL be followed by one line labelled `differs` per field,
giving the field's key, its value in the library and its value from
the service, with every author named where the field is the authors,
and `nothing` for a value a record lacks, and then by one line
labelled `decision` saying what became of the candidate, or, while it
is pending, that keeping leaves the library's record as it is.

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
answer, as is a refresh candidate the question asks about — is the
question's to show and is not held to that. A pending
record is a fact about the question being put: the verdict reported
after the answer says what the answer made of it.

Where the run looked an identifier up, the identifier the description
names SHALL be the one the run looked up, with the pass that found it.
Where nothing was looked up, because the library or a library reference
answered, it SHALL be the record's own identifier, named without where
it was found, as above. A record may carry identifiers that were not
looked up, and naming one of those as found would be evidence the run
does not have.

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
- **WHEN** the content index answers for the file with the DOI an
  earlier run extracted from it, no library reference for its bytes
  answering, and Crossref's response cache holds that DOI's record
- **THEN** the description names the DOI as found by an earlier run,
  names Crossref as the service that supplied the record, and says no
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
- **WHEN** the content index answers for a file with its evidence, no
  library reference for its bytes answering
- **THEN** the description names the identifier it looked up without
  naming an extraction pass of this run as where it was found, and
  says it comes from an earlier run

#### Scenario: An operator's answer from an earlier run
- **WHEN** the content index answers for a file with a DOI an operator
  supplied in an earlier run, no library reference for its bytes
  answering
- **THEN** the description names the DOI as supplied in an earlier run

#### Scenario: A reference answer described
- **WHEN** an interactive run asks about a file outside any library
  that a reference to the library at `/home/u/papers` answered for,
  whose item's provenance names Crossref
- **THEN** the description names the item's DOI without saying where it
  was found, and its `record` line reads `Crossref, from the library at
  /home/u/papers`

#### Scenario: A reference that could not answer described
- **WHEN** an interactive run asks about a file whose entry held a
  reference to `/media/old`, which no longer exists, and whose evidence
  answered
- **THEN** the description carries a `reference` line saying
  `/media/old is missing`

#### Scenario: A candidate is described as pending
- **WHEN** an interactive run describes the record a supplied DOI
  reached, before the operator answers
- **THEN** the description says the record is pending, although the
  file's `resolved` event, once the operator renames from it, reports
  it as accepted

#### Scenario: A differing candidate described
- **WHEN** an interactive run given `--refresh` asks about a tracked file
  whose item's title differs from Crossref's
- **THEN** the description carries a `refresh` line with the DOI, a
  line saying Crossref differs from the library's record, a `differs`
  line giving the title in the library and from Crossref, and a
  `decision` line saying the candidate is pending

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
asked about its name, and it is asked only when that setting is off; a
run passing over named files asks nothing about their names. A run
given `--refresh` may first ask about an already-named file's library
item, whatever the setting, as the requirement "An interactive run
asks whether a differing refresh candidate replaces the library's
record" states.

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
about the file it quit at, and so nothing about that file's candidates,
beyond the replacement of its library item's record an operator made at
an earlier question about it, which was reported when it was written.

A refresh candidate the operator declined is evidence of the same kind:
it is reported only under the file's `resolved` event's `refresh`, and
nothing is written, cited or indexed from it.

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

#### Scenario: A declined refresh candidate is evidence
- **WHEN** the operator keeps the library's record over a differing
  refresh candidate
- **THEN** the candidate appears only in the file's `resolved` event,
  under `refresh`, and no item, sidecar, bibliography entry or
  content-index fact is written from it

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
over under `rename.skip-named` as any already-named file is. A run given
`--refresh` is the exception: a file whose item's candidate differs is
first asked about the item, and one with a refresh to report is not
passed over, as the requirements "An interactive run asks whether a
differing refresh candidate replaces the library's record" and "An
interactive run passes over already-named files" state.

#### Scenario: Deciding on a library answer
- **WHEN** an interactive run is about to ask whether to rename a
  tracked file whose item's provenance names Crossref
- **THEN** the description names the item's DOI without saying where it
  was found, its `record` line reads `Crossref, from the library`, and
  it says nothing was read from the file

#### Scenario: A library that could not answer
- **WHEN** an interactive run is about to ask about a file whose record
  links to an item the library does not hold, and which resolved from
  the content index instead, by the identifier an earlier run extracted
- **THEN** the description carries a `library` line naming the artifact
  and the missing item, and its identifier line says the identifier
  comes from an earlier run

#### Scenario: A file nothing identified, whose library could not answer
- **WHEN** an interactive run offers to supply an identifier for a file
  that carries none and whose record links an item the library does not
  hold
- **THEN** the description carries a `library` line naming the artifact
  and the missing item after the reason the file was not identified

#### Scenario: A tracked file already named is passed over
- **WHEN** an interactive run with `rename.skip-named` on, not given
  `--refresh`, reaches a tracked file that already carries the name its
  item's record implies
- **THEN** nothing is shown or asked about it, and its `resolved` and
  `already-named` events are in the run log
