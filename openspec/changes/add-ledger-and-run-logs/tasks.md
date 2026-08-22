# Tasks: add-ledger-and-run-logs

`add-core-pipeline` is implemented, archived and released as v0.1.0, so
everything this change builds on (record model, planner, event schema,
config resolution) exists. That also means this change edits working
code: group 4 deletes the shipped journal rather than writing a new
component, and group 5 is mostly verification of behaviour v0.1.0
already has.

TDD throughout: red test first for every pure behaviour.

## 1. Ledger core (borax-core)

- [x] 1.1 Tests + implementation: ledger entry model (hash,
      identifiers, relative path, entry type, run id, timestamp, tool
      version) with lossless JSONL round-trip
- [x] 1.2 Tests + implementation: in-memory ledger index (hash → entry,
      identifier → entry) built from parsed lines; torn trailing line
      ignored with a reported warning; mid-file corruption reported as
      unparsable
- [x] 1.3 Tests + implementation: dual duplicate check with distinct
      reasons (content duplicate before resolution; work duplicate
      after) feeding the skip queue
- [x] 1.4 Tests + implementation: deterministic rebuild serialization
      (entries sorted by relative path, byte-stable output)

## 2. Ledger adapter and subcommand (borax)

- [x] 2.1 Tests + implementation: collection-root discovery (nearest
      `.borax.toml`, `collection-root` override) shared with config
      resolution
- [x] 2.2 Tests + implementation: ledger load/append adapter with
      degrade-loudly behaviour (absent, unparsable, no collection,
      `--no-ledger`)
- [x] 2.3 Tests + implementation: `borax ledger rebuild` scanning files
      + sidecars; compaction of entries for missing files
- [x] 2.4 Tests + implementation: pipeline wiring — content check after
      hashing, work check after resolution, applied admissions append
      (units only: `resolve_file_checking_ledger` and `admission_entry`
      are pinned and green; composing them into `Adapters` is 2.5)
- [x] 2.5 Tests + implementation: carry the ledger and the collection
      root on `Adapters`, so `rename` runs the duplicate checks and an
      applied run appends its admissions. Touches 36 `Adapters` literals
      across the test suite; do it once, with the run-log plumbing of
      group 3, which needs the same collection root — done by adding
      `state_root` in the same pass, unused until group 3 reads it
- [x] 2.6 Tests + implementation: dispatch `borax ledger rebuild` —
      a rebuilt event, `scan_collection` + `rebuild` + write — and drop
      the preflight refusal standing in for it. `events_for`'s catch-all
      must never be what a real subcommand falls through

## 3. Run logs (borax)

- [x] 3.1 Tests + implementation: run-log writer persisting the event
      stream to `<UTC>-<command>-<dry|apply>.jsonl`; equality with the
      `--json` stream
- [x] 3.2 Tests + implementation: mandatory apply-log path — created
      and plan-events flushed before first rename; abort-before-mutation
      when unwritable; `--no-run-log` affects dry runs only
- [x] 3.3 Tests + implementation: XDG state fallback outside a
      collection, discoverable by undo

## 4. Undo rewiring (borax), replacing the shipped journal

- [x] 4.1 Tests + implementation: replace journal reads with
      latest-apply-log discovery (collection first, then XDG state)
- [x] 4.2 Tests + implementation: reverse replay with per-entry hash
      verification (carried over from the journal, behaviour unchanged)
      and schema-version refusal
- [ ] 4.3 Carry over the journal's apply gate: `--apply` aborts before
      touching anything when the apply-run log cannot be created or
      written, with a message naming the log rather than the journal
- [x] 4.4 Delete `crates/borax/src/journal.rs`, its tests, and the
      `Journal`/`FileJournal` adapter wiring in `run.rs`; port the
      cases in `crates/borax/tests/journal.rs` that still describe
      wanted behaviour onto the run-log path and drop the rest. Nothing
      reads a v0.1.0 `renames.jsonl` afterwards

## 5. Config hardening (borax)

Verification-first: 5.1 and 5.3 are largely shipped, so each starts by
writing the test that proves it and only then changes code if the test
is red.

- [ ] 5.1 Tests: unknown key and wrongly typed value are load-time
      errors naming the key and expected type (`deny_unknown_fields`
      over typed TOML should already give this); implement only what
      the tests show missing
- [x] 5.2 Tests + implementation: `--no-` negation for every
      config-settable boolean, including the `ledger` and `run-log`
      booleans this change adds. All four — `sidecars`, `cache`,
      `ledger`, `run-log` — reject an invocation giving both forms
      rather than resolving to the last one given; the spec was
      reworded to the shipped convention rather than v0.1.0's public
      surface changed under it
- [ ] 5.3 Tests: `apply = true` in a config file is a load-time error;
      implement the message naming it as command-line-only if the
      shipped unknown-key wording does not already say so
- [x] 5.4 Tests + implementation: the `collection-root` key overriding
      discovery, with `borax config` reporting its origin

## 6. Integration

- [ ] 6.1 End-to-end test: batch with a content duplicate and a work
      duplicate — skip reasons, untouched sources, ledger unchanged
- [ ] 6.2 End-to-end test: apply run → ledger append + apply log →
      `borax undo` → files restored, ledger state after undo defined
      and tested
- [ ] 6.3 End-to-end test: rebuild determinism on a fixture collection
      (byte-identical double rebuild; compaction after deletion)
- [ ] 6.4 Windows CI coverage for collection-root discovery and run-log
      paths
- [ ] 6.5 `CHANGELOG.md` entry under `[Unreleased]` stating plainly
      that the journal is gone and an undo spanning the upgrade will
      not find its run
