## Why

Most of this library is named and cited by one pattern —
`YYYY-ABBREV-VOL-PAGE`, `2024-JACS-146-1234` — and borax cannot render
any part of it. The journal abbreviation comes from a hand-curated table
that borax has no way to read; `volume` and the first page are in the
record but not in the template vocabulary; and a record without a volume
would render `2024-JACS--1234`, because a token that renders empty
leaves its literal separators behind.

The abbreviation table is already maintained for another tool
(`zotero-jcode`, whose Zotero plugin writes `jcode: JACS` into an item's
Extra field from a tab-separated journal table). Maintaining a second
copy for borax is the failure this change is meant to prevent: one
curated file, read by both, producing the same abbreviation for the same
journal.

## What Changes

- **A new `external-tables` capability.** Configuration may declare
  named lookup tables, each a tab-separated file with a header row, a
  column supplying the keys and a column supplying the values. Tables
  are loaded and validated before the first file is processed. Nothing
  in the mechanism executes: a table can only substitute one string for
  another, so template rendering stays total, infallible and
  deterministic.
- **A `lookup("<table>")` template filter.** String in, string out, so
  it composes with every existing field and filter and is not tied to
  journals: `[journal:lookup("jcode")]`,
  `[publisher:lookup("pubcodes")]`.
- **Keys match after normalization.** Both sides of a lookup are folded
  to a canonical key by the rules the existing `slug` filter already
  defines. A table row may draw its keys from more than one column, so
  one row matches both `Journal of the American Chemical Society` and
  `J. Am. Chem. Soc.`.
- **Misses are reported, never silent.** A lookup that finds no row
  renders empty — `||` alternatives handle the fallback — and emits a
  deduplicated event naming the table and the unmatched input, counted
  in the run summary. This is what tells the user which row to add.
- **Five fields join the template vocabulary**: `volume`, `issue`,
  `pages`, `firstpage`, and `publisher`. All five are already on the
  record and populated by the Crossref and OpenAlex readers; only the
  template engine could not reach them.
- **Two affix filters join the filter set**: `prefix("<text>")` and
  `suffix("<text>")`, each the identity on the empty string. This is
  how an absent segment takes its separator with it —
  `[volume:prefix("-")]` contributes `-146` or nothing at all — without
  adding conditionals to the grammar.
- **A run records the tables it read**, by path and content digest, so
  the run log can still answer "why did this file get that name" after
  the table has been edited.
- Not breaking. Every addition was previously a load-time error; no
  default changes, and a configuration that declares no table behaves
  exactly as it does today.

## Capabilities

### New Capabilities

- `external-tables`: declaring, loading, validating and matching
  against externally maintained lookup tables — the file format, the
  normalization that decides what matches what, the load-time failures,
  the reporting of misses, and the record a run keeps of which tables
  it used.

### Modified Capabilities

- `templates`: the field vocabulary gains `volume`, `issue`, `pages`,
  `firstpage` and `publisher`; the filter set gains `lookup`, `prefix`
  and `suffix`; and the fail-at-load requirement extends to a `lookup`
  naming a table no configuration declared.
- `cli`: configuration gains a `tables` table with open-ended keys,
  merged per table name the way `templates` and `citation-keys` already
  are, reported by `borax config` with the origin of each value, and
  refused at load when a declared table cannot be read or lacks a named
  column.

## Impact

- `crates/borax-core/src/template.rs`: five `Field` variants, three
  `Filter` variants, a `LookupTables` value threaded through
  `RenderInput`, and the miss record a render produces. The `slug`
  helper is promoted to the public normalization function both the
  filter and the table loader use.
- `crates/borax-core/src/tables.rs` (new): the TSV parser and the
  key-folding, duplicate detection and lookup, all pure.
- `crates/borax/src/config.rs`: a `tables` map beside `templates`, its
  TOML shape, its per-name merging, its `borax config` rendering, and
  the resolution of a relative table path against the file that
  declared it.
- `crates/borax/src/run.rs`: table loading in `preflight`, beside
  template compilation; the tables travel with the compiled templates in
  `Group`, because a table is per-directory configuration exactly as a
  template is.
- `crates/borax/src/event.rs`: a lookup-miss event and its count in the
  run summary; the tables a run read named in `RunStarted`.
- `README.md`: the field and filter tables, a section on external
  tables, and the normalization rules stated normatively enough for
  another tool to reimplement them.
- No new dependency. The TSV format needs no crate, and `slug` already
  exists.

## Deferred

- **Book series.** The pattern applies partially to book series, and the
  record has no field to hold one: CSL's `collection-title` is absent
  from `Record`, so there is nothing for a `[series]` field to render
  and nothing for a lookup to fold. Adding it reaches into
  `record-model` and `resolution` (both source readers, and their
  cassettes), which is a change of its own. This one is built so that
  change adds a field and a table row and nothing else.
- **Tilde expansion in configured paths.** `~/lib/journal_titles.tsv`
  is the natural way to write a table path, and no path setting in
  borax expands `~` today. Doing it for `tables.*.path` alone would be
  the one path setting that behaves differently; doing it for all of
  them is a separate change with its own Windows question.
