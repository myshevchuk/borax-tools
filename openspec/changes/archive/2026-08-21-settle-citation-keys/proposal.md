# Proposal: settle-citation-keys

## Why

A citation key is the name a work is cited under, and it is the one
piece of borax's output that gets copied into a document and stays
there. It is currently derived from the *filename* template table: the
key is the rendered filename with whitespace and BibTeX's forbidden
characters stripped. Two consequences follow, both bad.

A user who changes how files are named silently changes every citation
key that borax will emit from then on. The two settings have no reason
to move together — a person may want long, sortable filenames and short
keys — and nothing warns that editing one rewrites the other.

The default is also wrong for the job. `templates.default` is
`[auth:lower][year]_[shorttitle3:camel]`, so the default key is
`smith2024_OnTheOrigin`, where every citation convention in use expects
`smith2024`. The `bib-output` spec has already assumed the short shape
without saying so: its uniqueness requirement is illustrated with "two
different DOIs generate the key `smith2024`", which the shipped default
cannot produce.

`openspec/STATE.md` lists this as an open decision to settle before
anyone depends on the keys being stable across versions. Every release
made without settling it widens the set of documents citing keys borax
would no longer generate.

## What Changes

- Citation keys get **their own template table**, configured under
  `citation-keys`, with exactly the shape `templates` already has: a
  mandatory `default` plus optional per-entry-type overrides, rendered
  by the same engine with the same filters and the same fail-at-load
  behaviour.
- The **default citation-key template is `[auth:lower][year]`**,
  producing `smith2024` — the shape the `bib-output` uniqueness
  requirement already illustrates, and the one the disambiguating
  `a`/`b`/`c` suffix was designed around.
- Citation keys **no longer read the filename templates**, so renaming
  policy and citation policy move independently.
- **BREAKING**: keys emitted by v0.1.0 differ from the keys emitted
  after this change. Per `CLAUDE.md` no compatibility is owed before
  `1.0.0`; the previous behaviour is reproducible by setting
  `citation-keys.default` to the filename template, which the changelog
  entry will say.

## Capabilities

### Modified Capabilities

- `bib-output`: citation keys gain a specified, overridable default and
  a source of their own, instead of being a by-product of the filename.
- `cli`: the `citation-keys` table joins the configuration surface.

## Impact

- `crates/borax/src/bib.rs`: `citation_key` takes the citation-key
  table rather than the filename table; the stripping pass is unchanged.
- `crates/borax/src/config.rs`: a `citation_keys` map beside
  `templates`, merged per key the same open-ended way.
- `crates/borax/src/run.rs`: compile a second `TemplateTable`; the
  existing `templates` helper generalises over which config map it
  reads.
- No change to `borax-core`: `TemplateTable` already supports a default
  plus per-entry-type overrides, which is the whole requirement.
