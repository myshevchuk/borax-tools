# Design: fit-summary-to-command

## Context

Where the summary comes from today:

- `Counts` (`crates/borax/src/event.rs`) holds seven totals, and
  `Counts::observe` folds each event into them.
- `dispatch` (`crates/borax/src/run.rs`) reads the totals after the
  body of the run and emits `Event::RunFinished { counts }` through the
  `Logging` sink. `Logging` writes the event to the run log as JSON and
  passes it to the terminal's `Rendering`.
- `Rendering::emit` special-cases `(RunFinished, Format::Human)` to
  `human_summary(counts, self.hidden)`. `hidden` is how many
  already-named files an interactive run passed over. Every other
  event goes to `event::render`, and `event::human_line` also maps
  `RunFinished` to `human_summary(counts, 0)`.
- `session::outcome_for` makes a run partial when
  `skipped + unreached + findings` is nonzero.

What each command can emit, read from `emit_events` and the functions
it calls. Only events that `Counts::observe` counts are listed.

| Command | Counted events it can emit | Nonzero totals possible |
|---|---|---|
| `resolve` | `resolved`, `skipped` (`resolve_batch`) | resolved, skipped |
| `rename` | all but `library-finding`; the run sets `unreached` | resolved, renamed, skipped, named, unmatched, unreached |
| `bib` | `resolved`, `skipped` (incl. `bib-write-failed`, `unciteable`), `lookup-missed` | resolved, skipped, unmatched |
| `status`, `status --identify` | none (`library-status` only) | none |
| `validate` | `library-finding` | findings |
| `reconcile` | none (`library-repair`, `library-reconciled`) | none |
| `adopt` | `lookup-missed` (plus `library-adoption`, `library-adopted`) | unmatched |
| `config` | none (`config-setting`) | none |
| `cache` | none (`cache-status` or `cache-cleared`) | none |

`Aftermath::unreached` is nonzero only from `rename_events`. Every
other arm of `emit_events` returns `Aftermath::default()`.

## D1. The command picks a summary shape; a pure function renders it

**Decision.** Three pieces, each pure.

```rust
// crates/borax/src/event.rs
/// Which closing line a command's human rendering ends with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Summary {
    /// `rename`: resolved, renamed, skipped, and the optional clauses.
    Renaming,
    /// `resolve` and `bib`: resolved and skipped, and the optional
    /// clauses other than already named.
    Resolution,
    /// `validate`: no line of its own; findings are on the
    /// `library-validated` line.
    Validation,
    /// `status`, `reconcile`, `adopt`, `config`, `cache`: no line of
    /// their own.
    Silent,
}

pub fn human_summary(summary: Summary, counts: &Counts, hidden: usize)
    -> Option<String>;

// crates/borax/src/cli.rs
impl Command {
    pub fn summary(&self) -> Summary; // exhaustive, no wildcard arm
}
```

`dispatch` stores `cli.command.summary()` in a new `Rendering::summary`
field. `Rendering::emit` renders `(RunFinished, Human)` as
`human_summary(self.summary, counts, self.hidden)` and writes a line
only when that returns `Some`. The names above are the contract the
tests are written against.

`Command::summary` sits beside `Command::name`, and is exhaustive for
the same reason: a new subcommand does not compile until someone
decides its summary. The review asked that each subcommand have an
intentional summary, and the compiler enforces that. The mapping and
the rendering are both pure functions of values, so the unit tests
need no sink, no terminal and no dispatch.

**Rejected: the renderer reads the command from `run-started`.**
`Rendering` sees `RunStarted { command, .. }` before anything else and
could remember the name. That would make the human rendering literally
a function of the stream. But it matches on strings, needs a fallback
for a name it does not recognise, and makes the sink stateful in a new
way. The sink can be constructed with the shape directly, and the
shape is a function of the same command `run-started` names, so the
property "a rendering of the same stream" holds either way (D4).

**Rejected: each command's emitter decides whether to emit
`run-finished`.** Dropping or varying the event per command changes
the JSON stream and the run log. That breaks the framing guarantee in
"A run reports as it goes", and a consumer would no longer be able to
tell a finished run from a cut-off one.

**Rejected: carry the shape, or the command, on `run-finished`.** A new
field is an addition, so it does not require a schema bump. But it
puts a rendering decision into the machine contract, and the proposal
promises that the event is unchanged.

**Rejected: filter the finished string.** Keeping one
`human_summary(counts, hidden)` and post-processing its output per
command (stripping `0 renamed, `) ties command logic to the wording of
a string, and nothing in the type system notices when the two drift
apart.

## D2. `human_line` says nothing about `run-finished`

**Decision.** `human_line(&Event::RunFinished { .. })` returns `None`,
and so `render(Format::Human, …)` does too. That puts `run-finished`
alongside `run-started`, the other framing event.

A context-free rendering cannot know which summary fits, so it has
nothing correct to say. `Rendering` is the only caller that renders a
real run, and it handles `run-finished` itself, as it already does. The
invariant `render(Human, e) == human_line(e)` for every event still
holds.

**Rejected: keep `human_line` returning the rename line.** A second
entry point would then render a summary that is wrong for eight of the
nine commands. The next caller to reach for `render` would bring back
the defect this change removes.

**Rejected: thread the shape through `render` and `human_line`.**
Every other event renders the same way whatever the command, so every
caller would carry a parameter that only one event reads.

## D3. The shapes, exactly

