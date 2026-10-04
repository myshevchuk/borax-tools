## MODIFIED Requirements

### Requirement: JSON Lines output is first-class
Every subcommand SHALL support `--json`, emitting one JSON object per line
to stdout. Each event SHALL carry an event type and a schema version.

The schema version SHALL change whenever a consumer that reads the
stream correctly today could read a later stream wrongly: an event or a
reason that is removed or renamed, or a field whose meaning changes. It
SHALL NOT change for an addition, which a consumer that ignores what it
does not know reads unchanged. Before `1.0.0` such changes are
permitted in any release, and the version is how a consumer is told one
happened rather than a promise that none will. The version separates schemas
as releases carry them: a schema version is fixed by the first release
that emits it. Until then, no consumer can have read a released stream
of it, and its events SHALL be free to change, removals and changes of
meaning included, without a further bump. Human-readable output
and JSON output SHALL be renderings of the same event stream, and
diagnostics SHALL go to stderr so stdout stays machine-parseable.

#### Scenario: Machine-readable run
- **WHEN** `borax rename --json` processes a batch
- **THEN** stdout contains only well-formed JSON Lines (per-file events
  plus a summary event) and any progress or warnings appear on stderr

#### Scenario: A removed reason changes the version
- **WHEN** a release stops emitting a skip reason a consumer could have
  been counting
- **THEN** the schema version every event carries is higher than the
  one before it

#### Scenario: An unreleased schema changes without a bump
- **WHEN** a value's meaning changes between two builds, and no release
  has yet emitted the schema version both carry
- **THEN** the version stays the same, and the first release to emit it
  carries the changed meaning
