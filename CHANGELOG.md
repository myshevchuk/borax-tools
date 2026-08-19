# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- A run reports as it goes. Each event is written when it happens
  rather than after the run has ended, so a network-bound run shows
  progress while it is bound and a slow run is distinguishable from a
  stopped one. A run ended by a configuration or usage error still
  emits neither `run-started` nor `run-finished`.
- `rename` and `bib` report one file at a time. A file's resolution,
  the rename planned or applied for it, and any sidecar written beside
  it now appear together and in the order the files were given,
  instead of every file's resolution followed by every file's rename —
  two lists that could not be paired, since the second omitted the
  files that were skipped. Output written to a shared master `.bib`
  is unchanged and still follows the files of its directory. No file
  receives a different name for this: collision suffixes stay
  deterministic and a preview stays identical to what `--apply` does.

## [0.1.0] - 2026-08-17

The first release. Nothing was published before it, so the "Changed"
and "Fixed" entries below record work done during development rather
than changes to a version anyone could have installed; "Added" opens
with the capability set as a whole and then lists what arrived late in
that development.

### Added

- The `borax` binary, with `resolve` (report what each file resolves
  to), `rename` (rename each file after its record), `bib` (write
  bibliography output and rename nothing), `undo` (move back everything
  the last applied run moved), `config` (print every setting and the
  layer it came from), and `cache` (report or clear the response
  cache).
- Renaming previews by default and moves only under `--apply`, never
  overwrites a file it did not plan for, and journals each move before
  making it so `borax undo` can reverse an interrupted run.
- Metadata resolution against Crossref, OpenAlex and arXiv, on a
  bounded thread pool, paced per service and cached on disk.
  Identifiers are extracted from the PDF in tiers — embedded metadata
  first, then a bounded scan of the text layer — and the pure-Rust
  extractor needs no native toolchain on any platform.
- A template language for filenames: bracket tokens over a record's
  fields, chained filters, `||` alternatives, `/` to file into
  subdirectories, and per-entry-type templates. Rendered names go
  through a sanitization pass that cannot be disabled, so a name is
  valid on Linux, macOS and Windows at once. The README documents the
  language.
- BibTeX output, either merged into one master `.bib` or written as a
  sidecar beside each file.
- Configuration in layers — built-in defaults, a global `config.toml`,
  a per-directory `.borax.toml`, environment variables, then flags —
  with `borax config` reporting the origin of every effective value.
- A JSON Lines event stream behind `--json` on every subcommand, as the
  integration contract for editor and watcher tooling.
- `borax resolve` emits the whole canonical record on each `resolved`
  event, not just the identifier it looked up — the JSON stream is the
  composable interface, and a caller should not have to go back to the
  network for what borax already resolved.
- Skip events for a metadata conflict report how similar the two titles
  were, so a near miss can be told from two unrelated works.

### Changed

- Files are resolved on a bounded pool of threads rather than one at a
  time, so a directory of PDFs is processed several times faster.
  `network.concurrency` sets the width. The event stream is unaffected:
  results are written back at their input position, so the JSON output
  is identical to a sequential run's and stays diffable. Requests stay
  paced per service, so more threads do not mean a faster rate of
  asking.

- A template containing `/` files a document into a subdirectory, which
  the templates specification always described and which failed for
  every file: the subdirectory was never created, and a name already
  taken inside it was invisible to the collision planner.

### Fixed

- A run spanning two directory trees applies each tree's own
  `.borax.toml` to its own files. Configuration was resolved once from
  the first input path, so the whole run took the first tree's
  overrides and swapping the arguments changed the result.
- `borax rename <dir>` finds `<dir>/.borax.toml` again. The override
  search took the parent of whatever path it was given, which is right
  for a file and one level too high for a directory, so naming a
  directory silently ran on the configuration above it while naming a
  file inside it did not.
- Title-conflict detection no longer skips a file over typographic
  differences it cannot control. Titles are compared as content words
  with both sides folded toward the lossiest encoding either could
  carry, and judged by similarity rather than exact equality, so a
  character the PDF's producer could not encode (a Greek letter, a
  Unicode hyphen) no longer reads as a different work.
- A PDF's title claims are no longer trusted blindly. Placeholders
  (`untitled`, `PowerPoint-Präsentation`), filenames, and bare
  identifiers no longer count as evidence of disagreement, and a
  document's XMP and Info titles are both considered rather than one
  preferred — agreement by either clears the file.

- The master `.bib` is replaced atomically instead of being truncated
  and rewritten, so a run that dies mid-write leaves the previous
  bibliography intact rather than a truncated one.
- Sidecars are named `paper.pdf.bib` rather than `paper.bib`, and a file
  borax did not write is never overwritten — the old name is the one a
  person keeping notes on `paper.pdf` by hand would have chosen.
- Applied renames are journaled before the file moves, not after the
  batch finishes. A run interrupted partway through no longer leaves
  moves that `borax undo` cannot see, and a move that cannot be
  journaled is no longer made.
- A named input path that does not exist is reported as a skipped file.
  It used to contribute nothing, so a mistyped filename produced an
  empty batch that exited 0, indistinguishable from a clean run.
- A `.borax.toml` that exists but cannot be read now ends the run
  instead of being silently ignored as though it were absent.
- Sources borax has no client for (DataCite, PubMed) are no longer in
  the default set and are refused by configuration, naming the sources
  that do work. Selecting one used to resolve nothing and report every
  file unresolvable.

- Requests to a service are now spaced out. `--min-interval-ms` and
  `network.min-interval-ms` were accepted, validated and reported while
  changing nothing, so borax asked for polite-pool access and
  rate-limited none of its traffic.
- Successful source responses are now cached on disk, not just the
  content-hash index. Two different files carrying the same DOI, or a
  re-run after the index is cleared, no longer re-query the service.

- arXiv identifiers issued before April 2007 (`math.GT/0309136`)
  resolve again. The reader took the abstract URL's last path segment,
  which drops the archive that is part of such an identifier, so every
  pre-2007 preprint failed with a malformed-response error.

[Unreleased]: https://github.com/myshevchuk/borax-tools/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/myshevchuk/borax-tools/releases/tag/v0.1.0
