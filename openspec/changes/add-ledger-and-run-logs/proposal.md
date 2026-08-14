# Proposal: add-ledger-and-run-logs

## Why

Two accounting concerns are missing from the core pipeline: nothing
prevents acquiring a paper twice (as re-downloaded bytes or as a second
file of the same work), and nothing but terminal scrollback records what
a run planned or did. The albumin project's `accession.py` proved both
ideas — a rebuildable log database and per-run manifest files — but its
implementations carry avoidable clumsiness (a mount-path-keyed SQLite
that orphans when the archive moves, four parallel text formats, a
config layer that re-declares the CLI surface and cannot be negated from
the command line). This change specifies both features for borax with
those lessons applied, and hardens the configuration contract; the
improved design is intended to flow back into accession later.

## What Changes

- A **collection ledger**: an optional, append-only JSON Lines file at
  the collection root (`.borax/ledger.jsonl`) recording every admitted
  file (content hash, identifiers, final path). It powers duplicate
  detection at two levels, reported distinctly: same bytes already
  archived, and same work (identifier) already archived as a different
  file. The ledger is derived accounting, rebuildable from disk
  (`borax ledger rebuild`), never a source of truth.
- **Run logs**: every run persists its JSON Lines event stream — the
  same versioned schema `--json` prints — to `.borax/runs/`, named so a
  dry run pairs visibly with the apply run that follows it. Apply-run
  logs are mandatory and flushed before the first rename executes;
  dry-run logs are optional.
- **Journal unification** (**BREAKING** relative to the unimplemented
  `add-core-pipeline` spec): the separate undo journal is removed;
  `borax undo` replays the latest apply-run log's rename events in
  reverse, with the same per-entry hash verification.
- A **collection root** concept in configuration: the nearest directory
  containing `.borax.toml` anchors `.borax/`. Apply-run logs fall back
  to the XDG state directory outside any collection; the ledger is
  simply inactive there.
- **Config hardening**: CLI flags and TOML keys are generated from one
  option schema (no parallel spec table to drift); every
  config-settable boolean has a `--no-*` negation so the command line
  overrides configuration in both directions; `--apply` and one-off
  destructive selectors are never settable from config; unknown config
  keys are load-time errors.

## Capabilities

### New Capabilities

- `ledger`: the collection ledger — format, location, optionality,
  dual-level duplicate detection, batch duplicate policy, deterministic
  rebuild, degradation when missing or corrupt.
- `run-logs`: per-run event-stream persistence — location, naming,
  dry/apply pairing, mandatory pre-flushed apply logs, XDG fallback.

### Modified Capabilities

- `rename`: the journal requirements are replaced — applied runs are
  recorded by the run log, and undo operates on it.
- `cli`: configuration resolution gains collection-root discovery;
  new requirements bind config keys and CLI flags to one schema, add
  boolean negations, forbid apply-gate keys in config, and make
  unknown keys errors.

## Impact

- `crates/borax-core`: ledger record model, in-memory duplicate index,
  deterministic rebuild serialization (pure).
- `crates/borax`: `.borax/` discovery, run-log writer, `borax undo`
  rewiring, `borax ledger` subcommand, config schema unification.
- No implementation exists yet for either change, so the `rename`/`cli`
  modifications cost nothing at runtime; `add-core-pipeline` should be
  implemented with this change's final shape in mind.
