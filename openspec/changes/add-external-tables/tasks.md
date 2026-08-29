# Tasks: add-external-tables

Every behaviour this change adds is a pure function of strings, so all of
it lands red-test-first in `borax-core` per `CLAUDE.md`. Each group is a
red/green commit pair unless it says otherwise: the tests in the `-tests`
task are written and run failing first, the implementation task makes
them pass without touching them.

Work on branch `change/add-external-tables`. Groups 1–4 are `borax-core`
and have no I/O; group 5 onward is the adapter.

## 1. Table loading and the matching fold

- [ ] 1.1 Promote `slug` in `crates/borax-core/src/template.rs` to `pub`
      with a doc comment stating the four folding steps normatively, and
      say in it that the fold is the contract other tools reimplement
- [ ] 1.2 Sketch `crates/borax-core/src/tables.rs`: `Table`,
      `TableSpec { key_columns, value_column }`, `LookupTables`,
      `TableError`, and `parse_tsv` / `Table::load` / `Table::get`
      signatures with `todo!()` bodies and full doc contracts
- [ ] 1.3 Red: `crates/borax-core/tests/tables.rs` covering the TSV
      contract — header discovery, BOM, CRLF, blank lines, ignored extra
      columns, a row with a missing or empty key or value cell warning
      and skipping, and a declared column absent from the header failing
- [ ] 1.4 Green: implement the parser
- [ ] 1.5 Red: fold and lookup cases — punctuation and case differences
      matching, `J. Am. Chem. Soc.` matching `J Am Chem Soc`,
      `Zeitschrift für Chemie` folding to `zeitschrift-fuer-chemie`, a
      key folding to empty being dropped with a warning, and an input
      folding to empty matching nothing
- [ ] 1.6 Green: implement folding and lookup over the parsed rows
- [ ] 1.7 Red: multi-column keys — one row contributing a key per named
      column, an identical pair of cells contributing one key without
      complaint, and two rows folding alike with different values
      failing with both values named
- [ ] 1.8 Green: implement multi-column keys and conflict detection

## 2. The publication fields

- [ ] 2.1 Red: `crates/borax-core/tests/template.rs` cases for `volume`,
      `issue`, `pages` and `publisher` rendering their record values and
      rendering empty when absent
- [ ] 2.2 Red: `firstpage` cases — `1234-1245` → `1234`, an en-dash and
      an em-dash range, whitespace around the separator, the article
      numbers `e0123456` and `045301` passing through whole, and an
      absent page value rendering empty
- [ ] 2.3 Green: five `Field` variants, their parse arms and their render
      arms

## 3. The affix filters

- [ ] 3.1 Red: `prefix` and `suffix` cases — wrapping a present value,
      returning the empty string unchanged, composing to the right of
      another filter, and the two quoted-argument parse errors (an
      unterminated argument, a missing closing paren)
- [ ] 3.2 Red: the optional-segment case end to end —
      `[year]-[journal:abbr][volume:prefix("-")]-[firstpage]` rendering
      `2024-JACS-146-1234` with a volume and `2024-JACS-1234` without
- [ ] 3.3 Green: two `Filter` variants reusing the existing
      `parse_quoted`, and their apply arms

## 4. The lookup filter and miss reporting

- [ ] 4.1 Change `Template::render` to return `Rendered { text, misses }`
      and add `tables: &LookupTables` to `RenderInput`; update the
      `TemplateTable::render` signature to match
- [ ] 4.2 Update every existing `render` call site and test in
      `borax-core` to the new signature, asserting `misses` is empty
      where no lookup is involved — a mechanical commit of its own,
      landed before the lookup behaviour so the red tests in 4.3 are the
      only failing ones
- [ ] 4.3 Red: lookup cases — a hit substituting the value, composing
      with a following filter, a miss rendering empty, a miss falling
      through to a `||` alternative, a miss recorded in `misses` even
      when an alternative supplied the output, and misses appearing in
      template evaluation order
- [ ] 4.4 Green: the `Lookup` filter variant, its parse arm, its apply
      arm, and miss collection through `Chain` and `Segment`
- [ ] 4.5 Red: compile-time refusal — `Template::compile` given a
      `lookup` naming no declared table yields the unknown-table error
      naming the table
- [ ] 4.6 Green: thread the declared table names into `compile` and add
      the `TemplateError::UnknownTable` variant with its `Display` arm

## 5. Configuration

- [ ] 5.1 Red: `crates/borax/tests/config.rs` cases for the `[tables]`
      TOML shape — a table with a single key column and with an array of
      them, merging per table name across layers, an unknown field
      inside a table declaration failing, and a `BORAX_TABLES_*`
      variable being refused as naming no setting
- [ ] 5.2 Green: a `tables` map on `Config` and `Layer`, its per-name
      merge in `resolve` beside `templates`, and its `Origin` keys
      `tables.<name>.path`, `.key` and `.value`
- [ ] 5.3 Red: `borax config` renders each declared table's three keys
      with the origin of each
- [ ] 5.4 Green: extend `Effective::entries` the way it already extends
      over `templates`
- [ ] 5.5 Red: a relative `path` resolving against the directory of the
      declaring file, for both the global file and a `.borax.toml`, with
      an unrelated working directory
- [ ] 5.6 Green: resolve the path when the layer is built, where the
      declaring file's path is still known

## 6. Wiring the run

- [ ] 6.1 Load the declared tables in `preflight`, beside template
      compilation, and pass the declared names into `run::templates` so
      compilation can refuse an unknown one
- [ ] 6.2 Carry `LookupTables` in `Group` next to `filenames` and
      `citation_keys`, and thread it into `RenderInput` in
      `renaming.rs` and `bib.rs`
- [ ] 6.3 Red: `crates/borax/tests/event.rs` cases for the lookup-miss
      event and its summary count in both the human and JSON renderings
- [ ] 6.4 Green: the event variant, its two rendering arms, and its
      `Counts` field
- [ ] 6.5 Deduplicate misses across files within a run and emit one event
      per distinct table-and-input pair
- [ ] 6.6 Name each loaded table's path and content digest in
      `RunStarted`, and update its two rendering arms
- [ ] 6.7 Red: `crates/borax/tests/binary.rs` cases for the three fatal
      table failures — a missing file, a header without a declared
      column, and two rows claiming one key — each ending the run with
      the fatal exit code, the reason on stderr, and neither
      `run-started` nor `run-finished` on stdout

## 7. End to end and documentation

- [ ] 7.1 An integration case rendering the target pattern over a fixture
      table: an article with a volume giving `2024-JACS-146-1234` and one
      without giving `2024-JACS-1234`, both from the one template
- [ ] 7.2 A fixture TSV under `crates/borax/tests/` carrying the real
      file's header and a handful of rows, including one whose `title`
      and `shorttitle` are identical
- [ ] 7.3 `README.md`: the five fields and three filters in their tables,
      a section on external tables covering declaration, the four fold
      steps stated normatively, misses and how they are reported, and
      the warning that editing a table changes keys already cited
- [ ] 7.4 `CHANGELOG.md` entry under Unreleased
- [ ] 7.5 `openspec/STATE.md`: record the new capability, and move book
      series from unstated to a named gap under "Not built yet" with
      `collection-title` as what it needs
- [ ] 7.6 Full suite green on Linux, macOS and Windows; `cargo clippy`
      clean; `openspec validate --specs` and
      `scripts/check-spec-deltas.py` both pass
