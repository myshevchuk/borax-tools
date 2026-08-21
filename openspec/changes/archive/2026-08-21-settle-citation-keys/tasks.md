# Tasks: settle-citation-keys

`TemplateTable` in `borax-core` already provides a default plus
per-entry-type overrides, so nothing pure needs building; the work is in
`crates/borax`. TDD throughout: red test first for every behaviour.

## 1. Configuration surface

- [x] 1.1 Tests + implementation: `citation_keys` map on `Config`,
      defaulting to `{ default = "[auth:lower][year]" }`, merged per key
      the same open-ended way `templates` is
- [x] 1.2 Tests + implementation: `citation-keys.*` origins tracked and
      rendered by `borax config`
- [x] 1.3 Tests + implementation: an uncompilable template, and a key
      naming no entry type, abort as configuration errors naming the key

## 2. Key generation

- [x] 2.1 Test (red): the default configuration cites a 2024 Smith
      record as `smith2024`
- [x] 2.2 Tests + implementation: `citation_key` renders from the
      citation-key table, not the filename table; stripping of
      whitespace and `,{}%` unchanged; empty result still reports
      uncitable
- [x] 2.3 Test: changing `templates.default` leaves the citation key
      untouched
- [x] 2.4 Test: a per-entry-type citation-key template applies to its
      type only

## 3. Wiring

- [x] 3.1 Generalise `run::templates` over which config map it compiles,
      so both tables are built by one helper reporting errors against
      the right key prefix
- [x] 3.2 Thread the citation-key table to every `citation_key` call
      site (master `.bib` merge and sidecars alike)

## 4. Documentation

- [x] 4.1 README: document `citation-keys` beside `templates`, with the
      default and a worked example
- [x] 4.2 `CHANGELOG.md` under `[Unreleased]`: state that keys have
      changed shape and how to restore the old behaviour
- [x] 4.3 `openspec/STATE.md`: drop citation-key generation from the
      open decisions
