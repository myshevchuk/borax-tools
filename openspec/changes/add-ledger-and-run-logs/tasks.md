# Tasks: add-ledger-and-run-logs

Depends on `add-core-pipeline` implementation (record model, planner,
event schema, config resolution). TDD throughout: red test first for
every pure behaviour.

## 1. Ledger core (borax-core)

- [ ] 1.1 Tests + implementation: ledger entry model (hash,
      identifiers, relative path, entry type, run id, timestamp, tool
      version) with lossless JSONL round-trip
- [ ] 1.2 Tests + implementation: in-memory ledger index (hash → entry,
      identifier → entry) built from parsed lines; torn trailing line
      ignored with a reported warning; mid-file corruption reported as
      unparsable
- [ ] 1.3 Tests + implementation: dual duplicate check with distinct
      reasons (content duplicate before resolution; work duplicate
      after) feeding the skip queue
- [ ] 1.4 Tests + implementation: deterministic rebuild serialization
      (entries sorted by relative path, byte-stable output)

## 2. Ledger adapter and subcommand (borax)

- [ ] 2.1 Tests + implementation: collection-root discovery (nearest
      `.borax.toml`, `collection-root` override) shared with config
      resolution
- [ ] 2.2 Tests + implementation: ledger load/append adapter with
      degrade-loudly behaviour (absent, unparsable, no collection,
      `--no-ledger`)
- [ ] 2.3 Tests + implementation: `borax ledger rebuild` scanning files
      + sidecars; compaction of entries for missing files
- [ ] 2.4 Tests + implementation: pipeline wiring — content check after
      hashing, work check after resolution, applied admissions append

## 3. Run logs (borax)

- [ ] 3.1 Tests + implementation: run-log writer persisting the event
      stream to `<UTC>-<command>-<dry|apply>.jsonl`; equality with the
      `--json` stream
- [ ] 3.2 Tests + implementation: mandatory apply-log path — created
      and plan-events flushed before first rename; abort-before-mutation
      when unwritable; `--no-run-log` affects dry runs only
- [ ] 3.3 Tests + implementation: XDG state fallback outside a
      collection, discoverable by undo

## 4. Undo rewiring (borax)

- [ ] 4.1 Tests + implementation: replace journal reads with
      latest-apply-log discovery (collection first, then XDG state)
- [ ] 4.2 Tests + implementation: reverse replay with per-entry hash
      verification (carried over) and schema-version refusal

## 5. Config hardening (borax)

- [ ] 5.1 Tests + implementation: single option schema emitting both
      clap flags and serde TOML keys; unknown-key and wrong-type
      load-time errors naming the key
- [ ] 5.2 Tests + implementation: auto-generated `--no-*` negations for
      config-settable booleans, last-one-wins
- [ ] 5.3 Tests + implementation: config-forbidden set (`apply`,
      destructive selectors) rejected at load with the
      pass-on-command-line message

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
