## MODIFIED Requirements

<!-- drops: `undo` leaves the subcommand list with the command itself -->

### Requirement: Single binary with subcommands
The suite SHALL ship as one binary, `borax`, with at minimum the
subcommands `resolve` (extract + resolve, emit records), `rename` (full
pipeline: resolve, plan, preview/apply), `bib` (emit/merge bibliography
output for already-resolved files), `config` (show effective
configuration), `cache` (inspect and clear the response cache), and
`ledger` (rebuild the collection's record of what it has admitted).

#### Scenario: Pipeline via one command
- **WHEN** `borax rename --apply <dir>` runs
- **THEN** extraction, resolution, planning, renaming, and configured
  bibliography output all occur in that single invocation
