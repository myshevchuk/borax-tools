# Proposal: add-core-pipeline

## Why

Researchers accumulate PDFs whose filenames say nothing about their content,
and existing fetch-and-rename tools are either slow and buggy (pdf-renamer)
or embed the feature inside a full library manager (papis, cobib, Zotero).
borax-tools needs a fast, error-free core pipeline that resolves a file's
bibliographic metadata from online sources and renames the file by
configurable rules — as a stateless CLI that never guesses wrong.

## What Changes

- Establish the `borax` CLI (single binary, git-style subcommands) with the
  MVP pipeline: ingest → extract → resolve → plan → apply.
- Tiered identifier extraction from PDFs: embedded XMP/Info metadata first,
  then DOI/arXiv-ID regex over the text layer; stop at the first hit.
  (Title-based fuzzy search is out of scope for this change.)
- Online resolution against Crossref, OpenAlex, and arXiv with a local
  response cache, rate limiting, and polite-pool identification.
- Canonical in-memory record model: a CSL-JSON superset with identifier,
  provenance, and confidence fields; BibTeX/BibLaTeX emission.
- Filename template engine: JabRef-style bracket surface syntax with
  chainable filters, alternatives, and per-entry-type template tables,
  followed by a mandatory filesystem-sanitization pass (Windows-safe).
- Safe renaming: preview by default, `--apply` to execute, journaled
  renames revertible via `borax undo`, collisions never overwrite.
- Bibliography output: merge/deduplicate entries into a master `.bib` file
  and write per-file sidecars.
- JSON Lines output (`--json`) with stable schemas on every subcommand;
  human-readable output renders the same event stream.
- TOML configuration: XDG global file, discoverable per-directory
  overrides, environment/flag overrides.

Out of scope for this change (future changes): fuzzy title matching and the
interactive ambiguity picker, DataCite/PubMed sources, XMP embedding, watch
mode, Zotero/Emacs integration packages, OCR fallback, ISBN/patent
identifier systems.

## Capabilities

### New Capabilities

- `extraction`: tiered identifier extraction from PDF files (embedded
  metadata, text-layer identifier patterns) with a stop-at-first-hit
  contract and typed failure modes.
- `resolution`: querying online sources by identifier, source priority,
  response caching, rate limiting/politeness, and the skip-and-report
  contract for unresolvable or ambiguous files.
- `record-model`: the canonical bibliographic record (CSL-JSON superset
  with identifiers, provenance, confidence) and BibTeX/BibLaTeX emission.
- `templates`: the filename/key template language (bracket syntax,
  filters, alternatives, per-type tables) and the mandatory sanitization
  pass.
- `rename`: rename planning, preview/apply semantics, collision handling,
  and the undo journal.
- `bib-output`: master-`.bib` merge/deduplication and per-file sidecar
  generation.
- `cli`: the `borax` binary's subcommand surface, JSON Lines event
  schemas, exit codes, and configuration resolution.

### Modified Capabilities

(None — greenfield project; no existing specs.)

## Impact

- New Cargo workspace: `crates/borax-core` (pure logic),
  `crates/borax-sources` (HTTP adapters), `crates/borax-pdf` (extraction
  adapter over a bundled pdfium), `crates/borax` (CLI binary).
- New external dependencies: pdfium (statically linked), HTTP client,
  clap, serde; recorded-cassette test infrastructure.
- Network use: Crossref, OpenAlex, and arXiv public APIs, identified via
  polite-pool headers; responses cached locally under the XDG cache
  directory.
- No existing code or users are affected; this change defines the public
  surface that later changes must stay compatible with.
