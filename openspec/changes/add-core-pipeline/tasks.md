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

- [x] 4.1 Tests + implementation: rename planner producing old→new
      mappings with collision detection (batch-internal, existing files,
      case-insensitive) and suffix/skip policies
- [x] 4.2 Tests + implementation: identical-content targets reported as
      already-named

## 5. Bib output (borax-core)

- [x] 5.1 Tests + implementation: master-`.bib` merge with
      identifier-based dedup, skip/update policy, byte-preservation of
      untouched content
- [x] 5.2 Tests + implementation: citation-key uniqueness with
      deterministic letter suffixes
- [x] 5.3 Tests + implementation: sidecar content (BibTeX + full JSON
      record)

## 6. PDF extraction (borax-pdf)

Everything above the `PdfSource` trait (6.1-6.4) is pure and tested
with fakes. What remains needs a real engine, and per the revised
backend decision the default one is pure-Rust: pdfium is an opt-in
build feature, never a mandatory dependency.

- [x] 6.1 Define the engine seam: the `PdfSource` trait and its typed
      failure model
- [x] 6.2 Tests + implementation: embedded XMP/Info metadata pass
- [x] 6.3 Tests + implementation: text-layer identifier pass with bounded
      page range and DOI/arXiv normalization
- [x] 6.4 Tests + implementation: tiered orchestration (stop at first
      hit) and typed failure modes
- [x] 6.5 Implement the default pure-Rust `PdfSource` backend (no native
      toolchain, works on every platform out of the box)
- [x] 6.6 Build the real-PDF fixture corpus (publisher PDFs, arXiv PDFs,
      encrypted, no-text-layer, malformed); ghostscript generates them
      locally
- [ ] 6.7 Optional `pdfium` cargo feature: a second `PdfSource` backend
      plus its prebuilt-binary pipeline. Deferred until Windows testing
      shows the default backend is not enough
- [x] 6.8 Tests + implementation: `scan::xmp_title`, reading `dc:title`
      (including its `rdf:Alt` language table) so the conflict check
      sees every title a document claims rather than only the Info
      dictionary's

## 7. Sources (borax-sources)

Response reading is separated from HTTP, and caching is a decorator
rather than a client feature, so everything here is tested offline
against real recorded responses. Only the ureq transport (7.6) touches
a socket, and only the file-backed cache (7.8) touches a disk.

- [x] 7.1 Define the `Source` trait and typed failure model; set up the
      recorded-cassette test harness
- [x] 7.2 Tests (cassettes) + implementation: tolerant Crossref
      response reader
- [x] 7.3 Tests (cassettes) + implementation: OpenAlex response reader
- [x] 7.4 Tests (cassettes) + implementation: arXiv response reader,
      including the entry-less feed that means "not found"
- [x] 7.5 Tests + implementation: priority/fallback dispatch by
      identifier type
- [x] 7.6 Tests + implementation: the HTTP clients implementing
      `Source` — polite-pool identification, timeouts, and the
      status-to-`SourceError` mapping; the ureq transport, with
      `tests/live.rs` as its (ignored) contract tests
- [x] 7.7 Tests + implementation: the cache seam — key derivation, the
      `Cache` trait, `MemoryCache`, and the `Cached` decorator that
      never stores a failure
- [x] 7.8 Implement the file-backed `Cache` under the XDG cache
      directory, plus the content-hash index that lets an already-seen
      file skip extraction entirely (the `--no-cache` bypass is a CLI
      flag, task 8)
- [x] 7.9 Tests + implementation: rate-limit pacing and bounded
      concurrency
- [x] 7.12 Tests + implementation: the `Paced` decorator, and both it
      and `Cached` wired into the binary. `delay_before` and the cache
      decorator existed and were tested but nothing in production
      called either, so `--min-interval-ms` was inert and borax asked
      for polite-pool trust while rate-limiting nothing
- [x] 7.10 Tests + implementation: conflict detection for the
      skip-and-report contract
