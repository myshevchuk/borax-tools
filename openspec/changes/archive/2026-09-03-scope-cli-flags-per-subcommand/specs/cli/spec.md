## ADDED Requirements

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

## MODIFIED Requirements

<!-- drops: nothing; the requirement gains the sentence reconciling one
     schema with a per-subcommand flag surface -->

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

<!-- drops: nothing; the requirement gains the sentence scoping "every
     boolean" to the subcommands that accept the option -->

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

<!-- drops: nothing; the requirement's premise is extended to
     `templates`, which is what removes the `--template` flag -->

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

<!-- drops: nothing; the requirement gains the scope shared with
     every other template table, which its "the run" was silent about -->

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
