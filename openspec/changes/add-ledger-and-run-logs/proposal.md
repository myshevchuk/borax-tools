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
- **Journal unification** (**BREAKING**): the separate undo journal —
  shipped in v0.1.0 as a single append-only `renames.jsonl` in the XDG
  state directory — is removed outright, code and tests with it.
  `borax undo` replays the latest apply-run log's rename events in
  reverse, with the same per-entry hash verification. Journals written
  by an earlier release are not read and not migrated; per `CLAUDE.md`
  no compatibility is owed before `1.0.0`, and the accounting is
  derived, so the cost of dropping it is one lost `undo` for anyone who
  upgrades between an apply run and its undo.
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
  rewiring, `borax ledger` subcommand, `collection-root` config key.
- **This change edits working code, not a plan.** `add-core-pipeline` is
  implemented, archived, and released as v0.1.0, so the `rename`
  modification deletes `crates/borax/src/journal.rs` and its 1107 lines
  of tests and rebuilds `borax undo` on the run log. The apply-run log
  inherits the journal's hard invariant unchanged: `--apply` refuses to
  run when there is nowhere to record the moves, because an unrecorded
  rename cannot be undone.
- Parts of the config hardening are already shipped and need verifying
  rather than building: unknown keys and wrongly typed values are
  already load-time errors (`deny_unknown_fields` over typed TOML), and
  both config-settable booleans already have `--no-` negations. What is
  genuinely new is the `collection-root` key, and holding the negation
  property as this change adds booleans of its own.