- [x] 7.11 Revise conflict detection, which 7.10 built as exact equality
      after normalization and which therefore skipped correct files over
      typography their producer chose. Titles compare as content words
      folded toward the lossiest encoding either side could carry and are
      judged by similarity; a document's title claims are filtered
      through a plausibility guard first, and agreement by any one of
      them clears the file. The skip reports the similarity

## 8. CLI (borax)

- [x] 8.1 Tests + implementation: TOML config model and resolution order
      (flags > env > nearest `.borax.toml` > XDG global > defaults)
- [x] 8.2 Tests + implementation: JSONL event schema (typed, versioned)
      and the dual human/JSON renderer; diagnostics to stderr
- [x] 8.3 Tests + implementation: `borax resolve`
- [x] 8.4 Tests + implementation: `borax rename` (preview default,
      `--apply`), wiring planner + journal
- [x] 8.5 Tests + implementation: append-only journal in XDG state dir
      and `borax undo` with per-entry verification
- [x] 8.6 Tests + implementation: `borax bib`, `borax config`,
      `borax cache`
- [x] 8.7 Tests + implementation: exit codes (success / partial /
      fatal) and never-prompt behavior when stdin is not a TTY
- [x] 8.8 The binary itself: argument parsing and subcommand dispatch,
      the real `Library` and `Filesystem` adapters, input-path walking,
      and a `main` thin enough that everything it does stays reachable
      from an in-process test. Tasks 8.1-8.7 build the library the
      binary is a shell over; 8.8 is the shell

## 9. Hardening and release readiness

- [x] 9.1 End-to-end test: mixed real-PDF batch through
      `borax rename --json` against cassettes, asserting events, renames,
      journal, master `.bib`, and sidecars. Every adapter is the real
      one but `Transport`; the batch covers both extraction tiers, both
      identifier kinds, and three ways of failing. It found 9.1a on its
      first run
- [x] 9.1a Fix the arXiv reader's identifier extraction, which took the
      abstract URL's last path segment. An identifier issued before
      April 2007 is `archive.subject/YYMMNNN` and the `/` is part of
      it, so `.../abs/math.GT/0309136v1` parsed as the bare number
      `0309136v1` and every pre-2007 preprint failed to resolve. The
      recorded cassette is a new-style id, so no unit test reached it
- [ ] 9.2 Windows CI green, including sanitization and case-insensitive
      collision tests
- [x] 9.3 Wire the scheduled live contract-test job against the real
      APIs, and widen what it covers. The parse-level tests said nothing
      about whether a client's own URL still lands on the service, so
      the clients are now exercised through `Source::fetch` over a real
      transport, including a pre-2007 arXiv identifier (the case 9.1a
      fixed) and dispatch across two live sources. The job is also
      startable by hand, which is what you want while changing a reader,
      and identifies itself with `BORAX_MAILTO` when the repository sets
      one. All eleven pass against the live services
- [x] 9.4 Release scaffolding: static binary builds attached to tagged
      GitHub releases; tag/version consistency check. Four targets —
      Linux is musl, so the binary runs wherever the kernel does rather
      than only where the runner's glibc is matched. Building and
      publishing are separate jobs because four matrix jobs racing to
      create one release is a race: they upload artifacts, and a single
      job afterwards makes the release. Both are gated on the tests and
      on the tag matching the workspace version
- [x] 9.5 Report a rename that moved but could not be journaled.
      Resolved by inverting the order rather than by reaching stderr:
      the entry is appended before the file moves, so a move that
      cannot be journaled is not made, and every abandoned move is an
      ordinary `Unjournalable` skip in the event stream. The stderr
      seam this task asked for is no longer needed for this case
