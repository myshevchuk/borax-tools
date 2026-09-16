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
policy, the bibliography settings, the ledger gate, the batch pair, and
`--apply`; `bib`
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

### Requirement: The apply gate is never configurable
Configuration SHALL NOT be able to set the `--apply` flag or any one-off
destructive selector; such keys in a configuration file are load-time
errors, and no configuration SHALL be able to authorise a move.

Authorisation for a move comes from the command line or from a person:
`--apply` given to a batch run, or an answer given to a question naming
the file and its target. Configuration selects which of the two a run
asks for — that is what `rename.batch` does — and can only ever make a
run move less than the command line asked for.

#### Scenario: apply in config
- **WHEN** a configuration file contains `apply = true`
- **THEN** the run aborts at config load stating the key must be passed
  on the command line

#### Scenario: Configuration cannot authorise a move
- **WHEN** a configuration file sets `rename.batch = false` and
  `borax rename papers/` runs with stdin redirected from /dev/null
- **THEN** the run is a batch preview and moves nothing, because the
  configuration selected a mode and authorised nothing

### Requirement: Configuration resolution order
Configuration SHALL be TOML and resolve with this precedence, highest
first: command-line flags, environment variables, the nearest per-directory
override file (`.borax.toml`, discovered upward from each input file's
directory), the XDG global configuration file, built-in defaults.
`borax config` SHALL print the effective configuration with the origin of
each value.

The override file SHALL be discovered per input file, so one invocation
spanning two directory trees applies each tree's overrides to its own
files and the result does not depend on the order the paths were given.

The settings deciding which services a run queries and how it identifies
itself — `sources`, `mailto`, and the `network` table — SHALL be taken
from the run's own configuration rather than per file, since the clients
are built once before any file is read. `borax config`, which takes no
paths, SHALL print the run's configuration.

`rename.batch` SHALL be taken from the run's own configuration for the
same reason: a run has one session with one operator, decided before its
first event, and a mode that changed between directories would be a run
that asks about some of its files and not others without saying so. A
`.borax.toml` under an input therefore does not select the mode unless
the run was started from within it.

The nearest `.borax.toml` additionally defines the collection root: the
directory containing it anchors the collection's `.borax/` accounting
directory (ledger and run logs); an explicit `collection-root`
configuration key overrides this for unusual layouts.

#### Scenario: Per-directory template override
- **WHEN** a directory tree contains a `.borax.toml` defining a filename
  template different from the global configuration
- **THEN** files under that directory render with the per-directory
  template and `borax config` run there reports the override file as the
  value's origin

#### Scenario: Collection root from config discovery
- **WHEN** files are processed under a directory whose ancestor holds
  `.borax.toml`
- **THEN** that ancestor is the collection root and `.borax/` accounting
  for the run lives there

#### Scenario: Two trees, one mode
- **WHEN** `borax rename tree-a tree-b` runs from a terminal outside
  both, and `tree-a/.borax.toml` sets `rename.batch = true`
- **THEN** the run is interactive throughout, because the mode comes
  from the run's own configuration, and `borax config` run inside
  `tree-a` reports that file as the origin of its value
