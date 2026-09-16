# Tasks: show-record-before-asking

Work on branch `change/show-record-before-asking`, stacked on
`change/skip-already-named-files`. It depends only on
`add-interactive-rename` and does not touch what the change below it
does; the stack is linear so that `supply-identifiers-interactively`,
which needs both, has one parent.

Every group is a red/green pair: the `-tests` task is written and run
failing first, and the implementation task makes it pass without
touching it.

## 1. The evidence reaches the stream

- [x] 1.1 Red: `crates/borax/tests/pipeline.rs` — a live resolution's
      `FileRecord` carries the XMP and document-information titles with
      their origins, including a placeholder the conflict check
      dismissed; a content-index answer carries none
- [x] 1.2 Red: `crates/borax/tests/event.rs` — `resolved` serializes
      `claims` as design D3 shows; a content-index answer whose
      provenance names Crossref reports `source: "crossref"` and
      `cached: true`; one whose provenance names no service keeps
      `"cache"`
- [x] 1.3 Green: `extract_from` keeps origins; `FileRecord.claims`;
      `Event::Resolved.claims`; source from provenance (design D4)

## 2. The description

- [x] 2.1 Red: `crates/borax/tests/describe.rs` — exact expected lines
      for: a full journal article from the text layer; a content-index
      answer (design D1's example); a preprint with no container; a
      record with five authors; a suffixed proposal; a title wrapped at
      width 60 with hanging indent and nothing truncated
- [x] 2.2 Green: `crates/borax/src/describe.rs` (design D2), including
      the `Proposal` carrying the unsuffixed rendering from
      `Planning::propose`

## 3. Wiring

- [x] 3.1 Red: `crates/borax/tests/run.rs` — with a scripted asker, the
      interactive human output shows the description in place of the
      resolution line before each question; batch human output is
      unchanged apart from the D4 source wording
- [x] 3.2 Green: the interactive renderer calls `describe` with the
      terminal width (80 when unknown)
- [x] 3.3 Verify by hand on a copy of the real-PDF corpus that the
      description is legible at 80 columns and on a wide terminal

## 4. Documents

- [x] 4.1 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, and `resolved`'s fields wherever the manual lists them)
      and `CHANGELOG.md`, with this proposal as source; check the diff
- [x] 4.2 Record the built state in `openspec/STATE.md`
- [x] 4.3 `openspec validate show-record-before-asking --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green
