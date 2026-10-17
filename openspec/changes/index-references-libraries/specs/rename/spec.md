## MODIFIED Requirements

### Requirement: An interactive question shows what the answer rests on
Before an interactive run puts a question about a resolved file, it SHALL show the operator a description of that file comprising, as far as the run knows them: the file's name and its position among the run's files; the identifier used and where it was found; the services that supplied the record; the record's type, title, authors, date of issue, container title, volume, issue and pages; every title the file claims for itself, with where each was read; and the proposed name.

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
- **WHEN** the content index answers for the file with the DOI an
  earlier run extracted from it, and Crossref's response cache holds
  that DOI's record
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
- **WHEN** the content index answers for a file with its evidence
- **THEN** the description names the identifier it looked up without
  naming an extraction pass of this run as where it was found, and
  says it comes from an earlier run

#### Scenario: An operator's answer from an earlier run
- **WHEN** the content index answers for a file with a DOI an operator
  supplied in an earlier run
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
