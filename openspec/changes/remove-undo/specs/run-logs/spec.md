## MODIFIED Requirements

<!-- drops: the undo rationale; the fallback now stands on the record's
     own merit, and the scenario no longer asserts a later revert -->

### Requirement: Apply-run logs fall back to XDG state outside a collection
An apply run on files outside any collection root SHALL write its run
log under the XDG state directory instead, so that an applied rename
leaves a readable record of what it moved wherever the files were.

#### Scenario: Apply in a downloads directory
- **WHEN** an apply run renames files in a directory with no
  `.borax.toml` above it
- **THEN** the run log is written under the XDG state directory,
  carrying the original path, the new path and the content hash of
  every rename the run applied
