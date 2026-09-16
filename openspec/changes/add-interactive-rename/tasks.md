# Tasks: add-interactive-rename

Work on branch `change/add-interactive-rename`. Every group but the
terminal adapter is a red/green pair: the `-tests` task is written and
run failing first, and the implementation task makes it pass without
touching it. The interactive driver is tested end to end through a
scripted `Asker`, never through a terminal.

## 1. Planning without claiming

- [ ] 1.1 Red: `crates/borax-core/tests/rename.rs` — `Planner::propose`
      decides exactly as `plan` does for every existing case, and
      leaves the claim set untouched (proposing the same target twice
      yields the same unsuffixed name)
- [ ] 1.2 Red: `claim` after `propose` yields what `plan` yields, and a
      proposal never claimed leaves a later file's target unsuffixed
- [ ] 1.3 Green: split `Planner::plan` into `propose` and `claim`;
      `plan` becomes the two in sequence
- [ ] 1.4 Red: `crates/borax/tests/renaming.rs` — `Planning::propose`
      does not claim, `Planning::accept` does, and the batch
      `plan_renames` results are unchanged, including a target in a
      subdirectory, so the lazy widening `reach` performs stays covered
- [ ] 1.5 Green: mirror the split in `renaming::Planning`

## 2. Choosing the mode

- [ ] 2.1 Red: `crates/borax/tests/session.rs` — the mode function over
      every combination of terminal, format, `batch` setting and
      `--apply` (design D1)
- [ ] 2.2 Red: `crates/borax/tests/cli.rs` — `rename --batch`,
      `rename --no-batch` and `config --batch` parse; `resolve --batch`
      and `bib --batch` are unknown arguments; `--batch --no-batch` and
      `--apply --no-batch` are usage errors naming both flags
- [ ] 2.3 Red: `crates/borax/tests/config.rs` — `rename.batch` is a
      boolean key reported by `borax config` with its origin, from a
      file, from the environment and from the flag
- [ ] 2.4 Green: the mode function in `session.rs`, replacing
      `Interaction`/`confirm`; the pair in `RenameOptions`; the key in
      `RenameLayer`; the `--apply --no-batch` refusal before
      `run-started`

## 3. The question

- [ ] 3.1 Define `Question`, `Answer` and the `Asker` trait in
      `session.rs` (design D4), with a scripted implementation under
      `crates/borax/tests/` that returns answers from a list and fails
      the test when asked more questions than scripted
- [ ] 3.2 Red: `crates/borax/tests/run.rs` — an interactive run over
      fixture files: rename answers produce `renamed` and a ledger
      admission; skip answers produce `skipped`/`declined`; files with
      nothing to decide are never asked about; per-file adjacency holds
- [ ] 3.3 Red: a declined proposal leaves the next file's collision
      target unsuffixed; an accepted one suffixes it
- [ ] 3.4 Red: quit at the third of five files — the first two moved,
      the third resolved but untouched, the last two never opened,
      `run-finished` counts three unreached and two resolved-but-not-
      moved files among them, outcome is partial; the master `.bib`
      merge still runs for the visited files
- [ ] 3.5 Green: the interactive driver in `rename_events` — propose,
      ask, then accept and carry out, or decline, or stop
- [ ] 3.6 Green: `SkipReason::Declined`, `Counts::unreached` (and its
      effect on `outcome_for`), `run-started`'s `interactive`, and their
      human renderings

## 4. The run log

- [ ] 4.1 Red: `crates/borax/tests/runlog.rs` — an interactive run's
      destination is mandatory, `apply`-suffixed, falls back to the
      state directory, and an unopenable log refuses the run before the
      scripted asker is consulted
- [ ] 4.2 Green: `runlog::destination` and `mandatory` take the run's
      mode rather than `--apply` alone
- [ ] 4.3 Red: with a log that accepts two writes and then fails, an
      applying batch run makes the first two moves, does not make the
      third, and aborts; the log holds the two rename events (design D7)
- [ ] 4.4 Red: a rename event is on disk before its move — a filesystem
      that records the log's contents at the moment it is asked to move
      sees the event already there; a move the filesystem refuses is
      followed in the log by its failure
- [ ] 4.5 Green: split `Logging::emit` into the best-effort write and
      the move's own write, and record a move before `Applying` carries
      it out

## 5. The terminal

- [ ] 5.1 Add `inquire` to the workspace dependencies with only the
      features the adapter uses; check that no crate it pulls in is
      AGPL-licensed (`openspec/project.md`) and that the MSRV build is
      green
- [ ] 5.2 Implement the `inquire::Select` adapter; map Ctrl-C and Esc to
      `Answer::Quit`; confirm the prompt draws on stderr
- [ ] 5.3 Wire the adapter in `execute` when the mode is interactive
- [ ] 5.4 Verify by hand against a copy of the real-PDF corpus on Linux
      and Windows: rename, skip, quit, Ctrl-C, and a redirected-stdin
      run that asks nothing

## 6. Documents

- [ ] 6.1 Update `openspec/project.md`'s file-safety contract to "moves
      nothing without an explicit decision" (design D2)
- [ ] 6.2 Run `codex-docs` update jobs on `docs/manual.org` (`borax
      rename`, the settings table), `README.md` and `CHANGELOG.md` (the
      breaking default), with this proposal and the implemented `--help`
      as sources; check the diff
- [ ] 6.3 Record the built state in `openspec/STATE.md`
- [ ] 6.4 `openspec validate add-interactive-rename --strict` and
      `scripts/check-spec-deltas.py` pass; full suite green on all three
      platforms
