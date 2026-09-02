# Tasks: scope-cli-flags-per-subcommand

This change moves where a setting can be typed; it changes what no
command does with one. The parsing surface is not a pure function of
strings in the `borax-core` sense, but it is testable without I/O —
`Cli::try_parse_from` over an argument vector — so every group below is
still a red/green pair: the tests in the `-tests` task are written and
run failing first, and the implementation task makes them pass without
touching them.

Work on branch `change/scope-cli-flags-per-subcommand`. Group 1 is
mechanical preparation with the suite green throughout; groups 2–4 are
the change proper; group 5 is what the surface change lets `run.rs` stop
doing.

## 1. Preparation, with nothing yet moved

- [x] 1.1 Add `Command::resolve(paths)`, `Command::rename(paths, apply)`
      and `Command::bib(paths)` to `crates/borax/src/cli.rs`, each
      building the variant as it reads today
- [x] 1.2 Move the eighty-six `Command::{Resolve,Rename,Bib} { … }`
      construction sites in `crates/borax/tests/` to those constructors,
      leaving match arms alone; suite stays green — a commit of its own,
      so the diff that follows is the change and not the migration

## 2. The option groups

- [x] 2.1 Red: `crates/borax/tests/cli.rs` cases for the accepted
      surface — each of the six commands parsing every setting it
      consumes, asserting the resulting `Settings` field by field
- [x] 2.2 Red: cases for the refused surface — `cache --no-cache`,
      `cache --mailto`, `rename --concurrency`, `bib --no-ledger`,
      `bib --collision`, `ledger rebuild --no-ledger`, and
      `resolve --bib`, each an unknown-argument error
- [x] 2.3 Red: replace `a_global_flag_before_the_subcommand_parses_…`
      with its inverse — `--mailto` before `rename` is an unknown
      argument — and keep a case pinning `--json` accepted on both sides
- [x] 2.4 Red: `--sidecars` and `--no-sidecars` together on `rename`
      still a usage error naming both, and the same for the three other
      pairs on a command that offers them
- [x] 2.5 Green: declare `ResolutionOptions`, `RenameOptions`,
      `BibliographyOptions`, `AccountingOptions` and `RunLogOptions` as
      `Args` groups, each `Default`, and flatten them per `Command`
      variant as `design.md` D2 tabulates; `--concurrency` on `Resolve`
      and `Config` directly
- [x] 2.6 Green: `Settings` stops deriving `Args` and keeps
      `Default + Clone + Debug + PartialEq + Eq`; `Cli` loses its
      `settings` field and gains `fn settings(&self) -> Settings`
      assembling one from the chosen command's groups
- [x] 2.7 Rewrite the `cli.rs` module documentation: the
      position-independence promise is what this change spends, and the
      new paragraph says which settings a subcommand carries and why
      `--json` is the one that stays global

## 3. `--template` is removed

- [x] 3.1 Red: `borax rename --template …` and `borax config --template
      …` are unknown-argument errors, and `flag_layers` has no arm that
      can produce a `templates` layer
- [x] 3.2 Green: delete the `template` field and its `flag_layers` arm;
      confirm `Origin::Flag("template")` appears nowhere
- [x] 3.3 Check that no environment-variable path reaches `templates`,
      so the config-file-only claim now holds for all three open-ended
      structures

## 4. Wiring the run

Done with group 2: removing `Cli::settings` and giving each variant
its option groups breaks `run.rs` and the five test `Cli` literals in
the same edit, so the suite has no green state between the two groups.

- [x] 4.1 `execute` and `dispatch` in `crates/borax/src/run.rs` call
      `cli.settings()` where they read `cli.settings`; drop
      `settings: Settings::default()` from the five test `Cli` literals
- [x] 4.2 `expanded` carries each variant's option groups through
      unchanged beside the expanded paths
- [x] 4.3 Move the `flag_layers` tests from `.settings` to
      `.settings()`; their subjects and assertions do not change, since
      they all parse against `config`, which keeps every flag. A test
      that needs more than that token is a surface mistake to fix in
      group 2, not a test to edit here

## 5. Templates compile where they are rendered

- [ ] 5.1 Red: `crates/borax/tests/binary.rs` — a configuration whose
      `templates.default` will not compile lets `borax bib` run to
      completion, and still ends `borax rename` with the fatal exit
      code, neither `run-started` nor `run-finished` on stdout
- [ ] 5.2 Red: a configuration whose `citation-keys.default` will not
      compile still ends `borax bib`, so the scoping cut in one
      direction only
- [ ] 5.3 Red: a declared table that cannot be read still ends both, so
      table loading did not get scoped along with the templates
- [ ] 5.4 Red: `borax config` reports a `templates.default` that will
      not compile as the source text it is, without refusing the run,
      as it does today — pinning that this change does not make `config`
      start validating
- [ ] 5.5 Green: `compiled_groups` takes which template tables the
      command needs — both for `Rename`, citation keys alone for `Bib`,
      and neither for `Config` and `Resolve`, which keep the
      `Prepared::Unchecked` path they have now

## 6. Documentation and release

- [ ] 6.1 `README.md`: state that setting flags follow their subcommand
      and `--json` does not; remove any `--template` usage; the
      configuration section's claim about templates now holds without
      exception
- [ ] 6.2 `CHANGELOG.md` under Unreleased: a `Removed` entry for
      `--template` and a `Changed` entry for the per-subcommand surface,
      both naming the position change, since these are the two things
      that will break an existing invocation
- [ ] 6.3 `openspec/STATE.md`: record that the flag surface is
      per-subcommand, and that `rename` and `bib` resolving serially is
      now visible in the CLI rather than only in the code
- [ ] 6.4 Full suite green on Linux, macOS and Windows; `cargo clippy`
      clean; `openspec validate --strict` and
      `scripts/check-spec-deltas.py` both pass
