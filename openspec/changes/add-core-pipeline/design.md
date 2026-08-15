# Design: add-core-pipeline

## Context

Greenfield project. borax-tools is a bibliography CLI suite whose first
milestone is a fetch-and-rename pipeline for PDFs. The design decisions
below were settled in a structured design interview and are recorded here
as the architectural baseline that later changes extend.

The tool positions itself against pdf-renamer (closest in purpose, but
slow and unreliable) and against library managers (papis, cobib, Zotero)
that bundle fetching into a heavier workflow. Long-term ambition is to
grow into a daily-driver replacement for the reference-manager workflow,
so the stateless core must not paint later library features into a corner.

## Goals / Non-Goals

**Goals:**

- A stateless pipeline: each invocation takes files, resolves metadata,
  renames and emits bibliography entries, and forgets. The only persistent
  state is a response cache and the rename journal.
- Error-free by construction: the tool never guesses in batch mode;
  unresolvable or ambiguous files are skipped and reported.
- Fast: tiered extraction stops at the first hit; resolution is concurrent,
  rate-limited, and cached; startup cost is a native binary's.
- Stable machine-readable contracts (JSON Lines) that editor and watcher
  integrations can build on without churn.
- Windows is first-class: filename rules, path handling, and CI.

**Non-Goals:**

- No library database, index, search, or watch mode in this change.
- No fuzzy title matching, OCR, or interactive candidate picker yet.
- No XMP write-back, Zotero interop, or Emacs package yet.
- Not a general PDF-metadata editor; the file's bytes are never modified
  in this change (renaming only).

## Decisions

### Language and workspace: Rust, four-crate monorepo

Rust delivers the two headline requirements directly: instant startup and
a single static binary (fast), plus a type system and error model that
support the never-guess contract (error-free). Python was rejected as the
performance ceiling being escaped; Go's PDF/bibliography ecosystem is the
weakest of the three.

Workspace layout, enforcing the pure-logic/adapter split:

```
crates/borax-core/    # PURE: record model, template engine, filename
                      #   sanitization, BibTeX serialization, .bib
                      #   merge/dedup, rename planning. No I/O.
crates/borax-sources/ # HTTP adapters: Source trait, Crossref/OpenAlex/
                      #   arXiv clients, rate limiting, response cache.
crates/borax-pdf/     # Extraction adapter: Extractor trait + pdfium
                      #   backend (statically linked).
crates/borax/         # CLI binary: clap subcommands, JSONL emitter,
                      #   config resolution, undo journal.
```

`borax-core` is fully unit-testable offline; adapters hide behind traits
so pipeline logic tests need neither network nor the native PDF library.

### Pipeline shape

```
Ingest → Extract (tiered) → Resolve (sources + cache) → Record
      → Plan (template render, sanitize, collision check) → Apply
```

Each stage emits typed events; the JSONL stream and the human output are
two renderings of the same events. Files that fail a stage divert to a
skip-queue reported in the run summary — the pipeline never aborts a batch
for one bad file and never falls through to a lower-confidence method
silently.

### Extraction: tiered, stop at first hit

1. Embedded XMP / document-info metadata (cheapest, often authoritative).
2. DOI / arXiv-ID regex over the text layer of the first pages.
3. (Future change) title extraction + fuzzy search.

Rationale: pdf-renamer's slowness comes from doing everything always. The
common case (publisher PDF with embedded DOI) must cost milliseconds plus
one cached HTTP round-trip.

### PDF backend: pure-Rust by default, pdfium as a build option

The engine sits behind the `PdfSource` trait, and the default build
uses a pure-Rust extractor: no native toolchain, no downloaded
binaries, `cargo install borax` works everywhere, and Windows CI stays
ordinary. pdfium (BSD-licensed) is available as an opt-in cargo
feature for the fidelity it buys on malformed publisher PDFs, but it is
not bundled — its cost is a per-platform prebuilt-binary pipeline, paid
only by those who want it.

This revises the original decision (statically linked pdfium as the
only backend). Two things changed it: the trait boundary proved that
every extraction rule is testable without any engine, so fidelity is no
longer on the critical path; and a mandatory native dependency is a
poor trade for a tool whose selling point is being fast to install and
run. MuPDF stays rejected on AGPL grounds (project is MIT/Apache-2.0);
shelling out to `pdftotext` stays rejected as a runtime dependency plus
per-file process overhead.

### Record model: CSL-JSON superset, BibTeX emission

CSL-JSON is the canonical internal model (structured, unambiguous, the
lingua franca of citeproc/Zotero/Pandoc), extended with: identifier set
(DOI, arXiv ID, PMID, ISBN), provenance (which source supplied each
field), and resolution confidence. BibTeX/BibLaTeX is the primary emission
format for the LaTeX/Emacs workflow. Emitting other formats later is a
serializer addition, not a model change. Doc types beyond articles
(preprints, books/chapters, theses, patents/standards) are representable
in the model now even though only article/preprint resolution ships in
this change.

