## MODIFIED Requirements

<!-- drops: the schema-3 note that the `unresolvable` reason's identifier and pass were additions; schema 4 moves them into the sections -->
### Requirement: A question describes whichever verdict it is asking about
A question put about a file SHALL be preceded by a description of the verdict the run is holding for it, whether that verdict is a resolution or a failure, and SHALL show only what that verdict's own event carries.

A description of a file no service could supply a record for SHALL name
the identifier that was looked up and what each service answered, one
service to a line, in the order they were asked. A description of a
conflict SHALL show both titles and how close they were, whether the
conflict is being asked about or was accepted. An identifier the
operator supplied SHALL be shown as supplied rather than as the find of
an extraction pass.

So that the first of these is possible without the description knowing
more than the stream does, a resolution's `skipped` event SHALL carry
the identifier that was looked up, where it came from, and the
services' answers in its `lookup` section, and the extraction pass that
read it in its `extraction` section, rather than in its reason.

#### Scenario: Asked to supply an identifier for a file nobody holds
- **WHEN** an interactive run reaches a file whose arXiv identifier no
  service holds, and puts the question that offers to supply another
- **THEN** the description names that identifier, where it was read, and
  what each service answered

#### Scenario: The log says which identifier failed
- **WHEN** a batch run skips a file because no service holds its
  identifier
- **THEN** the `skipped` event's `lookup` section carries that
  identifier as well as the services' answers, and its reason is
  `{"kind": "unresolvable"}`
