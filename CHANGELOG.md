# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

### Added

- `borax resolve` now emits the whole canonical record on each
  `resolved` event, not just the identifier it looked up — the JSON
  stream is the composable interface, and a caller should not have to
  go back to the network for what borax already resolved.
- Skip events for a metadata conflict now report how similar the two
  titles were, so a near miss can be told from two unrelated works.
