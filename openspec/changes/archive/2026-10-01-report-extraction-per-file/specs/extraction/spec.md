## ADDED Requirements

### Requirement: Extraction reports one result for each file it is given
Extraction over files a caller selects SHALL yield exactly one result
for each file it is given, in the order given, and SHALL NOT open,
report on or add any file it was not given.

The caller decides which files are extracted. Library context the
caller holds — its root, its stores, its survey — MAY shape how a
result is reported, such as the path it is reported under, and SHALL
NOT add a file to the selection or remove one from it.

The result SHALL be exactly one of the following, and no two of them
SHALL be reported as the same result:

- `found`: a pass found an identifier. The result carries the
  identifier in the form the event stream writes one (`doi:…`,
  `arXiv:…`) and the pass that found it (`embedded-metadata` or
  `text-layer`).
- `no-text-layer`: the metadata held no identifier, and no page the
  text-layer pass read held text, including when it read no page
  because the document has none or the page limit is zero.
- `text-without-identifier`: the metadata held no identifier, and the
  text-layer pass read text that held none.
- `encrypted`: the document cannot be read without a password.
- `unreadable`: the file could not be opened or parsed as a PDF. The
  result carries the reason the reader gave.

A title in the document's metadata SHALL count only through an
identifier the embedded-metadata pass recognises in it, as it already
scans the document-information `Title` among its other fields. A title
holding such an identifier gives `found` with `embedded-metadata`. A
title holding none SHALL NOT change which result a file gets: the file
gets the result its other metadata and its text give it.

The passes run are the ones "Tiered extraction stops at the first hit"
and "Text-layer pass scans a bounded page range" define, which are the
passes resolution runs. Producing a result queries no service, reads
and writes no cache, and writes nothing to a library.

A resolving command's skip reasons are not this result, and this
requirement does not govern them.

#### Scenario: One result per file, and no other file
- **WHEN** extraction is given three files in a directory holding five,
  and the second of the three cannot be opened
- **THEN** there are exactly three results, in the order the files were
  given, the second is `unreadable`, and neither of the other two files
  in the directory is opened

#### Scenario: A title holding no identifier changes nothing
- **WHEN** a PDF's only page is blank and its metadata carries a title
  and no identifier, in the title or anywhere else
- **THEN** its result is `no-text-layer`

#### Scenario: An identifier in the title is found by the metadata pass
- **WHEN** a PDF's only page is blank, its metadata carries no
  identifier outside its title, and its document-information title is
  `10.1234/example`
- **THEN** its result is `found` with `doi:10.1234/example` and
  `embedded-metadata`

#### Scenario: Readable text without an identifier
- **WHEN** a PDF's page holds readable prose and no identifier, and its
  metadata carries a title and no identifier
- **THEN** its result is `text-without-identifier`, not `no-text-layer`

#### Scenario: Encrypted is not unreadable
- **WHEN** one PDF is encrypted under a user password and another is
  truncated so that it cannot be parsed
- **THEN** the first result is `encrypted` and the second is
  `unreadable` with the reader's message

#### Scenario: The pass that found an identifier is named
- **WHEN** one PDF carries a DOI in its XMP packet and another carries
  no identifier in its metadata and prints an arXiv identifier on its
  first page
- **THEN** the first result is `found` with that DOI and
  `embedded-metadata`, and the second is `found` with that arXiv
  identifier and `text-layer`
