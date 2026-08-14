# cli — delta for add-ledger-and-run-logs

## MODIFIED Requirements

### Requirement: Configuration resolution order
Configuration SHALL be TOML and resolve with this precedence, highest
first: command-line flags, environment variables, the nearest per-directory
override file (`.borax.toml`, discovered upward from each input file's
directory), the XDG global configuration file, built-in defaults.
`borax config` SHALL print the effective configuration with the origin of
each value. The nearest `.borax.toml` additionally defines the collection
root: the directory containing it anchors the collection's `.borax/`
accounting directory (ledger and run logs); an explicit `collection-root`
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

## ADDED Requirements

### Requirement: Configuration keys and CLI flags share one schema
Configuration keys SHALL be derived from the same option declarations as
the CLI flags: a key is accepted in configuration exactly when its option
is declared configurable, values carry the option's native TOML type, and
an unknown key or a wrongly typed value is a load-time error naming the
key and the expected type. There SHALL be no independently maintained
config-key table.

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
`true` as well as a configured `false`. Between the pair, the last one
given wins.

#### Scenario: CLI overrides a configured true
- **WHEN** configuration enables an option and its `--no-` form is
  passed on the command line
- **THEN** the option is off for that run

### Requirement: The apply gate is never configurable
Configuration SHALL NOT be able to set the `--apply` flag or any one-off
destructive selector; such keys in a configuration file are load-time
errors, and previews remain the default in every configuration.

#### Scenario: apply in config
- **WHEN** a configuration file contains `apply = true`
- **THEN** the run aborts at config load stating the key must be passed
  on the command line
