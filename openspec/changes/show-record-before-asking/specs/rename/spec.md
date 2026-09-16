## ADDED Requirements

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
position in the run, the proposed name, and the note saying which
rendered name a collision suffix stepped around — is the question's to
show and is not held to that.

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
