# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `rename` now checks each file against the collection's ledger and
  skips the ones already admitted, so re-downloading a paper you
  already filed no longer files it twice. A file is a duplicate by
  content when its bytes are already recorded — caught after hashing,
  before any network request — and by work when a different file
  resolves to an identifier already recorded; both are reported with
  the path of the file they duplicate and leave the source untouched.
  The ledger lives at `.borax/ledger.jsonl` under the directory holding
  the nearest `.borax.toml`, so it travels with the files it accounts
  for. Only an applied run records anything; a preview records nothing.
  Disk decides: an entry whose file has been moved or deleted never
  keeps a file out, and a run that meets one warns that
  `borax ledger rebuild` is due. A missing or unreadable ledger costs
  the run its duplicate detection and nothing else, and `--no-ledger`
  turns it off outright.
- `borax ledger rebuild` regenerates the ledger from the collection
  itself, scanning its files and the sidecars beside them, so the
  accounting is derived rather than authoritative: delete
  `.borax/ledger.jsonl` and a rebuild restores it. Entries come out
  sorted by path, which makes two rebuilds of an unchanged collection
  byte-identical and a ledger something you can keep under version
  control and read a diff of. A file the scan cannot account for —
  no sidecar, no record borax wrote, or no readable bytes — yields no
  entry, so a rebuild also compacts away whatever has since been
  deleted or moved. The new ledger replaces the old one in a single
  atomic write, leaving the previous one intact if the rebuild cannot
  finish.
- Every run can now leave a record of itself. A run log is the same
  versioned event stream `--json` prints, written to
  `.borax/runs/<UTC-timestamp>-<command>-<dry|apply>.jsonl`, so a
  preview and the run that applied it sit next to each other in a
  listing and a directory of them sorts by time. The log is written
  whichever output format the terminal got, since the format chooses
  what you read, not what is recorded. `--no-run-log` suppresses it,
  and a log that cannot be written warns without failing the run —
  except for `borax rename --apply`, whose log is created before the
  first file moves and whose failure stops the run while every file
  still has its original name. An applying rename outside any
  collection writes that log under the XDG state directory instead, so
  a one-off rename in a downloads directory stays recoverable.

### Changed

- The `renamed` event carries the content hash of the file it moved,
  which is what lets the run log stand in for the journal undo used to
  read. The `unjournalable` skip reason is now `unrecordable`, naming
  what it always meant rather than the component that has been
  removed.
- Citation keys now come from their own `[citation-keys]` table
  instead of the `[templates]` table that names files, and their
  built-in default is `[auth:lower][year]`, so a 2024 paper by Smith is
  cited as `smith2024`. Keys written by 0.1.0 were the rendered file
  name with whitespace and `,{}%` removed, so this release cites the
  same works under different keys; setting `citation-keys.default` to
  your file-name template restores the old keys exactly. Changing how
  files are named no longer changes how works are cited.
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

### Removed

- The rename journal is gone. `borax undo` now reads the most recent
  apply-run log instead, which records the same moves with the same
  content hashes and is verified the same way, so undo behaves as it
  did — it simply reads the record every run already writes rather
  than a second file kept only for it. One consequence is worth
  stating plainly: an undo spanning the upgrade will not find its run.
  If you applied a rename with 0.1.0 and upgrade before undoing it,
  that run is not recoverable through borax. A `renames.jsonl` written
  by 0.1.0 is left exactly where it lies — not read, not converted,
  and not deleted behind your back.

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
