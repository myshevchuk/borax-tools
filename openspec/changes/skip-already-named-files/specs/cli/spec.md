## MODIFIED Requirements

### Requirement: A subcommand accepts only the settings it consumes
Each subcommand SHALL declare exactly the setting flags that are
operative for it — those whose value can change what that command
reports, writes or moves — so that `--help` for a subcommand describes
that subcommand and an invocation naming an inoperative setting is
refused as an unknown argument rather than accepted and silently
dropped. A setting that cannot change a command's outcome SHALL NOT be
able to end that command's run.

The criterion is effect on the outcome, not whether a value is touched
during setup. Shared preparation a run performs before dispatch may
read a setting on behalf of a command that will not use the result;
that is not what makes a setting operative, and this requirement does
not govern it.

A setting flag SHALL follow its subcommand. `--json` is the exception
and remains accepted before or after, because every subcommand renders
its stream through it.

The surface is: `resolve` accepts the resolution, extraction, network
and response-cache settings, and how many files may be resolved at once;
`rename` accepts those, minus how many files at once, plus the collision
policy, the bibliography settings, the ledger gate, the batch pair, the
skip-named pair, and `--apply`; `bib`
accepts the resolution settings and the bibliography settings; `cache`
accepts `--clear`; `ledger rebuild` accepts no setting of its own. Every
subcommand additionally accepts the run-log pair, which is decided at
dispatch and is therefore operative on all of them.

`config` accepts every configurable setting. Passing an override there
is not a no-op but the question the command answers — what this
invocation would resolve to, and from which layer — so restricting it
would remove the only way to ask.

Configuration is unaffected: a configuration file and an environment
variable SHALL continue to set every key regardless of which subcommand
runs, since neither is an argument to an invocation.

#### Scenario: Inapplicable setting is refused
- **WHEN** `borax cache --no-cache` runs
- **THEN** the invocation is rejected as an unknown argument, nothing is
  read or cleared, and the exit code is the fatal one

#### Scenario: Inert setting is refused rather than ignored
- **WHEN** `borax rename --concurrency 8 <dir>` runs
- **THEN** the invocation is rejected as an unknown argument, rather than
  accepted and resolving one file at a time regardless

#### Scenario: Subcommand help lists that subcommand's settings
- **WHEN** `borax ledger rebuild --help` runs
- **THEN** the settings listed are the run-log pair and `--json`, and no
  extraction, resolution, rename, bibliography or ledger-gate setting
  appears

#### Scenario: Setting flag follows its subcommand
- **WHEN** `borax --mailto me@example.org rename f.pdf` runs
- **THEN** the invocation is rejected as an unknown argument naming
  `--mailto`, and `borax rename --mailto me@example.org f.pdf` is the
  accepted form

#### Scenario: Configuration still sets what a flag no longer offers
- **WHEN** a `.borax.toml` sets `network.concurrency` and `borax rename`
  runs under it
- **THEN** the run proceeds, `borax config` reports that value with the
  file as its origin, and nothing is refused

#### Scenario: The batch pair belongs to rename
- **WHEN** `borax rename --batch papers/` and `borax config --no-batch`
  run
- **THEN** both are accepted, and `borax resolve --batch papers/` is
  rejected as an unknown argument

#### Scenario: The skip-named pair belongs to rename
- **WHEN** `borax rename --no-skip-named papers/` runs
- **THEN** it is accepted, and `borax bib --no-skip-named papers/` is
  rejected as an unknown argument

### Requirement: A run reports as it goes
Each event SHALL be written to stdout at the moment it occurs, rather
than accumulated and rendered once the run is over. A run whose work is
network-bound therefore shows progress while it is bound, and a reader
can tell a slow run from a stopped one without waiting for it to end.

This constrains when a line is written, not what it says: the event
schemas are unchanged, and human and JSON output remain two renderings
of the same stream in the same order.

An interactive run MAY hold what it shows about one file until that
file's planning outcome is known, which is what lets it pass over a
file that needs no decision without having already spoken about it.
The hold covers one file, ends before that file's question is put or
its fate is reported, and never spans a wait on the network or on the
operator. What is written to the run log and to a `--json` stream is
not held.

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

### Requirement: JSON Lines output is first-class
Every subcommand SHALL support `--json`, emitting one JSON object per line
to stdout. Each event SHALL carry an event type and a schema version.

The schema version SHALL change whenever a consumer that reads the
stream correctly today could read a later stream wrongly: an event or a
reason that is removed or renamed, or a field whose meaning changes. It
SHALL NOT change for an addition, which a consumer that ignores what it
does not know reads unchanged. Before `1.0.0` such changes are
permitted in any release, and the version is how a consumer is told one
happened rather than a promise that none will. Human-readable output
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

