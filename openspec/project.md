# Project conventions: borax-tools

Context for anyone (human or AI) working on this repository. Behavioural
requirements live in `openspec/specs/`; this file records the conventions
around them.

## What this project is

A CLI suite for bibliography work. Core pipeline: given a file (typically
a PDF), extract an identifier, resolve bibliographic metadata from online
sources, and rename the file by configurable template rules. Stateless by
design (the only persistent state is a response cache and the rename
journal); "fast and error-free" are the two governing requirements, in
that order of scrutiny but with error-free winning any conflict.

## Architecture

- Rust, Cargo workspace, single `borax` binary with subcommands.
- Pure-logic/adapter split: `crates/borax-core` has no I/O; network lives
  in `crates/borax-sources`, PDF access in `crates/borax-pdf`, terminal
  and filesystem wiring in `crates/borax`.
- Canonical record model is a CSL-JSON superset; BibTeX is an emission
  format, never the internal model.
- Every subcommand emits a typed event stream; `--json` (JSON Lines) and
  human output are two renderings of it. JSONL schemas are the intended
  public integration contract, but they are not frozen: before `1.0.0`
  they may change in any release (see `CLAUDE.md`).

## Development workflow

- Spec-driven: behaviour changes start as an OpenSpec change proposal
  (`openspec/changes/`), are validated, implemented, then archived into
  the living specs. Each change is implemented on a `change/<name>`
  branch; only pure infrastructure lands directly on `main`.
- TDD is non-negotiable: every pure behaviour lands red-test-first in
  `borax-core`. Adapters are tested against recorded HTTP cassettes and a
  real-PDF fixture corpus; a scheduled CI job runs live contract tests.
- Windows is first-class: filename semantics, path handling, and CI all
  treat it as a primary platform.
- A `## MODIFIED Requirements` block replaces a living requirement
  wholesale and is matched by title alone, so it is a snapshot of that
  requirement as it read when the delta was written. If a sibling
  change archives first and edits the same requirement, archiving this
  one reverts the sibling's edit and says nothing.
  `scripts/check-spec-deltas.py` fails when a delta drops a scenario or
  a paragraph the living spec still carries; CI runs it. Deliberate
  removals — usually the point of the change — are declared with a
  marker above the requirement:

  ```markdown
  <!-- drops: why this content is going away -->
  ```

  Re-copy the requirement from `openspec/specs/` and re-apply the edit
  when the check fires on something you meant to keep. After archiving,
  `git diff openspec/specs` should show no deletion you cannot account
  for.

## Conventions

- Conventional Commits, 50/72; changelog discipline per Keep a
  Changelog. Versions are `0.y.z` release counters and promise no
  compatibility — `CLAUDE.md` is the authority on that.
- License: MIT OR Apache-2.0. AGPL dependencies are not acceptable
  (this ruled out MuPDF as the PDF backend).
- Network requests always identify the tool (User-Agent, configurable
  mailto) and are rate-limited; tests never hit the network.
