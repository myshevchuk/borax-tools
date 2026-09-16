## MODIFIED Requirements

### Requirement: A run reports as it goes
Each event SHALL be written to stdout at the moment it occurs, rather
than accumulated and rendered once the run is over. A run whose work is
network-bound therefore shows progress while it is bound, and a reader
can tell a slow run from a stopped one without waiting for it to end.

This constrains when a line is written, not what it says: the event
schemas are unchanged, and human and JSON output remain two renderings
of the same stream in the same order.

An interactive run MAY hold what it says about one file until that
file's fate is settled: that is what lets it pass over a file needing
no decision without having already spoken about it, and what lets a
file the operator re-identifies be reported once rather than twice.

The hold covers one file and ends when that file's fate does. What the
operator is shown while deciding is the question and its description,
which are not the stream, so a run never sits waiting on an answer
with something unsaid that it would have said had nobody been asked.
What is written to the run log and to a `--json` stream is not held.

The framing is unchanged. A run that starts SHALL open with
`run-started` and close with `run-finished`; a run ended by a
configuration or usage error SHALL emit neither, so a consumer still
tells a run that produced nothing from one that never began. Every
check that can end a run this way therefore happens before its first
event.

#### Scenario: A long run shows its progress
- **WHEN** `borax rename` resolves a directory of files against the
  network
- **THEN** each file's lines appear as that file is processed, and not
  only after the last file is done

#### Scenario: A fatal error emits no stream
- **WHEN** a run ends because a template will not compile
- **THEN** stdout carries neither `run-started` nor `run-finished`, the
  reason appears on stderr, and the exit code is the fatal one

#### Scenario: A passed-over file is never half-reported
- **WHEN** an interactive run reaches a file that turns out to be
  already named, with the setting to pass over such files on
- **THEN** nothing about that file has reached the terminal, and the
  file after it is reported as it is reached
