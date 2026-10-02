## ADDED Requirements

### Requirement: A resolution keeps extraction's result and the file's titles
A resolution that runs extraction over a file SHALL retain extraction's result as one of the kinds the requirement "Extraction reports one result for each file it is given" defines, and SHALL NOT merge two of those kinds, whatever skip reason the resolution reports.

The result retained by a resolution SHALL be the result that
`borax status --identify` reports for the same file under the same
extraction settings, since both run the same passes.

A resolution SHALL retain the titles the file claims separately from
extraction's result, in exactly one of three states:

- **read**: every title read, each with where it was read, in the
  order read. There MAY be none, which means the file was opened and
  claims no title.
- **failed**: the file could not be opened, retained with the reason.
- **not attempted**: the file was not opened, retained with the reason.

Titles that were read SHALL be retained when extraction then finds no
identifier, or fails while reading the file's pages. A file without an
identifier therefore still carries the titles it claims.

The titles are evidence about the work. Whether a title counts against
a record is the title check's to decide, under the `resolution`
capability. A title still affects extraction's result only through an
identifier the metadata pass finds in it, as before.

#### Scenario: A blank page and readable prose, both titled
- **WHEN** a resolution runs over two PDFs whose document information
  carries the title `A Title` and no identifier, one with a single
  blank page and one with a page of readable prose that holds no
  identifier
- **THEN** the first retains `no-text-layer` and the second
  `text-without-identifier`, both are skipped as before, and both
  retain the titles as read, holding `A Title` read from the document
  information dictionary

#### Scenario: Encrypted is not unreadable
- **WHEN** a resolution runs over a PDF encrypted under a user password
  and over a PDF truncated so that it cannot be parsed
- **THEN** the first retains `encrypted` and the second `unreadable`
  with the reader's message, and both retain their titles as failed,
  each with its reason

#### Scenario: A page that cannot be read after the file opens
- **WHEN** a PDF opens, its document information carries a title, and
  its first page cannot be read
- **THEN** the resolution retains `unreadable` as the result, and the
  title as read

#### Scenario: A file that claims no title
- **WHEN** a PDF with no title in its metadata holds readable prose and
  no identifier
- **THEN** the resolution retains `text-without-identifier`, and the
  titles as read with none, which is distinct from titles that failed
  and from titles not attempted

#### Scenario: Titles not read because the file was not opened
- **WHEN** a file is resolved from the content index
- **THEN** its titles are retained as not attempted because the content
  index answered, not as read with none