- [x] 9.6 Resolve configuration per input directory rather than once
      per run. `.borax.toml` is specified as discovered upward from
      each input file's directory, but a run climbs from the first
      input path only, so one invocation spanning two trees applies the
      first tree's overrides to both — and which tree wins depends on
      the order the paths were typed.
      Done as `Configs`, which resolves each directory once and answers
      per path. Renaming and bibliography output work a directory at a
      time, which they nearly did already — collisions are a property of
      a directory — so a run over one directory produces exactly the
      stream it did before. Network settings stay run-level, and the
      spec now says so: the clients are built once, before any file is
      read
- [x] 9.7 Start the override search from the directory a path names,
      not from its parent. `start_directory` calls `.parent()` on the
      argument unconditionally, which is right for a file and one level
      too high for a directory, so `borax rename ~/library` silently
      ignores `~/library/.borax.toml` while
      `borax rename ~/library/paper.pdf` honours it. It also runs on
      the raw arguments, before `expanded` turns directories into
      files, which is why the two forms disagree. Separate from 9.6:
      that one picks the wrong tree among several, this one misses the
      only tree there is

## 10. Write safety

Fixes to behaviour already specified, found by review after 8.8. Each is
a case where the implementation could lose data a user had, rather than
a feature that was missing.

- [x] 10.1 Tests + implementation: `store::write_atomically`, and the
      master `.bib` written through it. A truncating rewrite put a
      bibliography the user may have kept for years at the mercy of a
      full disk for as long as the write took
- [x] 10.2 Tests + implementation: sidecars named by appending the
      extension (`paper.pdf.bib`, not `paper.bib`) and never overwriting
      a file borax did not write. The old name is the one a person
      keeping notes by hand would have chosen
- [x] 10.3 Tests + implementation: write-ahead journaling (see 9.5)
- [x] 10.4 Tests + implementation: a named input path that cannot be
      reached is kept, so the pipeline reports it. Dropping it let a
      typo produce an empty batch that exited 0
- [x] 10.5 Tests + implementation: a configuration file that is present
      but unreadable ends the run, distinct from one that is absent
- [x] 10.6 Tests + implementation: `SourceName::SUPPORTED`, the sources
      a run may be configured to use. `ALL` keeps naming DataCite and
      PubMed for the dispatch table, but a configuration selecting one
      resolved nothing and reported every file unresolvable

## 11. Deferred, with the reason

Both were in scope for the write-safety review and are held back
deliberately: each is a design change rather than a fix, and doing
either badly costs more than leaving it undone.

- [ ] 11.1 Bounded concurrency across files. `map_bounded` is written
      and tested but requires `F: Fn(I) -> O + Sync`, so wiring it means
      adding `Sync` to the `Source`, `Library`, and `Cache` seams. Every
      test fake counts calls through `Cell`/`RefCell`/`Rc`, none of
      which is `Sync`, so the bound would have to be paid for across
      seven test files at the same time as the three trait definitions.
      Worth doing as its own change, where the seam change and the fake
      rework can be reviewed together
- [ ] 11.2 Nested rename destinations. The templates spec makes `/` a
      directory separator and `sanitize` honours it, but a subdirectory
      target currently fails every file: `RealFilesystem::rename` never
      creates the target's parent, and `Filesystem::existing` lists the
      source's directory rather than the target's, so collisions inside
      a subdirectory are invisible to the planner.
      The fix is not to add a `create_dir_all`. `borax_core::rename::plan`
      is a single-directory planner by contract — one flat, case-folded
      namespace — and that contract is what makes deterministic suffixing
      and case-insensitive collision detection work. Supporting nesting
      means grouping by *target* directory in `plan_renames` rather than
      by source directory, which in turn changes what "already named"
      means for a file whose source and target directories differ: the
      source is no longer a member of the namespace being planned, so
      the `exempt` rule the planner uses cannot apply unchanged.
      Adding the `create_dir_all` alone would be worse than the current
      failure: files would move, but two files claiming one nested name
      would collide at the filesystem instead of being suffixed, losing
      the deterministic-collision guarantee that is the point of the
      planner. Needs its own change and its own spec delta
