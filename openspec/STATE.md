# State of borax-tools

A running assessment of where the project actually is, kept apart from
the specifications because they describe intent and this describes
reality. Read it before planning a change or cutting a release; update it
whenever it stops being true, and at the latest before every version
bump.

Last reviewed: 2026-08-17, when `add-core-pipeline` was archived.

## What is built

The `add-core-pipeline` change is implemented and archived, so the living
specifications in `openspec/specs/` describe working code rather than a
plan. That covers the whole path from a file to a renamed file: tiered
PDF extraction, identifier normalization, resolution against Crossref,
OpenAlex and arXiv with on-disk caching and per-service pacing, the
CSL-JSON record model, the template engine, collision-aware rename
planning with a journal that `borax undo` reads, and BibTeX output to a
master file or to sidecars.

Tests run green on Linux, macOS and Windows. A scheduled job exercises
the real APIs, so schema drift at a source surfaces without a user
finding it first.

## Not built yet

- **The optional `pdfium` backend.** The pure-Rust `PdfSource` is the
  only extraction backend. The second one was always conditional on
  evidence that the first is insufficient, and that evidence has not
  appeared: the fixture corpus and the live contract tests pass on all
  three platforms without it. Picking a binding crate and a
  prebuilt-binary pipeline for three platforms is the cost being
  avoided; revisit only if real PDFs start failing extraction.

  This is task 6.7 of `add-core-pipeline`, and that change was archived
  with the task still open — 59 of 60 — rather than pretending it was
  done. Nothing in the living specifications depends on it: the
  `extraction` spec constrains behaviour (tiering, page bounds, typed
  failures, offline operation) and never names a backend, so which
  engine reads the PDF is an implementation choice the specs leave
  free.
- **Fuzzy title matching**, for files carrying no usable identifier.
  Named out of scope in the `add-core-pipeline` proposal. Today such a
  file is reported unresolvable and skipped, which is correct but leaves
  a class of documents borax cannot help with.
- **The ledger and run logs.** Designed in full on the
  `change/add-ledger-and-run-logs` branch — proposal, design and four
  spec deltas — with no implementation behind it. It replaces the
  journal with a JSONL ledger and unifies the run log, and it imports
  ideas from the `accession.py` prototype with a critique recorded in
  its `design.md`.
- **Everything the design names as a later change**: no library index,
  search or watch mode; no OCR or interactive candidate picker; no XMP
  write-back, Zotero interop or Emacs package; no bibliography format
  other than BibTeX. The file's bytes are never modified — borax renames
  and never edits.

## Open decisions

- **Citation-key generation.** The same template engine will produce
  citation keys, but the default key scheme was never settled, so
  `bib-output` currently derives keys without a specified default that a
  user can rely on or override by entry type. Settle this before anyone
  depends on the keys being stable across versions.
- **Whether the cache and the journal share one on-disk store.** They
  are separate flat files today. The specifications constrain behaviour
  and not storage, so this stays open — and the ledger change would
  answer it by replacing the journal outright, which is the more likely
  path.
- **Whether `pdfium` is ever worth it.** See above; the decision is
  evidence-gated rather than open in the usual sense.

## Live risks

- **The public surface is untested by users.** The JSONL event schemas
  are declared the stable integration contract, and no external consumer
  has yet built against them. Their weaknesses are therefore unknown,
  which is the main reason the project is in `0.y.z` and not approaching
  `1.0.0`. Do not promise compatibility until something has actually
  consumed the stream.
- **Source schema drift** at Crossref, OpenAlex or arXiv. Mitigated, not
  eliminated: the readers ignore unknown fields and type missing ones as
  absent, and the scheduled live contract tests compare real responses
  against the cassettes. A silent change in the *meaning* of a field
  would still pass.
- **BibTeX emission is lossy in both directions.** Provenance fields and
  sidecars carry the full record, so nothing is lost to borax; a
  `.bib` round-trip through another tool is where information goes
  missing.
- **The template grammar invites scope creep** toward a general
  expression language. The grammar is specified and versioned; anything
  beyond the specified filter set needs a spec change rather than an
  implementation.
