# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

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

### Added

- Skip events for a metadata conflict now report how similar the two
  titles were, so a near miss can be told from two unrelated works.
