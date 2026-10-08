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
policy, the bibliography settings, the record gate, the batch pair, the
skip-named pair, and `--apply`; `bib`
accepts the resolution settings and the bibliography settings; `cache`
accepts `--clear`; `status` accepts the extraction settings and
`--identify`; `validate` accepts no setting of its own; `reconcile`
accepts the extraction settings, `--rehash` and `--accept`, since what
it accepts as a changed artifact depends on what extraction reads;
`adopt` accepts no setting of its own, since it
queries nothing, opens no document and renames nothing. Every
subcommand additionally accepts the run-log pair, which is decided at
dispatch and is therefore operative on all of them.

`--identify`, `--rehash` and `--accept` are per-invocation selectors
rather than settings: each says what this run is being asked to do
rather than how borax behaves, so none is settable from configuration
and none takes a `--no-` form. `--accept` names the paths whose pending
changes this run accepts, and is given once per path.

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
- **WHEN** `borax validate --help` runs
- **THEN** the settings listed are the run-log pair and `--json`, and no
  extraction, resolution, rename, bibliography or record-gate setting
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

#### Scenario: A selector is not a setting
- **WHEN** a configuration file sets `identify = true`
- **THEN** the run aborts at config load naming the unknown key, and
  `borax status --identify` is the accepted form

#### Scenario: Reconcile reads documents as status does
- **WHEN** `borax reconcile --page-limit 3` runs
- **THEN** it is accepted, and the extraction it makes of a changed
  artifact reads at most three pages

#### Scenario: Accepting is a selector
- **WHEN** a configuration file sets `accept`, and separately `borax
  reconcile --no-accept` runs
- **THEN** the first aborts at config load naming the unknown key, the
  second is rejected as an unknown argument, and `borax reconcile
  --accept a.pdf --accept b.pdf` is the accepted form
