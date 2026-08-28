# Tasks: remove-undo

This change deletes shipped, working, tested code. There is no red-green
cycle to run for behaviour that is going away: the TDD rule in
`CLAUDE.md` governs behaviour expressible as pure functions, and nothing
here adds any. What replaces the red test is the deletion of the tests
that pinned the removed behaviour, in the same commit as the code they
pinned, so the suite is never green about a command that no longer
exists.

The one place a test is *written* rather than deleted is group 3, where
`end_to_end.rs` loses a production helper it was borrowing.

Work on branch `change/remove-undo`. Each group is a commit.

## 1. Remove the undo engine and its command

- [x] 1.1 Delete `crates/borax/src/undo.rs` and
      `crates/borax/tests/undo.rs`, and drop the `mod undo;` declaration
      and any `pub use` of it from `crates/borax/src/lib.rs`
- [x] 1.2 Remove `Command::Undo` from `crates/borax/src/cli.rs`,
      including its arms in `Command::name` and `Command::paths`
- [x] 1.3 Remove the undo path from `crates/borax/src/run.rs`:
      `Prepared::Undoing`, `undo_events`, `recorded_moves`, and the
      `Undo` arms of `preflight`, `emit_events`, `applying` and
      `expanded`
- [x] 1.4 Confirm `cargo build` is clean and that no `Undo`, `undo::`
      or `recorded_moves` reference survives outside CHANGELOG history

## 2. Remove the undo event vocabulary

- [x] 2.1 Remove `Event::Reverted` and its rendering arm from
      `crates/borax/src/event.rs`
- [x] 2.2 Remove `SkipReason::Missing`, `SkipReason::ContentChanged` and
      `SkipReason::OriginalTaken` with their rendering arms; leave
      `SkipReason::Unrecordable` in place (design decision 3)
- [x] 2.3 Delete the cases in `crates/borax/tests/event.rs` that pin the
      removed variants' rendering, keeping every `Unrecordable` case
- [x] 2.4 Confirm the JSON and human renderings still agree on every
      remaining event, and that the event schema version is unchanged —
      removing variants a reader never had to understand does not break
      the schema's promise about the ones that remain

## 3. Remove latest-apply-log selection

- [x] 3.1 Delete `latest_apply_log`, `latest_apply_in` and
      `is_apply_log` from `crates/borax/src/runlog.rs`, keeping
      `log_name`'s `<dry|apply>` suffix and the `phase` helper it needs
- [x] 3.2 Delete the `latest_apply_log` cases from
      `crates/borax/tests/runlog.rs` (section 3.3 and its header
      comment), keeping every `destination`, `log_name` and
      `state_root` case
- [x] 3.3 Add a local helper to `crates/borax/tests/end_to_end.rs` that
      finds a run log in a directory, and repoint the three
      `latest_apply_log` call sites at it, so those assertions keep
      their coverage without a production function only tests use
- [x] 3.4 Confirm `cargo test` is green across the workspace

## 4. Re-ground the specifications

- [x] 4.1 Verify the four delta files under
      `openspec/changes/remove-undo/specs/` against the living specs
      after any concurrent edits, re-copying MODIFIED blocks if a
      sibling change has archived in the meantime
- [x] 4.2 Run `openspec validate remove-undo --strict` and
      `python3 scripts/check-spec-deltas.py`; both must pass

## 5. Documentation

- [x] 5.1 `README.md`: drop "and can undo" from the opening claim, and
      add a short recovery section — where run logs live
      (`.borax/runs/`, or the XDG state directory outside a collection),
      what a `renamed` line carries (original path, new path, content
      hash), and how to read the mapping back. Explain the fields rather
      than offering a copy-paste `mv` pipeline, which would encourage
      skipping the verification a reader should be doing
- [x] 5.2 `openspec/STATE.md`: update "What is built" so the
      `add-core-pipeline` and `add-ledger-and-run-logs` paragraphs no
      longer describe an undo; narrow the sidecar-orphan defect to its
      one remaining entry point (renaming an already-renamed file) and
      note that removing undo closed the other; refresh "Last reviewed"
- [x] 5.3 `CHANGELOG.md`: a `0.3.0` section recording the removal as
      **BREAKING**, saying plainly what is gone, that run logs are
      unchanged and still mandatory for `--apply`, and how to recover
      names from a log

## 6. Release

- [x] 6.1 Bump the workspace version to `0.3.0`
- [x] 6.2 Full suite green on Linux, macOS and Windows in CI
- [x] 6.3 Archive the change with `openspec archive remove-undo` and
      confirm the living specs no longer mention undo anywhere:
      `grep -ri 'undo\|revert' openspec/specs/` returns nothing
