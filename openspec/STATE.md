# State of borax-tools

A running assessment of where the project actually is, kept apart from
the specifications because they describe intent and this describes
reality. Read it before planning a change or cutting a release; update it
whenever it stops being true, and at the latest before every version
bump.

Last reviewed: 2026-08-22, when the ledger was wired into `rename`.

## What is built

The `add-core-pipeline` change is implemented and archived, so the living
specifications in `openspec/specs/` describe working code rather than a
plan. That covers the whole path from a file to a renamed file: tiered
PDF extraction, identifier normalization, resolution against Crossref,
OpenAlex and arXiv with on-disk caching and per-service pacing, the
CSL-JSON record model, the template engine, collision-aware rename
planning with an undo that reverses an applied run, and BibTeX output
to a master file or to sidecars.

`stream-per-file-events` is implemented on top of that. A
run writes each event when it happens rather than assembling the whole
stream first, and `rename` and `bib` work one file at a time, so a
file's verdict and its fate are adjacent. The decisions are unchanged:
`borax_core::rename::Planner` is the batch planner's own state made
drivable one input at a time, and the batch entry point is a fold over
it.

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
- **Nothing from this change.** `add-ledger-and-run-logs` is
  implemented bar its integration tests: the ledger, `borax ledger
  rebuild`, duplicate checking in `rename`, run logs, and `borax undo`
  reading the latest apply-run log. `journal.rs` is deleted and
  nothing reads a v0.1.0 `renames.jsonl`. What is left is group 6 —
  end-to-end tests and Windows CI coverage — and tasks 5.1 and 5.3,
  which are verification-first and may need no code at all.
- **Everything the design names as a later change**: no library index,
  search or watch mode; no OCR or interactive candidate picker; no XMP
  write-back, Zotero interop or Emacs package; no bibliography format
  other than BibTeX. The file's bytes are never modified — borax renames
  and never edits.

## Open decisions

- **How `rename` could ever honour `concurrency`.** `resolve` runs its
  batch on a bounded pool; `rename` is serial, and
  `stream-per-file-events` made that harder to change by choosing live
  per-file reporting. The two pull against each other: resolving
  concurrently means files finish in whatever order the network
  answers, so a concurrent `rename` would either report out of order or
  buffer completions to restore input order — which is the buffering
  that change removed. Nothing is broken today, and the answer is not
  obvious enough to guess at.
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
