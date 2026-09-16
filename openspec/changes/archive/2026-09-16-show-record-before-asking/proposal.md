## Why

`add-interactive-rename` asks one question per file: this file, this new
name, rename or skip. The new name is a rendering of a record, and
whether the record is the right one is the thing the operator is
actually judging. A name like `zeng2026_PiezochiralEffect.pdf` is
plausible for the wrong paper as easily as for the right one, and the
failure borax most needs a person to catch — a text-layer DOI that
belongs to a reference in the bibliography rather than to the paper —
produces exactly such a plausible name.

The run already holds what the judgement rests on. It knows which
identifier it used and which extraction pass found it, which service
answered, the whole record, and the titles the file claims for itself.
Almost none of it reaches the terminal: the resolution line reads
`resolved doi:… via crossref`, and the file's own titles are read,
compared, and dropped.

## What Changes

- **Each question is preceded by a description of the file.** Before
  asking, an interactive run shows the file and its position in the run;
  the identifier and where it was found (embedded metadata, the text
  layer, or the content index from an earlier run); the service or
  services that supplied the record; the record's type, title, authors,
  date, container, volume, issue, pages and publisher, as far as it has
  them; the titles the file claims for itself and where each was read;
  and the proposed name, with a note when it was suffixed because the
  rendered name was taken.
- **The file's own titles reach the stream.** The `resolved` event
  gains the titles the file claims, each with where it was read, so the
  description tells the operator nothing the event stream does not also
  carry and a `--json` consumer gains the same evidence.
- **The identifier the description names is the one extraction found.**
  A `resolved` event today reports the record's identifier, preferring
  its DOI, which is not always what was looked up: an arXiv identifier
  found in the text layer resolves to a record carrying both, and the
  event names the DOI. Saying that came "from the text layer" would be
  false about the one judgement this change exists to inform, so the
  event carries the identifier that was looked up as well as the
  record's own.
- **The description is part of the question, so it goes where the
  question goes.** It is written to standard error beside the menu, not
  into the event stream on standard output. A run whose stdout is
  redirected to a file still asks its questions with the evidence
  attached, and the report in the file is what it always was.
- **The source survives a content-index answer.** When the content
  index answers, the event names no service today. The description, and
  the event, name the services the record's per-field provenance
  records instead, in a fixed order and with a sidecar named as one.
- **Batch output keeps its shape.** The description replaces the
  one-line resolution report only in interactive runs, where there is a
  decision for it to inform. The batch line changes in one word: a
  content-index answer reads `via crossref (cached)` rather than `via
  cache (cached)`.

## Capabilities

### Modified Capabilities

- `rename`: an interactive question is preceded by a description of the
  file and the record behind the proposal.
- `resolution`: a resolution reports the titles the file claims, and
  names the record's sources even when the content index answered.

## Impact

- `crates/borax/src/pipeline.rs`: `extract_from` keeps where each
  claimed title was read; `FileRecord` carries the claims. A content
  index answer carries none, because the file was not opened.
- `crates/borax/src/event.rs`: `Event::Resolved` gains `claims` and
  `found` — the identifier that was looked up, with the pass that found
  it — and its `source` is filled from provenance when the content
  index answered.
- New module `crates/borax/src/describe.rs`: a pure function from a
  `resolved` event, a proposed rename and a terminal width to the lines
  of the description. Tested by exact expected output; no terminal
  involved.
- `crates/borax/src/run.rs`: the interactive renderer prints the
  description in place of the resolution line before each question.
- Documents a person reads: `docs/manual.org` (`borax rename`, the event
  reference if it lists `resolved`'s fields), `CHANGELOG.md`,
  `openspec/STATE.md`.

## Deferred

- **Describing files that ask nothing.** Skipped and already-named files
  keep their one-line reports. `supply-identifiers-interactively` gives
  some of them questions, and they gain the description when they do.
- **A description in batch output.** A batch report is scanned after
  the fact, and a dozen lines per file would bury the outcomes. A
  `--verbose` rendering is possible later from the same function.
- **Whether a response-cache hit behind a service is visible.** The
  event's `cached` flag reports only the content index; a response
  served from the on-disk cache still reads as live. The description
  inherits that and does not claim more.
