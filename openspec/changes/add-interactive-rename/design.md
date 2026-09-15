## Context

A rename run today is a fold over its files: resolve one, plan its
target through `renaming::Planning`, carry the decision out through
`renaming::Applying`, cite it, and move to the next. `Applying` is built
once per group with `apply` fixed for the run, so every decision in a
run is carried out the same way. That fixed `apply` is the whole of
what makes the run a batch, and it is the thing this change makes
per-file.

`session.rs` already holds the other half. `interaction(stdin_is_terminal,
format)` answers whether a run may ask, and `confirm(interaction, ask)`
grants nothing when it may not. Both are tested and neither is called.

Two properties of the pipeline constrain the design:

- `borax_core::rename::Planner::plan` claims the name it decides on. A
  batch run carries out every decision, so deciding and claiming have
  never needed to be apart.
- A run writes each event when it happens (`stream-per-file-events`),
  and a file's resolution, move and sidecar are adjacent. An interactive
  run keeps both: the question for a file sits between its resolution
  line and its fate.

## Goals / Non-Goals

Goals:

- On a terminal, `borax rename <dir>` puts each proposed move to the
  operator and carries out the ones accepted.
- Every non-terminal invocation, and every existing `--apply`
  invocation, behaves exactly as before.
- A declined proposal leaves no trace on the plan: the names later
  files receive are the names they would receive had the declined file
  not been in the run.
- The file-safety contract holds: nothing is overwritten, and nothing
  moves without a decision made with the target in view.

Non-goals:

- Showing the record behind a proposal (change 3), supplying
  identifiers or overriding conflicts (change 4), passing over
  already-named files (change 2). This change asks only about files a
  batch run would have renamed.
- Interactive `resolve` or `bib`.

## Decisions

### D1. The mode is decided once, before the first event

```text
batch   := apply || setting(batch) || !stdin_is_terminal || format == json
```

with `--apply --no-batch` refused as a usage error before the run
starts. The decision is a pure function in `session.rs`, replacing the
`Interaction` pair's use: the terminal and the format are what
`interaction` already weighs, and the two new inputs are the setting and
`--apply`.

`--apply` selects batch rather than being refused outside it, because
`borax rename --apply <dir>` is the invocation every existing user and
every existing document spells, and in batch it means what it always
meant. The alternative — `--apply` valid only beside `--batch` — makes
the common batch invocation longer to buy a strictness nothing needs:
there is no interactive reading of `--apply` it could be confused with.

`--apply --no-batch` is the one combination that states two
incompatible intentions, and the `cli` capability's rule for such pairs
is a usage error rather than a silent choice.

`batch` is a configurable setting and `apply` stays unconfigurable.
Batch without `--apply` is a preview, so configuration that selects it
makes a run move less, never more. The asymmetry is the one the apply
gate requirement already draws.

### D2. A yes is the apply gate for one file

The contract `openspec/project.md` states is that borax previews by
default. What that sentence protects is that no file moves because of a
default, a setting, or a guess — that a move is always something a
person asked for, knowing what it was. `--apply` asks for a whole plan
the person has had the chance to preview. A yes asks for one move whose
exact target is on the screen as it is given. It is the narrower
authorisation of the two, so an interactive run needs no `--apply`
before a yes can act.

The contract sentence is rewritten to say that directly: borax moves
nothing without an explicit decision, and the decisions are `--apply`
over a previewable plan or a yes to a question naming the target.

### D3. Deciding a target and claiming it become two steps

`Planner` gains `propose(&self, item, policy) -> PlannedAction`, which
decides exactly as `plan` does and records nothing, and
`claim(&mut self, action)`, which records the name a `Rename` takes.
`plan` becomes `propose` followed by `claim`, so batch planning is
unchanged by construction and its tests stand as they are.

`renaming::Planning` mirrors the split: `propose` returns the
`PlannedRename` without claiming, and `accept` claims it. The batch
path calls both; the interactive path calls `accept` only on a yes.

Proposing twice before claiming is safe — a proposal reads the claim
set, never writes it — which is what lets change 4 re-propose a file
under a different identifier without unwinding anything.

The alternative was cloning the planner before each question and
restoring the clone on a decline. It works, and it copies a directory's
listing per file; the split costs one method and states the intent.

### D4. The question is behind a trait, and `inquire` answers it

```rust
pub trait Asker {
    fn choose(&mut self, question: &Question) -> Answer;
}
```

