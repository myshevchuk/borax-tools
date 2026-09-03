# cli Specification

## Purpose
TBD - created by archiving change add-core-pipeline. Update Purpose after archive.
## Requirements
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

### Requirement: JSON Lines output is first-class
Every subcommand SHALL support `--json`, emitting one JSON object per line
to stdout. Each event SHALL carry an event type and a schema version;
schemas are stable within a major version of borax. Human-readable output
and JSON output SHALL be renderings of the same event stream, and
diagnostics SHALL go to stderr so stdout stays machine-parseable.

#### Scenario: Machine-readable run
- **WHEN** `borax rename --json` processes a batch
- **THEN** stdout contains only well-formed JSON Lines (per-file events
  plus a summary event) and any progress or warnings appear on stderr

### Requirement: Exit codes distinguish partial success
The binary SHALL exit 0 when every input file succeeded, exit with a
dedicated nonzero code when the run completed but one or more files were
skipped, and exit with a distinct code for fatal errors (bad
configuration, unusable arguments).

#### Scenario: Batch with skips
- **WHEN** a run completes with 8 successes and 2 skipped files
- **THEN** the exit code is the dedicated partial-success code, distinct
  from both 0 and the fatal-error code

### Requirement: An input that cannot be reached is reported
The binary SHALL report a path named on the command line that does not
exist, or cannot be read, as a skipped file rather than letting it
silently contribute nothing. A run whose inputs were all unreachable
SHALL NOT exit 0: a mistyped filename must not be indistinguishable from
a clean run.

A directory that cannot be read still contributes nothing rather than
ending the run — the files that were readable deserve their run — but a
path the user typed is always accounted for.

#### Scenario: Mistyped filename
- **WHEN** `borax resolve does-not-exist.pdf` runs
- **THEN** the file is reported as skipped and the exit code is the
  partial-success code, not 0

### Requirement: Configuration errors are never silent
A configuration file that is present but cannot be read SHALL end the run
with the fatal exit code, distinct from the file being absent, which
contributes no layer. Running on settings the user wrote and borax could
not read is the outcome this rules out.

Configuration SHALL reject a source borax has no client for, naming the
sources it does support, rather than accepting it and resolving nothing.
`borax config` SHALL report only sources a run can actually query.

#### Scenario: Unreadable override file
- **WHEN** a `.borax.toml` exists at or above an input but cannot be read
- **THEN** the run stops with the fatal exit code and names the file

#### Scenario: Source with no client
- **WHEN** a configuration names a source borax cannot query
- **THEN** the run stops with a configuration error listing the supported
  sources

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

### Requirement: Non-interactive contexts never prompt
When stdin is not a terminal, the binary SHALL never wait for interactive
input; any operation that would prompt falls back to its safe
non-interactive behavior (preview, skip, or fail with a typed error).

#### Scenario: Piped invocation
- **WHEN** `borax rename --apply` runs with stdin redirected from
  /dev/null
- **THEN** the run completes without prompting, skipping anything that
  would have required confirmation

### Requirement: A run reports as it goes
Each event SHALL be written to stdout at the moment it occurs, rather
than accumulated and rendered once the run is over. A run whose work is
network-bound therefore shows progress while it is bound, and a reader
can tell a slow run from a stopped one without waiting for it to end.

This constrains when a line is written, not what it says: the event
schemas are unchanged, and human and JSON output remain two renderings
of the same stream in the same order.

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

### Requirement: Citation-key templates are configured separately
The configuration SHALL carry a `citation-keys` table beside
`templates`, with the same shape and the same open-ended keys: a
`default` that every entry type falls back to, plus optional
per-entry-type overrides. A key naming no entry type, and a template
that will not compile, SHALL be configuration errors reported before any
file is processed — the same treatment `templates` receives, for the
same reason: a broken template is wrong for every file in the batch.
`borax config` SHALL report the effective citation-key templates and the
origin of each.

That validation SHALL follow the same scope every template table
has: a command compiles the citation-key templates when it renders
citation keys, and a command that renders none is not ended by one
that will not compile. "Before any file is processed" says when the
check happens for the commands that make it, not that every command
makes it.

