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
  human output are two renderings of it. JSONL schemas are the public
  integration contract and follow SemVer with the binary.

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

## Conventions

- Conventional Commits, 50/72; version and changelog discipline per Keep
  a Changelog and SemVer (`0.y.z` until the stable surface exists).
- License: MIT OR Apache-2.0. AGPL dependencies are not acceptable
  (this ruled out MuPDF as the PDF backend).
- Network requests always identify the tool (User-Agent, configurable
  mailto) and are rate-limited; tests never hit the network.
