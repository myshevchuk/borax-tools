# Design: add-ledger-and-run-logs

## Context

borax's core pipeline (`add-core-pipeline`) is implemented, archived and
released as v0.1.0; `stream-per-file-events` landed on top of it. The
pipeline is stateless: the only persistent state today is the response
cache and the rename journal — a single append-only `renames.jsonl` in
the XDG state directory, which this change removes. The
albumin project's `accession.py` — same disk-is-truth ideology, for
media files — demonstrates two accounting features borax needs
(duplicate prevention via a rebuildable log; per-run manifests for
review) and one convenience (config-file presets for CLI flags). This
design adopts the ideas and rejects the implementations' weak points,
which were reviewed in detail before this change was drafted:

- accession's log DB defaults its identity to a hash of the archive's
  absolute mount path, stored in `XDG_STATE_HOME` — moving or
  re-mounting the archive, or working from another machine, silently
  orphans the accounting. SQLite as the store drags in network-FS/WAL
  fallbacks, busy timeouts, and a whole `log export`/`log import`
  subsystem that exists only because SQLite is neither diffable nor
  committable; the exported text drifts from the live DB between manual
  exports.
- accession's run manifests are a fourth bespoke text format in one
  tool (manifest TSV, export TSV, export JSONL, rejects TSV), with
  lossy escaping (tabs/newlines in filenames collapse to a space), and
  their location is derived from the DB path even when the DB is
  disabled.
- accession's config layer re-declares the CLI surface in a parallel
  key table with hand-rolled string coercion (INI), and its booleans
  have OR semantics: config can switch a flag on, but the CLI cannot
  switch it off unless a dedicated negation flag was hand-written —
  only one ever was. Worth keeping: layered precedence, a set of keys
  forbidden in config (the dry-run gate above all), and loud
  unknown-key validation.

## Goals / Non-Goals

**Goals:**

- Prevent duplicate acquisition at two levels — identical bytes and
  identical work — with distinct, actionable reports.
- Give every run a reviewable, diffable record; make the applied-run
  record the single authority `borax undo` operates on.
- Keep all accounting derived and rebuildable: deleting `.borax/` loses
  bookkeeping, never data or correctness.
- One serialization everywhere: the versioned JSONL event/record
  schemas already specified for `--json`.
- Config presets whose schema cannot drift from the CLI and whose every
  setting the CLI can override in both directions.

**Non-Goals:**

