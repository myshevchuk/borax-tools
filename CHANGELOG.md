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

### Added

- Skip events for a metadata conflict now report how similar the two
  titles were, so a near miss can be told from two unrelated works.
