## ADDED Requirements

### Requirement: An interactive question shows what the answer rests on
Before an interactive run puts a question about a resolved file, it SHALL show the operator a description of that file comprising, as far as the run knows them: the file's name and its position among the run's files; the identifier used and where it was found; the services that supplied the record; the record's type, title, authors, date of issue, container title, volume, issue, pages and publisher; every title the file claims for itself, with where each was read; and the proposed name.

A field the record does not hold SHALL be left out rather than shown
empty. No title SHALL be truncated. When the proposed name carries a
collision suffix, the description SHALL say which rendered name was
taken.

Every fact in the description other than the proposed name and its
collision note SHALL be a rendering of the file's `resolved` event, so
that the description shows nothing about the resolution that the event
stream does not carry.

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
