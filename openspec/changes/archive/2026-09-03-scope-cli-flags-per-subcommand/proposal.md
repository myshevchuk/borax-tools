## Why

Every setting borax has is declared as a global Clap argument, so every
subcommand accepts all seventeen of them. Most subcommands consume a
fraction. `borax rename --concurrency 8` names a setting only
`resolve_events` reads. `borax bib --no-ledger` names a gate `bib`
already bypasses unconditionally. `borax cache --no-cache` reads as
"inspect the cache without the cache", when the flag it names decides
whether *resolving* commands consult the response cache and has nothing
to say about `borax cache`. `borax ledger rebuild --no-ledger` looks
like a contradiction and rebuilds the ledger anyway.

None of these are refused, and none of them change an outcome. `--help`
lists them all under every subcommand, so the help text asserts an
effect the code does not have, and a plausible-looking invocation is
indistinguishable from an effective one. (`execute` does build the
source adapters from `sources`, `mailto` and the pacing settings before
every dispatch, including `cache`'s. Those clients go unused, and that
shared setup is out of scope here.)

They are also not quite inert. A flag becomes a configuration layer
before dispatch knows which command is running, so an irrelevant flag
still passes through layering and validation and can end a run that was
never going to read it. The sharpest case is `--template`: shared
preflight compiles the filename template table for `bib`, which renders
no filename, so an unparseable `--template` aborts a bibliography run
over a template that run would never have used.

`--template` should not exist at all. `README.md` states that templates
are settable only in configuration files, because their keys are
open-ended, and the `tables` requirement in the `cli` specification
reasons from that same premise for itself — "like those two". The flag
is the one place the implementation contradicts the documented model,
and there is no `--citation-key` counterpart to it.

## What Changes

- **Settings are declared per subcommand.** The flag surface is split
  into small `clap::Args` groups — resolution, rename, bibliography,
  accounting, run-log — and each subcommand flattens the groups whose
  settings it actually reads. A setting a command does not consume is
  not among its arguments, so `--help` describes that command and an
  invocation naming an inapplicable setting is refused as an unknown
  argument rather than accepted and ignored.
- **`--template` is removed.** Filename templates become what the
  README already says they are: configuration-file-only, like
  `citation-keys` and `tables`, because their keys are open-ended.
- **Breaking, and deliberately without a shim.** A setting flag now
  follows its subcommand: `borax rename --mailto me@example.org f.pdf`
  is the only accepted form, and `borax --mailto me@example.org rename
  f.pdf` is a usage error. Clap propagates a global argument downward
  from the parent, never upward from a subcommand, so per-subcommand
  declaration and position-independence cannot both hold. `--json` is
  the exception: it stays global, because it is the one flag every
  subcommand honours.
- **A command compiles only the template tables it renders from.**
  `rename` compiles both filename and citation-key templates; `bib`
  compiles citation keys alone. A filename template that will not
  compile therefore stops a `rename` and no longer stops a `bib`.
- **Nothing about configuration changes.** The layering, precedence,
  origins and `borax config` output are untouched. `borax config`
  continues to accept every setting override, because inspecting what
  an override would resolve to is that command's whole purpose.
  Environment variables stay ambient and uncommanded.

## Capabilities

### Modified Capabilities

- `cli`: the flag surface becomes per-subcommand rather than global,
  with a requirement naming which settings each subcommand accepts;
  filename templates join `citation-keys` and `tables` as
  configuration-file-only; the shared-schema and boolean-negation
  requirements gain the sentence that reconciles them with a
  per-command surface.
- `templates`: the fail-fast requirement gains the scope it was silent
  about — a template table is compiled by the commands that render from
  it, so a command that renders no filename cannot be ended by a
  filename template.

## Impact

- `crates/borax/src/cli.rs`: `Settings` stops deriving `Args` and
  becomes the plain struct `flag_layers` already treats it as; five
  `Args` groups take its place, flattened per `Command` variant;
  `Cli::settings` assembles one from whichever groups the chosen
  command carries. The `template` arm of `flag_layers` goes. The module
  documentation's position-independence promise is what this change
  spends, and is rewritten.
- `crates/borax/src/run.rs`: `execute` and `dispatch` call
  `cli.settings()`; `expanded` carries each variant's groups through;
  `compiled_groups` takes which template tables the command needs.
- `crates/borax/tests/`: the position-independence test is replaced by
  its inverse; per-command acceptance and rejection tests are new. The
  thirty-odd `flag_layers` tests keep their subjects and their
  assertions — they all parse against `config`, which keeps every flag —
  but each reaches its settings through `.settings()` rather than the
  field, and the `--template` case goes. Eighty-six `Command`
  construction sites gain a field, which constructors on `Command`
  absorb.
- `README.md`: the configuration section's claim about templates
  becomes true without exception; flag placement is stated.
- No dependency changes, and no change to any event schema.

## Deferred

- **Concurrent `rename` and `bib`.** `--concurrency` lands on `resolve`
  and `config` only, because those are the commands that read it.
  `rename` and `bib` resolve serially on purpose: a file's verdict and
  its fate are adjacent in the stream, which is what
  `stream-per-file-events` bought. Making either concurrent is a change
  to that contract, not to this flag surface, and would move
  `--concurrency` into the shared resolution group when it happens.
- **Scoping the shared setup.** `execute` resolves configuration and
  builds the source adapters before dispatch, for every command,
  including the two that query nothing. Those clients open no
  connection, so the cost is small and the correctness is unaffected —
  but after this change `borax cache` will be a command that cannot be
  given `--sources` and still constructs the sources. Making setup
  follow the command is a `run.rs` change with its own reasoning about
  where adapters are built, and it does not belong in a change about
  what the command line accepts.
- **Run-log controls on read-only commands.** `--run-log` and
  `--no-run-log` stay on every subcommand, including `config`, `cache`
  and `ledger rebuild`. They are genuinely operative there — logging is
  wired at dispatch — so removing them would be a behaviour change
  rather than a correction of the flag surface. Whether a read-only
  inspection should log at all is a question for the `run-logs`
  capability.