`Question` carries the file, the proposed target and the choices on
offer; `Answer` is one of them or `Quit`. The interactive driver in
`run.rs` is written against the trait, so every path through it is
tested with a scripted asker and no terminal.

The terminal adapter is `inquire::Select`, the arrow-key menu the
operator chose over a line of letter keys: later changes add choices,
and a menu stays legible as they arrive. `inquire` is MIT-licensed,
draws with crossterm (so Windows is supported), and writes its prompt to
stderr, which keeps stdout the event stream in both modes. The adapter
is the whole of what touches the terminal and is verified by hand.

An interrupt at a prompt — Ctrl-C or Esc, which `inquire` reports as
errors rather than delivering a signal — is `Quit`. An interrupt while
borax is resolving is an ordinary signal and ends the process, which a
batch run already survives and an interactive run survives the same
way: the moves already made stay made and logged.

### D5. The menu for this change

For a file whose proposal is `Rename`:

```text
50-Article Text-95-2-10-20240507.pdf
  → lyutenko2023_ApplicationsChiralSulfinyl.pdf
? Rename this file?
> Rename
  Skip
  Quit
```

The first choice is the default so Enter accepts. Every other planning
outcome — `AlreadyNamed`, `TargetTaken`, `Unnameable` — and every
skipped resolution is reported as the batch run reports it and asks
nothing, because in this change there is nothing the operator could
choose that borax would do differently. Changes 2 and 4 are where some of
those gain questions.

### D6. What the stream says

- `run-started` gains `interactive: bool`, and an interactive run
  reports `applying: true` (D7).
- A yes is `renamed`, exactly as a batch apply reports it.
- A no is `skipped` with the new reason `declined`. It is a skip in
  every sense the summary and the exit code use: the file is left for
  attention, and a script-free run still ends saying so.
- No `planned` event is emitted for a question. `planned` means "a
  preview would do this", and the question is not a preview; the target
  reaches the stream in the `renamed` line or not at all.
- Quitting emits nothing per remaining file. `Counts` gains
  `unreached`, the number of input files left without a fate — the one
  the operator quit at and every one after it — and `run-finished`
  carries it. A per-file event would put hundreds of identical lines in
  the log of a session quit early in a large folder, saying nothing the
  count does not; the human summary reads "… 140 not reached".
- A run with `unreached > 0` exits with the partial-success code: those
  files did not succeed.

The questions themselves are not events. They are the interaction, not
the report, and a run log records what happened to each file rather
than the conversation that decided it.

### D7. An interactive run's log is an applying run's log

The run-log placement rules key off whether a run may move files, and
an interactive run may. So `runlog::destination` and `mandatory` treat
it as applying: its log is mandatory, falls back to the state directory
outside a collection, is named with the `apply` suffix, and must open —
with its first event written — before the first question is asked. A
session that would lose its record is refused before the operator has
answered anything.

A session in which every answer is no still leaves an `apply` log. That
is accurate: the log records a run that was authorised to move files and
moved none.

The rename and run-logs requirements state that an applying run's
planned renames are flushed before its first move. Since
`stream-per-file-events`, the implementation writes each `renamed` line
unbuffered after its move instead, and an interactive run cannot have
a whole plan up front. This change inherits whatever the requirement
guarantees of an applying run and does not settle that divergence,
which predates it and affects `--apply` equally.

### D8. Everything after the decision is the batch path

Bibliography output, ledger admission and lookup-miss reporting are
unchanged and follow the file's fate as they do in batch: a sidecar is
written beside the name the file carries after its decision, a renamed
file is admitted to the ledger, and the master `.bib` merge trails the
group. A quit still merges what the visited files produced, because the
entries for files that were visited are as correct as they would have
been in a finished run.

## Risks / Trade-offs

- **The default changes under existing users.** `borax rename <dir>` on
  a terminal used to be a harmless preview and now asks questions. It
  still moves nothing without a yes, and `rename.batch = true` restores
  the old default. Pre-`1.0.0`, this is recorded in the changelog as a
  breaking change rather than shimmed.
- **A long session holds a stale snapshot.** A directory is listed when
  its group starts, and an operator can take minutes over it. Something
  created in the meantime at a proposed target is not seen by the
  planner. The move itself refuses to overwrite (hard link then unlink),
  so the cost is a `rename-failed` skip, not a lost file.
- **The terminal adapter is untested by the suite.** It is kept to
  translating a `Question` into an `inquire::Select` and its result
  back, so that what is untested is also what is thin.