- No library index, search, or query layer (the ledger answers "have I
  admitted this?", not "what do I have?").
- No multi-collection registry; each collection is self-contained.
- No automatic ledger repair; `ledger rebuild` is explicit.
- Nothing here modifies file contents (unchanged from core).

## Decisions

### The ledger is an append-only JSONL file at the collection root

`<collection>/.borax/ledger.jsonl`, one JSON object per admitted file.
Rationale against accession's SQLite-in-XDG:

- **Location**: the accounting travels with the files it accounts. A
  collection moved, re-mounted, or opened on another machine keeps its
  ledger; nothing is keyed to an absolute path. (This is the direct fix
  for accession's orphaning problem.)
- **Format**: the file is its own export — diffable, committable,
  greppable, mergeable in review. No export/import subsystem, no
  drift between a binary DB and its text snapshot, no locking machinery
  on network filesystems (appends are line-atomic in practice; borax
  runs are the only writer and concurrent runs on one collection are
  already out of scope for the rename planner).
- **Scale**: the whole file is read once per run into an in-memory
  index (hash → entry, identifier → entry). At 10^5 entries ×
  ~300 bytes this is ~30 MB of text parsed in well under a second in
  Rust; a database buys nothing at this size.

A ledger entry records: content hash, normalized identifiers, final
path relative to the collection root, entry type, run id, timestamp,
tool version. Appends happen only on applied admissions.

### Duplicate detection is two distinct checks

- **Content duplicate**: incoming file's hash already in the ledger →
  "same bytes already archived at <path>".
- **Work duplicate**: hash unknown, but a resolved identifier (DOI,
  arXiv ID) already in the ledger → "same work already archived at
  <path> (different file)".

Batch policy follows the pipeline's never-guess rule: duplicates are
reported and skipped (diverted to the skip queue with the duplicate
reason and the existing path); nothing is deleted or overwritten.
Content duplicates are detectable before any network work, so the check
runs right after hashing — a re-downloaded file costs one hash, no
resolution.

### The ledger is derived, optional, and degrades loudly

Rebuild: `borax ledger rebuild` scans the collection (files + sidecars,
which carry full records including identifiers), regenerates entries,
and writes the file sorted by relative path — deterministic output, so
rebuilds diff cleanly and double as compaction of superseded entries.
A missing or unparsable ledger (or a run outside any collection) means
duplicate detection is off for that run: borax warns once and proceeds.
The ledger can never block the pipeline, and `--no-ledger` disables it
explicitly.

### Run logs are the event stream, persisted

Every subcommand already emits a typed, versioned event stream (`cli`
spec). A run log is that stream written to
`.borax/runs/<UTC-timestamp>-<command>-<dry|apply>.jsonl`. One schema
serves terminal `--json`, the run record, and undo — where accession
grew four formats. The `<dry|apply>` suffix keeps accession's one
genuinely good manifest idea: a dry plan sorts directly above the apply
record that follows it.

### Apply-run logs are mandatory and pre-flushed; they are the journal

For an `--apply` run, the run log is created and the planned rename
events flushed to disk *before the first rename executes*; failure to
create or write it aborts the run before any mutation. This is the
shipped journal's invariant, moved rather than invented: `--apply`
already refuses when the system names no state directory to journal
into. The undo journal is removed: `borax undo` reads
the most recent apply-run log and replays its rename events in reverse,
verifying each file sits at its recorded new path with its recorded
hash (verification semantics unchanged from the journal spec).
Rationale: two files recording the same facts in two formats is exactly
the accession disease; and accession's best-effort manifest writes mean
an executive run's record can silently not exist — unacceptable once
the record is what undo depends on. Dry-run logs stay optional
(`--no-run-log`) and best-effort, since nothing depends on them.

### Outside a collection, apply logs fall back to XDG state

Undo must work everywhere, including a one-off rename in a downloads
directory. Without a collection root, apply-run logs go to the XDG
state directory (`borax` subdirectory); the ledger is simply inactive.
`borax undo` searches the collection of the current directory first,
then XDG state, most recent apply log wins.

### Collection root = nearest `.borax.toml`

The `cli` spec's per-directory config discovery already walks upward
for `.borax.toml`; the nearest one now also defines the collection root
that anchors `.borax/`. One mechanism, two roles; an explicit
`collection-root` config key can override for unusual layouts.

### Config: one schema, typed values, symmetric booleans

Most of this is already true in the shipped code and is written down
here to be locked in and held, not built: `Config` is typed TOML behind
`deny_unknown_fields`, so unknown keys and wrong types are already
load-time errors; `--apply` is not a `Config` field, so `apply = true`
is already rejected; and both config-settable booleans (`sidecars`,
`cache`) already have `--no-` forms. The work this change actually
carries is the `collection-root` key, and keeping the negation property
total as it adds `ledger` and `run-log` booleans of its own.

- CLI flags and TOML keys are generated from a single declaration
  (shared clap+serde structs); a key exists in config exactly when its
  flag is declared configurable — no parallel table to drift, unlike
  accession's `CONFIG_SPEC`.
- TOML is natively typed: a boolean is `true`, a list is an array — no
  string-coercion layer, wrong types are load-time errors naming key
  and expected type.
- Every config-settable boolean flag has an auto-generated `--no-*`
  negation, so the CLI can always override config in *both* directions
  — fixing accession's one-way OR semantics instead of hand-writing
  negations one by one.
- Never settable from config: `--apply` (the dry-run gate) and one-off
  destructive selectors (accession's `FORBIDDEN_KEYS` rule, kept).
- Unknown config keys are load-time errors (accession's loud
  validation, kept).

### Nothing migrates off the old journal

A `renames.jsonl` written by v0.1.0 is not read, not converted, and not
deleted; it simply stops being consulted, and the file is left where it
lies rather than cleaned up behind the user's back. `CLAUDE.md` owes no
compatibility before `1.0.0`, and the journal is derived accounting,
so the entire loss is that an undo spanning the upgrade — apply on the
old binary, undo on the new one — does not find its run. A reader kept
alive for that window would be exactly the "legacy branch" the project
rules out.

## Risks / Trade-offs

- [Append-only file corruption mid-write (power loss) leaves a torn
  last line] → the reader treats a torn trailing line as absent,
  warns, and continues; `ledger rebuild` restores the full state from
  disk. The ledger is accounting, so the blast radius is one missed
  dedup warning.
- [Two runs on one collection could interleave appends] → concurrent
  runs against one collection are already unsupported by the rename
  planner; the run id in each entry makes any interleaving visible
  after the fact. A lock file is deliberately not introduced.
- [`.borax/` inside the collection pollutes the tree some users want
  pristine] → it is one dotted directory, the price of accounting that
  travels with the data; `--no-ledger` plus run logs' XDG fallback
  give a zero-footprint mode.
- [Run logs accumulate] → they are small text files; a retention
  policy (`borax runs prune`) is deferred to a future change rather
  than speculated on now.
- [Undo now depends on run-log schema stability] → the event schema is
  already versioned per the `cli` spec; undo refuses (with a clear
  error) to replay a log whose schema version it does not understand.

## Open Questions

- Whether `ledger rebuild` should also consult the master `.bib` when
  sidecars are absent (decide when implementing; the spec requires
  sidecars + files only).
- Exact XDG state layout for out-of-collection apply logs (flat vs.
  per-directory-key subdirectories) — implementation detail, spec
  constrains discoverability by `borax undo` only.
