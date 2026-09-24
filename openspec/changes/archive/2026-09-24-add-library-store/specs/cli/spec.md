## MODIFIED Requirements

<!-- drops: `ledger` leaves the subcommand list with the ledger file
     itself; `status`, `validate`, `reconcile` and `adopt` take its
     place. -->

### Requirement: Single binary with subcommands
The suite SHALL ship as one binary, `borax`, with at minimum the
subcommands `resolve` (extract + resolve, emit records), `rename` (full
pipeline: resolve, plan, preview/apply), `bib` (emit/merge bibliography
output for already-resolved files), `config` (show effective
configuration), `cache` (inspect and clear the response cache), `status`
(report what a library holds), `validate` (report a library's findings),
`reconcile` (bring artifact records back into agreement with the tree),
and `adopt` (record what the library already holds, from what borax
already knows locally).

#### Scenario: Pipeline via one command
- **WHEN** `borax rename --apply <dir>` runs
- **THEN** extraction, resolution, planning, renaming, and configured
  bibliography output all occur in that single invocation

#### Scenario: A library is reportable with no preceding command
- **WHEN** `borax status` runs over a directory of PDFs borax has never
  seen
- **THEN** it reports what the directory holds, with no initialization,
  import or ingestion subcommand having been run first

<!-- drops: the scenario name "Collection root from config discovery",
     kept in substance as "Library root from config discovery" — the
     marker is here because the name changes with the concept, and the
     scenario itself is unchanged in what it asserts. -->

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
that asks about some of its files and not others without saying so.

The run's own configuration is the one discovered from the run's start
directory — the first path it was given, or the working directory when
it was given none — which is the same configuration `sources`, `mailto`
and the `network` table are taken from. A second input tree's
`.borax.toml` therefore does not change the mode, and neither does the
working directory when the run was given a path.

The nearest `.borax.toml` additionally defines the library root: the
directory containing it anchors the library's `.borax/` directory — the
run logs, and the artifact records that are the library's authoritative
per-file state — and is the directory the item store and every recorded
path are relative to; an explicit `library-root` configuration key
overrides this for unusual layouts. One marker and one root: the
library is the collection under a name that fits what the boundary now
governs, not a second scope discovered separately.

#### Scenario: Per-directory template override
- **WHEN** a directory tree contains a `.borax.toml` defining a filename
  template different from the global configuration
- **THEN** files under that directory render with the per-directory
  template and `borax config` run there reports the override file as the
  value's origin

#### Scenario: Library root from config discovery
- **WHEN** files are processed under a directory whose ancestor holds
  `.borax.toml`
- **THEN** that ancestor is the library root, `.borax/` for the run
  lives there, and `items/` beneath it is the library's item store

#### Scenario: Two trees, one mode
- **WHEN** `borax rename tree-a tree-b` runs from a terminal outside
  both, and `tree-a/.borax.toml` sets `rename.batch = true`
- **THEN** the whole run is a batch run, `tree-b`'s files included,
  because the mode comes from the configuration of the run's start
  directory and not from each file's own

#### Scenario: A second tree does not change the mode
- **WHEN** the same run is given `tree-b` first, and only
  `tree-a/.borax.toml` sets `rename.batch = true`
- **THEN** the whole run is interactive, `tree-a`'s files included

### Requirement: Boolean options are negatable from the command line
Every config-settable boolean option SHALL have a `--no-<option>`
command-line negation, so the command line can override a configured
`true` as well as a configured `false`. Giving both forms in one
invocation SHALL be a usage error rather than resolving to either: the
pair states two incompatible intentions, and a run that picks one of
them silently acts on a setting nobody chose.

A pair SHALL appear on exactly the subcommands that accept the option,
and both halves together: a subcommand offering `--sidecars` offers
`--no-sidecars`, and one offering neither is not thereby missing a
negation.

#### Scenario: CLI overrides a configured true
- **WHEN** configuration enables an option and its `--no-` form is
  passed on the command line
- **THEN** the option is off for that run

#### Scenario: Both forms of a pair
- **WHEN** an option and its `--no-` form are both passed on one
  command line
- **THEN** the invocation is rejected as a usage error naming the two
  flags, and nothing is read, resolved, or moved

#### Scenario: A pair is offered whole or not at all
- **WHEN** a subcommand accepts `--record`
- **THEN** it accepts `--no-record`, and a subcommand accepting neither
  is not missing a negation

<!-- drops: `ledger rebuild` from the surface list and from the help
     scenario, with the subcommand. -->

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
accepts `--rehash`; `adopt` accepts no setting of its own, since it
queries nothing, opens no document and renames nothing. Every
subcommand additionally accepts the run-log pair, which is decided at
dispatch and is therefore operative on all of them.

`--identify` and `--rehash` are per-invocation selectors rather than
settings: each says what this run is being asked to do rather than how
borax behaves, so neither is settable from configuration and neither
takes a `--no-` form.

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
