## Context

`crates/borax/src/cli.rs` declares one `Settings` struct carrying every
configurable setting, flattened into `Cli` with `global = true` on each
field. That decision is stated in the module documentation as a promise:

> Every setting flag is global rather than per-subcommand, so `borax
> --mailto … rename` and `borax rename --mailto …` are the same
> invocation. A person who has learned a flag once should not have to
> learn where it goes.

It was the right instinct and it bought a real property, which
`tests/cli.rs` pins. What it did not anticipate is that a global flag is
also an *advertised* flag: Clap lists every global under every
subcommand's `--help`, so the promise that a flag works anywhere became
a claim that it means something everywhere. It does not. Four of the
thirteen settings — the filename template, the collision policy,
`concurrency` and the ledger gate — are operative for exactly one
subcommand each, and two subcommands, `cache` and `ledger rebuild`, are
affected by none of them.

"Operative" is the word to be careful with. `execute` builds the source
adapters from `sources`, `mailto`, `min_interval_ms` and the
response-cache setting before it dispatches anything, so `borax cache`
does construct clients it will never use. That is shared setup this
change does not touch: the values reach no decision `cache` makes, and
the flag surface is about which settings can change an outcome.

`crates/borax/src/run.rs` is where that shows. `preflight` settles
`cache` and `ledger rebuild` entirely from their own arguments;
`emit_events` re-emits what preflight built. `resolve_events` is the
only caller that reaches `resolve_batch`, so it is the only consumer of
`network.concurrency`. `bib_events` resolves with `None` for the
collection, so the ledger gate cannot reach it. `group.filenames` is
read only by `Planning::new`, inside `rename_events`.

## Goals / Non-Goals

Goals:

- A subcommand's `--help` lists the settings that subcommand reads, and
  no others.
- An invocation naming an inapplicable setting is refused, rather than
  accepted and silently dropped.
- No irrelevant flag can end a run: a setting a command never reads
  cannot fail that command's validation or preflight.
- Configuration behaviour — precedence, origins, merging, `borax config`
  output — is bit-for-bit unchanged.

Non-goals:

- Changing which settings the pipeline actually reads. This change moves
  where a setting can be typed, not what any command does with one.
- Scoping *configuration*. A `.borax.toml` continues to set every key
  regardless of which command runs; a file is ambient context, not an
  argument to an invocation.
- Environment variables. `BORAX_NETWORK_CONCURRENCY` will still layer in
  on a `rename` that ignores it. That is correct for the same reason:
  ambient configuration is not something the user typed at this command,
  and `borax config` still reports it.

## Decisions

### D1. Per-subcommand declaration, and position-independence is spent

Clap propagates `global = true` downward, from a parent to its
subcommands. There is no upward propagation: an argument declared on
`Rename` is recognized only after the word `rename`. So the two
properties are mutually exclusive, and this change chooses the flag
surface over the flag position.

The alternative was to keep every flag global and add a per-command
relevance check that refuses an inapplicable one. It fixes the silent
no-op and the irrelevant-flag-ends-the-run problem, and it keeps the
promise. It does not fix `--help`, which is the misleading surface the
review actually found: Clap would still print all seventeen globals
under `borax cache --help`. It also introduces a second table — flag
against command — that has to be kept in agreement with dispatch by
hand, where flattening makes the type system carry that relation.

The cost is real and worth stating plainly: `borax --mailto … rename`
stops working. It is the convention every neighbouring tool follows
(`git commit -m`, `cargo build --release`), the error Clap produces
names the flag, and the project is pre-`1.0.0`, where `CLAUDE.md`
prefers replacing an interface cleanly over carrying a shim for it.

`--json` stays global. It is the one flag every subcommand honours, so
declaring it once is not an over-promise, and `borax --json config`
keeps working.

### D2. The groups follow consumption, not category

Five `Args` groups:

- `ResolutionOptions` — `--sources`, `--mailto`, `--page-limit`,
  `--min-interval-ms`, `--cache` / `--no-cache`. Everything needed to
  turn a file into a record.
- `RenameOptions` — `--collision`.
- `BibliographyOptions` — `--bib`, `--duplicates`, `--sidecars` /
  `--no-sidecars`.
- `AccountingOptions` — `--ledger` / `--no-ledger`.
- `RunLogOptions` — `--run-log` / `--no-run-log`.

| Subcommand       | Flattens                                                                     |
|------------------|------------------------------------------------------------------------------|
| `resolve`        | Resolution, `--concurrency`, RunLog                                          |
| `rename`         | Resolution, Rename, Bibliography, Accounting, RunLog, `--apply`              |
| `bib`            | Resolution, Bibliography, RunLog                                             |
| `config`         | every group, and `--concurrency`                                             |
| `cache`          | RunLog, `--clear`                                                            |
| `ledger rebuild` | RunLog                                                                       |

