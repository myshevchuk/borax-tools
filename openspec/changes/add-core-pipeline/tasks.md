# Tasks: add-core-pipeline

TDD throughout: every pure behaviour lands as a failing test first, then
the implementation, then refactor with the suite green.

## 1. Workspace scaffolding

- [x] 1.1 Create the Cargo workspace with crates `borax-core`,
      `borax-sources`, `borax-pdf`, `borax` (empty lib/bin targets,
      shared workspace lints and metadata)
- [x] 1.2 Add `CHANGELOG.md` (Keep a Changelog, empty `[Unreleased]`),
      dual MIT/Apache-2.0 license files, and a `README.md` stub
- [x] 1.3 Add CI workflow: fmt + clippy + test on Linux, macOS, Windows;
      stub the scheduled live contract-test job

## 2. Record model (borax-core)

- [x] 2.1 Tests + implementation: CSL-JSON superset record with
      identifier set, per-field provenance, confidence; lossless JSON
      round-trip
- [x] 2.2 Tests + implementation: identifier normalization (DOI, arXiv ID
      old/new formats, PMID, ISBN)
- [x] 2.3 Tests + implementation: entry-type mapping for article,
      preprint, book, chapter, thesis, report, patent, standard
- [x] 2.4 Tests + implementation: deterministic BibTeX/BibLaTeX emission
      with escaping

## 3. Template engine (borax-core)

- [x] 3.1 Tests + implementation: bracket-syntax parser with load-time
      failure on unknown fields/filters and syntax errors
- [x] 3.2 Tests + implementation: filter set (`lower`, `upper`,
      `capitalize`, `titlecase`, `slug`, `abbr`, `truncN`,
      `transliterate`, `regex`) with left-to-right chaining
- [x] 3.3 Tests + implementation: `||` alternatives and per-entry-type
      template tables with `default` fallback
- [x] 3.4 Tests + implementation: mandatory sanitization pass (invalid
      chars, Windows reserved names, length limits, `/` as directory
      separator)

## 4. Rename planning (borax-core)

- [ ] 4.1 Tests + implementation: rename planner producing old→new
      mappings with collision detection (batch-internal, existing files,
      case-insensitive) and suffix/skip policies
- [ ] 4.2 Tests + implementation: identical-content targets reported as
      already-named

## 5. Bib output (borax-core)

- [ ] 5.1 Tests + implementation: master-`.bib` merge with
      identifier-based dedup, skip/update policy, byte-preservation of
      untouched content
- [ ] 5.2 Tests + implementation: citation-key uniqueness with
      deterministic letter suffixes
- [ ] 5.3 Tests + implementation: sidecar content (BibTeX + full JSON
      record)

## 6. PDF extraction (borax-pdf)

- [ ] 6.1 Choose and wire the pdfium binding with static/prebuilt
      binaries for Linux, macOS, Windows; define the `Extractor` trait
- [ ] 6.2 Build the real-PDF fixture corpus (publisher PDFs, arXiv PDFs,
      encrypted, no-text-layer, malformed)
- [ ] 6.3 Tests + implementation: embedded XMP/Info metadata pass
- [ ] 6.4 Tests + implementation: text-layer identifier pass with bounded
      page range and DOI/arXiv normalization
- [ ] 6.5 Tests + implementation: tiered orchestration (stop at first
      hit) and typed failure modes

## 7. Sources (borax-sources)

- [ ] 7.1 Define the `Source` trait and typed failure model; set up the
      recorded-cassette test harness
- [ ] 7.2 Tests (cassettes) + implementation: Crossref client with
      polite-pool headers and tolerant response parsing
- [ ] 7.3 Tests (cassettes) + implementation: OpenAlex client
- [ ] 7.4 Tests (cassettes) + implementation: arXiv client
- [ ] 7.5 Tests + implementation: priority/fallback dispatch by
      identifier type
- [ ] 7.6 Tests + implementation: on-disk response cache (identifier key
      + content-hash index), `--no-cache` bypass, rate limiting, bounded
      concurrency
- [ ] 7.7 Tests + implementation: conflict detection and the
      skip-and-report contract

## 8. CLI (borax)

- [ ] 8.1 Tests + implementation: TOML config model and resolution order
      (flags > env > nearest `.borax.toml` > XDG global > defaults)
- [ ] 8.2 Tests + implementation: JSONL event schema (typed, versioned)
      and the dual human/JSON renderer; diagnostics to stderr
- [ ] 8.3 Tests + implementation: `borax resolve`
- [ ] 8.4 Tests + implementation: `borax rename` (preview default,
      `--apply`), wiring planner + journal
- [ ] 8.5 Tests + implementation: append-only journal in XDG state dir
      and `borax undo` with per-entry verification
- [ ] 8.6 Tests + implementation: `borax bib`, `borax config`,
      `borax cache`
- [ ] 8.7 Tests + implementation: exit codes (success / partial /
      fatal) and never-prompt behavior when stdin is not a TTY

## 9. Hardening and release readiness

- [ ] 9.1 End-to-end test: mixed real-PDF batch through
      `borax rename --json` against cassettes, asserting events, renames,
      journal, master `.bib`, and sidecars
- [ ] 9.2 Windows CI green, including sanitization and case-insensitive
      collision tests
- [ ] 9.3 Wire the scheduled live contract-test job against the real
      APIs
- [ ] 9.4 Release scaffolding: static binary builds attached to tagged
      GitHub releases; tag/version consistency check