`hidden` is read only by `Renaming`. Clause wording and order are
today's.

- **`Renaming`** is today's `human_summary` output, byte for byte:
  `{resolved} resolved, {renamed} renamed, {skipped} skipped`, then
  `, {named} already named` with its `(not shown)` / `({hidden} not
  shown)` variants, `, {unmatched} unmatched`, `, {unreached} not
  reached` and `, {findings} findings`, each only when nonzero. It is
  always `Some`.
- **`Resolution`**: `{resolved} resolved, {skipped} skipped`, then
  `, {unmatched} unmatched`, `, {unreached} not reached` and
  `, {findings} findings`, each only when nonzero. It is always
  `Some`. Resolved and skipped are always written, even at zero,
  because they answer the question the command was run to ask. A
  `resolve` run that found nothing says `0 resolved, 0 skipped`.
- **`Silent`**: `None` when skipped, unreached and findings are all
  zero. Otherwise it is the nonzero ones among `{skipped} skipped`,
  `{unreached} not reached` and `{findings} findings`, in that order,
  joined by `, `.
- **`Validation`**: the same as `Silent`, without findings.

`Resolution` carries the `not reached` and `findings` clauses although
neither command that uses it produces those totals today. The rule in
D3a then holds for it without an exception. `already named` is left
out of `Resolution` because it does not decide the exit and neither
command can produce it.

### D3a. A partial-success total is never hidden

**Decision.** The `Silent` and `Validation` fallback lines are how the
hard constraint holds by construction rather than by coincidence. Today
no `Silent` command can produce a skip, an unreached file or a finding,
so today they always return `None`. A later change that gives `adopt`
a skip, for example, would otherwise exit 2 with nothing on the
terminal saying why. With the fallback, that run's output ends with
`1 skipped`.

`Validation` omits findings from the fallback because `validate`
already states them twice. The `library-validated` line carries
`{findings} findings` (always, zero included), and each finding
precedes it on a line of its own. A summary repeating the count is the
redundancy this change removes.

This gives the property the unit tests pin. For every `Summary`
variant and every `Counts` with a nonzero skipped or unreached total,
the line is `Some` and names that total. The same holds for a nonzero
findings total, except under `Validation`.

**Rejected: rely on the table in Context.** The table shows that the
silent commands cannot produce these totals today, and that is enough
for the current code. But nothing would check it after the next change
to an emitter. The fallback is a few lines in a pure function, and a
test covers it without a fixture.

**Rejected: give `validate` a findings-only summary.** For example
`2 findings` after `…: 2 findings, 7 orphans, 0 missing, 0 unlinked`.
It repeats the line directly above it.

## D4. Per-command decisions

| Command | Shape | Partial-success totals it can produce | Where a reader sees them |
|---|---|---|---|
| `rename` (batch, interactive) | `Renaming` | skipped, unreached | summary; each skip also on its own line |
| `resolve` | `Resolution` | skipped | summary; each skip on its own line |
| `bib` | `Resolution` | skipped | summary; each skip on its own line |
| `status`, `status --identify` | `Silent` | none | — |
| `validate` | `Validation` | findings | `library-validated` line; each finding on its own line |
| `reconcile` | `Silent` | none | — |
| `adopt` | `Silent` | none | — |
| `config` | `Silent` | none | — |
| `cache` | `Silent` | none | — |

Every `Silent` or `Validation` command still has a last line of its
own: the `library-status`, `library-validated`, `library-reconciled`,
`library-adopted`, `cache-status` or `cache-cleared` line, or the last
`config-setting`. A person can still tell that the run finished.

**`bib` shares `Resolution` with `resolve`.** The brief for `bib` was to
drop only the counters it can never produce. Those are `renamed` and
`already named`, and dropping them gives exactly the `Resolution`
shape. A `bib`-specific summary is deferred (proposal), and when it
comes it becomes its own variant.

**`adopt` is `Silent`, including when a lookup misses.** A miss is
reported as `<table>: no row for "<input>"`. `adopt_events` writes the
misses after the adoptions and directly before the `library-adopted`
totals, so every miss is on the terminal a line or two above the end.
Unmatched lookups do not decide the exit. The count is in
`run-finished` for a script. **Rejected:** keeping a trailing
`N unmatched` line for `adopt` when there were misses. It would be the
only content of a summary line, it would follow a line about adoption
counts, and it would repeat the number of lines directly above it.
This decision is why the `external-tables` delta restates where the
count is carried.

**`resolve` drops `renamed`.** `resolve_batch` emits `resolved` and
`skipped` only. The review asked for this explicitly.

## D5. Human output only; JSON and the run log are untouched

`Logging::emit` writes `json_line(&event)` to the log before handing the
event to `Rendering`. `Rendering` in `Format::Json` goes through
`render`, whose `Json` arm is `json_line`. Neither path reads
`Summary`. The event, its seven counters and `SCHEMA` (3) stay as they
are. The cli requirement changes the schema version only when an event
or a field changes, so this change owes no bump, and the proposal says
so.

## Not changed, deliberately

- The wording of any report or totals line (`library-status`,
  `library-validated` and the rest).
- The per-file `skipped` lines, which are the skip report the
  `resolution` requirement is about.
- `status --identify` reports how many artifacts yield an identifier
  and no per-file failures, as it does today. The `extraction`
  requirement's "reported in the run summary" is the skip report of
  commands that skip. `status` skips nothing and has no skip report,
  before this change and after it.