#### Scenario: Citation-key override reported
- **WHEN** a configuration file sets `citation-keys.default`
- **THEN** `borax config` reports that value with the file as its origin

#### Scenario: Uncompilable citation-key template
- **WHEN** `citation-keys.default` will not compile
- **THEN** the run aborts as a configuration error naming the key,
  before any file is processed

#### Scenario: Citation-key key names no entry type
- **WHEN** `citation-keys` carries a key matching no entry type
- **THEN** the run aborts as a configuration error naming the key

### Requirement: Configuration keys and CLI flags share one schema
Configuration keys SHALL be derived from the same option declarations as
the CLI flags: a key is accepted in configuration exactly when its option
is declared configurable, values carry the option's native TOML type, and
an unknown key or a wrongly typed value is a load-time error naming the
key and the expected type. There SHALL be no independently maintained
config-key table.

Which subcommands expose an option as a flag SHALL NOT narrow which keys
configuration accepts. The schema is one; the flag surface is a per-
subcommand view of it, and a key whose flag appears on no subcommand a
run happens to use is still a key that configuration sets.

#### Scenario: Unknown key
- **WHEN** a configuration file contains a key matching no declared
  option
- **THEN** the run aborts at config load naming the unknown key

#### Scenario: Wrong type
- **WHEN** a configuration file sets a boolean option to the string
  "yes"
- **THEN** the run aborts at config load naming the key and the expected
  boolean type

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
- **WHEN** a subcommand accepts `--ledger`
- **THEN** it accepts `--no-ledger`, and a subcommand accepting neither
  is not missing a negation

### Requirement: The apply gate is never configurable
Configuration SHALL NOT be able to set the `--apply` flag or any one-off
destructive selector; such keys in a configuration file are load-time
errors, and previews remain the default in every configuration.

#### Scenario: apply in config
- **WHEN** a configuration file contains `apply = true`
- **THEN** the run aborts at config load stating the key must be passed
  on the command line

### Requirement: External lookup tables are configured per collection
The configuration SHALL carry a `tables` table beside `templates` and `citation-keys`, with the same open-ended keys: each key names a lookup table, and its value declares the file to read, the column or columns supplying keys, the column supplying values, and whether those values are literal text or template fragments. Like those two, `tables` MUST be settable only in configuration files, never from an environment variable or a command-line flag, because its keys are open-ended.

That restriction SHALL hold for all three without exception. `templates` therefore has no command-line flag: a flag could reach only the `default` key, leaving every per-entry-type template unreachable, so it would be a partial door into a structure the command line is otherwise not allowed to open.

Merging SHALL be per table name, so a `.borax.toml` that declares one table keeps every table it inherits from the global file. `borax config` SHALL report each declared table's path, key columns, value column and value kind, with the origin of each.

A `path` that is relative SHALL resolve against the directory of the configuration file that declared it, rather than the working directory, so a configuration file can name a data file beside itself and stay correct wherever the run is started from.

#### Scenario: Table declaration reported with its origin
- **WHEN** a global configuration file declares a `jcode` table
- **THEN** `borax config` reports that table's path, key columns, value
  column and value kind, each with that file as its origin

#### Scenario: Per-directory table added to the inherited set
- **WHEN** the global file declares a `jcode` table and a `.borax.toml`
  declares a `pubcodes` table
- **THEN** files under that directory see both tables, and `borax config`
  run there reports each with its own origin

#### Scenario: Relative path is read beside its declaring file
- **WHEN** a `.borax.toml` declares a table with a relative path and the
  run is started from an unrelated working directory
- **THEN** the file read is the one beside that `.borax.toml`

#### Scenario: Table cannot be set from the environment
- **WHEN** an environment variable is set that appears to name a table
- **THEN** it is refused as naming no setting, the way a templates key
  would be

#### Scenario: Undeclared table refused at load
- **WHEN** a template looks up a table no configuration declares
- **THEN** the run aborts as a configuration error naming the template
  and the table, before any file is processed

#### Scenario: No command-line flag sets a filename template
- **WHEN** `borax rename --template "[auth][year]" f.pdf` runs
- **THEN** the invocation is rejected as an unknown argument, and the
  template is set in a configuration file instead

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
policy, the bibliography settings, the ledger gate, and `--apply`; `bib`
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

