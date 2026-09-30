## ADDED Requirements

### Requirement: An interactive question says whether the library answered
The description an interactive run shows before a question SHALL say when the file's record is its library item's, and SHALL state what the library could not answer for when it could not.

Where the file's `resolved` event reports `library` as its `tier`, the
description SHALL name the record's own identifier without claiming
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
event: it shows what that event's `library` and `tier` carry, and
nothing the event does not. What the questions offer and what their
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
