## MODIFIED Requirements

### Requirement: borax reports a library it has never seen
`borax status` SHALL report a library without any preceding
initialization, import or ingestion step, naming at minimum the number
of artifacts in the tree, the number of items in the item store, the
number of artifact records, and the number of orphans. A library borax
has never seen SHALL be reported exactly as one it wrote itself:
nothing in the count depends on the files having entered through borax.

`borax status` SHALL open no document to produce those counts. They come
from the tree and from the text store, so the cost of the command is a
directory walk and a read of the store rather than a pass over the
artifacts.

`borax status --identify` SHALL additionally report how many artifacts
an identifier can be extracted from, running the same extraction passes
`resolve` runs and querying no service. It is the same command asked for
more rather than a separate operation, and the extra cost is the pass
over the files that the count requires.

`borax status --identify` SHALL also report, for each artifact it
counts, what extraction made of that artifact: the result the
`extraction` capability defines, which is the identifier and the pass
that found it, or the way extraction failed. The artifacts the count is
taken over are the selection, and reporting on them SHALL NOT widen it.
A file the count leaves out, such as one beneath a nested library, a
symlink, a citation sidecar, or a file in the item store or under
`.borax/`, is neither opened nor reported.

Each artifact's result SHALL be a `library-extraction` event carrying
the artifact's library-relative, `/`-separated path and its result,
written when that artifact's extraction is done rather than once the
pass over the library is over. Every `library-extraction` event of a
run SHALL precede its `library-status` event, which remains the last
event before `run-finished`. The identifiable count SHALL equal the
number of that run's `library-extraction` events reporting an
identifier, so the totals and the per-file results cannot disagree.
The event is an addition and does not change the event schema version.

The human rendering SHALL write one line per artifact, naming its path
and either the identifier and the pass that found it or the failure.
Every value on that line that comes from the file, its name or the PDF
reader SHALL have its control characters written out rather than sent
to the terminal. The report line keeps its wording and remains the
last line of human output.

An extraction result SHALL NOT be a skip or a finding. `run-finished`
counts nothing for it, `status` gains no summary line, and the run's
exit status does not depend on what extraction found.

#### Scenario: Two hundred files borax has never seen
- **WHEN** `borax status` runs over a marked directory holding 200 PDFs,
  no items and no artifact records
- **THEN** it reports 200 artifacts, 0 items, 0 artifact records and 200
  orphans, having been given no command before it

#### Scenario: What is identifiable is asked for
- **WHEN** `borax status --identify` runs over the same directory
- **THEN** it additionally reports how many of the 200 artifacts yield
  an identifier, and no service is queried; each of the 200 artifacts
  is reported once with its own result, before the report

#### Scenario: Status opens nothing by default
- **WHEN** `borax status` runs over a library whose artifacts cannot be
  read
- **THEN** the counts are reported and no artifact is opened, so an
  unreadable document costs the command nothing

#### Scenario: Each artifact is reported with what extraction found
- **WHEN** `borax status --identify` runs over a library holding
  `a.pdf`, whose XMP packet carries a DOI, and `sub/b.pdf`, whose
  metadata carries no identifier and whose first page prints an arXiv
  identifier
- **THEN** a `library-extraction` event reports `a.pdf` with the DOI
  and the `embedded-metadata` pass, another reports `sub/b.pdf` with
  the arXiv identifier and the `text-layer` pass, both precede the
  `library-status` event, and the report counts 2 identifiable

#### Scenario: A blank page with an embedded title has no text layer
- **WHEN** `borax status --identify` runs over a library holding a PDF
  whose only page is blank and whose metadata carries a title, with no
  identifier in the title or anywhere else in the metadata
- **THEN** the PDF is reported as `no-text-layer`, it does not count as
  identifiable, and the run exits 0

#### Scenario: Readable text without an identifier is told apart
- **WHEN** `borax status --identify` runs over a library holding a PDF
  whose page holds readable prose and no identifier, and whose metadata
  carries a title, with no identifier in the title or anywhere else in
  the metadata, beside the blank-page PDF of the previous scenario
- **THEN** the prose PDF is reported as `text-without-identifier` and
  the blank-page PDF as `no-text-layer`, so the two results differ, and
  neither counts as identifiable

#### Scenario: Encrypted and unreadable artifacts are told apart
- **WHEN** `borax status --identify` runs over a library holding a PDF
  encrypted under a user password and a PDF truncated so that it
  cannot be parsed
- **THEN** the first is reported as `encrypted` and the second as
  `unreadable` carrying the reader's message, and neither is reported
  as a file without an identifier

#### Scenario: Only the counted artifacts are inspected
- **WHEN** `borax status --identify` runs over a library holding
  `paper.pdf`, its `paper.pdf.bib` sidecar, a symlink to a PDF, and a
  nested library holding PDFs
- **THEN** exactly one `library-extraction` event is written, for
  `paper.pdf`, and no other file is opened

#### Scenario: Failed extraction is not a partial run
- **WHEN** `borax status --identify` runs in human mode over a library
  in which no artifact yields an identifier
- **THEN** each artifact's failure is on a line of its own, the last
  line is the report counting 0 identifiable, `run-finished` counts no
  skip and no finding, and the run exits 0

#### Scenario: A file's text cannot drive the terminal
- **WHEN** `borax status --identify` runs in human mode over an
  artifact that cannot be parsed and whose reader message carries an
  escape character
- **THEN** the human line writes that character as `\x1b`, and the
  `library-extraction` event in `--json` output carries the message
  unchanged
