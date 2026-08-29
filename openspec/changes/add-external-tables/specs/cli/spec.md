## ADDED Requirements

### Requirement: External lookup tables are configured per collection
The configuration SHALL carry a `tables` table beside `templates` and `citation-keys`, with the same open-ended keys: each key names a lookup table, and its value declares the file to read, the column or columns supplying keys, and the column supplying values. Like those two, `tables` MUST be settable only in configuration files, never from an environment variable or a command-line flag, because its keys are open-ended.

Merging SHALL be per table name, so a `.borax.toml` that declares one table keeps every table it inherits from the global file. `borax config` SHALL report each declared table's path, key columns and value column, with the origin of each.

A `path` that is relative SHALL resolve against the directory of the configuration file that declared it, rather than the working directory, so a configuration file can name a data file beside itself and stay correct wherever the run is started from.

#### Scenario: Table declaration reported with its origin
- **WHEN** a global configuration file declares a `jcode` table
- **THEN** `borax config` reports that table's path, key columns and
  value column, each with that file as its origin

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
