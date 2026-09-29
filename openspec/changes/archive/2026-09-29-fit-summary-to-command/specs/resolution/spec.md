## MODIFIED Requirements

### Requirement: The run summary reports the skip queue
Every run that can skip a file SHALL report each skipped file with its reason, in both human and JSON output modes, and SHALL end with a summary counting the skips.

The listing is the skip queue as it happened: a `skipped` event for
each reason a file was skipped, written as a line of its own in the
human output and as a JSON object in the JSON output. A file is
usually skipped once, but not always — a `bib` run whose sidecar write
fails and whose master merge then fails too skips that file twice, once
for each. The count is the `skipped` total of `run-finished`, which
counts those events rather than distinct files, and which the human
output states in the
closing summary line of each command that can skip — `resolve`,
`rename` and `bib`. A command that cannot skip a file, such as
`status`, owes no skip queue and no summary line for one; what its
human output closes with is the `cli` requirement "A run's human
summary fits its command".

#### Scenario: Summary after a mixed batch
- **WHEN** a batch resolves 8 files and skips 2
- **THEN** the summary lists the 2 skipped files with their reasons, and
  the JSON stream contains one skip event per skipped file, and the
  closing summary line of the human output counts 2 skipped