### Resolution: Source trait, priority order, polite by default

Sources implement a common trait: given an identifier, return a Record or
a typed failure. Priority: Crossref → OpenAlex → arXiv (arXiv first for
arXiv IDs). Responses are cached keyed by identifier and by file hash;
requests carry polite-pool identification (User-Agent + mailto from
config) and are rate-limited per source. Re-running over a directory is
nearly free.

### Ambiguity: skip and report, never guess

In batch mode any low-confidence or conflicting resolution diverts the
file to the skip-queue, untouched. The run summary lists skipped files
with reasons; a later change adds `borax resolve --interactive` to triage
the queue with an inline picker. A confidence-threshold auto-accept mode
was rejected: occasionally-wrong renames are the failure mode this tool
exists to eliminate.

### Template engine: JabRef bracket surface, BBT-grade semantics

Surface syntax is JabRef-style brackets embedded in TOML strings —
readable inline, familiar to reference-manager users:

```toml
[templates.filename]
default = "[authors2:slug]/[year] - [title:trunc60] ([journal:abbr]).pdf"
fallback = "[doi:slug] || [sha1:trunc8]"
```

Semantics are Better-BibTeX-grade: filters chain freely (`:lower`,
`:slug`, `:abbr`, `:truncN`, `:regex("p","r")`, `:transliterate`, case
filters), `||` provides alternatives, and templates are per-entry-type
tables with a default fallback. Chosen over a faithful JabRef clone
(composition limited to concatenation) and over reimplementing BBT's full
JavaScript-ish formula language (heavier parser, unfamiliar syntax) — the
hybrid keeps the readable surface and the expressive core.

Rendering ends with a mandatory, non-optional sanitization pass: invalid
character replacement, Windows reserved names and length limits,
transliteration hooks; `/` in a template maps to subdirectories.

### Rename safety: preview default, journal, no overwrites

Runs print the old→new mapping and require `--apply` to execute. Applied
renames append to a journal (XDG state directory) so `borax undo` reverts
the last run. Name collisions never overwrite: the planner detects them
(including case-insensitive filesystems) and either suffixes or skips per
config. Chosen over apply-by-default because renaming is the destructive
heart of the tool and previews cost one flag.

### CLI: one binary, JSONL first-class, TOML config

Single `borax` binary with git-style subcommands (`resolve`, `rename`,
`bib`, `undo`, `config`, `cache`). Every subcommand supports `--json`
emitting JSON Lines with stable schemas — the integration contract for
Emacs/scripts. Config is TOML: XDG global file, discoverable per-directory
override files (`.borax.toml`, editorconfig-style), then environment
variables and flags. A suite of separate piped tools was rejected for
distribution and discovery cost; the subcommands share the pipeline
internally.

### Testing: cassettes + scheduled live contract tests

Unit and integration tests run offline against recorded HTTP cassettes;
extraction runs against a corpus of real-world PDFs kept as fixtures. A
scheduled CI job replays contract tests against the live APIs to catch
upstream drift without making regular CI flaky. TDD per project
convention: every pure behaviour lands red-test-first in `borax-core`.

## Risks / Trade-offs

- [pdfium static linking is heavy and complicates cross-compilation,
  especially Windows] → isolate behind the `Extractor` trait; use
  prebuilt pdfium binaries in CI; a pure-Rust fallback extractor can be
  added as a feature flag without touching pipeline logic.
- [Crossref/OpenAlex schema drift breaks parsing silently] → scheduled
  live contract tests diff real responses against the cassettes; parsers
  are tolerant readers (unknown fields ignored, missing fields typed as
  absent).
- [Template engine scope creep toward a general language] → grammar is
  specified in the `templates` spec and versioned; features beyond the
  specified filter set require a spec change.
- [Stateless-now vs. library-manager-later tension] → the JSONL event
  schemas and the record model are the stable contracts; a future index
  layer consumes them without changing existing subcommand behaviour.
- [BibTeX emission from CSL-JSON is lossy in both directions] →
  provenance fields record the source values; sidecar files carry the
  full record so no information is lost to the `.bib` round-trip.

## Open Questions

- Exact pdfium binding/crate choice and prebuilt-binary sourcing for the
  three platforms (resolve during implementation of `extraction`).
- Whether the cache and journal share one on-disk store (single SQLite
  file) or stay separate flat files (decide when implementing `rename`
  and `resolution`; spec constrains behaviour, not storage).
- Citation-key generation (as opposed to filenames) is specified by the
  same template engine but the default key scheme is deferred until the
  `bib-output` capability is implemented.