`--concurrency` sits outside `ResolutionOptions` and is declared on
`resolve` and `config` directly. Folding it into the shared group would
put it back on `rename` and `bib`, which is the exact inertness this
change exists to remove. It joins the group on the day one of them
resolves concurrently, and not before.

`rename` keeps the bibliography group because an applying rename writes
the bibliography output its configuration asks for on the side, and
keeps the accounting group because it is the only command that consults
and updates the ledger.

### D3. `config` keeps every setting

`borax config` prints each effective value with its origin, so passing
an override to it is not a no-op — it is the question being asked:
*what would this invocation resolve to?* Every group flattens there.

This also keeps the change cheap to test. The thirty-odd `flag_layers`
tests in `tests/cli.rs` all parse their flag against `config`, which
keeps every flag, so none of them moves or changes meaning. Each does
change one token — `.settings` becomes `.settings()` — which is the
mechanical cost of D4 and not a revision of what any of them asserts.

### D4. `Settings` survives as an assembled struct

`Settings` stops deriving `Args` and becomes a plain
`Default + Clone + PartialEq` struct. `Cli` loses its `settings` field
and gains `fn settings(&self) -> Settings`, which reads whichever groups
the chosen command carries and fills the rest with absence.

This is what keeps the blast radius off the configuration layer.
`flag_layers` keeps its signature, its `Origin::Flag` naming, and every
one of its arms but `template`. `config.rs` is untouched. A group that a
command does not flatten contributes no layer, which is exactly what a
flag left unsaid already did — so "absent means a lower layer shows
through" continues to mean one thing.

The cost is that each path-taking `Command` variant gains an options
field, and eighty-six construction sites across seven test files gain a
line. `Command::resolve(paths)`, `Command::rename(paths, apply)` and
`Command::bib(paths)` constructors filling defaults absorb that, and
leave those call sites shorter than they are now.

### D5. `--template` goes rather than moving to `rename`

Restricting `--template` to `rename` would fix its worst symptom — a
bibliography run aborted by a filename template — and leave the
contradiction. `README.md` says templates are settable only in
configuration files because their keys are open-ended; the `tables`
requirement reasons "like those two" from the same premise. A flag that
sets `templates.default` makes both statements false for one key, and
there is no `--citation-key` beside it to make the exception look
deliberate.

The open-endedness argument is the real one. `--template` can only ever
reach `default`; per-entry-type templates have no flag and cannot get
one, so the flag is a partial door into a structure the CLI is otherwise
not allowed to open. A one-off filename override is `.borax.toml` in the
directory, or `--json` and a different tool.

### D6. A command compiles the template tables it renders from

`compiled_groups` compiles both `templates` and `citation-keys` for
`rename` and for `bib`. Parameterizing it — both for `rename`, citation
keys alone for `bib` — is what stops a filename template from ending a
run that renders no filename.

This survives `--template`'s removal and is worth doing on its own: the
residual case is an uncompilable `templates.default` in a configuration
file, which today stops `borax bib` in a directory whose rename
configuration happens to be broken. After this change `borax rename`
reports it, which is where it is actionable.

`borax config` compiles nothing, and this change does not make it start.
`preflight` settles it as `Prepared::Unchecked`, so it prints a template
as the source text a configuration file gave, never as something
compiled. That is the right behaviour for a command whose value is
highest exactly when the configuration is broken: `borax config` has to
keep working on a configuration that no other command will accept. The
same reasoning is why the rule below is "compiles what it renders from"
rather than "validates everything it can see" — `config` renders
nothing.

Lookup tables stay loaded for both, unconditionally. Loading a table is
what validates the `lookup` tokens in whichever templates *are*
compiled, and a table that will not load is a property of the
configuration rather than of a template.

## Risks / Trade-offs

- **Muscle memory.** Anyone who has typed `borax --mailto … rename` gets
  a usage error. Clap names the unknown argument, and the subcommand's
  `--help` now lists it in the right place, so the error is
  self-correcting. There is no deprecation period; pre-`1.0.0` is what
  makes that acceptable.
- **`bib` becomes tolerant of a broken filename template.** A user whose
  `templates.default` will not compile learns it from `rename` or
  `config` rather than from any command that reads configuration. This
  is the intended trade: a `bib` run that would have succeeded should
  not fail over a value it never reads.
- **The groups are a judgement about consumption, and consumption can
  change.** If `bib` ever grows a filename-shaped output, or `rename`
  ever goes concurrent, a group's membership moves. That is a one-line
  change at the flatten site, and the type system makes the omission
  visible rather than silent — which is the property the current global
  surface lacks.
